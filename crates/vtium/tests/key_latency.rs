//! The time from a key press to the frame that shows its echo, measured
//! through the seat as `main` runs it: the fake VT, a display in memory,
//! and a real program on a real pty that echoes each key it reads.
//!
//! vtium stamps each key it sends to the program, and a display stamps
//! the present of the first frame that carries output the program wrote
//! after that key. The time in between is the delay a person feels while
//! typing. The test here checks that each key's echo is measured once, at
//! a frame that shows it. The probe, which the test run leaves out, types
//! keys one at a time and reports the time at the median and the worst:
//!
//! ```sh
//! cargo build --release -p vtium --tests
//! taskset -c 7 cargo test --release -p vtium --test key_latency -- --ignored --nocapture
//! ```
//!
//! `VTIUM_PROBE_KEYS` sets how many keys the probe types, 500 by default.
//! The display presents each frame the moment it is swapped and reports
//! its flip at once, so the probe measures vtium's own path: the pty both
//! ways, the parser, the loop, the render thread's compose and the swap.
//! A real display adds the wait for its flip, which tier 3 measures in a
//! guest with a real KMS device.

use std::any::Any;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

const ID: DisplayId = DisplayId(3);

/// How long a wait on a state another thread or process brings may run
/// before the test fails: only a bound, since each wait ends at the state
/// it waits for.
const BOUND: Duration = Duration::from_secs(60);

/// The real time. The probe measures in it, and it bounds every wait.
#[expect(
    clippy::disallowed_methods,
    reason = "the probe measures real time, and a wait on another process is bounded in it"
)]
fn now() -> Instant {
    Instant::now()
}

/// A display in memory with two buffers, which it lends in turn, each as
/// old as the frames drawn since it was last drawn into. A swap with
/// damage shows the frame at once and reports its flip, and a swap
/// without damage sends nothing, as a real display does.
struct Display {
    size: (u32, u32),
    buffers: [Arc<Mutex<Vec<u8>>>; 2],
    /// The frame each buffer last held, `None` while it holds none.
    drawn: [Option<u32>; 2],
    back: usize,
    /// The frames shown.
    frames: u32,
    lent: bool,
    awake: bool,
    events: Vec<Event>,
}

impl Display {
    fn new(size: (u32, u32)) -> Display {
        let len = usize::try_from(size.0 * size.1 * 4).expect("a display's bytes");
        Display {
            size,
            buffers: [
                Arc::new(Mutex::new(vec![0; len])),
                Arc::new(Mutex::new(vec![0; len])),
            ],
            drawn: [None; 2],
            back: 0,
            frames: 0,
            lent: false,
            awake: false,
            events: Vec::new(),
        }
    }
}

impl Video for Display {
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
        Some(self.size)
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((72.0, 72.0))
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        self.lent = true;
        let age = self.drawn[self.back].map_or(0, |d| self.frames - d + 1);
        let memory: Arc<Shared> = self.buffers[self.back].clone();
        Some(Token::new(id, self.size, self.size.0 * 4, age, memory))
    }
    fn give_back(&mut self, _: Token) {
        self.lent = false;
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.lent = false;
        drop(token);
        if damage.is_empty() {
            return Ok(Swapped::Nothing);
        }
        self.frames += 1;
        self.drawn[self.back] = Some(self.frames);
        self.back = 1 - self.back;
        self.events.push(Event::Flipped(ID));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake && !self.lent
    }
    fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        let first = !self.awake && self.frames == 0;
        self.awake = true;
        self.drawn = [None; 2];
        self.events.push(if first {
            Event::Added(ID)
        } else {
            Event::Redraw(ID)
        });
        self.events.push(Event::Awake);
        Ok(())
    }
    fn unblank(&mut self) -> io::Result<()> {
        Ok(())
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

/// The program on the pty: it puts the terminal in raw mode with no echo
/// of its own, says it is ready, and then writes back each byte it reads
/// as soon as it reads it.
const ECHO: &str = "stty raw -echo; printf ready; exec cat";

/// The seat, its loop and the display's path.
struct Seat {
    lp: Loop<App>,
    app: App,
}

impl Seat {
    /// The seat on the fake VT, drawing the built-in font on a display of
    /// `size`, with [`ECHO`] as its program, once the program is ready.
    fn new(size: (u32, u32)) -> Seat {
        let args: Vec<OsString> = ["--font-size=16", "--login", "--", "/bin/sh", "-c", ECHO]
            .iter()
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let clock: vtium::term::Clock = Rc::new(now);
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            clock,
            env,
        );
        app.set_font_sources(Sources {
            frits: PathBuf::from("/nonexistent/vtium-test-frits"),
            trust: vtium::font::trust(),
            helper: None,
            vconsole_conf: PathBuf::from("/dev/null"),
            kernel_cmdline: PathBuf::from("/dev/null"),
        });
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(Display::new(size)));
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        let mut seat = Seat { lp, app };
        assert!(
            seat.until(|a| a.screen_text().iter().any(|l| l.contains("ready"))),
            "the program is ready: {:?}",
            seat.app.screen_text()
        );
        assert!(seat.until(|a| a.vt.is_active()), "the VT is shown");
        seat.settle();
        seat
    }

    /// Turns the loop as `main` does until `done` holds, at most
    /// [`BOUND`].
    fn until(&mut self, mut done: impl FnMut(&App) -> bool) -> bool {
        let end = now() + BOUND;
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

    /// Turns the loop until no frame is composing and the display has no
    /// event left to read, so the next key starts from a still screen.
    fn settle(&mut self) {
        assert!(
            self.until(|a| a.heads().iter().all(|h| h.frames_out() == 0)),
            "no frame composing"
        );
        self.app.turn(now());
    }

    fn display(&mut self) -> &mut Display {
        self.app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .expect("the display")
            .as_any()
            .downcast_mut::<Display>()
            .expect("the memory display")
    }

    /// The frames whose echo the shown head's displays presented, and the
    /// time of the last.
    fn echoes(&self) -> (u64, Option<Duration>) {
        let e = self.app.term().expect("a shown head").echoes();
        (e.count, e.last)
    }

    /// Types `c` as a key pressed with no modifier.
    fn press(&mut self, c: char) {
        let key = KeyEvent {
            keycode: 0,
            ascii: 0,
            unshifted: 0,
            shifted: 0,
            base: None,
            mods: Mods::empty(),
            keysyms: vec![u32::from(c)],
            codepoints: vec![Some(c)],
            repeat: false,
        };
        self.app.input_event(InputEvent::Key(key), now());
    }

    /// Types `c` and turns the loop until the display presents the frame
    /// that carries its echo. Returns the key-to-present time.
    fn echo(&mut self, c: char) -> Duration {
        let (before, _) = self.echoes();
        self.press(c);
        assert!(
            self.until(|a| a.term().is_some_and(|t| t.echoes().count > before)),
            "the echo of {c:?} was presented"
        );
        self.echoes().1.expect("a key-to-present time")
    }
}

impl Drop for Seat {
    /// Waits for the render threads the seat went on without, so none is
    /// still running when the test binary exits.
    fn drop(&mut self) {
        let _ = vtium::painter::wait_for_left(BOUND, &now);
    }
}

// Each key vtium sends to a program that echoes it is measured once, at
// the frame that shows the echo: the frame the display presents when the
// count goes up shows the key on the screen, and the frames that follow
// count no echo again.
#[test]
fn each_keys_echo_is_measured_once_at_the_frame_that_shows_it() {
    let mut seat = Seat::new((320, 160));
    assert_eq!(seat.echoes(), (0, None), "no key yet");
    for (i, c) in ('a'..='e').enumerate() {
        let frames = seat.display().frames;
        seat.echo(c);
        let typed: String = ('a'..=c).collect();
        assert!(
            seat.app.screen_text().iter().any(|l| l.contains(&typed)),
            "the screen shows {typed:?}: {:?}",
            seat.app.screen_text()
        );
        assert!(
            seat.display().frames > frames,
            "the echo of {c:?} went out in a frame"
        );
        seat.settle();
        let count = u64::try_from(i + 1).expect("a count");
        assert_eq!(seat.echoes().0, count, "one echo for each key so far");
    }
}

/// The probe: types [`VTIUM_PROBE_KEYS`] keys one at a time at a display
/// of 1366×768, minspec's size, and prints the key-to-present time at the
/// median and the worst. The first keys warm the glyph cache and the
/// pty, so the probe leaves them out of the figures.
#[test]
#[ignore = "a measurement, which `--ignored` runs"]
fn probe_key_to_present() {
    const WARM: usize = 20;
    let keys: usize = std::env::var("VTIUM_PROBE_KEYS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(500);
    let mut seat = Seat::new((1366, 768));
    let letters: Vec<char> = ('a'..='z').collect();
    let mut times = Vec::with_capacity(keys);
    for i in 0..WARM + keys {
        let time = seat.echo(letters[i % letters.len()]);
        seat.settle();
        if i >= WARM {
            times.push(time);
        }
    }
    times.sort_unstable();
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    let at = |q: f64| {
        let last = times.len() - 1;
        times[((last as f64) * q).round() as usize]
    };
    println!(
        "key to present over {} keys: median {:.0} us, p99 {:.0} us, worst {:.0} us",
        times.len(),
        us(at(0.5)),
        us(at(0.99)),
        us(times[times.len() - 1]),
    );
    assert_eq!(
        seat.echoes().0,
        (WARM + keys) as u64,
        "one echo for each key"
    );
}
