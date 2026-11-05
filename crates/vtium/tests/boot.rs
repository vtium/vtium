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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vtium::app::{App, Exit};
use vtium::config;
use vtium::font::Sources;
use vtium::start::Start;
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{Button, InputEvent, KeyEvent, Mods, PointerEvent};
use vtium_platform::vt::{FakeVt, Switch, Vt, VtEvent};

#[path = "common/clock.rs"]
mod clock;
#[path = "common/vt.rs"]
mod counting;
#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/helper.rs"]
mod stand_in;
use clock::SeatClock;
use counting::CountingVt;
use scratch::Scratch;
use stand_in::real_helper;

const W: u32 = 320;
const H: u32 = 160;
const ID: DisplayId = DisplayId(7);

/// A device with one display, its frame buffer in memory. It holds the
/// seat to what a framebuffer holds it to with its one buffer's token: a
/// second token is refused while one is out, a token from
/// before a sleep, a wake-up or a blank comes back too late and is
/// dropped, a dropped token leaves the buffer at age 0, a failed swap still
/// leaves the frame in the buffer, and a sleep that finds the buffer lent
/// reports `Asleep` only once the token comes back.
struct Memory {
    w: u32,
    h: u32,
    /// What the display shows: the buffer lent, as the last swap left it.
    pixels: Vec<u8>,
    /// The buffer each token lends, drawn into on the render thread.
    drawn: Arc<Mutex<Vec<u8>>>,
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
    /// Each token lent's age, in order.
    ages: Vec<u32>,
    /// Taken anew at each sleep, wake-up and blank.
    lease: u64,
    /// A token was lent and not given back since.
    lent: bool,
    /// Tokens asked for and refused while one was out.
    refused_while_out: u32,
    /// The sleep found the buffer lent: it clears the buffer and reports
    /// `Asleep` when the token comes back.
    asleep_owed: bool,
    refresh_mhz: u32,
    /// Every swap fails, as on a display announced and never shown.
    refuses: bool,
    /// The density the display measures, across and down; `None` for one
    /// that reports no physical size.
    dpi: Option<(f64, f64)>,
    /// A second buffer and its age, for a display that lends its two
    /// buffers in turn, each drawn two frames back once both hold a frame.
    /// What leaves the buffer lent at age 0 leaves this one at age 0 too,
    /// and a sleep clears both, as a display's two buffers are.
    double: Option<(Arc<Mutex<Vec<u8>>>, u32)>,
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
            drawn: Arc::default(),
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
            lease: 0,
            lent: false,
            refused_while_out: 0,
            asleep_owed: false,
            refresh_mhz: 60_000,
            refuses: false,
            // the C reference's fixed density, where a point is a pixel
            dpi: Some((72.0, 72.0)),
            double: None,
        }
    }
}

impl Memory {
    /// Every buffer of the display holds a frame of unknown age.
    fn ages_unknown(&mut self) {
        self.age = 0;
        if let Some((_, other_age)) = &mut self.double {
            *other_age = 0;
        }
    }

    /// Ends a sleep the token held, once no token holds the buffer: clears
    /// it and reports `Asleep`.
    fn pay_clear(&mut self) {
        if self.asleep_owed && Arc::strong_count(&self.drawn) == 1 {
            self.asleep_owed = false;
            self.drawn.lock().unwrap().fill(0);
            self.events.push(Event::Asleep);
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // One token at a time: the seat asks again while one is out when
        // a new renderer, as a zoom or a font switch makes, draws before
        // the old one's frame comes back, and is refused, as a display
        // refuses.
        if Arc::strong_count(&self.drawn) > 1 {
            self.refused_while_out += 1;
            return None;
        }
        if !self.ready_for_frame(id) {
            return None;
        }
        if std::mem::take(&mut self.lent) {
            // dropped without being given back
            self.ages_unknown();
        }
        self.lent = true;
        let age = self.age;
        self.ages.push(age);
        let memory: Arc<Shared> = self.drawn.clone();
        Some(Token::new(id, (self.w, self.h), self.w * 4, age, memory).leased(self.lease))
    }
    fn give_back(&mut self, token: Token) {
        self.lent = false;
        drop(token);
        self.pay_clear();
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        self.lent = false;
        if token.lease() != self.lease {
            drop(token);
            self.pay_clear();
            self.ages_unknown();
            return Err(SwapError::Dropped);
        }
        if damage.is_empty() && self.dpms == Some(Dpms::On) {
            return Ok(Swapped::Nothing);
        }
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        // The buffer holds the frame, whether the swap takes or not.
        self.age = 1;
        if self.refuses {
            return Err(SwapError::Failed(io::Error::other("refused")));
        }
        self.frames += 1;
        // The display shows the buffer the frame was drawn into.
        self.pixels.clone_from(&self.drawn.lock().unwrap());
        // the next frame goes into the other buffer, which has missed this
        // frame too, unless it never held one
        if let Some((other, other_age)) = &mut self.double {
            std::mem::swap(&mut self.drawn, other);
            self.age = match *other_age {
                0 => 0,
                n => n + 1,
            };
            *other_age = 1;
        }
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake && self.dpms == Some(Dpms::On) && Arc::strong_count(&self.drawn) == 1
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        if dpms != Dpms::On && self.dpms == Some(Dpms::On) {
            self.lease += 1;
        }
        self.dpms = Some(dpms);
        self.dpms_sets.push(dpms);
        if dpms == Dpms::On {
            self.ages_unknown();
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.sleeps += 1;
        self.lease += 1;
        self.awake = false;
        if let Some((other, _)) = &self.double {
            other.lock().unwrap().fill(0);
        }
        if Arc::strong_count(&self.drawn) > 1 {
            self.asleep_owed = true;
        } else {
            self.drawn.lock().unwrap().fill(0);
            self.events.push(Event::Asleep);
        }
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.wake_ups += 1;
        self.lease += 1;
        // A sleep the token still holds ends now.
        if std::mem::take(&mut self.asleep_owed) {
            self.events.push(Event::Asleep);
        }
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
        // A token from before still holds the buffer, which the next frame
        // after it comes back draws whole.
        if Arc::strong_count(&self.drawn) == 1 {
            *self.drawn.lock().unwrap() = self.pixels.clone();
        }
        self.awake = true;
        self.dpms = Some(Dpms::On);
        self.ages_unknown();
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
    /// Times its bell rang.
    bells: u32,
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
    fn bell(&mut self) {
        self.bells += 1;
    }
    fn number(&self) -> Option<u16> {
        None
    }
}

struct Harness {
    lp: Loop<App>,
    app: App,
    /// The seat's clock, which only [`Harness::skip`] moves.
    seat: SeatClock,
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

/// The real time, which bounds a wait on another thread or process and
/// times the flips of the [`Slow`] device. The seat itself reads
/// [`Harness::at`], which only the test moves.
#[expect(
    clippy::disallowed_methods,
    reason = "a wait on another thread is bounded in real time"
)]
fn now() -> Instant {
    Instant::now()
}

impl Harness {
    fn new(args: &[&str]) -> Harness {
        Harness::with_vt(args, FakeVt::new())
    }

    /// [`Harness::new`] on the fake VT, counting the releases vtium
    /// acknowledges; the harness and the count.
    fn counting(args: &[&str]) -> (Harness, Rc<Cell<u32>>) {
        let (vt, released) = CountingVt::new(FakeVt::new());
        (Harness::with_vt(args, vt), released)
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
        Harness::boot_set(args, vt, video, fonts, |app| {
            if let Some(console) = console {
                app.set_console_font(console);
            }
        })
    }

    /// [`Harness::boot_with`], with `set` run on the seat before its video
    /// device is added and it starts.
    fn boot_set<V: Vt + 'static>(
        args: &[&str],
        vt: V,
        video: Option<Box<dyn Video>>,
        fonts: Sources,
        set: impl FnOnce(&mut App),
    ) -> Harness {
        Harness::boot_seat(SeatClock::new(), args, vt, video, fonts, set)
    }

    /// [`Harness::build`] on the seat's clock `seat`, which a test's device
    /// reads too.
    fn build_on<V: Vt + 'static>(
        seat: SeatClock,
        args: &[&str],
        vt: V,
        video: Box<dyn Video>,
    ) -> Harness {
        Harness::boot_seat(seat, args, vt, Some(video), no_fonts(), |_| {})
    }

    /// [`Harness::boot_set`] on the seat's clock `seat`.
    fn boot_seat<V: Vt + 'static>(
        seat: SeatClock,
        args: &[&str],
        vt: V,
        video: Option<Box<dyn Video>>,
        fonts: Sources,
        set: impl FnOnce(&mut App),
    ) -> Harness {
        let sized = args.iter().any(|a| a.starts_with("--font-size"));
        let args: Vec<OsString> = (!sized)
            .then_some("--font-size=16")
            .into_iter()
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the harness's command line");
        // What `main` takes before the seat; the seat's input is the
        // test's.
        let _ = Start::new(&loaded.config, "C".to_owned());
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), seat.clock(), env);
        app.set_font_sources(fonts);
        set(&mut app);
        if let Some(video) = video {
            app.add_video(PathBuf::from("/dev/mem0"), video);
        }
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(seat.now());
        Harness { lp, app, seat }
    }

    /// The seat's time: the time it booted at, moved on by each
    /// [`Harness::skip`].
    fn at(&self) -> Instant {
        self.seat.now()
    }

    /// Moves the seat's clock ahead by `by`, as if that long passed with
    /// nothing to do.
    fn skip(&mut self, by: Duration) {
        self.seat.advance(by);
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
        if any.is::<Withhold>() {
            return &mut any
                .downcast_mut::<Withhold>()
                .expect("a withholding device")
                .memory;
        }
        if any.is::<Planed>() {
            return &mut any
                .downcast_mut::<Planed>()
                .expect("a planed device")
                .memory;
        }
        any.downcast_mut::<Memory>().expect("the memory device")
    }

    /// Moves the seat's clock ahead by `by` and runs one turn at the new
    /// time, which acts on every timer then due.
    fn skip_turn(&mut self, by: Duration) {
        self.skip(by);
        self.app.turn(self.at());
    }

    /// One turn of the loop as `main` runs it: the loop waits for the
    /// wake-up the seat asks for, at most `most`, then the seat's clock
    /// moves on by that wait and the seat turns. The clock moves by the
    /// wait the loop was told, also when something woke the loop sooner.
    /// Returns the wake-up the seat asked for.
    fn sleep_turn(&mut self, most: Duration) -> Option<Duration> {
        let asked = self.app.next_wake(self.at());
        let wait = asked.map_or(most, |w| w.min(most));
        self.lp
            .dispatch(Some(wait), &mut self.app)
            .expect("dispatch");
        self.skip_turn(wait);
        asked
    }

    /// Turns the loop until a frame is shown and its flip read, so a skip
    /// of the seat's clock past `display-wait` finds a display.
    fn shown(&mut self) {
        assert!(
            self.until_memory(BOUND, |m| m.frames > 0 && m.events.is_empty()),
            "a frame shown"
        );
    }

    /// Turns the loop until no display has a frame composing, so the
    /// next frame a display shows is composed from the screen as it is
    /// then.
    fn settled(&mut self) {
        assert!(
            self.until(BOUND, |a| a.heads().iter().all(|h| h.frames_out() == 0)),
            "no frame composing"
        );
    }

    /// The pixels of a frame drawn whole from the screen as it is now,
    /// with no frame left composing after it.
    fn drawn_now(&mut self) -> Vec<u8> {
        self.settled();
        let before = self.memory().frames;
        self.memory().events.push(Event::Redraw(ID));
        assert!(
            self.until_memory(BOUND, |m| m.frames > before),
            "a frame drawn"
        );
        self.settled();
        self.memory().pixels.clone()
    }

    /// Turns the loop until `done` holds of the whole harness, at most
    /// `limit`, for a state that only a device's own fields show.
    fn eventually(&mut self, limit: Duration, mut done: impl FnMut(&mut Harness) -> bool) -> bool {
        let end = now() + limit;
        loop {
            if done(self) {
                return true;
            }
            if now() > end {
                return false;
            }
            self.until(Duration::from_millis(20), |_| false);
        }
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

/// How long a wait on a state that another thread or process brings, such
/// as a frame the render thread draws or a line a session prints, may run
/// before the test fails: long enough for a loaded machine, and only a
/// bound, since each wait ends at the state it waits for.
const BOUND: Duration = Duration::from_secs(10);

#[test]
fn the_seat_boots_draws_takes_keys_and_exits() {
    let (mut h, released) = Harness::counting(&[
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
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.contains("hello from"))
    }));
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    assert!(
        h.until_memory(BOUND, |m| m.frames > 0),
        "a frame was presented"
    );
    assert!(
        h.memory().pixels.iter().any(|&b| b != 0),
        "something was drawn"
    );

    // Keys go through the key order to the child, which echoes them.
    h.type_str("ok\r");
    assert!(h.until(BOUND, |a| {
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
        .input_event(InputEvent::Key(ctrl_super_f12.clone()), h.at());
    assert!(
        h.until_memory(BOUND, |m| !m.awake),
        "asleep for the release"
    );
    assert!(
        h.until(BOUND, |_| released.get() == 1),
        "the release acknowledged"
    );
    assert!(!h.app.vt.is_active());
    let before = h.memory().frames;
    h.app.input_event(InputEvent::Key(ctrl_super_f12), h.at());
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    assert!(
        h.until_memory(BOUND, |m| m.awake && m.frames > before),
        "the acquire wakes the display and draws it again"
    );

    // After a second with no input the display blanks; a key wakes it.
    let unblanks = h.memory().unblanks;
    h.skip_turn(Duration::from_millis(900));
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit before dpms-timeout");
    h.skip_turn(Duration::from_millis(200));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    h.type_str("x");
    assert_eq!(h.memory().unblanks, unblanks + 1);
    assert_eq!(h.memory().dpms, Some(Dpms::On));

    // The child ends; under oneshot vtium ends, switching back first.
    h.type_str("\r");
    assert!(h.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::Oneshot));
    assert!(!h.app.vt.is_active(), "the VT was given back");
}

#[test]
fn a_command_that_fails_at_once_is_started_five_times_more() {
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", "exit 3"]);
    assert!(h.until(BOUND, |a| a.finished().is_some()));
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
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "ready")));
    h.type_keys("a", false);
    h.type_keys("b", true);
    h.type_keys("\r", false);
    assert!(h.until(BOUND, |a| {
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
    /// Whether any pixel of the 8 by 16 cells `cols` of text row `row`
    /// is lit: the display shows something drawn there.
    fn inked(&self, row: u32, cols: std::ops::Range<u32>) -> bool {
        (row * 16..row * 16 + 16).any(|y| {
            cols.clone().flat_map(|c| c * 8..c * 8 + 8).any(|x| {
                let at = ((y * self.w + x) * 4) as usize;
                self.pixels.get(at..at + 3).is_some_and(|p| p != [0, 0, 0])
            })
        })
    }

    /// Whether any pixel is `rgb`; the buffer is XRGB8888, little-endian.
    fn has(&self, [r, g, b]: [u8; 3]) -> bool {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] == [b, g, r])
    }
}

/// The seat with `options`, once the program's `text` is drawn.
fn decorated(options: &[&str], text: &str) -> Harness {
    let script = format!("printf '{text}\\n'; read x");
    let mut args = options.to_vec();
    args.extend(["--login", "--", "/bin/sh", "-c", &script]);
    let mut h = Harness::new(&args);
    assert!(h.until(BOUND, |a| shows(a, "U S")));
    // The S, two cells past the cursor's first place, is drawn.
    assert!(
        h.until_memory(BOUND, |m| m.inked(0, 2..3)),
        "the text drawn"
    );
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
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "held")));
    assert!(h.until_memory(BOUND, |m| m.frames > 0 && m.age == 1));
    let (wake_ups, frames) = (h.memory().wake_ups, h.memory().frames);
    let lent = h.memory().ages.len();
    h.scripted().events.push(VtEvent::Retaken);
    assert!(
        h.until_memory(BOUND, |m| m.wake_ups == wake_ups + 1 && m.frames > frames),
        "the device is set up again and a frame drawn after it"
    );
    // The wake-up made the buffer's contents unknown, and a token lent
    // before it comes back too late: the first token lent after it is age
    // 0, so the frame is drawn whole.
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
    h.shown();
    h.skip_turn(Duration::from_millis(1100));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    let wake_ups = h.memory().wake_ups;
    h.scripted().events.push(VtEvent::Retaken);
    // The next turn takes the re-take.
    h.app.turn(h.at());
    // Set up again only when input ends the blank, as a hotplug while
    // blanked is: until then the displays stay dark.
    assert_eq!(h.memory().wake_ups, wake_ups, "no set-up while blanked");
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "still dark");
    h.type_str("a");
    assert_eq!(h.memory().wake_ups, wake_ups + 1, "set up at the input");
    assert_eq!(h.memory().dpms, Some(Dpms::On));
    // Without a re-take, input only unblanks.
    h.shown();
    h.skip_turn(Duration::from_millis(1100));
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "blanked again");
    h.type_str("b");
    assert_eq!(h.memory().wake_ups, wake_ups + 1);
}

/// A seat that blanks after a minute with no input, on the scripted VT.
/// Its tests move the seat's clock with [`Harness::skip_turn`].
fn blanking() -> Harness {
    Harness::with_vt(
        &[
            "--dpms-timeout=60",
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
    h.shown();
    h.skip_turn(Duration::from_secs(40));
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit before the timeout");
    let wake_ups = h.memory().wake_ups;
    h.scripted().events.push(VtEvent::Retaken);
    assert!(
        h.until_memory(BOUND, |m| m.wake_ups == wake_ups + 1),
        "set up again"
    );
    // The re-take is not activity: the blank comes a minute after the
    // start, not a minute after the re-take.
    h.skip_turn(Duration::from_secs(21));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked at the start's timeout"
    );
}

#[test]
fn an_acquire_restarts_the_blanks_countdown() {
    let mut h = blanking();
    h.shown();
    h.skip_turn(Duration::from_secs(40));
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    let wake_ups = h.memory().wake_ups;
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.until_memory(BOUND, |m| m.wake_ups == wake_ups + 1),
        "woken by the acquire"
    );
    // The switch back is the user's: the countdown starts again from it.
    h.skip_turn(Duration::from_secs(40));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::On),
        "lit 40 s after the acquire"
    );
    h.skip_turn(Duration::from_secs(21));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked a minute after the acquire"
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
    assert!(h.until(BOUND, |a| a.vt.is_active()), "acquired");
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
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    add(&mut h, "/dev/mem2");
    assert_eq!(counts(&mut h, "/dev/mem2"), (1, 0), "not shown: slept");
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.eventually(BOUND, |h| counts(h, "/dev/mem2") == (1, 1)),
        "woken by the acquire"
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
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
    // The re-take's wake-up retries past the blank.
    h.late().held = Some(Event::Awake);
    h.scripted().events.push(VtEvent::Retaken);
    h.app.turn(h.at());
    h.skip_turn(Duration::from_millis(1100));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    // Its modeset lights the display, and its `Awake` arrives.
    h.memory().dpms = Some(Dpms::On);
    h.late().held = None;
    assert!(
        h.until_memory(BOUND, |m| m.events.is_empty()),
        "the `Awake` taken"
    );
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "dark again");
    // Still blanked: the next key unblanks.
    h.type_str("a");
    assert_eq!(h.memory().unblanks, 1);
    assert_eq!(h.memory().dpms, Some(Dpms::On));
}

/// A file the test writes for its session to go on at, in a directory
/// named for `name`, and the shell that waits for it.
fn gate(name: &str) -> (Scratch, PathBuf, String) {
    let dir = Scratch::new(name);
    let path = dir.join("go");
    let wait = format!("while [ ! -e {} ]; do sleep 0.05; done", path.display());
    (dir, path, wait)
}

/// A seat that blanks after a minute with no input, on the scripted VT,
/// whose session runs `script` with `options` before the command; its
/// tests move the seat's clock, as [`blanking`]'s do.
fn program_blank_in_a_minute(options: &[&str], script: &str) -> Harness {
    let mut args = vec!["--dpms-timeout=60"];
    args.extend(options);
    args.extend(["--login", "--", "/bin/sh", "-c", script]);
    Harness::with_vt(&args, ScriptedVt::default())
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
    let (_dir, go, wait) = gate("wake-go");
    let mut h = program_blank_in_a_minute(&[], &format!(r"{wait}; printf '\033[13]'; read x"));
    h.shown();
    h.skip_turn(Duration::from_secs(61));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    let unblanks = h.memory().unblanks;
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(BOUND, |m| m.dpms == Some(Dpms::On)),
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
    let go_dir = Scratch::new("ris-go");
    let go = go_dir.join("go");
    let script = format!(
        r"printf '\033[9;0]ready'; while [ ! -e {} ]; do sleep 0.05; done; printf '\033creset'; read x",
        go.display()
    );
    let mut h = Harness::with_vt(
        &[
            "--dpms-timeout=60",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            &script,
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until(BOUND, |a| shows(a, "ready")),
        "the program's 0 taken"
    );
    h.shown();
    h.skip_turn(Duration::from_secs(120));
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit under the program's 0");
    std::fs::write(&go, b"").expect("the go file");
    assert!(h.until(BOUND, |a| shows(a, "reset")), "the RIS taken");
    h.skip_turn(Duration::from_secs(61));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked by dpms-timeout after RIS"
    );
}

// A session that turns the blank off and exits leaves the next session
// `dpms-timeout`: the turn that starts it takes the RIS's request, so no
// request waits for a later turn.
#[test]
fn a_respawned_session_blanks_after_dpms_timeout() {
    let marker_dir = Scratch::new("blank-respawn");
    let marker = marker_dir.join("marker");
    let script = format!(
        "if [ -e {m} ]; then read x; else : > {m}; printf '\\033[9;0]'; fi",
        m = marker.display()
    );
    let mut h = program_blank(&[], &script);
    let end = now() + BOUND;
    let mut blanked = false;
    // Each turn moves the seat's clock 20 ms, so the respawn and the
    // blank come in their turns however fast the machine runs.
    while now() < end && !blanked {
        let wait = h
            .app
            .next_wake(h.at())
            .unwrap_or(Duration::from_millis(20))
            .min(Duration::from_millis(20));
        h.lp.dispatch(Some(wait), &mut h.app).expect("dispatch");
        h.skip_turn(Duration::from_millis(20));
        let left = h
            .app
            .term_mut()
            .expect("a head")
            .terminal
            .take_blank_request();
        assert_eq!(left, None, "a request left for a later turn");
        blanked = h.memory().dpms == Some(Dpms::Off);
    }
    assert!(blanked, "the next session blanks after dpms-timeout");
}

// While the VT is another's, a program's wake touches no device and its
// timeout is still taken: the VT released while blanked, the program sends
// `CSI 9 ; 2 ]` and `CSI 13 ]`.
#[test]
fn a_program_wakes_nothing_while_the_vt_is_anothers() {
    let (_dir, go, wait) = gate("anothers-go");
    let mut h = program_blank_in_a_minute(
        &[],
        &format!(r"{wait}; printf '\033[9;2]\033[13]sent'; read x"),
    );
    h.shown();
    h.skip_turn(Duration::from_secs(61));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    let (unblanks, sets) = (h.memory().unblanks, h.memory().dpms_sets.len());
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until(BOUND, |a| shows(a, "sent")),
        "the program's sequences taken"
    );
    assert_eq!(h.memory().unblanks, unblanks, "no unblank");
    assert_eq!(h.memory().dpms_sets[sets..], [], "no DPMS set");
    let wake_ups = h.memory().wake_ups;
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.until_memory(BOUND, |m| m.wake_ups == wake_ups + 1
            && m.dpms == Some(Dpms::On)),
        "lit by the acquire"
    );
    // The program's two minutes were taken while the VT was another's.
    h.skip_turn(Duration::from_secs(90));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::On),
        "lit past dpms-timeout's minute"
    );
    h.skip_turn(Duration::from_secs(31));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked at the program's two"
    );
}

#[test]
fn a_programs_timeout_is_in_minutes() {
    let mut h = Harness::with_vt(
        &[
            "--dpms-timeout=60",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            r"printf '\033[9;2]ready'; read x",
        ],
        ScriptedVt::default(),
    );
    assert!(
        h.until(BOUND, |a| shows(a, "ready")),
        "the program's 2 taken"
    );
    h.shown();
    h.skip_turn(Duration::from_secs(90));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::On),
        "two minutes, in place of dpms-timeout's one"
    );
    h.skip_turn(Duration::from_secs(31));
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "blanked at two minutes");
}

#[test]
fn under_no_program_blank_a_program_neither_times_nor_wakes_the_blank() {
    let (_dir, go, wait) = gate("no-program-blank-go");
    let mut h = program_blank_in_a_minute(
        &["--no-program-blank"],
        &format!(r"printf '\033[9;0]ready'; {wait}; printf '\033[13]sent'; read x"),
    );
    assert!(
        h.until(BOUND, |a| shows(a, "ready")),
        "the program's 0 read"
    );
    h.shown();
    h.skip_turn(Duration::from_secs(61));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked after dpms-timeout"
    );
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until(BOUND, |a| shows(a, "readysent")),
        "the program's wake read"
    );
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "still dark");
    assert_eq!(h.memory().unblanks, 0);
    h.type_str("a");
    assert_eq!(h.memory().dpms, Some(Dpms::On), "a key still wakes");
}

/// A seat that blanks after a minute with no input, on the scripted VT,
/// whose session runs `script`; its tests move the seat's clock, as
/// [`blanking`]'s do.
fn blank_in_a_minute(script: &str) -> Harness {
    Harness::with_vt(
        &[
            "--dpms-timeout=60",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            script,
        ],
        ScriptedVt::default(),
    )
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
    let mut h = blank_in_a_minute("read x");
    h.shown();
    h.skip_turn(Duration::from_secs(36));
    for event in [PointerEvent::Sync, PointerEvent::Hide] {
        h.app.input_event(pointer(event), h.at());
    }
    h.skip_turn(Duration::from_secs(18));
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit before the minute");
    // Due a minute after the start; counted from the hide, 96 s.
    h.skip_turn(Duration::from_secs(12));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked a minute after the start, not a minute after the hide"
    );
}

#[test]
fn a_pointer_move_restarts_the_blanks_countdown() {
    let mut h = blank_in_a_minute("read x");
    h.shown();
    h.skip_turn(Duration::from_secs(36));
    h.app.input_event(pointer(over(1, 1, None)), h.at());
    h.skip_turn(Duration::from_secs(30));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::On),
        "lit a minute after the start"
    );
    h.skip_turn(Duration::from_secs(31));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked a minute after the move"
    );
}

// The VT review's step on vtium/vtium#79, in seconds: a program's timeout,
// the pointer's hide on the way, and the program's wake. `CSI 9 ; n ]`
// counts in minutes, so RIS returns the timeout instead, a request on the
// same path.
// covers: ctl.linux-unblank
#[test]
fn a_programs_timeout_blanks_past_the_pointers_hide_and_its_wake_ends_it() {
    // The program goes on at each file the test writes.
    let dir = Scratch::new("past-hide");
    let go = |n: u32| dir.join(format!("go-{n}"));
    let script = format!(
        r"printf '\033[9;0]off'; while [ ! -e {} ]; do sleep 0.05; done; \
          printf '\033creset'; while [ ! -e {} ]; do sleep 0.05; done; \
          printf '\033[13]'; read x",
        go(1).display(),
        go(2).display()
    );
    let mut h = blank_in_a_minute(&script);
    assert!(h.until(BOUND, |a| shows(a, "off")), "the program's 0 taken");
    h.shown();
    std::fs::write(go(1), b"").expect("the go file");
    assert!(h.until(BOUND, |a| shows(a, "reset")), "the RIS taken");
    // The RIS restarts the countdown: due a minute after it; counted from
    // the hide, 96 s after it.
    h.skip_turn(Duration::from_secs(36));
    h.app.input_event(pointer(PointerEvent::Hide), h.at());
    h.skip_turn(Duration::from_secs(30));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "blanked a minute after the RIS, not a minute after the hide"
    );
    let unblanks = h.memory().unblanks;
    std::fs::write(go(2), b"").expect("the go file");
    assert!(
        h.until_memory(BOUND, |m| m.dpms == Some(Dpms::On)),
        "woken by the program"
    );
    assert_eq!(h.memory().unblanks, unblanks + 1);
}

// The VT review's step on vtium/vtium#79 as it ran: `CSI 9 ; 1 ]`, the
// pointer's hide 20 s later, the blank at the minute, and the program's
// wake. The seat's clock skips the minute.
// covers: ctl.linux-blank
#[test]
fn a_programs_minute_blanks_past_the_pointers_hide() {
    // The program wakes the displays once the test has seen the blank.
    let go_dir = Scratch::new("minute-go");
    let go = go_dir.join("go");
    let mut h = blank_in_two(&format!(
        r"printf '\033[9;1]ready'; while [ ! -e {} ]; do sleep 0.05; done; printf '\033[13]'; read x",
        go.display()
    ));
    assert!(
        h.until(BOUND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.starts_with("ready"))),
        "the program's minute taken"
    );
    h.shown();
    h.skip(Duration::from_secs(20));
    h.app.input_event(pointer(PointerEvent::Hide), h.at());
    h.skip_turn(Duration::from_secs(39));
    assert_eq!(h.memory().dpms, Some(Dpms::On), "lit at 59 s");
    h.skip(Duration::from_secs(2));
    assert!(h.until_memory(BOUND, OFF), "blanked at the minute");
    let unblanks = h.memory().unblanks;
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until_memory(BOUND, |m| m.dpms == Some(Dpms::On)),
        "woken by the program"
    );
    assert_eq!(h.memory().unblanks, unblanks + 1);
}

/// The harness's seat clock moves only when the test moves it, so the
/// cursor blinks only then: with blinking on, a frame drawn after the loop
/// has turned for longer than a blink period of real time shows what the
/// first frame showed, and moving the seat's clock a blink period hides
/// the cursor.
#[test]
fn the_cursor_blinks_only_when_the_test_moves_the_seats_clock() {
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", "echo ready; read x"]);
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "ready")));
    let first = h.drawn_now();
    // The loop turns for 0.6 s of real time, past the 0.5 s period.
    h.until(Duration::from_millis(600), |_| false);
    assert!(h.drawn_now() == first, "the cursor blinked on its own");
    h.skip_turn(vtium::blink::TEXT_PERIOD);
    assert!(h.drawn_now() != first, "the cursor blinked with the clock");
    h.skip_turn(vtium::blink::TEXT_PERIOD);
    assert!(h.drawn_now() == first, "and shows again a period later");
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
        h.until_memory(BOUND, |m| m.frames > 0 && m.events.is_empty()),
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
    assert!(h.until(BOUND, |a| a.cells().len() == 2));
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
    h.app.input_event(key(Mods::CTRL, equal), h.at());
    assert_eq!(h.app.grid(), (20, 5), "zoomed in to 16x32 cells");
    assert_eq!(h.app.cells(), [(16, 32)]);
    h.app.input_event(key(Mods::CTRL, minus), h.at());
    assert_eq!(h.app.grid(), (40, 10), "and back");
    // Logo+= turns the display a quarter: 160 across, 320 down.
    h.app.input_event(key(Mods::LOGO, equal), h.at());
    assert_eq!(h.app.grid(), (20, 20));
    h.app.input_event(key(Mods::LOGO, minus), h.at());
    assert_eq!(h.app.grid(), (40, 10));
    // The keys reach no program.
    assert!(h.until(BOUND, |a| a.screen_text().iter().all(|l| l.is_empty())));
}

/// A device whose flips take 100 ms on the seat's clock and report no
/// completion, as a lost flip event does until the platform's flip wait
/// ends it.
struct SlowFlip {
    memory: Memory,
    swapped: Option<Instant>,
    seat: SeatClock,
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // A display lends a token only while it is ready for a frame.
        if !self.ready_for_frame(id) {
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.swapped = Some(self.seat.now());
        self.memory.swap(token, damage)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.swapped
            .is_some_and(|t| self.seat.now() < t + Duration::from_millis(100))
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        !self.is_flipping(id) && self.memory.ready_for_frame(id)
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
    let seat = SeatClock::new();
    let slow = SlowFlip {
        memory: Memory::default(),
        swapped: None,
        seat: seat.clone(),
    };
    let mut h = Harness::build_on(
        seat,
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
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    h.settled();
    h.type_str("\r");
    // 1.5 s on the seat's clock, which moves by each wait the loop is
    // told, at most 5 ms. A loop that spins is told to wake at once on
    // every turn, so the clock never moves, and the loop turns until the
    // turn count or the real-time bound stops it.
    let (start, end) = (h.at(), now() + BOUND);
    let (mut turns, mut zero) = (0_u32, 0_u32);
    while h.at() < start + Duration::from_millis(1500) && turns < 1000 && now() < end {
        if h.sleep_turn(Duration::from_millis(5)) == Some(Duration::ZERO) {
            zero += 1;
        }
        turns += 1;
    }
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.contains("xxxx"))
    }));
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
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "ready")));
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
        h.app.input_event(event, h.at());
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
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.ends_with("ready"))
    }));
    h.app
        .input_event(pointer(PointerEvent::Wheel(i32::MAX)), h.at());
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| a
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
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "line39")));
    h.app
        .input_event(pointer(PointerEvent::Wheel(i32::MAX)), h.at());
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back(),
        3
    );
    h.app.input_event(pointer(PointerEvent::Wheel(-1)), h.at());
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
    assert!(h.until(BOUND, |a| shows(a, "second one")));
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
        h.until(BOUND, |a| shows(a, "got:first line|second one")),
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
    assert!(h.until(BOUND, |a| shows(a, "second line here")));
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
    h.app.input_event(key(Mods::SHIFT, 0xff63), h.at());
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
        h.until(BOUND, |a| shows(a, "got:beta|second line here")),
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
    assert!(h.until(BOUND, |a| shows(a, "ready")));
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
        h.until(BOUND, |a| shows(a, "E[200~pick meE[201~E[<0;7;1ME[<0;7;1m")),
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
    assert!(h.until(BOUND, |a| {
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
        h.until(BOUND, |a| a
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
    assert!(h.until(BOUND, |a| shows(a, "ready")));
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
        h.until(BOUND, |a| shows(a, "E[<0;1;1ME[<36;5;1ME[<0;5;1m")),
        "{:?}",
        h.app.screen_text()
    );
}

/// What the shown screen's frame says of links: the hovered link's URI,
/// the columns of row 0 that draw it hovered, and the overlay's row and
/// text.
fn links(app: &App) -> (Option<String>, Vec<usize>, Option<(usize, String)>) {
    let terminal = &app.term().expect("a head").terminal;
    let screen = terminal.screen();
    let frame = screen.frame(terminal.palette(), vtium_term::Age::NEVER);
    let uri = screen
        .hovered_link()
        .and_then(|l| screen.link_uri(l))
        .map(str::to_owned);
    let hovered = (0..frame.cols())
        .filter(|&c| frame.cell(0, c).is_some_and(|fc| fc.hovered))
        .collect();
    let overlay = frame.overlay().map(|o| (o.row, o.uri.to_owned()));
    (uri, hovered, overlay)
}

/// The text of view row `row`, what the display shows there.
fn view_row(app: &App, row: usize) -> String {
    let terminal = &app.term().expect("a head").terminal;
    let frame = terminal
        .screen()
        .frame(terminal.palette(), vtium_term::Age::NEVER);
    frame.line(row).map_or_else(String::new, |line| {
        line.cells()
            .iter()
            .map(|c| match c.content() {
                vtium_term::Content::Char(ch) => ch,
                _ => ' ',
            })
            .collect::<String>()
            .trim_end()
            .to_owned()
    })
}

const LINKED: &str = "printf 'see \\033]8;;file://pixie/home/a.md\\007link\\033]8;;\\007 here\\n'";

/// ADR-1002, "What a link does": the pointer over a link hovers all of
/// it, Ctrl shows its URI on the bottom row, and the pointer leaving or
/// hiding ends both. Ctrl pressed with the pointer still shows the URI
/// too.
// covers: osc.8
#[test]
fn the_pointer_over_a_link_hovers_it_and_ctrl_shows_its_uri() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        &format!("{LINKED}; read x"),
    ]);
    assert!(h.until(BOUND, |a| shows(a, "see link here")));
    let uri = || Some("file://pixie/home/a.md".to_owned());
    assert_eq!(links(&h.app), (None, vec![], None));
    h.point([over(5, 0, None)], Mods::empty());
    assert_eq!(links(&h.app), (uri(), vec![4, 5, 6, 7], None));
    h.point([over(4, 0, None)], Mods::CTRL);
    assert_eq!(
        links(&h.app),
        (
            uri(),
            vec![4, 5, 6, 7],
            Some((9, "file://pixie/home/a.md".into()))
        )
    );
    // Off the link, the hover and the overlay end.
    h.point([over(10, 0, None)], Mods::CTRL);
    assert_eq!(links(&h.app), (None, vec![], None));
    // Ctrl pressed with the pointer still over the link shows the URI.
    h.point([over(6, 0, None)], Mods::empty());
    h.app.input_event(key(Mods::CTRL, 0xffe3), h.at());
    assert_eq!(links(&h.app).2, Some((9, "file://pixie/home/a.md".into())));
    // The pointer hiding hovers nothing.
    h.point([PointerEvent::Hide], Mods::empty());
    assert_eq!(links(&h.app), (None, vec![], None));
}

/// ADR-1002, "What a link does": Ctrl+click on a link copies its URI
/// into the selection buffer, replacing what it held, and clears the
/// selection; nothing reaches the program but the paste the middle
/// button then makes.
// covers: osc.8
#[test]
fn ctrl_click_on_a_link_copies_its_uri_and_the_middle_button_pastes_it() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        &format!("{LINKED}; read x; echo \"got:$x\"; read y"),
    ]);
    assert!(h.until(BOUND, |a| shows(a, "see link here")));
    let none = Mods::empty();
    h.point(
        [
            over(1, 0, None),
            press(Button::Left, false),
            release(Button::Left),
            press(Button::Left, true),
            release(Button::Left),
        ],
        none,
    );
    assert_eq!(h.app.selection(), Some(&b"see"[..]));
    h.point(
        [
            over(5, 0, None),
            press(Button::Left, false),
            release(Button::Left),
        ],
        Mods::CTRL,
    );
    assert_eq!(h.app.selection(), Some(&b"file://pixie/home/a.md"[..]));
    let screen = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .has_selection()
    };
    assert!(!screen(&h));
    // Ctrl+click off a link selects as a click does.
    h.point(
        [
            over(9, 0, None),
            press(Button::Left, false),
            over(12, 0, Some(Button::Left)),
            release(Button::Left),
        ],
        Mods::CTRL,
    );
    assert_eq!(h.app.selection(), Some(&b"here"[..]));
    h.point(
        [
            over(5, 0, None),
            press(Button::Left, false),
            release(Button::Left),
        ],
        Mods::CTRL,
    );
    h.point(
        [press(Button::Middle, false), release(Button::Middle)],
        none,
    );
    h.type_str("\r");
    assert!(
        h.until(BOUND, |a| shows(a, "got:file://pixie/home/a.md")),
        "{:?}",
        h.app.screen_text()
    );
}

/// ADR-1002, "What a link does": while a program tracks the pointer,
/// Ctrl+click is reported, and Ctrl+Shift+click copies the link.
// covers: osc.8, input.shift-override
#[test]
fn ctrl_shift_click_copies_a_link_while_the_program_tracks_the_pointer() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        &format!(
            "{LINKED}; stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
             awk 'BEGIN{{RS=\"q\"}}{{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}}'; read x"
        ),
    ]);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    let click = [
        over(5, 0, None),
        press(Button::Left, false),
        release(Button::Left),
    ];
    h.point(click, Mods::CTRL);
    assert_eq!(h.app.selection(), None);
    h.point(click, Mods::CTRL.union(Mods::SHIFT));
    assert_eq!(h.app.selection(), Some(&b"file://pixie/home/a.md"[..]));
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "E[<16;6;1ME[<16;6;1m")),
        "{:?}",
        h.app.screen_text()
    );
}

/// Mode 1007, alternate scroll: on the alternate screen, with no
/// tracking, a wheel step goes to the program as three cursor keys, one a
/// line; with the mode reset it sends nothing.
// covers: DEC-1007
#[test]
fn the_wheel_sends_cursor_keys_on_the_alternate_screen_under_mode_1007() {
    for (reset, want) in [("", "[E[AE[AE[AE[BE[BE[B]"), ("\\033[?1007l", "[]")] {
        let mut h = Harness::new(&[
            "--login",
            "--",
            "/bin/sh",
            "-c",
            &format!(
                "stty raw -echo; printf '{reset}\\033[?1049hready'; \
                 awk 'BEGIN{{RS=\"q\"}}{{gsub(/\\033/,\"E\");printf \"\\r\\n[%s]\\r\\n\",$0;exit}}'; read x"
            ),
        ]);
        assert!(h.until(BOUND, |a| shows(a, "ready")));
        h.point(
            [PointerEvent::Wheel(1), PointerEvent::Wheel(-1)],
            Mods::empty(),
        );
        h.type_str("q");
        assert!(
            h.until(BOUND, |a| shows(a, want)),
            "{reset:?}: {:?}",
            h.app.screen_text()
        );
    }
}

/// #203: natural scrolling reverses the wheel, as the C reference's
/// `natural-scrolling` did (`src/terminal.c:1474-1476, 1646` at kmscon
/// origin/main): a step up scrolls the view down and a step down scrolls
/// it back, the cursor keys mode 1007 sends on the alternate screen
/// reverse, and a program tracking the pointer reads the other wheel
/// button. Ctrl's zoom keeps the wheel's own direction.
#[test]
fn natural_scrolling_reverses_the_wheel_but_not_its_zoom() {
    let back = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back()
    };
    // the scrollback
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "i=0; while [ $i -lt 40 ]; do echo line$i; i=$((i+1)); done; echo ready; read x",
    ]);
    h.app.set_natural_scrolling(true);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    h.point([PointerEvent::Wheel(1)], Mods::empty());
    assert_eq!(back(&h), 0, "a step up scrolls down, at the bottom already");
    h.point([PointerEvent::Wheel(-1)], Mods::empty());
    assert_eq!(back(&h), 3, "a step down scrolls back");
    h.point([PointerEvent::Wheel(1)], Mods::empty());
    assert_eq!(back(&h), 0);
    // Ctrl's zoom
    let before = h.app.cells();
    h.point([PointerEvent::Wheel(1)], Mods::CTRL);
    assert!(
        h.app.cells().first().map(|c| c.1) > before.first().map(|c| c.1),
        "a step up with Ctrl zooms in: {before:?} to {:?}",
        h.app.cells()
    );
    // mode 1007's cursor keys
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf '\\033[?1049hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n[%s]\\r\\n\",$0;exit}'; read x",
    ]);
    h.app.set_natural_scrolling(true);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    h.point([PointerEvent::Wheel(1)], Mods::empty());
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "[E[BE[BE[B]")),
        "a step up sends the down key: {:?}",
        h.app.screen_text()
    );
    // the report to a program tracking the pointer
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n[%s]\\r\\n\",$0;exit}'; read x",
    ]);
    h.app.set_natural_scrolling(true);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    h.point([over(0, 0, None), PointerEvent::Wheel(1)], Mods::empty());
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "[E[<65;1;1M]")),
        "a step up reports the wheel's down button: {:?}",
        h.app.screen_text()
    );
}

/// XTSHIFTESCAPE 1: a program that asks for Shift gets it in its reports,
/// so Shift neither selects nor scrolls back while it tracks the pointer.
// covers: input.xtshiftescape
#[test]
fn a_program_that_asks_for_shift_gets_it_in_its_reports() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "i=0; while [ $i -lt 40 ]; do echo line$i; i=$((i+1)); done; \
         stty raw -echo; printf 'pick me\\r\\n\\033[?1000h\\033[?1006h\\033[>1sready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    let row = 8;
    h.point(
        [
            over(0, row, None),
            press(Button::Left, false),
            release(Button::Left),
            PointerEvent::Wheel(1),
        ],
        Mods::SHIFT,
    );
    assert_eq!(h.app.selection(), None);
    assert_eq!(
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back(),
        0
    );
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "E[<4;1;9ME[<4;1;9mE[<68;1;9M")),
        "{:?}",
        h.app.screen_text()
    );
}

/// `bell`: a BEL rings the VT's bell, once a turn however many came,
/// while mode 1042 lets it through; with mode 1042 reset, or `bell` off,
/// its default, nothing rings.
// covers: ctl.bel, DEC-1042
#[test]
fn bell_rings_the_vt_at_a_bel_while_mode_1042_lets_it_through() {
    let script = "printf '\\a\\a\\aone\\n'; read x; printf '\\033[?1042l\\atwo\\n'; read y";
    for (args, want) in [(&["--bell"][..], 1), (&[][..], 0)] {
        let mut all: Vec<&str> = args.to_vec();
        all.extend(["--login", "--", "/bin/sh", "-c", script]);
        let mut h = Harness::with_vt(&all, ScriptedVt::default());
        assert!(h.until(BOUND, |a| shows(a, "one")));
        assert_eq!(h.scripted().bells, want, "{args:?}");
        h.type_str("\r");
        assert!(h.until(BOUND, |a| shows(a, "two")));
        assert_eq!(h.scripted().bells, want, "{args:?}: mode 1042 reset");
    }
}

/// ADR-1002, "What a link does": a rotation puts a cell under a still
/// pointer that was past the grid, and the hover follows it as at a zoom.
/// On the 320 by 160 display the pointer is 200 pixels down, below the
/// grid's 10 rows of 16; turned a quarter the grid is 20 rows, the line of
/// 240 A's and 8 B's rewraps to 20 columns, and the second link starts at
/// row 12, under the pointer.
// covers: osc.8
#[test]
fn a_rotation_moves_the_hover_to_the_link_now_under_the_pointer() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf '\\033]8;;https://a.example/\\007'; printf '%0240d' 0 | tr 0 A; \
         printf '\\033]8;;https://b.example/\\007BBBBBBBB\\033]8;;\\007\\n'; read x",
    ]);
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.ends_with("BBBBBBBB"))
    }));
    let still = PointerEvent::Moved {
        x: 4,
        y: 200,
        held: None,
    };
    h.point([still], Mods::empty());
    assert_eq!(links(&h.app).0, None, "below the grid, over no cell");
    h.app.input_event(key(Mods::LOGO, 0x3d), h.at());
    assert_eq!(h.app.grid(), (20, 20), "turned a quarter");
    assert_eq!(
        links(&h.app).0.as_deref(),
        Some("https://b.example/"),
        "the second link, rewrapped under the pointer"
    );
}

/// ADR-1002, "What a link does": a zoom puts another cell under a still
/// pointer, and the hover follows it, so the URI the overlay shows is the
/// URI Ctrl+click copies. The pointer rests on the second of two links,
/// 8 by 16 cells; Ctrl+wheel zooms to 16 by 32, which puts the first under
/// it.
// covers: osc.8
#[test]
fn a_zoom_moves_the_hover_to_the_link_now_under_the_pointer() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf '\\033]8;;https://a.example/\\007AAAAAAAA\\033]8;;https://b.example/\\007BBBBBBBB\\033]8;;\\007\\n'; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "AAAAAAAABBBBBBBB")));
    let still = PointerEvent::Moved {
        x: 100,
        y: 8,
        held: None,
    };
    h.point([still], Mods::CTRL);
    assert_eq!(links(&h.app).2, Some((9, "https://b.example/".into())));
    h.point([PointerEvent::Wheel(1)], Mods::CTRL);
    assert_eq!(h.app.cells(), [(16, 32)], "zoomed in");
    assert_eq!(
        links(&h.app).2,
        Some((4, "https://a.example/".into())),
        "column 6 of the zoomed grid"
    );
    h.point(
        [press(Button::Left, false), release(Button::Left)],
        Mods::CTRL,
    );
    assert_eq!(h.app.selection(), Some(&b"https://a.example/"[..]));
}

/// `pointer-hide-while-typing`: the key that hides the pointer ends its
/// hover, since a hidden pointer is over no cell.
// covers: osc.8
#[test]
fn typing_that_hides_the_pointer_ends_the_hover() {
    let mut h = Harness::new(&[
        "--pointer-hide-while-typing",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        &format!("{LINKED}; read x"),
    ]);
    assert!(h.until(BOUND, |a| shows(a, "see link here")));
    h.point([over(5, 0, None)], Mods::empty());
    assert_eq!(links(&h.app).0, Some("file://pixie/home/a.md".to_owned()));
    h.type_str("a");
    assert_eq!(links(&h.app), (None, vec![], None));
}

/// ADR-1002, "What the marks are for": while a program tracks the
/// pointer, Ctrl+Shift+right-click copies a command's output and reports
/// nothing; Ctrl+right-click alone is reported.
// covers: osc.133, input.shift-override
#[test]
fn ctrl_shift_right_click_copies_a_commands_output_while_the_program_tracks() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf '\\033]133;A\\007$ \\033]133;B\\007ls\\n\\033]133;C\\007one\\ntwo\\n\\033]133;A\\007$ '; \
         stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n[%s]\\r\\n\",$0;exit}'; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "$ ready")));
    let click = [
        over(1, 2, None),
        press(Button::Right, false),
        release(Button::Right),
    ];
    h.point(click, Mods::CTRL.union(Mods::SHIFT));
    assert_eq!(h.app.selection(), Some(&b"one\ntwo\n"[..]));
    h.point(click, Mods::CTRL);
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "[E[<18;2;3ME[<18;2;3m]")),
        "{:?}",
        h.app.screen_text()
    );
}

/// ADR-1002, "What the marks are for": Ctrl+right-click on a line of a
/// command's output selects that output as whole lines and copies it; the
/// release keeps the selection, and a right click without Ctrl clears it.
/// On the prompt's line it selects nothing.
// covers: osc.133
#[test]
fn ctrl_right_click_copies_the_output_of_the_command_under_the_pointer() {
    let mut h = Harness::new(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf '\\033]133;A\\007$ \\033]133;B\\007ls\\n\\033]133;C\\007one\\ntwo\\n\\033]133;A\\007$ '; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "two")));
    let selected = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .has_selection()
    };
    h.point(
        [
            over(1, 2, None),
            press(Button::Right, false),
            release(Button::Right),
        ],
        Mods::CTRL,
    );
    assert_eq!(h.app.selection(), Some(&b"one\ntwo\n"[..]));
    assert!(selected(&h));
    h.point(
        [press(Button::Right, false), release(Button::Right)],
        Mods::empty(),
    );
    assert!(!selected(&h));
    h.point(
        [
            over(3, 0, None),
            press(Button::Right, false),
            release(Button::Right),
        ],
        Mods::CTRL,
    );
    assert!(!selected(&h));
    assert_eq!(h.app.selection(), Some(&b"one\ntwo\n"[..]));
}

/// ADR-1002, "What the marks are for": Ctrl+Shift+Up lands each marked
/// prompt in the scrollback at the view's top row, one a press, and
/// Ctrl+Shift+Down goes back down and past the newest to the live
/// screen; neither reaches the program on the main screen. On the
/// alternate screen both reach the program.
// covers: osc.133
#[test]
fn ctrl_shift_up_and_down_jump_between_the_marked_prompts() {
    let mut h = Harness::new(&[
        "--font-size=16",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; i=0; while [ $i -lt 3 ]; do printf '\\033]133;A\\007$ p%d\\r\\n' $i; j=0; \
         while [ $j -lt 12 ]; do printf 'out%d.%d\\r\\n' $i $j; j=$((j+1)); done; i=$((i+1)); done; \
         printf '\\033]133;A\\007$ last'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\nmain:[%s]\\r\\n\",$0;exit}'; \
         head -c 1 >/dev/null; printf '\\033[?1049halt'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\n%s\\r\\n\",$0;exit}'; read y",
    ]);
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "$ last")));
    let back = |h: &Harness| {
        h.app
            .term()
            .expect("a head")
            .terminal
            .screen()
            .scrolled_back()
    };
    let jump = |h: &mut Harness, up: bool| {
        let keysym = if up { 0xff52 } else { 0xff54 };
        h.app
            .input_event(key(Mods::CTRL.union(Mods::SHIFT), keysym), h.at());
    };
    assert_eq!(back(&h), 0);
    for (want, top) in [(4, "$ p2"), (17, "$ p1"), (30, "$ p0"), (30, "$ p0")] {
        jump(&mut h, true);
        assert_eq!((back(&h), view_row(&h.app, 0)), (want, top.to_owned()));
    }
    for (want, top) in [(17, "$ p1"), (4, "$ p2"), (0, "out2.3")] {
        jump(&mut h, false);
        assert_eq!((back(&h), view_row(&h.app, 0)), (want, top.to_owned()));
    }
    // The program on the main screen read nothing before the `q`.
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.starts_with("main:"))),
        "{:?}",
        h.app.screen_text()
    );
    assert!(shows(&h.app, "main:[]"), "{:?}", h.app.screen_text());
    h.type_str("x");
    assert!(
        h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "alt")),
        "{:?}",
        h.app.screen_text()
    );
    jump(&mut h, true);
    jump(&mut h, false);
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "E[1;6AE[1;6B")),
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
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    assert_eq!(h.app.selection(), Some(&b"pasted"[..]));
    h.app.input_event(alt_p, h.at());
    h.app.input_event(shift_insert.clone(), h.at());
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "E[200~pastedE[201~E[2~")),
        "{:?}",
        h.app.screen_text()
    );
    let mut h = Harness::new(&["--grab-paste=", "--login", "--", "/bin/sh", "-c", script]);
    assert!(h.until(BOUND, |a| shows(a, "ready")));
    h.app.input_event(shift_insert, h.at());
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| shows(a, "E[2~")),
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
        let mut args = vec![
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "stty -echo; read x",
        ];
        if hides {
            args.insert(0, "--pointer-hide-while-typing");
        }
        let mut h = Harness::new(&args);
        assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
        // The arrow's tip is at the pointer, its fill a pixel below it.
        h.point([over(10, 5, None)], Mods::empty());
        let (x, y) = (10 * 8 + 4, 5 * 16 + 8 + 1);
        assert!(h.until_memory(BOUND, |m| m.white_at(x, y)), "shown");
        // A frame drawn after the keys says whether the pointer shows.
        let after_keys = |h: &mut Harness, keys: &mut dyn FnMut(&mut Harness)| {
            keys(h);
            h.drawn_now();
            !h.memory().white_at(x, y)
        };
        let hidden = after_keys(&mut h, &mut |h| h.type_str("a"));
        assert_eq!(hidden, hides, "typing hides it: {hides}");
        // A key the program is not sent, a grab's or a modifier's alone,
        // leaves it.
        h.point([over(10, 5, None)], Mods::empty());
        assert!(h.until_memory(BOUND, |m| m.white_at(x, y)), "shown again");
        let hidden = after_keys(&mut h, &mut |h| {
            h.app.input_event(key(Mods::SHIFT, 0xff52), h.at());
            h.app.input_event(key(Mods::empty(), 0xffe1), h.at());
        });
        assert!(!hidden, "a grab and a modifier leave it");
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
        assert!(h.until(BOUND, |a| shows(a, "ready")));
        h.app.input_event(ctrl_shift_v.clone(), h.at());
        h.type_str("q");
        let want = if pasted { "[E[200~pastedE[201~]" } else { "[]" };
        assert!(
            h.until(BOUND, |a| shows(a, want)),
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
    assert!(h.until(BOUND, |a| shows(a, "alpha beta gamma")));
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
    let (mut h, released) = Harness::counting(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'first line\\nsecond one\\n'; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "second one")));
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
    h.app.input_event(switch.clone(), h.at());
    assert!(h.until_memory(BOUND, |m| !m.awake), "asleep");
    // The switch back finds the VT let go only once vtium acknowledged
    // the release, which waits for the display to sleep.
    assert!(h.until(BOUND, |_| released.get() == 1), "released");
    assert!(!h.app.vt.is_active());
    h.app.input_event(switch, h.at());
    assert!(h.until(BOUND, |a| a.vt.is_active()), "acquired");
    h.point([over(9, 1, Some(Button::Left))], none);
    assert_eq!(selected(&h).as_deref(), Some("sec"));
}

/// vtium/vtium#229's review: with the fake VT released and its pointers
/// still open, vtium delivers no pointer action to the console, so a
/// program tracking the mouse gets no report of a move, a click or the
/// wheel from a pointer device, as the C reference's pointer callback
/// returned while its terminal was not awake (`src/terminal.c:1610`).
/// Taken back, the VT's console gets the next key alone.
#[test]
fn a_released_fake_vt_reports_no_pointer_action_to_the_program() {
    let (mut h, released) = Harness::counting(&[
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty raw -echo; printf '\\033[?1000h\\033[?1006hready'; \
         awk 'BEGIN{RS=\"q\"}{gsub(/\\033/,\"E\");printf \"\\r\\ngot:%s.\\r\\n\",$0;exit}'; read x",
    ]);
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.ends_with("ready"))
    }));
    let switch = key(Mods::CTRL.union(Mods::LOGO), 0xffc9);
    h.app.input_event(switch.clone(), h.at());
    assert!(h.until(BOUND, |_| released.get() == 1), "released");
    assert!(!h.app.vt.is_active());
    let mouse = Path::new("/dev/input/event9");
    for event in [
        over(2, 0, None),
        press(Button::Left, false),
        release(Button::Left),
        PointerEvent::Wheel(1),
        PointerEvent::Sync,
    ] {
        h.app
            .device_input(mouse, InputEvent::Pointer(event), h.at());
    }
    h.app.input_event(switch, h.at());
    assert!(h.until(BOUND, |a| a.vt.is_active()), "taken back");
    // The acquire wakes the display and the terminals.
    assert!(h.until_memory(BOUND, |m| m.awake), "awake");
    h.type_str("q");
    assert!(
        h.until(BOUND, |a| a
            .screen_text()
            .iter()
            .any(|l| l.starts_with("got:"))),
        "{:?}",
        h.app.screen_text()
    );
    assert!(shows(&h.app, "got:."), "{:?}", h.app.screen_text());
}

/// The fix branch's re-review: a released fake VT keeps its pointers open,
/// so the mouse moving in the other session keeps arming the pointer's
/// hide, and that hide leaves vtium's selection for the user's return, as
/// the C reference's pointer callback dropped the hide while its session
/// was not in the foreground (`src/terminal.c:1609-1611, 1655-1659` at
/// kmscon origin/main). On vtium's own VT, blanked, the hide still clears
/// the selection.
#[test]
fn a_pointer_hide_on_a_released_vt_keeps_the_selection() {
    let (mut h, released) = Harness::counting(&[
        "--dpms-timeout=60",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "printf 'first line\\nsecond one\\n'; read x",
    ]);
    assert!(h.until(BOUND, |a| shows(a, "second one")));
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
            over(0, 1, None),
            press(Button::Left, false),
            over(2, 1, Some(Button::Left)),
            release(Button::Left),
        ],
        Mods::empty(),
    );
    assert_eq!(selected(&h).as_deref(), Some("sec"));
    // The arrow's tip is at the pointer, its fill a pixel below it.
    let (x, y) = (2 * 8 + 4, 16 + 8 + 1);
    assert!(
        h.until_memory(BOUND, |m| m.white_at(x, y)),
        "the arrow shown"
    );
    let switch = key(Mods::CTRL.union(Mods::LOGO), 0xffc9);
    h.app.input_event(switch.clone(), h.at());
    assert!(h.until(BOUND, |_| released.get() == 1), "released");
    assert!(!h.app.vt.is_active());
    let mouse = Path::new("/dev/input/event9");
    h.app
        .device_input(mouse, InputEvent::Pointer(PointerEvent::Hide), h.at());
    assert_eq!(selected(&h).as_deref(), Some("sec"), "kept while released");
    h.app.input_event(switch, h.at());
    assert!(h.until(BOUND, |a| a.vt.is_active()), "taken back");
    assert!(h.until_memory(BOUND, |m| m.awake), "awake");
    assert_eq!(selected(&h).as_deref(), Some("sec"), "kept at the return");
    // The pointer moved in the other session, so vtium draws no arrow at
    // its old place on the return.
    h.drawn_now();
    assert!(!h.memory().white_at(x, y), "no arrow at the return");
    // On vtium's own VT, blanked, the hide clears the selection.
    h.shown();
    h.skip_turn(Duration::from_secs(61));
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "blanked");
    assert!(h.app.vt.is_active());
    h.app
        .device_input(mouse, InputEvent::Pointer(PointerEvent::Hide), h.at());
    assert_eq!(selected(&h), None, "cleared on vtium's own VT");
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
    assert!(h.until(BOUND, |a| shows(a, "ready")));
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
    h.app.input_event(shift_insert(false), h.at());
    h.app.input_event(shift_insert(true), h.at());
    h.app.input_event(shift_insert(true), h.at());
    h.point([press(Button::Middle, false)], Mods::empty());
    h.type_str("X");
    h.point([release(Button::Middle)], Mods::empty());
    h.type_str("q");
    let paste = "E[200~a [201~b cRdE[201~";
    assert!(
        h.until(BOUND, |a| shows(a, &format!("{paste}X{paste}"))),
        "{:?}",
        h.app.screen_text()
    );
}

/// The program changing its mouse tracking clears the selection.
#[test]
fn a_change_of_mouse_tracking_clears_the_selection() {
    let go_dir = Scratch::new("boot-tracking");
    let go = go_dir.join("go");
    let script = format!(
        "printf 'text\\n'; while [ ! -e '{}' ]; do sleep 0.02; done; \
         printf '\\033[?1000hon\\n'; read x",
        go.display()
    );
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", &script]);
    assert!(h.until(BOUND, |a| shows(a, "text")));
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
    let cleared = h.until(BOUND, |a| {
        shows(a, "on") && !a.term().expect("a head").terminal.screen().has_selection()
    });
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

/// A burst of output that leaves a frame owed to coalescing is drawn when
/// the wait the loop is told runs out. The loop here moves the seat's
/// clock by each wake-up it is told and by nothing else, as `main`'s loop
/// does when nothing else wakes it, so a loop told no wake-up for the
/// owed frame never draws it.
#[test]
fn a_frame_owed_to_coalescing_is_drawn_at_the_wake_up_the_loop_is_told() {
    let (_dir, go, wait) = gate("owed-frame");
    let script = format!("{wait}; head -c 20000 /dev/zero | tr '\\0' x; echo; echo done; read y");
    let mut h = Harness::new(&[
        "--no-blink",
        "--dpms-timeout=0",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        &script,
    ]);
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    h.shown();
    h.settled();
    let before = h.memory().frames;
    std::fs::write(&go, b"").expect("the go file");
    // The burst is read with no turn between its reads, so the one turn
    // after it finds 20,000 bytes parsed and owes its frame to coalescing.
    let end = now() + BOUND;
    while !h.app.screen_text().iter().any(|l| l == "done") {
        assert!(now() < end, "the burst read");
        h.lp.dispatch(Some(Duration::from_millis(20)), &mut h.app)
            .expect("dispatch");
    }
    h.app.turn(h.at());
    h.settled();
    assert_eq!(h.memory().frames, before, "the frame owed, not drawn");
    let (mut turns, mut moved) = (0_u32, Duration::ZERO);
    while h.memory().frames == before {
        assert!(turns < 4, "drawn within a few turns");
        let wait = h
            .app
            .next_wake(h.at())
            .expect("a wake-up while a frame is owed");
        moved += wait;
        h.skip_turn(wait);
        h.settled();
        turns += 1;
    }
    assert!(
        moved <= Duration::from_millis(50),
        "drawn a refresh on at most, not {moved:?} on"
    );
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
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    h.shown();
    h.settled();
    let before = h.memory().frames;
    let start = h.at();
    h.type_str("\r");
    // The seat's clock moves by each wait the loop is told, at most 10 ms.
    let end = now() + Duration::from_secs(30);
    while !h.app.screen_text().iter().any(|l| l == "done") {
        assert!(now() < end, "the write ended");
        h.sleep_turn(Duration::from_millis(10));
    }
    let elapsed = h.at() - start;
    let frames = h.memory().frames - before;
    // A frame each 50 ms of the seat's clock, less a fifth for the
    // timing, and a few for the key and the end; drawn each turn, it is a
    // frame each burst.
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // A display lends a token only while it is ready for a frame.
        if !self.ready_for_frame(id) {
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(token, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        self.memory.is_flipping(id)
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        self.memory.ready_for_frame(id)
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
        "--no-blink",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        "stty -echo; read x",
    ]);
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
    // The pixels of a frame drawn after the move.
    let frame = |h: &mut Harness, x: i32, y: i32| {
        h.point([PointerEvent::Moved { x, y, held: None }], Mods::empty());
        h.drawn_now()
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // A display lends a token only while it is ready for a frame.
        if !self.ready_for_frame(id) {
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(token, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        self.memory.is_flipping(id)
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        self.memory.ready_for_frame(id)
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
        assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
        h.point([over(10, 5, None)], Mods::empty());
        let (x, y) = (10 * 8 + 4, 5 * 16 + 8);
        // A frame drawn after the move shows the pointer one way or the
        // other.
        h.drawn_now();
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
    assert!(h.until(BOUND, |a| a.cells().len() == 2));
    h.scripted().events.push(VtEvent::Release);
    // The other device asleep, its sleep reported and taken.
    let asleep = |h: &mut Harness| {
        h.app
            .videos
            .get_mut(Path::new("/dev/mem1"))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .is_some_and(|m| m.sleeps == 1 && !m.asleep_owed && m.events.is_empty())
    };
    assert!(h.eventually(BOUND, asleep), "the other device sleeps");
    assert!(
        h.app.vt.is_active(),
        "acknowledged before every device slept"
    );
    h.late().held = None;
    assert!(
        h.until(BOUND, |a| !a.vt.is_active()),
        "acknowledged once the last device slept"
    );
}

/// The next session starts from a hard reset, drawn at once: the screen
/// the last session left is not shown until the new one writes.
#[test]
fn a_restart_draws_the_reset_screen() {
    let flag_dir = Scratch::new("restart");
    let flag = flag_dir.join("flag");
    let go_dir = Scratch::new("restart-go");
    let go = go_dir.join("go");
    // The first session exits once the test saw it drawn.
    let script = format!(
        "if [ -e {f} ]; then exec sleep 30; fi; : > {f}; printf 'first'; \
         while [ ! -e {g} ]; do sleep 0.05; done",
        f = flag.display(),
        g = go.display()
    );
    // "irst", in the first row's cells 1 to 4, past the cursor's cell.
    let irst = |m: &Memory| {
        (0..16).any(|y| {
            let row = (y * W * 4) as usize;
            m.pixels[row + 8 * 4..row + 40 * 4].iter().any(|&b| b != 0)
        })
    };
    let mut h = Harness::new(&["--no-blink", "--login", "--", "/bin/sh", "-c", &script]);
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "first")));
    assert!(h.until_memory(BOUND, irst), "the first session drawn");
    std::fs::write(&go, b"").expect("the go file");
    assert!(h.until(BOUND, |a| a.screen_text().iter().all(|l| l.is_empty())));
    assert!(
        h.until_memory(BOUND, |m| !irst(m)),
        "the reset screen is drawn"
    );
    assert!(!h.app.term().expect("a head").unparsed);
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // A display lends a token only while it is ready for a frame.
        if !self.ready_for_frame(id) {
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(token, damage)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.flipping
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        !self.is_flipping(id) && self.memory.ready_for_frame(id)
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
    assert!(h.until(BOUND, |a| a.cells().len() == 2));
    h.settled();
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
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "got")));
    // The other display shows "got", past the cursor's cell, so it has
    // its frame of the output and asks for no other.
    assert!(
        h.eventually(BOUND, |h| other(h).inked(1, 1..3)),
        "the other display drawn"
    );
    h.settled();
    let (frames, lent) = (gated(&mut h).memory.frames, other(&mut h).ages.len());
    let g = gated(&mut h);
    g.flipping = false;
    g.memory.events.push(Event::Flipped(ID));
    assert!(
        h.until_memory(BOUND, |m| m.frames > frames),
        "the waiting frame drawn"
    );
    assert_eq!(
        other(&mut h).ages.len(),
        lent,
        "the other display left alone"
    );
}

/// A device that lends no token while the test withholds it, and reports
/// nothing when it lends again, as a DRM display lends none while a
/// modeset waits for another display's flip.
struct Withhold {
    memory: Memory,
    withholding: bool,
    /// The tokens asked for while withheld, shared with the test so a
    /// wait can read it.
    refused: Rc<Cell<u32>>,
}

impl Video for Withhold {
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if self.withholding {
            self.refused.set(self.refused.get() + 1);
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        self.memory.swap(token, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        self.memory.is_flipping(id)
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        !self.withholding && self.memory.ready_for_frame(id)
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

/// The seat on a device that withholds its token, once the session
/// printed `got` while the device withheld its token and a frame of it
/// found no token. Each wait is on the state it needs, the first frame
/// shown and then a token refused, however long the render thread takes;
/// the bound only fails a test that would otherwise hang.
fn withheld() -> (Harness, u32, Vec<u8>) {
    let refused = Rc::new(Cell::new(0));
    let device = Withhold {
        memory: Memory::default(),
        withholding: false,
        refused: Rc::clone(&refused),
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
        Box::new(device),
    );
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    assert!(
        h.until_memory(BOUND, |m| m.frames > 0),
        "the first frame shown"
    );
    let (frames, shown) = (h.memory().frames, h.memory().pixels.clone());
    withholding(&mut h).withholding = true;
    h.type_str("\r");
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "got")));
    assert!(
        h.until(BOUND, |_| refused.get() > 0),
        "a frame found no token"
    );
    assert_eq!(h.memory().frames, frames, "nothing drawn while withheld");
    (h, frames, shown)
}

fn withholding(h: &mut Harness) -> &mut Withhold {
    h.app
        .videos
        .get_mut(Path::new("/dev/mem0"))
        .and_then(|v| v.as_any().downcast_mut::<Withhold>())
        .expect("the withholding device")
}

/// A frame that finds no token while the VT is shown waits for its
/// display: once the display lends one again, the frame reaches the
/// screen with no other event (vtium/vtium#174).
#[test]
fn a_frame_that_found_no_canvas_is_drawn_once_the_display_lends_again() {
    let (mut h, frames, shown) = withheld();
    withholding(&mut h).withholding = false;
    assert!(
        h.until_memory(Duration::from_secs(10), |m| m.frames > frames),
        "the waiting frame drawn"
    );
    assert_ne!(h.memory().pixels, shown, "the screen shows the line");
}

/// A display that lends no token for a long run does not turn the loop at
/// full speed: the waiting frame is composed again only once the display
/// takes a frame.
#[test]
fn a_display_lending_no_canvas_does_not_spin_the_loop() {
    let (mut h, _, _) = withheld();
    let refused = withholding(&mut h).refused.get();
    // A second on the seat's clock, which moves by each wait the loop is
    // told, at most 50 ms. A loop that spins is told to wake at once on
    // every turn, so the clock never moves, and the loop turns until the
    // turn count or the real-time bound stops it.
    let (start, end) = (h.at(), now() + BOUND);
    let mut turns = 0_u32;
    while h.at() < start + Duration::from_secs(1) && turns < 1000 && now() < end {
        h.sleep_turn(Duration::from_millis(50));
        turns += 1;
    }
    let asked = withholding(&mut h).refused.get() - refused;
    assert!(
        turns < 50 && asked < 5,
        "{turns} turns and {asked} tokens asked for in 1 s"
    );
}

/// A selection a program sets with OSC 52 is taken and held; with
/// `no-clipboard-write` it is dropped.
#[test]
fn osc_52_is_held_or_dropped_as_clipboard_write_says() {
    let set = "printf '\\033]52;c;aGVsbG8=\\007set\\n'; read x";
    let mut h = Harness::new(&["--login", "--", "/bin/sh", "-c", set]);
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "set")));
    assert_eq!(h.app.selection(), Some(&b"hello"[..]));
    let mut h = Harness::new(&[
        "--no-clipboard-write",
        "--login",
        "--",
        "/bin/sh",
        "-c",
        set,
    ]);
    assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "set")));
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

/// Whether the lines logged since the last call to [`logged`] include the
/// line vtium logs on the turn its wait for a display runs out.
fn ran_out() -> bool {
    logged()
        .iter()
        .any(|l| l.starts_with("no display appeared within"))
}

/// No display comes within `display-wait`: vtium exits with status 1,
/// once the wait has run, so the unit's `OnFailure=` can start a getty.
/// The wait runs out on the first turn after its due time, which logs it.
#[test]
fn with_no_display_by_the_wait_vtium_exits_1() {
    logged();
    let mut h = Harness::bare(&WAITING);
    h.skip_turn(Duration::from_millis(1000));
    assert!(!ran_out(), "the wait runs to its due time");
    h.skip_turn(Duration::from_millis(1));
    assert!(ran_out(), "the wait ran out a millisecond after it");
    assert!(h.until(BOUND, |a| a.finished().is_some()));
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
    // The wait runs out at the first turn after its due time.
    h.skip_turn(Duration::from_millis(1));
    assert!(h.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
}

/// A display that comes during the wait ends it: vtium runs past the
/// wait and draws on it.
#[test]
fn a_display_coming_during_the_wait_ends_it() {
    let mut h = Harness::bare(&WAITING);
    h.app.turn(h.at());
    assert_eq!(h.app.finished(), None, "waiting");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    h.shown();
    // The seat's clock past the second's wait.
    h.skip_turn(Duration::from_secs(2));
    assert_eq!(h.app.finished(), None, "still running past the wait");
}

/// A device that can't be used, its displays never set up, counts as
/// none.
#[test]
fn a_device_whose_displays_never_come_on_counts_as_none() {
    let mut h = Harness::bare(&WAITING);
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Unusable));
    h.skip_turn(Duration::from_millis(1100));
    // The switch back waits `LEAVE_WAIT` for the device to sleep, which
    // this one never reports.
    h.skip_turn(vtium::app::LEAVE_WAIT);
    assert!(h.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
}

/// Once running, a display that goes is hotplug: vtium keeps running
/// with none, past the wait, and draws on the next that comes.
#[test]
fn a_display_gone_after_the_start_is_hotplug() {
    let mut h = Harness::new(&WAITING);
    // A frame on screen: its flip read, which ends the start's wait.
    assert!(h.until_memory(BOUND, |m| m.frames > 0 && m.events.is_empty()));
    h.app.remove_video(Path::new("/dev/mem0"));
    assert!(h.app.cells().is_empty(), "no display");
    // Past the 1 s wait, on the seat's clock.
    h.skip_turn(Duration::from_millis(1500));
    assert_eq!(h.app.finished(), None, "still running with none");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(
        h.until_memory(BOUND, |m| m.frames > 0),
        "the new display drawn"
    );
    assert_eq!(h.app.finished(), None);
}

/// Another program taking the VT while vtium sets up its first frame, as
/// X starting on another VT does at boot (vtium/vtium#270), stops the
/// wait: vtium waits past `display-wait` for
/// the VT to come back, as for a switch away later in its life, and draws
/// once it does.
#[test]
fn a_vt_released_before_the_first_frame_waits_for_the_vt_to_come_back() {
    let mut h = Harness::with_vt(&WAITING, ScriptedVt::default());
    // The boot's turn acquired the VT and set the display up; no frame
    // of it has reached the screen yet.
    assert_eq!(h.memory().frames, 0, "no frame shown at the release");
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    assert_eq!(h.memory().frames, 0, "the first frame never showed");
    // Past the 1 s wait, on the seat's clock.
    h.skip_turn(Duration::from_secs(2));
    assert_eq!(h.app.finished(), None, "waiting for the VT past the wait");
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    h.shown();
    h.skip_turn(Duration::from_secs(2));
    assert_eq!(h.app.finished(), None, "a display came: running");
}

/// A release before any frame showed stops the wait, and the acquire
/// starts it again from the full `display-wait`: with still no display by then, the VT being
/// vtium's, vtium exits with status 1.
#[test]
fn the_wait_a_release_stopped_runs_whole_again_from_the_acquire() {
    logged();
    let mut h = Harness::boot(&WAITING, ScriptedVt::default(), None);
    h.skip_turn(Duration::from_millis(800));
    assert_eq!(h.app.finished(), None, "0.8 s into the 1 s wait");
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    h.skip_turn(Duration::from_secs(60));
    assert_eq!(h.app.finished(), None, "the VT is another's: no wait runs");
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    h.app.turn(h.at());
    assert!(h.app.vt.is_active(), "acquired");
    // 0.8 s and 0.5 s make more than the wait: had the wait kept the
    // time before the release, it would have run out here.
    h.skip_turn(Duration::from_millis(500));
    assert!(!ran_out(), "0.5 s into the wait started again");
    h.skip_turn(Duration::from_millis(500));
    assert!(!ran_out(), "the wait started again runs to its due time");
    h.skip_turn(Duration::from_millis(1));
    assert!(ran_out(), "the wait ran out a millisecond after it");
    assert!(h.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
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
    fn lend(&mut self, _: DisplayId) -> Option<Token> {
        None
    }
    fn give_back(&mut self, _: Token) {}
    fn swap(&mut self, _: Token, _: &[Rect]) -> Result<Swapped, SwapError> {
        Err(SwapError::Off)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
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
    logged();
    let mut h = Harness::bare(&["--login", "--", "/bin/sh", "-c", "read x"]);
    h.skip_turn(Duration::from_millis(10_000));
    assert!(!ran_out(), "the wait runs to its due time");
    h.skip_turn(Duration::from_millis(1));
    assert!(ran_out(), "the wait ran out a millisecond after it");
    assert!(h.until(BOUND, |a| a.finished().is_some()));
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
    h.skip_turn(Duration::from_secs(3_600));
    assert_eq!(h.app.finished(), None, "an hour on, still waiting");
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
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
    assert!(h.until_memory(BOUND, |m| m.frames > 0));
    h.type_str("a\r");
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
    let (_, winch, size) = got(&h.app).expect("a line");
    assert_eq!(size, "10 40");

    h.app.remove_video(Path::new("/dev/mem0"));
    // Past dpms-timeout with no input and no display.
    h.skip_turn(Duration::from_millis(1_500));
    assert!(
        !h.app.blank.blanked(),
        "the blank does not run with no display"
    );
    assert_eq!(h.app.grid(), (40, 10), "the last display's grid");
    h.type_str("b\r");
    assert!(
        h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "b")),
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
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
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
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "c")));
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
    h.shown();
    h.skip_turn(Duration::from_millis(1100));
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "blanked");
    h.app.remove_video(Path::new("/dev/mem0"));
    // A turn with no display.
    h.app.turn(h.at());
    h.app
        .add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::default()));
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
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
    assert!(h.until(BOUND, |a| a.cells().len() == 1), "announced");
    h.skip_turn(Duration::from_millis(1100));
    assert!(h.until(BOUND, |a| a.finished().is_some()));
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
    // As `main` runs it, the loop sleeps until the wake-up it is told,
    // here on the seat's clock moved on by that much; a loop told none
    // would sleep until something else woke it. Told to wake at once, as
    // on the turn at the due time, `main`'s loop returns at once and turns
    // again a moment later. The millisecond added to each wait stands in
    // for the real time a turn takes, so the turn after the due time
    // comes.
    let mut waited = Duration::ZERO;
    for _ in 0..10 {
        if h.app.finished().is_some() {
            break;
        }
        let wait = h
            .app
            .next_wake(h.at())
            .expect("a wake-up for the wait's end")
            .max(Duration::from_millis(1));
        waited += wait;
        h.skip_turn(wait);
    }
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    assert!(
        waited < Duration::from_millis(2_500),
        "woken at the wait's end: {waited:?}"
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
            "--dpms-timeout=60",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x",
        ],
        FakeVt::new(),
        Box::new(late),
    );
    h.skip_turn(Duration::from_secs(40));
    assert!(h.app.cells().is_empty(), "no display yet");
    h.late().held = None;
    assert!(h.until(BOUND, |a| a.cells().len() == 1));
    h.shown();
    // 80 s from the start, 40 s from the display.
    h.skip_turn(Duration::from_secs(40));
    assert_ne!(
        h.memory().dpms,
        Some(Dpms::Off),
        "not a minute from the start"
    );
    h.skip_turn(Duration::from_secs(21));
    assert_eq!(
        h.memory().dpms,
        Some(Dpms::Off),
        "a minute from the display"
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
    h.shown();
    h.skip_turn(Duration::from_millis(1100));
    assert_eq!(h.memory().dpms, Some(Dpms::Off), "blanked");
    h.scripted().events.push(VtEvent::Retaken);
    // The re-take in a turn of its own, before the release.
    h.app.turn(h.at());
    h.scripted().events.push(VtEvent::Release);
    assert!(h.until(BOUND, |a| !a.vt.is_active()), "released");
    let (wake_ups, unblanks) = (h.memory().wake_ups, h.memory().unblanks);
    h.memory().events.push(Event::Removed(ID));
    assert!(h.until(BOUND, |a| a.cells().is_empty()), "the display went");
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
    assert!(h.until_memory(BOUND, |m| m.frames > 0));
    h.type_str("\r");
    assert!(h.until(BOUND, |a| shows(a, "cdef end")));
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
    let loaded = config::load(&args, &mut |_: &Path| Ok(None));
    assert_eq!(loaded.reports, [], "the test's command line");
    let mut lp: Loop<App> = Loop::new().expect("a loop");
    let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
    let seat = SeatClock::new();
    let clock = seat.clock();
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
    app.turn(seat.now());
    Harness { lp, app, seat }
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.grid(), (40, 10));
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.cells().len(), 2, "both displays");
    let (cols, rows) = h.app.grid();
    assert_ne!((cols, rows), (80, 24));
    assert_eq!(first(&h.app).map(|f| f.0), Some(format!("{rows} {cols}")));
    h.type_str("a\r");
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
    h.type_str("a\r");
    assert!(h.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    let (size, pid) = first(&h.app).expect("a first line");
    assert_eq!(size, "10 40");
    h.type_str("\r");
    assert!(
        h.until(BOUND, |a| first(a).is_some_and(|f| f.1 != pid)),
        "started again"
    );
    assert_eq!(first(&h.app).map(|f| f.0), Some("10 40".to_owned()));
}

/// Under `display-wait=0` with no display, vtium exits at the first turn
/// the VT is shown, and the child never starts.
#[test]
fn under_no_wait_with_no_display_no_child_starts() {
    let mut h = Harness::bare(&["--display-wait=0", "--login", "--", "/bin/sh", "-c", FIRST]);
    // The wait runs out at the first turn after its due time.
    h.skip_turn(Duration::from_millis(1));
    assert!(h.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(h.app.finished(), Some(Exit::NoDisplay));
    assert!(!h.app.started(), "no child");
}

/// Under `display-wait=0` a display set up at the start is drawn, its
/// flip read, and vtium runs on.
#[test]
fn under_no_wait_a_display_at_the_start_runs() {
    let mut h = Harness::new(&["--display-wait=0", "--login", "--", "/bin/sh", "-c", FIRST]);
    assert!(h.until_memory(BOUND, |m| m.frames > 0), "drawn");
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.finished(), None, "a display was there");
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
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // A display lends a token only while it is ready for a frame.
        if !self.ready_for_frame(id) {
            return None;
        }
        self.memory.lend(id)
    }
    fn give_back(&mut self, token: Token) {
        self.memory.give_back(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        let swapped = self.memory.swap(token, damage)?;
        self.memory.events.retain(|e| *e != Event::Flipped(id));
        self.landed = Some(now() + Slow::FLIP);
        Ok(swapped)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.landed.is_some()
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        !self.is_flipping(id) && self.memory.ready_for_frame(id)
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
    assert!(h.until(BOUND, |a| first(a).is_some()), "the child started");
    assert_eq!(h.app.finished(), None, "the flip was waited for");
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
        h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "E[?999;1n")),
        "visible as the mode is set: {:?}",
        h.app.screen_text()
    );
    h.scripted().events.push(VtEvent::Release);
    assert!(
        h.until(BOUND, |a| a.screen_text().iter().any(|l| l == "E[?999;2n")),
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
        h.until(BOUND, |a| a
            .screen_text()
            .iter()
            .any(|l| l == "E[48;10;40;160;320t")),
        "the text area as the mode is set: {:?}",
        h.app.screen_text()
    );
    h.app.input_event(key(Mods::CTRL, 0x3d), h.at());
    assert_eq!(h.app.grid(), (20, 5), "zoomed in to 16x32 cells");
    assert!(
        h.until(BOUND, |a| a
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
        h.app.input_event(key(Mods::CTRL, k), h.at());
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
    assert!(h.until(BOUND, |a| {
        a.screen_text().iter().any(|l| l.contains("done"))
    }));
    h
}

/// ADR-1401 D7: the start logs each error of its head file with the file
/// and the line, and each warning with its line, while the console shows
/// only the outcome's line.
// covers: platform.heads.policy
#[test]
fn the_start_logs_each_head_file_error_with_its_line() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("boot-head-file-log");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o700)).expect("0700");
    let path = dir.join("vtium.conf");
    let text = "version = 1\n[head desk]\n[head bad]\ncolour = red\n[rules]\ndisplay * -> desk\ndisplay * -> desk\ninput * -> desc\n";
    std::fs::write(&path, text).expect("vtium.conf");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("0644");
    // read as the console reads it, by the account the test runs as
    let configdir = std::ffi::OsString::from(format!("--configdir={}", dir.display()));
    let loaded = vtium::config::load(&[configdir], &mut vtium::config::read_file);
    logged();
    let start = vtium::head::rules(&loaded.path, &loaded.file);
    let lines = logged();
    assert_eq!(
        start.line(),
        Some("config partly invalid, running what's valid")
    );
    let at = |n: usize| format!("{}:{n}: ", path.display());
    for n in [4, 7, 8] {
        assert!(
            lines.iter().any(|l| l.starts_with(&at(n))),
            "line {n} logged: {lines:#?}"
        );
    }
    assert!(lines.iter().all(|l| !l.starts_with(&at(6))), "{lines:#?}");
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
    let scratch = Scratch::new(&format!("boot-frit-{name}"));
    let dir = scratch.to_path_buf();
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

/// One cell's fill as the helper sends it: U+25A1 a solid block, then the
/// fill's end.
fn solid_white_square(cell: vtium_font::Cell) -> Vec<u8> {
    use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
    let mut s = StrikeBuilder::new(PixelFormat::R8, cell, vtium_font::Style::default());
    let g = s.glyph(
        vtium_font::Width::Narrow,
        vec![255; (cell.width * cell.height) as usize],
    );
    s.map('\u{25a1}', g);
    let set = SetBuilder::new("t").strike(s).write().expect("a set");
    let mut out = 4u32.to_le_bytes().to_vec();
    out.extend((set.len() as u32).to_le_bytes());
    out.extend(set);
    out.extend(5u32.to_le_bytes());
    out.extend(8u32.to_le_bytes());
    out.extend(cell.width.to_le_bytes());
    out.extend(cell.height.to_le_bytes());
    out
}

// vtium/vtium#314: a character no font holds draws the font's missing
// glyph, the built-in U+25A1. A set that brings its own U+25A1 changes
// that drawing though no cell shows U+25A1, so the console draws a frame
// at once, and the character draws the set's square.
#[test]
fn a_set_that_brings_u25a1_redraws_a_missing_character_at_once() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let (_dir, fonts, go) = frit_fonts(
        "missing",
        &solid_white_square(cell),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; exec 1>&-; wait",
    );
    // U+10FFFD, which neither the frit nor the built-in unifont holds
    let session = "stty -echo; printf '\\364\\217\\277\\275'; read x";
    let mut h = Harness::fonts(
        &[
            "--font-name=t",
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            session,
        ],
        Memory::default(),
        fonts.clone(),
    );
    assert!(
        h.until_memory(Duration::from_secs(10), |m| !solid(&top_cell(m, 0))),
        "the built-in U+25A1 is drawn for U+10FFFD"
    );
    let background = top_cell(h.memory(), 5)[0];
    let frames = h.memory().frames;
    std::fs::write(&go, b"").expect("the go file");
    // the wait only bounds a test that would otherwise hang
    assert!(
        h.until_memory(Duration::from_secs(10), |m| {
            let x = top_cell(m, 0);
            solid(&x) && x[0] != background
        }),
        "U+10FFFD still shows the built-in U+25A1 after the set's arrived"
    );
    assert_eq!(h.memory().frames, frames + 1, "one frame draws the arrival");
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
    h.app.input_event(key(Mods::CTRL, equal), h.at());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(20, 40)], (16, 4)));
    h.app.input_event(key(Mods::CTRL, minus), h.at());
    h.app.input_event(key(Mods::CTRL, minus), h.at());
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
// line, and the first frame waits no longer than the bound. The helper
// here never answers, so the boot returns only because the wait has a
// bound, and the line says the wait ran to it.
#[test]
fn a_bitmap_frits_own_cell_unanswered_falls_back_within_its_bound() {
    let (_dir, fonts, _go) = frit_fonts_of("own-none", "source=kbd:x\n", b"", "cat >/dev/null");
    logged();
    let h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 8)], (40, 20)));
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

// ADR-1200 D17 (the maintainer, 2026-11-03): a bitmap frit's own cell that
// comes after the 100 ms wait is kept and used. The held helper answers
// nothing until the test writes its go file, so the first frame draws in
// the display's 8x8 cell; once the answer comes, the frit draws in its
// own 8x16 cell, its grid laid out again, with one line, and the answer
// stays known to the frit
#[test]
fn a_bitmap_frits_own_cell_that_comes_late_is_taken_from_the_next_frame() {
    let (_dir, fonts, go) = frit_fonts_of(
        "own-late",
        "source=kbd:x\n",
        &own_cell(8, 16),
        "cat >/dev/null & while [ ! -e \"$2\" ]; do sleep 0.02; done; cat \"$1\"; wait",
    );
    logged();
    let mut h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    assert_eq!((h.app.cells(), h.app.grid()), (vec![(8, 8)], (40, 20)));
    let fell = logged();
    assert!(
        fell.iter().any(|l| l
            == "the frit t's own cell is not known (the helper gave no answer in 100 ms): it draws in the display's cell"),
        "{fell:?}"
    );
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        h.until(Duration::from_secs(10), |a| a.cells() == [(8, 16)]),
        "the late answer is taken: {:?}",
        h.app.cells()
    );
    assert_eq!(h.app.grid(), (40, 10));
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let known: Vec<_> = h.app.frit_cells("t").into_iter().map(|(_, a)| a).collect();
    assert_eq!(known, [Ok(vtium::helper::OwnCell::Bitmap(cell))]);
    let took = logged();
    let lines: Vec<&String> = took.iter().filter(|l| l.contains("came after")).collect();
    assert_eq!(
        lines,
        ["the frit t's own 8x16 cell came after the wait: it draws in it from the next frame"],
        "{took:?}"
    );
}

/// The outline font fixture vtium-config's tests measure: a subset of
/// DejaVu Sans Mono whose W rule gives 9 columns at 16 pixels and 12 at
/// 22.
fn dejavu() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vtium-config/tests/fixtures/DejaVuSansMono-W-M-0.ttf")
}

/// The fonts of an outline frit `t` drawing [`dejavu`], served by
/// [`real_helper`], its answers held until the test opens the go FIFO
/// when `held`; the directory, the fonts, the kinds file and the go FIFO.
fn outline_frit(name: &str, held: bool) -> (Scratch, Sources, PathBuf, PathBuf) {
    let scratch = Scratch::new(&format!("boot-outline-{name}"));
    let dir = scratch.to_path_buf();
    std::fs::write(
        dir.join("t.vtfrit"),
        format!("source=outline:{}\n", dejavu().display()),
    )
    .expect("the frit");
    let (kinds, go) = (dir.join("kinds"), dir.join("go"));
    if held {
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &go,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .expect("the go FIFO");
    }
    let fonts = Sources {
        frits: dir,
        helper: Some(real_helper(&kinds, held.then_some(go.as_path()))),
        ..no_fonts()
    };
    (scratch, fonts, kinds, go)
}

/// The kind of each frame the stand-in sent so far.
fn kinds_sent(kinds: &Path) -> Vec<u32> {
    std::fs::read_to_string(kinds)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.parse().ok())
        .collect()
}

/// Frame 10, an outline cell, and frame 7, no cell: the answers to a cell
/// query.
fn cell_answers(kinds: &Path) -> usize {
    kinds_sent(kinds)
        .iter()
        .filter(|&&k| matches!(k, 6 | 7 | 10))
        .count()
}

// overhang part 1 (the maintainer, 2026-11-01): an outline frit that
// states no cell asks the helper for its cell at the height it draws at,
// here font-size=16 at 72 DPI, and takes the W rule's 9 columns, where the
// built-in font's are 8. Each zoom asks again at the new height, and a
// height asked before is not asked again.
#[test]
fn an_outline_frit_draws_in_the_helpers_cell_at_each_height() {
    let (equal, minus) = (0x3d, 0x2d);
    let (_dir, fonts, kinds, _go) = outline_frit("each", false);
    logged();
    let mut h = with_fonts(&["--font-name=t", "--font-size=16"], fonts);
    assert_eq!(h.app.cells(), vec![(9, 16)]);
    assert_eq!(
        (h.app.grid().0, h.app.grid().1),
        (W as usize / 9, H as usize / 16)
    );
    assert!(
        logged()
            .iter()
            .any(|l| l == "the frit t draws in its outline font's 9x16 cell at 16 pixels"),
    );
    assert_eq!(cell_answers(&kinds), 1, "the first frame asked once");
    for _ in 16..22 {
        h.app.input_event(key(Mods::CTRL, equal), h.at());
    }
    assert_eq!(h.app.cells(), vec![(12, 22)]);
    assert_eq!(cell_answers(&kinds), 7, "each zoom asked at its height");
    for _ in 16..22 {
        h.app.input_event(key(Mods::CTRL, minus), h.at());
    }
    assert_eq!(h.app.cells(), vec![(9, 16)]);
    assert_eq!(cell_answers(&kinds), 7, "each height asked once");
    assert_eq!(
        kinds_sent(&kinds).iter().filter(|&&k| k == 10).count(),
        7,
        "every answer was an outline cell: {:?}",
        kinds_sent(&kinds)
    );
}

// overhang part 1: an outline cell that comes after the console's wait
// is dropped, the built-in font's width draws, and the next zoom asks
// again, its answer taken, a height that failed before included
#[test]
fn a_late_outline_cell_is_dropped_and_the_next_zoom_asks_again() {
    let (equal, minus) = (0x3d, 0x2d);
    let (_dir, fonts, kinds, go) = outline_frit("late", true);
    logged();
    let mut h = with_fonts(&["--font-name=t", "--font-size=22"], fonts);
    assert_eq!(h.app.cells(), vec![(8, 22)], "the built-in font's width");
    let lines = logged();
    assert!(
        lines.iter().any(|l| l
            == "the frit t's cell at 22 pixels is not known (the helper gave no answer in 100 ms): it draws as wide as the built-in font"),
        "{lines:?}"
    );
    // opening the FIFO to write lets the stand-in's answers go
    let _go = std::fs::OpenOptions::new()
        .write(true)
        .open(&go)
        .expect("the go FIFO");
    // the late answer arrives, and is dropped with no failure
    assert!(h.until(BOUND, |_| cell_answers(&kinds) == 1));
    assert_eq!(
        h.app.cells(),
        vec![(8, 22)],
        "the late answer changes nothing"
    );
    // The zoom's ask at 21 finds the late answer at 22 first, whichever
    // turn reads it, and takes the answer at 21 behind it.
    h.app.input_event(key(Mods::CTRL, minus), h.at());
    let at21 = h.app.cells();
    assert!(
        at21.first().is_some_and(|c| c.1 == 21 && c.0 != 8),
        "the helper's cell at 21, not the built-in font's: {at21:?}"
    );
    h.app.input_event(key(Mods::CTRL, equal), h.at());
    assert_eq!(h.app.cells(), vec![(12, 22)], "asked again at 22");
    let lines = logged();
    assert!(
        lines
            .iter()
            .all(|l| !l.contains("cell query not made") && !l.contains("gives up")),
        "{lines:?}"
    );
}

// ADR-1200 D17 and the operator's decision on #145, 2026-10-28: a bitmap
// frit asks for its fonts' own cell, and may wait, only at the first frame
// and at an OSC 50 switch. Once that ask failed, the failure holds at
// every height, as an answer does, so a zoom neither asks nor waits again
// and logs no second line (binary-review, finding 1).
#[test]
fn a_bitmap_frits_failed_cell_is_not_asked_again_at_a_zoom() {
    let (_dir, fonts, _go) =
        frit_fonts_of("own-none-zoom", "source=kbd:x\n", b"", "cat >/dev/null");
    logged();
    let mut h = with_fonts(&["--font-name=t", "--font-size=0"], fonts.clone());
    let unknown = |lines: &[String]| {
        lines
            .iter()
            .filter(|l| l.contains("own cell is not known"))
            .count()
    };
    assert_eq!(unknown(&logged()), 1, "the first frame asked once");
    assert_eq!(h.app.cells(), [(8, 8)]);
    h.app.input_event(key(Mods::CTRL, 0x3d), h.at());
    assert_eq!(h.app.cells(), [(8, 16)], "zoomed");
    assert_eq!(unknown(&logged()), 0, "the zoom asked nothing");
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
    h.settled();
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
    assert!(h.until(BOUND, |a| {
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
    h.settled();
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
    // Drawn from the screen as it is, with no older frame on its way.
    h.drawn_now();
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
    let scratch = Scratch::new(&format!("boot-rec-{name}"));
    let dir = scratch.to_path_buf();
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
    let rest = bytes.strip_prefix(b"VTVSREQ4")?.get(4..)?;
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
    assert!(
        h.eventually(BOUND, asked),
        "the cluster e U+0301 is asked for whole"
    );
}

// ADR-1200 D21: italic and bold text is asked for in an ask of its own
// style, beside the regular text's
// covers: sgr.italic, sgr.bold
#[test]
fn italic_and_bold_text_is_asked_for_in_its_style() {
    let (_dir, fonts) = recording("style-ask");
    let mut h = Harness::fonts(
        &[
            "--font-name=ter-v16n",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "printf '\\033[3mx\\033[0m \\033[1my\\033[0m z'; read x",
        ],
        Memory::default(),
        fonts.clone(),
    );
    // the style, the reserved capabilities, a codepoint count of 1, then
    // the codepoint
    let ask = |style: u32, c: char| -> Vec<u8> {
        [style, 0, 1, u32::from(c)]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect()
    };
    let asked = |h: &mut Harness, want: &[u8]| {
        requests(h, &fonts, 1)
            .iter()
            .any(|b| b.windows(want.len()).any(|w| w == want))
    };
    let (italic, bold) = (ask(2, 'x'), ask(1, 'y'));
    assert!(
        h.eventually(BOUND, |h| asked(h, &italic)),
        "x is asked for in italic"
    );
    assert!(h.eventually(BOUND, |h| asked(h, &bold)), "y in bold");
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
        h.app.input_event(key(Mods::CTRL, k), h.at());
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
    h.app.input_event(key(Mods::LOGO, equal), h.at());
    assert_eq!(h.app.cells()[0].1, 12, "turned a quarter, 72 DPI across");
    h.app.input_event(key(Mods::LOGO, minus), h.at());
    assert_eq!(h.app.cells()[0].1, 24);
    let second = Memory {
        dpi: Some((96.0, 96.0)),
        ..Memory::default()
    };
    h.app
        .add_video(PathBuf::from("/dev/mem1"), Box::new(second));
    assert!(h.until(BOUND, |a| a.cells().len() == 2));
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
        h.app.input_event(key(Mods::CTRL, k), h.at());
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
    h.app.input_event(key(Mods::CTRL, 0x2d), h.at());
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
    h.until(BOUND, |a| {
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

// ADR-1200 D5: `vtium-config set` writes the frit and then the query in
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
    // the cell, the style, regular, the reserved capabilities, then the
    // codepoints, the clusters and the held ranges, each counted
    assert_eq!(words(&rest[8..], 2), [0, 0], "regular, and no capabilities");
    let codepoints = words(&rest[16..], 1)[0] as usize;
    let mut rest = &rest[20 + 4 * codepoints..];
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

/// A release that finds a frame composing waits for it, as a framebuffer's
/// sleep waits for its token: the display reports `Asleep` once the
/// frame's token comes back, on a later turn, so the release is
/// acknowledged then. The frame is not shown, and the first frame after
/// the acquire is drawn whole, into a buffer of age 0.
#[test]
fn a_release_waits_for_the_frame_composing() {
    let mut h = Harness::with_vt(
        &[
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            "read x; echo got; read y",
        ],
        ScriptedVt::default(),
    );
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    h.shown();
    h.settled();
    let (entered, composing) = std::sync::mpsc::channel();
    let (go, gone) = std::sync::mpsc::channel::<()>();
    let gate = Mutex::new((entered, gone));
    h.app.set_draw_hook(
        "console",
        Some(Arc::new(move || {
            let gate = gate.lock().unwrap();
            gate.0.send(()).ok();
            gate.1.recv().ok();
        })),
    );
    h.type_str("\r");
    assert!(
        h.eventually(BOUND, |_| composing.try_recv().is_ok()),
        "a frame composing"
    );
    let frames = h.memory().frames;
    h.scripted().events.push(VtEvent::Release);
    assert!(
        h.until_memory(BOUND, |m| m.sleeps == 1),
        "the device sleeps"
    );
    h.app.turn(h.at());
    assert!(
        h.app.vt.is_active(),
        "not acknowledged while the frame composes"
    );
    h.app.set_draw_hook("console", None);
    go.send(()).unwrap();
    assert!(
        h.until(BOUND, |a| !a.vt.is_active()),
        "acknowledged once the token came back"
    );
    assert_eq!(h.memory().frames, frames, "the frame is not shown");
    let lent = h.memory().ages.len();
    h.scripted().active = true;
    h.scripted().events.push(VtEvent::Acquired);
    assert!(
        h.until_memory(BOUND, |m| m.frames > frames),
        "drawn after the acquire"
    );
    assert_eq!(h.memory().ages.get(lent), Some(&0));
}

/// A display whose swaps fail still holds each frame in its buffer, so the
/// seat draws the next frame at age 1, the cells that changed, and not
/// the whole screen again.
#[test]
fn a_frame_after_a_failed_swap_is_drawn_at_age_1() {
    let memory = Memory {
        refuses: true,
        ..Memory::default()
    };
    let mut h = Harness::build(
        &["--no-blink", "--login", "--", "/bin/sh", "-c", "read x"],
        ScriptedVt::default(),
        Box::new(memory),
    );
    assert!(h.until(BOUND, |a| a.vt.is_active()));
    h.type_str("ab");
    assert!(
        h.until_memory(BOUND, |m| m.ages.contains(&1)),
        "a frame drawn into the buffer the failed swap left: {:?}",
        h.memory().ages
    );
}

/// A zoom while a frame composes makes a new renderer, whose first frame
/// asks for a token while the old frame's is still out: the display
/// refuses it, and the new frame is drawn once the old token comes back.
#[test]
fn a_zoom_while_a_frame_composes_waits_for_its_token() {
    let mut h = Harness::new(&["--no-blink", "--login", "--", "/bin/sh", "-c", "read x"]);
    h.shown();
    h.settled();
    let (entered, composing) = std::sync::mpsc::channel();
    let (go, gone) = std::sync::mpsc::channel::<()>();
    let gate = Mutex::new((entered, gone));
    h.app.set_draw_hook(
        "console",
        Some(Arc::new(move || {
            let gate = gate.lock().unwrap();
            gate.0.send(()).ok();
            gate.1.recv().ok();
        })),
    );
    h.type_str("x");
    assert!(
        h.eventually(BOUND, |_| composing.try_recv().is_ok()),
        "a frame composing"
    );
    let (lent, frames) = (h.memory().ages.len(), h.memory().frames);
    h.app.input_event(key(Mods::CTRL, 0x3d), h.at());
    assert!(
        h.until_memory(BOUND, |m| m.refused_while_out > 0),
        "the new renderer's frame asks while the old token is out"
    );
    assert_eq!(h.memory().ages.len(), lent, "no second token lent");
    h.app.set_draw_hook("console", None);
    go.send(()).unwrap();
    assert!(
        h.until_memory(BOUND, |m| m.frames > frames && m.ages.len() > lent),
        "the new renderer's frame drawn once the old token came back"
    );
}

/// ADR-1303 D9, D10: a head with `hwaccel` on draws through the GL context
/// it makes on its render thread, here Mesa's llvmpipe in place of a card's
/// render node, and the frame the display shows is the CPU path's, byte for
/// byte; the display's log line names the path. A head whose context
/// cannot be made says why and draws on the CPU path, and with `hwaccel`
/// off a head never makes one.
#[cfg(feature = "gpu")]
mod gpu_path {
    use vtium::painter::{Fault, GpuSource, TestGpu};

    use super::*;

    /// Colors, a bold line and a block, with `done` once written.
    const SCRIPT: &str = "printf '\\033[41m red \\033[42;30m green \\033[0m\\n\\033[1;34mblue\\033[0m \\342\\226\\210 done'; read x";

    /// The seat booted with `args`, running `script` in its shell, with
    /// the test's GPU `source`. Every test here boots through it.
    ///
    /// The cursor does not blink, so a frame drawn after a test moves the
    /// seat's clock shows what a frame drawn before it shows, as
    /// [`a_frame_drawn_a_blink_period_later_shows_the_same_pixels`] checks.
    fn booted(args: &[&str], script: &str, source: GpuSource) -> Harness {
        let args: Vec<&str> = args
            .iter()
            .copied()
            .chain(["--no-blink", "--login", "--", "/bin/sh", "-c", script])
            .collect();
        Harness::boot_set(
            &args,
            FakeVt::new(),
            Some(Box::new(Memory::default())),
            no_fonts(),
            |app| app.set_gpu_source(source),
        )
    }

    /// The seat booted with `args` and the test's GPU `source`: the pixels
    /// of the frame it shows once the script is written, those of a frame
    /// drawn whole after it, and the lines it logged.
    fn drawn(args: &[&str], source: GpuSource) -> (Vec<u8>, Vec<u8>, Vec<String>) {
        logged();
        let mut h = booted(args, SCRIPT, source);
        assert!(
            h.until(BOUND, |a| a
                .screen_text()
                .iter()
                .any(|l| l.contains("done"))),
            "the script written"
        );
        h.settled();
        let shown = h.memory().pixels.clone();
        let whole = h.drawn_now();
        assert!(whole.iter().any(|&b| b != 0), "a frame drawn");
        (shown, whole, logged())
    }

    fn renderer(lines: &[String]) -> Vec<&str> {
        lines
            .iter()
            .filter(|l| l.starts_with("Display "))
            .filter_map(|l| l.split("text renderer [").nth(1)?.split(']').next())
            .collect()
    }

    // The tests here compare frames drawn at different times, so a frame
    // drawn a blink period later must show the same pixels on either path
    #[test]
    fn a_frame_drawn_a_blink_period_later_shows_the_same_pixels() {
        for args in [&[][..], &["--hwaccel"]] {
            let mut h = booted(args, SCRIPT, GpuSource::Software);
            assert!(h.until(BOUND, |a| {
                a.screen_text().iter().any(|l| l.contains("done"))
            }));
            let early = h.drawn_now();
            h.skip_turn(vtium::blink::TEXT_PERIOD);
            let late = h.drawn_now();
            assert!(late == early, "{args:?}: the frame changed with the clock");
        }
    }

    #[test]
    fn a_head_with_hwaccel_and_a_render_node_draws_the_cpu_paths_frame_on_the_gpu_path() {
        let (cpu_shown, cpu_whole, cpu_log) = drawn(&[], GpuSource::Software);
        assert_eq!(renderer(&cpu_log), ["cpu"], "hwaccel off: {cpu_log:#?}");
        let (gpu_shown, gpu_whole, gpu_log) = drawn(&["--hwaccel"], GpuSource::Software);
        assert_eq!(renderer(&gpu_log), ["gpu"], "{gpu_log:#?}");
        assert!(
            !gpu_log.iter().any(|l| l.contains("GPU path")),
            "{gpu_log:#?}"
        );
        assert!(
            gpu_shown == cpu_shown,
            "the frame shown differs from the CPU path's"
        );
        assert!(
            gpu_whole == cpu_whole,
            "the frame drawn whole differs from the CPU path's"
        );
    }

    // ADR-1303 D6, D10: the GPU path draws no 1-bit display yet, so the
    // CPU path draws it, and its line names cpu on a head on the GPU path
    #[test]
    fn a_1_bit_display_on_a_gpu_head_logs_the_cpu_path_that_draws_it() {
        let (cpu_shown, _, _) = drawn(&["--display-depth=1"], GpuSource::Software);
        let (shown, _, log) = drawn(&["--hwaccel", "--display-depth=1"], GpuSource::Software);
        assert_eq!(renderer(&log), ["cpu"], "{log:#?}");
        assert!(
            shown == cpu_shown,
            "the frame shown differs from the CPU path's"
        );
    }

    // ADR-1303 D3, D6: a display turned a quarter during the run draws on
    // the GPU path the CPU path's frame; the renderer turns its pipeline to
    // its own orientation as each frame starts
    #[test]
    fn a_display_turned_during_the_run_draws_the_cpu_paths_frame_on_the_gpu_path() {
        let turned = |args: &[&str], source: GpuSource| {
            let mut h = booted(args, SCRIPT, source);
            assert!(h.until(BOUND, |a| {
                a.screen_text().iter().any(|l| l.contains("done"))
            }));
            let upright = h.drawn_now();
            h.app.input_event(key(Mods::LOGO, 0x3d), h.at());
            let quarter = h.drawn_now();
            assert!(quarter != upright, "the display turned");
            quarter
        };
        let cpu = turned(&[], GpuSource::Software);
        let gpu = turned(&["--hwaccel"], GpuSource::Software);
        assert!(gpu == cpu, "the turned frame differs from the CPU path's");
    }

    // ADR-1303 D9, D10: a display the GPU path cannot set up holds its
    // head on the CPU path, with one line naming its connector and the
    // display's line again naming cpu, and its set-up is not tried again at
    // its size
    #[test]
    fn a_display_the_gpu_cannot_set_up_logs_once_and_its_line_names_cpu() {
        let (cpu_shown, cpu_whole, _) = drawn(&[], GpuSource::Software);
        let gpu = Arc::new(TestGpu::default());
        gpu.refuse_set_up.lock().unwrap().insert(ID);
        let (shown, whole, log) = drawn(&["--hwaccel"], GpuSource::Test(Arc::clone(&gpu)));
        let refused: Vec<&String> = log
            .iter()
            .filter(|l| l.contains("the GPU path cannot set it up"))
            .collect();
        assert_eq!(refused.len(), 1, "{log:#?}");
        assert!(refused[0].starts_with("display MEM-1: "), "{log:#?}");
        assert_eq!(renderer(&log), ["gpu", "cpu"], "{log:#?}");
        assert_eq!(
            *gpu.set_ups.lock().unwrap(),
            [(PathBuf::from("/dev/mem0"), ID)],
            "tried once"
        );
        assert!(
            shown == cpu_shown,
            "the frame shown differs from the CPU path's"
        );
        assert!(
            whole == cpu_whole,
            "the frame drawn whole differs from the CPU path's"
        );
    }

    /// The same through this machine's first render node, on its GPU. It
    /// opens a render node, which needs no DRM master, and runs only when
    /// asked.
    #[test]
    #[ignore = "needs a GPU's render node: cargo test -p vtium --features gpu --test boot -- --ignored gpu_path"]
    fn a_head_on_a_gpus_render_node_draws_the_cpu_paths_frame() {
        let node = std::fs::read_dir("/dev/dri")
            .expect("/dev/dri")
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("renderD"))
            })
            .min()
            .expect("a render node");
        let (cpu_shown, cpu_whole, _) = drawn(&[], GpuSource::Software);
        let (shown, whole, log) = drawn(&["--hwaccel"], GpuSource::RenderNode(node));
        assert_eq!(renderer(&log), ["gpu"], "{log:#?}");
        assert!(
            shown == cpu_shown,
            "the frame shown differs from the CPU path's"
        );
        assert!(
            whole == cpu_whole,
            "the frame drawn whole differs from the CPU path's"
        );
    }

    /// A head with hwaccel on the test's GPU, the script written and no
    /// fault queued yet, and what the test sees of the GPU.
    fn on_test_gpu() -> (Harness, Arc<TestGpu>) {
        let gpu = Arc::new(TestGpu::default());
        let mut h = booted(&["--hwaccel"], SCRIPT, GpuSource::Test(Arc::clone(&gpu)));
        assert!(h.until(BOUND, |a| {
            a.screen_text().iter().any(|l| l.contains("done"))
        }));
        h.settled();
        (h, gpu)
    }

    fn queue(gpu: &TestGpu, add: &[Fault]) {
        gpu.faults.lock().unwrap().extend(add.iter().copied());
    }

    fn failed(lines: &[String]) -> usize {
        lines
            .iter()
            .filter(|l| l.contains("the GPU path failed"))
            .count()
    }

    // ADR-1303 D9: a GL error drops one frame from the GPU path, which the
    // CPU path draws whole, and the next frame draws on the GPU path whole;
    // a frame that draws counts the errors from nothing again
    #[test]
    fn a_gl_error_drops_one_frame_and_the_head_stays_on_the_gpu_path() {
        let (_, cpu, _) = drawn(&[], GpuSource::Software);
        let (mut h, gpu) = on_test_gpu();
        logged();
        for _ in 0..2 {
            queue(&gpu, &[Fault::Gl]);
            assert!(
                h.drawn_now() == cpu,
                "the failed frame differs from the CPU path's"
            );
            assert!(
                gpu.faults.lock().unwrap().is_empty(),
                "the frame raised no fault"
            );
            assert!(
                h.drawn_now() == cpu,
                "the next frame differs from the CPU path's"
            );
        }
        let lines = logged();
        assert_eq!(
            (failed(&lines), renderer(&lines)),
            (0, vec![]),
            "{lines:#?}"
        );
    }

    // ADR-1303 D9: a display's first frame the GPU path fails is logged
    // once, naming the display's connector, and its later failed frames are
    // not, while the head stays on the GPU path; two failures, since a third
    // within 60 frames would end the path
    #[test]
    fn a_displays_failed_gpu_frames_are_logged_once_by_its_connector() {
        let (mut h, gpu) = on_test_gpu();
        logged();
        for _ in 0..2 {
            queue(&gpu, &[Fault::Gl]);
            h.drawn_now();
            h.drawn_now();
        }
        let all = logged();
        let lines: Vec<&String> = all
            .iter()
            .filter(|l| l.contains("a frame failed on the GPU path"))
            .collect();
        assert_eq!(lines.len(), 1, "{all:#?}");
        assert!(lines[0].starts_with("display MEM-1: "), "{all:#?}");
        assert_eq!(failed(&all), 0, "the head left the GPU path: {all:#?}");
    }

    // ADR-1303 D9: a frame the GPU path draws and then fails to read back
    // into the display's buffer is drawn again on the CPU path, after the
    // renderer forgets its history, so the display's other buffer, drawn
    // next at age 2, shows the pointer where it moved and nowhere else
    #[test]
    fn a_frame_whose_read_back_fails_leaves_no_ghost_in_the_other_buffer() {
        // z writes Z at the top left and leaves the cursor where it is
        const KEYS: &str =
            "stty -echo; printf ready; while read -r k; do printf '\\0337\\033[1;1HZ\\0338'; done";
        let shown = |hwaccel: bool| {
            let gpu = Arc::new(TestGpu::default());
            let args: Vec<&str> = hwaccel.then_some("--hwaccel").into_iter().collect();
            let mut h = booted(&args, KEYS, GpuSource::Test(Arc::clone(&gpu)));
            assert!(h.until(BOUND, |a| {
                a.screen_text().iter().any(|l| l.contains("ready"))
            }));
            h.point(
                [PointerEvent::Moved {
                    x: 20,
                    y: 20,
                    held: None,
                }],
                Mods::empty(),
            );
            let second = h.memory().drawn.lock().unwrap().clone();
            h.memory().double = Some((Arc::new(Mutex::new(second)), 0));
            h.drawn_now();
            h.drawn_now();
            if hwaccel {
                logged();
                queue(&gpu, &[Fault::LostAtRead]);
            }
            // the frame that moves the pointer, then one that writes Z
            let before = h.memory().frames;
            h.point(
                [PointerEvent::Moved {
                    x: 100,
                    y: 60,
                    held: None,
                }],
                Mods::empty(),
            );
            assert!(
                h.until_memory(BOUND, |m| m.frames > before),
                "the move drawn"
            );
            h.settled();
            let before = h.memory().frames;
            h.type_str("z\r");
            assert!(h.until_memory(BOUND, |m| m.frames > before), "Z drawn");
            assert!(h.until(BOUND, |a| a.screen_text().iter().any(|l| l.contains('Z'))));
            h.settled();
            assert!(gpu.faults.lock().unwrap().is_empty(), "the fault raised");
            if hwaccel {
                let lines = logged();
                assert_eq!(failed(&lines), 1, "the read ended the GPU path: {lines:#?}");
            }
            assert_eq!(h.memory().ages.last(), Some(&2), "the last frame at age 2");
            h.memory().pixels.clone()
        };
        let cpu = shown(false);
        let gpu = shown(true);
        assert!(gpu == cpu, "the buffer shown differs from the CPU path's");
    }

    // ADR-1303 D9, D10: a GL error that fails every other frame ends the
    // GPU path at its third failure within the display's last 60 frames,
    // though no two failed frames are in a row, and the log says so once
    #[test]
    fn a_gl_error_in_every_other_frame_ends_the_gpu_path_at_the_third() {
        let (_, cpu, _) = drawn(&[], GpuSource::Software);
        let (mut h, gpu) = on_test_gpu();
        logged();
        let mut lines = Vec::new();
        for failure in 1..=3 {
            queue(&gpu, &[Fault::Gl]);
            assert!(h.drawn_now() == cpu, "failed frame {failure} differs");
            assert!(gpu.faults.lock().unwrap().is_empty(), "the fault raised");
            lines.extend(logged());
            let want = usize::from(failure == 3);
            assert_eq!(failed(&lines), want, "after failure {failure}: {lines:#?}");
            assert!(h.drawn_now() == cpu, "the frame after {failure} differs");
        }
        lines.extend(logged());
        assert_eq!(failed(&lines), 1, "{lines:#?}");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("in 3 of a display's last 60 frames")),
            "{lines:#?}"
        );
        assert_eq!(renderer(&lines), ["cpu"], "{lines:#?}");
    }

    // ADR-1303 D9, D10: a GL error in two frames in a row, out of memory or
    // a lost context leaves the GPU path until vtium restarts, said once,
    // with the display's line again naming cpu, and the frames are the CPU
    // path's from then on
    #[test]
    fn a_second_error_running_out_of_memory_or_a_lost_context_leave_the_gpu_path_once() {
        let (_, cpu, _) = drawn(&[], GpuSource::Software);
        for fatal in [
            &[Fault::Gl, Fault::Gl][..],
            &[Fault::OutOfMemory],
            &[Fault::ContextLost],
        ] {
            let (mut h, gpu) = on_test_gpu();
            logged();
            queue(&gpu, fatal);
            for _ in fatal {
                assert!(h.drawn_now() == cpu, "{fatal:?}: a failed frame differs");
            }
            let lines = logged();
            assert_eq!(failed(&lines), 1, "{fatal:?}: {lines:#?}");
            assert_eq!(renderer(&lines), ["cpu"], "{fatal:?}: {lines:#?}");
            // nothing raises a fault now: the head draws on the CPU path
            queue(&gpu, &[Fault::Gl]);
            assert!(h.drawn_now() == cpu, "{fatal:?}: a CPU frame differs");
            assert_eq!(
                gpu.faults.lock().unwrap().len(),
                1,
                "{fatal:?}: the GPU path drew"
            );
            let lines = logged();
            assert_eq!((failed(&lines), renderer(&lines)), (0, vec![]), "{fatal:?}");
        }
    }

    // ADR-1303 D11 part 6: a display that leaves frees what the GPU path
    // kept for it, its frames in GL and its glyphs, and one that comes back
    // is drawn on the GPU path again
    #[test]
    fn a_display_that_leaves_frees_what_the_gpu_path_kept_for_it() {
        use std::sync::atomic::Ordering::SeqCst;
        let (mut h, gpu) = on_test_gpu();
        assert_eq!(
            gpu.kept.load(SeqCst),
            1,
            "the display drawn on the GPU path"
        );
        h.memory().events.push(Event::Removed(ID));
        assert!(
            h.until(BOUND, |_| gpu.kept.load(SeqCst) == 0),
            "the display's frames kept after it left"
        );
        h.memory().events.push(Event::Added(ID));
        assert!(
            h.until(BOUND, |_| gpu.kept.load(SeqCst) == 1),
            "the display that came back not drawn on the GPU path"
        );
    }

    #[test]
    fn a_head_whose_gpu_cannot_start_logs_the_step_and_draws_on_the_cpu_path() {
        let (cpu_shown, _, _) = drawn(&[], GpuSource::Software);
        let missing = GpuSource::RenderNode(PathBuf::from("/nonexistent/renderD128"));
        let (shown, whole, log) = drawn(&["--hwaccel"], missing);
        assert_eq!(renderer(&log), ["cpu"], "{log:#?}");
        assert!(
            log.iter().any(
                |l| l.contains("the GPU path's set-up failed at the render node")
                    && l.contains("/nonexistent/renderD128")
            ),
            "{log:#?}"
        );
        assert!(shown == cpu_shown, "the frame differs from the CPU path's");
        assert!(whole.iter().any(|&b| b != 0));
    }
}
