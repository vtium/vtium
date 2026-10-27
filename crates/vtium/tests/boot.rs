//! The seat booted with no hardware: the fake VT, a video device in
//! memory, and a real child on a real pty. It drives the wiring's paths
//! through the loop as `main` runs it: the child's output drawn, keys
//! through the key order to the child, a VT release and acquire, the DPMS
//! blank and its wake-up, the in-band reports a release and a zoom send a
//! program waiting in a read, and the exit with the switch back.

use std::any::Any;
use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium::app::{App, Exit};
use vtium::config;
use vtium::start::Start;
use vtium::video::Video;
use vtium_platform::display::{Canvas, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{Button, InputEvent, KeyEvent, Mods, PointerEvent};
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};

const W: u32 = 320;
const H: u32 = 160;
const ID: DisplayId = DisplayId(7);

/// A device with one display, its frame buffer in memory.
struct Memory {
    w: u32,
    h: u32,
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
    refresh_mhz: u32,
    /// Every swap fails, as on a display announced and never shown.
    refuses: bool,
}

impl Default for Memory {
    fn default() -> Memory {
        Memory::sized(W, H)
    }
}

impl Memory {
    fn sized(w: u32, h: u32) -> Memory {
        Memory {
            w,
            h,
            pixels: Vec::new(),
            events: Vec::new(),
            awake: false,
            dpms: None,
            frames: 0,
            age: 0,
            unblanks: 0,
            wake_ups: 0,
            ages: Vec::new(),
            refresh_mhz: 60_000,
            refuses: false,
        }
    }
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
        Some((self.w, self.h))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(self.refresh_mhz)
    }
    fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
        if !self.awake || self.dpms != Some(Dpms::On) {
            return None;
        }
        let age = self.age;
        self.ages.push(age);
        Some(Canvas {
            pixels: &mut self.pixels,
            width: self.w,
            height: self.h,
            stride: self.w * 4,
            age,
        })
    }
    fn swap(&mut self, id: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        if self.refuses {
            return Err(SwapError::Failed(io::Error::other("refused")));
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
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
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
    fn new(args: &[&str]) -> Harness {
        Harness::with_vt(args, FakeVt::new())
    }
}

impl Harness {
    fn with_vt<V: Vt + 'static>(args: &[&str], vt: V) -> Harness {
        Harness::build(args, vt, Box::new(Memory::default()))
    }

    /// The seat on `vt` with `video` as `/dev/mem0`.
    fn build<V: Vt + 'static>(args: &[&str], vt: V, video: Box<dyn Video>) -> Harness {
        Harness::boot(args, vt, Some(video))
    }

    /// The seat on the fake VT with no video device.
    fn bare(args: &[&str]) -> Harness {
        Harness::boot(args, FakeVt::new(), None)
    }

    fn boot<V: Vt + 'static>(args: &[&str], vt: V, video: Option<Box<dyn Video>>) -> Harness {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the harness's command line");
        // What `main` takes before the seat; the seat's input is the
        // test's.
        let _ = Start::new(&loaded.config, "C".to_owned());
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let ahead = Rc::new(Cell::new(Duration::ZERO));
        let clock = {
            let ahead = ahead.clone();
            Rc::new(move || now() + ahead.get())
        };
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), clock, env);
        if let Some(video) = video {
            app.add_video(PathBuf::from("/dev/mem0"), video);
        }
        app.insert_blank();
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Harness { lp, app, ahead }
    }

    /// The seat's time: the real time, or ahead of it after [`Harness::skip`].
    fn at(&self) -> Instant {
        now() + self.ahead.get()
    }

    /// Moves the seat's clock ahead by `by`, as if that long passed with
    /// nothing to do.
    fn skip(&mut self, by: Duration) {
        self.ahead.set(self.ahead.get() + by);
    }

    /// Turns the loop until `done` holds, at most `limit`.
    fn until(&mut self, limit: Duration, mut done: impl FnMut(&App) -> bool) -> bool {
        let end = now() + limit;
        while now() < end {
            if done(&self.app) {
                return true;
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
        done(&self.app)
    }

    /// The memory device, or the one a late or gated device holds.
    fn memory(&mut self) -> &mut Memory {
        let any = self
            .app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .expect("the memory device")
            .as_any();
        if any.is::<Late>() {
            return &mut any.downcast_mut::<Late>().expect("a late device").memory;
        }
        if any.is::<Gate>() {
            return &mut any.downcast_mut::<Gate>().expect("a gated device").memory;
        }
        any.downcast_mut::<Memory>().expect("the memory device")
    }

    fn late(&mut self) -> &mut Late {
        self.app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .and_then(|v| v.as_any().downcast_mut::<Late>())
            .expect("the late device")
    }

    /// The scripted VT the harness was built with.
    fn scripted(&mut self) -> &mut ScriptedVt {
        self.app.vt.get::<ScriptedVt>().expect("a scripted VT")
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
            self.app.input_event(InputEvent::Key(key), self.at());
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
    // A slow run can blank before the first key too, which that key woke.
    let unblanks = h.memory().unblanks;
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked after dpms-timeout"
    );
    h.type_str("x");
    assert_eq!(h.memory().unblanks, unblanks + 1);
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

// covers: colour.palette.named
#[test]
fn a_named_palette_reaches_the_terminal() {
    let h = Harness::new(&["--palette=solarized-white"]);
    assert_eq!(
        Some(h.app.term.terminal.palette()),
        vtium_term::Palette::named("solarized-white").as_ref()
    );
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
    h.scripted().events.push(VtEvent::Retaken);
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

#[test]
fn a_retake_while_blanked_waits_dark_for_input() {
    let mut h = Harness::with_vt(
        &[
            "--dpms-timeout=1",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked after dpms-timeout"
    );
    let wake_ups = h.memory().wake_ups;
    h.scripted().events.push(VtEvent::Retaken);
    h.until(Duration::from_millis(200), |_| false);
    // Set up again only when input ends the blank, as a hotplug while
    // blanked is: until then the displays stay dark.
    assert_eq!(h.memory().wake_ups, wake_ups, "no set-up while blanked");
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "still dark");
    h.type_str("a");
    assert_eq!(h.memory().wake_ups, wake_ups + 1, "set up at the input");
    assert_eq!(h.memory().dpms, Some(Dpms::On));
    // Without a re-take, input only unblanks.
    assert!(h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)));
    h.type_str("b");
    assert_eq!(h.memory().wake_ups, wake_ups + 1);
}

/// A seat that blanks after 3 s with no input, on the scripted VT.
fn blanking() -> Harness {
    Harness::with_vt(
        &[
            "--dpms-timeout=3",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        ScriptedVt::default(),
    )
}

#[test]
fn a_retake_keeps_the_blanks_countdown() {
    let mut h = blanking();
    let start = now();
    h.until(Duration::from_secs(2), |_| false);
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit before the timeout");
    let wake_ups = h.memory().wake_ups;
    h.scripted().events.push(VtEvent::Retaken);
    // The re-take sets the display up again and is not activity: the
    // blank comes 3 s after the start, not 3 s after the re-take.
    assert!(
        h.until_memory(Duration::from_secs(2), |m| m.dpms == Some(Dpms::Off)),
        "blanked at the start's timeout, {:?} after the start",
        now() - start
    );
    assert_eq!(h.memory().wake_ups, wake_ups + 1, "set up again");
}

#[test]
fn an_acquire_restarts_the_blanks_countdown() {
    let mut h = blanking();
    h.until(Duration::from_secs(2), |_| false);
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(SECOND, |a| !a.vt.is_active()), "released");
    let wake_ups = h.memory().wake_ups;
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.until_memory(SECOND, |m| m.wake_ups == wake_ups + 1),
        "woken by the acquire"
    );
    // The switch back is the user's: the countdown starts again from it.
    assert!(
        !h.until_memory(Duration::from_secs(2), |m| m.dpms == Some(Dpms::Off)),
        "lit 2 s after the acquire"
    );
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked 3 s after the acquire"
    );
}

#[test]
fn a_retakes_wake_up_landing_after_the_blank_leaves_it_dark() {
    let late = Late {
        memory: Memory::default(),
        held: None,
    };
    let mut h = Harness::build(
        &[
            "--dpms-timeout=1",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        ScriptedVt::default(),
        Box::new(late),
    );
    assert!(h.until_memory(SECOND, |m| m.frames > 0), "drawn");
    // The re-take's wake-up retries past the blank.
    h.late().held = Some(Event::Awake);
    h.scripted().events.push(VtEvent::Retaken);
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked after dpms-timeout"
    );
    // Its modeset lights the display, and its `Awake` arrives.
    h.memory().dpms = Some(Dpms::On);
    h.late().held = None;
    assert!(
        h.until_memory(SECOND, |m| m.events.is_empty()),
        "the `Awake` taken"
    );
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "dark again");
    // Still blanked: the next key unblanks.
    h.type_str("a");
    assert_eq!(h.memory().unblanks, 1);
    assert_eq!(h.memory().dpms, Some(Dpms::On));
}

fn key(mods: Mods, keysym: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods,
        keysyms: vec![keysym],
        codepoints: vec![None],
        repeat: false,
    })
}

#[test]
fn a_larger_display_draws_a_larger_font_on_the_same_grid() {
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", "read x"]);
    h.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::sized(640, 320)),
    );
    h.app.vt.activate().ok();
    assert!(h.until(SECOND, |a| a.cells().len() == 2));
    // 320x160 holds 40x10 cells of 8x16; 640x320 holds 40x10 of 16x32.
    assert_eq!(h.app.grid(), (40, 10));
    assert_eq!(h.app.cells(), [(8, 16), (16, 32)]);
}

#[test]
fn zoom_and_rotate_change_the_grid() {
    let mut h = Harness::new(&["--font-size=16", "--login", "--", "/bin/sh", "-c", "read x"]);
    assert_eq!(h.app.grid(), (40, 10));
    let equal = 0x3d;
    let minus = 0x2d;
    h.app.input_event(key(Mods::CTRL, equal), now());
    assert_eq!(h.app.grid(), (20, 5), "zoomed in to 16x32 cells");
    assert_eq!(h.app.cells(), [(16, 32)]);
    h.app.input_event(key(Mods::CTRL, minus), now());
    assert_eq!(h.app.grid(), (40, 10), "and back");
    // Logo+= turns the display a quarter: 160 across, 320 down.
    h.app.input_event(key(Mods::LOGO, equal), now());
    assert_eq!(h.app.grid(), (20, 20));
    h.app.input_event(key(Mods::LOGO, minus), now());
    assert_eq!(h.app.grid(), (40, 10));
    // The keys reach no program.
    assert!(h.until(SECOND, |a| a.screen_text().iter().all(|l| l.is_empty())));
}

/// A device whose flips take 100 ms and report no completion, as a lost
/// flip event does until the platform's flip wait ends it.
struct SlowFlip {
    memory: Memory,
    swapped: Option<Instant>,
}

impl Video for SlowFlip {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        let mut events = self.memory.events();
        events.retain(|e| !matches!(e, Event::Flipped(_)));
        events
    }
    fn displays(&self) -> Vec<DisplayId> {
        self.memory.displays()
    }
    fn name(&self, id: DisplayId) -> String {
        self.memory.name(id)
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        self.memory.size(id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        self.memory.refresh_mhz(id)
    }
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        self.memory.canvas(id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.swapped = Some(now());
        self.memory.swap(id, damage)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.swapped
            .is_some_and(|t| now() < t + Duration::from_millis(100))
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.memory.set_dpms(id, dpms)
    }
    fn sleep(&mut self) {
        self.memory.sleep();
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.memory.wake_up()
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.memory.unblank()
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

/// A coalesced draw that comes due while a flip is in flight ends its
/// wait: the loop is not told to turn again at once for it.
#[test]
fn a_due_draw_held_by_a_flip_does_not_spin_the_loop() {
    let slow = SlowFlip {
        memory: Memory::default(),
        swapped: None,
    };
    let mut h = Harness::build(
        &[
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x; i=0; while [ $i -lt 100 ]; do head -c 20000 /dev/zero | tr '\\0' x; sleep 0.03; i=$((i+1)); done; read y",
        ],
        FakeVt::new(),
        Box::new(slow),
    );
    assert!(h.until(SECOND, |a| a.vt.is_active()));
    h.until(Duration::from_millis(200), |_| false);
    h.type_str("\r");
    let end = now() + Duration::from_millis(1500);
    let (mut turns, mut zero) = (0_u32, 0_u32);
    while now() < end {
        let wait = h.app.next_wake(now());
        if wait == Some(Duration::ZERO) {
            zero += 1;
        }
        let wait = wait.map(|w| w.min(Duration::from_millis(200)));
        h.lp.dispatch(wait, &mut h.app).expect("dispatch");
        h.app.turn(now());
        turns += 1;
    }
    assert!(h.app.screen_text().iter().any(|l| l.contains("xxxx")));
    // A timer that falls due between a turn and the wait asks for none
    // now and then; a spin asks for none on every turn.
    assert!(
        zero < 10 && turns < 1000,
        "told not to wait {zero} times of {turns} turns"
    );
}

fn pointer(event: PointerEvent) -> InputEvent {
    InputEvent::Pointer(event)
}

fn moved(x: i32) -> InputEvent {
    pointer(PointerEvent::Moved {
        x,
        y: 0,
        held: None,
    })
}

fn button(pressed: bool) -> InputEvent {
    pointer(PointerEvent::Button {
        button: Button::Left,
        pressed,
        double_click: false,
    })
}

/// A press and a release reach a child that is not reading, behind
/// keys that fill the queue: motion after them coalesces, buttons never.
#[test]
fn button_reports_are_queued_whole_while_the_child_does_not_read() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf '\\033[?1003h\\033[?1006hready'; sleep 2; \
         awk 'BEGIN{RS=\"q\"}{p=gsub(/<0;[0-9]+;[0-9]+M/,\"\");r=gsub(/<0;[0-9]+;[0-9]+m/,\"\");printf \"press=%d release=%d\\r\\n\",p,r;exit}'; read x",
    ]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "ready")));
    let queued = |h: &Harness| h.app.pty.as_ref().map_or(0, |p| p.queued());
    // Keys fill the pty and then the queue past the point motion
    // coalesces from.
    let mut keys = 0;
    while queued(&h) < vtium_platform::MOTION_COALESCE_AT && keys < 1_000_000 {
        keys += 1;
        h.type_str("a");
    }
    assert!(
        queued(&h) >= vtium_platform::MOTION_COALESCE_AT,
        "the queue filled"
    );
    for event in [
        moved(0),
        moved(8),
        button(true),
        moved(0),
        moved(8),
        button(false),
        moved(0),
    ] {
        h.app.input_event(event, now());
    }
    h.type_str("q");
    assert!(
        h.until(Duration::from_secs(10), |a| a
            .screen_text()
            .iter()
            .any(|l| l.contains("press="))),
        "{:?}",
        h.app.screen_text()
    );
    assert!(
        h.app
            .screen_text()
            .iter()
            .any(|l| l.ends_with("press=1 release=1")),
        "{:?}",
        h.app.screen_text()
    );
}

/// A wheel event is one report while the program tracks the pointer, and
/// three lines of scrollback while it does not, however many steps it
/// carries.
#[test]
fn a_wheel_event_is_one_report_or_three_lines() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "i=0; while [ $i -lt 40 ]; do echo line$i; i=$((i+1)); done; \
         stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{n=gsub(/<64;/,\"\");printf \"wheel=%d\\r\\n\",n;exit}'; read x",
    ]);
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.ends_with("ready"))
    }));
    h.app
        .input_event(pointer(PointerEvent::Wheel(i32::MAX)), now());
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.ends_with("wheel=1"))),
        "{:?}",
        h.app.screen_text()
    );
    // The program stops tracking: the wheel scrolls back three lines.
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "i=0; while [ $i -lt 40 ]; do echo line$i; i=$((i+1)); done; read x",
    ]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "line39")));
    h.app
        .input_event(pointer(PointerEvent::Wheel(i32::MAX)), now());
    assert_eq!(h.app.term.terminal.screen().scrolled_back(), 3);
    h.app.input_event(pointer(PointerEvent::Wheel(-1)), now());
    assert_eq!(h.app.term.terminal.screen().scrolled_back(), 0);
}

/// The pointer in the middle of cell `col`, `row` (8 by 16 pixels), with
/// `held` pressed.
fn over(col: i32, row: i32, held: Option<Button>) -> PointerEvent {
    PointerEvent::Moved {
        x: col * 8 + 4,
        y: row * 16 + 8,
        held,
    }
}

fn press(button: Button, double_click: bool) -> PointerEvent {
    PointerEvent::Button {
        button,
        pressed: true,
        double_click,
    }
}

fn release(button: Button) -> PointerEvent {
    PointerEvent::Button {
        button,
        pressed: false,
        double_click: false,
    }
}

impl Harness {
    /// Pointer events, each with `mods` held.
    fn point(&mut self, events: impl IntoIterator<Item = PointerEvent>, mods: Mods) {
        for event in events {
            self.app.pointer_event(event, mods, self.at());
        }
    }
}

/// Whether a row of the screen reads `text`.
fn shows(app: &App, text: &str) -> bool {
    app.screen_text().iter().any(|l| l == text)
}

/// A drag across two lines selects them, its release copies them, and the
/// middle button pastes them back to the program.
// covers: clipboard.selection, clipboard.paste
#[test]
fn a_drag_across_two_lines_copies_them_and_the_middle_button_pastes_them() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'first line\\nsecond one\\n'; read x; read y; echo \"got:$x|$y\"; read z",
    ]);
    assert!(h.until(SECOND, |a| shows(a, "second one")));
    h.point(
        [
            over(0, 0, None),
            press(Button::Left, false),
            over(4, 1, Some(Button::Left)),
            over(9, 1, Some(Button::Left)),
            release(Button::Left),
        ],
        Mods::empty(),
    );
    assert_eq!(h.app.selection(), Some(&b"first line\nsecond one"[..]));
    h.point(
        [press(Button::Middle, false), release(Button::Middle)],
        Mods::empty(),
    );
    h.type_str("\r");
    assert!(
        h.until(SECOND, |a| shows(a, "got:first line|second one")),
        "{:?}",
        h.app.screen_text()
    );
}

/// A double click selects a word and a triple click a line, each copied
/// as it is made; Shift+Insert pastes as the middle button does. A click
/// with no drag copies nothing.
// covers: clipboard.selection, clipboard.paste
#[test]
fn a_double_click_selects_a_word_and_a_triple_click_a_line() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'alpha beta gamma\\nsecond line here\\n'; read x; read y; echo \"got:$x|$y\"; read z",
    ]);
    assert!(h.until(SECOND, |a| shows(a, "second line here")));
    let none = Mods::empty();
    h.point(
        [
            over(7, 0, None),
            press(Button::Left, false),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"beta"[..]));
    // A key clears the selection, and the buffer keeps it.
    assert!(h.app.term.terminal.screen().has_selection());
    h.app.input_event(key(Mods::SHIFT, 0xff63), now());
    assert!(!h.app.term.terminal.screen().has_selection());
    h.type_str("\r");
    h.point(
        [
            over(3, 1, None),
            press(Button::Left, false),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(
        h.app.selection(),
        Some(&b"beta"[..]),
        "a click copies nothing"
    );
    h.point(
        [
            press(Button::Left, true),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"second line here\n"[..]));
    h.point(
        [press(Button::Middle, false), release(Button::Middle)],
        none,
    );
    assert!(
        h.until(SECOND, |a| shows(a, "got:beta|second line here")),
        "{:?}",
        h.app.screen_text()
    );
}

/// While a program tracks the pointer, a drag and the middle button with
/// Shift held select and paste, as xterm's do, and reach the program only
/// as the paste, bracketed; without Shift the buttons are reported.
// covers: clipboard.selection, clipboard.paste, input.shift-override
#[test]
fn shift_selects_and_pastes_while_the_program_tracks_the_pointer() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf 'pick me\\r\\n\\033[?1000h\\033[?1006h\\033[?2004hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read x",
    ]);
    assert!(h.until(SECOND, |a| shows(a, "ready")));
    h.point(
        [
            over(0, 0, None),
            press(Button::Left, false),
            over(6, 0, Some(Button::Left)),
            release(Button::Left),
            press(Button::Middle, false),
            release(Button::Middle),
        ],
        Mods::SHIFT,
    );
    assert_eq!(h.app.selection(), Some(&b"pick me"[..]));
    h.point(
        [press(Button::Left, false), release(Button::Left)],
        Mods::empty(),
    );
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| shows(
            a,
            "E[200~pick meE[201~E[<0;7;1ME[<0;7;1m"
        )),
        "{:?}",
        h.app.screen_text()
    );
}

/// A press in the strip past the grid's last cell selects from that cell,
/// so a new selection never extends the one before it.
// covers: clipboard.selection
#[test]
fn a_press_past_the_grid_selects_from_its_last_cell() {
    let mut h = Harness::build(
        &[
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "printf 'alpha beta gamma\\n'; read x",
        ],
        FakeVt::new(),
        Box::new(Memory::sized(326, 168)),
    );
    assert_eq!(h.app.grid(), (40, 10));
    assert!(h.until(SECOND, |a| shows(a, "alpha beta gamma")));
    let none = Mods::empty();
    h.point(
        [
            over(7, 0, None),
            press(Button::Left, false),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"beta"[..]));
    h.point(
        [
            PointerEvent::Moved {
                x: 323,
                y: 8,
                held: None,
            },
            press(Button::Left, false),
            over(0, 0, Some(Button::Left)),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"alpha beta gamma"[..]));
}

/// A drag ends when its button is no longer held, as when its release was
/// lost, and when the VT is released.
#[test]
fn a_drag_ends_without_its_button_and_at_a_vt_release() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'first line\\nsecond one\\n'; read x",
    ]);
    assert!(h.until(SECOND, |a| shows(a, "second one")));
    let none = Mods::empty();
    let selected = |h: &Harness| h.app.term.terminal.screen().selection_text();
    h.point(
        [
            over(0, 0, None),
            press(Button::Left, false),
            over(4, 0, Some(Button::Left)),
            over(5, 1, None),
            over(9, 1, Some(Button::Left)),
        ],
        none,
    );
    assert_eq!(selected(&h).as_deref(), Some("first"));
    h.point(
        [
            over(0, 1, None),
            press(Button::Left, false),
            over(2, 1, Some(Button::Left)),
        ],
        none,
    );
    assert_eq!(selected(&h).as_deref(), Some("sec"));
    let switch = key(Mods::CTRL.union(Mods::LOGO), 0xffc9);
    h.app.input_event(switch.clone(), now());
    assert!(h.until_memory(SECOND, |m| !m.awake), "asleep");
    assert!(h.until(SECOND, |a| !a.vt.is_active()), "released");
    h.app.input_event(switch, now());
    assert!(h.until(SECOND, |a| a.vt.is_active()), "acquired");
    h.point([over(9, 1, Some(Button::Left))], none);
    assert_eq!(selected(&h).as_deref(), Some("sec"));
}

/// A selection a program sets with OSC 52 is pasted as any paste is:
/// filtered, so its end marker and C0 controls arrive as spaces, each LF
/// as CR, and bracketed. Shift+Insert pastes once a press, however long
/// it repeats; the middle button pastes at its release, as xterm's does.
// covers: clipboard.paste, osc.52.write, input.paste-end-marker
#[test]
fn a_hostile_selection_is_pasted_filtered_once_a_press() {
    let mut h = Harness::build(
        &[
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "stty raw -echo; \
             printf '\\033]52;c;%s\\a' \"$(printf 'a\\033[201~b\\003c\\nd' | base64)\"; \
             printf '\\033[?2004hready'; \
             awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");gsub(/\\r/,\"R\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read x",
        ],
        FakeVt::new(),
        Box::new(Memory::sized(640, 160)),
    );
    assert!(h.until(SECOND, |a| shows(a, "ready")));
    assert_eq!(h.app.selection(), Some(&b"a\x1b[201~b\x03c\nd"[..]));
    let shift_insert = |repeat: bool| {
        InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            mods: Mods::SHIFT,
            keysyms: vec![0xff63],
            codepoints: vec![None],
            repeat,
        })
    };
    h.app.input_event(shift_insert(false), now());
    h.app.input_event(shift_insert(true), now());
    h.app.input_event(shift_insert(true), now());
    h.point([press(Button::Middle, false)], Mods::empty());
    h.type_str("X");
    h.point([release(Button::Middle)], Mods::empty());
    h.type_str("q");
    let paste = "E[200~a [201~b cRdE[201~";
    assert!(
        h.until(SECOND, |a| shows(a, &format!("{paste}X{paste}"))),
        "{:?}",
        h.app.screen_text()
    );
}

/// The program changing its mouse tracking clears the selection.
#[test]
fn a_change_of_mouse_tracking_clears_the_selection() {
    let go = std::env::temp_dir().join(format!("vtium-boot-tracking-{}", std::process::id()));
    let _ = std::fs::remove_file(&go);
    let script = format!(
        "printf 'text\\n'; while [ ! -e '{}' ]; do sleep 0.02; done; \
         printf '\\033[?1000hon\\n'; read x",
        go.display()
    );
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", &script]);
    assert!(h.until(SECOND, |a| shows(a, "text")));
    h.point(
        [
            over(0, 0, None),
            press(Button::Left, false),
            over(3, 0, Some(Button::Left)),
            release(Button::Left),
        ],
        Mods::empty(),
    );
    assert!(h.app.term.terminal.screen().has_selection());
    std::fs::write(&go, b"").expect("the go file");
    let cleared = h.until(SECOND, |a| {
        shows(a, "on") && !a.term.terminal.screen().has_selection()
    });
    let _ = std::fs::remove_file(&go);
    assert!(cleared, "{:?}", h.app.screen_text());
    assert_eq!(h.app.selection(), Some(&b"text"[..]));
}

/// Every key a file may set is read by the seat as `main` starts it, or
/// listed as not built.
#[test]
fn every_key_a_file_sets_is_read_or_listed_as_not_built() {
    use vtium::config::{KEYS, Scope, keys};
    // The custom palette's keys are read only under palette=custom.
    let h = Harness::new(&[
        "--palette=custom",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read x",
    ]);
    let read = h.app.config().read();
    let unread: Vec<&str> = KEYS
        .iter()
        .filter(|k| k.scope == Scope::Anywhere)
        .map(|k| k.name)
        .filter(|name| !read.contains(name))
        .filter(|name| !keys::NOT_BUILT.iter().any(|k| k.name == *name))
        .collect();
    assert_eq!(unread, Vec::<&str>::new());
    let readme = include_str!("../README.md");
    let not_built = readme
        .split("## Not yet built")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .expect("the README's Not yet built");
    for key in keys::NOT_BUILT {
        assert!(
            not_built.contains(&format!("`{}`", key.name)),
            "{}",
            key.name
        );
    }
}

/// Output of 16 KiB or more a turn draws at most once a refresh: the
/// app asks coalescing before it draws for output.
#[test]
fn a_long_write_draws_at_most_once_a_refresh() {
    // 20 Hz, the slowest refresh coalescing takes: 50 ms a frame.
    let slow = Memory {
        refresh_mhz: 20_000,
        ..Memory::default()
    };
    let mut h = Harness::build(
        &[
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x; i=0; while [ $i -lt 100 ]; do head -c 17000 /dev/zero | tr '\\0' x; sleep 0.01; i=$((i+1)); done; echo; echo done; read y",
        ],
        FakeVt::new(),
        Box::new(slow),
    );
    assert!(h.until(SECOND, |a| a.vt.is_active()));
    h.until(Duration::from_millis(200), |_| false);
    let before = h.memory().frames;
    let start = now();
    h.type_str("\r");
    assert!(h.until(Duration::from_secs(30), |a| {
        a.screen_text().iter().any(|l| l == "done")
    }));
    let elapsed = now() - start;
    let frames = h.memory().frames - before;
    // A frame each 50 ms, less a fifth for the timing, and a few for the
    // key and the end; drawn each turn, it is a frame each burst.
    let most = u32::try_from(elapsed.as_millis() / 40).unwrap_or(u32::MAX) + 4;
    assert!(frames <= most, "{frames} frames in {elapsed:?}");
}

/// A device whose events of one kind wait until the test lets them
/// through: an `Asleep` as a flip in flight delays a DRM device's, an
/// `Awake` as a wake-up's retries do.
struct Late {
    memory: Memory,
    held: Option<Event>,
}

impl Video for Late {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        let (late, events) = self
            .memory
            .events()
            .into_iter()
            .partition(|e| Some(*e) == self.held);
        self.memory.events = late;
        events
    }
    fn displays(&self) -> Vec<DisplayId> {
        self.memory.displays()
    }
    fn name(&self, id: DisplayId) -> String {
        self.memory.name(id)
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        self.memory.size(id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        self.memory.refresh_mhz(id)
    }
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        self.memory.canvas(id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(id, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        self.memory.is_flipping(id)
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.memory.set_dpms(id, dpms)
    }
    fn sleep(&mut self) {
        self.memory.sleep();
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.memory.wake_up()
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.memory.unblank()
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

/// A release is acknowledged once every device reports asleep, however
/// many turns the last takes.
#[test]
fn a_release_waits_for_every_device_to_sleep() {
    let late = Late {
        memory: Memory::default(),
        held: Some(Event::Asleep),
    };
    let mut h = Harness::build(
        &["--login", "--", "/bin/sh", "-c", "read x"],
        ScriptedVt::default(),
        Box::new(late),
    );
    h.app
        .add_video(PathBuf::from("/dev/mem1"), Box::new(Memory::default()));
    assert!(h.until(SECOND, |a| a.cells().len() == 2));
    h.scripted().events.push(VtEvent::Release);
    h.until(Duration::from_millis(200), |_| false);
    assert!(
        h.app.vt.is_active(),
        "acknowledged before every device slept"
    );
    h.late().held = None;
    assert!(
        h.until(SECOND, |a| !a.vt.is_active()),
        "acknowledged once the last device slept"
    );
}

/// The next session starts from a hard reset, drawn at once: the screen
/// the last session left is not shown until the new one writes.
#[test]
fn a_restart_draws_the_reset_screen() {
    let flag = std::env::temp_dir().join(format!("vtium-restart-{}", std::process::id()));
    std::fs::remove_file(&flag).ok();
    let script = format!(
        "if [ -e {f} ]; then exec sleep 30; fi; : > {f}; printf 'first'; sleep 0.3",
        f = flag.display()
    );
    // "irst", in the first row's cells 1 to 4, past the cursor's cell.
    let irst = |m: &Memory| {
        (0..16).any(|y| {
            let row = (y * W * 4) as usize;
            m.pixels[row + 8 * 4..row + 40 * 4].iter().any(|&b| b != 0)
        })
    };
    let mut h = Harness::new(&["--no-blink", "--login", "--", "/bin/sh", "-c", &script]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "first")));
    assert!(h.until_memory(SECOND, irst), "the first session drawn");
    assert!(h.until(SECOND, |a| a.screen_text().iter().all(|l| l.is_empty())));
    assert!(
        h.until_memory(SECOND, |m| !irst(m)),
        "the reset screen is drawn"
    );
    assert!(!h.app.term.unparsed);
    std::fs::remove_file(&flag).ok();
}

/// A device whose flip lasts until the test ends it, with a `Flipped`.
struct Gate {
    memory: Memory,
    flipping: bool,
}

impl Video for Gate {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        self.memory.events()
    }
    fn displays(&self) -> Vec<DisplayId> {
        self.memory.displays()
    }
    fn name(&self, id: DisplayId) -> String {
        self.memory.name(id)
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        self.memory.size(id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        self.memory.refresh_mhz(id)
    }
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        self.memory.canvas(id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(id, damage)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.flipping
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.memory.set_dpms(id, dpms)
    }
    fn sleep(&mut self) {
        self.memory.sleep();
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.memory.wake_up()
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.memory.unblank()
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

/// A display's flip ending draws that display's waiting frame, and no
/// other display.
#[test]
fn a_flip_draws_only_its_own_display() {
    let gate = Gate {
        memory: Memory::default(),
        flipping: false,
    };
    let mut h = Harness::build(
        &[
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x; echo got; read y",
        ],
        FakeVt::new(),
        Box::new(gate),
    );
    h.app
        .add_video(PathBuf::from("/dev/mem1"), Box::new(Memory::default()));
    assert!(h.until(SECOND, |a| a.cells().len() == 2));
    h.until(Duration::from_millis(200), |_| false);
    fn gated(h: &mut Harness) -> &mut Gate {
        h.app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .and_then(|v| v.as_any().downcast_mut::<Gate>())
            .expect("the gated device")
    }
    fn other(h: &mut Harness) -> &mut Memory {
        h.app
            .videos
            .get_mut(Path::new("/dev/mem1"))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("the other device")
    }
    gated(&mut h).flipping = true;
    h.type_str("\r");
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "got")));
    h.until(Duration::from_millis(200), |_| false);
    let (frames, lent) = (gated(&mut h).memory.frames, other(&mut h).ages.len());
    let g = gated(&mut h);
    g.flipping = false;
    g.memory.events.push(Event::Flipped(ID));
    assert!(
        h.until_memory(SECOND, |m| m.frames > frames),
        "the waiting frame drawn"
    );
    assert_eq!(
        other(&mut h).ages.len(),
        lent,
        "the other display left alone"
    );
}

/// A selection a program sets with OSC 52 is taken and held; with
/// `no-clipboard-write` it is dropped.
#[test]
fn osc_52_is_held_or_dropped_as_clipboard_write_says() {
    let set = "printf '\\033]52;c;aGVsbG8=\\007set\\n'; read x";
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", set]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "set")));
    assert_eq!(h.app.selection(), Some(&b"hello"[..]));
    let mut h = Harness::new(&[
        "--no-clipboard-write",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        set,
    ]);
    assert!(h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "set")));
    assert_eq!(h.app.selection(), None);
    assert_eq!(
        h.app.term.terminal.take_clipboard_write(),
        None,
        "taken and dropped"
    );
}

/// The command line of the waits for a display: a second's wait.
const WAITING: [&str; 6] = [
    "--display-wait=1",
    "--login",
    "--",
    "/bin/sh",
    "-c",
    "read x",
];

/// No display comes within `display-wait`: vtium exits with status 1,
/// once the wait has run, so the unit's `OnFailure=` can start a getty.
#[test]
fn with_no_display_by_the_wait_vtium_exits_1() {
    let started = now();
    let mut h = Harness::bare(&WAITING);
    assert!(h.until(Duration::from_secs(3), |a| a.finished().is_some()));
    assert!(
        now() - started >= Duration::from_secs(1),
        "the wait was run"
    );
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    assert_eq!(h.app.finished().map(Exit::status), Some(1));
    assert!(!h.app.vt.is_active(), "the VT was given back");
}

/// `display-wait=0` does not wait.
#[test]
fn with_no_wait_and_no_display_vtium_exits_at_once() {
    let mut h = Harness::bare(&[
        "--display-wait=0",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read x",
    ]);
    assert!(h.until(Duration::from_millis(500), |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
}

/// A display that comes during the wait ends it: vtium runs past the
/// wait and draws on it.
#[test]
fn a_display_coming_during_the_wait_ends_it() {
    let mut h = Harness::bare(&WAITING);
    h.until(Duration::from_millis(300), |_| false);
    assert_eq!(h.app.finished(), None);
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until_memory(SECOND, |m| m.frames > 0), "drawn");
    h.until(Duration::from_millis(1500), |a| a.finished().is_some());
    assert_eq!(h.app.finished(), None, "still running past the wait");
}

/// A device that can't be used, its displays never set up, counts as
/// none.
#[test]
fn a_device_whose_displays_never_come_on_counts_as_none() {
    let mut h = Harness::bare(&WAITING);
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Unusable));
    assert!(h.until(Duration::from_secs(3), |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
}

/// Once running, a display that goes is hotplug: vtium keeps running
/// with none, past the wait, and draws on the next that comes.
#[test]
fn a_display_gone_after_the_start_is_hotplug() {
    let mut h = Harness::new(&WAITING);
    // A frame on screen: its flip read, which ends the start's wait.
    assert!(h.until_memory(SECOND, |m| m.frames > 0 && m.events.is_empty()));
    h.app.remove_video(Path::new("/dev/mem0"));
    assert!(h.app.cells().is_empty(), "no display");
    h.until(Duration::from_millis(1500), |a| a.finished().is_some());
    assert_eq!(h.app.finished(), None, "still running with none");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(
        h.until_memory(SECOND, |m| m.frames > 0),
        "the new display drawn"
    );
    assert_eq!(h.app.finished(), None);
}

/// A device whose wake-up fails, as a card whose modeset is refused:
/// it reports no display.
struct Unusable;

impl Video for Unusable {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        Vec::new()
    }
    fn displays(&self) -> Vec<DisplayId> {
        Vec::new()
    }
    fn name(&self, _: DisplayId) -> String {
        "MEM-1".into()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        None
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        None
    }
    fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
        None
    }
    fn swap(&mut self, _: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
        Err(SwapError::Off)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
        Ok(())
    }
    fn sleep(&mut self) {}
    fn wake_up(&mut self) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(22))
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

/// The default wait is 10 s, counted on the seat's clock.
#[test]
fn the_default_wait_is_ten_seconds() {
    let mut h = Harness::bare(&["--login", "--", "/bin/sh", "-c", "read x"]);
    // 8.5 s ahead, and what the real clock adds stays under 1.5 s.
    h.skip(Duration::from_millis(8_500));
    h.until(Duration::from_millis(200), |_| false);
    assert_eq!(h.app.finished(), None, "not yet");
    h.skip(Duration::from_millis(1_500));
    assert!(h.until(SECOND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
}

/// `display-wait=forever` waits for the first display however long it
/// takes, as for a USB display adapter plugged in long after boot.
#[test]
fn forever_waits_for_the_first_display() {
    let mut h = Harness::bare(&[
        "--display-wait=forever",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read x",
    ]);
    h.until(Duration::from_millis(100), |_| false);
    h.skip(Duration::from_secs(3_600));
    h.until(Duration::from_millis(300), |_| false);
    assert_eq!(h.app.finished(), None, "an hour on, still waiting");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until_memory(SECOND, |m| m.frames > 0), "drawn");
    assert_eq!(h.app.grid(), (40, 10));
    assert_eq!(h.app.finished(), None);
}

/// The session's last line that starts with `got:`, split into its
/// fields: the line typed, the SIGWINCHes so far, and the pty's rows and
/// columns.
fn got(app: &App) -> Option<(String, u32, String)> {
    let line = app
        .screen_text()
        .into_iter()
        .rev()
        .find(|l| l.starts_with("got:"))?;
    let mut f = line.split_whitespace();
    let typed = f.next()?.strip_prefix("got:")?.to_owned();
    let winch = f.next()?.strip_prefix("winch:")?.parse().ok()?;
    let size = f.collect::<Vec<_>>().join(" ");
    Some((typed, winch, size))
}

/// With the last display gone the session runs on: its grid keeps the
/// last display's size, so it sees no SIGWINCH; keys still reach it; the
/// blank does not run. The next display, smaller, resizes the grid once
/// and is drawn whole.
#[test]
fn with_the_last_display_gone_the_session_runs_on_and_the_next_is_drawn_whole() {
    // dash's `read` fails at a trapped SIGWINCH as it does at the end of
    // input, so a `read` that fails with the count moved is read again.
    // The next display's resize lands in a `read`.
    let script = "n=0; trap 'n=$((n+1))' WINCH; \
        while :; do m=$n; \
        if read x; then echo \"got:$x winch:$n $(stty size)\"; \
        elif [ $n = $m ]; then exit; fi; done";
    let mut h = Harness::new(&[
        "--dpms-timeout=1",
        "--no-blink",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        script,
    ]);
    assert!(h.until_memory(SECOND, |m| m.frames > 0));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    let (_, winch, size) = got(&h.app).expect("a line");
    assert_eq!(size, "10 40");

    h.app.remove_video(Path::new("/dev/mem0"));
    // Past dpms-timeout with no input and no display.
    h.until(Duration::from_millis(1_500), |_| false);
    assert!(
        !h.app.blank.blanked(),
        "the blank does not run with no display"
    );
    assert_eq!(h.app.grid(), (40, 10), "the last display's grid");
    h.type_str("b\r");
    assert!(
        h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "b")),
        "keys reach the session with no display"
    );
    assert_eq!(
        got(&h.app),
        Some(("b".into(), winch, "10 40".into())),
        "no SIGWINCH"
    );
    assert_eq!(h.app.finished(), None);

    // 240 x 128 in 8 x 16 cells.
    h.app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Memory::sized(240, 128)),
    );
    assert!(h.until_memory(SECOND, |m| m.frames > 0), "drawn");
    assert_eq!(h.app.grid(), (30, 8));
    // Drawn whole: each row of text is drawn, though none changed since.
    let m = h.memory();
    assert_eq!(m.ages.first(), Some(&0));
    let lines = h.app.screen_text();
    let m = h.memory();
    for (y, line) in lines.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        let rows = (y * 16 * 240 * 4)..((y + 1) * 16 * 240 * 4);
        assert!(
            m.pixels[rows].iter().any(|&b| b != 0),
            "row {y}, {line:?}, drawn"
        );
    }
    h.type_str("c\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "c")));
    assert_eq!(
        got(&h.app),
        Some(("c".into(), winch + 1, "8 30".into())),
        "one SIGWINCH"
    );
}

/// A blank in force ends with the last display: the next display comes
/// on and is drawn, with no key pressed.
#[test]
fn a_display_coming_after_the_blanked_last_one_went_is_drawn() {
    let mut h = Harness::new(&[
        "--dpms-timeout=1",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read x",
    ]);
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked"
    );
    h.app.remove_video(Path::new("/dev/mem0"));
    h.until(Duration::from_millis(100), |_| false);
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until_memory(SECOND, |m| m.frames > 0), "drawn");
}

/// The wait runs out on a display that is announced and never shows a
/// frame: the wait ends on the first frame on screen.
#[test]
fn a_display_whose_every_swap_fails_counts_as_none() {
    let mut h = Harness::bare(&WAITING);
    let refusing = Memory {
        refuses: true,
        ..Memory::default()
    };
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(refusing));
    assert!(h.until(SECOND, |a| a.cells().len() == 1), "announced");
    assert!(h.until(Duration::from_secs(3), |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    assert_eq!(h.memory().frames, 0);
}

/// The loop wakes for the wait's end with nothing else to wake it: no
/// blink, no blank, no output.
#[test]
fn the_loop_wakes_for_the_waits_end() {
    let mut h = Harness::bare(&[
        "--display-wait=1",
        "--no-blink",
        "--dpms-timeout=0",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read x",
    ]);
    let started = now();
    while h.app.finished().is_none() && now() - started < Duration::from_secs(5) {
        // As `main` runs it, but a missing wake-up costs 3 s, not ever.
        let wait = h.app.next_wake(h.at()).unwrap_or(Duration::from_secs(3));
        h.lp.dispatch(Some(wait), &mut h.app).expect("dispatch");
        h.app.turn(h.at());
    }
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    let took = now() - started;
    assert!(
        took < Duration::from_millis(2_500),
        "woken at the wait's end: {took:?}"
    );
}

/// A display coming after none, here a connector on a card awake since
/// the start, starts the blank's countdown afresh.
#[test]
fn a_display_coming_later_starts_the_blanks_countdown_afresh() {
    let late = Late {
        memory: Memory::default(),
        held: Some(Event::Added(ID)),
    };
    let mut h = Harness::build(
        &[
            "--display-wait=forever",
            "--dpms-timeout=3",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        FakeVt::new(),
        Box::new(late),
    );
    h.until(Duration::from_secs(2), |_| false);
    assert!(h.app.cells().is_empty(), "no display yet");
    h.late().held = None;
    assert!(h.until(SECOND, |a| a.cells().len() == 1));
    // 4 s from the start, 2 s from the display.
    h.until(Duration::from_secs(2), |_| false);
    assert_ne!(h.memory().dpms, Some(Dpms::Off), "not 3 s from the start");
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "3 s from the display"
    );
}

/// The last display going while the VT is not ours, blanked with a
/// re-take owed, wakes nothing: the acquire sets every device up.
#[test]
fn the_last_display_going_while_released_wakes_nothing() {
    let mut h = Harness::with_vt(
        &[
            "--dpms-timeout=1",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::Off)),
        "blanked"
    );
    h.scripted().events.push(VtEvent::Retaken);
    h.until(Duration::from_millis(50), |_| false);
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(SECOND, |a| !a.vt.is_active()), "released");
    let (wake_ups, unblanks) = (h.memory().wake_ups, h.memory().unblanks);
    h.memory().events.push(Event::Removed(ID));
    assert!(
        h.until(SECOND, |a| a.cells().is_empty()),
        "the display went"
    );
    let m = h.memory();
    assert_eq!(
        (m.wake_ups, m.unblanks, m.awake),
        (wake_ups, unblanks, false),
        "nothing woke"
    );
}

/// A line that wraps is one line to the selection: a double click takes a
/// word across the wrap, and a triple click every row of the line, copied
/// with no line end where it wrapped (vtium/vtium#97). The session
/// prints once the display has drawn, which resizes the grid from its
/// default, so the line wraps at the screen's width.
// covers: clipboard.selection
#[test]
fn a_selection_follows_a_wrapped_line() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "read go; w=$(stty size | cut -d' ' -f2); \
         printf '%*s' $((w - 3)) '' | tr ' ' x; printf ' abcdef end\\n'; read x",
    ]);
    assert!(h.until_memory(SECOND, |m| m.frames > 0));
    h.type_str("\r");
    assert!(h.until(SECOND, |a| shows(a, "cdef end")));
    let cols = h.app.term.terminal.screen().cols();
    let last = i32::try_from(cols).unwrap() - 1;
    let text = h.app.screen_text();
    let row = text.iter().position(|l| l == "cdef end").unwrap();
    let row = i32::try_from(row).unwrap();
    let none = Mods::empty();
    h.point(
        [
            over(last, row - 1, None),
            press(Button::Left, false),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"abcdef"[..]));
    h.point(
        [
            over(2, row, None),
            press(Button::Left, false),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    let line = format!("{} abcdef end\n", "x".repeat(cols - 3));
    assert_eq!(h.app.selection(), Some(line.as_bytes()));
}

/// The session's first look at its pty, as a child prints it: its size
/// then, `rows cols`, and its pid.
fn first(app: &App) -> Option<(String, String)> {
    let line = app
        .screen_text()
        .into_iter()
        .find(|l| l.starts_with("first:"))?;
    let (size, pid) = line.strip_prefix("first:")?.rsplit_once(' ')?;
    Some((size.to_owned(), pid.to_owned()))
}

/// A child that prints its first size, and on each line typed the line
/// and the SIGWINCHes so far. dash's `read` fails at a trapped SIGWINCH as
/// it does at the end of input, so a `read` that fails with the count
/// moved is read again.
const FIRST: &str = "n=0; trap 'n=$((n+1))' WINCH; echo \"first:$(stty size) $$\"; \
    while :; do m=$n; \
    if read x; then echo \"got:$x winch:$n $(stty size)\"; \
    elif [ $n = $m ]; then exit; fi; done";

/// The seat booted as [`Harness::boot`] boots it, with `videos`, but the
/// loop is served for up to `hold` between the start and the first turn,
/// so a child started then shows the size it started at before any turn
/// can resize it.
fn held(args: &[&str], videos: Vec<Box<dyn Video>>, hold: Duration) -> Harness {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
    assert_eq!(loaded.reports, [], "the test's command line");
    let mut lp: Loop<App> = Loop::new().expect("a loop");
    let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
    let ahead = Rc::new(Cell::new(Duration::ZERO));
    let clock = Rc::new(now);
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        clock,
        env,
    );
    for (n, video) in videos.into_iter().enumerate() {
        app.add_video(PathBuf::from(format!("/dev/mem{n}")), video);
    }
    app.insert_blank();
    app.vt.activate().expect("the fake VT");
    app.start();
    let end = now() + hold;
    while now() < end && first(&app).is_none() {
        lp.dispatch(Some(Duration::from_millis(20)), &mut app)
            .expect("dispatch");
    }
    app.turn(now());
    Harness { lp, app, ahead }
}

/// The child starts on the first display's grid: its first size is the
/// display's, and no SIGWINCH follows, however long the first turn takes
/// to come.
#[test]
fn the_child_starts_on_the_first_displays_grid() {
    let mut h = held(
        &["--login", "--", "/bin/sh", "-c", FIRST],
        vec![Box::new(Memory::default())],
        Duration::from_millis(500),
    );
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.grid(), (40, 10));
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    assert_eq!(
        got(&h.app),
        Some(("a".into(), 0, "10 40".into())),
        "no SIGWINCH"
    );
}

/// With several displays at the start the child starts on the layout's
/// grid for all of them.
#[test]
fn the_child_starts_on_the_grid_of_every_display_at_the_start() {
    let mut h = held(
        &["--login", "--", "/bin/sh", "-c", FIRST],
        vec![
            Box::new(Memory::default()),
            Box::new(Memory::sized(240, 128)),
        ],
        Duration::from_millis(500),
    );
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.cells().len(), 2, "both displays");
    let (cols, rows) = h.app.grid();
    assert_ne!((cols, rows), (80, 24));
    assert_eq!(first(&h.app).map(|f| f.0), Some(format!("{rows} {cols}")));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    assert_eq!(got(&h.app).map(|g| g.1), Some(0), "no SIGWINCH");
}

/// Under `display-wait`, the child waits for the first display and starts
/// on its grid.
#[test]
fn under_a_wait_the_child_starts_when_the_first_display_comes() {
    let mut h = Harness::bare(&["--display-wait=5", "--login", "--", "/bin/sh", "-c", FIRST]);
    h.until(Duration::from_millis(500), |_| false);
    assert!(h.app.pty.is_none(), "no child before a display");
    assert_eq!(first(&h.app), None);
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    assert_eq!(got(&h.app).map(|g| g.1), Some(0), "no SIGWINCH");
    assert_eq!(h.app.finished(), None);
}

/// Under `display-wait=forever` with no display, the child starts once
/// the VT is shown, on 80x24; the first display resizes it once.
#[test]
fn under_forever_with_no_display_the_child_starts_on_80x24() {
    let mut h = Harness::bare(&[
        "--display-wait=forever",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        FIRST,
    ]);
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    assert_eq!(first(&h.app).map(|f| f.0), Some("24 80".to_owned()));
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until(SECOND, |a| a.grid() == (40, 10)));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    assert_eq!(
        got(&h.app),
        Some(("a".into(), 1, "10 40".into())),
        "one SIGWINCH"
    );
}

/// A child started again starts on the grid the session runs on.
#[test]
fn a_child_started_again_starts_on_the_grid() {
    let script = "echo \"first:$(stty size) $$\"; read x";
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", script]);
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    let (size, pid) = first(&h.app).expect("a first line");
    assert_eq!(size, "10 40");
    h.type_str("\r");
    assert!(
        h.until(SECOND, |a| first(a).is_some_and(|f| f.1 != pid)),
        "started again"
    );
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
}

/// Under `display-wait=0` with no display, vtium exits at the first turn
/// the VT is shown, and the child never starts.
#[test]
fn under_no_wait_with_no_display_no_child_starts() {
    let mut h = Harness::bare(&["--display-wait=0", "--login", "--", "/bin/sh", "-c", FIRST]);
    assert!(h.until(Duration::from_millis(500), |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    assert!(!h.app.started(), "no child");
}

/// Under `display-wait=0` a display set up at the start is drawn, its
/// flip read, and vtium runs on.
#[test]
fn under_no_wait_a_display_at_the_start_runs() {
    let mut h = Harness::new(&["--display-wait=0", "--login", "--", "/bin/sh", "-c", FIRST]);
    h.until(Duration::from_millis(500), |a| a.finished().is_some());
    assert_eq!(h.app.finished(), None, "a display was there");
    assert!(h.memory().frames > 0, "drawn");
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
}

/// A device whose flips land [`Slow::FLIP`] after the swap, as a DRM
/// card's page flip lands at the next vblank: until then the frame is on
/// its way and `Flipped` is not yet raised.
struct Slow {
    memory: Memory,
    landed: Option<Instant>,
}

impl Slow {
    const FLIP: Duration = Duration::from_millis(50);
}

impl Video for Slow {
    fn backend(&self) -> &'static str {
        self.memory.backend()
    }
    fn events(&mut self) -> Vec<Event> {
        let mut events = self.memory.events();
        if self.landed.is_some_and(|t| now() >= t) {
            self.landed = None;
            events.push(Event::Flipped(ID));
        }
        events
    }
    fn displays(&self) -> Vec<DisplayId> {
        self.memory.displays()
    }
    fn name(&self, id: DisplayId) -> String {
        self.memory.name(id)
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        self.memory.size(id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        self.memory.refresh_mhz(id)
    }
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        self.memory.canvas(id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let swapped = self.memory.swap(id, damage)?;
        self.memory.events.retain(|e| *e != Event::Flipped(id));
        self.landed = Some(now() + Slow::FLIP);
        Ok(swapped)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.landed.is_some()
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.memory.set_dpms(id, dpms)
    }
    fn sleep(&mut self) {
        self.memory.sleep();
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.memory.wake_up()
    }
    fn unblank(&mut self) -> io::Result<()> {
        self.memory.unblank()
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

/// Under `display-wait=0` a display whose flip lands after the first turn
/// is waited for: vtium runs on once the flip is read.
#[test]
fn under_no_wait_a_frame_on_its_way_is_waited_for() {
    let slow = Slow {
        memory: Memory::default(),
        landed: None,
    };
    let mut h = Harness::build(
        &["--display-wait=0", "--login", "--", "/bin/sh", "-c", FIRST],
        FakeVt::new(),
        Box::new(slow),
    );
    h.until(Duration::from_millis(500), |a| a.finished().is_some());
    assert_eq!(h.app.finished(), None, "the flip was waited for");
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
}

// Mode 2033 (foot(7)): a program waiting in a read hears the VT switch
// away at once, with no key pressed. The child prints each report with
// its ESC as `E`.
// covers: DEC-2033
#[test]
fn a_release_reports_the_visibility_to_a_program_waiting_in_a_read() {
    let mut h = Harness::with_vt(
        &[
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "stty -icanon -echo min 1 time 0; printf '\\033[?2033h'; \
             head -c 9 | tr '\\033' E; printf '\\r\\n'; \
             head -c 9 | tr '\\033' E; printf '\\r\\n'; read x",
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "E[?999;1n")),
        "visible as the mode is set: {:?}",
        h.app.screen_text()
    );
    h.scripted().events.push(VtEvent::Release);
    assert!(
        h.until(SECOND, |a| a.screen_text().iter().any(|l| l == "E[?999;2n")),
        "not visible once released, with no key: {:?}",
        h.app.screen_text()
    );
}

// Mode 2048 (rockorager's in-band resize notifications): a program waiting
// in a read hears a zoom's new grid at once. The child prints each report,
// read up to its `t`, with its ESC as `E`.
// covers: DEC-2048
#[test]
fn a_zoom_reports_the_text_area_to_a_program_waiting_in_a_read() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty -icanon -echo min 1 time 0; \
         rd() { s=; while :; do c=$(head -c 1); s=\"$s$c\"; [ \"$c\" = t ] && break; done; \
         printf '%s\\r\\n' \"$s\" | tr '\\033' E; }; \
         printf '\\033[?2048h'; rd; rd; read x",
    ]);
    assert!(
        h.until(SECOND, |a| a
            .screen_text()
            .iter()
            .any(|l| l == "E[48;10;40;160;320t")),
        "the text area as the mode is set: {:?}",
        h.app.screen_text()
    );
    h.app.input_event(key(Mods::CTRL, 0x3d), now());
    assert_eq!(h.app.grid(), (20, 5), "zoomed in to 16x32 cells");
    assert!(
        h.until(SECOND, |a| a
            .screen_text()
            .iter()
            .any(|l| l == "E[48;5;20;160;320t")),
        "the zoomed text area, with no other key: {:?}",
        h.app.screen_text()
    );
}
