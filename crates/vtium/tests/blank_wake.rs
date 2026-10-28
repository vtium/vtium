//! Blanking and waking from outside the session (vtium/vtium#116): the
//! seat booted with no hardware, as `main` runs it, takes `SIGRTMIN+1`
//! and `SIGRTMIN+2` through its loop and blanks and wakes a display in
//! memory. Each test raises the signal in its own thread, which the
//! loop's signalfd blocks and reads; no other process is signalled.

use std::any::Any;
use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::blank::Order;
use vtium::config;
use vtium::video::Video;
use vtium_platform::display::{Canvas, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::rt_signal;
use vtium_platform::vt::FakeVt;

const W: u32 = 320;
const H: u32 = 160;
const ID: DisplayId = DisplayId(7);

/// A device with one display, its frame buffer in memory.
#[derive(Default)]
struct Memory {
    pixels: Vec<u8>,
    events: Vec<Event>,
    awake: bool,
    dpms: Option<Dpms>,
    unblanks: u32,
}

impl Video for Memory {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    fn displays(&self) -> Vec<DisplayId> {
        if self.awake { vec![ID] } else { Vec::new() }
    }
    fn name(&self, _: DisplayId) -> String {
        "MEM-1".into()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((W, H))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
        if !self.awake || self.dpms != Some(Dpms::On) {
            return None;
        }
        Some(Canvas {
            pixels: &mut self.pixels,
            width: W,
            height: H,
            stride: W * 4,
            age: 0,
        })
    }
    fn swap(&mut self, id: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.dpms = Some(dpms);
        if dpms == Dpms::On {
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (W * H * 4) as usize];
        self.awake = true;
        self.dpms = Some(Dpms::On);
        self.events.push(if first {
            Event::Added(ID)
        } else {
            Event::Redraw(ID)
        });
        self.events.push(Event::Awake);
        Ok(())
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.unblanks += 1;
        self.set_dpms(ID, Dpms::On)
    }
    fn input(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn hotplug(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

struct Harness {
    lp: Loop<App>,
    app: App,
    /// How far the seat's clock runs ahead of the real one.
    ahead: Rc<Cell<Duration>>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

impl Harness {
    /// The seat on the fake VT with the memory device, its loop taking
    /// the signals as `main`'s does.
    fn new(options: &[&str]) -> Harness {
        let mut args: Vec<OsString> = options.iter().map(OsString::from).collect();
        args.extend(["--login", "--", "/bin/sh", "-c", "read x"].map(OsString::from));
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the harness's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        vtium::blank::listen(&lp.handle()).expect("the blank's signals");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let ahead = Rc::new(Cell::new(Duration::ZERO));
        let clock = {
            let ahead = ahead.clone();
            Rc::new(move || now() + ahead.get())
        };
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            clock,
            env,
        );
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        let mut h = Harness { lp, app, ahead };
        assert!(h.until(|m| m.dpms == Some(Dpms::On)), "the display is lit");
        h
    }

    fn at(&self) -> Instant {
        now() + self.ahead.get()
    }

    fn memory(&mut self) -> &mut Memory {
        self.app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .expect("the memory device")
            .as_any()
            .downcast_mut::<Memory>()
            .expect("the memory device")
    }

    /// Turns the loop as `main` does until the device satisfies `done`,
    /// at most 3 s.
    fn until(&mut self, done: impl Fn(&Memory) -> bool) -> bool {
        let end = now() + Duration::from_secs(3);
        loop {
            if done(self.memory()) {
                return true;
            }
            if now() > end {
                return false;
            }
            let wait = self
                .app
                .next_wake(self.at())
                .unwrap_or(Duration::from_millis(20))
                .min(Duration::from_millis(20));
            self.lp
                .dispatch(Some(wait), &mut self.app)
                .expect("dispatch");
            self.app.turn(self.at());
        }
    }

    /// Raises `order`'s signal in this thread, where the loop reads it.
    fn signal(&mut self, order: Order) {
        rt_signal::raise(order.offset()).expect("the signal raised");
    }

    fn key(&mut self, at: Instant) {
        let key = KeyEvent {
            keycode: 0,
            ascii: 0,
            mods: Mods::empty(),
            keysyms: vec![u32::from('a')],
            codepoints: vec![Some('a')],
            repeat: false,
        };
        self.app.input_event(InputEvent::Key(key), at);
    }
}

const OFF: fn(&Memory) -> bool = |m| m.dpms == Some(Dpms::Off);
const ON: fn(&Memory) -> bool = |m| m.dpms == Some(Dpms::On);

#[test]
fn sigrtmin_1_blanks_at_once_and_sigrtmin_2_wakes() {
    let mut h = Harness::new(&[]);
    h.signal(Order::Blank);
    assert!(
        h.until(OFF),
        "blanked by SIGRTMIN+1, long before dpms-timeout"
    );
    assert_eq!(h.memory().unblanks, 0);
    h.signal(Order::Wake);
    assert!(h.until(ON), "woken by SIGRTMIN+2");
    assert_eq!(h.memory().unblanks, 1, "through the blank's unblank");
}

/// A blank by signal is the timer's blank: a key ends it, and the
/// countdown runs again from the key.
#[test]
fn a_key_ends_a_blank_sent_by_signal() {
    let mut h = Harness::new(&["--dpms-timeout=60"]);
    h.signal(Order::Blank);
    assert!(
        h.until(OFF),
        "blanked by the signal, not the minute's timer"
    );
    assert_eq!(h.app.blank.deadline(), None, "nothing due while blanked");
    let pressed = h.at();
    h.key(pressed);
    assert!(h.until(ON), "woken by the key");
    assert_eq!(h.memory().unblanks, 1);
    assert_eq!(
        h.app.blank.deadline(),
        Some(pressed + Duration::from_secs(60)),
        "the countdown runs from the key"
    );
}

/// Blanking off and `no-program-blank` hold the timer and the programs
/// back, never the signals.
#[test]
fn the_signals_act_with_blanking_and_program_control_off() {
    let mut h = Harness::new(&["--dpms-timeout=0", "--no-program-blank"]);
    h.signal(Order::Blank);
    assert!(h.until(OFF), "blanked by the signal with no timeout");
    h.signal(Order::Wake);
    assert!(h.until(ON));
}

/// A wake with nothing blanked leaves the displays as they are; a second
/// blank while blanked does nothing more.
#[test]
fn a_wake_while_lit_and_a_blank_while_blanked_change_nothing() {
    let mut h = Harness::new(&[]);
    h.signal(Order::Wake);
    assert!(!h.until(|m| m.unblanks > 0 || m.dpms != Some(Dpms::On)));
    h.signal(Order::Blank);
    assert!(h.until(OFF));
    h.signal(Order::Blank);
    h.signal(Order::Wake);
    assert!(h.until(ON));
    assert_eq!(h.memory().unblanks, 1);
}
