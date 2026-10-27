//! The seat booted with no hardware: the fake VT, a video device in
//! memory, and a real child on a real pty. It drives the wiring's paths
//! through the loop as `main` runs it: the child's output drawn, keys
//! through the key order to the child, a VT release and acquire, the DPMS
//! blank and its wake-up, and the exit with the switch back.

use std::any::Any;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium::app::{App, Exit};
use vtium::config;
use vtium::video::Video;
use vtium_platform::display::{Canvas, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};

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
    frames: u32,
    age: u32,
    unblanks: u32,
    wake_ups: u32,
    /// Each canvas lent's age, in order.
    ages: Vec<u32>,
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
        let age = self.age;
        self.ages.push(age);
        Some(Canvas {
            pixels: &mut self.pixels,
            width: W,
            height: H,
            stride: W * 4,
            age,
        })
    }
    fn swap(&mut self, id: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        self.frames += 1;
        self.age = 1;
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.dpms = Some(dpms);
        if dpms == Dpms::On {
            self.age = 0;
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.wake_ups += 1;
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (W * H * 4) as usize];
        self.awake = true;
        self.dpms = Some(Dpms::On);
        self.age = 0;
        if first {
            self.events.push(Event::Added(ID));
        } else {
            self.events.push(Event::Redraw(ID));
        }
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

/// A VT the test drives: always shown, and it raises what the test
/// queues, `Retaken` among them, which the fake VT never does.
#[derive(Default)]
struct ScriptedVt {
    events: Vec<VtEvent>,
    active: bool,
}

impl Vt for ScriptedVt {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.drain(..)
    }
    fn acknowledge_release(&mut self) -> io::Result<()> {
        self.active = false;
        Ok(())
    }
    fn refuse_release(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn activate(&mut self) -> io::Result<Switch> {
        if !self.active {
            self.active = true;
            self.events.push(VtEvent::Acquired);
        }
        Ok(Switch::Pending)
    }
    fn leave(&mut self) -> io::Result<Switch> {
        Ok(Switch::Done)
    }
    fn wait(&mut self) -> io::Result<Switch> {
        Ok(Switch::Done)
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn key(&mut self, _: &KeyEvent) -> bool {
        false
    }
    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn bell(&mut self) {}
    fn number(&self) -> Option<u16> {
        None
    }
}

struct Harness<V: Vt + 'static = FakeVt> {
    lp: Loop<App<V>>,
    app: App<V>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

impl Harness {
    fn new(args: &[&str]) -> Harness {
        Harness::with_vt(args, FakeVt::new())
    }
}

impl<V: Vt + 'static> Harness<V> {
    fn with_vt(args: &[&str], vt: V) -> Harness<V> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the harness's command line");
        let lp: Loop<App<V>> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let mut app = App::new(loaded.config, vt, lp.handle(), Rc::new(now), env);
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
        app.insert_blank();
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Harness { lp, app }
    }

    /// Turns the loop until `done` holds, at most `limit`.
    fn until(&mut self, limit: Duration, mut done: impl FnMut(&App<V>) -> bool) -> bool {
        let end = now() + limit;
        while now() < end {
            if done(&self.app) {
                return true;
            }
            let wait = self
                .app
                .next_wake(now())
                .unwrap_or(Duration::from_millis(20))
                .min(Duration::from_millis(20));
            self.lp
                .dispatch(Some(wait), &mut self.app)
                .expect("dispatch");
            self.app.turn(now());
        }
        done(&self.app)
    }

    fn memory(&mut self) -> &mut Memory {
        self.app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("the memory device")
    }

    /// Turns the loop until the device's state satisfies `done`.
    fn until_memory(&mut self, limit: Duration, done: impl Fn(&Memory) -> bool) -> bool {
        let end = now() + limit;
        loop {
            if done(self.memory()) {
                return true;
            }
            if now() > end {
                return false;
            }
            self.until(Duration::from_millis(20), |_| false);
        }
    }

    fn type_str(&mut self, text: &str) {
        self.type_keys(text, false);
    }

    fn type_keys(&mut self, text: &str, repeat: bool) {
        for c in text.chars() {
            let keysym = if c == '\r' { 0xff0d } else { u32::from(c) };
            let key = KeyEvent {
                keycode: 0,
                ascii: 0,
                mods: Mods::empty(),
                keysyms: vec![keysym],
                codepoints: vec![(c != '\r').then_some(c)],
                repeat,
            };
            self.app.input_event(InputEvent::Key(key), now());
        }
    }
}

const SECOND: Duration = Duration::from_secs(5);

#[test]
fn the_seat_boots_draws_takes_keys_and_exits() {
    let mut h = Harness::new(&[
        "--oneshot",
        "--dpms-timeout=1",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'hello from the child\\n'; read x; echo \"got:$x\"; read y",
    ]);
    // The display came online and the grid follows it: 320 / 8, 160 / 16.
    assert_eq!(h.app.grid(), (40, 10));

    // The child's output reaches the screen and is drawn.
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.contains("hello from"))
    }));
    assert!(h.until(SECOND, |a| a.vt.is_active()));
    let frames = h.memory().frames;
    assert!(frames > 0, "a frame was presented");
    assert!(
        h.memory().pixels.iter().any(|&b| b != 0),
        "something was drawn"
    );

    // Keys go through the key order to the child, which echoes them.
    h.type_str("ok\r");
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.contains("got:ok"))
    }));

    // A release puts the device to sleep and lets the VT go; the
    // acquire wakes it and it is drawn whole.
    let ctrl_super_f12 = KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![0xffc9],
        codepoints: vec![None],
        repeat: false,
    };
    h.app
        .input_event(InputEvent::Key(ctrl_super_f12.clone()), now());
    assert!(
        h.until_memory(SECOND, |m| !m.awake),
        "asleep for the release"
    );
    assert!(
        h.until(SECOND, |a| !a.vt.is_active()),
        "the release acknowledged"
    );
    let before = h.memory().frames;
    h.app.input_event(InputEvent::Key(ctrl_super_f12), now());
    assert!(h.until(SECOND, |a| a.vt.is_active()));
    assert!(
        h.until_memory(SECOND, |m| m.awake && m.frames > before),
        "the acquire wakes the display and draws it again"
    );

    // After a second with no input the display blanks; a key wakes it.
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked after dpms-timeout"
    );
    h.type_str("x");
    assert_eq!(h.memory().unblanks, 1);
    assert_eq!(h.memory().dpms, Some(Dpms::On));

    // The child ends; under oneshot vtium ends, switching back first.
    h.type_str("\r");
    assert!(h.until(SECOND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::Oneshot));
    assert!(!h.app.vt.is_active(), "the VT was given back");
}

#[test]
fn a_command_that_fails_at_once_is_started_five_times_more() {
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", "exit 3"]);
    assert!(h.until(SECOND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::TooQuick));
    assert_eq!(h.app.finished().map(Exit::status), Some(1));
}

#[test]
fn a_repeat_is_dropped_once_the_program_resets_decarm() {
    let mut h = Harness::new(&[
        "--oneshot",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf '\\033[?8lready\\n'; read x; echo \"got:$x\"",
    ]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "ready")));
    h.type_keys("a", false);
    h.type_keys("b", true);
    h.type_keys("\r", false);
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.starts_with("got:"))
    }));
    assert!(
        h.app.screen_text().iter().any(|l| l == "got:a"),
        "{:?}",
        h.app.screen_text()
    );
}

#[test]
fn the_fonts_policy_follows_the_keys() {
    use vtium_font::sprite::Range;
    use vtium_font::{Missing, Source, SpritePolicy};
    let h = Harness::new(&[]);
    // The built-in unifont draws the sprites, as the C reference did.
    assert_eq!(
        h.app.font_policy(),
        Some((SpritePolicy::SPRITES, Missing::Replacement))
    );
    let h = Harness::new(&[
        "--glyph-source=font",
        "--glyph-source-braille=auto",
        "--missing=hex",
    ]);
    let want = SpritePolicy::all(Source::Font).with(Range::Braille, Source::Auto);
    assert_eq!(h.app.font_policy(), Some((want, Missing::Hex)));
}

#[test]
fn a_custom_palette_reaches_the_terminal() {
    use vtium_term::Rgb;
    let h = Harness::new(&[
        "--palette=custom",
        "--palette-red=1,2,3",
        "--palette-background=4,5,6",
    ]);
    let p = h.app.term.terminal.palette();
    assert_eq!(p.color(1), Rgb::new(1, 2, 3));
    assert_eq!(p.bg(), Rgb::new(4, 5, 6));
    // Unset keys keep the C reference's custom defaults.
    assert_eq!(p.color(4), Rgb::new(0, 0, 238));
    let h = Harness::new(&[]);
    assert_eq!(
        h.app.term.terminal.palette(),
        &vtium_term::Palette::default()
    );
}

#[test]
fn a_retaken_vt_sets_the_displays_up_again_and_draws_them_whole() {
    let mut h = Harness::with_vt(
        &["--login", "--", "/bin/sh", "-c", "printf 'held\\n'; read x"],
        ScriptedVt::default(),
    );
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "held")));
    assert!(h.until_memory(SECOND, |m| m.frames > 0 && m.age == 1));
    let (wake_ups, frames) = (h.memory().wake_ups, h.memory().frames);
    let lent = h.memory().ages.len();
    h.app.vt.events.push(VtEvent::Retaken);
    assert!(
        h.until_memory(SECOND, |m| m.wake_ups == wake_ups + 1 && m.frames > frames),
        "the device is set up again and a frame drawn after it"
    );
    // The wake-up made the buffer's contents unknown: the frame after it
    // was drawn whole, into a canvas of age 0.
    assert_eq!(h.memory().ages.get(lent), Some(&0));
    assert_eq!(h.memory().wake_ups, wake_ups + 1);
    assert!(h.app.vt.is_active());
}

/// Prints a render-timer line as the log writes it, for tier 3's parser:
/// `VTIUM_PRINT_TIMER=1 cargo test -p vtium --test boot render_timer -- --nocapture`.
#[test]
fn a_render_timer_line() {
    let mut t = vtium_render::RenderTimer::new();
    for _ in 0..30 {
        t.frame(Duration::from_micros(1500), true, 640 * 16, 2);
        t.flip();
    }
    let report = t
        .report(Duration::from_secs(1), u64::from(W) * u64::from(H))
        .expect("a second of frames");
    let line = vtium::app::render_timer_line(&report, Path::new("/dev/dri/card0"), DisplayId(42));
    assert!(line.starts_with("render-timer: interval_ms="), "{line}");
    assert!(line.ends_with(" display=/dev/dri/card0[42]"), "{line}");
    if std::env::var_os("VTIUM_PRINT_TIMER").is_some() {
        println!("[   12.345678] vtium[505]: terminal: {line}");
    }
}
