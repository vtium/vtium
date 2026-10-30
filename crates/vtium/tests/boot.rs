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
use vtium::font::Sources;
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
    sleeps: u32,
    /// Each DPMS state set, in order.
    dpms_sets: Vec<Dpms>,
    /// Each canvas lent's age, in order.
    ages: Vec<u32>,
    refresh_mhz: u32,
    /// Every swap fails, as on a display announced and never shown.
    refuses: bool,
    /// The density the display measures, across and down; `None` for one
    /// that reports no physical size.
    dpi: Option<(f64, f64)>,
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
            sleeps: 0,
            dpms_sets: Vec::new(),
            ages: Vec::new(),
            refresh_mhz: 60_000,
            refuses: false,
            // the C reference's fixed density, where a point is a pixel
            dpi: Some((72.0, 72.0)),
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
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        self.dpi
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
        self.dpms_sets.push(dpms);
        if dpms == Dpms::On {
            self.age = 0;
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.sleeps += 1;
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

/// No frits and no vconsole font, so the seat draws its built-in font
/// whatever the machine running the tests holds.
fn no_fonts() -> Sources {
    Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
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

    /// The seat booted with `args`. The harness draws 16-pixel cells, 16
    /// points on a display measuring 72 DPI, the size every test's grid is
    /// reckoned in, unless a test sets `font-size`: at 320x160 the automatic size is the built-in
    /// Unscii-8's 8 pixels (vtium/vtium#118), which its own tests check.
    fn boot<V: Vt + 'static>(args: &[&str], vt: V, video: Option<Box<dyn Video>>) -> Harness {
        Harness::boot_with(args, vt, video, no_fonts())
    }

    /// The seat on the fake VT with `video`, reading its font's sets from
    /// `fonts`.
    fn fonts(args: &[&str], video: Memory, fonts: Sources) -> Harness {
        Harness::boot_with(args, FakeVt::new(), Some(Box::new(video)), fonts)
    }

    fn boot_with<V: Vt + 'static>(
        args: &[&str],
        vt: V,
        video: Option<Box<dyn Video>>,
        fonts: Sources,
    ) -> Harness {
        Harness::boot_console(args, vt, video, fonts, None)
    }

    /// [`Harness::boot_with`], with the kernel's font `console` read at
    /// start.
    fn boot_console<V: Vt + 'static>(
        args: &[&str],
        vt: V,
        video: Option<Box<dyn Video>>,
        fonts: Sources,
        console: Option<vtium_platform::vt::font::ConsoleFont>,
    ) -> Harness {
        let sized = args.iter().any(|a| a.starts_with("--font-size"));
        let args: Vec<OsString> = (!sized)
            .then_some("--font-size=16")
            .into_iter()
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
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
        app.set_font_sources(fonts);
        if let Some(console) = console {
            app.set_console_font(console);
        }
        if let Some(video) = video {
            app.add_video(PathBuf::from("/dev/mem0"), video);
        }
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
    assert!(
        h.until_memory(SECOND, |m| m.frames > 0),
        "a frame was presented"
    );
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

// covers: color.palette.named
#[test]
fn a_named_palette_reaches_the_terminal() {
    let h = Harness::new(&["--palette=solarized-white"]);
    assert_eq!(
        Some(h.app.term().expect("a head").terminal.palette()),
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
    let p = h.app.term().expect("a head").terminal.palette();
    assert_eq!(p.color(1), Rgb::new(1, 2, 3));
    assert_eq!(p.bg(), Rgb::new(4, 5, 6));
    // Unset keys keep the C reference's custom defaults.
    assert_eq!(p.color(4), Rgb::new(0, 0, 238));
    let h = Harness::new(&[]);
    assert_eq!(
        h.app.term().expect("a head").terminal.palette(),
        &vtium_term::Palette::default()
    );
}

impl Memory {
    /// Whether any pixel is `rgb`; the buffer is XRGB8888, little-endian.
    fn has(&self, [r, g, b]: [u8; 3]) -> bool {
        self.pixels.chunks_exact(4).any(|p| p[..3] == [b, g, r])
    }
}

/// The seat with `options`, once the program's `text` is drawn.
fn decorated(options: &[&str], text: &str) -> Harness {
    let script = format!("printf '{text}\\n'; read x");
    let mut args = options.to_vec();
    args.extend(["--login", "--", "/bin/sh", "-c", &script]);
    let mut h = Harness::new(&args);
    assert!(h.until(SECOND, |a| shows(a, "U S")));
    h.until(Duration::from_millis(100), |_| false);
    h
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// `underline-color` draws every underline in its color, a program's SGR
/// 58 aside.
#[test]
fn underline_color_colors_the_underline() {
    let text = "\\033[4mU\\033[24m \\033[9mS\\033[29m";
    assert!(!decorated(&[], text).memory().has(RED));
    let mut h = decorated(&["--underline-color=#ff0000"], text);
    assert!(h.memory().has(RED));
    // A palette index follows the palette.
    let mut h = decorated(
        &[
            "--underline-color=4",
            "--palette=custom",
            "--palette-blue=0,0,255",
        ],
        text,
    );
    assert!(h.memory().has(BLUE));
}

/// `strike-color` draws every strike in its color.
#[test]
fn strike_color_colors_the_strike() {
    let text = "\\033[4mU\\033[24m \\033[9mS\\033[29m";
    assert!(!decorated(&[], text).memory().has(GREEN));
    let mut h = decorated(&["--strike-color=#00ff00"], text);
    assert!(h.memory().has(GREEN));
}

/// `force-decoration-colors` draws the configured underline color over
/// the one a program sets with SGR 58; off, the program's wins.
#[test]
fn force_decoration_colors_wins_over_sgr_58() {
    let text = "\\033[4;58:2::0:0:255mU\\033[24;59m \\033[9mS\\033[29m";
    let mut h = decorated(&["--underline-color=#ff0000"], text);
    assert!(h.memory().has(BLUE) && !h.memory().has(RED));
    let mut h = decorated(
        &["--underline-color=#ff0000", "--force-decoration-colors"],
        text,
    );
    assert!(h.memory().has(RED) && !h.memory().has(BLUE));
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

/// A device added while the VT is not shown, as a card plugged in then,
/// is put to sleep, which lets a card's DRM master go, and the acquire
/// wakes it; one added while the VT is shown is woken and never slept
/// (vtium/vtium#139).
#[test]
fn a_device_added_while_the_vt_is_not_shown_sleeps_until_the_acquire() {
    let mut h = Harness::with_vt(
        &["--login", "--", "/bin/sh", "-c", "read x"],
        ScriptedVt::default(),
    );
    assert!(h.until(SECOND, |a| a.vt.is_active()), "acquired");
    let add = |h: &mut Harness, node: &str| {
        h.app
            .add_video(PathBuf::from(node), Box::new(Memory::default()));
    };
    let counts = |h: &mut Harness, node: &str| {
        let m = h
            .app
            .videos
            .get_mut(Path::new(node))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("the device added");
        (m.sleeps, m.wake_ups)
    };
    add(&mut h, "/dev/mem1");
    assert_eq!(
        counts(&mut h, "/dev/mem1"),
        (0, 1),
        "shown: woken, not slept"
    );
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(SECOND, |a| !a.vt.is_active()), "released");
    add(&mut h, "/dev/mem2");
    assert_eq!(counts(&mut h, "/dev/mem2"), (1, 0), "not shown: slept");
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    h.until(Duration::from_millis(200), |_| false);
    assert_eq!(counts(&mut h, "/dev/mem2"), (1, 1), "woken by the acquire");
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

/// A seat that blanks after 1 s with no input, on the scripted VT, whose
/// session runs `script` with `options` before the command.
fn program_blank(options: &[&str], script: &str) -> Harness {
    let mut args = vec!["--dpms-timeout=1"];
    args.extend(options);
    args.extend(["--login", "--", "/bin/sh", "-c", script]);
    Harness::with_vt(&args, ScriptedVt::default())
}

const OFF: fn(&Memory) -> bool = |m| m.dpms == Some(Dpms::Off);

// The Linux console's `CSI 13 ]` wakes the displays as a key does.
// covers: ctl.linux-unblank
#[test]
fn a_programs_wake_ends_the_blank() {
    let mut h = program_blank(&[], r"sleep 2; printf '\033[13]'; read x");
    assert!(
        h.until_memory(Duration::from_secs(2), OFF),
        "blanked after dpms-timeout"
    );
    let unblanks = h.memory().unblanks;
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::On)),
        "woken by the program"
    );
    assert_eq!(h.memory().unblanks, unblanks + 1);
}

// `CSI 9 ; 0 ]` turns the blank off until the program changes it or RIS
// returns it to `dpms-timeout` (the operator's decisions on
// vtium/vtium#79).
// covers: ctl.linux-blank
#[test]
fn a_programs_timeout_holds_until_ris() {
    let mut h = program_blank(&[], r"printf '\033[9;0]'; sleep 3; printf '\033c'; read x");
    assert!(
        !h.until_memory(Duration::from_millis(2500), OFF),
        "lit under the program's 0"
    );
    assert!(
        h.until_memory(Duration::from_secs(3), OFF),
        "blanked by dpms-timeout after RIS"
    );
}

// A session that turns the blank off and exits leaves the next session
// `dpms-timeout`: the turn that starts it takes the RIS's request, so no
// request waits for a later turn.
#[test]
fn a_respawned_session_blanks_after_dpms_timeout() {
    let marker = std::env::temp_dir().join(format!("vtium-blank-respawn-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        "if [ -e {m} ]; then read x; else : > {m}; printf '\\033[9;0]'; sleep 2.5; fi",
        m = marker.display()
    );
    let mut h = program_blank(&[], &script);
    let end = now() + Duration::from_secs(6);
    let mut blanked = None;
    while now() < end && blanked.is_none() {
        let wait = h
            .app
            .next_wake(h.at())
            .unwrap_or(Duration::from_millis(20))
            .min(Duration::from_millis(20));
        h.lp.dispatch(Some(wait), &mut h.app).expect("dispatch");
        h.app.turn(h.at());
        let left = h
            .app
            .term_mut()
            .expect("a head")
            .terminal
            .take_blank_request();
        assert_eq!(left, None, "a request left for a later turn");
        if h.memory().dpms == Some(Dpms::Off) {
            blanked = Some(now());
        }
    }
    let _ = std::fs::remove_file(&marker);
    assert!(
        blanked.is_some(),
        "the next session blanks after dpms-timeout"
    );
}

// While the VT is another's, a program's wake touches no device and its
// timeout is still taken: the VT released while blanked, the program sends
// `CSI 9 ; 1 ]` and `CSI 13 ]` 2 s after its start.
#[test]
fn a_program_wakes_nothing_while_the_vt_is_anothers() {
    let mut h = program_blank(&[], r"sleep 2; printf '\033[9;1]\033[13]'; read x");
    assert!(
        h.until_memory(Duration::from_millis(1800), OFF),
        "blanked after dpms-timeout"
    );
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(SECOND, |a| !a.vt.is_active()), "released");
    let (unblanks, sets) = (h.memory().unblanks, h.memory().dpms_sets.len());
    // Past the program's sequences at 2 s.
    h.until(Duration::from_millis(1500), |_| false);
    assert_eq!(h.memory().unblanks, unblanks, "no unblank");
    assert_eq!(h.memory().dpms_sets[sets..], [], "no DPMS set");
    let wake_ups = h.memory().wake_ups;
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.until_memory(SECOND, |m| m.wake_ups == wake_ups + 1
            && m.dpms == Some(Dpms::On)),
        "lit by the acquire"
    );
    // The program's minute was taken while the VT was another's.
    assert!(
        !h.until_memory(Duration::from_millis(1500), OFF),
        "lit past dpms-timeout's second"
    );
}

#[test]
fn a_programs_timeout_is_in_minutes() {
    let mut h = program_blank(&[], r"printf '\033[9;1]'; read x");
    assert!(
        !h.until_memory(Duration::from_millis(2500), OFF),
        "a minute, not dpms-timeout's second"
    );
}

#[test]
fn under_no_program_blank_a_program_neither_times_nor_wakes_the_blank() {
    let mut h = program_blank(
        &["--no-program-blank"],
        r"printf '\033[9;0]'; sleep 2; printf '\033[13]'; read x",
    );
    assert!(
        h.until_memory(Duration::from_secs(2), OFF),
        "blanked after dpms-timeout"
    );
    // Past the program's wake at 2 s.
    h.until(Duration::from_secs(2), |_| false);
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "still dark");
    assert_eq!(h.memory().unblanks, 0);
    h.type_str("a");
    assert_eq!(h.memory().dpms, Some(Dpms::On), "a key still wakes");
}

/// A seat that blanks after 2 s with no input, on the scripted VT, whose
/// session runs `script`.
fn blank_in_two(script: &str) -> Harness {
    Harness::with_vt(
        &["--dpms-timeout=2", "--login", "--", "/bin/sh", "-c", script],
        ScriptedVt::default(),
    )
}

// The end of a pointer frame and the pointer's hide are vtium's own, not
// the user's: neither restarts the blank's countdown, as the C reference's
// did not (`src/seat.c:578-590` at kmscon b1f7a7c5).
#[test]
fn the_pointers_frame_end_and_hide_are_not_activity() {
    let mut h = blank_in_two("read x");
    let start = now();
    h.until(Duration::from_millis(1200), |_| false);
    for event in [PointerEvent::Sync, PointerEvent::Hide] {
        h.app.input_event(pointer(event), h.at());
    }
    // Due 2 s after the start; counted from the hide, 3.2 s.
    let left = (start + Duration::from_millis(2700)).saturating_duration_since(now());
    assert!(
        h.until_memory(left, OFF),
        "blanked 2 s after the start, not 2 s after the hide"
    );
}

#[test]
fn a_pointer_move_restarts_the_blanks_countdown() {
    let mut h = blank_in_two("read x");
    let start = now();
    h.until(Duration::from_millis(1200), |_| false);
    h.app.input_event(pointer(over(1, 1, None)), h.at());
    let left = (start + Duration::from_millis(2700)).saturating_duration_since(now());
    assert!(!h.until_memory(left, OFF), "lit 2 s after the start");
    assert!(
        h.until_memory(Duration::from_secs(2), OFF),
        "blanked 2 s after the move"
    );
}

// The VT review's step on vtium/vtium#79, in seconds: a program's timeout,
// the pointer's hide on the way, and the program's wake. `CSI 9 ; n ]`
// counts in minutes, so RIS returns the timeout instead, a request on the
// same path.
// covers: ctl.linux-unblank
#[test]
fn a_programs_timeout_blanks_past_the_pointers_hide_and_its_wake_ends_it() {
    let mut h = blank_in_two(
        r"printf '\033[9;0]'; sleep 0.3; printf '\033c'; sleep 4; printf '\033[13]'; read x",
    );
    let start = now();
    h.until(Duration::from_millis(1800), |_| false);
    h.app.input_event(pointer(PointerEvent::Hide), h.at());
    // The RIS at 0.3 s restarts the countdown: due at 2.3 s; counted from
    // the hide, 3.8 s.
    let left = (start + Duration::from_millis(3000)).saturating_duration_since(now());
    assert!(
        h.until_memory(left, OFF),
        "blanked 2 s after the RIS, not 2 s after the hide"
    );
    let unblanks = h.memory().unblanks;
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::On)),
        "woken by the program at 4.3 s"
    );
    assert_eq!(h.memory().unblanks, unblanks + 1);
}

// The VT review's step on vtium/vtium#79 as it ran: `CSI 9 ; 1 ]`, the
// pointer's hide 20 s later, the blank at the minute, and the program's
// wake. The seat's clock skips the minute.
// covers: ctl.linux-blank
#[test]
fn a_programs_minute_blanks_past_the_pointers_hide() {
    let mut h = blank_in_two(r"printf '\033[9;1]ready'; sleep 2; printf '\033[13]'; read x");
    assert!(
        h.until(SECOND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.starts_with("ready"))),
        "the program's minute taken"
    );
    h.skip(Duration::from_secs(20));
    h.app.input_event(pointer(PointerEvent::Hide), h.at());
    h.skip(Duration::from_secs(39));
    h.until(Duration::from_millis(300), |_| false);
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit at 59 s");
    h.skip(Duration::from_secs(2));
    assert!(h.until_memory(SECOND, OFF), "blanked at the minute");
    let unblanks = h.memory().unblanks;
    assert!(
        h.until_memory(Duration::from_secs(3), |m| m.dpms == Some(Dpms::On)),
        "woken by the program"
    );
    assert_eq!(h.memory().unblanks, unblanks + 1);
}

// `main`'s loop waits `next_wake` on the seat's clock: with nothing else
// to time, that is the time left to the blank.
#[test]
fn the_loop_wakes_for_the_blank() {
    let mut h = Harness::with_vt(
        &[
            "--no-blink",
            "--dpms-timeout=60",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until_memory(SECOND, |m| m.frames > 0 && m.events.is_empty()),
        "drawn"
    );
    let wake = h.app.next_wake(h.at());
    assert!(
        wake.is_some_and(|w| w > Duration::from_secs(55) && w <= Duration::from_secs(60)),
        "the blank's minute ahead: {wake:?}"
    );
    h.skip(Duration::from_secs(45));
    let wake = h.app.next_wake(h.at());
    assert!(
        wake.is_some_and(|w| w > Duration::from_secs(10) && w <= Duration::from_secs(15)),
        "15 s left: {wake:?}"
    );
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
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        self.memory.dpi(id)
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
    let queued = |h: &Harness| h.app.pty().map_or(0, |p| p.queued());
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
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back(),
        3
    );
    h.app.input_event(pointer(PointerEvent::Wheel(-1)), now());
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back(),
        0
    );
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
    assert!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .has_selection()
    );
    h.app.input_event(key(Mods::SHIFT, 0xff63), now());
    assert!(
        !h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .has_selection()
    );
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

/// With Shift held the wheel scrolls vtium's scrollback while the program
/// tracks the pointer, as xterm's Shift override covers the wheel too;
/// without it the wheel is reported.
// covers: input.shift-override
#[test]
fn shift_and_the_wheel_scroll_back_while_the_program_tracks_the_pointer() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "i=0; while [ $i -lt 40 ]; do echo line$i; i=$((i+1)); done; \
         stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{n=gsub(/</,\"\");printf \"reports=%d\\r\\n\",n;exit}'; read x",
    ]);
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.ends_with("ready"))
    }));
    let back = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back()
    };
    h.point([PointerEvent::Wheel(1)], Mods::SHIFT);
    assert_eq!(back(&h), 3);
    h.point([PointerEvent::Wheel(-1)], Mods::SHIFT);
    assert_eq!(back(&h), 0);
    h.point([PointerEvent::Wheel(1)], Mods::empty());
    assert_eq!(back(&h), 0);
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.ends_with("reports=1"))),
        "{:?}",
        h.app.screen_text()
    );
}

/// A drag whose press was reported stays reported when Shift is pressed
/// during it: the button keeps the side its press chose.
// covers: input.shift-override
#[test]
fn shift_pressed_during_a_reported_drag_changes_nothing() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf '\\033[?1002h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read x",
    ]);
    assert!(h.until(SECOND, |a| shows(a, "ready")));
    h.point(
        [over(0, 0, None), press(Button::Left, false)],
        Mods::empty(),
    );
    h.point([over(4, 0, Some(Button::Left))], Mods::SHIFT);
    h.point([release(Button::Left)], Mods::empty());
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .selection_text(),
        None
    );
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| shows(a, "E[<0;1;1ME[<36;5;1ME[<0;5;1m")),
        "{:?}",
        h.app.screen_text()
    );
}

/// `grab-paste` binds the paste to the chords it names, in place of
/// Shift+Insert, and empty to none, so Shift+Insert reaches the program.
// covers: clipboard.paste
#[test]
fn grab_paste_binds_the_paste() {
    let script = "stty raw -echo; printf '\\033]52;c;cGFzdGVk\\a\\033[?2004hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read x";
    let shift_insert = key(Mods::SHIFT, 0xff63);
    let alt_p = InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: u32::from('p'),
        mods: Mods::ALT,
        keysyms: vec![u32::from('p')],
        codepoints: vec![Some('p')],
        repeat: false,
    });
    let mut h = Harness::new(&[
        "--grab-paste=<Alt>p",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        script,
    ]);
    assert!(h.until(SECOND, |a| shows(a, "ready")));
    assert_eq!(h.app.selection(), Some(&b"pasted"[..]));
    h.app.input_event(alt_p, now());
    h.app.input_event(shift_insert.clone(), now());
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| shows(a, "E[200~pastedE[201~E[2~")),
        "{:?}",
        h.app.screen_text()
    );
    let mut h = Harness::new(&["--grab-paste=", "--login", "--", "/bin/sh", "-c", script]);
    assert!(h.until(SECOND, |a| shows(a, "ready")));
    h.app.input_event(shift_insert, now());
    h.type_str("q");
    assert!(
        h.until(SECOND, |a| shows(a, "E[2~")),
        "{:?}",
        h.app.screen_text()
    );
}

impl Memory {
    /// Whether the pixel at `x`, `y` is white, the arrow's fill.
    fn white_at(&self, x: u32, y: u32) -> bool {
        let at = ((y * self.w + x) * 4) as usize;
        self.pixels.get(at..at + 3) == Some(&[255, 255, 255][..])
    }
}

/// Under `pointer-hide-while-typing` a key the program is sent hides the
/// pointer until it next moves; off, as by default, typing leaves it.
#[test]
fn typing_hides_the_pointer_under_pointer_hide_while_typing() {
    for hides in [true, false] {
        let mut args = vec!["--login", "--", "/bin/sh", "-c", "stty -echo; read x"];
        if hides {
            args.insert(0, "--pointer-hide-while-typing");
        }
        let mut h = Harness::new(&args);
        h.until(Duration::from_millis(200), |_| false);
        // The arrow's tip is at the pointer, its fill a pixel below it.
        h.point([over(10, 5, None)], Mods::empty());
        let (x, y) = (10 * 8 + 4, 5 * 16 + 8 + 1);
        assert!(h.until_memory(SECOND, |m| m.white_at(x, y)), "shown");
        h.type_str("a");
        let hidden = h.until_memory(Duration::from_millis(500), |m| !m.white_at(x, y));
        assert_eq!(hidden, hides, "typing hides it: {hides}");
        // A key the program is not sent, a grab's or a modifier's alone,
        // leaves it.
        h.point([over(10, 5, None)], Mods::empty());
        assert!(h.until_memory(SECOND, |m| m.white_at(x, y)), "shown again");
        h.app.input_event(key(Mods::SHIFT, 0xff52), now());
        h.app.input_event(key(Mods::empty(), 0xffe1), now());
        assert!(
            !h.until_memory(Duration::from_millis(300), |m| !m.white_at(x, y)),
            "a grab and a modifier leave it"
        );
    }
}

/// A chord with Shift names the keysym Shift makes, as the C reference
/// matched grabs: Ctrl+Shift+v makes `V`, so `<Ctrl><Shift>V` binds it and
/// `<Ctrl><Shift>v` never fires.
#[test]
fn a_shift_chord_names_the_shifted_keysym() {
    let script = "stty raw -echo; printf '\\033]52;c;cGFzdGVk\\a\\033[?2004hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n[%s]\\r\\n\",$0;exit}'; read x";
    let ctrl_shift_v = InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: u32::from('v'),
        mods: Mods::CTRL.union(Mods::SHIFT),
        keysyms: vec![u32::from('V')],
        codepoints: vec![Some('V')],
        repeat: false,
    });
    for (chord, pasted) in [("<Ctrl><Shift>V", true), ("<Ctrl><Shift>v", false)] {
        let arg = format!("--grab-paste={chord}");
        let mut h = Harness::new(&[&arg, "--login", "--", "/bin/sh", "-c", script]);
        assert!(h.until(SECOND, |a| shows(a, "ready")));
        h.app.input_event(ctrl_shift_v.clone(), now());
        h.type_str("q");
        let want = if pasted { "[E[200~pastedE[201~]" } else { "[]" };
        assert!(
            h.until(SECOND, |a| shows(a, want)),
            "{chord}: {:?}",
            h.app.screen_text()
        );
    }
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
    let selected = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .selection_text()
    };
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
    assert!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .has_selection()
    );
    std::fs::write(&go, b"").expect("the go file");
    let cleared = h.until(SECOND, |a| {
        shows(a, "on") && !a.term().expect("a head").terminal.screen().has_selection()
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
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        self.memory.dpi(id)
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

/// Under `pointer-char` the pointer is the glyph, on the cell under it:
/// anywhere in a cell draws the same frame, and another cell another.
#[test]
fn pointer_char_draws_the_glyph_on_the_cell_under_the_pointer() {
    let mut h = Harness::new(&[
        "--pointer-char=+",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty -echo; read x",
    ]);
    h.until(Duration::from_millis(200), |_| false);
    let frame = |h: &mut Harness, x: i32, y: i32| {
        h.point([PointerEvent::Moved { x, y, held: None }], Mods::empty());
        h.until(Duration::from_millis(100), |_| false);
        h.memory().pixels.clone()
    };
    let (a, b) = (frame(&mut h, 81, 81), frame(&mut h, 87, 95));
    let other = frame(&mut h, 89, 81);
    assert!(a == b, "the same cell, the same frame");
    assert!(a != other, "another cell, another frame");
    // the glyph's middle is white: unifont's '+' crosses at 4,9 of its cell
    assert!(h.memory().white_at(88 + 4, 80 + 9));
}

/// A device with a cursor plane: what was set up on it and where it shows.
struct Planed {
    memory: Memory,
    set_ups: u32,
    shown: Option<(i32, i32)>,
}

impl Video for Planed {
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
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        self.memory.dpi(id)
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
    fn setup_cursor(&mut self, _: DisplayId, _: &vtium_render::Arrow) -> io::Result<()> {
        self.set_ups += 1;
        Ok(())
    }
    fn show_cursor(&mut self, _: DisplayId, at: (i32, i32)) -> io::Result<()> {
        self.shown = Some(at);
        Ok(())
    }
    fn hide_cursor(&mut self, _: DisplayId) {
        self.shown = None;
    }
    fn hardware_cursor(&self, _: DisplayId) -> bool {
        self.set_ups > 0
    }
}

/// A display with a cursor plane shows the pointer there; under
/// `soft-cursor` the renderer draws it and the plane is never set up.
#[test]
fn soft_cursor_draws_the_pointer_in_software() {
    for soft in [false, true] {
        let mut args = vec!["--login", "--", "/bin/sh", "-c", "read x"];
        if soft {
            args.insert(0, "--soft-cursor");
        }
        let planed = Planed {
            memory: Memory::default(),
            set_ups: 0,
            shown: None,
        };
        let mut h = Harness::build(&args, FakeVt::new(), Box::new(planed));
        h.until(Duration::from_millis(200), |_| false);
        h.point([over(10, 5, None)], Mods::empty());
        let (x, y) = (10 * 8 + 4, 5 * 16 + 8);
        h.until(Duration::from_millis(300), |_| false);
        let p = h
            .app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .and_then(|v| v.as_any().downcast_mut::<Planed>())
            .expect("the planed device");
        if soft {
            assert_eq!((p.set_ups, p.shown), (0, None));
            assert!(p.memory.white_at(x, y + 1), "drawn in software");
        } else {
            assert_eq!((p.set_ups, p.shown), (1, Some((84, 88))));
            assert!(!p.memory.white_at(x, y + 1), "not drawn in software");
        }
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
    assert!(!h.app.term().expect("a head").unparsed);
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
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        self.memory.dpi(id)
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
        h.app
            .term_mut()
            .expect("a head")
            .terminal
            .take_clipboard_write(),
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
    let cols = h.app.term().expect("a head").terminal.screen().cols();
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
    app.set_font_sources(no_fonts());
    for (n, video) in videos.into_iter().enumerate() {
        app.add_video(PathBuf::from(format!("/dev/mem{n}")), video);
    }
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
        &["--font-size=16", "--login", "--", "/bin/sh", "-c", FIRST],
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
    assert!(h.app.pty().is_none(), "no child before a display");
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

/// Under `display-wait=forever` with no display, the head has no display,
/// so its child does not start (ADR-1401 D1); the first display starts it
/// on its grid, with no SIGWINCH.
#[test]
fn under_forever_with_no_display_the_child_starts_at_the_first_display() {
    let mut h = Harness::bare(&[
        "--display-wait=forever",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        FIRST,
    ]);
    h.until(Duration::from_millis(300), |_| false);
    assert!(!h.app.started(), "no child before a display");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until(SECOND, |a| first(a).is_some()), "the child started");
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
    assert_eq!(
        got(&h.app),
        Some(("a".into(), 0, "10 40".into())),
        "no SIGWINCH"
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
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        self.memory.dpi(id)
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

/// The seat with `args` and a session that waits, its frits from `fonts`.
fn with_fonts(args: &[&str], fonts: Sources) -> Harness {
    let args: Vec<&str> = args
        .iter()
        .copied()
        .chain(["--login", "--", "/bin/sh", "-c", "read x"])
        .collect();
    Harness::fonts(&args, Memory::default(), fonts)
}

// vtium/vtium#118: at 320x240 with no font size, the built-in Unscii-8
// draws 40 by 30 cells of 8x8; at 640x480 unifont draws 80 by 30 of 8x16.
#[test]
fn a_320x240_display_draws_40_by_30_cells_of_8x8() {
    for (size, grid, cell, name) in [
        ((320, 240), (40, 30), (8, 8), "unscii-8"),
        ((640, 480), (80, 30), (8, 16), "unifont"),
    ] {
        let h = Harness::fonts(
            &["--font-size=0", "--login", "--", "/bin/sh", "-c", "read x"],
            Memory::sized(size.0, size.1),
            no_fonts(),
        );
        assert_eq!(h.app.grid(), grid, "{size:?}");
        assert_eq!(h.app.cells(), [cell], "{size:?}");
        assert_eq!(h.app.font_name(), name, "{size:?}");
    }
}

// ADR-1200 D7: zoom steps through the built-ins, Unscii-8's 8, then
// unifont's 16 and 32, and stops at the smallest
#[test]
fn zoom_steps_through_the_built_ins_and_stops_at_the_smallest() {
    let (equal, minus) = (0x3d, 0x2d);
    let mut h = with_fonts(&["--font-size=8"], no_fonts());
    let mut cells = vec![h.app.cells()[0]];
    for k in [equal, equal, minus, minus, minus] {
        h.app.input_event(key(Mods::CTRL, k), now());
        cells.push(h.app.cells()[0]);
    }
    assert_eq!(cells, [(8, 8), (8, 16), (16, 32), (8, 16), (8, 8), (8, 8)]);
}

/// The seat with `args` whose session writes `script` with printf, then
/// `done`, and waits.
fn osc50(args: &[&str], script: &str, fonts: Sources) -> Harness {
    osc50_after(args, "", script, fonts)
}

/// [`osc50`], the session running the shell command `before` first.
fn osc50_after(args: &[&str], before: &str, script: &str, fonts: Sources) -> Harness {
    let command = format!("{before}printf '{script}done'; read x");
    let args: Vec<&str> = args
        .iter()
        .copied()
        .chain(["--login", "--", "/bin/sh", "-c", &command])
        .collect();
    let mut h = Harness::fonts(&args, Memory::default(), fonts);
    // The tty echoes the reply to `read`, after `done` on its line.
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.contains("done"))
    }));
    h
}

/// The log lines this thread wrote since the last call.
fn logged() -> Vec<String> {
    use std::cell::RefCell;
    use std::sync::Once;
    thread_local! {
        static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    struct Capture;
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            LINES.with(|l| l.borrow_mut().push(record.args().to_string()));
        }
        fn flush(&self) {}
    }
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&Capture).expect("the tests' logger");
        log::set_max_level(log::LevelFilter::Info);
    });
    LINES.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// A directory of a test's own in the temporary directory, made empty and
/// removed when it is dropped, as its test ends or panics. A test holds it
/// past its harness, so no helper starts in it once its removal begins; a
/// recording stand-in's checker, which outlives its helper, may still move
/// a file within it meanwhile, and the removal is tried again a while.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's directory");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    #[expect(
        clippy::disallowed_methods,
        reason = "the removal waits in real time for a checker's move to end"
    )]
    fn drop(&mut self) {
        for _ in 0..100 {
            if std::fs::remove_dir_all(&self.0).is_ok() || !self.0.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        eprintln!(
            "{} is left behind: it could not be removed",
            self.0.display()
        );
    }
}

/// A frit `t` in a directory of its own, and a stand-in helper that
/// answers each request with `answer` once `go` exists, then exits as at
/// its idle interval; the directory, the sources and the go file.
fn frit_fonts(name: &str, answer: &[u8], script: &str) -> (Scratch, Sources, PathBuf) {
    frit_fonts_of(name, "source=builtin:unifont\n", answer, script)
}

/// [`frit_fonts`] with the frit `frit`.
fn frit_fonts_of(
    name: &str,
    frit: &str,
    answer: &[u8],
    script: &str,
) -> (Scratch, Sources, PathBuf) {
    let scratch = Scratch::new(&format!("vtium-boot-frit-{name}"));
    let dir = scratch.path().to_owned();
    std::fs::write(dir.join("t.vtfrit"), frit).expect("the frit");
    let fixture = dir.join("answer");
    std::fs::write(&fixture, answer).expect("the answer");
    let go = dir.join("go");
    let helper = vtium::helper::Command {
        program: PathBuf::from("/bin/sh"),
        args: ["-c", script, "helper"]
            .into_iter()
            .map(OsString::from)
            .chain([fixture.into_os_string(), go.clone().into_os_string()])
            .collect(),
        account: None,
    };
    let fonts = Sources {
        frits: dir,
        helper: Some(helper),
        ..no_fonts()
    };
    (scratch, fonts, go)
}

/// One cell's fill as the helper sends it: every printable ASCII character
/// a solid block, then the fill's end.
fn solid_ascii(cell: vtium_font::Cell) -> Vec<u8> {
    use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, vtium_font::Style::default());
    let g = s.glyph(
        vtium_font::Width::Narrow,
        vec![255; (cell.width * cell.height) as usize],
    );
    for c in (0x21u8..0x7f).map(char::from) {
        s.map(c, g);
    }
    let set = SetBuilder::new("t").strike(s).write().expect("a set");
    // a fill's frame, then the fill's end
    let mut out = 4u32.to_le_bytes().to_vec();
    out.extend((set.len() as u32).to_le_bytes());
    out.extend(set);
    out.extend(5u32.to_le_bytes());
    out.extend(8u32.to_le_bytes());
    out.extend(cell.width.to_le_bytes());
    out.extend(cell.height.to_le_bytes());
    out
}

// ADR-1200 D1, S5: a warning the frit's text gives as the helper reads it
// reaches the log, once however often the helper sends it
#[test]
fn a_frits_warning_from_the_helper_is_logged_once() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let why = b"t:2: compress= has nothing to act on, and is ignored";
    let mut warning = 9u32.to_le_bytes().to_vec();
    warning.extend((why.len() as u32).to_le_bytes());
    warning.extend(why);
    let mut answer = warning.clone();
    answer.extend(&warning);
    answer.extend(solid_ascii(cell));
    let (_dir, fonts, _go) = frit_fonts("warning", &answer, "cat >/dev/null & cat \"$1\"; wait");
    logged();
    let mut h = with_fonts(&["--font-name=t"], fonts.clone());
    let line = "the frit t:2: compress= has nothing to act on, and is ignored";
    let mut lines = Vec::new();
    h.until(Duration::from_secs(10), |_| {
        lines.extend(logged());
        lines.iter().any(|l| l == line)
    });
    h.until(Duration::from_millis(300), |_| false);
    lines.extend(logged());
    assert_eq!(lines.iter().filter(|l| *l == line).count(), 1, "{lines:?}");
    assert_eq!(h.app.font_name(), "t", "the helper is not given up");
}

/// The 8x16 cell at column `col` of the top row, as the memory display's
/// pixels: one value each.
fn top_cell(m: &Memory, col: u32) -> Vec<u32> {
    (0..16)
        .flat_map(|y| (0..8).map(move |x| (col * 8 + x, y)))
        .map(|(x, y)| {
            let at = ((y * m.w + x) * 4) as usize;
            m.pixels
                .get(at..at + 4)
                .map_or(0, |p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
        })
        .collect()
}

fn solid(cell: &[u32]) -> bool {
    cell.iter().all(|&p| p == cell[0])
}

// ADR-1301 D2: a configured frit draws in the built-in font from the first
// frame, with no frame waiting on the helper, and the helper's glyphs
// replace the built-in's in the cells that show them once they arrive.
#[test]
fn a_frit_draws_the_built_in_font_first_and_the_helpers_glyphs_after() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let (_dir, fonts, go) = frit_fonts(
        "swap",
        &solid_ascii(cell),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; exec 1>&-; wait",
    );
    let mut h = with_fonts(&["--font-name=t"], fonts.clone());
    assert_eq!(h.app.font_name(), "t");
    h.type_str("AB");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| {
            let a = top_cell(m, 0);
            a.iter().any(|&p| p != a[0]) && !solid(&top_cell(m, 1))
        }),
        "the built-in font's AB is drawn before the helper answers"
    );
    let frames = h.memory().frames;
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell(m, 0))
            && solid(&top_cell(m, 1))),
        "the helper's glyphs replace A and B"
    );
    assert!(h.memory().frames > frames);
    // a blank cell past the cursor stays the background
    let m = h.memory();
    let blank = top_cell(m, 3);
    assert!(solid(&blank) && blank[0] != top_cell(m, 0)[0]);
}

// ADR-1200 D7: the built-in font draws only what the frit has not yet
// sent. A character drawn in the built-in font and cleared before its
// glyph arrives draws the frit's glyph when the screen shows it again.
#[test]
fn a_glyph_that_arrives_off_screen_draws_when_shown_again() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let (_dir, fonts, go) = frit_fonts(
        "offscreen",
        &solid_ascii(cell),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; exec 1>&-; wait",
    );
    // A at the top left; then, at each Enter, the screen cleared with B
    // beside where A was, and A drawn there again
    let session = "stty -echo; printf A; read x; printf '\\033[H\\033[2J B'; read y; printf '\\033[HA'; read z";
    let mut h = Harness::fonts(
        &["--font-name=t", "--login", "--", "/bin/sh", "-c", session],
        Memory::default(),
        fonts.clone(),
    );
    assert!(
        h.until_memory(Duration::from_secs(10), |m| !solid(&top_cell(m, 0))),
        "the built-in font's A is drawn"
    );
    h.type_str("\r");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell(m, 0))
            && !solid(&top_cell(m, 1))),
        "A is cleared and the built-in font's B drawn"
    );
    let background = top_cell(h.memory(), 0)[0];
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell(m, 1))),
        "the helper's glyphs, A's among them, arrive and replace B"
    );
    h.type_str("\r");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| {
            let a = top_cell(m, 0);
            solid(&a) && a[0] != background
        }),
        "A, shown again, draws the helper's glyph"
    );
}

// ADR-1301 D2: a helper that dies leaves the built-in font drawing, with
// one log line however much the screen asks after.
#[test]
fn a_helper_that_dies_leaves_the_built_in_font_with_one_log_line() {
    let (_dir, fonts, _go) = frit_fonts("dies", b"", "kill -SEGV $$");
    logged();
    let mut h = with_fonts(&["--font-name=t"], fonts.clone());
    h.type_str("AB");
    assert!(h.until(Duration::from_secs(10), |app| {
        app.screen_text()
            .first()
            .is_some_and(|l| l.starts_with("AB"))
    }));
    // given up after its starts again, however long a loaded machine
    // takes to start a shell four times; then no second line
    let given = |l: &String| l.starts_with("the font's glyphs are not generated");
    let mut lines = Vec::new();
    h.until(Duration::from_secs(10), |_| {
        lines.extend(logged());
        lines.iter().any(given)
    });
    h.until(Duration::from_millis(300), |_| false);
    lines.extend(logged());
    assert!(
        h.until_memory(Duration::from_secs(10), |m| !solid(&top_cell(m, 0))),
        "A is the built-in font's"
    );
    let given_up: Vec<&String> = lines.iter().filter(|l| given(l)).collect();
    assert_eq!(given_up.len(), 1, "{lines:?}");
    assert!(given_up[0].contains("signal 11"), "{given_up:?}");
    // ADR-1301 D1: the same line says the clusters draw unshaped
    assert!(
        given_up[0].ends_with("and each cluster unshaped, a glyph a codepoint"),
        "{given_up:?}"
    );
}

/// The cell `cell` at column `col` of the top row, as the memory
/// display's pixels.
fn top_cell_of(m: &Memory, cell: vtium_font::Cell, col: u32) -> Vec<u32> {
    (0..cell.height)
        .flat_map(|y| (0..cell.width).map(move |x| (col * cell.width + x, y)))
        .map(|(x, y)| {
            let at = ((y * m.w + x) * 4) as usize;
            m.pixels
                .get(at..at + 4)
                .map_or(0, |p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
        })
        .collect()
}

// #145: the frit states the cell where no font-size is configured; the
// built-in draws in it from the first frame, the helper's glyphs arrive
// at it, and only a zoom resizes the grid
#[test]
fn a_frits_stated_cell_is_the_grids_and_the_glyphs_never_move_it() {
    let (equal, minus) = (0x3d, 0x2d);
    let cell = vtium_font::Cell {
        width: 10,
        height: 20,
    };
    let (_dir, fonts, go) = frit_fonts_of(
        "cell",
        "source=builtin:unifont\ncell=10x20\n",
        &solid_ascii(cell),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; exec 1>&-; wait",
    );
    let mut h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(10, 20)], (32, 8)));
    h.type_str("AB");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| {
            let a = top_cell_of(m, cell, 0);
            a.iter().any(|&p| p != a[0])
        }),
        "the built-in font's A is drawn in the 10x20 cell"
    );
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell_of(m, cell, 0))
            && solid(&top_cell_of(m, cell, 1))),
        "the helper's glyphs at 10x20 replace A and B"
    );
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(10, 20)], (32, 8)));
    h.app.input_event(key(Mods::CTRL, equal), now());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(20, 40)], (16, 4)));
    h.app.input_event(key(Mods::CTRL, minus), now());
    h.app.input_event(key(Mods::CTRL, minus), now());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(10, 20)], (32, 8)));
}

// #145, vtium/vtium#118, ADR-1200 D17: with no stated cell, the display's;
// the 320x160 memory display is under the small-display floor, so
// Unscii-8's 8x8; a configured size is as wide as the built-in font there
#[test]
fn a_frit_with_no_stated_cell_takes_the_displays() {
    let (_dir, fonts, _go) =
        frit_fonts_of("auto", "source=builtin:unifont\n", b"", "cat >/dev/null");
    let h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 8)], (40, 20)));
    let h = with_fonts(&["--font-name=t", "--font-size=19"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 19)], (40, 8)));
}

/// The helper's answer to a cell query: the frit's own cell.
fn own_cell(width: u32, height: u32) -> Vec<u8> {
    let mut out = 6u32.to_le_bytes().to_vec();
    out.extend(8u32.to_le_bytes());
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out
}

// the operator's decision on #145, 2026-10-28: a frit of bitmap sources
// with no cell= takes its fonts' own cell, asked of the helper before the
// first frame, where the display's would be 8x8 at 320x160; a configured
// font-size keeps winning
#[test]
fn a_bitmap_frit_with_no_cell_takes_its_fonts_own_cell() {
    let (_dir, fonts, _go) = frit_fonts_of(
        "own",
        "source=kbd:x\n",
        &own_cell(8, 16),
        "cat >/dev/null & cat \"$1\"; wait",
    );
    logged();
    let h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 16)], (40, 10)));
    let lines = logged();
    assert!(
        lines
            .iter()
            .any(|l| l == "the frit t draws in its fonts' own 8x16 cell"),
        "{lines:?}"
    );
    let h = with_fonts(&["--font-name=t", "--font-size=8"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 8)], (40, 20)));
}

// ADR-1200 D5 and the operator's decision on #145, 2026-10-28: OSC 50's
// switch to a frit of bitmap sources with no cell= takes its fonts' own
// cell too, on the switch's new grid
#[test]
fn osc_50_to_a_bitmap_frit_with_no_cell_takes_its_fonts_own_cell() {
    let (_dir, fonts, _go) = frit_fonts_of(
        "own-osc50",
        "source=kbd:x\n",
        &own_cell(8, 16),
        "cat >/dev/null & cat \"$1\"; wait",
    );
    let h = osc50(&["--font-size=0"], "\\033]50;t\\007", fonts.clone());
    assert_eq!(h.app.font_name(), "t");
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 16)], (40, 10)));
}

// the operator's decision on #145, 2026-10-28: a helper that does not
// answer the cell query in its bound leaves the display's cell, with one
// line, and the first frame waits no longer than the bound
#[test]
fn a_bitmap_frits_own_cell_unanswered_falls_back_within_its_bound() {
    let (_dir, fonts, _go) = frit_fonts_of("own-none", "source=kbd:x\n", b"", "cat >/dev/null");
    logged();
    let t = now();
    let h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    let took = now().saturating_duration_since(t);
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 8)], (40, 20)));
    assert!(took >= Duration::from_millis(100), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    let lines = logged();
    let fell: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("the frit t's own cell is not known"))
        .collect();
    assert_eq!(
        fell,
        [
            "the frit t's own cell is not known (the helper gave no answer in 100 ms): it draws in the display's cell"
        ],
        "{lines:?}"
    );
}

// #145, ADR-1200 D9: a frit whose cell= is unreadable is not drawn; the
// built-in font is, with one line naming the font and why, and no command
#[test]
fn a_frit_with_an_unreadable_cell_leaves_the_built_in_font() {
    let (_dir, fonts, _go) = frit_fonts_of(
        "bad",
        "source=builtin:unifont\ncell=10\n",
        b"",
        "cat >/dev/null",
    );
    logged();
    let h = with_fonts(&["--font-name=t"], fonts.clone());
    assert_eq!(h.app.font_name(), "unifont");
    let lines = logged();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("the font t is not drawn (line 2: cell=10 is not WIDTHxHEIGHT")),
        "{lines:?}"
    );
    assert!(lines.iter().all(|l| !l.contains("create")), "{lines:?}");
}

/// The seat with the frit `t`, whose helper answers every printable ASCII
/// character as a solid block once `go` exists, its session running
/// `script`; the directory, the harness and the go file.
fn frit_session(name: &str, script: &str) -> (Scratch, Harness, PathBuf) {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let (scratch, fonts, go) = frit_fonts(
        name,
        &solid_ascii(cell),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; exec 1>&-; wait",
    );
    let h = Harness::fonts(
        &[
            "--font-name=t",
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            script,
        ],
        Memory::default(),
        fonts,
    );
    (scratch, h, go)
}

// #149: the glyphs that land damage the cells each buffer shows, a
// cluster's included
#[test]
fn a_cluster_cell_draws_the_helpers_glyph_once_it_lands() {
    let (_dir, mut h, go) = frit_session("cluster", "printf 'e\\314\\201'; read x");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| !solid(&top_cell(m, 0))),
        "the built-in font's e with its accent"
    );
    h.until(Duration::from_millis(300), |_| false);
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell(m, 0))),
        "the helper's e draws in the cluster's cell"
    );
}

// #149: a scrolled-back view's cells take the glyphs too
#[test]
fn a_scrolled_back_cell_draws_the_helpers_glyph_once_it_lands() {
    let (_dir, mut h, go) = frit_session(
        "scrolled",
        "echo A; i=0; while [ $i -lt 12 ]; do printf '\\303\\251\\n'; i=$((i+1)); done; printf '\\303\\251\\303\\251'; read x",
    );
    // the live screen shows only é, which the helper does not send
    assert!(h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l == "\u{e9}\u{e9}")
    }));
    h.point(
        [PointerEvent::Wheel(1), PointerEvent::Wheel(1)],
        Mods::empty(),
    );
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back(),
        4
    );
    assert!(
        h.until_memory(Duration::from_secs(10), |m| !solid(&top_cell(m, 0))),
        "the built-in font's A, scrolled back"
    );
    // the scroll's own redraw is done before the glyphs land
    h.until(Duration::from_millis(300), |_| false);
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(Duration::from_secs(10), |m| solid(&top_cell(m, 0))),
        "the helper's A draws in the scrolled-back view"
    );
}

// The operator's damage decision (2026-10-28): a fill of characters no
// buffer shows starts no draw pass, and swaps no frame
#[test]
fn glyphs_no_buffer_shows_start_no_draw() {
    let (_dir, mut h, go) = frit_session("unseen", "read x");
    assert!(h.until_memory(Duration::from_secs(10), |m| m.frames > 0));
    h.until(Duration::from_millis(200), |_| false);
    let (frames, passes) = (h.memory().frames, h.memory().ages.len());
    std::fs::write(&go, b"").expect("the go file");
    // the helper answers and exits; the screen shows none of its glyphs
    h.until(Duration::from_millis(1000), |_| false);
    assert_eq!((h.memory().frames, h.memory().ages.len()), (frames, passes));
}

/// The recording stand-in: it becomes `cat`, writing what it is sent to
/// a file of its own as it arrives, so a helper the console stops at once
/// has still recorded its request. A checker beside it, in a session and
/// process group of its own (`setsid`) and holding none of the helper's
/// pipes, so it outlives the helper's end and keeps the console waiting on nothing, reads the first
/// request's frit name and shows the file as `request.*`
/// only for a name of ADR-1200 D1's characters and at most 64 bytes, which
/// the helper's request reader takes; any other it moves to `refused.*`.
const RECORDER: &str = r#"
part="$1/part.$$"
: >"$part"
check='
    part=$1 dir=$2 pid=$3
    has() {
        i=0
        while [ "$(wc -c <"$part")" -lt "$1" ]; do
            i=$((i + 1))
            [ "$i" -le 1000 ] || exit 1
            sleep 0.01
        done
    }
    has 16
    n=$(od -An -tu4 -j12 -N4 "$part" | tr -d " ")
    if [ "$n" -ge 1 ] && [ "$n" -le 64 ]; then
        has $((16 + n))
        name=$(dd if="$part" bs=1 skip=16 count="$n" status=none)
        case $name in
        .* | *[!A-Za-z0-9._+-]*) ;;
        *) [ ${#name} -eq "$n" ] && exec mv "$part" "$dir/request.$pid" ;;
        esac
    fi
    mv "$part" "$dir/refused.$pid"
'
setsid sh -c "$check" checker "$part" "$1" $$ </dev/null >/dev/null &
exec cat >>"$part"
"#;

/// An empty frits' directory for `name`, and a stand-in helper,
/// [`RECORDER`], that writes what it is sent to a file of its own there
/// and answers nothing; the directory and the sources.
fn recording(name: &str) -> (Scratch, Sources) {
    let scratch = Scratch::new(&format!("vtium-boot-rec-{name}"));
    let dir = scratch.path().to_owned();
    let helper = vtium::helper::Command {
        program: PathBuf::from("/bin/sh"),
        args: ["-c", RECORDER, "helper"]
            .into_iter()
            .map(OsString::from)
            .chain([dir.clone().into_os_string()])
            .collect(),
        account: None,
    };
    let fonts = Sources {
        frits: dir,
        helper: Some(helper),
        ..no_fonts()
    };
    (scratch, fonts)
}

/// The first request in `bytes`: the frit's name and text, and the
/// rest of the request.
fn first_request(bytes: &[u8]) -> Option<(String, String, &[u8])> {
    let u32_at = |b: &[u8]| Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?) as usize);
    let rest = bytes.strip_prefix(b"VTVSREQ3")?.get(4..)?;
    let n = u32_at(rest)?;
    let name = String::from_utf8(rest.get(4..4 + n)?.to_vec()).ok()?;
    let rest = rest.get(4 + n..)?;
    let n = u32_at(rest)?;
    let text = String::from_utf8(rest.get(4..4 + n)?.to_vec()).ok()?;
    Some((name, text, rest.get(4 + n..)?))
}

/// The first request of each helper the stand-in in `fonts` started,
/// sorted, each as its bytes; waited for until there are `n`.
fn requests(h: &mut Harness, fonts: &Sources, n: usize) -> Vec<Vec<u8>> {
    let read = || -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = std::fs::read_dir(&fonts.frits)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("request."))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter(|b| first_request(b).is_some())
            .collect();
        out.sort();
        out
    };
    h.until(Duration::from_secs(10), |_| read().len() >= n);
    read()
}

/// The frit's name and text in the first request of each helper the
/// stand-in in `fonts` started, sorted; waited for until there are `n`.
fn recorded(h: &mut Harness, fonts: &Sources, n: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = requests(h, fonts, n)
        .iter()
        .filter_map(|b| first_request(b).map(|(name, text, _)| (name, text)))
        .collect();
    out.sort();
    out
}

fn pair(name: &str, text: &str) -> (String, String) {
    (name.to_owned(), text.to_owned())
}

// ADR-1200 D9: a kbd name no frit bears is drawn from a one-source frit
// made on the fly, handed to the helper as text, with no file
#[test]
fn a_kbd_name_no_frit_bears_is_generated_as_a_one_source_frit() {
    let (_dir, fonts) = recording("kbd");
    let mut h = with_fonts(&["--font-name=ter-v16n"], fonts.clone());
    assert_eq!(h.app.font_name(), "ter-v16n");
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("ter-v16n", "source=kbd:ter-v16n\n")]
    );
}

// ADR-1200 D21: a cluster the screen shows is asked for whole, by its
// text, beside each of its characters
#[test]
fn a_cluster_the_screen_shows_is_asked_for_whole() {
    let (_dir, fonts) = recording("cluster-ask");
    let mut h = Harness::fonts(
        &[
            "--font-name=ter-v16n",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "printf 'e\\314\\201'; read x",
        ],
        Memory::default(),
        fonts.clone(),
    );
    // a cluster count of 1, then the cluster: its length and each scalar
    let words: Vec<u8> = [1u32, 2, 0x65, 0x301]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let asked = |h: &mut Harness| {
        requests(h, &fonts, 1)
            .iter()
            .any(|b| b.windows(words.len()).any(|w| w == words.as_slice()))
    };
    let mut found = false;
    for _ in 0..50 {
        if asked(&mut h) {
            found = true;
            break;
        }
        h.until(Duration::from_millis(100), |_| false);
    }
    assert!(found, "the cluster e U+0301 is asked for whole");
}

// ADR-1200 D9: a path under `font-engine=psf` is a one-source frit with a
// file: source, and vconsole's font, the kernel command line's over
// vconsole.conf's, one with the vconsole source; a name outside D1's
// characters, as a path is, goes to the helper as `generated`, a name it
// takes, and the font keeps the name it was given
#[test]
fn a_path_or_vconsoles_font_is_generated_as_a_one_source_frit() {
    let path = "/usr/share/kbd/consolefonts/ter-v12n.psf.gz";
    let (_dir, fonts) = recording("path");
    let mut h = with_fonts(
        &["--font-engine=psf", &format!("--font-name={path}")],
        fonts.clone(),
    );
    assert_eq!(h.app.font_name(), path);
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("generated", &format!("source=file:{path}\n"))]
    );
    let (_dir, mut fonts) = recording("vconsole");
    fonts.vconsole_conf = fonts.frits.join("vconsole.conf");
    fonts.kernel_cmdline = fonts.frits.join("cmdline");
    std::fs::write(&fonts.vconsole_conf, "FONT=lat9w-16\n").expect("vconsole.conf");
    std::fs::write(&fonts.kernel_cmdline, "quiet\n").expect("cmdline");
    let mut h = with_fonts(&[], fonts.clone());
    assert_eq!(h.app.font_name(), "lat9w-16");
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("lat9w-16", "source=vconsole\n")]
    );
    drop(h);
    for entry in std::fs::read_dir(&fonts.frits).expect("the directory") {
        let path = entry.expect("an entry").path();
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("request."))
        {
            std::fs::remove_file(path).expect("the old request");
        }
    }
    std::fs::write(&fonts.kernel_cmdline, "quiet vconsole.font=ter-v32n\n").expect("cmdline");
    let mut h = with_fonts(&[], fonts.clone());
    assert_eq!(h.app.font_name(), "ter-v32n");
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("ter-v32n", "source=vconsole\n")]
    );
}

// ADR-1200 D9: a name a frit bears is that frit, vconsole's included, and
// font-name wins over vconsole's font
#[test]
fn a_name_a_frit_bears_is_the_frit_and_font_name_wins_over_vconsole() {
    let (_dir, mut fonts) = recording("bears");
    fonts.vconsole_conf = fonts.frits.join("vconsole.conf");
    std::fs::write(&fonts.vconsole_conf, "FONT=t\n").expect("vconsole.conf");
    std::fs::write(fonts.frits.join("t.vtfrit"), "source=kbd:x\n").expect("the frit");
    let mut h = with_fonts(&[], fonts.clone());
    assert_eq!(h.app.font_name(), "t");
    assert_eq!(recorded(&mut h, &fonts, 1), [pair("t", "source=kbd:x\n")]);
    drop(h);
    let h = with_fonts(&["--font-name=ter-v16n"], fonts.clone());
    assert_eq!(h.app.font_name(), "ter-v16n");
}

// ADR-1200 D9: an outline family, font-name under font-engine=pango or
// freetype, is a one-source frit with a font: source
#[test]
fn a_family_is_generated_as_a_one_source_frit() {
    let (_dir, fonts) = recording("family");
    let mut h = with_fonts(
        &["--font-engine=pango", "--font-name=DejaVu Sans Mono"],
        fonts.clone(),
    );
    assert_eq!(h.app.font_name(), "DejaVu Sans Mono");
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("generated", "source=font:DejaVu Sans Mono\n")]
    );
}

// ADR-1200 D17: a zoom steps an outline frit by 1 pixel, as the C
// reference's outline engines did, each height but the stated one as
// wide as the built-in font there
#[test]
fn a_zoom_steps_an_outline_frit_by_one_pixel() {
    let (equal, minus) = (0x3d, 0x2d);
    let (_dir, fonts) = recording("outline-zoom");
    std::fs::write(
        fonts.frits.join("t.vtfrit"),
        "source=outline:/f.ttf\ncell=10x20\n",
    )
    .expect("the frit");
    let mut h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    let mut cells = vec![h.app.cells()[0]];
    for k in [equal, minus, minus] {
        h.app.input_event(key(Mods::CTRL, k), now());
        cells.push(h.app.cells()[0]);
    }
    assert_eq!(cells, [(10, 20), (8, 21), (10, 20), (8, 19)]);
}

/// The seat on the memory display measuring `dpi`, across and down, or
/// none, with `args`, the frit `t` of `text` in `fonts`, and a session that
/// waits.
fn at_density(args: &[&str], dpi: Option<(f64, f64)>, fonts: &Sources, text: &str) -> Harness {
    std::fs::write(fonts.frits.join("t.vtfrit"), text).expect("the frit");
    let args: Vec<&str> = args
        .iter()
        .copied()
        .chain(["--font-name=t", "--login", "--", "/bin/sh", "-c", "read x"])
        .collect();
    let memory = Memory {
        dpi,
        ..Memory::default()
    };
    Harness::fonts(&args, memory, fonts.clone())
}

/// The lines logged since the last call that say a display's density is
/// assumed.
fn assumed() -> Vec<String> {
    logged()
        .into_iter()
        .filter(|l| l.contains("reports no physical size"))
        .collect()
}

/// A 27-inch 2560x1440 monitor's density, 597x336 mm: about 109 DPI.
const DPI_27_INCH: Option<(f64, f64)> = Some((108.9, 108.9));

// the operator's decision on #145, 2026-10-28: an outline frit's size= is
// points, pixels = points x DPI / 72, at the display's measured density,
// else at 96 DPI assumed with one warning naming the display
#[test]
fn an_outline_frits_size_is_points_at_the_displays_density() {
    let (_dir, fonts) = recording("points");
    let frit = "source=outline:/f.ttf\nsize=12\n";
    assumed();
    let h = at_density(&["--font-size=0"], DPI_27_INCH, &fonts, frit);
    assert_eq!(h.app.cells(), [(8, 18)], "12 pt at 109 DPI");
    assert_eq!(assumed(), Vec::<String>::new());
    drop(h);
    let h = at_density(&["--font-size=0"], None, &fonts, frit);
    assert_eq!(h.app.cells(), [(8, 16)], "12 pt at 96 DPI");
    assert_eq!(
        assumed(),
        [
            "display MEM-1 reports no physical size: vtium assumes 96 DPI, which display-dpi can state"
        ]
    );
    // font-size is points too, and wins over size=
    let h = at_density(&["--font-size=10"], DPI_27_INCH, &fonts, frit);
    assert_eq!(h.app.cells(), [(8, 15)], "10 pt at 109 DPI");
}

// the operator's decision on #145, 2026-10-28: display-dpi states the
// density, over a measured one, and nothing is logged
#[test]
fn display_dpi_wins_over_the_measured_density_unlogged() {
    let (_dir, fonts) = recording("display-dpi");
    let frit = "source=outline:/f.ttf\nsize=12\n";
    assumed();
    for dpi in [None, DPI_27_INCH] {
        let h = at_density(&["--font-size=0", "--display-dpi=144"], dpi, &fonts, frit);
        assert_eq!(
            h.app.cells(),
            [(8, 24)],
            "12 pt at 144 DPI, measured {dpi:?}"
        );
    }
    assert_eq!(assumed(), Vec::<String>::new());
}

// #145, accepted by the team lead for the operator, 2026-10-28: points
// measure height, so they convert at the density down the rows as vtium
// draws them, across on a display turned a quarter, and at the lowest
// density of several displays
#[test]
fn points_convert_down_the_rows_at_the_lowest_density() {
    let equal = 0x3d;
    let minus = 0x2d;
    let (_dir, fonts) = recording("axis");
    let frit = "source=outline:/f.ttf\nsize=12\n";
    let mut h = at_density(&["--font-size=0"], Some((72.0, 144.0)), &fonts, frit);
    assert_eq!(h.app.cells()[0].1, 24, "12 pt at 144 DPI down");
    h.app.input_event(key(Mods::LOGO, equal), now());
    assert_eq!(h.app.cells()[0].1, 12, "turned a quarter, 72 DPI across");
    h.app.input_event(key(Mods::LOGO, minus), now());
    assert_eq!(h.app.cells()[0].1, 24);
    let second = Memory {
        dpi: Some((96.0, 96.0)),
        ..Memory::default()
    };
    h.app
        .add_video(PathBuf::from("/dev/mem1"), Box::new(second));
    assert!(h.until(SECOND, |a| a.cells().len() == 2));
    let heights: Vec<u32> = h.app.cells().iter().map(|c| c.1).collect();
    assert_eq!(heights, [16, 16], "the lower of 144 and 96 DPI");
}

// the operator's decision on #145, 2026-10-28: an outline frit zooms 1 pt
// at a time, converted to pixels: from 12 pt at 96 DPI, 13 pt is 17 px
#[test]
fn a_zoom_steps_an_outline_frit_by_one_point() {
    let (equal, minus) = (0x3d, 0x2d);
    let (_dir, fonts) = recording("point-zoom");
    let frit = "source=outline:/f.ttf\nsize=12\n";
    let mut h = at_density(&["--font-size=0"], None, &fonts, frit);
    let mut heights = vec![h.app.cells()[0].1];
    for k in [equal, equal, minus, minus, minus] {
        h.app.input_event(key(Mods::CTRL, k), now());
        heights.push(h.app.cells()[0].1);
    }
    assert_eq!(heights, [16, 17, 19, 17, 16, 15]);
}

// ADR-1200 D16, D17: a configured font-size wins over the frit's cell, as
// wide as the built-in font at that height; font-size=0 leaves the cell
#[test]
fn a_configured_font_size_wins_over_the_frits_cell() {
    let (_dir, fonts) = recording("sized");
    std::fs::write(fonts.frits.join("t.vtfrit"), "source=kbd:x\ncell=10x20\n").expect("the frit");
    let h = with_fonts(&["--font-name=t", "--font-size=24"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 24)], (40, 6)));
    let h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(10, 20)], (32, 8)));
}

/// [`osc50`] with the frits `frits`, each its name and text, in `fonts`.
fn osc50_frits(args: &[&str], script: &str, fonts: &Sources, frits: &[(&str, &str)]) -> Harness {
    for (name, text) in frits {
        std::fs::write(fonts.frits.join(format!("{name}.vtfrit")), text).expect("a frit");
    }
    osc50(args, script, fonts.clone())
}

// ADR-1200 D5: OSC 50 naming a frit switches the live console to it as a
// zoom does, in the frit's cell on a new grid, with a helper of its own
// asked for its glyphs; a built-in by name keeps to its own sizes
// covers: text.font-change
#[test]
fn osc_50_switches_to_a_frit_and_its_helper_is_asked() {
    let (_dir, fonts) = recording("osc50");
    let mut h = osc50_frits(
        &["--font-size=0"],
        "\\033]50;t\\007",
        &fonts,
        &[("t", "source=kbd:x\ncell=10x20\n")],
    );
    assert_eq!(h.app.font_name(), "t");
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(10, 20)], (32, 8)));
    assert_eq!(
        recorded(&mut h, &fonts, 1),
        [pair("t", "source=kbd:x\ncell=10x20\n")]
    );
    let mut h = osc50(&["--font-size=8"], "\\033]50;unifont\\007", no_fonts());
    assert_eq!(h.app.font_name(), "unifont");
    assert_eq!(h.app.cells(), [(8, 16)]);
    h.app.input_event(key(Mods::CTRL, 0x2d), now());
    assert_eq!(h.app.cells(), [(8, 16)], "no Unscii-8 below it");
}

// ADR-1200 D5: a switch from one frit to another starts a helper for the
// new frit, with a store of its own
#[test]
fn osc_50_from_one_frit_to_another_starts_a_helper_for_it() {
    let (_dir, fonts) = recording("osc50-two");
    for (name, text) in [("a", "source=kbd:a\n"), ("b", "source=kbd:b\n")] {
        std::fs::write(fonts.frits.join(format!("{name}.vtfrit")), text).expect("a frit");
    }
    // The switch waits for `a`'s helper to have its request, which the
    // switch's end of `a` would otherwise cut short.
    let before = format!(
        "while ! ls {}/request.* >/dev/null 2>&1; do sleep 0.01; done; ",
        fonts.frits.display()
    );
    let mut h = osc50_after(
        &["--font-name=a"],
        &before,
        "\\033]50;b\\007",
        fonts.clone(),
    );
    assert_eq!(h.app.font_name(), "b");
    assert_eq!(
        recorded(&mut h, &fonts, 2),
        [pair("a", "source=kbd:a\n"), pair("b", "source=kbd:b\n")]
    );
}

/// [`RECORDER`]'s start for [`recording_held`]: the first helper started
/// holds off reading for 5 s, a process it starts holding its output
/// meanwhile, as the recorder's checker does. It shows that process's pid
/// as `descendant.*`, its output's pipe in `output`, and then its own pid
/// as `held.*`; each later helper shows its pid as `started.*`.
const HOLD: &str = r#"
if mkdir "$1/held" 2>/dev/null; then
    sleep 5 &
    : >"$1/descendant.$!"
    readlink /proc/$!/fd/1 >"$1/output"
    : >"$1/held.$$"
    wait
fi
: >"$1/started.$$"
"#;

/// [`recording`], its first helper held by [`HOLD`].
fn recording_held(name: &str) -> (Scratch, Sources) {
    let (scratch, mut fonts) = recording(name);
    if let Some(helper) = &mut fonts.helper {
        helper.args[1] = OsString::from(format!("{HOLD}{RECORDER}"));
    }
    (scratch, fonts)
}

/// The pids the stand-in in `fonts` showed as `prefix.*`.
fn shown_pids(fonts: &Sources, prefix: &str) -> Vec<u32> {
    std::fs::read_dir(&fonts.frits)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix(prefix)?.strip_prefix('.')?.parse().ok()
        })
        .collect()
}

/// The one pid the stand-in in `fonts` showed as `prefix.*`.
fn shown_pid(fonts: &Sources, prefix: &str) -> u32 {
    let pids = shown_pids(fonts, prefix);
    let &[pid] = pids.as_slice() else {
        panic!("one {prefix}: {pids:?}");
    };
    pid
}

/// Whether the process `pid` is gone and reaped.
fn gone(pid: u32) -> bool {
    !Path::new(&format!("/proc/{pid}")).exists()
}

/// The seat drawing frit `a`, whose helper [`HOLD`] holds, switched by
/// OSC 50 to frit `b` while it holds; the directory, the seat and the
/// sources.
fn switched_while_held(name: &str) -> (Scratch, Harness, Sources) {
    let (dir, fonts) = recording_held(name);
    for (name, text) in [("a", "source=kbd:a\n"), ("b", "source=kbd:b\n")] {
        std::fs::write(fonts.frits.join(format!("{name}.vtfrit")), text).expect("a frit");
    }
    let before = format!(
        "while ! ls {}/held.* >/dev/null 2>&1; do sleep 0.01; done; ",
        fonts.frits.display()
    );
    let h = osc50_after(
        &["--font-name=a"],
        &before,
        "\\033]50;b\\007",
        fonts.clone(),
    );
    assert_eq!(h.app.font_name(), "b");
    (dir, h, fonts)
}

// vtium/vtium#160: a switch that ends the first frit's helper before it
// has read its request starts the next frit's helper once, and the helper
// ended is reaped as it exits, though a process it started held its
// output
#[test]
fn osc_50_ending_a_helper_before_it_reads_starts_one_for_the_next_and_reaps_it() {
    let (_dir, mut h, fonts) = switched_while_held("osc50-early");
    assert_eq!(recorded(&mut h, &fonts, 1), [pair("b", "source=kbd:b\n")]);
    let a = shown_pid(&fonts, "held");
    assert!(
        h.until(Duration::from_secs(2), |_| gone(a)),
        "a's helper is reaped"
    );
    h.until(Duration::from_millis(500), |_| false);
    assert_eq!(
        shown_pids(&fonts, "started").len(),
        1,
        "b's helper starts once"
    );
    assert_eq!(recorded(&mut h, &fonts, 1), [pair("b", "source=kbd:b\n")]);
}

// A helper the console ends takes the processes it started in its process
// group with it, so none holds its output, and the console's end of that
// output closes as the helper exits
#[test]
fn ending_a_helper_ends_what_it_started_and_closes_its_output() {
    let (_dir, mut h, fonts) = switched_while_held("osc50-group");
    let descendant = shown_pid(&fonts, "descendant");
    let output = std::fs::read_to_string(fonts.frits.join("output")).expect("the output's pipe");
    let output = output.trim();
    assert!(output.starts_with("pipe:["), "{output}");
    let open = || {
        std::fs::read_dir("/proc/self/fd")
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|e| std::fs::read_link(e.path()).is_ok_and(|l| l.as_os_str() == output))
    };
    assert!(
        h.until(Duration::from_secs(2), |_| gone(descendant)),
        "what a's helper started is ended"
    );
    assert!(
        h.until(Duration::from_secs(2), |_| !open()),
        "the console's end of a's output is closed"
    );
}

// ADR-1200 D5: a name no frit bears, a path, a frit whose cell cannot be
// read, or OSC 50 under `no-allow-font-ops` leaves the font as it was
#[test]
fn osc_50_leaves_the_font_for_a_name_no_frit_bears_or_with_font_ops_off() {
    let (_dir, fonts) = recording("osc50-off");
    for (args, script) in [
        (&["--font-size=12"][..], "\\033]50;absent\\007"),
        (&["--font-size=12"][..], "\\033]50;/etc/passwd\\007"),
        (&["--font-size=12"][..], "\\033]50;bad\\007"),
        (
            &["--font-size=12", "--no-allow-font-ops"][..],
            "\\033]50;t\\007",
        ),
    ] {
        let h = osc50_frits(
            args,
            script,
            &fonts,
            &[("t", "source=kbd:x\n"), ("bad", "source=kbd:x\ncell=10\n")],
        );
        assert_eq!(h.app.font_name(), "unscii-8", "{args:?} {script}");
        assert_eq!(h.app.cells(), [(8, 8)], "{args:?} {script}");
    }
}

/// Whether the session's screen shows OSC 50's reply naming `name`, as
/// its tty echoed it; waited for.
fn replied(h: &mut Harness, name: &str) -> bool {
    let reply = format!("]50;{name}");
    h.until(SECOND, |a| {
        a.screen_text().iter().any(|l| l.contains(&reply))
    })
}

// ADR-1200 D5: `OSC 50 ; ?` names the current frit, a frit made on the fly
// included
#[test]
fn osc_50_query_names_the_current_frit() {
    let (_dir, fonts) = recording("osc50-query");
    let mut h = osc50_frits(
        &["--font-name=t"],
        "\\033]50;?\\007",
        &fonts,
        &[("t", "source=kbd:x\n")],
    );
    assert!(replied(&mut h, "t"), "{:?}", h.app.screen_text());
    let mut h = osc50(&["--font-name=ter-v16n"], "\\033]50;?\\007", fonts.clone());
    assert!(replied(&mut h, "ter-v16n"), "{:?}", h.app.screen_text());
}

// ADR-1200 D5: `vtium-vitrify set` writes the frit and then the query in
// one write; the query's reply names the new frit
#[test]
fn a_query_after_osc_50_names_the_new_frit() {
    let (_dir, fonts) = recording("osc50-then");
    let mut h = osc50_frits(
        &["--font-size=12"],
        "\\033]50;t\\007\\033]50;?\\007",
        &fonts,
        &[("t", "source=kbd:x\n")],
    );
    assert!(replied(&mut h, "t"), "{:?}", h.app.screen_text());
}

// A program's flood of refused OSC 50 requests logs one line, and the
// switch that follows counts the rest.
#[test]
fn a_flood_of_refused_osc_50_requests_logs_one_line() {
    let (_dir, fonts) = recording("osc50-flood");
    logged();
    let flood = "\\033]50;absent\\007\\033]50;other\\007".repeat(20);
    let h = osc50_frits(
        &["--font-size=12"],
        &format!("{flood}\\033]50;t\\007"),
        &fonts,
        &[("t", "source=kbd:x\n")],
    );
    assert_eq!(h.app.font_name(), "t");
    let lines = logged();
    let refused: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("OSC 50 asked"))
        .collect();
    assert_eq!(refused.len(), 1, "{lines:?}");
    assert!(refused[0].contains("absent"), "{refused:?}");
    assert!(
        lines
            .iter()
            .any(|l| l == "OSC 50 was refused 39 more times before this switch"),
        "{lines:?}"
    );
}

// ADR-1200 D8: the kernel's font, read at start, is handed to the helper
// with each request, for a frit's console: source to read
#[test]
fn the_kernels_font_is_handed_to_the_helper() {
    use vtium_platform::vt::font::{ConsoleFont, UniPair};
    let (_dir, fonts) = recording("console");
    std::fs::write(fonts.frits.join("t.vtfrit"), "source=console:1\n").expect("the frit");
    let console = ConsoleFont {
        width: 8,
        height: 2,
        charcount: 2,
        data: vec![0x81, 0x42, 0x24, 0x18],
        unimap: vec![UniPair {
            unicode: 0x41,
            fontpos: 1,
        }],
    };
    let mut h = Harness::boot_console(
        &["--font-name=t", "--login", "--", "/bin/sh", "-c", "read x"],
        FakeVt::new(),
        Some(Box::new(Memory::default())),
        fonts.clone(),
        Some(console),
    );
    let all = requests(&mut h, &fonts, 1);
    let (_, _, rest) = first_request(&all[0]).expect("a request");
    let words = |b: &[u8], n: usize| -> Vec<u32> {
        b.chunks(4)
            .take(n)
            .map(|w| u32::from_le_bytes(w.try_into().expect("a word")))
            .collect()
    };
    // the cell, the reserved capabilities, then the codepoints, the
    // clusters and the held ranges, each counted
    let codepoints = words(&rest[12..], 1)[0] as usize;
    let mut rest = &rest[16 + 4 * codepoints..];
    let clusters = words(rest, 1)[0] as usize;
    rest = &rest[4..];
    for _ in 0..clusters {
        let len = words(rest, 1)[0] as usize;
        rest = &rest[4 + 4 * len..];
    }
    let held = words(rest, 1)[0] as usize;
    let rest = &rest[4 + 8 * held..];
    assert_eq!(words(rest, 5), [1, 8, 2, 2, 4], "the font's shape");
    assert_eq!(&rest[20..24], [0x81, 0x42, 0x24, 0x18], "its glyphs");
    assert_eq!(words(&rest[24..], 1), [1], "one pair in its map");
    assert_eq!(&rest[28..], [0x41, 0, 1, 0], "U+0041 at 1");
}

// ADR-1200 D1: the frit's glyph-source= lines set the font's sprite
// policy, and the configuration's keys override them
#[test]
fn a_frits_glyph_source_sets_the_policy_and_the_keys_override_it() {
    use vtium_font::sprite::Range;
    use vtium_font::{Missing, Source, SpritePolicy};
    let (_dir, fonts) = recording("glyph-source");
    std::fs::write(
        fonts.frits.join("t.vtfrit"),
        "source=kbd:x\nglyph-source=font\n",
    )
    .expect("the frit");
    let h = with_fonts(&["--font-name=t"], fonts.clone());
    assert_eq!(
        h.app.font_policy(),
        Some((SpritePolicy::all(Source::Font), Missing::Replacement))
    );
    let h = with_fonts(
        &["--font-name=t", "--glyph-source-braille=auto"],
        fonts.clone(),
    );
    let want = SpritePolicy::all(Source::Font).with(Range::Braille, Source::Auto);
    assert_eq!(h.app.font_policy(), Some((want, Missing::Replacement)));
}
