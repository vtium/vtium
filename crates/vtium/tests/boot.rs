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
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the harness's command line");
        // What `main` takes before the seat; the seat's input is the
        // test's.
        let _ = Start::new(&loaded.config, "C".to_owned());
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), Rc::new(now), env);
        app.add_video(PathBuf::from("/dev/mem0"), video);
        app.insert_blank();
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Harness { lp, app }
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

/// A device whose `Asleep` waits until the test lets it through, as a
/// flip in flight delays a DRM device's.
struct LateSleep {
    memory: Memory,
    held: bool,
}

impl Video for LateSleep {
    fn backend(&self) -> &'static str {
        "memory"
    }
    fn events(&mut self) -> Vec<Event> {
        let held = self.held;
        let mut events = self.memory.events();
        let late: Vec<Event> = events
            .iter()
            .filter(|e| held && matches!(e, Event::Asleep))
            .cloned()
            .collect();
        events.retain(|e| !(held && matches!(e, Event::Asleep)));
        self.memory.events.extend(late);
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
    let late = LateSleep {
        memory: Memory::default(),
        held: true,
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
    h.app
        .videos
        .get_mut(Path::new("/dev/mem0"))
        .and_then(|v| v.as_any().downcast_mut::<LateSleep>())
        .expect("the late device")
        .held = false;
    assert!(
        h.until(SECOND, |a| !a.vt.is_active()),
        "acknowledged once the last device slept"
    );
}
