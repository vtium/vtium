//! Heads (ADR-1401 D1, D3): the seat booted with displays in memory, on
//! the fake VT, with a real child on a real pty in each head. With no head
//! file every display mirrors one grid, as before heads; a head file gives
//! each head its own grid at its own displays' density.

use std::any::Any;
use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::head::{Head, Start};
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Edid, EdidId, Event, Identity, Memory as Shared, Rect, SwapError, Swapped,
    Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;
use vtium_settings::Class;

#[path = "common/vt.rs"]
mod counting;
// The helper that writes the stand-in programs, kept with vtium-config's
// tests and shared by every crate whose tests run a program they made.
#[path = "common/scratch.rs"]
mod scratch;
use vtium_testscript as script;
#[path = "common/helper.rs"]
mod stand_in;
use counting::CountingVt;
use scratch::Scratch;
use stand_in::real_helper;

const ID: DisplayId = DisplayId(1);

/// How long a wait on a state that another thread or process brings, such
/// as a frame the render thread draws or a line a session prints, may run
/// before the test fails: long enough for a loaded machine, and only a
/// bound, since each wait ends at the state it waits for.
const BOUND: Duration = Duration::from_secs(10);

/// A device with one display, its frame buffer in memory, named for the
/// rules by its connector and its card.
struct Memory {
    w: u32,
    h: u32,
    /// The refresh rate of its mode, in mHz.
    refresh_mhz: u32,
    connector: &'static str,
    card: &'static str,
    /// The monitor its EDID names, where it has one.
    edid: Option<EdidId>,
    dpi: f64,
    /// Its connector was lit at start, so it is the boot display.
    lit: bool,
    /// Left dark at its bind, as a DRM card leaves a display the rules
    /// leave dark while another is lit: never announced until let go.
    held: bool,
    /// Let go: the next hotplug announces it.
    released: bool,
    /// What the display shows: the buffer lent, as the last swap left it.
    pixels: Vec<u8>,
    /// The buffer each token lends, drawn into on the render thread. As a
    /// framebuffer's one buffer, it is lent to one token at a time, a token
    /// from before a sleep, a wake-up or a blank comes back too late, a
    /// dropped token leaves it at age 0, and a sleep with it lent clears
    /// it when the token comes back.
    drawn: Arc<Mutex<Vec<u8>>>,
    /// The buffer's age: 1 once it holds a frame, 0 when unknown.
    age: u32,
    /// Taken anew at each sleep, wake-up and blank.
    lease: u64,
    /// A token was lent and not given back since.
    lent: bool,
    /// The sleep found the buffer lent.
    clear_owed: bool,
    /// The age of each token lent, in order.
    ages: Vec<u32>,
    /// The lease of each frame shown, in order.
    shown: Vec<u64>,
    events: Vec<Event>,
    awake: bool,
    dpms: Option<Dpms>,
    frames: u32,
    /// Each unblank of the device, and each power state set, in order.
    unblanks: u32,
    dpms_sets: Vec<Dpms>,
    /// How many times vtium parked the display: set it off and freed its
    /// buffer, which [`Memory::pixels`] then no longer holds.
    parks: u32,
    /// Connected, but the device cannot light it, as a DRM card leaves a
    /// display that no placement of the card's displays lights: it shows
    /// nothing and lends no buffer.
    unlit: bool,
}

impl Memory {
    /// At 72 DPI, where a point is a pixel.
    fn new(w: u32, h: u32, connector: &'static str, card: &'static str) -> Memory {
        Memory {
            w,
            h,
            refresh_mhz: 60_000,
            connector,
            card,
            edid: None,
            dpi: 72.0,
            lit: false,
            held: false,
            released: false,
            pixels: Vec::new(),
            drawn: Arc::default(),
            age: 0,
            lease: 0,
            lent: false,
            clear_owed: false,
            ages: Vec::new(),
            shown: Vec::new(),
            events: Vec::new(),
            awake: false,
            dpms: None,
            frames: 0,
            unblanks: 0,
            dpms_sets: Vec::new(),
            parks: 0,
            unlit: false,
        }
    }

    /// This display, measuring `dpi`.
    fn at(self, dpi: f64) -> Memory {
        Memory { dpi, ..self }
    }

    /// This display, with an EDID naming maker `AAA`, product 0x0001 and
    /// the serial number `serial`.
    fn named(self, serial: u32) -> Memory {
        let mut base = [0u8; 128];
        base[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        // `A` is 1 in each five-bit letter of the maker id
        base[8..12].copy_from_slice(&[0x04, 0x21, 0x01, 0x00]);
        base[12..16].copy_from_slice(&serial.to_le_bytes());
        base[127] = 0u8.wrapping_sub(base.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)));
        let edid = Edid::parse(&base).map(|e| e.id().clone());
        assert!(edid.is_some(), "the fixture is an EDID");
        Memory { edid, ..self }
    }

    /// This display, lit at start.
    fn lit(self) -> Memory {
        Memory { lit: true, ..self }
    }

    /// This display, left dark at its bind.
    fn held(self) -> Memory {
        Memory { held: true, ..self }
    }
}

impl Memory {
    /// A new mode in place, `w` by `h`, on the same connector, as a
    /// modeset that falls back to the display's default mode sets it: the
    /// display draws into new buffers at the new size, a token lent before
    /// comes back too late, and the owner is told to redraw.
    fn set_mode(&mut self, w: u32, h: u32) {
        (self.w, self.h) = (w, h);
        self.lease += 1;
        self.age = 0;
        self.pixels = vec![0; (w * h * 4) as usize];
        self.drawn = Arc::new(Mutex::new(self.pixels.clone()));
        self.events.push(Event::Redraw(ID));
    }

    /// A new refresh rate in place, `mhz`, at the same size, as a modeset
    /// that falls back to a default mode of the same size sets it: the
    /// display keeps its buffer, which is age 0 at the new mode, and the
    /// owner is told to redraw.
    fn set_refresh(&mut self, mhz: u32) {
        self.refresh_mhz = mhz;
        self.age = 0;
        self.events.push(Event::Redraw(ID));
    }

    /// The device can no longer light the display, which stays connected:
    /// the owner hears it unlit.
    fn unlight(&mut self) {
        self.unlit = true;
        self.lease += 1;
        self.events.push(Event::Unlit(ID));
    }

    /// A modeset lights the display again, with what its buffer held
    /// unknown: the owner is told to draw it whole.
    fn relight(&mut self) {
        self.unlit = false;
        self.lease += 1;
        self.age = 0;
        self.events.push(Event::Redraw(ID));
    }

    /// Pays a clear the sleep owed, once no token holds the buffer.
    fn pay_clear(&mut self) {
        if self.clear_owed && Arc::strong_count(&self.drawn) == 1 {
            self.clear_owed = false;
            self.drawn.lock().unwrap().fill(0);
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
        if self.awake && !self.held {
            vec![ID]
        } else {
            Vec::new()
        }
    }
    fn name(&self, _: DisplayId) -> String {
        self.connector.to_owned()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((self.w, self.h))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(self.refresh_mhz)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((self.dpi, self.dpi))
    }
    fn identity(&self, _: DisplayId) -> Option<Identity> {
        Some(Identity {
            connector: self.connector.to_owned(),
            card: Some(self.card.to_owned()),
            edid: self.edid.clone(),
            connector_path: None,
            lit_at_start: self.lit,
        })
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        // One token at a time: the seat asks again while one is out when
        // a new renderer, as an OSC 50 font switch makes, draws before the
        // old one's frame comes back, and is refused, as a display refuses.
        if !self.ready_for_frame(id) {
            return None;
        }
        if std::mem::take(&mut self.lent) {
            // dropped without being given back
            self.age = 0;
        }
        self.lent = true;
        self.ages.push(self.age);
        let memory: Arc<Shared> = self.drawn.clone();
        Some(Token::new(id, (self.w, self.h), self.w * 4, self.age, memory).leased(self.lease))
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
            self.age = 0;
            return Err(SwapError::Dropped);
        }
        if damage.is_empty() && self.dpms == Some(Dpms::On) {
            return Ok(Swapped::Nothing);
        }
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        self.age = 1;
        self.frames += 1;
        self.shown.push(token.lease());
        // The display shows the buffer the frame was drawn into.
        self.pixels.clone_from(&self.drawn.lock().unwrap());
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake
            && !self.unlit
            && self.dpms == Some(Dpms::On)
            && Arc::strong_count(&self.drawn) == 1
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        if dpms != Dpms::On && self.dpms == Some(Dpms::On) {
            self.lease += 1;
        }
        if dpms == Dpms::On && self.dpms != Some(Dpms::On) {
            self.age = 0;
        }
        self.dpms = Some(dpms);
        self.dpms_sets.push(dpms);
        Ok(())
    }
    fn sleep(&mut self) {
        self.lease += 1;
        self.awake = false;
        if Arc::strong_count(&self.drawn) > 1 {
            self.clear_owed = true;
        } else {
            self.drawn.lock().unwrap().fill(0);
        }
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.lease += 1;
        self.age = 0;
        self.clear_owed = false;
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
        // A token from before still holds the buffer, which the next frame
        // after it comes back draws whole.
        if Arc::strong_count(&self.drawn) == 1 {
            *self.drawn.lock().unwrap() = self.pixels.clone();
        }
        self.awake = true;
        self.dpms = Some(Dpms::On);
        if !self.held {
            self.events.push(if first {
                Event::Added(ID)
            } else {
                Event::Redraw(ID)
            });
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
        if std::mem::take(&mut self.released) {
            // a display parked before comes back with a new buffer, lit
            if self.pixels.is_empty() {
                self.pixels = vec![0; (self.w * self.h * 4) as usize];
                self.dpms = Some(Dpms::On);
            }
            self.events.push(Event::Added(ID));
        }
        Ok(())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn dark(&self) -> Vec<Identity> {
        (self.awake && self.held)
            .then(|| self.identity(ID))
            .flatten()
            .into_iter()
            .collect()
    }
    fn park(&mut self, _: DisplayId) -> io::Result<()> {
        if !self.awake || self.held {
            return Err(io::Error::other("the display is not online"));
        }
        // as a DRM card parks it: off, its buffer freed, reported removed,
        // and left dark until let go
        self.held = true;
        self.dpms = Some(Dpms::Off);
        self.pixels = Vec::new();
        self.parks += 1;
        self.events.push(Event::Removed(ID));
        Ok(())
    }
    fn release_dark(&mut self, connector: &str) -> bool {
        let let_go = self.held && connector == self.connector;
        if let_go {
            self.held = false;
            self.released = true;
        }
        let_go
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// A gate a head's frames wait at, through the head's draw hook on its
/// render thread, until the test opens it. It also opens when the test
/// drops its [`Opens`], so a failing test never leaves a render thread
/// waiting. A frame waits at most three times [`BOUND`], only so that a
/// frame the gate never lets through fails the test instead of hanging
/// it, and the gate records that it gave up.
struct Gate {
    open: Mutex<bool>,
    opened: std::sync::Condvar,
    expired: std::sync::atomic::AtomicBool,
}

impl Gate {
    /// A closed gate, the hook that holds a frame at it, and the guard
    /// that opens it when dropped.
    fn closed() -> (Arc<Gate>, vtium::head::DrawHook, Opens) {
        let gate = Arc::new(Gate {
            open: Mutex::new(false),
            opened: std::sync::Condvar::new(),
            expired: std::sync::atomic::AtomicBool::new(false),
        });
        let held = Arc::clone(&gate);
        let hook: vtium::head::DrawHook = Arc::new(move || {
            let open = held.open.lock().expect("the gate");
            let (open, waited) = held
                .opened
                .wait_timeout_while(open, 3 * BOUND, |open| !*open)
                .expect("the gate");
            drop(open);
            if waited.timed_out() {
                held.expired
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let opens = Opens(Arc::clone(&gate));
        (gate, hook, opens)
    }

    fn open(&self) {
        *self.open.lock().expect("the gate") = true;
        self.opened.notify_all();
    }

    /// Whether a frame gave up waiting before the gate opened.
    fn expired(&self) -> bool {
        self.expired.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Opens its gate when dropped.
struct Opens(Arc<Gate>);

impl Drop for Opens {
    fn drop(&mut self) {
        self.0.open();
    }
}

/// Holds every later frame of head `name` at a closed gate, once no frame
/// of the head is out: a frame composed before the hook went in would come
/// past the gate. No turn runs between the wait and the hook, so no frame
/// starts between them.
fn hold_frames(s: &mut Seat, name: &str) -> (Arc<Gate>, Opens) {
    assert!(
        s.until(BOUND, |a| a.head(name).is_some_and(|h| h.frames_out() == 0)),
        "no frame of head {name} is out"
    );
    let (gate, hook, opens) = Gate::closed();
    s.app.set_draw_hook(name, Some(hook));
    (gate, opens)
}

/// No frits and no vconsole font, so the seat draws its built-in font
/// whatever the machine running the tests holds.
fn no_fonts() -> Sources {
    Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        trust: vtium::font::trust(),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

/// The small display and the large one, each its own device.
fn two_displays() -> [(&'static str, Memory); 2] {
    [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ]
}

struct Seat {
    lp: Loop<App>,
    app: App,
    /// How many VT releases vtium has acknowledged.
    released: Rc<Cell<u32>>,
    /// Dropped after the app, as fields drop in order.
    _reap: Reap,
}

/// Joins, as the harness drops after its app, the render threads the app
/// went on without because their GPU driver had not let go in time
/// (ADR-1303 D9). No such thread is then inside the GPU driver when the
/// test binary runs its exit handlers, which would tear libEGL and Mesa
/// down under it, as the gpu harness's seat does too.
struct Reap;

impl Drop for Reap {
    fn drop(&mut self) {
        let running = vtium::painter::wait_for_left(BOUND, &now);
        if !std::thread::panicking() {
            assert_eq!(running, 0, "a render thread left running");
        }
    }
}

impl Drop for Seat {
    /// Ends the session of each head's shown console, with its process
    /// group, and reaps it, before the seat's app drops. The app's ptys,
    /// dropped, send their sessions SIGHUP and wait for none of them, so
    /// without this a session could outlive its test.
    fn drop(&mut self) {
        let sessions: Vec<u32> = self
            .app
            .heads()
            .iter()
            .filter_map(|h| h.pty().and_then(|p| p.child_id()))
            .collect();
        for pid in sessions {
            end_session(pid);
        }
    }
}

/// Kills the session `pid` leads, its whole process group, with SIGKILL,
/// and reaps the leader. `pid` is a pty's child id, which the pty holds
/// until it reaps the child, so no other process can have taken it: the
/// pidfd opened here names the session.
fn end_session(pid: u32) {
    use rustix::io::Errno;
    use rustix::process::{PidfdFlags, Signal, WaitId, WaitIdOptions};
    use std::os::fd::AsFd;
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return;
    };
    let Ok(pidfd) = rustix::process::pidfd_open(pid, PidfdFlags::empty()) else {
        return;
    };
    // The session's leader leads its process group too.
    let _ = rustix::process::kill_process_group(pid, Signal::KILL);
    while let Err(Errno::INTR) =
        rustix::process::waitid(WaitId::PidFd(pidfd.as_fd()), WaitIdOptions::EXITED)
    {}
}

impl Seat {
    /// The seat booted with `args` at 16 points, 16 pixels at 72 DPI, and
    /// `displays`, its session running `script`.
    fn boot(
        args: &[&str],
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
    ) -> Seat {
        Seat::with_heads(args, None, displays, script)
    }

    /// [`Seat::boot`], with the head file `heads`.
    fn with_heads(
        args: &[&str],
        heads: Option<&str>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
    ) -> Seat {
        Seat::with_fonts(args, heads, displays, script, no_fonts())
    }

    /// [`Seat::with_heads`], its fonts' sets read from `fonts`.
    fn with_fonts(
        args: &[&str],
        heads: Option<&str>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
        fonts: Sources,
    ) -> Seat {
        let rules = heads.map(|text| {
            let start = start(text);
            assert_eq!(start.line(), None, "a valid head file");
            start
        });
        Seat::with_rules(args, rules, displays, script, fonts)
    }

    /// [`Seat::with_fonts`], running what the start's head file resolved
    /// to, as [`vtium::head::rules`] gives it, on the VT it names, else on
    /// VT 1.
    fn with_rules(
        args: &[&str],
        rules: Option<Start>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
        fonts: Sources,
    ) -> Seat {
        Seat::set_up(args, rules, displays, script, fonts, |_| {})
    }

    /// [`Seat::with_rules`], with `set` run on the app before it starts.
    fn set_up(
        args: &[&str],
        rules: Option<Start>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
        fonts: Sources,
        set: impl FnOnce(&mut App),
    ) -> Seat {
        let args: Vec<OsString> = ["--font-size=16"]
            .iter()
            .chain(args)
            .chain(&["--login", "--", "/bin/sh", "-c", script])
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the seat's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        // the VT the start builds the heads of, VT 1 where it names none
        let number = rules.as_ref().and_then(Start::vt).unwrap_or(1);
        let (vt, released) = CountingVt::new(FakeVt::with_number(number));
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), Rc::new(now), env);
        app.set_font_sources(fonts);
        if let Some(start) = rules {
            app.set_heads(start);
        }
        for (node, memory) in displays {
            app.add_video(PathBuf::from(node), Box::new(memory));
        }
        set(&mut app);
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat {
            lp,
            app,
            released,
            _reap: Reap,
        }
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

    /// The memory device at `node`.
    fn memory(&mut self, node: &str) -> &mut Memory {
        self.app
            .videos
            .get_mut(Path::new(node))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("a memory device")
    }

    /// Turns the loop until the memory device at `node` satisfies `done`,
    /// at most `limit`.
    fn until_memory(
        &mut self,
        node: &str,
        limit: Duration,
        done: impl Fn(&Memory) -> bool,
    ) -> bool {
        let end = now() + limit;
        while !done(self.memory(node)) {
            if now() > end {
                return false;
            }
            self.until(Duration::from_millis(20), |_| false);
        }
        true
    }

    /// Types `text` as a keyboard does.
    fn type_str(&mut self, text: &str) {
        for c in text.chars() {
            self.app.input_event(InputEvent::Key(char_key(c)), now());
        }
    }

    /// Types `text` on the keyboard at `node`.
    fn type_on(&mut self, node: &str, text: &str) {
        for c in text.chars() {
            self.app
                .device_input(Path::new(node), InputEvent::Key(char_key(c)), now());
        }
    }
}

/// What the start runs for a trusted head file holding `text`.
fn start(text: &str) -> Start {
    Start::trusted(Path::new("heads.conf"), text.as_bytes())
}

/// The key that types `c`, `\r` Return.
fn char_key(c: char) -> KeyEvent {
    let keysym = if c == '\r' { 0xff0d } else { u32::from(c) };
    KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::empty(),
        keysyms: vec![keysym],
        codepoints: vec![(c != '\r').then_some(c)],
        repeat: false,
    }
}

/// A keyboard at the USB port `path`, as the rules read it.
fn keyboard(path: &str) -> vtium_settings::Identity {
    vtium_settings::Identity {
        path: Some(path.to_owned()),
        ..vtium_settings::Identity::input().with_class(Class::Keyboard)
    }
}

/// Whether a row of the shown screen holds `text`.
fn shows(app: &App, text: &str) -> bool {
    app.screen_text().iter().any(|l| l.contains(text))
}

/// Whether a row of head `name`'s screen holds `text`.
fn head_shows(app: &App, name: &str, text: &str) -> bool {
    app.head(name)
        .is_some_and(|h| h.screen_text().iter().any(|l| l.contains(text)))
}

/// D11's first row, written against the code before heads and kept after
/// them: with no head file every display lights and mirrors one grid by
/// the scaled rule, and the keys of every keyboard type into the one
/// session. The keys reach the seat as the input drains them, each
/// keyboard's after the other's, with no device named, so "every
/// keyboard" here is two keyboards' keys in turn.
#[test]
fn with_no_head_file_every_display_mirrors_one_grid_and_every_keyboard_types() {
    let script = "read a; echo \"one:$a\"; read b; echo \"two:$b\"; read c";
    let mut s = Seat::boot(&[], two_displays(), script);
    assert!(
        s.until(BOUND, |a| a.cells().len() == 2),
        "both displays lit"
    );
    // 320x160 holds 40x10 cells of 8x16; 640x320 holds 40x10 of 16x32.
    assert_eq!(s.app.grid(), (40, 10));
    assert_eq!(s.app.cells(), [(8, 16), (16, 32)]);
    s.type_str("a\r");
    assert!(
        s.until(BOUND, |a| shows(a, "one:a")),
        "the first keyboard types"
    );
    s.type_str("b\r");
    assert!(
        s.until(BOUND, |a| shows(a, "two:b")),
        "the second keyboard types"
    );
}

/// A head for each display, by its card.
const TWO_HEADS: &str = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input class:keyboard -> roaming
";

/// The grid of the head `name`.
fn grid(app: &App, name: &str) -> Option<(usize, usize)> {
    app.head(name).map(Head::grid)
}

/// What the session of head `name` printed of its size, as `stty size`
/// prints it.
fn size(app: &App, name: &str) -> Option<String> {
    let text = app.head(name)?.screen_text();
    let line = text.iter().find_map(|l| l.strip_prefix("size:"))?;
    Some(line.to_owned())
}

/// D1: a head's grid is its own. With a head for each display, each draws
/// its own grid, 320x160 holding 40x10 cells of 8x16 and 640x320 holding
/// 80x20, where one head mirrors both on 40x10; each head's session runs
/// on its own grid.
#[test]
fn two_heads_draw_two_grids_at_their_own_density() {
    let script = "echo \"size:$(stty size)\"; read x";
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), two_displays(), script);
    assert!(
        s.until(BOUND, |a| a.cells().len() == 2),
        "both displays lit"
    );
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(8, 16)]));
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(8, 16)]));
    assert!(s.until(BOUND, |a| size(a, "small").is_some()
        && size(a, "large").is_some()));
    assert_eq!(size(&s.app, "small").as_deref(), Some("10 40"));
    assert_eq!(size(&s.app, "large").as_deref(), Some("20 80"));
}

/// D1: a size in points converts at each head's own density. 16 points
/// on the large display, at 144 DPI, are 32 pixels in a head of its own,
/// a 16x32 cell on 40x10; mirrored with the small one at 72 DPI, the
/// lowest density converts and the large display draws its 16x32 cell on
/// the small one's 40x10 by the scaled rule.
#[test]
fn a_heads_points_convert_at_its_own_displays_density() {
    let displays = || {
        [
            ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
            (
                "/dev/mem1",
                Memory::new(640, 320, "MEM-2", "mem-large").at(144.0),
            ),
        ]
    };
    let mut one = Seat::boot(&[], displays(), "read x");
    assert!(one.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(one.app.grid(), (40, 10));
    assert_eq!(one.app.cells(), [(8, 16), (16, 32)]);
    let mut two = Seat::with_heads(&[], Some(TWO_HEADS), displays(), "read x");
    assert!(two.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(grid(&two.app, "small"), Some((40, 10)));
    assert_eq!(grid(&two.app, "large"), Some((40, 10)));
    assert_eq!(two.app.cells(), [(8, 16), (16, 32)]);
}

/// ADR-1401 D4: an `edid:` rule places a display by its EDID's maker,
/// product code and serial, so two monitors of one model on one card go
/// to the heads their serials name.
#[test]
fn edid_rules_place_two_monitors_of_one_model_by_their_serials() {
    let heads = "version = 1
[head small]
[head large]
[rules]
display edid:AAA:0x0001:0x00000002 -> large
display edid:AAA:0x0001:0x00000001 -> small
";
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-card").named(1),
        ),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-card").named(2),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    assert!(
        s.until(BOUND, |a| a.cells().len() == 2),
        "both displays lit"
    );
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
}

/// D3: a display a rule sends `dark` and a display no rule claims are
/// left off, set dark, and join no head.
#[test]
fn a_display_sent_dark_or_claimed_by_no_rule_is_left_off() {
    let heads = "version = 1
[head small]
[rules]
display card:mem-small -> small
display card:mem-large -> off
";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
        ("/dev/mem2", Memory::new(320, 240, "MEM-3", "mem-other")),
    ];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0),
        "the small display draws"
    );
    assert_eq!(s.app.cells(), [(8, 16)]);
    for node in ["/dev/mem1", "/dev/mem2"] {
        let m = s.memory(node);
        assert_eq!((m.dpms, m.frames), (Some(Dpms::Off), 0), "{node} is dark");
    }
}

/// D5: every keyboard types into the head holding the focus, which starts
/// on the head of the boot display, the connector lit at start, and no
/// other head's session reads the keys.
#[test]
fn every_keyboard_types_into_the_head_of_the_boot_display() {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let script = "echo ready; read a; echo \"got:$a\"; read b";
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, script);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    s.type_str("k\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:k")),
        "the boot display's head reads the keys"
    );
    s.until(Duration::from_millis(200), |_| false);
    assert!(
        !head_shows(&s.app, "small", "got:"),
        "the other head reads none"
    );
}

/// D1: a zoom and a rotation change the grid of the head holding the
/// focus, and no other head's.
#[test]
fn a_zoom_and_a_rotation_change_the_focused_heads_grid_alone() {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    let key = |mods: Mods, keysym: u32| {
        InputEvent::Key(KeyEvent {
            keycode: 0,
            ascii: 0,
            mods,
            keysyms: vec![keysym],
            codepoints: vec![None],
            repeat: false,
        })
    };
    let (equal, minus) = (0x3d, 0x2d);
    s.app.input_event(key(Mods::CTRL, equal), now());
    assert_eq!(
        grid(&s.app, "large"),
        Some((40, 10)),
        "zoomed in to 16x32 cells"
    );
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 10)),
        "the other head keeps its grid"
    );
    s.app.input_event(key(Mods::CTRL, minus), now());
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
    // Logo+= turns the focused head's display a quarter: 320 across, 640
    // down.
    s.app.input_event(key(Mods::LOGO, equal), now());
    assert_eq!(grid(&s.app, "large"), Some((40, 40)));
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
}

/// D1: a head whose displays never appear starts no session until one
/// does, under `display-wait=forever` too.
#[test]
fn a_head_with_no_display_starts_no_session() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(
        &["--display-wait=forever"],
        Some(TWO_HEADS),
        displays,
        "echo ready; read x",
    );
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.head("small").map(Head::started), Some(true));
    assert_eq!(
        s.app.head("large").map(Head::started),
        Some(false),
        "no display, no session"
    );
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large")),
    );
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "ready")),
        "its first display starts it"
    );
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
}

/// The frits `names`, each of one built-in source, in a directory of its
/// own, and a stand-in helper that writes its pid to the directory's
/// `starts` as it starts and answers nothing; the sources and the file.
fn frits(test: &str, names: &[&str]) -> (Scratch, Sources, PathBuf) {
    let dir = Scratch::new(&format!("heads-{test}"));
    for name in names {
        std::fs::write(
            dir.join(format!("{name}.vtfrit")),
            "source=builtin:unifont\n",
        )
        .expect("a frit");
    }
    let starts = dir.join("starts");
    let helper = vtium::helper::Command {
        program: stand_in::stand_in_shell().to_path_buf(),
        args: ["-c", "echo $$ >> \"$1\"; exec sleep 30", "helper"]
            .into_iter()
            .map(OsString::from)
            .chain([starts.clone().into_os_string()])
            .collect(),
        account: None,
    };
    let fonts = Sources {
        frits: dir.to_path_buf(),
        helper: Some(helper),
        ..no_fonts()
    };
    (dir, fonts, starts)
}

/// D3 and D6, ADR-1403 D5: the more specific setting wins. A head's look
/// names its frit, over the top level's font; a head with the default look
/// takes the top level's, the built-in font here. A head has no font key
/// of its own.
#[test]
fn a_heads_look_names_its_frit_over_the_top_levels_font() {
    let (_dir, fonts, starts) = frits("profile", &["t"]);
    let heads = "version = 1
[look t]
font-name = t
[head small]
[head large]
look = t
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(&[], Some(heads), two_displays(), "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(8, 16)]));
    // both at vtium.conf's size, 16
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(8, 16)]));
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
    assert_eq!(
        s.app.font_name(),
        "unifont",
        "the small head draws the built-in"
    );
    assert!(
        s.until(BOUND, |_| starts.exists()),
        "the large head's frit started its helper"
    );
    drop(s);
}

/// ADR-1403 D5: a look's `font-size` and `display-dpi` reach the head
/// whose section names the look, over the top level's, and a head whose
/// section names no look draws at the top level's. The seat's command
/// line sets 16 points, which a section wins over too. The small head's
/// look sets 144 DPI, so 16 points draw a 16x32 cell on the 72 DPI small
/// display; the large head's look sets 32 points, a 16x32 cell at 72 DPI.
/// A head's section sets no font key of its own, since a head's font
/// comes from its look.
#[test]
fn a_heads_look_sets_the_size_its_head_draws_at() {
    let heads = "version = 1
[look dense]
display-dpi = 144
[look big]
font-size = 32
[head small]
look = dense
[head large]
look = big
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let script = "echo \"size:$(stty size)\"; read x";
    let mut s = Seat::with_heads(&[], Some(heads), two_displays(), script);
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(16, 32)]),
        "the look's display-dpi"
    );
    assert_eq!(
        s.app.head("large").map(Head::cells),
        Some(vec![(16, 32)]),
        "the look's font-size"
    );
    assert_eq!(grid(&s.app, "small"), Some((20, 5)));
    assert_eq!(grid(&s.app, "large"), Some((40, 10)));
    assert!(s.until(BOUND, |a| size(a, "small").is_some()
        && size(a, "large").is_some()));
    assert_eq!(size(&s.app, "small").as_deref(), Some("5 20"));
    assert_eq!(size(&s.app, "large").as_deref(), Some("10 40"));
    // a head section with no look draws at the top level's 16 points
    let plain = "version = 1
[look big]
font-size = 32
[head small]
[head large]
look = big
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_heads(&[], Some(plain), two_displays(), "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(8, 16)]));
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(16, 32)]));
}

/// ADR-1403 D5, ADR-1200 D16: a look's `cell` reaches the head whose
/// section names the look. It wins over the cell its frit states, 10x20,
/// and over the seat's 16 points, so the small head draws 12x24 cells on
/// its 320x160 display. The small head's frit is an outline frit, which
/// would draw at the 16 points' height were the cell to lose to them.
/// Within one look, the look's `font-size` wins over its `cell`: the large
/// head's look sets both, and the head draws at 32 points, a 16x32 cell at
/// 72 DPI, as with no cell at all.
#[test]
fn a_heads_look_sets_its_cell_and_the_looks_size_wins_over_it() {
    let (dir, fonts, _starts) = frits("look-cell", &[]);
    std::fs::write(dir.join("c.vtfrit"), "cell=10x20\nsource=builtin:unifont\n")
        .expect("a frit that states its cell");
    std::fs::write(
        dir.join("o.vtfrit"),
        "cell=10x20\nsource=outline:/nonexistent/vtium-test.ttf\n",
    )
    .expect("an outline frit that states its cell");
    let heads = "version = 1
[look cell]
font-name = o
cell = 12x24
[look both]
font-name = c
cell = 12x24
font-size = 32
[head small]
look = cell
[head large]
look = both
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(&[], Some(heads), two_displays(), "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(12, 24)]),
        "the look's cell"
    );
    assert_eq!(grid(&s.app, "small"), Some((26, 6)));
    assert_eq!(
        s.app.head("large").map(Head::cells),
        Some(vec![(16, 32)]),
        "the look's font-size"
    );
    assert_eq!(grid(&s.app, "large"), Some((40, 10)));
    drop(s);
}

/// ADR-1403 D5, ADR-1200 D5: a look's `cell` holds across an OSC 50
/// switch. The head's look draws frit `c` in a 12x24 cell, which
/// `a_heads_look_sets_its_cell_and_the_looks_size_wins_over_it` checks;
/// the program switches to frit `d`, which states a 10x20 cell of its own, and `d`
/// draws in the look's 12x24 cell too, since the look is more specific
/// than any frit the head draws. A switch to a built-in font, `unifont`,
/// draws it in the look's cell as well (ADR-1200 D7).
#[test]
fn an_osc_50_switch_keeps_the_looks_cell() {
    let (dir, fonts, _starts) = frits("look-cell-osc50", &[]);
    for name in ["c", "d"] {
        std::fs::write(
            dir.join(format!("{name}.vtfrit")),
            "cell=10x20\nsource=builtin:unifont\n",
        )
        .expect("a frit that states its cell");
    }
    let heads = "version = 1
[look cell]
font-name = c
cell = 12x24
[head small]
look = cell
[rules]
display card:mem-small -> small
";
    let script = "printf '\\033]50;d\\007'; echo switched; read b";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(heads), [small], script, fonts.clone());
    assert!(s.until(BOUND, |a| head_shows(a, "small", "switched")));
    assert_eq!(s.app.font_name(), "d", "the switch took");
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(12, 24)]),
        "the new frit draws in the look's cell"
    );
    drop(s);
    let script = "printf '\\033]50;unifont\\007'; echo switched; read b";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(heads), [small], script, fonts);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "switched")));
    assert_eq!(s.app.font_name(), "unifont", "the switch took");
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(12, 24)]),
        "the built-in font draws in the look's cell"
    );
    drop(s);
}

/// ADR-1403 D5, ADR-1200 D5, D9: a look's `font-name` that no frit bears
/// draws a frit made on the fly under that name, and OSC 50 switches back
/// to it by the name, in the look's cell. The look names the kbd font
/// `ter-v16n` and states a 12x24 cell, which wins over the 8x16 cell the
/// display's density gives; the program switches to frit `d`, and back by
/// the name `ter-v16n`, and the head draws that font in the look's cell on
/// the grid it started on.
#[test]
fn an_osc_50_switch_returns_to_a_looks_font_by_its_name() {
    let (_dir, fonts, _starts) = frits("look-made-osc50", &["d"]);
    let heads = "version = 1
[look made]
font-name = ter-v16n
cell = 12x24
[head small]
look = made
[rules]
display card:mem-small -> small
";
    let script = "echo ready; read a; \
                  printf '\\033]50;d\\007'; echo away; read b; \
                  printf '\\033]50;ter-v16n\\007'; echo back; read c";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(heads), [small], script, fonts);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    assert_eq!(s.app.font_name(), "ter-v16n");
    let started = (s.app.head("small").map(Head::cells), grid(&s.app, "small"));
    assert_eq!(started.0, Some(vec![(12, 24)]), "the look's cell");
    s.type_str("x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "away")));
    assert_eq!(s.app.font_name(), "d", "the switch took");
    s.type_str("y\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "back")));
    assert_eq!(s.app.font_name(), "ter-v16n", "the switch back took");
    assert_eq!(
        (s.app.head("small").map(Head::cells), grid(&s.app, "small")),
        started
    );
}

/// ADR-1200 D7, D17, ADR-1403 D5: a head that draws the built-in fonts
/// alone draws them in the cell its look states, fitted to that cell, as
/// it would draw a frit, and not at the display's automatic size. The
/// look of the small head names the frit `bad`, whose `cell=` line vtium
/// cannot read, so vtium draws the built-in fonts; the look of the large
/// head names no font, and nothing else names one. Both looks state a
/// 12x24 cell, which wins over the seat's 16 points, so the small head's
/// 320x160 display holds 26 by 6 cells and the large head's 640x320
/// display 53 by 13. A look that names a built-in font, `unscii-8`, draws
/// it in the look's cell too.
#[test]
fn the_built_in_fonts_alone_draw_in_the_looks_cell() {
    let (dir, fonts, _starts) = frits("builtin-cell", &[]);
    std::fs::write(
        dir.join("bad.vtfrit"),
        "cell=wide\nsource=builtin:unifont\n",
    )
    .expect("a frit whose cell cannot be read");
    let heads = "version = 1
[look unread]
font-name = bad
cell = 12x24
[look none]
cell = 12x24
[head small]
look = unread
[head large]
look = none
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(&[], Some(heads), two_displays(), "read x", fonts.clone());
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(12, 24)]),
        "a frit vtium cannot read"
    );
    assert_eq!(grid(&s.app, "small"), Some((26, 6)));
    assert_eq!(
        s.app.head("large").map(Head::cells),
        Some(vec![(12, 24)]),
        "no font named"
    );
    assert_eq!(grid(&s.app, "large"), Some((53, 13)));
    drop(s);
    let named = "version = 1
[look small]
font-name = unscii-8
cell = 12x24
[head small]
look = small
[rules]
display card:mem-small -> small
";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(named), [small], "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(
        s.app.head("small").map(Head::cells),
        Some(vec![(12, 24)]),
        "a built-in font named"
    );
    drop(s);
    // A 6x16 cell is too narrow for unifont's 8 columns, so Unscii-8
    // draws in it, and OSC 50's query names Unscii-8.
    let narrow = "version = 1
[look narrow]
cell = 6x16
[head small]
look = narrow
[rules]
display card:mem-small -> small
";
    let (_dir, fonts, _starts) = frits("builtin-narrow", &[]);
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(narrow), [small], "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(6, 16)]));
    assert_eq!(s.app.font_name(), "unscii-8", "the strike that draws");
    drop(s);
}

/// ADR-1200 D9, ADR-1403 D5: a look whose `font-name` names no frit in the
/// frits' directory names a kbd font, and vtium makes a one-source frit
/// for it on the fly. That frit draws in the look's 12x24 cell, and the
/// built-in fonts draw its glyphs while no helper sends any.
#[test]
fn a_look_naming_a_frit_with_no_file_draws_in_the_looks_cell() {
    let (_dir, fonts, _starts) = frits("look-no-frit", &[]);
    let heads = "version = 1
[look missing]
font-name = no-such-frit
cell = 12x24
[head small]
look = missing
[rules]
display card:mem-small -> small
";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(heads), [small], "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(12, 24)]));
    assert_eq!(grid(&s.app, "small"), Some((26, 6)));
    assert_eq!(s.app.font_name(), "no-such-frit");
    drop(s);
}

/// ADR-1200 D5, D16: a configured `font-size` wins over the cell of a frit
/// that an OSC 50 switch draws, as it does over the frit vtium starts with.
/// At the seat's 72 DPI, 32 points are 32 pixels; the head starts in the
/// built-in fonts at that size, and the program switches to frit `c`,
/// which states a 10x20 cell, and `c` draws in the 16x32 cell of 32
/// pixels.
#[test]
fn an_osc_50_switch_under_a_configured_font_size_drops_the_frits_cell() {
    let (dir, fonts, _starts) = frits("osc50-sized", &[]);
    std::fs::write(dir.join("c.vtfrit"), "cell=10x20\nsource=builtin:unifont\n")
        .expect("a frit that states its cell");
    let heads = "version = 1
[head small]
[rules]
display card:mem-small -> small
";
    let script = "printf '\\033]50;c\\007'; echo switched; read b";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&["--font-size=32"], Some(heads), [small], script, fonts);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "switched")));
    assert_eq!(s.app.font_name(), "c", "the switch took");
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(16, 32)]));
    drop(s);
}

/// ADR-1200 D16, D17: a head that draws the built-in fonts alone, with no
/// look's cell, draws them at the top level's `font-size`. At the seat's
/// 72 DPI, 32 points are 32 pixels, unifont's 16x32 cell, so the small
/// head's 320x160 display holds 20 by 5 cells. Its look names the frit
/// `bad`, which vtium cannot read; the large head has no look.
#[test]
fn the_built_in_fonts_alone_draw_at_the_top_levels_font_size() {
    let (dir, fonts, _starts) = frits("builtin-size", &[]);
    std::fs::write(
        dir.join("bad.vtfrit"),
        "cell=wide\nsource=builtin:unifont\n",
    )
    .expect("a frit whose cell cannot be read");
    let heads = "version = 1
[look unread]
font-name = bad
[head small]
look = unread
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(
        &["--font-size=32"],
        Some(heads),
        two_displays(),
        "read x",
        fonts,
    );
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(16, 32)]));
    assert_eq!(grid(&s.app, "small"), Some((20, 5)));
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(16, 32)]));
    drop(s);
}

/// ADR-1200 D16, ADR-1403 D5: within one look, its `font-size` wins over
/// its `cell` on the built-in fonts too. The look names the frit `bad`,
/// which vtium cannot read, states a 12x24 cell and sets 32 points, so the
/// head draws the built-in fonts at 32 pixels, unifont's 16x32 cell, and
/// not in the 12x24 cell.
#[test]
fn within_one_look_its_font_size_wins_over_its_cell_on_the_built_in_fonts() {
    let (dir, fonts, _starts) = frits("builtin-both", &[]);
    std::fs::write(
        dir.join("bad.vtfrit"),
        "cell=wide\nsource=builtin:unifont\n",
    )
    .expect("a frit whose cell cannot be read");
    let heads = "version = 1
[look both]
font-name = bad
cell = 12x24
font-size = 32
[head small]
look = both
[rules]
display card:mem-small -> small
";
    let [small, _] = two_displays();
    let mut s = Seat::with_fonts(&[], Some(heads), [small], "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(16, 32)]));
    assert_eq!(grid(&s.app, "small"), Some((20, 5)));
    drop(s);
}

/// ADR-1200 D17: a bitmap frit asks for its fonts' own cell at an OSC 50
/// switch. Switched away and back while another head still draws it, a frit
/// whose last ask has had no answer asks again; one the helper refused
/// keeps the refusal and asks nothing, a refusal that came after the wait
/// included (the maintainer, 2026-11-03). The helper is the real one, so
/// each query gets one answer: its frit names a kbd font no machine has,
/// which it refuses. Held, it answers nothing until the test opens its go
/// FIFO, so the start's ask has no answer in time whatever the machine's
/// speed. In the `refused` case the test then lets the helper answer, and
/// waits until vtium has kept its late refusal of the start's ask; neither
/// switch back asks again. In the `timeout` case the helper stays held, and
/// each switch back asks again.
// covers: text.font-change
#[test]
fn an_osc_50_switch_back_asks_again_after_a_timeout_but_not_a_refusal() {
    for (case, refused) in [("timeout", false), ("refused", true)] {
        let dir = scratch_dir(&format!("osc50-{case}"));
        std::fs::write(dir.join("t.vtfrit"), "source=kbd:vtium-no-such-font\n").expect("the frit");
        let (kinds, go) = (dir.join("kinds"), dir.join("go"));
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &go,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .expect("the go FIFO");
        let fonts = Sources {
            frits: dir.to_path_buf(),
            helper: Some(real_helper(&kinds, Some(go.as_path()))),
            ..no_fonts()
        };
        let heads = "version = 1
[look t]
font-name = t
[head a]
look = t
[head b]
look = t
[rules]
display card:mem-small -> a
display card:mem-large -> b
input class:keyboard -> roaming
";
        let [(small_node, small), large] = two_displays();
        let displays = [(small_node, small.lit()), large];
        let switch = "printf '\\033]50;unifont\\007'; echo away; read b; \
                      printf '\\033]50;t\\007'; echo back; read c; \
                      printf '\\033]50;unifont\\007'; echo gone; read d; \
                      printf '\\033]50;t\\007'; echo again; read e";
        let script = format!("echo ready; read a; {switch}");
        logged();
        let mut s = Seat::with_fonts(&["--font-size=0"], Some(heads), displays, &script, fonts);
        assert!(s.until(BOUND, |a| head_shows(a, "a", "ready")), "{case}");
        // Each call of `logged` takes the lines logged since the last.
        let unknown = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .filter(|l| l.contains("own cell is not known"))
                .cloned()
                .collect()
        };
        let late = |line: &String| line.contains("gave no answer");
        // The frit `t` holds the helper's refusal at every height asked.
        let refusal_kept = |a: &App| {
            let known = a.frit_cells("t");
            !known.is_empty()
                && known
                    .iter()
                    .all(|(_, k)| k.as_ref().is_err_and(|no| !no.late))
        };
        let mut seen = unknown(&logged());
        assert_eq!(seen.len(), 1, "{case}: the start asked once for both heads");
        assert!(
            late(&seen[0]),
            "{case}: the held helper answered nothing: {seen:?}"
        );
        if refused {
            // The helper waits on the FIFO with its answer, so an open to
            // write succeeds once it does.
            let open = |_: &App| {
                rustix::fs::open(
                    &go,
                    rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK,
                    rustix::fs::Mode::empty(),
                )
                .is_ok()
            };
            assert!(s.until(BOUND, open), "the helper answers");
            assert!(
                s.until(BOUND, |a| refusal_kept(a)),
                "the start's ask refused, late, and kept: {:?}",
                s.app.frit_cells("t")
            );
        }
        // Away and back, then away and back again.
        for (n, (away, back)) in [("away", "back"), ("gone", "again")]
            .into_iter()
            .enumerate()
        {
            s.type_str("x\r");
            assert!(s.until(BOUND, |a| head_shows(a, "a", away)), "{case}");
            assert_eq!(s.app.font_name(), "unifont", "{case}");
            s.type_str("y\r");
            assert!(s.until(BOUND, |a| head_shows(a, "a", back)), "{case}");
            assert_eq!(s.app.font_name(), "t", "{case}");
            seen.extend(unknown(&logged()));
            let asked = if refused { 0 } else { n + 1 };
            assert_eq!(
                seen.len() - 1,
                asked,
                "{case}: switch back {}: {seen:?}",
                n + 1
            );
        }
        if refused {
            assert!(refusal_kept(&s.app), "{:?}", s.app.frit_cells("t"));
            let refusal = vtium_config::serve::FRAME_NO_OWN_CELL.to_string();
            let refusals = std::fs::read_to_string(&kinds)
                .expect("the helper's frames")
                .lines()
                .filter(|l| *l == refusal)
                .count();
            assert_eq!(refusals, 1, "the helper was asked once");
        } else {
            assert!(seen.iter().all(late), "{seen:?}");
        }
        drop(s);
    }
}

/// D6: with no head file the seat runs the compiled-in configuration,
/// the parse of the template: its one `console` head.
#[test]
fn with_no_head_file_the_seat_runs_the_compiled_in_heads() {
    let s = Seat::boot(&[], two_displays(), "read x");
    let names: Vec<&str> = s.app.heads().iter().map(Head::name).collect();
    let builtin = vtium_settings::Config::builtin();
    let want: Vec<&str> = builtin.heads.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, want);
    assert_eq!(names, ["console"]);
}

/// D1: one glyph helper for each frit in use. Two heads drawing the frit
/// `t` share its helper, and a head drawing the frit `u` has its own; each
/// helper the stand-in starts writes a line. The seat gone, each helper is
/// killed and reaped, so no process, a zombie included, keeps its pid.
#[test]
fn heads_drawing_one_frit_share_its_helper_and_another_frit_has_its_own() {
    let (_dir, fonts, starts) = frits("share", &["t", "u"]);
    let heads = "version = 1
[look t]
font-name = t
[look u]
font-name = u
[head a]
look = t
[head b]
look = t
[head c]
look = u
[rules]
display card:mem-a -> a
display card:mem-b -> b
display card:mem-c -> c
";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-a")),
        ("/dev/mem1", Memory::new(320, 160, "MEM-2", "mem-b")),
        ("/dev/mem2", Memory::new(320, 160, "MEM-3", "mem-c")),
    ];
    let mut s = Seat::with_fonts(&[], Some(heads), displays, "read x", fonts);
    assert!(s.until(BOUND, |a| a.cells().len() == 3));
    assert_eq!(s.app.font_name(), "t");
    let started = || {
        std::fs::read_to_string(&starts)
            .map(|t| t.lines().count())
            .unwrap_or(0)
    };
    assert!(s.until(BOUND, |_| started() >= 2), "the helpers started");
    s.until(Duration::from_millis(300), |_| false);
    assert_eq!(started(), 2, "one helper for t, one for u");
    let pids: Vec<String> = std::fs::read_to_string(&starts)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    drop(s);
    let gone = |pid: &String| !Path::new(&format!("/proc/{pid}")).exists();
    let end = now() + BOUND;
    while now() < end && !pids.iter().all(gone) {
        std::thread::yield_now();
    }
    assert!(
        pids.iter().all(gone),
        "each helper killed and reaped: {pids:?}"
    );
}

/// D3: a display the rules leave dark is never unblanked. A framebuffer's
/// display left dark stays off when the blank ends, and only the lit
/// display powers up again.
#[test]
fn the_end_of_the_blank_never_unblanks_a_dark_display() {
    let heads = "version = 1
[head small]
[rules]
display card:mem-small -> small
display card:mem-large -> off
";
    let mut s = Seat::with_heads(&["--dpms-timeout=1"], Some(heads), two_displays(), "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.dpms == Some(Dpms::Off)),
        "blanked after dpms-timeout"
    );
    let before = (
        s.memory("/dev/mem1").unblanks,
        s.memory("/dev/mem1").dpms_sets.len(),
    );
    s.type_str("x");
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::On),
        "the lit display wakes"
    );
    let dark = s.memory("/dev/mem1");
    assert_eq!(
        dark.unblanks, before.0,
        "the dark display is never unblanked"
    );
    assert!(
        !dark.dpms_sets.iter().skip(before.1).any(|&d| d == Dpms::On),
        "nor powered on: {:?}",
        dark.dpms_sets
    );
    assert_eq!(dark.dpms, Some(Dpms::Off));
}

/// The session's script for the tests of vtium/vtium#208: it counts the
/// SIGWINCHs it gets, and at each line typed prints `got:`, the line, the
/// count and the pty's size as `stty size` gives it. A read a signal ends
/// reads again.
const MODE_SCRIPT: &str = "n=0; trap 'n=$((n+1))' WINCH; echo ready; \
    while :; do m=$n; \
    if read x; then echo \"got:$x winch:$n $(stty size)\"; \
    elif [ $n = $m ]; then exit; fi; done";

/// What the session printed for the last line typed: the line, the
/// SIGWINCHs it counted, and the pty's rows and columns.
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

/// The last row of the shown screen whose text starts with `start`.
fn row_of(app: &App, start: &str) -> Option<usize> {
    app.screen_text().iter().rposition(|l| l.starts_with(start))
}

/// Whether the display shows a lit pixel on the cell row `row`, 16 pixels
/// high, right of `x`, on a display `width` pixels wide.
fn lit_right_of(m: &Memory, width: usize, row: usize, x: usize) -> bool {
    (row * 16..(row + 1) * 16).any(|y| {
        (x..width).any(|px| {
            let i = (y * width + px) * 4;
            m.pixels
                .get(i..i + 4)
                .is_some_and(|p| p.iter().any(|&b| b != 0))
        })
    })
}

/// Types a line of 50 characters, wider than the old grid's 40 columns,
/// and waits until the session prints it back; then waits until the
/// display shows the printed line's text right of x = 330, past the old
/// width of 320, which only a renderer at the new size draws.
fn shows_a_long_line(s: &mut Seat) -> (u32, String) {
    let long = "x".repeat(50);
    s.type_str(&format!("{long}\r"));
    assert!(
        s.until(BOUND, |a| got(a).is_some_and(|g| g.0 == long)),
        "the session prints the long line"
    );
    let row = row_of(&s.app, &format!("got:{long}")).expect("the printed line's row");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| lit_right_of(m, 640, row, 330)),
        "the long line is drawn past the old width, on row {row}"
    );
    let (_, winch, size) = got(&s.app).expect("the printed line");
    (winch, size)
}

/// vtium/vtium#208: a display whose mode changes in place while vtium
/// runs, the same connector at a new size, is laid out again at that size.
/// The grid grows to fill it, the session gets one SIGWINCH and the new
/// size, the display's next frame is drawn whole, and its renderer draws
/// the whole new width.
#[test]
fn a_display_whose_mode_changes_in_place_draws_whole_at_its_new_size() {
    let small = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::boot(&[], small, MODE_SCRIPT);
    assert!(s.until(BOUND, |a| shows(a, "ready")), "the session runs");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0),
        "the display draws"
    );
    // 320x160 holds 40x10 cells of 8x16.
    assert_eq!(s.app.grid(), (40, 10));
    s.type_str("a\r");
    assert!(s.until(BOUND, |a| got(a).is_some_and(|g| g.0 == "a")));
    let (_, winch, size) = got(&s.app).expect("the printed line");
    assert_eq!(size, "10 40");
    let (frames, lent) = {
        let m = s.memory("/dev/mem0");
        (m.frames, m.ages.len())
    };
    s.memory("/dev/mem0").set_mode(640, 320);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "the display draws again after its mode changed"
    );
    assert_eq!(s.app.grid(), (80, 20), "the grid fills the new size");
    {
        let m = s.memory("/dev/mem0");
        assert_eq!(
            m.ages.get(lent),
            Some(&0),
            "the first frame after is drawn whole"
        );
        assert_eq!(m.pixels.len(), 640 * 320 * 4);
    }
    let after = shows_a_long_line(&mut s);
    assert_eq!(
        after,
        (winch + 1, "20 80".to_owned()),
        "one SIGWINCH, and the pty at the new size"
    );
}

/// vtium/vtium#216: a display whose mode changes in place to a new refresh
/// rate at the same size paces its frames at the new rate. The grid and
/// the renderer stay as they were, and the display's next frame is drawn
/// whole.
#[test]
fn a_display_whose_refresh_rate_changes_in_place_paces_at_its_new_rate() {
    let small = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::boot(&[], small, MODE_SCRIPT);
    assert!(s.until(BOUND, |a| shows(a, "ready")), "the session runs");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0),
        "the display draws"
    );
    let intervals = |s: &Seat| -> Vec<Option<Duration>> {
        s.app
            .heads()
            .iter()
            .flat_map(Head::refresh_intervals)
            .collect()
    };
    assert_eq!(intervals(&s), [Some(Duration::from_nanos(16_666_666))]);
    // A frame the render thread is still composing would be shown after
    // the mode changed, and the frame after it drawn on top of it.
    assert!(
        s.until(BOUND, |a| a.heads().iter().all(|h| h.frames_out() == 0)),
        "no frame composing"
    );
    let (frames, lent) = {
        let m = s.memory("/dev/mem0");
        (m.frames, m.ages.len())
    };
    s.memory("/dev/mem0").set_refresh(30_000);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "the display draws again after its mode changed"
    );
    assert_eq!(
        intervals(&s),
        [Some(Duration::from_nanos(33_333_333))],
        "the view paces at 30 Hz"
    );
    assert_eq!(s.app.grid(), (40, 10), "the grid keeps its size");
    assert_eq!(
        s.memory("/dev/mem0").ages.get(lent),
        Some(&0),
        "the first frame after is drawn whole"
    );
}

/// vtium/vtium#208: a frame composing at the old size while the display's
/// mode changes is dropped when it comes back, and does not bring the old
/// renderer back: the frames after it draw the whole new width.
#[test]
fn a_frame_composing_across_a_mode_change_is_dropped() {
    let small = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::boot(&[], small, MODE_SCRIPT);
    assert!(s.until(BOUND, |a| shows(a, "ready")), "the session runs");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    let (gate, opens) = hold_frames(&mut s, "console");
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    assert!(
        s.until(BOUND, |a| a.heads().iter().any(|h| h.frames_out() > 0)),
        "a frame at the old size is composing"
    );
    let (lease, shown) = {
        let m = s.memory("/dev/mem0");
        (m.lease, m.shown.len())
    };
    let lent = s.memory("/dev/mem0").ages.len();
    s.memory("/dev/mem0").set_mode(640, 320);
    assert!(
        s.until(BOUND, |a| a.grid() == (80, 20)),
        "the head is laid out again while the old frame is held"
    );
    s.app.set_draw_hook("console", None);
    drop(opens);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.shown.len() > shown),
        "the display draws again"
    );
    {
        let m = s.memory("/dev/mem0");
        assert!(
            m.shown[shown..].iter().all(|&l| l > lease),
            "no frame from before the mode change is shown: {:?}",
            m.shown
        );
        assert_eq!(m.ages.get(lent), Some(&0), "{:?}", m.ages);
    }
    shows_a_long_line(&mut s);
    assert!(!gate.expired());
}

/// vtium/vtium#208: a mode smaller than one cell leaves the view at that
/// size, drawing nothing of the grid, and a mode at a usable size after it
/// draws the whole new width again.
#[test]
fn a_mode_smaller_than_a_cell_and_back_draws_at_the_new_size() {
    let small = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::boot(&[], small, MODE_SCRIPT);
    assert!(s.until(BOUND, |a| shows(a, "ready")), "the session runs");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    let frames = s.memory("/dev/mem0").frames;
    s.memory("/dev/mem0").set_mode(6, 10);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "a frame at 6x10"
    );
    let frames = s.memory("/dev/mem0").frames;
    s.memory("/dev/mem0").set_mode(640, 320);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "the display draws again once its mode is large"
    );
    assert_eq!(s.app.grid(), (80, 20));
    shows_a_long_line(&mut s);
}

/// D2: one render thread per head. The slow head, first in the file,
/// composes no frame past a gate the test holds closed; while its frame
/// waits there, a key reaches the session of the fast head, which holds
/// the focus, and the fast head's display shows its echo. The slow head's
/// frame comes once the test opens the gate. The order is set by the
/// gate, and each wait is bounded only so the test fails instead of
/// hanging.
#[test]
fn a_slow_head_delays_neither_another_heads_frame_nor_input() {
    let heads = "version = 1
[head slow]
[head fast]
[rules]
display card:mem-small -> slow
display card:mem-large -> fast
input class:keyboard -> roaming
";
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "read a; echo \"got:$a\"; read b";
    let mut s = Seat::with_heads(&[], Some(heads), displays, script);
    assert!(s.until(BOUND, |a| a.cells().len() == 2), "both lit");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    assert!(s.until_memory("/dev/mem1", BOUND, |m| m.frames > 0));
    assert!(
        s.until(BOUND, |a| a.head("fast").is_some_and(Head::started)),
        "the fast head's session runs"
    );
    let (gate, _opens) = hold_frames(&mut s, "slow");
    let (slow_before, fast_before) = (s.memory("/dev/mem0").frames, s.memory("/dev/mem1").frames);
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    assert!(
        s.until(BOUND, |a| a
            .head("slow")
            .is_some_and(|h| h.frames_out() > 0)),
        "the slow head's frame is composing"
    );
    s.type_str("x\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "fast", "got:x")),
        "the key reached the fast head's session"
    );
    assert!(
        s.until_memory("/dev/mem1", BOUND, |m| m.frames > fast_before),
        "the fast head drew"
    );
    assert_eq!(
        s.memory("/dev/mem0").frames,
        slow_before,
        "the slow head's frame is still held at the gate"
    );
    gate.open();
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > slow_before),
        "the slow head's frame comes once the gate opens"
    );
    assert!(
        !gate.expired(),
        "a frame of the slow head stopped waiting before the gate opened"
    );
}

/// Two heads, `desk` on the large display and `panel` on the small one,
/// each with a keyboard of its own: the head `flood`'s program writes as
/// fast as it can, and the other's echoes each line it reads.
fn flooding(flood: &str) -> Seat {
    const YES: &str =
        "exec yes 0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz";
    const ECHO: &str = "echo ready; while read a; do echo got:$a; done";
    let login = |head: &str| if head == flood { YES } else { ECHO };
    let heads = format!(
        "version = 1
[head desk]
login = /bin/sh -c \"{desk}\"
[head panel]
login = /bin/sh -c \"{panel}\"
[rules]
display card:mem-large -> desk
display card:mem-small -> panel
input path:kbd-desk -> desk
input path:kbd-panel -> panel
",
        desk = login("desk"),
        panel = login("panel"),
    );
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, "read x");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-desk"));
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-panel"));
    s
}

/// D2: a flood in one head's pty takes a bounded share of each turn.
/// While head `flood`'s program writes as fast as it can, a key typed on
/// head `quiet`'s keyboard `kbd` reaches quiet's session, and quiet's
/// display `node` has a frame of the echo on its way, within 12 turns;
/// no turn parses more than 64 KiB of flood's output, and flood's output
/// keeps coming. The frame then comes however long it takes to compose,
/// since a loaded machine composes slowly, and the share is in turns.
fn a_flood_leaves_the_other_head_its_turn(flood: &str, quiet: &str, kbd: &str, node: &str) {
    let mut s = flooding(flood);
    let parsed = |a: &App| a.head(flood).map_or(0, |h| h.term().parsed);
    // The first MiB of the flood takes as long as the machine needs to
    // parse a MiB, not one event's wait, so this wait has a longer bound.
    // Under qemu-user, where make arm-test runs this test, the MiB alone
    // takes about BOUND, and longer beside the other tests.
    assert!(
        s.until(6 * BOUND, |a| head_shows(a, quiet, "ready")
            && parsed(a) > 1 << 20),
        "{quiet}'s session runs and {flood} floods"
    );
    let flooded = parsed(&s.app);
    s.type_on(kbd, "k\r");
    let mut echoed = None;
    let mut turns = 0;
    while turns < 12 {
        let before = parsed(&s.app);
        s.lp.dispatch(Some(Duration::from_millis(20)), &mut s.app)
            .expect("dispatch");
        s.app.turn(now());
        turns += 1;
        let share = parsed(&s.app) - before;
        assert!(
            share <= 64 * 1024,
            "turn {turns} parsed {share} bytes of {flood}'s output"
        );
        let frames = s.memory(node).frames;
        let out = s.app.head(quiet).map_or(0, Head::frames_out);
        match echoed {
            None if head_shows(&s.app, quiet, "got:k") => echoed = Some(frames),
            Some(at) if frames > at || out > 0 => break,
            _ => {}
        }
    }
    let Some(at) = echoed else {
        panic!("the key's echo reached {quiet}'s screen within 12 turns");
    };
    assert!(
        turns < 12,
        "{quiet}'s display had the echo's frame on its way within 12 turns"
    );
    assert!(
        s.until_memory(node, BOUND, |m| m.frames > at),
        "{quiet}'s display draws the echo"
    );
    assert!(parsed(&s.app) > flooded, "{flood}'s output keeps coming");
}

/// The large head's flood leaves the small head its turn.
#[test]
fn a_flood_in_the_large_head_leaves_the_small_head_its_keys_and_frames() {
    a_flood_leaves_the_other_head_its_turn("desk", "panel", KBD_SMALL, "/dev/mem0");
}

/// The small head's flood leaves the large head its turn: the share is
/// the pty's, whatever the size of either head's displays.
#[test]
fn a_flood_in_the_small_head_leaves_the_large_head_its_keys_and_frames() {
    a_flood_leaves_the_other_head_its_turn("panel", "desk", KBD_LARGE, "/dev/mem1");
}

/// D2: a frame composing holds up no VT switch. While the head's frame
/// waits at a gate the test holds closed, the VT's release is
/// acknowledged and the device sleeps, the frame still waiting; the
/// acquire draws the display again once the gate opens.
#[test]
fn a_frame_composing_holds_up_no_vt_release() {
    let display = (
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    );
    let mut s = Seat::boot(&[], [display], "read x");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    let (gate, _opens) = hold_frames(&mut s, "console");
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    assert!(
        s.until(BOUND, |a| a.heads().iter().any(|h| h.frames_out() > 0)),
        "the slow frame is composing"
    );
    let ctrl_super_f12 = KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![0xffc9],
        codepoints: vec![None],
        repeat: false,
    };
    s.app
        .input_event(InputEvent::Key(ctrl_super_f12.clone()), now());
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| !m.awake),
        "asleep for the release"
    );
    let released = Rc::clone(&s.released);
    assert!(
        s.until(BOUND, |_| released.get() == 1),
        "the release acknowledged"
    );
    assert!(
        s.app.heads().iter().any(|h| h.frames_out() > 0),
        "the release is acknowledged while the frame still composes"
    );
    s.app.set_draw_hook("console", None);
    gate.open();
    let before = s.memory("/dev/mem0").frames;
    s.app.input_event(InputEvent::Key(ctrl_super_f12), now());
    assert!(s.until(BOUND, |a| a.vt.is_active()));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.awake && m.frames > before),
        "the acquire draws the display again"
    );
    assert!(
        !gate.expired(),
        "the frame stopped waiting before the gate opened"
    );
}

/// ADR-1401 D2: a frame in flight across a VT release is never shown. On
/// a seat of two heads, the slow head's frame waits at a gate the test
/// holds closed; the VT is released while it waits and acquired again,
/// and only then does the test open the gate. The token for that frame
/// comes back with a lease from before the release, so the display drops
/// it, and the first token lent after the acquire has age 0, so the frame
/// after the acquire is drawn whole. The fast head's display draws again
/// too.
#[test]
fn a_frame_in_flight_across_a_vt_release_is_dropped_and_the_next_drawn_whole() {
    let heads = "version = 1
[head slow]
[head fast]
[rules]
display card:mem-small -> slow
display card:mem-large -> fast
input class:keyboard -> roaming
";
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 2), "both lit");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    assert!(s.until_memory("/dev/mem1", BOUND, |m| m.frames > 0));
    let (gate, _opens) = hold_frames(&mut s, "slow");
    let (lease, shown) = {
        let m = s.memory("/dev/mem0");
        (m.lease, m.shown.len())
    };
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    assert!(
        s.until(BOUND, |a| a
            .head("slow")
            .is_some_and(|h| h.frames_out() > 0)),
        "the slow head's frame is in flight"
    );
    let ctrl_super_f12 = KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![0xffc9],
        codepoints: vec![None],
        repeat: false,
    };
    s.app
        .input_event(InputEvent::Key(ctrl_super_f12.clone()), now());
    // The turn that puts both devices to sleep acknowledges the release.
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| !m.awake),
        "the slow head's display asleep for the release"
    );
    assert!(!s.memory("/dev/mem1").awake, "both asleep");
    assert_eq!(s.released.get(), 1, "the release acknowledged");
    s.app.set_draw_hook("slow", None);
    let fast = s.memory("/dev/mem1").frames;
    s.app.input_event(InputEvent::Key(ctrl_super_f12), now());
    assert!(s.until(BOUND, |a| a.vt.is_active()), "the acquire");
    // The display lends no token while the frame's is out, so the next
    // token lent is the first after the acquire.
    assert!(
        s.app.head("slow").is_some_and(|h| h.frames_out() > 0)
            && Arc::strong_count(&s.memory("/dev/mem0").drawn) > 1,
        "the frame from before the release is still in flight at the acquire"
    );
    let lent = s.memory("/dev/mem0").ages.len();
    gate.open();
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.shown.len() > shown
            && m.ages.len() > lent),
        "the slow head's display draws after the acquire"
    );
    let m = s.memory("/dev/mem0");
    let stale: Vec<u64> = m.shown[shown..]
        .iter()
        .copied()
        .filter(|l| *l <= lease)
        .collect();
    assert_eq!(stale, [], "a frame from before the release was shown");
    assert_eq!(
        m.ages.get(lent),
        Some(&0),
        "the first token after the acquire draws whole: {:?}",
        m.ages
    );
    assert!(
        s.until_memory("/dev/mem1", BOUND, |m| m.frames > fast),
        "the fast head's display draws again"
    );
    assert!(
        !gate.expired(),
        "the frame stopped waiting before the gate opened"
    );
}

/// Two heads, a keyboard claimed by each, and every other keyboard
/// following the focus.
const CLAIMED: &str = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-large -> large
input class:keyboard -> roaming
";

/// The keyboards [`claimed`] adds: one claimed by each head, and one
/// following the focus.
const KBD_SMALL: &str = "/dev/input/event0";
const KBD_LARGE: &str = "/dev/input/event1";
const KBD_FOCUS: &str = "/dev/input/event2";

/// A session echoing each line it reads, and ending at `q`.
const ECHO: &str = "echo ready; while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done";

/// Two heads booted on [`CLAIMED`], the large display the boot display,
/// with [`KBD_SMALL`], [`KBD_LARGE`] and [`KBD_FOCUS`], each head's
/// session echoing each line it reads, and ending at `q`.
fn claimed() -> Seat {
    claimed_with(&[])
}

/// [`claimed`], booted with `args`.
fn claimed_with(args: &[&str]) -> Seat {
    claimed_on(args, CLAIMED)
}

/// [`CLAIMED`] with head small's `consoles` set to `n`.
fn claimed_consoles(n: usize) -> String {
    CLAIMED.replace("[head small]\n", &format!("[head small]\nconsoles = {n}\n"))
}

/// [`claimed`], booted with `args` on the head file `heads`.
fn claimed_on(args: &[&str], heads: &str) -> Seat {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(args, Some(heads), displays, ECHO);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    s
}

/// D5: a keyboard a rule sends to a head types into that head alone,
/// whichever head holds the focus.
#[test]
fn a_claimed_keyboard_types_into_its_head_alone() {
    let mut s = claimed();
    s.type_on(KBD_SMALL, "c\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:c")),
        "the claimed keyboard types into its head"
    );
    s.type_on(KBD_FOCUS, "f\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:f")),
        "the keyboard following the focus types into the boot display's head"
    );
    s.until(Duration::from_millis(200), |_| false);
    assert!(!head_shows(&s.app, "large", "got:c"), "nor elsewhere");
    assert!(!head_shows(&s.app, "small", "got:f"));
}

/// The key `keysym` held with Ctrl and Super.
fn ctrl_super(keysym: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::LOGO),
        keysyms: vec![keysym],
        codepoints: vec![None],
        repeat: false,
    })
}

const UP: u32 = 0xff52;
const DOWN: u32 = 0xff54;

/// D5 and the operator's chord (2026-10-30): Ctrl-Super-Up moves the
/// active head to the next head with a lit display, and Ctrl-Super-Down to
/// the previous. Only a roaming keyboard takes the chord, and it moves only
/// the roaming devices; a claimed keyboard keeps typing into its head.
// covers: platform.heads.focus
#[test]
fn the_roaming_chord_moves_only_the_roaming_devices() {
    let mut s = claimed();
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move().as_deref(), Some("small"));
    s.type_on(KBD_FOCUS, "f\r");
    s.type_on(KBD_LARGE, "c\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:f")
            && head_shows(a, "large", "got:c")),
        "the focus's keyboard types into small, the claimed one into large"
    );
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(DOWN), now());
    assert_eq!(s.app.take_focus_move().as_deref(), Some("large"));
    s.type_on(KBD_FOCUS, "g\r");
    assert!(s.until(BOUND, |a| head_shows(a, "large", "got:g")));
    s.until(Duration::from_millis(200), |_| false);
    assert!(!head_shows(&s.app, "small", "got:c"));
    assert!(!head_shows(&s.app, "small", "got:g"));
    assert_eq!(s.app.take_focus_move(), None, "taken once");
}

/// D5: the terminal of each head a keyboard delivers to has mode 1004's
/// focus, several at once, one a head. A keyboard following the focus
/// gives it to the boot display's head; the focus chord takes it from
/// that head and gives it to the next.
#[test]
fn mode_1004_follows_the_keyboards_delivering_to_each_head() {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "stty -icanon -echo; printf '\\033[?1004h'; echo ready; \
                  while :; do echo \"k:$(head -c 3 | od -An -tx1)\"; done";
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, script);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    let (focus_in, focus_out) = ("1b 5b 49", "1b 5b 4f");
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "large", "k:"), "no keyboard yet");
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", focus_in)),
        "the keyboard gives the boot display's head the focus"
    );
    assert!(!head_shows(&s.app, "small", "k:"));
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", focus_in)
            && head_shows(a, "large", focus_out)),
        "the chord moves it"
    );
}

/// ADR-1005 D7: a program that asked the kitty keyboard protocol for
/// releases gets one for every press it got. The focus chord moves the
/// keyboard to the other head while Escape is held, so the head it leaves
/// gets Escape's release at once, `CSI 27 ; 1:3 u` after the press's
/// `CSI 27 u`, and the key's real release later goes nowhere.
// covers: input.kitty-keyboard
#[test]
fn the_focus_chord_lets_go_of_the_keys_held_on_the_head_it_leaves() {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "stty -icanon -echo; printf '\\033[>3u'; echo ready; \
                  while :; do echo \"k:$(head -c 14 | tr '\\033' E)\"; done";
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, script);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    let escape = KeyEvent {
        keycode: 9,
        ascii: 0,
        mods: Mods::empty(),
        keysyms: vec![0xff1b],
        codepoints: vec![None],
        repeat: false,
    };
    let kbd = Path::new(KBD_FOCUS);
    s.app
        .device_input(kbd, InputEvent::Key(escape.clone()), now());
    s.app.device_input(kbd, ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move().as_deref(), Some("small"));
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "k:E[27uE[27;1:3u")),
        "the head left gets the release before the key is let go: {:?}",
        s.app.head("large").map(Head::screen_text)
    );
    // The key's real release goes nowhere: small never got its press.
    s.app.device_input(kbd, InputEvent::Release(escape), now());
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "small", "k:"), "small got no release");
}

/// Mode 1035 reads NumLock from the modifiers the input reports, which
/// the binary hands the terminal unchanged: under DECKPAM, KP_1 with
/// NumLock on sends its digit, as xterm's numLock does, where the
/// application keypad would send SS3 q; with NumLock off the key is KP_End,
/// which sends what End sends (ctlseqs, mode 1035; vtium/vtium#356).
// covers: DEC-1035
#[test]
fn mode_1035_reads_the_num_lock_the_input_reports() {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "stty -icanon -echo; printf '\\033='; echo ready; \
                  while :; do echo \"k:$(head -c 4 | tr '\\033' E)\"; done";
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, script);
    assert!(s.until(BOUND, |a| head_shows(a, "large", "ready")));
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    let kp_1 = |mods: Mods| {
        InputEvent::Key(KeyEvent {
            keycode: 87,
            ascii: 0,
            mods,
            keysyms: vec![if mods.contains(Mods::NUM) {
                0xffb1
            } else {
                0xff9c
            }],
            codepoints: vec![mods.contains(Mods::NUM).then_some('1')],
            repeat: false,
        })
    };
    let kbd = Path::new(KBD_FOCUS);
    s.app.device_input(kbd, kp_1(Mods::NUM), now());
    s.app.device_input(kbd, kp_1(Mods::empty()), now());
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "k:1E[F")),
        "{:?}",
        s.app.head("large").map(Head::screen_text)
    );
}

/// Two heads of two consoles each, a keyboard claimed by each.
const TWO_CONSOLES: &str = "version = 1
[head small]
consoles = 2
[head large]
consoles = 2
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-large -> large
";

/// The rows of every console of head `name`, in the ring's order.
fn every_console(app: &App, name: &str) -> Vec<Vec<String>> {
    app.head(name).map(Head::consoles_text).unwrap_or_default()
}

/// The line holding `prefix` on each console of head `name`, in the
/// ring's order, `None` where none does.
fn console_lines(app: &App, name: &str, prefix: &str) -> Vec<Option<String>> {
    every_console(app, name)
        .into_iter()
        .map(|rows| rows.into_iter().find(|l| l.starts_with(prefix)))
        .collect()
}

/// [`TWO_CONSOLES`] booted on both displays, every console running
/// `script`.
fn two_consoles(script: &str) -> Seat {
    Seat::with_heads(&[], Some(TWO_CONSOLES), two_displays(), script)
}

/// Adds [`KBD_SMALL`] and [`KBD_LARGE`] to `s`.
fn claim_keyboards(s: &mut Seat) {
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
}

/// vtium/vtium#167, D1 and D5: each console's session is told its head
/// and the console's own id, distinct in every console of every head.
/// Under respawn a console whose program exits starts it again in the same
/// console, from a hard reset, and the console keeps its id: the head
/// keeps its two consoles, the shown one stays shown, and the other head
/// is untouched.
// covers: platform.heads
#[test]
fn each_console_is_told_its_head_and_its_own_id_through_a_restart() {
    let script = "echo \"is:$VTIUM_HEAD:$VTIUM_CONSOLE\"; \
                  while read a; do [ \"$a\" = q ] && exit; done";
    let mut s = two_consoles(script);
    claim_keyboards(&mut s);
    let told = |a: &App, name: &str| console_lines(a, name, "is:");
    let line = |text: &str| Some(text.to_owned());
    assert!(
        s.until(BOUND, |a| told(a, "small").iter().all(Option::is_some)
            && told(a, "large").iter().all(Option::is_some))
    );
    assert_eq!(
        told(&s.app, "small"),
        [line("is:small:1"), line("is:small:2")]
    );
    assert_eq!(
        told(&s.app, "large"),
        [line("is:large:1"), line("is:large:2")]
    );
    let typed = |a: &App| head_shows(a, "small", "typed");
    s.type_on(KBD_SMALL, "typed\r");
    assert!(s.until(BOUND, typed));
    s.type_on(KBD_SMALL, "q\r");
    // The line typed goes only with the hard reset of the restart.
    assert!(
        s.until(BOUND, |a| !typed(a) && head_shows(a, "small", "is:small:1")),
        "the first console's program started again"
    );
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "in its place");
    assert_eq!(
        told(&s.app, "small"),
        [line("is:small:1"), line("is:small:2")],
        "1 keeps its id"
    );
    assert_eq!(
        told(&s.app, "large"),
        [line("is:large:1"), line("is:large:2")]
    );
    assert!(s.app.finished().is_none());
}

/// vtium/vtium#167 and D5: focus reporting is the signal of a console
/// switch. A head showing another console sends a focus-out to the one it
/// leaves and a focus-in to the one it shows, while a keyboard delivers to
/// the head.
// covers: platform.heads
#[test]
fn a_console_switch_sends_a_focus_out_to_the_console_left_and_a_focus_in_to_the_one_shown() {
    let script = "stty -icanon -echo; printf '\\033[?1004h'; echo ready; \
                  while :; do echo \"k:$(head -c 3 | od -An -tx1)\"; done";
    let (focus_in, focus_out) = ("k: 1b 5b 49", "k: 1b 5b 4f");
    let mut s = two_consoles(script);
    let ready = |a: &App| console_lines(a, "small", "ready");
    assert!(s.until(BOUND, |a| ready(a).iter().all(Option::is_some)));
    // Once both consoles report focus, so the keyboard's focus is seen.
    claim_keyboards(&mut s);
    let seen = |a: &App| -> Vec<Vec<String>> {
        every_console(a, "small")
            .into_iter()
            .map(|rows| rows.into_iter().filter(|l| l.starts_with("k:")).collect())
            .collect()
    };
    let lines = |texts: &[&str]| -> Vec<String> { texts.iter().map(|t| (*t).to_owned()).collect() };
    assert!(
        s.until(BOUND, |a| seen(a) == [lines(&[focus_in]), lines(&[])]),
        "the shown console has the focus: {:?}",
        seen(&s.app)
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert!(
        s.until(BOUND, |a| seen(a)
            == [lines(&[focus_in, focus_out]), lines(&[focus_in])]),
        "out to the console left, in to the one shown: {:?}",
        seen(&s.app)
    );
}

/// D5 and the operator's answer of 2026-10-30: the focus starts on the
/// boot display's head even when that display lights a turn late, until
/// the first key; after a key it stays where it is. The key comes from a
/// keyboard following the focus, which reaches a lit head, so the
/// anti-lockout guard never acts (D9).
#[test]
fn the_focus_waits_for_a_late_boot_display_until_the_first_key() {
    let late = |key: bool| {
        let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
        let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
        s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
        assert!(s.until(BOUND, |a| a.cells().len() == 1));
        assert_eq!(s.app.focus_head(), Some("small"), "the only lit head");
        if key {
            s.type_on(KBD_FOCUS, "k");
        }
        s.app.add_video(
            PathBuf::from("/dev/mem1"),
            Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
        );
        assert!(s.until(BOUND, |a| a.cells().len() == 2));
        s.app.focus_head().map(str::to_owned)
    };
    assert_eq!(late(false).as_deref(), Some("large"), "the boot display's");
    assert_eq!(late(true).as_deref(), Some("small"), "a key settled it");
}

const RIGHT: u32 = 0xff53;
const LEFT: u32 = 0xff51;
const RETURN: u32 = 0xff0d;
const BACKSPACE: u32 = 0xff08;

/// The console count and the shown console of head `name`.
fn consoles(app: &App, name: &str) -> Option<(usize, usize)> {
    app.head(name)
        .map(|h| (h.console_count(), h.shown_console()))
}

/// #82, D1 and D5: a head starts with the consoles its `consoles` gives
/// it. Ctrl-Super-Right and Ctrl-Super-Left show the next and the
/// previous, wrapping, each console a session with its own screen. The
/// other head's console stays as it was.
// covers: platform.heads
#[test]
fn the_console_chords_switch_a_heads_consoles() {
    let mut s = claimed_on(&[], &claimed_consoles(3));
    assert_eq!(
        consoles(&s.app, "small"),
        Some((3, 0)),
        "three from the start"
    );
    // A chord shows another console as the key is delivered.
    let chord = |s: &mut Seat, keysym: u32| {
        s.app
            .device_input(Path::new(KBD_SMALL), ctrl_super(keysym), now());
    };
    s.type_on(KBD_SMALL, "one\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:one")));
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    assert!(!head_shows(&s.app, "small", "got:one"), "its own screen");
    s.type_on(KBD_SMALL, "two\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:two")));
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 2)));
    s.type_on(KBD_SMALL, "three\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:three")));
    chord(&mut s, RIGHT);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((3, 0)),
        "wraps to the first"
    );
    assert!(head_shows(&s.app, "small", "got:one"));
    chord(&mut s, LEFT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 2)), "wraps to the last");
    assert!(head_shows(&s.app, "small", "got:three"));
    chord(&mut s, LEFT);
    assert!(head_shows(&s.app, "small", "got:two"));
    assert_eq!(consoles(&s.app, "large"), Some((1, 0)));
}

/// ADR-1005 D10: Ctrl+Shift+Delete resets the console a head shows, and
/// no other: the hidden console of the same head and the other head's
/// console keep their screens.
#[test]
fn the_reset_key_resets_the_shown_console_and_no_other() {
    let mut s = claimed_on(&[], &claimed_consoles(2));
    s.type_on(KBD_SMALL, "one\r");
    s.type_on(KBD_LARGE, "large\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:one")
        && head_shows(a, "large", "got:large")));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    s.type_on(KBD_SMALL, "two\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:two")));
    let reset = InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::SHIFT),
        keysyms: vec![0xffff],
        codepoints: vec![None],
        repeat: false,
    });
    s.app.device_input(Path::new(KBD_SMALL), reset, now());
    assert_eq!(
        console_lines(&s.app, "small", "got:"),
        [Some("got:one".to_owned()), None],
        "the shown console alone is cleared"
    );
    assert!(
        head_shows(&s.app, "large", "got:large"),
        "the other head keeps its screen"
    );
    // The console runs on after its reset.
    s.type_on(KBD_SMALL, "three\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:three")));
}

/// A session that prints its console's id, then echoes each line it
/// reads.
const TOLD: &str = "echo \"is:$VTIUM_CONSOLE\"; while read a; do echo \"got:$a\"; done";

/// The ids of head `name`'s consoles in the ring's order, as each
/// console's session printed it, `None` where it has not yet.
fn told_ids(app: &App, name: &str) -> Vec<Option<String>> {
    console_lines(app, name, "is:")
}

/// Presses the chord `keysym` on the keyboard at `node`.
fn press_chord(s: &mut Seat, node: &str, keysym: u32) {
    s.app
        .device_input(Path::new(node), ctrl_super(keysym), now());
}

/// D1 and D5 under an open policy, the maintainer's answers of
/// 2026-11-01: Ctrl-Super-Return opens a console just after the one the
/// head shows, starts the head's login in it and shows it; the new
/// console takes the next id the head has not given, and an id is never
/// given twice. Ctrl-Super-Backspace closes the console the head shows
/// and shows the console after it, or the first when it was the last.
/// Neither chord reaches a session, and the other head is untouched.
// covers: platform.heads
#[test]
fn under_open_the_chords_open_a_console_after_the_shown_one_and_close_the_shown_one() {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(3)), displays, TOLD);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let is = |ids: &[&str]| -> Vec<Option<String>> {
        ids.iter().map(|id| Some(format!("is:{id}"))).collect()
    };
    assert!(s.until(BOUND, |a| told_ids(a, "small") == is(&["1", "2", "3"])));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((4, 2)),
        "opened after the shown console, and shown"
    );
    assert!(
        s.until(BOUND, |a| told_ids(a, "small") == is(&["1", "2", "4", "3"])),
        "the new console runs the login as console 4: {:?}",
        told_ids(&s.app, "small")
    );
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((3, 2)),
        "closed, showing the console after it"
    );
    assert_eq!(told_ids(&s.app, "small"), is(&["1", "2", "3"]));
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((2, 0)),
        "the last closed, showing the first"
    );
    assert_eq!(told_ids(&s.app, "small"), is(&["1", "2"]));
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    assert!(
        s.until(BOUND, |a| told_ids(a, "small") == is(&["1", "5", "2"])),
        "no id is given twice: {:?}",
        told_ids(&s.app, "small")
    );
    // Each live session echoes a last line typed into it; a chord that had
    // reached a session as Return would have echoed before it.
    for (n, id) in ["5", "2", "1"].into_iter().enumerate() {
        if n > 0 {
            press_chord(&mut s, KBD_SMALL, RIGHT);
        }
        assert!(
            head_shows(&s.app, "small", &format!("is:{id}")),
            "console {id} shown"
        );
        s.type_on(KBD_SMALL, &format!("end{id}\r"));
        assert!(
            s.until(BOUND, |a| head_shows(a, "small", &format!("got:end{id}"))),
            "console {id} echoes"
        );
    }
    assert!(
        every_console(&s.app, "small")
            .iter()
            .flatten()
            .filter(|l| l.starts_with("got:"))
            .all(|l| l.starts_with("got:end")),
        "no chord reached a session: {:?}",
        every_console(&s.app, "small")
    );
    assert_eq!(consoles(&s.app, "large"), Some((1, 0)));
}

/// D5: closing the console shown closes its pty, so its programs get
/// SIGHUP, as on a hang-up; the other consoles' sessions run on. Each
/// session waits in `read` on its pty. When the pty closes, the kernel
/// ends that read before it sends its own SIGHUP, so a shell that vtium
/// did not signal before the close can leave its loop and exit without
/// ever running its trap. Ids are each head's own, and the large head's
/// console is 1 too, so each marker names its head.
// covers: platform.heads
#[test]
fn closing_a_console_hangs_up_its_session_alone() {
    let d = Scratch::new("heads-close");
    let script = format!(
        "trap 'echo > {d}/hup-$VTIUM_HEAD-$VTIUM_CONSOLE; exit 0' HUP; \
         echo \"is:$VTIUM_CONSOLE\"; while read a; do echo \"got:$a\"; done",
        d = d.display()
    );
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(2)), displays, &script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    assert!(s.until(BOUND, |a| told_ids(a, "small").iter().all(Option::is_some)));
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    let hup = |head: &str, id: u32| d.join(format!("hup-{head}-{id}")).exists();
    let end = now() + BOUND;
    while !hup("small", 1) && now() < end {
        s.until(Duration::from_millis(20), |_| false);
    }
    assert!(hup("small", 1), "console 1's session got SIGHUP");
    assert!(!hup("small", 2), "console 2's runs on");
    assert!(!hup("large", 1), "the large head's runs on");
    s.type_on(KBD_SMALL, "on\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:on")));
}

/// D5: a session the open chord cannot start leaves the run alive. The
/// console that could not start leaves the head, which shows the console it
/// showed before, and the top of the head says so; the other sessions run
/// on. The head's login here is a script removed once the first console's
/// session has started, so the second cannot start.
// covers: platform.heads
#[test]
fn a_console_the_open_chord_cannot_start_leaves_and_the_run_goes_on() {
    let dir = Scratch::new("heads-unstartable");
    let login = dir.join("login");
    script::executable(
        &login,
        "#!/bin/sh\necho ready; while read a; do echo \"got:$a\"; done\n",
    );
    let heads = CLAIMED.replace(
        "[head small]\n",
        &format!("[head small]\nlogin = {}\n", login.display()),
    );
    let mut s = claimed_on(&[], &heads);
    std::fs::remove_file(&login).expect("the login removed");
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "the console left");
    assert_eq!(
        notice(&s.app, "small").as_deref(),
        Some("Console not opened: login could not start."),
        "the notice at the top of the head"
    );
    s.type_on(KBD_SMALL, "on\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:on")),
        "the first console's session runs on"
    );
    s.type_on(KBD_LARGE, "on\r");
    assert!(s.until(BOUND, |a| head_shows(a, "large", "got:on")));
    assert_eq!(s.app.finished(), None, "vtium runs on");
}

/// A login script at `dir/login` that turns on mode 1004, prints its
/// console's id, then echoes each line it reads through `cat -v`, so a
/// focus report it was sent shows on its screen.
fn focus_login(dir: &Path) -> PathBuf {
    let login = dir.join("login");
    script::executable(
        &login,
        "#!/bin/sh\nprintf '\\033[?1004h'; echo \"is:$VTIUM_CONSOLE\"; while read a; do echo \"got:$a\" | cat -v; done\n",
    );
    login
}

/// A scratch directory for test `name`, on disk under the build's own.
fn scratch_dir(name: &str) -> Scratch {
    Scratch::new(&format!("heads-{name}"))
}

/// Head small of [`CLAIMED`] with three consoles running [`focus_login`]
/// from `dir`, each printed its id, with its own keyboard and
/// [`KBD_LARGE`].
fn three_focus_consoles(dir: &Path) -> (Seat, PathBuf) {
    let login = focus_login(dir);
    let heads = CLAIMED.replace(
        "[head small]\n",
        &format!("[head small]\nconsoles = 3\nlogin = {}\n", login.display()),
    );
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, ECHO);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| told_ids(a, "small").iter().all(Option::is_some)));
    (s, login)
}

/// D5: a failed open never shows its console, so the console shown before
/// is told nothing: no focus-out and focus-in reach its program before the
/// line typed next. The focus-in the head's keyboard gave it at the start
/// may come before, whichever came first of the keyboard and the program's
/// mode 1004.
// covers: platform.heads
#[test]
fn a_failed_open_tells_the_console_shown_nothing() {
    let dir = scratch_dir("failed-open-focus");
    let (mut s, login) = three_focus_consoles(&dir);
    std::fs::rename(&login, dir.join("gone")).expect("the login moved");
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(consoles(&s.app, "small"), Some((3, 0)));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:")));
    let got = console_lines(&s.app, "small", "got:")
        .first()
        .cloned()
        .flatten();
    assert!(
        got.as_deref()
            .is_some_and(|l| l.ends_with('x') && !l.contains("^[[O")),
        "console 1's program got no focus-out before the line: {got:?}"
    );
}

/// D5, binary-recheck's R1: a failed open from the middle of a ring of
/// three keeps the ring's order and shows the console shown before; the
/// id it took is used up, and the next open, once the login can start
/// again, takes the id after it and lands just after the shown console.
// covers: platform.heads
#[test]
fn a_failed_open_mid_ring_keeps_the_order_and_uses_up_its_id() {
    let dir = scratch_dir("mid-ring");
    let (mut s, login) = three_focus_consoles(&dir);
    let is = |ids: &[&str]| -> Vec<Option<String>> {
        ids.iter().map(|id| Some(format!("is:{id}"))).collect()
    };
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    std::fs::rename(&login, dir.join("gone")).expect("the login moved");
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((3, 1)),
        "the count and the place"
    );
    assert!(head_shows(&s.app, "small", "is:2"), "console 2 shown again");
    assert_eq!(
        told_ids(&s.app, "small"),
        is(&["1", "2", "3"]),
        "the ring's order"
    );
    std::fs::rename(dir.join("gone"), &login).expect("the login back");
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert!(
        s.until(BOUND, |a| told_ids(a, "small") == is(&["1", "2", "5", "3"])),
        "id 4 is never given again, and 5 opens after console 2: {:?}",
        told_ids(&s.app, "small")
    );
    assert_eq!(s.app.finished(), None);
}

/// D5, the maintainer's answer of 2026-11-01: on a head's last console
/// Ctrl-Super-Backspace closes nothing and reaches no session.
// covers: platform.heads
#[test]
fn the_close_chord_on_a_heads_last_console_does_nothing() {
    let mut s = claimed();
    s.type_on(KBD_SMALL, "xy");
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    s.type_on(KBD_SMALL, "z\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:xyz")),
        "the session runs on, and the chord reached it as no Backspace: {:?}",
        s.app.head("small").map(Head::screen_text)
    );
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)));
    assert!(
        s.app.head("small").is_some_and(|h| h.pty().is_some()),
        "its pty is open"
    );
}

/// D1 and D5: under `policy = closed` a head keeps the consoles it started
/// with. vtium takes the open and close chords and does nothing with
/// them, so neither reaches the session as Return or Backspace.
// covers: platform.heads.policy
#[test]
fn under_closed_the_open_and_close_chords_do_nothing_and_reach_no_session() {
    let heads = claimed_consoles(2)
        .replace("version = 1\n", "version = 1\npolicy = closed\n")
        .replace("input class:keyboard -> roaming\n", "");
    let mut s = claimed_on(&[], &heads);
    s.type_on(KBD_SMALL, "ab");
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((2, 0)),
        "none opened or closed"
    );
    s.type_on(KBD_SMALL, "c\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:abc")),
        "neither chord reached the session: {:?}",
        s.app.head("small").map(Head::screen_text)
    );
}

/// D5 and D10, the maintainer's answers of 2026-11-01: where the head has
/// `head-consoles-max` consoles, or every head together `consoles-max`,
/// Ctrl-Super-Return opens nothing, and the OSD shows a brief line at the
/// top of the head naming the cap, under `osd = minimal`; under
/// `osd = off` it shows nothing.
// covers: platform.heads
#[test]
fn the_open_chord_at_a_cap_opens_nothing_and_says_so_on_the_osd() {
    let cases = [
        (
            &["--head-consoles-max=2"][..],
            Some("No more consoles: small has its limit of 2."),
        ),
        (
            &["--consoles-max=3"][..],
            Some("No more consoles: all heads together have the limit of 3."),
        ),
        (&["--head-consoles-max=2", "--osd=off"][..], None),
    ];
    for (args, line) in cases {
        let mut s = claimed_on(args, &claimed_consoles(2));
        assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "{args:?}");
        press_chord(&mut s, KBD_SMALL, RETURN);
        assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "{args:?}");
        assert!(
            s.until(BOUND, |a| a.head("small").and_then(Head::status) == line),
            "{args:?}: {:?}",
            s.app.head("small").and_then(Head::status)
        );
        assert_eq!(s.app.head("large").and_then(Head::status), None, "{args:?}");
    }
}

/// D5, the maintainer's answer of 2026-11-01: under `osd = full`, opening
/// or closing a console shows the full line with the new count.
// covers: platform.heads
#[test]
fn under_osd_full_opening_and_closing_a_console_shows_the_new_count() {
    let mut s = claimed_with(&["--osd=full"]);
    let status = |a: &App| a.head("small").and_then(Head::status).map(str::to_owned);
    // The start's OSD, if any, runs out first.
    assert!(s.until(BOUND, |a| status(a).is_none()));
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert!(
        s.until(BOUND, |a| status(a).as_deref()
            == Some("\u{25cb} small \u{b7} console 2 of 2")),
        "{:?}",
        status(&s.app)
    );
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert!(
        s.until(BOUND, |a| status(a).as_deref()
            == Some("\u{25cb} small \u{b7} console 1 of 1")),
        "{:?}",
        status(&s.app)
    );
}

/// D5: closing the shown console gives mode 1004's focus to the console
/// shown next. Turning mode 1004 on reports nothing, so the keyboard
/// comes, and the switch is pressed, only once both sessions have printed
/// their ids, which they print after they turn the mode on: a focus given
/// before then is never reported.
// covers: platform.heads
#[test]
fn closing_a_console_gives_the_console_shown_next_the_focus() {
    let script = "stty -icanon -echo; printf '\\033[?1004h'; echo \"is:$VTIUM_CONSOLE\"; \
                  while :; do echo \"k:$(head -c 3 | od -An -tx1)\"; done";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(2)), displays, script);
    assert!(
        s.until(BOUND, |a| told_ids(a, "small").iter().all(Option::is_some)),
        "both sessions turned mode 1004 on"
    );
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let (focus_in, focus_out) = ("1b 5b 49", "1b 5b 4f");
    assert!(s.until(BOUND, |a| head_shows(a, "small", focus_in)));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert!(s.until(BOUND, |a| head_shows(a, "small", focus_in)
        && head_shows(a, "small", "is:2")));
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)));
    assert!(
        s.until(BOUND, |a| {
            let text = a.head("small").map(Head::screen_text).unwrap_or_default();
            let lines: Vec<&String> = text.iter().filter(|l| l.starts_with("k:")).collect();
            lines.len() == 3 && lines.last().is_some_and(|l| l.contains(focus_in))
        }),
        "console 1 lost the focus at the switch and gains it at the close: {:?}",
        s.app.head("small").map(Head::screen_text)
    );
    assert!(head_shows(&s.app, "small", focus_out));
}

/// ADR-1002, "What a link does", across ADR-1401 D5's console chords: the
/// pointer is over the shown console alone. A console chord leaves the
/// console it hides hovering nothing and the console it shows hovering the
/// link under the pointer at once, with Ctrl, which the chord holds,
/// showing its URI, with no pointer event in between.
// covers: osc.8
#[test]
fn a_console_chord_moves_the_hover_to_the_console_it_shows() {
    let heads = "version = 1
[head main]
consoles = 2
[rules]
display card:mem-small -> main
input class:keyboard -> roaming
";
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let script = "printf 'see \\033]8;;file://pixie/a.md\\007link\\033]8;;\\007\\n'; read x";
    let mut s = Seat::with_heads(&[], Some(heads), displays, script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let both = |a: &App| {
        a.head("main").is_some_and(|h| {
            h.consoles_text()
                .iter()
                .all(|t| t.iter().any(|l| l == "see link"))
        })
    };
    assert!(
        s.until(BOUND, both),
        "both consoles printed the link: {:?}",
        s.app.head("main").map(Head::consoles_text)
    );
    let (cw, ch) = *s.app.cells().first().expect("a cell");
    let over = vtium_platform::input::PointerEvent::Moved {
        x: i32::try_from(5 * cw + 1).expect("a pixel"),
        y: i32::try_from(ch / 2).expect("a pixel"),
        held: None,
    };
    s.app.pointer_event(over, Mods::empty(), now());
    let uri = || Some("file://pixie/a.md".to_owned());
    let hovered = |a: &App| a.head("main").map(Head::consoles_hovered);
    assert_eq!(hovered(&s.app), Some(vec![uri(), None]));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    assert_eq!(consoles(&s.app, "main"), Some((2, 1)));
    assert_eq!(hovered(&s.app), Some(vec![None, uri()]));
    let overlay = |a: &App| {
        let terminal = &a.term().expect("a head").terminal;
        let frame = terminal
            .screen()
            .frame(terminal.palette(), vtium_term::Age::NEVER);
        frame.overlay().map(|o| o.uri.to_owned())
    };
    assert_eq!(
        overlay(&s.app),
        uri(),
        "Ctrl, held for the chord, shows the URI"
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(LEFT), now());
    assert_eq!(hovered(&s.app), Some(vec![uri(), None]));
}

/// The seat of [`the_head_losing_the_focus_shows_no_overlay`] and the
/// tests after it: two heads, each printing a link at columns 4 to 7 of
/// its first row, the focus on `large` and [`KBD_FOCUS`] following it.
fn linked_heads() -> Seat {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "printf 'see \\033]8;;file://pixie/a.md\\007link\\033]8;;\\007\\n'; read x";
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, script);
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "see link")
        && head_shows(a, "large", "see link")));
    assert_eq!(s.app.focus_head(), Some("large"));
    s
}

/// A pointer move to column `col` of the first row of head `name`.
fn over_cell(s: &Seat, name: &str, col: u32) -> vtium_platform::input::PointerEvent {
    let (cw, ch) = *s
        .app
        .head(name)
        .map(Head::cells)
        .unwrap_or_default()
        .first()
        .expect("a cell");
    vtium_platform::input::PointerEvent::Moved {
        x: i32::try_from(col * cw + 1).expect("a pixel"),
        y: i32::try_from(ch / 2).expect("a pixel"),
        held: None,
    }
}

/// The URI the overlay of head `name` shows.
fn overlay_of(app: &App, name: &str) -> Option<String> {
    app.head(name).and_then(Head::link_overlay)
}

/// The URI of the link head `name`'s shown console hovers, which it draws
/// underlined.
fn hovered_on(app: &App, name: &str) -> Option<String> {
    app.head(name)
        .and_then(|h| h.consoles_hovered().into_iter().flatten().next())
}

const A_MD: &str = "file://pixie/a.md";

/// ADR-1002, "What a link does", across the focus chord: Ctrl's overlay
/// is the focus's. The head losing the focus shows no overlay, though the
/// Ctrl the chord held was never let go as far as vtium knows; it still
/// underlines the link under its pointer and still draws its pointer.
/// The head taking the focus shows the overlay of the link under its own
/// pointer at once, since the keyboard that holds Ctrl now reaches it.
// covers: osc.8
#[test]
fn the_head_losing_the_focus_shows_no_overlay() {
    let mut s = linked_heads();
    // small's pointer rests over its link, with no Ctrl held.
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert_eq!(s.app.focus_head(), Some("small"));
    let over = over_cell(&s, "small", 5);
    s.app.pointer_event(over, Mods::empty(), now());
    assert_eq!(hovered_on(&s.app, "small").as_deref(), Some(A_MD));
    assert_eq!(overlay_of(&s.app, "small"), None);
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert_eq!(s.app.focus_head(), Some("large"));
    let over = over_cell(&s, "large", 5);
    s.app.pointer_event(over, Mods::CTRL, now());
    assert_eq!(overlay_of(&s.app, "large").as_deref(), Some(A_MD));
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert_eq!(s.app.focus_head(), Some("small"));
    assert_eq!(overlay_of(&s.app, "large"), None, "large lost the focus");
    assert_eq!(
        hovered_on(&s.app, "large").as_deref(),
        Some(A_MD),
        "large still underlines the link under its pointer"
    );
    assert!(
        s.app.head("large").and_then(Head::pointer).is_some(),
        "large still draws its pointer"
    );
    assert_eq!(
        overlay_of(&s.app, "small").as_deref(),
        Some(A_MD),
        "small took the focus with Ctrl held"
    );
    s.app
        .pointer_event(over_cell(&s, "small", 0), Mods::empty(), now());
    assert_eq!(overlay_of(&s.app, "large"), None);
}

/// ADR-1002, "What a link does": letting go of Ctrl over a link ends its
/// overlay. A release sends no key, so the seat reads the modifiers the
/// head's own keyboards hold when they change.
// covers: osc.8
#[test]
fn releasing_ctrl_over_a_link_hides_its_overlay() {
    let mut s = linked_heads();
    let over = over_cell(&s, "large", 5);
    s.app.pointer_event(over, Mods::CTRL, now());
    assert_eq!(overlay_of(&s.app, "large").as_deref(), Some(A_MD));
    s.app
        .device_input(Path::new(KBD_FOCUS), InputEvent::Mods(Mods::empty()), now());
    assert_eq!(overlay_of(&s.app, "large"), None);
    assert_eq!(
        hovered_on(&s.app, "large").as_deref(),
        Some(A_MD),
        "the link stays underlined"
    );
}

/// ADR-1002, "What a link does": after a VT switch away and back, the
/// keyboards start afresh holding no Ctrl, so the overlay Ctrl showed
/// before the switch ends at the return, with no key pressed after it.
// covers: osc.8
#[test]
fn a_vt_switch_away_and_back_ends_the_overlay() {
    let mut s = linked_heads();
    let over = over_cell(&s, "large", 5);
    s.app.pointer_event(over, Mods::CTRL, now());
    assert_eq!(overlay_of(&s.app, "large").as_deref(), Some(A_MD));
    s.app.vt.leave().expect("the fake VT");
    s.app.turn(now());
    s.app.turn(now());
    assert!(!s.app.vt.is_active(), "the VT switched away");
    s.app.vt.activate().expect("the fake VT");
    assert!(s.until_memory("/dev/mem1", BOUND, |m| m.awake));
    s.app.turn(now());
    assert_eq!(
        overlay_of(&s.app, "large"),
        None,
        "the overlay ended at the return"
    );
    assert_eq!(
        hovered_on(&s.app, "large").as_deref(),
        Some(A_MD),
        "the link stays underlined"
    );
}

/// ADR-1002, "What a link does": unplugging the keyboard that holds Ctrl
/// ends the overlay. Its route goes before its modifiers' end is queued,
/// so the seat reads the head's keyboards again as the device goes.
// covers: osc.8
#[test]
fn unplugging_the_keyboard_that_holds_ctrl_hides_the_overlay() {
    let mut s = linked_heads();
    let over = over_cell(&s, "large", 5);
    s.app.pointer_event(over, Mods::CTRL, now());
    assert_eq!(overlay_of(&s.app, "large").as_deref(), Some(A_MD));
    s.app.remove_input(Path::new(KBD_FOCUS));
    assert_eq!(overlay_of(&s.app, "large"), None);
}

/// The key events xkb makes of a chord pressed as a keyboard sends it:
/// Left Ctrl, then Left Super, then the key `code` (evdev's), each press
/// an event with the modifiers as they then stand; the releases make
/// none. Each event is the built-in keymap's and the system keymap's
/// alike, with no character, as for the arrow keys.
fn pressed_chord(code: u32, keysym: u32) -> [InputEvent; 3] {
    let press = |code: u32, mods: Mods, keysym: u32| {
        InputEvent::Key(KeyEvent {
            keycode: code + 8,
            ascii: 0,
            mods,
            keysyms: vec![keysym],
            codepoints: vec![None],
            repeat: false,
        })
    };
    let ctrl_super = Mods::CTRL.union(Mods::LOGO);
    [
        press(29, Mods::CTRL, 0xffe3),
        press(125, ctrl_super, 0xffeb),
        press(code, ctrl_super, keysym),
    ]
}

/// The two mirrored displays of [`two_displays`].
const MIRRORED: [&str; 2] = ["/dev/mem0", "/dev/mem1"];

/// Whether the shown screen of `app` holds `got:line`, the session's
/// answer to `line`, with the cursor at the start of the row below it:
/// the screen the answer leaves, whatever reads the pty split.
fn answered(app: &App, line: &str) -> bool {
    let rows = app.screen_text();
    let want = format!("got:{line}");
    let at = rows.iter().position(|l| l.contains(&want));
    let cursor = app.term().map(|t| t.terminal.screen().cursor());
    at.is_some_and(|row| cursor == Some((0, row + 1)))
}

/// The pixels of each display of `s` after the frames the turn just run
/// asked for: the loop first runs until no display has a frame
/// composing, so the next frame each display shows is the turn's; each
/// display's frame count is taken before the turn, and the loop runs
/// until the head's render thread has handed each display its next
/// frame, which [`BOUND`] bounds.
fn next_frames(s: &mut Seat, turn: impl FnOnce(&mut Seat)) -> [Vec<u8>; 2] {
    assert!(
        s.until(BOUND, |a| a.heads().iter().all(|h| h.frames_out() == 0)),
        "no frame composing"
    );
    let before = MIRRORED.map(|node| s.memory(node).frames);
    turn(s);
    for (node, before) in MIRRORED.into_iter().zip(before) {
        assert!(
            s.until_memory(node, BOUND, |m| m.frames > before),
            "{node} draws a frame"
        );
    }
    MIRRORED.map(|node| s.memory(node).pixels.clone())
}

/// The pixels of both displays of `s` drawn from the screen as it is
/// now. Each display is asked to draw whole twice, and the second frame
/// is the one: a display has one frame on its way at a time, so the
/// second is taken from the screen after the first came back, and a
/// frame of an older screen, still on its way, can only be the first.
fn drawn_now(s: &mut Seat) -> [Vec<u8>; 2] {
    let redraw = |s: &mut Seat| {
        for node in MIRRORED {
            s.memory(node).events.push(Event::Redraw(ID));
        }
        s.app.turn(now());
    };
    next_frames(s, redraw);
    next_frames(s, redraw)
}

/// What both displays show of a seat with no head file and one console
/// whose session was sent `line`: the picture of a console with that
/// screen, drawn by a seat that never switched.
fn drawn_alone(line: &str) -> [Vec<u8>; 2] {
    let mut s = Seat::boot(&["--no-blink"], two_displays(), ECHO);
    assert!(s.until(BOUND, |a| a.cells().len() == 2 && shows(a, "ready")));
    s.type_str(&format!("{line}\r"));
    assert!(s.until(BOUND, |a| answered(a, line)));
    drawn_now(&mut s)
}

/// One head of two consoles on both displays of [`two_displays`],
/// mirrored, as the compiled-in configuration's one head mirrors them.
const MIRRORED_TWO: &str = "version = 1
[head console]
consoles = 2
[rules]
display card:mem-small -> console
display card:mem-large -> console
";

/// One head of two consoles on two mirrored displays. Ctrl-Super-Right and
/// Ctrl-Super-Left, pressed as a keyboard sends them, switch the shown
/// console at once, and the frame of the turn that took the keys draws the
/// console now shown on both displays, pixel for pixel as a seat of one
/// console that never switched draws that screen. The only wait after a
/// chord is for that frame to come back from the head's render thread.
#[test]
fn the_console_chords_draw_the_console_they_show() {
    const KBD: &str = "/dev/input/event0";
    let one = drawn_alone("one");
    let two = drawn_alone("two");
    assert_ne!(one, two, "the two screens draw apart");
    let mut s = Seat::with_heads(&["--no-blink"], Some(MIRRORED_TWO), two_displays(), ECHO);
    s.app.add_input(Path::new(KBD), keyboard("kbd"));
    assert!(s.until(BOUND, |a| a.cells().len() == 2 && shows(a, "ready")));
    let shown = |s: &Seat| {
        s.app
            .heads()
            .first()
            .map(|h| (h.console_count(), h.shown_console()))
    };
    let press = |s: &mut Seat, code: u32, keysym: u32| {
        for event in pressed_chord(code, keysym) {
            s.app.device_input(Path::new(KBD), event, now());
        }
    };
    // The chord's keys, the console then shown, and the frame of the turn
    // that took them.
    let chord = |s: &mut Seat, code: u32, keysym: u32| {
        let mut state = None;
        let frame = next_frames(s, |s| {
            press(s, code, keysym);
            state = shown(s);
            s.app.turn(now());
        });
        (state, frame)
    };
    s.type_on(KBD, "one\r");
    assert!(s.until(BOUND, |a| answered(a, "one")));
    assert!(drawn_now(&mut s) == one, "the first console");
    press(&mut s, 106, RIGHT);
    assert_eq!(shown(&s), Some((2, 1)), "Right shows the second");
    assert!(s.until(BOUND, |a| shows(a, "ready")));
    s.type_on(KBD, "two\r");
    assert!(s.until(BOUND, |a| answered(a, "two")));
    assert!(drawn_now(&mut s) == two, "the second console");
    let (state, frame) = chord(&mut s, 106, RIGHT);
    assert_eq!(state, Some((2, 0)), "Right wraps to the first");
    assert!(
        frame == one,
        "Right's frame draws the first console: {}; the screen {:?}",
        apart(&frame, &one),
        s.app.screen_text()
    );
    let (state, frame) = chord(&mut s, 105, LEFT);
    assert_eq!(state, Some((2, 1)), "Left wraps to the second");
    assert!(
        frame == two,
        "Left's frame draws the second console: {}; the screen {:?}",
        apart(&frame, &two),
        s.app.screen_text()
    );
}

/// Where two displays' pictures differ, for a failure's message: for each
/// display, the pixels that differ and the box that holds them.
fn apart(got: &[Vec<u8>; 2], want: &[Vec<u8>; 2]) -> String {
    let widths = [320usize, 640];
    got.iter()
        .zip(want)
        .zip(widths)
        .map(|((g, w), width)| {
            let px: Vec<usize> = (0..g.len().min(w.len()) / 4)
                .filter(|i| g.get(i * 4..i * 4 + 4) != w.get(i * 4..i * 4 + 4))
                .collect();
            let (xs, ys): (Vec<usize>, Vec<usize>) =
                px.iter().map(|i| (i % width, i / width)).unzip();
            format!(
                "{} pixels differ in x {:?}..={:?}, y {:?}..={:?} (lengths {} and {})",
                px.len(),
                xs.iter().min(),
                xs.iter().max(),
                ys.iter().min(),
                ys.iter().max(),
                g.len(),
                w.len()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// D3 and D10: a head starts the consoles its file asks for, at most 4 a
/// head by default and 8 in all, each a session; a head asking for more
/// gets its cap. Each head keeps its first console whatever the total.
#[test]
fn a_head_starts_the_consoles_its_file_asks_for_within_the_caps() {
    let heads = "version = 1
[head a]
consoles = 6
[head b]
consoles = 4
[head c]
consoles = 3
[rules]
display card:mem-a -> a
display card:mem-b -> b
display card:mem-c -> c
";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-a")),
        ("/dev/mem1", Memory::new(320, 160, "MEM-2", "mem-b")),
        ("/dev/mem2", Memory::new(320, 160, "MEM-3", "mem-c")),
    ];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "echo ready; read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 3));
    let counts = |a: &App| -> Vec<usize> { a.heads().iter().map(Head::console_count).collect() };
    assert_eq!(counts(&s.app), [4, 3, 1], "a's cap, then the total's");
    assert!(s.until(BOUND, |a| a.heads().iter().all(Head::started)));
    let raised = Seat::with_heads(
        &["--head-consoles-max=6", "--consoles-max=12"],
        Some(heads),
        [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-a"))],
        "read x",
    );
    assert_eq!(
        counts(&raised.app)[0],
        6,
        "the configuration raises the caps"
    );
}

/// D1, pending the operator's read (ADR-1401's item 1): under respawn a
/// hidden console whose program exits starts it again in its place, and
/// the head keeps showing the console it showed. The hidden console's
/// program exits once, at its first start.
// covers: platform.heads
#[test]
fn under_respawn_a_hidden_consoles_program_starts_again_in_its_place() {
    let dir = Scratch::new("heads-hidden");
    let script = format!(
        "d={d}; echo $VTIUM_CONSOLE >> $d/starts; \
         if [ $VTIUM_CONSOLE = 2 ] && [ ! -e $d/once ]; then : > $d/once; exit 0; fi; \
         echo ready; while read a; do echo \"got:$a\"; done",
        d = dir.display()
    );
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(2)), two_displays(), &script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let starts = || -> Vec<String> {
        let mut lines: Vec<String> = std::fs::read_to_string(dir.join("starts"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        lines.sort();
        lines
    };
    let started = s.until(BOUND, |_| starts().len() == 4);
    let seen = starts();
    assert!(started, "small's two, large's one, and 2 again: {seen:?}");
    assert_eq!(seen, ["1", "1", "2", "2"], "2 started again as 2");
    assert_eq!(
        consoles(&s.app, "small"),
        Some((2, 0)),
        "the first still shown"
    );
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:x")));
}

/// D1, pending the operator's read (ADR-1401's item 1): under `oneshot` a
/// console whose program exits ends and keeps its place. The head shows
/// that the shown console's session ended, naming the console chord while
/// its other console still runs; that one is shown by the chord with no
/// notice, and the head ends only when it has ended too, when the notice
/// names the roaming chord. vtium runs on while large runs.
// covers: platform.heads
#[test]
fn under_oneshot_a_console_whose_program_exits_ends_in_its_place() {
    let mut s = claimed_on(&["--oneshot"], &claimed_consoles(2));
    s.type_on(KBD_SMALL, "q\r");
    assert!(
        s.until(BOUND, |a| notice(a, "small").as_deref()
            == Some(ENDED_CONSOLE)),
        "the shown console's session ended: {:?}",
        notice(&s.app, "small")
    );
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "in its place");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert_eq!(notice(&s.app, "small"), None, "the second runs");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:x")));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(LEFT), now());
    assert_eq!(
        notice(&s.app, "small").as_deref(),
        Some(ENDED_CONSOLE),
        "still ended"
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(LEFT), now());
    assert_eq!(
        notice(&s.app, "small").as_deref(),
        Some(ENDED),
        "no console of small runs"
    );
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.finished(), None, "large runs on");
    s.type_on(KBD_LARGE, "q\r");
    assert!(
        s.until(BOUND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
        "every head ended"
    );
}

/// D1: a head ends only once every one of its consoles has. Under
/// `oneshot`, with large ended first and then one of small's two
/// consoles, vtium runs on while small's other console runs, and exits
/// once that one ends too.
// covers: platform.heads
#[test]
fn under_oneshot_vtium_runs_on_while_any_console_of_any_head_runs() {
    let mut s = claimed_on(&["--oneshot"], &claimed_consoles(2));
    s.type_on(KBD_LARGE, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "large").as_deref() == Some(ENDED)));
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").is_some()));
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.finished(), None, "small's second console runs");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    s.type_on(KBD_SMALL, "q\r");
    assert!(
        s.until(BOUND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
        "every console of every head ended"
    );
}

/// D1: keys typed at a shown console whose session ended under `oneshot`
/// reach no session: the console stays ended in its place, its notice
/// stays, and nothing starts or ends.
#[test]
fn keys_at_an_ended_console_start_nothing() {
    let mut s = claimed_with(&["--oneshot"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    s.type_on(KBD_SMALL, "x\r");
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)));
    assert_eq!(notice(&s.app, "small").as_deref(), Some(ENDED));
    assert!(!head_shows(&s.app, "small", "got:x"));
    assert_eq!(s.app.finished(), None, "large runs on");
}

/// D1 and D5, and ADR-1201 D11: a hidden console's program starts again
/// at that console's own size, which its session's end returns to the
/// configured size. Console 2, zoomed in to 20 by 5, is hidden behind
/// console 1, zoomed out to 40 by 20, when its program exits, and the
/// program started again reads 10 rows of 40, 16 points' grid: neither its
/// zoom nor the shown console's.
// covers: platform.heads
#[test]
fn a_hidden_console_starts_again_at_the_configured_size() {
    let dir = Scratch::new("heads-own-size");
    let script = format!(
        "d={d}; if [ -e $d/done ]; then w=again; else w=size; fi; echo \"$w:$(stty size)\"; \
         if [ $VTIUM_CONSOLE = 2 ] && [ ! -e $d/done ]; then \
         while [ ! -e $d/go ]; do sleep 0.02; done; : > $d/done; exit 0; fi; \
         while read a; do echo \"got:$a\"; done",
        d = dir.display()
    );
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(2)), displays, &script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let sizes = |a: &App| console_lines(a, "small", "size:");
    assert!(s.until(BOUND, |a| sizes(a).iter().all(Option::is_some)));
    // Each key acts within the input call: the chord shows another
    // console, the zoom lays it out again.
    let key = |s: &mut Seat, event: InputEvent| {
        s.app.device_input(Path::new(KBD_SMALL), event, now());
    };
    let zoom = |c: char| {
        InputEvent::Key(KeyEvent {
            mods: Mods::CTRL,
            ..char_key(c)
        })
    };
    key(&mut s, ctrl_super(RIGHT));
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    key(&mut s, zoom('='));
    assert_eq!(grid(&s.app, "small"), Some((20, 5)), "console 2 zoomed in");
    key(&mut s, ctrl_super(LEFT));
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)));
    key(&mut s, zoom('-'));
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 20)),
        "console 1 zoomed out"
    );
    std::fs::write(dir.join("go"), "").expect("the go mark");
    let again = |a: &App| console_lines(a, "small", "again:");
    let restarted = s.until(BOUND, |a| {
        again(a).get(1) == Some(&Some("again:10 40".to_owned()))
    });
    let seen = again(&s.app);
    assert!(
        restarted,
        "console 2 started again at 10 rows of 40: {seen:?}"
    );
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 20)),
        "console 1 keeps its zoom"
    );
}

/// D1, pending the operator's read (ADR-1401's item 1): every console
/// starts again, so a crash loop counts the quick exits of any console of
/// any head. A hidden console's program that keeps exiting at once, in a
/// head whose shown console runs on, makes vtium exit with status 1.
// covers: platform.heads
#[test]
fn a_hidden_consoles_crash_loop_ends_the_run() {
    let script = "if [ \"$VTIUM_HEAD:$VTIUM_CONSOLE\" = small:2 ]; then exit 1; fi; read x";
    let mut s = Seat::with_heads(&[], Some(&claimed_consoles(2)), two_displays(), script);
    assert!(
        s.until(BOUND, |a| a.finished().is_some()),
        "the crash loop ended vtium"
    );
    assert_eq!(s.app.finished(), Some(vtium::app::Exit::TooQuick));
    assert_eq!(consoles(&s.app, "small").map(|(n, _)| n), Some(2));
}

/// Two heads whose consoles run the head file's own `login`: small runs
/// `small`, large runs `large`, each a shell command.
fn logins(args: &[&str], small: &str, large: &str) -> Seat {
    logins_on(args, small, large, two_displays())
}

/// [`logins`] on `displays`.
fn logins_on(
    args: &[&str],
    small: &str,
    large: &str,
    displays: impl IntoIterator<Item = (&'static str, Memory)>,
) -> Seat {
    let heads = format!(
        "version = 1
[head small]
login = /bin/sh -c \"{small}\"
[head large]
login = /bin/sh -c \"{large}\"
[rules]
display card:mem-small -> small
display card:mem-large -> large
"
    );
    Seat::with_heads(args, Some(&heads), displays, "read x")
}

/// D3, and the operator's answers (2026-10-30): each head runs its own
/// `login`; under respawn, a session ending brings back that head's
/// login alone, and the other head's session runs on.
#[test]
fn under_respawn_a_session_ending_restarts_its_head_alone() {
    let dir = Scratch::new("heads-respawn");
    let marker = dir.join("marker");
    let small = format!(
        "if [ -e {m} ]; then echo again; read x; else : > {m}; echo first; fi",
        m = marker.display()
    );
    let mut s = logins(&[], &small, "echo large; read x");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "again")),
        "small's login came back"
    );
    assert!(head_shows(&s.app, "large", "large"), "large runs on");
    assert!(!head_shows(&s.app, "large", "again"));
}

/// The operator's answers (2026-10-30): under oneshot a head whose
/// program ends shows that the session ended and stays dark; vtium exits when the
/// last head's program ends.
#[test]
fn under_oneshot_vtium_exits_when_the_last_heads_program_ends() {
    // Large's program ends once the test writes this file.
    let dir = Scratch::new("heads-oneshot");
    let go = dir.join("go");
    let large = format!(
        "echo large; while [ ! -e {} ]; do sleep 0.05; done",
        go.display()
    );
    let mut s = logins(&["--oneshot"], "echo bye", &large);
    assert!(
        s.until(BOUND, |a| a.head("small").and_then(Head::notice)
            == Some(ENDED)
            && head_shows(a, "large", "large")),
        "small's program ended, large's runs"
    );
    assert_eq!(s.app.finished(), None, "large's runs on");
    assert_eq!(s.app.head("large").and_then(Head::notice), None);
    std::fs::write(&go, b"").expect("the go file");
    assert!(
        s.until(BOUND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
        "the last head's program ended"
    );
}

/// A seat whose head `looping` has one console whose program keeps
/// exiting at once and whose other head has two consoles, each marking
/// its SIGHUP in `dir`. The looping program's first start waits until both
/// of the other head's consoles have set their trap, so the exit comes
/// after them; every start after the first exits at once.
fn crash_loop(dir: &Path, looping: &str, policy: &str) -> Seat {
    let script = format!(
        "d={d}; if [ \"$VTIUM_HEAD\" = {looping} ]; then \
         while [ ! -e $d/ready-1 ] || [ ! -e $d/ready-2 ]; do sleep 0.01; done; exit 1; fi; \
         trap \"echo > $d/hup-$VTIUM_CONSOLE; exit 0\" HUP; : > $d/ready-$VTIUM_CONSOLE; \
         while :; do read x; done",
        d = dir.display()
    );
    let heads = format!(
        "version = 1
policy = {policy}
[head small]
consoles = {small}
[head large]
consoles = {large}
isolated = no
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-large -> large
input path:kbd-roaming -> roaming
",
        small = if looping == "small" { 1 } else { 2 },
        large = if looping == "large" { 1 } else { 2 },
    );
    Seat::with_heads(&[], Some(&heads), two_displays(), &script)
}

/// D1, the operator's decision of 2026-10-31: a session that keeps
/// ending as soon as it starts, on any head, makes vtium exit with status
/// 1, so the unit's getty takes the VT, and the exit hangs up every
/// console of the other head, the hidden one included. No notice is
/// drawn, since vtium leaves in that turn. The loop runs on the first head
/// vtium reaches in a turn, then on the second.
#[test]
fn a_head_restarting_too_fast_ends_vtium_and_hangs_up_every_head() {
    for (looping, other) in [("small", "large"), ("large", "small")] {
        let dir = Scratch::new(&format!("heads-crash-loop-{looping}"));
        let mut s = crash_loop(&dir, looping, "open");
        let finished = s.until(BOUND, |a| a.finished().is_some());
        let marks = [dir.join("hup-1"), dir.join("hup-2")];
        let hung_up = finished && s.until(BOUND, |_| marks.iter().all(|m| m.exists()));
        let marked = marks.iter().map(|m| m.exists()).collect::<Vec<_>>();
        assert!(finished, "{looping}'s crash loop ended vtium");
        assert_eq!(
            consoles(&s.app, other).map(|(n, _)| n),
            Some(2),
            "{other} has a hidden console"
        );
        assert_eq!(s.app.finished(), Some(vtium::app::Exit::TooQuick));
        assert_eq!(s.app.finished().map(vtium::app::Exit::status), Some(1));
        assert!(
            hung_up,
            "the exit hung up both of {other}'s consoles, shown and hidden: {marked:?}"
        );
        assert_eq!(
            s.app.head(looping).and_then(Head::notice),
            None,
            "no notice on {looping}"
        );
    }
}

/// D1, D5 and D9: once a crash loop stopped a closed seat, no keyboard
/// reaches anything: the VT key pressed on the keyboard of `small`, a head
/// isolated by the closed policy, on that of `large`, which is not
/// isolated, and on a roaming keyboard switches no VT, and the keys typed
/// on each reach no session. Two defences hold this: the stopped seat
/// delivers nothing, and the stopped seat's rules keep each head's
/// isolation, which alone keeps `small`'s keyboard from the VT.
// covers: platform.heads.policy
#[test]
fn after_a_closed_crash_loop_no_keyboard_switches_the_vt_or_reaches_a_session() {
    let dir = Scratch::new("heads-crash-loop-keys");
    let mut s = crash_loop(&dir, "small", "closed");
    let roaming = "/dev/input/event6";
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    s.app.add_input(Path::new(roaming), keyboard("kbd-roaming"));
    let stopped = vtium::notice::stopped();
    let shown = s.until(BOUND, |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    assert!(shown, "the crash loop stopped the seat");
    // the isolated head's keyboard first, so its check stands alone
    for node in [KBD_SMALL, KBD_LARGE, roaming] {
        s.app.device_input(Path::new(node), vt_key(), now());
        assert!(s.app.vt.is_active(), "{node}: the VT stays");
        s.type_on(node, "typed\r");
    }
    s.until(Duration::from_millis(200), |_| false);
    for head in s.app.heads() {
        assert!(
            head.consoles_text()
                .iter()
                .flatten()
                .all(|l| !l.contains("typed")),
            "{}: nothing reached it",
            head.name()
        );
    }
}

/// D1 and D9: under `policy = closed` a crash loop ends the run the same
/// way, every head's sessions hung up, but vtium does not exit, since a
/// getty's login on its VT would take any keyboard. It holds its VT: every
/// head's display goes dark, no session starts again, no keyboard
/// delivers, and the boot display shows why on the notice screen for as
/// long as vtium runs.
// covers: platform.heads.policy
#[test]
fn under_closed_a_crash_loop_stops_every_head_and_holds_the_vt() {
    let dir = Scratch::new("heads-crash-loop-closed");
    let mut s = crash_loop(&dir, "small", "closed");
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    let marks = [dir.join("hup-1"), dir.join("hup-2")];
    let hung_up = s.until(BOUND, |_| marks.iter().all(|m| m.exists()));
    let stopped = vtium::notice::stopped();
    let shown = s.until(BOUND, |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    assert!(hung_up, "large's consoles hung up, shown and hidden");
    assert!(shown, "the notice on the notice screen");
    assert!(
        s.until(BOUND, |a| a
            .heads()
            .iter()
            .all(|h| h.cells().is_empty() && h.pty().is_none())),
        "every head dark, no session"
    );
    assert_eq!(s.app.finished(), None, "vtium holds its VT");
    assert!(s.app.vt.is_active());
    assert_eq!(
        s.app.notice_screen().map(|n| n.cells().len()),
        Some(1),
        "the boot display alone"
    );
    s.app.turn(now() + Duration::from_secs(31));
    assert_eq!(
        s.app.notice_screen().and_then(Head::notice),
        Some(stopped.as_str()),
        "for good"
    );
}

/// D1: a VT switch away and back after a closed crash loop's stop changes
/// nothing. The acquire wakes every device, but no session starts again,
/// no keyboard types into a session, the VT key of `large`, a head that is
/// not isolated, switches no VT, and the notice stays.
// covers: platform.heads.policy
#[test]
fn after_a_closed_crash_loop_a_vt_switch_away_and_back_changes_nothing() {
    let dir = Scratch::new("heads-crash-loop-vt-switch");
    let mut s = crash_loop(&dir, "small", "closed");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    let stopped = vtium::notice::stopped();
    let shown = s.until(BOUND, |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    assert!(shown, "the crash loop stopped the seat");
    // A turn takes the release and puts the displays to sleep, and the next
    // reads them asleep and lets the VT go, so the activation takes it back.
    s.app.vt.leave().expect("the fake VT");
    s.app.turn(now());
    s.app.turn(now());
    assert!(!s.app.vt.is_active(), "the VT switched away");
    s.app.vt.activate().expect("the fake VT");
    assert!(s.app.vt.is_active(), "and back");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.awake)
            && s.until_memory("/dev/mem1", BOUND, |m| m.awake),
        "the acquire woke every display"
    );
    s.until(Duration::from_millis(300), |_| false);
    assert!(
        s.app.heads().iter().all(|h| h.pty().is_none()),
        "no session started again"
    );
    s.app.device_input(Path::new(KBD_LARGE), vt_key(), now());
    assert!(s.app.vt.is_active(), "the VT key switches no VT");
    s.type_on(KBD_LARGE, "typed\r");
    s.until(Duration::from_millis(200), |_| false);
    for head in s.app.heads() {
        assert!(
            head.consoles_text()
                .iter()
                .flatten()
                .all(|l| !l.contains("typed")),
            "{}: nothing reached it",
            head.name()
        );
    }
    assert_eq!(
        s.app.notice_screen().and_then(Head::notice),
        Some(stopped.as_str()),
        "the notice stays"
    );
}

/// D1: a zoom changes the grid of the console shown alone; each console
/// keeps its own grid, which shows again with it.
#[test]
fn a_zoom_changes_the_grid_of_the_console_shown_alone() {
    let mut s = claimed_on(&[], &claimed_consoles(2));
    // A chord and a zoom act as the key is delivered.
    let key = |s: &mut Seat, event: InputEvent| {
        s.app.device_input(Path::new(KBD_SMALL), event, now());
    };
    key(&mut s, ctrl_super(RIGHT));
    let equal = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('=')
    });
    key(&mut s, equal);
    assert_eq!(grid(&s.app, "small"), Some((20, 5)), "zoomed to 16x32");
    key(&mut s, ctrl_super(RIGHT));
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 10)),
        "the first console's own"
    );
    key(&mut s, ctrl_super(RIGHT));
    assert_eq!(
        grid(&s.app, "small"),
        Some((20, 5)),
        "the second keeps its zoom"
    );
}

/// D5: pointers follow the same rules: a pointer a rule sends to a head
/// moves on that head's displays, whichever head holds the focus; the
/// pointer's hide reaches every head.
#[test]
fn a_claimed_pointer_moves_on_its_heads_display_alone() {
    let heads = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:mouse-small -> small
input class:mouse -> roaming
";
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    let mouse = vtium_settings::Identity {
        path: Some("mouse-small".to_owned()),
        ..vtium_settings::Identity::input().with_class(Class::Mouse)
    };
    s.app.add_input(Path::new("/dev/input/event5"), mouse);
    let moved = InputEvent::Pointer(vtium_platform::input::PointerEvent::Moved {
        x: 24,
        y: 40,
        held: None,
    });
    s.app
        .device_input(Path::new("/dev/input/event5"), moved, now());
    assert_eq!(s.app.focus_head(), Some("large"));
    assert_eq!(s.app.head("small").and_then(Head::pointer), Some((24, 40)));
    assert_eq!(s.app.head("large").and_then(Head::pointer), None);
    // The hide comes from no one device: it hides the pointer on every
    // head.
    let hide = InputEvent::Pointer(vtium_platform::input::PointerEvent::Hide);
    s.app.input_event(hide, now());
    assert_eq!(s.app.head("small").and_then(Head::pointer), None);
}

/// The operator's answers (2026-10-30), with the team lead's steering of
/// 2026-10-30: a head whose display never lit has nothing left to run, so
/// it holds up no exit. With the large display unplugged, the small
/// head's program ending under `oneshot` ends vtium.
#[test]
fn a_head_that_never_lit_holds_up_no_exit() {
    let small = || [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut once = logins_on(&["--oneshot"], "echo bye", "read x", small());
    assert!(
        once.until(BOUND, |a| a.finished().is_some()),
        "the small head's program ended, and the large never lit"
    );
    assert_eq!(once.app.finished(), Some(vtium::app::Exit::Oneshot));
}

/// A hidden console's OSC 50 waits until it is shown: the font is the
/// head's, and only the console shown switches it.
#[test]
fn a_hidden_consoles_font_request_waits_until_it_is_shown() {
    let heads = "version = 1
[head one]
consoles = 2
[rules]
display card:mem-small -> one
";
    // `asking` is written with the request, and its console's screen
    // shows it once the console has read that far.
    let script = "echo ready; read a; printf 'asking\\033]50;unscii-8\\007'; echo asked; read b";
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(heads), displays, script);
    assert!(s.until(BOUND, |a| head_shows(a, "one", "ready")));
    // A chord shows another console as the key is delivered, so the
    // second console is hidden before any turn reads what the line
    // typed into it makes its session write.
    let chord = |s: &mut Seat, keysym: u32| {
        s.app.input_event(ctrl_super(keysym), now());
    };
    chord(&mut s, RIGHT);
    assert!(s.until(BOUND, |a| head_shows(a, "one", "ready")));
    s.type_str("x\r");
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "one"), Some((2, 0)), "the second hidden");
    let asked = |a: &App| {
        a.head("one").is_some_and(|h| {
            h.consoles_text()
                .iter()
                .flatten()
                .any(|l| l.contains("asking"))
        })
    };
    assert!(s.until(BOUND, asked), "the hidden console asked");
    assert_eq!(s.app.font_name(), "unifont", "the request waits");
    chord(&mut s, RIGHT);
    assert!(
        s.until(BOUND, |a| a.font_name() == "unscii-8"),
        "shown, it switches the font"
    );
    assert!(s.until(BOUND, |a| head_shows(a, "one", "asked")));
}

/// The files a [`losing`] seat's sessions write, in a directory of the
/// test's own, removed when dropped, whether the test passed or not.
struct Files {
    dir: PathBuf,
    script: PathBuf,
    /// Written at a session's SIGHUP.
    mark: PathBuf,
    /// A line at each session's start: its console's `VTIUM_CONSOLE`.
    starts: PathBuf,
}

impl Files {
    /// How many sessions have started.
    fn starts(&self) -> usize {
        self.started().len()
    }

    /// The console of each session started, sorted.
    fn started(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_to_string(&self.starts)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        ids.sort();
        ids
    }
}

impl Drop for Files {
    /// Retires the directory and removes it. The [`losing`] test drops
    /// its files before its seat, whose drop ends the sessions it shows
    /// and sends SIGHUP to any other, which then writes its mark by the
    /// directory's path; once the directory is retired, that write fails.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(scratch::retire(&self.dir));
    }
}

/// A session still running when its test ends writes by the path of a
/// scratch directory after the guard has started removing it. The guard
/// retires the directory first, so the late write fails and nothing is
/// left beside the retired directory once it is removed.
#[test]
fn a_write_after_a_scratch_directory_is_retired_fails_and_leaves_nothing() {
    let dir = Scratch::new("heads-retire");
    let old = dir.to_path_buf();
    std::fs::write(old.join("starts"), "1\n").expect("a session's write");
    let gone = scratch::retire(&old);
    assert_ne!(gone, old, "the directory is renamed");
    let late = std::fs::write(old.join("hup"), "\n");
    assert_eq!(
        late.map_err(|e| e.kind()),
        Err(std::io::ErrorKind::NotFound),
        "a write by the old path after the rename fails"
    );
    assert!(!old.exists(), "the late write brought nothing back");
    std::fs::remove_dir_all(&gone).expect("the retired directory removed");
    assert!(!gone.exists() && !old.exists(), "nothing is left");
    drop(dir);
    assert!(!old.exists(), "the guard's own drop leaves nothing");
}

/// A head losing a display (ADR-1401 D8): the seat booted with `args`,
/// `small` holding the small display, or both when `mirrored`, with the
/// head file's `keys`, each of its sessions the script `test` writes,
/// which counts its start, marks its SIGHUP, echoes each line and exits
/// at `q`; `large` holds the large display. Each head claims a keyboard,
/// so one reaches a lit head while `small` has none and the anti-lockout
/// guard never acts (D9). Returns the seat, booted, and the sessions'
/// files.
fn losing(test: &str, args: &[&str], keys: &str, mirrored: bool) -> (Seat, Files) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vtium-heads-losing-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test's directory");
    let files = Files {
        script: dir.join("session.sh"),
        mark: dir.join("hup"),
        starts: dir.join("starts"),
        dir,
    };
    // The trap is set before the start is counted, so a test that waits
    // for the count can hang the session up and find its mark.
    std::fs::write(
        &files.script,
        format!(
            "trap 'echo > {m}; exit 0' HUP\n\
             echo $VTIUM_CONSOLE >> {starts}\necho ready\n\
             while :; do read a; [ \"$a\" = q ] && exit 0; echo \"got:$a\"; done\n",
            starts = files.starts.display(),
            m = files.mark.display()
        ),
    )
    .expect("the script");
    let large = if mirrored { "small" } else { "large" };
    let heads = format!(
        "version = 1
[head small]
{keys}
login = /bin/sh {script}
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> {large}
input path:kbd-small -> small
input path:kbd-large -> large
",
        script = files.script.display()
    );
    let mut s = Seat::with_heads(args, Some(&heads), two_displays(), "read x");
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    // A frame on a screen ends the start's wait, which a turn far ahead
    // would otherwise run out.
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    (s, files)
}

impl Seat {
    /// The display of the memory device at `node` goes, in a turn at `at`.
    fn unplug(&mut self, node: &str, at: Instant) {
        self.memory(node).events.push(Event::Removed(ID));
        self.app.turn(at);
    }

    /// The memory device at `node` can no longer light its display, which
    /// stays connected, in a turn at `at`.
    fn unlight(&mut self, node: &str, at: Instant) {
        self.memory(node).unlight();
        self.app.turn(at);
    }

    /// The memory device at `node` lights its display again, in a turn at
    /// `at`.
    fn relight(&mut self, node: &str, at: Instant) {
        self.memory(node).relight();
        self.app.turn(at);
    }

    /// The display of the memory device at `node` comes back, in a turn at
    /// `at`.
    fn plug(&mut self, node: &str, at: Instant) {
        self.memory(node).events.push(Event::Added(ID));
        self.app.turn(at);
    }

    /// The process id of head `name`'s shown console's session.
    fn session(&self, name: &str) -> Option<u32> {
        self.app
            .head(name)
            .and_then(Head::pty)
            .and_then(|p| p.child_id())
    }

    /// Whether head `name`'s session was hung up within `limit`: its
    /// SIGHUP marked `mark`.
    fn hung_up(&mut self, mark: &Path, limit: Duration) -> bool {
        self.until(limit, |_| mark.exists())
    }
}

/// D8: under `when-display-lost = hangup` a head losing its last display hangs its
/// sessions up at once, as a line losing its carrier: the pty closes and
/// the session gets SIGHUP. The head then follows the respawn policy,
/// which starts its login again; the other head runs on.
#[test]
fn under_hangup_a_head_losing_its_last_display_hangs_its_sessions_up_at_once() {
    let (mut s, f) = losing("hangup", &[], "when-display-lost = hangup", false);
    let before = s.session("small");
    let large = s.session("large");
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, BOUND), "SIGHUP at once");
    assert!(
        s.until(BOUND, |a| a.head("small").and_then(Head::pty).is_some()),
        "the respawn policy starts the login again"
    );
    assert_ne!(s.session("small"), before, "a new session");
    assert_eq!(s.session("large"), large, "the other head runs on");
    assert!(s.app.finished().is_none());
}

/// D8: under `keep`, the default, a head losing its last display keeps
/// its session, however long the display stays away, and the keys of the
/// devices reaching it still type into it, as the kernel console's tty
/// does; a display of the head coming back draws it again.
#[test]
fn under_keep_a_head_losing_its_last_display_runs_on_and_shows_again_on_its_return() {
    let (mut s, f) = losing("keep", &[], "when-display-lost = keep", false);
    let before = s.session("small");
    let t = now();
    s.unplug("/dev/mem0", t);
    s.app.turn(t + Duration::from_secs(3600));
    s.until(Duration::from_millis(300), |_| false);
    assert!(!f.mark.exists(), "no SIGHUP");
    assert_eq!(s.session("small"), before, "the session runs on");
    s.type_on(KBD_SMALL, "x\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:x")),
        "the keys still reach it"
    );
    let frames = s.memory("/dev/mem0").frames;
    s.plug("/dev/mem0", t + Duration::from_secs(3601));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "drawn again on its return"
    );
    assert_eq!(s.session("small"), before);
}

/// D8: a display the device cannot light beside the others, its monitor
/// still plugged in, stays present but dark. Under `when-display-lost =
/// hangup` its head keeps the display and its session while the display
/// stays dark, nothing is drawn on it, and the keys of the devices reaching
/// the head still type into it. Once a modeset lights the display, the
/// head draws on it again.
#[test]
fn under_hangup_a_display_left_unlit_hangs_nothing_up_and_draws_once_lit() {
    let (mut s, f) = losing("unlit", &[], "when-display-lost = hangup", false);
    let before = s.session("small");
    let t = now();
    s.unlight("/dev/mem0", t);
    let frames = s.memory("/dev/mem0").frames;
    s.app.turn(t + Duration::from_secs(1));
    assert!(!s.hung_up(&f.mark, Duration::from_millis(300)), "no SIGHUP");
    assert_eq!(s.session("small"), before, "the session runs on");
    assert_eq!(
        s.app.head("small").map(|h| h.cells().len()),
        Some(1),
        "the head keeps its display"
    );
    s.type_on(KBD_SMALL, "x\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:x")),
        "the keys still reach it"
    );
    assert_eq!(s.memory("/dev/mem0").frames, frames, "nothing drawn");
    s.relight("/dev/mem0", t + Duration::from_secs(2));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "drawn again once lit"
    );
    assert_eq!(s.session("small"), before);
}

/// D8: under `hangup 30s` a display returning within the 30 seconds
/// resumes the head's sessions; one staying away longer hangs them up
/// after it. The time is the loop's, which the test gives.
#[test]
fn under_hangup_30s_a_return_within_the_time_resumes_and_after_it_hangs_up() {
    let (mut s, f) = losing("hangup-30s", &[], "when-display-lost = hangup 30s", false);
    let before = s.session("small");
    let t = now();
    let at = |secs: u64| t + Duration::from_secs(secs);
    s.unplug("/dev/mem0", at(0));
    s.app.turn(at(29));
    s.plug("/dev/mem0", at(29));
    s.app.turn(at(31));
    s.app.turn(at(100));
    assert!(!s.hung_up(&f.mark, Duration::from_millis(300)), "resumed");
    assert_eq!(s.session("small"), before);
    s.unplug("/dev/mem0", at(100));
    s.app.turn(at(129));
    assert!(
        !s.hung_up(&f.mark, Duration::from_millis(300)),
        "within the time"
    );
    s.app.turn(at(130));
    assert!(s.hung_up(&f.mark, BOUND), "after it, SIGHUP");
}

/// D8: a mirrored head keeps its sessions while any of its displays
/// remains, under `hangup` too; its last going hangs them up.
#[test]
fn a_mirrored_head_keeps_its_sessions_while_one_display_remains() {
    let (mut s, f) = losing("mirrored", &[], "when-display-lost = hangup", true);
    assert_eq!(s.app.head("small").map(|h| h.cells().len()), Some(2));
    let before = s.session("small");
    s.unplug("/dev/mem1", now());
    assert!(
        !s.hung_up(&f.mark, Duration::from_millis(300)),
        "one remains"
    );
    assert_eq!(s.session("small"), before);
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, BOUND), "the last went");
}

/// D8: a head reattaches only to a display its rules match. A display no
/// rule gives the head stops neither its absence nor its timer; its own
/// display coming back reattaches it.
#[test]
fn a_head_reattaches_only_to_a_display_its_rules_match() {
    let (mut s, f) = losing("reattach", &[], "when-display-lost = hangup 30s", false);
    let t = now();
    s.unplug("/dev/mem0", t);
    s.app.add_video(
        PathBuf::from("/dev/mem2"),
        Box::new(Memory::new(320, 160, "MEM-3", "mem-other")),
    );
    s.app.turn(t + Duration::from_secs(1));
    assert_eq!(s.app.head("small").map(|h| h.cells().len()), Some(0));
    s.app.turn(t + Duration::from_secs(30));
    assert!(s.hung_up(&f.mark, BOUND), "its timer ran out");
    s.plug("/dev/mem0", t + Duration::from_secs(31));
    assert_eq!(s.app.head("small").map(|h| h.cells().len()), Some(1));
}

/// D8: the focus never moves to a head or away from one because its
/// display left or came back, even before the first key settles it. A
/// keyboard `small` claims reaches a lit head throughout, so the
/// anti-lockout guard, the one other move of the focus, never acts (D9).
#[test]
fn the_focus_never_moves_because_a_heads_display_left_or_came_back() {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, "read x");
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"), "the boot display's");
    s.unplug("/dev/mem1", now());
    s.app.add_video(
        PathBuf::from("/dev/mem2"),
        Box::new(Memory::new(320, 160, "MEM-3", "mem-small")),
    );
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"), "it left");
    s.plug("/dev/mem1", now());
    assert!(s.until(BOUND, |a| a.cells().len() == 3));
    assert_eq!(s.app.focus_head(), Some("large"), "it came back");
}

/// D8: a head whose display never lit receives nothing, even from a
/// keyboard its rule claims.
#[test]
fn a_head_whose_display_never_lit_receives_no_key() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, "read x");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    s.type_on(KBD_LARGE, "never");
    s.app
        .device_input(Path::new(KBD_LARGE), ctrl_super(RETURN), now());
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "large", "never"));
    assert!(
        s.app.head("large").is_some_and(
            |h| h.pty().is_none() && h.screen_text().iter().all(|l| l.trim().is_empty())
        ),
        "the chord's Return reached no session"
    );
    assert_eq!(consoles(&s.app, "large"), Some((1, 0)));
}

/// D5 and D8: before the first key the focus starts over every display
/// lit since the start, one that left included, so the boot display
/// lighting late takes it even after the focus's head lost its display.
#[test]
fn a_late_boot_display_takes_the_focus_after_the_focus_heads_display_left() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(s.app.focus_head(), Some("small"));
    s.unplug("/dev/mem0", now());
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
    );
    assert!(s.until(BOUND, |a| a.head("large").map(|h| h.cells().len())
        == Some(1)));
    assert_eq!(s.app.focus_head(), Some("large"), "no key yet");
}

/// D5 and D8: only a key reaching a head settles the focus's start; one
/// from the keyboard of a head whose display never lit reaches none.
#[test]
fn a_key_reaching_no_head_leaves_the_start_focus_open() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, "read x");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| a.cells().len() == 1));
    assert_eq!(s.app.focus_head(), Some("small"));
    s.type_on(KBD_LARGE, "k");
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
    );
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"), "the boot display's");
}

/// A session that shows each byte it reads, as hex, once a key arrives.
const BYTES: &str =
    "stty -icanon -echo; echo ready; while :; do echo \"k:$(head -c 1 | od -An -tx1)\"; done";

/// D5: the roaming chord pressed on a keyboard a rule sends to a head moves
/// nothing and reaches that head's session as an ordinary key, from a lit
/// head's keyboard and from an unlit one's, which receives nothing (D8).
// covers: platform.heads.focus
#[test]
fn the_roaming_chord_from_a_claimed_keyboard_moves_nothing_and_types_it() {
    let heads = "version = 1
[head small]
[head large]
[head unlit]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-unlit -> unlit
";
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(heads), displays, BYTES);
    let unlit = "/dev/input/event3";
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(unlit), keyboard("kbd-unlit"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.app.device_input(Path::new(unlit), ctrl_super(UP), now());
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move(), None, "nothing moved");
    assert_eq!(s.app.focus_head(), Some("large"));
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "k: 1b")),
        "small's session read the chord as a key"
    );
}

/// D5: the roaming chord moves the active head only to a lit head that is
/// not isolated, so with the other lit head isolated it moves nothing.
// covers: platform.heads.isolated
#[test]
fn the_roaming_chord_skips_an_isolated_head() {
    let heads = TWO_HEADS.replace("[head small]\n", "[head small]\nisolated = yes\n");
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, "read x");
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move(), None);
    assert_eq!(s.app.focus_head(), Some("large"), "small is isolated");
}

/// The fake VT's switch key, Ctrl-Super-F12, which stands in for the
/// kernel's `Ctrl-Alt-F<n>`.
fn vt_key() -> InputEvent {
    ctrl_super(0xffc9)
}

/// D5 and D11's VT-keys test: an isolated head's keyboard sends the VT's
/// keys to its session as an ordinary key and the VT stays; a keyboard of
/// a head that is not isolated switches the VT.
// covers: platform.heads.isolated
#[test]
fn an_isolated_heads_keyboard_sends_the_vt_keys_to_its_session() {
    let heads = CLAIMED.replace("[head small]\n", "[head small]\nisolated = yes\n");
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, BYTES);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.app.device_input(Path::new(KBD_SMALL), vt_key(), now());
    assert!(s.app.vt.is_active(), "the VT stays");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "k: 1b")),
        "the session read it as a key"
    );
    s.app.device_input(Path::new(KBD_LARGE), vt_key(), now());
    assert!(
        !s.app.vt.is_active(),
        "large is not isolated: the VT switched"
    );
}

/// D3 and D5: under `policy = closed` a key from a device the seat has no
/// route for, as one queued before its device went, delivers nothing, the
/// VT's keys included, though a head that is not isolated is active; under
/// open it roams as an unclaimed keyboard does.
// covers: platform.heads.policy
#[test]
fn under_closed_a_device_with_no_route_delivers_nothing() {
    let unknown = "/dev/input/event9";
    let large_open = TWO_HEADS.replace("[head large]\n", "[head large]\nisolated = no\n");
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    for closed in [true, false] {
        let heads = if closed {
            large_open.replace("version = 1\n", "version = 1\npolicy = closed\n")
        } else {
            large_open.clone()
        };
        let [small, (node, large)] = two_displays();
        let displays = [small, (node, large.lit())];
        let mut s = Seat::with_heads(&[], Some(&heads), displays, script);
        assert!(s.until(BOUND, |a| head_shows(a, "large", "ready")));
        assert_eq!(s.app.focus_head(), Some("large"), "closed: {closed}");
        s.type_on(unknown, "u\r");
        if closed {
            s.until(Duration::from_millis(200), |_| false);
            assert!(!head_shows(&s.app, "large", "got:u"), "nothing typed");
        } else {
            assert!(
                s.until(BOUND, |a| head_shows(a, "large", "got:u")),
                "it roams"
            );
        }
        s.app.device_input(Path::new(unknown), vt_key(), now());
        assert_eq!(s.app.vt.is_active(), closed, "closed: {closed}: the VT key");
    }
}

/// D3 and D5: the same through the device's handle, the path every event
/// the input drains takes. Under `policy = closed` an event drained from a
/// keyboard that went before its turn, whose handle the seat no longer
/// routes, delivers nothing, the VT's keys included; under open it roams.
// covers: platform.heads.policy
#[test]
fn under_closed_an_event_from_a_handle_with_no_route_delivers_nothing() {
    use vtium_platform::input::{DeviceInput, Direct, Input, InputConfig, Keymap};
    let gone = "/dev/input/event9";
    let large_open = TWO_HEADS.replace("[head large]\n", "[head large]\nisolated = no\n");
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    for closed in [true, false] {
        let heads = if closed {
            large_open.replace("version = 1\n", "version = 1\npolicy = closed\n")
        } else {
            large_open.clone()
        };
        let [small, (node, large)] = two_displays();
        let displays = [small, (node, large.lit())];
        let mut s = Seat::with_heads(&[], Some(&heads), displays, script);
        assert!(s.until(BOUND, |a| head_shows(a, "large", "ready")));
        assert_eq!(s.app.focus_head(), Some("large"), "closed: {closed}");
        // Asleep, the input opens no device but gives each its handle.
        let keymap = Keymap::fallback().expect("the fallback keymap");
        s.app.input = Some(Input::new(Direct, keymap, InputConfig::default()));
        s.app.add_input(Path::new(gone), keyboard("kbd-gone"));
        let id = s
            .app
            .input
            .as_ref()
            .and_then(|i| i.find(Path::new(gone)))
            .expect("the keyboard's handle");
        s.app.remove_input(Path::new(gone));
        let drained = |event| DeviceInput {
            device: Some(id),
            rescue: false,
            event,
        };
        for c in "u\r".chars() {
            s.app
                .drained_input(drained(InputEvent::Key(char_key(c))), now());
        }
        if closed {
            s.until(Duration::from_millis(200), |_| false);
            assert!(!head_shows(&s.app, "large", "got:u"), "nothing typed");
        } else {
            assert!(
                s.until(BOUND, |a| head_shows(a, "large", "got:u")),
                "it roams"
            );
        }
        s.app.drained_input(drained(vt_key()), now());
        assert_eq!(s.app.vt.is_active(), closed, "closed: {closed}: the VT key");
    }
}

/// D5: a roaming keyboard types into the active head, and with none,
/// since under `policy = closed` every head is isolated unless it says
/// otherwise, it delivers nothing. A rule sends it to `roaming`, so a path
/// places it, and its VT keys still switch the VT (D3, "Who switches
/// VTs").
// covers: platform.heads.isolated
#[test]
fn a_roaming_keyboard_with_no_active_head_delivers_nothing_but_switches_vts() {
    let heads = TWO_HEADS.replace("version = 1\n", "version = 1\npolicy = closed\n");
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, BYTES);
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| head_shows(a, "large", "ready")));
    assert_eq!(s.app.focus_head(), None, "no head can be the active head");
    s.type_on(KBD_FOCUS, "x");
    s.app.device_input(Path::new(KBD_FOCUS), vt_key(), now());
    assert!(!s.app.vt.is_active(), "the VT switches");
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "large", "k:") && !head_shows(&s.app, "small", "k:"));
}

/// D8 and the respawn policy: a hang-up is vtium's, not the session's
/// exit, so a display flapping under `hangup` brings the login back each
/// time and never gives the head up as exiting too quickly.
#[test]
fn a_display_flapping_under_hangup_brings_the_login_back_each_time() {
    let (mut s, f) = losing("flap", &[], "when-display-lost = hangup", false);
    for n in 1..=7 {
        s.unplug("/dev/mem0", now());
        assert!(s.hung_up(&f.mark, BOUND), "hang-up {n}");
        let _ = std::fs::remove_file(&f.mark);
        assert!(s.until(BOUND, |_| f.starts() == n + 1), "start {}", n + 1);
        s.plug("/dev/mem0", now());
        assert!(
            s.until(BOUND, |a| a
                .head("small")
                .is_some_and(|h| !h.cells().is_empty())),
            "the display is back on its head"
        );
    }
    assert_eq!(s.app.head("small").and_then(Head::notice), None);
    assert!(s.app.head("small").and_then(Head::pty).is_some());
    assert_eq!(s.app.finished(), None);
}

/// D8: a session's exit read in the turn of its head's hang-up is the
/// hung-up session's, never the new one's: the new session runs on, and
/// no quick exit is counted, so seven in a row never give the head up.
#[test]
fn an_exit_read_before_a_hang_up_is_the_hung_up_sessions() {
    let (mut s, _files) = losing("exit-and-hangup", &[], "when-display-lost = hangup", false);
    for n in 1..=7 {
        s.type_on(KBD_SMALL, "q\r");
        // The loop reads the exit, and no turn takes it before the hang-up:
        // it dispatches, with no turn, until the exit is read.
        let read = |a: &App| a.head("small").is_some_and(|h| h.term().exited.is_some());
        let end = now() + BOUND;
        while !read(&s.app) && now() < end {
            s.lp.dispatch(Some(Duration::from_millis(20)), &mut s.app)
                .expect("dispatch");
        }
        assert!(read(&s.app), "exit {n} is read, its turn not yet come");
        s.unplug("/dev/mem0", now());
        assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
        s.plug("/dev/mem0", now());
        assert!(
            s.until(BOUND, |a| a
                .head("small")
                .is_some_and(|h| !h.cells().is_empty())),
            "the display is back on its head"
        );
    }
    assert_eq!(s.app.head("small").and_then(Head::notice), None);
    assert!(s.app.head("small").and_then(Head::pty).is_some());
}

/// D8 and the operator's answers (2026-10-30): a head hung up comes back
/// with the consoles its file asks for, as at its start. Each session
/// starts again in its own console, which keeps its `VTIUM_CONSOLE` id
/// (D5).
#[test]
fn a_head_hung_up_comes_back_with_the_consoles_its_file_asks_for() {
    let (mut s, f) = losing(
        "consoles",
        &[],
        "when-display-lost = hangup\nconsoles = 4",
        false,
    );
    assert!(s.until(BOUND, |a| consoles(a, "small") == Some((4, 0))));
    assert!(s.until(BOUND, |_| f.starts() == 4));
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, BOUND));
    assert!(s.until(BOUND, |_| f.starts() == 8), "four more");
    assert_eq!(consoles(&s.app, "small"), Some((4, 0)));
    assert_eq!(
        f.started(),
        ["1", "1", "2", "2", "3", "3", "4", "4"],
        "each console again with its id"
    );
}

/// D8 and the operator's answers (2026-10-30): under `oneshot` a head
/// hung up ends with the notice that the session ended, which stands first
/// in the head's box, and vtium exits once no head has anything left to
/// run.
#[test]
fn under_oneshot_a_hang_up_ends_the_head_and_then_vtium() {
    let (mut s, f) = losing(
        "oneshot",
        &["--oneshot"],
        "when-display-lost = hangup",
        true,
    );
    s.unplug("/dev/mem1", now());
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, BOUND));
    let shown = notice(&s.app, "small").unwrap_or_default();
    assert_eq!(shown.lines().next(), Some(ENDED), "{shown}");
    assert!(
        s.until(BOUND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
        "large never lit, so nothing is left to run"
    );
}

/// A head file on disk, in a directory of its own that only the test's
/// user can write, read as the start reads root's: the file, and the trust
/// that reads it. The directory goes when it is dropped. Each head file
/// has its own directory, however many tests make one of the same name at
/// once.
struct HeadFile {
    dir: PathBuf,
    path: PathBuf,
}

impl HeadFile {
    fn new(test: &str, text: &str) -> HeadFile {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        // under the build's own temporary directory, which no other user
        // can write, so the ownership check passes it (ADR-1403 D3)
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("vtium-heads-{test}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("0700");
        let path = dir.join("vtium.conf");
        std::fs::write(&path, text).expect("vtium.conf");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("0644");
        HeadFile { dir, path }
    }

    /// What the start runs for the file, read as the console reads its
    /// vtium.conf, by the account the test runs as (ADR-1403 D3).
    fn rules(&self) -> Start {
        let configdir = OsString::from(format!("--configdir={}", self.dir.display()));
        let loaded = vtium::config::load(&[configdir], &mut vtium::config::read_file);
        vtium::head::rules(&loaded.path, &loaded.file)
    }

    /// Every file of the directory, each with the hash of its bytes and
    /// its modification time.
    fn listing(&self) -> Vec<(OsString, u64, std::time::SystemTime)> {
        use std::hash::{Hash, Hasher};
        let mut out: Vec<_> = std::fs::read_dir(&self.dir)
            .expect("the directory")
            .map(|e| {
                let e = e.expect("an entry");
                let mut h = std::hash::DefaultHasher::new();
                // a directory at the path has no bytes to hash
                std::fs::read(e.path()).unwrap_or_default().hash(&mut h);
                let modified = e.metadata().and_then(|m| m.modified()).expect("its time");
                (e.file_name(), h.finish(), modified)
            })
            .collect();
        out.sort();
        out
    }
}

impl Drop for HeadFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `Ctrl-Alt-F<n>`, the kernel's key for VT `n`.
fn ctrl_alt_f(n: u32) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        mods: Mods::CTRL.union(Mods::ALT),
        keysyms: vec![0xffbe + n - 1],
        codepoints: vec![None],
        repeat: false,
    })
}

/// [`CLAIMED`] with a keyboard the rules ignore.
const RESCUE: &str = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-ignored -> ignore
input class:keyboard -> roaming
";

const KBD_IGNORED: &str = "/dev/input/event3";

/// Two heads booted on `rules`, the large display the boot display, so
/// the focus rests on `large`, with [`KBD_SMALL`] and [`KBD_IGNORED`].
fn rescue_seat(rules: Start) -> Seat {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_rules(&[], Some(rules), displays, script, no_fonts());
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app
        .add_input(Path::new(KBD_IGNORED), keyboard("kbd-ignored"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    assert_eq!(s.app.focus_head(), Some("large"));
    s
}

/// D9 and D11's rescue row: `Ctrl-Alt-F<n>` for vtium's own VT, pressed on
/// a keyboard a head claims or on one the rules ignore, makes that
/// keyboard follow the focus for the rest of the boot, and writes nothing:
/// the head file's bytes and its directory are as they were.
#[test]
fn the_rescue_key_moves_a_claimed_keyboard_and_an_ignored_one_to_the_focus_and_writes_nothing() {
    let file = HeadFile::new("rescue", RESCUE);
    let before = file.listing();
    let mut s = rescue_seat(file.rules());
    s.type_on(KBD_IGNORED, "i\r");
    s.type_on(KBD_SMALL, "c\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:c")));
    s.until(Duration::from_millis(100), |_| false);
    assert!(
        !head_shows(&s.app, "large", "got:i"),
        "ignored: it types nowhere"
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_alt_f(1), now());
    s.app
        .device_input(Path::new(KBD_IGNORED), ctrl_alt_f(1), now());
    let shown = s
        .app
        .head("large")
        .and_then(Head::notice)
        .unwrap_or_default();
    assert!(
        shown.contains(KBD_SMALL) && shown.contains(KBD_IGNORED),
        "a notice on the focus's head: {shown:?}"
    );
    s.type_on(KBD_SMALL, "s\r");
    s.type_on(KBD_IGNORED, "j\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:s")
            && head_shows(a, "large", "got:j")),
        "both follow the focus"
    );
    assert!(!head_shows(&s.app, "small", "got:s"));
    assert_eq!(file.listing(), before, "nothing written");
}

/// The paper of vtium's notice, as a display's bytes hold it: blue,
/// green, red.
const PAPER: [u8; 3] = [0xe2, 0xe2, 0xfe];

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

    /// Whether a pixel of the frame is `bgr`.
    fn has(&self, bgr: [u8; 3]) -> bool {
        self.pixels.as_chunks::<4>().0.iter().any(|p| p[..3] == bgr)
    }
}

/// The notice of a head whose session ended for good under `oneshot`.
const ENDED: &str =
    "Session ended. It will not start again. Press Ctrl+Super+↑ to switch to another screen.";

/// The same notice while another console of its head still runs.
const ENDED_CONSOLE: &str = "Session ended. It will not start again. Press Ctrl+Super+→ to show another console on this screen.";

/// The start of the anti-lockout guard's notice.
const LOCKED_OUT: &str = "because no keyboard reached a screen that is on";

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

/// The text of head `name`'s notice.
fn notice(app: &App, name: &str) -> Option<String> {
    app.head(name).and_then(Head::notice).map(str::to_owned)
}

/// The notices of D7's outcomes: each console line as the catalog words
/// it, then what to do next about vtium's head file.
const SKIPPED: &str = "vtium.conf not used: running the built-in configuration.\nTo see why, run vtium check-config /etc/vtium/vtium.conf.";
const PARTLY: &str = "vtium.conf has errors: running the parts without errors.\nTo see why, run vtium check-config /etc/vtium/vtium.conf.";
const CLOSED_OFF: &str = "vtium.conf has errors and counts as closed: all input is off.\nThis console takes no input. For the full report, run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media.";
const NO_DISPLAY: &str = "No head has a connected display.\nConnect a display that a rule in /etc/vtium/vtium.conf sends to a head.";

/// The panic screen of a broken closed file whose one error, on line 4,
/// is the key `focs`, read as `heads.conf` (D7): the console line, that
/// no head is left, the error with its file and line, and the next step,
/// each naming the file the start read.
const PANIC: &str = concat!(
    "vtium.conf has errors and counts as closed: all input is off.\n",
    "No head can run because of the errors in heads.conf, listed below.\n",
    "heads.conf:4: This section has no key named \"focs\". Check the spelling, or remove the line.\n",
    "This console takes no input. For the full report, run vtium check-config heads.conf over ssh or from other boot media.",
);

/// A head file's head and its rule for every display, which [`d7_row`]
/// boots on two displays.
const DESK: &str = "[head desk]\n[rules]\ndisplay * -> desk\ninput path:kbd-desk -> desk\n";

/// One row of D7's outcomes: the file, how its path is made, the heads
/// that run, how many displays the first lights, and the console line, on
/// the heads or on the notice screen.
struct Row {
    test: &'static str,
    text: String,
    kind: Kind,
    heads: &'static [&'static str],
    lit: usize,
    line: Option<&'static str>,
}

/// What stands at the head file's path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Missing,
    File,
    /// A file its group may write, which fails the ownership check.
    Writable,
    /// A directory, which is not readable as a file.
    Directory,
}

/// Boots `row`'s head file on two displays, the large one the boot
/// display, and checks what runs, the console line it shows, and that the
/// file and its directory are as they were.
fn d7_row(row: &Row) {
    use std::os::unix::fs::PermissionsExt;
    let test = row.test;
    let file = HeadFile::new(&format!("d7-{test}"), &row.text);
    match row.kind {
        Kind::Missing => std::fs::remove_file(&file.path).expect("no file"),
        Kind::File => {}
        Kind::Writable => {
            std::fs::set_permissions(&file.path, std::fs::Permissions::from_mode(0o664))
                .expect("0664")
        }
        Kind::Directory => {
            std::fs::remove_file(&file.path).expect("no file");
            std::fs::create_dir(&file.path).expect("a directory");
        }
    }
    let before = file.listing();
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_rules(&[], Some(file.rules()), displays, script, no_fonts());
    let names: Vec<&str> = s.app.heads().iter().map(Head::name).collect();
    assert_eq!(names, row.heads, "{test}: what runs");
    match row.heads.first() {
        Some(&first) => {
            assert!(
                s.until(BOUND, |a| head_shows(a, first, "ready")),
                "{test}: its session starts"
            );
            assert_eq!(notice(&s.app, first).as_deref(), row.line, "{test}");
            let lit = s.app.head(first).map(|h| h.cells().len());
            assert_eq!(lit, Some(row.lit), "{test}: the displays it lights");
            let lit = s.app.notice_screen().is_some_and(|n| !n.cells().is_empty());
            assert!(!lit, "{test}: a head holds the display");
        }
        None => {
            assert!(
                s.until(BOUND, |a| a
                    .notice_screen()
                    .is_some_and(|n| n.cells().len() == row.lit)),
                "{test}: the notice screen lit"
            );
            let screen = s
                .app
                .notice_screen()
                .expect("a closed file's notice screen");
            if row.line == Some(CLOSED_OFF) {
                // the panic screen: the console line, that no head can run,
                // the file's errors, each after the file and its line, and
                // the next step, every line naming the file the start read
                let path = file.path.display();
                let at = |line: &str, error: &str| format!("{path}{line}: {error}");
                let error = match test {
                    "broken-closed" => at(
                        ":4",
                        "This section has no key named \"focs\". Check the spelling, or remove the line.",
                    ),
                    // the policy line vtium cannot read, at its line
                    "unreadable-policy" | "unreadable-policy-no-head" => at(
                        ":2",
                        "`policy` cannot be \"maybe\", so vtium treats the file as broken with the closed policy. Change it to one of `open`, `closed`.",
                    ),
                    _ => at(
                        "",
                        "`version = 1` must be at the top of vtium.conf, above the first section. Add that line at the top.",
                    ),
                };
                let want = [
                    "vtium.conf has errors and counts as closed: all input is off.".to_owned(),
                    format!("No head can run because of the errors in {path}, listed below."),
                    error,
                    format!(
                        "This console takes no input. For the full report, run vtium check-config {path} over ssh or from other boot media."
                    ),
                ];
                let errors: Vec<&str> = screen.notice().map_or(Vec::new(), |n| n.lines().collect());
                assert_eq!(errors, want, "{test}: the panic screen");
            } else {
                assert_eq!(screen.notice(), row.line, "{test}");
            }
            assert_eq!(screen.cells().len(), row.lit, "{test}: lit for it alone");
        }
    }
    if row.line.is_some() {
        assert!(
            s.until_memory("/dev/mem1", BOUND, |m| m.has(PAPER)),
            "{test}: drawn"
        );
    }
    // a policy line vtium cannot read closes the file too, so no keyboard
    // delivers anything
    if row.text.contains("policy = closed") || row.line == Some(CLOSED_OFF) {
        d7_closed_input(&mut s, row);
    }
    assert_eq!(file.listing(), before, "{test}: the file as it was");
}

/// A closed row's input (ADR-1401 D3, D5, D9): a keyboard a surviving rule
/// claims types into its head, which keeps its isolation, so the VT key
/// pressed on it switches no VT; a keyboard no rule claims types nowhere
/// and switches no VT; and a file that runs no head leaves every display
/// powered off but the one lit for its notice.
fn d7_closed_input(s: &mut Seat, row: &Row) {
    let test = row.test;
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-desk"));
    s.app.add_input(Path::new(KBD_B), keyboard("kbd-other"));
    s.type_on(KBD_A, "c\r");
    s.type_on(KBD_B, "u\r");
    match row.heads.first() {
        Some(&first) => assert!(
            s.until(BOUND, |a| head_shows(a, first, "got:c")),
            "{test}: the claimed keyboard types into {first}"
        ),
        None => {
            s.until(Duration::from_millis(200), |_| false);
        }
    }
    let typed = |a: &App, text: &str| a.heads().iter().any(|h| head_shows(a, h.name(), text));
    assert!(
        !typed(&s.app, "got:u"),
        "{test}: the unclaimed keyboard types nowhere"
    );
    if row.heads.is_empty() {
        assert!(!typed(&s.app, "got:c"), "{test}: no head to type into");
    }
    for node in [KBD_A, KBD_B] {
        s.app.device_input(Path::new(node), vt_key(), now());
        assert!(
            s.app.vt.is_active(),
            "{test}: {node}'s VT key switches no VT"
        );
    }
    if row.heads.is_empty() {
        let off = [("/dev/mem0", true), ("/dev/mem1", row.line.is_none())];
        for (node, dark) in off {
            assert_eq!(
                s.memory(node).dpms == Some(Dpms::Off),
                dark,
                "{test}: {node}'s power"
            );
        }
    }
}

/// D7 and D11: one boot for each row of D7's outcomes, each checking what
/// runs, the console line it shows, and that the file's bytes, by their
/// hash, and its directory are as they were.
// covers: platform.heads.policy
#[test]
fn each_row_of_d7s_outcomes_runs_what_it_says_and_shows_its_line() {
    let rows = [
        Row { test: "missing", text: String::new(), kind: Kind::Missing, heads: &["console"], lit: 2, line: None },
        Row {
            test: "untrusted",
            text: format!("version = 1\n{DESK}"),
            kind: Kind::Writable,
            heads: &["console"], lit: 2,
            line: Some(SKIPPED),
        },
        Row { test: "not-a-file", text: String::new(), kind: Kind::Directory, heads: &["console"], lit: 2, line: Some(SKIPPED) },
        Row { test: "valid", text: format!("version = 1\n{DESK}"), kind: Kind::File, heads: &["desk"], lit: 2, line: None },
        Row { test: "empty-open", text: "version = 1\n".into(), kind: Kind::File, heads: &["console"], lit: 2, line: None },
        Row {
            test: "headless-open",
            text: "version = 1\n[rules]\ninput * -> ignore\n".into(),
            kind: Kind::File,
            heads: &["console"], lit: 1,
            line: None,
        },
        Row {
            test: "headless-closed",
            text: "policy = closed\nversion = 1\n[rules]\ndisplay * -> off\n".into(),
            kind: Kind::File,
            heads: &[], lit: 0,
            line: None,
        },
        Row {
            test: "partly-open",
            text: format!("version = 1\n{DESK}display * -> desc\n"),
            kind: Kind::File,
            heads: &["desk"], lit: 2,
            line: Some(PARTLY),
        },
        Row {
            test: "unreadable-policy",
            text: format!("version = 1\npolicy = maybe\n{DESK}"),
            kind: Kind::File,
            heads: &[], lit: 1,
            line: Some(CLOSED_OFF),
        },
        Row {
            test: "partly-closed",
            text: format!("version = 1\npolicy = closed\n{DESK}[head bad]\ncolour = red\n"),
            kind: Kind::File,
            heads: &["desk"], lit: 2,
            line: Some(PARTLY),
        },
        Row {
            test: "broken-open",
            text: "version = 1\n[head desk]\ncolour = red\n[rules]\ndisplay * -> desk\n".into(),
            kind: Kind::File,
            heads: &["console"], lit: 2,
            line: Some(SKIPPED),
        },
        Row { test: "no-version-open", text: DESK.into(), kind: Kind::File, heads: &["console"], lit: 2, line: Some(SKIPPED) },
        Row {
            test: "unreadable-policy-no-head",
            text: "version = 1\npolicy = maybe\n".into(),
            kind: Kind::File,
            heads: &[], lit: 1,
            line: Some(CLOSED_OFF),
        },
        Row {
            test: "broken-closed",
            text: "version = 1\npolicy = closed\n[head kiosk]\nfocs = 1\n[rules]\ndisplay * -> kiosk\n".into(),
            kind: Kind::File,
            heads: &[], lit: 1,
            line: Some(CLOSED_OFF),
        },
        Row {
            test: "no-version-closed",
            text: format!("policy = closed\n{DESK}"),
            kind: Kind::File,
            heads: &[], lit: 1,
            line: Some(CLOSED_OFF),
        },
    ];
    for row in &rows {
        d7_row(row);
    }
}

/// D7: a partly broken open file runs what survives and then the shipped
/// rules' tail, under which a keyboard no surviving rule places roams, and
/// every surviving head acts as one that is not isolated, so it can be the
/// active head the roaming keyboard types into.
// covers: platform.heads.policy
#[test]
fn a_partly_broken_open_file_runs_its_survivors_then_the_shipped_tail_with_no_head_isolated() {
    let text = "version = 1\n[head desk]\nisolated = yes\n[head bad]\ncolour = red\n[rules]\ndisplay * -> desk\ninput path:kbd-bad -> bad\n";
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = Seat::with_rules(&[], Some(start(text)), displays, ECHO, no_fonts());
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-bad"));
    assert!(s.until(BOUND, |a| head_shows(a, "desk", "ready")));
    assert_eq!(s.app.focus_head(), Some("desk"), "not isolated");
    s.type_on(KBD_A, "t\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "desk", "got:t")),
        "the dropped rule's keyboard walks the tail and roams"
    );
}

/// D7 and D9: a partly broken closed file runs what survives as written:
/// a keyboard on a dropped rule's port is placed by no rule, so it is
/// ignored, and a surviving rule's keyboard types into its head.
// covers: platform.heads.policy
#[test]
fn a_partly_broken_closed_file_delivers_nothing_from_a_dropped_rules_keyboard() {
    let text = "version = 1\npolicy = closed\n[head desk]\n[head bad]\ncolour = red\n[rules]\ndisplay * -> desk\ninput path:kbd-bad -> bad\ninput path:kbd-desk -> desk\n";
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = Seat::with_rules(&[], Some(start(text)), displays, ECHO, no_fonts());
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-bad"));
    s.app.add_input(Path::new(KBD_B), keyboard("kbd-desk"));
    assert!(s.until(BOUND, |a| head_shows(a, "desk", "ready")));
    assert_eq!(notice(&s.app, "desk").as_deref(), Some(PARTLY));
    s.type_on(KBD_A, "x\r");
    s.type_on(KBD_B, "d\r");
    assert!(s.until(BOUND, |a| head_shows(a, "desk", "got:d")));
    s.until(Duration::from_millis(100), |_| false);
    assert!(
        !head_shows(&s.app, "desk", "got:x"),
        "the dropped rule's keyboard types nowhere"
    );
    s.app.device_input(Path::new(KBD_A), ctrl_alt_f(1), now());
    s.type_on(KBD_A, "y\r");
    // A line typed after it, on the keyboard that types into desk: had y
    // reached desk, its echo would come first.
    s.type_on(KBD_B, "e\r");
    assert!(s.until(BOUND, |a| head_shows(a, "desk", "got:e")));
    assert!(
        !head_shows(&s.app, "desk", "got:y"),
        "and the rescue key is off"
    );
}

/// D7 and D9: a broken closed file runs nothing. The boot display shows
/// the panic screen alone, on the notice screen, with no session behind
/// it, still up past 30 s, and no keyboard delivers anything.
// covers: platform.heads.policy
#[test]
fn a_broken_closed_file_runs_nothing_and_shows_the_panic_screen_on_the_boot_display_for_good() {
    let text = "version = 1\npolicy = closed\n[head kiosk]\nfocs = 1\n[rules]\ndisplay * -> kiosk\ninput * -> kiosk\n";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    logged();
    let mut s = Seat::with_rules(&[], Some(start(text)), displays, ECHO, no_fonts());
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    assert!(
        s.until_memory("/dev/mem1", BOUND, |m| m.has(PAPER)),
        "drawn on the boot display"
    );
    // the start logged each error with its line, so the seat logs the
    // console line alone, never the report
    let lines = logged();
    let line = "vtium.conf has errors and counts as closed: all input is off.";
    assert!(lines.iter().all(|l| !l.contains('\n')), "{lines:#?}");
    assert!(lines.iter().any(|l| l == line), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("the display guard lights display") && l.ends_with(line)),
        "{lines:#?}"
    );
    let screen = |a: &App| {
        a.notice_screen()
            .map(|n| (n.notice().map(str::to_owned), n.cells().len()))
    };
    assert_eq!(screen(&s.app), Some((Some(PANIC.to_owned()), 1)));
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::Off),
        "the other display dark"
    );
    assert!(s.app.heads().is_empty());
    s.type_on(KBD_A, "x\r");
    s.type_str("y\r");
    s.app.turn(now() + Duration::from_secs(31));
    assert_eq!(
        screen(&s.app),
        Some((Some(PANIC.to_owned()), 1)),
        "no timeout, no key ends it"
    );
    assert!(
        s.app
            .notice_screen()
            .is_some_and(|n| n.pty().is_none() && n.screen_text().iter().all(|l| l.is_empty())),
        "no session"
    );
}

/// D9: under `policy = closed` a file whose heads have no display lights
/// the boot display for `No head has a connected display.` alone, which
/// stays until a head lights, when the head takes its display.
// covers: platform.heads.guards
#[test]
fn under_closed_heads_with_no_display_light_the_boot_display_for_the_notice_alone() {
    let text =
        "version = 1\npolicy = closed\n[head desk]\n[rules]\ndisplay card:mem-large -> desk\n";
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = Seat::with_rules(&[], Some(start(text)), displays, ECHO, no_fonts());
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)));
    assert_eq!(
        s.app.notice_screen().and_then(Head::notice),
        Some(NO_DISPLAY)
    );
    s.app.turn(now() + Duration::from_secs(31));
    assert_eq!(
        s.app.notice_screen().and_then(Head::notice),
        Some(NO_DISPLAY),
        "it stays"
    );
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large")),
    );
    assert!(
        s.until(BOUND, |a| head_shows(a, "desk", "ready")),
        "a head lit"
    );
    assert!(
        s.app.notice_screen().is_some_and(|n| n.cells().is_empty()),
        "the notice screen let go"
    );
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::Off));
}

/// D8: under `policy = closed` vtium never gives up for lack of a
/// display: with none, it holds its VT past `display-wait` and waits,
/// where an open file exits with status 1.
// covers: platform.heads.policy
#[test]
fn under_closed_with_no_display_vtium_waits_past_display_wait() {
    let closed = start(&format!("version = 1\npolicy = closed\n{DESK}"));
    let mut s = Seat::with_rules(
        &["--display-wait=0"],
        Some(closed),
        [],
        "read x",
        no_fonts(),
    );
    s.until(Duration::from_millis(300), |_| false);
    assert_eq!(s.app.finished(), None, "it holds its VT");
    let open = start(&format!("version = 1\n{DESK}"));
    let mut s = Seat::with_rules(&["--display-wait=0"], Some(open), [], "read x", no_fonts());
    assert!(s.until(BOUND, |a| a.finished().is_some()));
    assert_eq!(s.app.finished().map(vtium::app::Exit::status), Some(1));
}

/// An invalid head file's notice, on one display.
fn noticed() -> (Seat, HeadFile) {
    let file = HeadFile::new(
        "noticed",
        "version = 1\n[head desk]\ncolour = red\n[rules]\ndisplay * -> desk\n",
    );
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_rules(
        &[],
        Some(file.rules()),
        displays,
        "echo ready; read x",
        no_fonts(),
    );
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)));
    (s, file)
}

/// D7 and the operator's answer to probe 5: the notice ends on any key
/// that reaches its head, and the display draws whole without it.
#[test]
fn the_notice_ends_on_a_key_reaching_its_head() {
    let (mut s, _file) = noticed();
    s.type_str("k");
    assert_eq!(notice(&s.app, "console"), None);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| !m.has(PAPER)),
        "gone from the display"
    );
}

/// D7 and the operator's answer to probe 5: with no key the notice ends
/// 30 s after it is first shown, the test's clock driving the time.
#[test]
fn the_notice_ends_after_30_s() {
    let (mut s, _file) = noticed();
    let shown = now();
    s.app.turn(shown + Duration::from_secs(29));
    assert!(notice(&s.app, "console").is_some(), "shown at 29 s");
    s.app.turn(shown + Duration::from_secs(31));
    assert_eq!(notice(&s.app, "console"), None, "ended at 30 s");
}

/// The operator's answers (2026-10-30): under `oneshot` a head whose
/// program ended shows that the session ended on its display, drawn by vtium,
/// while the other head runs on.
#[test]
fn under_oneshot_a_head_whose_program_ended_shows_session_ended() {
    let mut s = claimed_with(&["--oneshot"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)),
        "drawn"
    );
    assert!(!s.memory("/dev/mem1").has(PAPER), "large runs on");
}

/// ADR-1005 D10: the ended session's notice names the key vtium.conf binds
/// to the focus chord, not its default.
#[test]
fn the_ended_sessions_notice_names_the_key_vtium_conf_binds() {
    let mut s = claimed_with(&["--oneshot", "--key-focus-next=<Alt>n"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").as_deref()
        == Some(
            "Session ended. It will not start again. Press Alt+n to switch to another screen."
        )));
}

/// The on-screen display's dots of the heads, in the head file's order.
fn marks(app: &App) -> Vec<Option<vtium_render::Mark>> {
    app.heads().iter().map(Head::mark).collect()
}

/// The on-screen display's lines of the heads, in the head file's order.
fn statuses(app: &App) -> Vec<Option<String>> {
    app.heads()
        .iter()
        .map(|h| h.status().map(str::to_owned))
        .collect()
}

/// The BGR of the box the on-screen display's line stands in alone, and
/// of the cell behind its dot.
const STATUS_PAPER: [u8; 3] = [0x33, 0x29, 0x1f];

/// The BGR of the on-screen display's text, and of its dot.
const STATUS_INK: [u8; 3] = [0xeb, 0xe7, 0xe5];

/// D5 and vtium/vtium#169: under `osd = minimal`, the default, no head
/// shows the on-screen display in normal use; the roaming chord shows
/// every head's dot, filled on the head that became active and hollow on
/// the other, over its top-right cell in the on-screen display's colors,
/// and it goes `osd-timeout`, 3 s,
/// after its first frame. The test turns the loop with its clock moved
/// on, and never waits on the wall clock.
// covers: platform.heads.focus
#[test]
fn the_osd_dot_shows_when_the_active_head_moves_and_goes_after_its_timeout() {
    use vtium_render::Mark;
    let mut s = claimed();
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(marks(&s.app), [None, None], "none in normal use");
    assert!(!s.memory("/dev/mem0").has(STATUS_PAPER));
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    let moved = now();
    s.app.turn(moved);
    assert_eq!(marks(&s.app), [Some(Mark::Focus), Some(Mark::NoFocus)]);
    assert_eq!(statuses(&s.app), [None, None], "no line under minimal");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.has(STATUS_PAPER)),
        "drawn on small"
    );
    assert!(
        s.until_memory("/dev/mem1", BOUND, |m| m.has(STATUS_PAPER)),
        "and on large"
    );
    for mem in ["/dev/mem0", "/dev/mem1"] {
        let m = s.memory(mem);
        assert!(
            m.has(STATUS_INK) && !m.has(PAPER),
            "{mem}: the dot in the on-screen display's colors, not the notice's"
        );
    }
    s.app.turn(moved + Duration::from_millis(2900));
    assert_eq!(
        marks(&s.app),
        [Some(Mark::Focus), Some(Mark::NoFocus)],
        "shown at 2.9 s"
    );
    s.app.turn(moved + Duration::from_secs(10));
    assert_eq!(marks(&s.app), [None, None], "gone after 3 s");
}

/// D5 and vtium/vtium#169: under `osd = full` the on-screen display
/// shows on one head when that head shows another console and when its
/// display returns after it had none; each time it goes after
/// `osd-timeout`, and the other head's never shows. A console's program
/// starting again changes neither the console shown nor the count, so it
/// shows none (D1). Under `osd = minimal` the dot says nothing of
/// consoles, and a console chord shows none.
// covers: platform.heads.focus
#[test]
fn the_osd_shows_on_a_head_whose_console_changes_or_whose_display_returns() {
    let mut s = claimed_on(&["--osd=full"], &claimed_consoles(2));
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(statuses(&s.app), [None, None]);
    // Each change at `t`, then the clock past the timeout.
    let shows_then_goes = |s: &mut Seat, t: Instant, line: &str, what: &str| {
        s.app.turn(t);
        assert_eq!(
            statuses(&s.app),
            [Some(line.to_owned()), None],
            "{what}: on small alone"
        );
        // Its time runs from the first frame that draws it.
        assert!(
            s.until_memory("/dev/mem0", BOUND, |m| m.has(STATUS_PAPER)),
            "{what}: drawn"
        );
        s.app.turn(t + Duration::from_secs(10));
        assert_eq!(statuses(&s.app), [None, None], "{what}: gone");
    };
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    let line = "○ small · console 2 of 2";
    shows_then_goes(&mut s, now(), line, "the next console");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(LEFT), now());
    let line = "○ small · console 1 of 2";
    shows_then_goes(&mut s, now(), line, "the previous console");
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:x")));
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| !head_shows(a, "small", "got:x")
        && head_shows(a, "small", "ready")));
    s.app.turn(now());
    assert_eq!(statuses(&s.app), [None, None], "a program started again");
    let t = now();
    s.unplug("/dev/mem0", t);
    s.app.turn(t + Duration::from_secs(10));
    assert_eq!(statuses(&s.app), [None, None], "nothing at the loss");
    let back = t + Duration::from_secs(11);
    s.plug("/dev/mem0", back);
    shows_then_goes(&mut s, back, line, "the display returned");
    let mut s = claimed_on(&[], &claimed_consoles(2));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    s.app.turn(now());
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert_eq!(marks(&s.app), [None, None], "no dot for a console");
}

/// D5 and vtium/vtium#169: under `osd = full` the on-screen display is a
/// line across the top of each head, the dot, the head's name and the
/// shown console's place among the head's consoles, in a dark box; no dot
/// in the top-right cell.
// covers: platform.heads.focus
#[test]
fn osd_full_draws_a_line_naming_the_head_and_its_console() {
    let mut s = claimed_on(&["--osd=full"], &claimed_consoles(2));
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(statuses(&s.app), [None, None], "none in normal use");
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    s.app.turn(now());
    assert_eq!(
        statuses(&s.app),
        [
            Some("● small · console 1 of 2".to_owned()),
            Some("○ large · console 1 of 1".to_owned())
        ]
    );
    assert_eq!(marks(&s.app), [None, None]);
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.has(STATUS_PAPER) && !m.has(PAPER)),
        "drawn in the status box, never the notice's red"
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    s.app.turn(now());
    assert_eq!(
        statuses(&s.app)[0].as_deref(),
        Some("● small · console 2 of 2")
    );
}

/// D5 and vtium/vtium#169: `osd-timeout = never` keeps every head's
/// on-screen display up from the start, and `osd = off` shows none, the
/// roaming chord's move included; `focus-mark`, the older name, reads as
/// them.
// covers: platform.heads.focus
#[test]
fn osd_timeout_never_keeps_it_up_and_osd_off_draws_none() {
    use vtium_render::Mark;
    for args in [["--osd-timeout=never"], ["--focus-mark=always"]] {
        let mut s = claimed_with(&args);
        let both = [Some(Mark::NoFocus), Some(Mark::Focus)];
        assert!(s.until(BOUND, |a| marks(a) == both), "{args:?}");
        s.app.turn(now() + Duration::from_secs(60));
        assert_eq!(
            marks(&s.app),
            [Some(Mark::NoFocus), Some(Mark::Focus)],
            "{args:?}: never gone"
        );
    }
    for args in [["--osd=off"], ["--focus-mark=off"]] {
        let mut s = claimed_with(&args);
        s.app
            .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
        s.app.turn(now());
        assert_eq!(marks(&s.app), [None, None], "{args:?}");
        assert_eq!(statuses(&s.app), [None, None], "{args:?}");
    }
}

/// D5 and vtium/vtium#169: `osd = off` governs status alone, and a notice
/// of a problem still shows: here a head's session that ended for good.
#[test]
fn osd_off_still_shows_a_notice_of_a_problem() {
    let mut s = claimed_with(&["--osd=off", "--oneshot"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").is_some()));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)),
        "drawn"
    );
}

/// [`CLAIMED`] with two consoles in `large`.
const LARGE_OF_TWO: &str = "version = 1
[head small]
[head large]
consoles = 2
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-large -> large
input class:keyboard -> roaming
";

/// D5: a head's first start is no change the on-screen display shows,
/// though the head opens its other consoles only as its display first
/// lights.
// covers: platform.heads.focus
#[test]
fn osd_full_shows_nothing_when_a_head_first_starts() {
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = Seat::with_heads(&["--osd=full"], Some(LARGE_OF_TWO), displays, ECHO);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large")),
    );
    assert!(s.until(BOUND, |a| consoles(a, "large") == Some((2, 0))
        && head_shows(a, "large", "ready")));
    s.app.turn(now());
    assert_eq!(statuses(&s.app), [None, None]);
}

/// D5: when the active head moves, the on-screen display shows on the
/// heads with a lit display alone, and a head that loses its last display
/// drops the display it showed, so neither shows when the display comes
/// back.
// covers: platform.heads.focus
#[test]
fn a_head_without_a_display_keeps_no_osd_for_its_return() {
    use vtium_render::Mark;
    let mut s = losing_with_focus();
    let t = now();
    s.unplug("/dev/mem0", t);
    assert_eq!(s.app.focus_head(), Some("large"), "the guard moved it");
    assert_eq!(marks(&s.app), [None, Some(Mark::Focus)]);
    s.app.turn(t + Duration::from_secs(10));
    s.plug("/dev/mem0", t + Duration::from_secs(3600));
    assert_eq!(marks(&s.app), [None, None], "nothing an hour later");
    let mut s = claimed();
    let t = now();
    s.app.device_input(Path::new(KBD_FOCUS), ctrl_super(UP), t);
    s.app.turn(t);
    assert_eq!(marks(&s.app), [Some(Mark::Focus), Some(Mark::NoFocus)]);
    s.unplug("/dev/mem0", t + Duration::from_secs(1));
    s.plug("/dev/mem0", t + Duration::from_secs(2));
    assert_eq!(
        marks(&s.app),
        [None, Some(Mark::NoFocus)],
        "dropped at the loss, within its time"
    );
}

/// Three heads, each on a display of its own, and every keyboard roaming.
const THREE_HEADS: &str = "version = 1
[head a]
[head b]
[head c]
[rules]
display card:mem-a -> a
display card:mem-b -> b
display card:mem-c -> c
input class:keyboard -> roaming
";

/// D5: a head already without a display when the active head moves takes
/// no on-screen display from the move, so none shows when its display
/// comes back much later.
// covers: platform.heads.focus
#[test]
fn a_head_dark_before_a_move_shows_no_osd_on_its_return() {
    use vtium_render::Mark;
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-a").lit()),
        ("/dev/mem1", Memory::new(320, 160, "MEM-2", "mem-b")),
        ("/dev/mem2", Memory::new(320, 160, "MEM-3", "mem-c")),
    ];
    let mut s = Seat::with_heads(&[], Some(THREE_HEADS), displays, "echo ready; read x");
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| a.cells().len() == 3));
    assert_eq!(s.app.focus_head(), Some("a"));
    let t = now();
    s.unplug("/dev/mem2", t);
    s.app.turn(t + Duration::from_secs(10));
    s.app.device_input(Path::new(KBD_FOCUS), ctrl_super(UP), t);
    s.app.turn(t + Duration::from_secs(10));
    assert_eq!(s.app.focus_head(), Some("b"));
    assert_eq!(
        marks(&s.app),
        [Some(Mark::NoFocus), Some(Mark::Focus), None]
    );
    s.app.turn(t + Duration::from_secs(20));
    s.plug("/dev/mem2", t + Duration::from_secs(3600));
    assert_eq!(marks(&s.app), [None, None, None], "nothing an hour later");
}

/// D5: a change on a head with no display waits for a frame to draw it,
/// so its time never runs out while there is nothing to draw on.
// covers: platform.heads.focus
#[test]
fn osd_time_waits_for_a_display() {
    let mut s = claimed_on(&["--osd=full"], &claimed_consoles(2));
    // A frame on a screen ends the start's wait, which the turns far ahead
    // would otherwise run out.
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    let t = now() + Duration::from_secs(10);
    s.unplug("/dev/mem0", t);
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), t);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    s.app.turn(t + Duration::from_secs(60));
    let line = "○ small · console 2 of 2";
    assert_eq!(statuses(&s.app)[0].as_deref(), Some(line), "not run out");
    s.plug("/dev/mem0", t + Duration::from_secs(61));
    assert_eq!(statuses(&s.app)[0].as_deref(), Some(line), "drawn");
    s.app.turn(t + Duration::from_secs(70));
    assert_eq!(statuses(&s.app)[0], None, "gone after its time");
}

/// D5: a change while the on-screen display shows starts its time again.
// covers: platform.heads.focus
#[test]
fn a_change_while_the_osd_shows_starts_its_time_again() {
    use vtium_render::Mark;
    let mut s = claimed();
    let t = now();
    s.app.device_input(Path::new(KBD_FOCUS), ctrl_super(UP), t);
    s.app.turn(t);
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(DOWN), t);
    s.app.turn(t + Duration::from_secs(2));
    s.app.turn(t + Duration::from_secs(4));
    assert_eq!(
        marks(&s.app),
        [Some(Mark::NoFocus), Some(Mark::Focus)],
        "3 s from the second move"
    );
    s.app.turn(t + Duration::from_secs(6));
    assert_eq!(marks(&s.app), [None, None]);
}

/// D5: an idle seat wakes to take the on-screen display down when its
/// time has passed.
#[test]
fn an_idle_seat_wakes_to_take_the_osd_down() {
    let mut s = claimed_with(&["--no-blink"]);
    s.until(Duration::from_millis(100), |_| false);
    let t = now();
    s.app.device_input(Path::new(KBD_FOCUS), ctrl_super(UP), t);
    s.app.turn(t);
    let wake = s.app.next_wake(t + Duration::from_millis(2900));
    assert!(
        wake.is_some_and(|w| w <= Duration::from_millis(100)),
        "wakes by 3 s: {wake:?}"
    );
}

/// The loop's turns a test gives a press to start a frame. Under
/// `minimal` the press's frame starts in the first turn after it, so a
/// frame that has not started after this many turns never will.
const TURNS: usize = 8;

/// The turn, counted from 1, in which the display at `node` is asked for
/// a frame after `press`, within [`TURNS`] turns: the loop asks the
/// display for a buffer, which [`Memory`] counts, as it starts each frame.
/// `None` when no frame starts. The seat's start is drawn first, with no
/// frame of it still out.
fn turn_of_frame(s: &mut Seat, node: &str, press: impl FnOnce(&mut Seat)) -> Option<usize> {
    assert!(
        s.until_memory(node, BOUND, |m| m.inked(0, 1..5)),
        "the start drawn"
    );
    assert!(
        s.until(BOUND, |a| a.heads().iter().all(|h| h.frames_out() == 0)),
        "no frame of the start out"
    );
    let lent = s.memory(node).ages.len();
    press(s);
    (1..=TURNS).find(|_| {
        s.app.turn(now());
        s.memory(node).ages.len() > lent
    })
}

/// D5: under `osd = off` the active head's move starts no frame, since it
/// leaves the picture as it was; under `minimal` it starts one with the
/// dot, in the first turn, the control.
#[test]
fn under_osd_off_a_move_composes_no_frame() {
    for (osd, drawn) in [("--osd=off", None), ("--osd=minimal", Some(1))] {
        let mut s = claimed_with(&[osd, "--no-blink"]);
        let moved = turn_of_frame(&mut s, "/dev/mem1", |s| {
            s.app
                .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
        });
        assert_eq!(moved, drawn, "{osd}");
    }
}

/// D5: with `osd = off`, the open chord at a cap leaves the picture as it
/// was, so it starts no frame; under `minimal` it starts one with the
/// brief line, in the first turn, the control.
#[test]
fn under_osd_off_the_open_chord_at_a_cap_composes_no_frame() {
    for (osd, drawn) in [("--osd=off", None), ("--osd=minimal", Some(1))] {
        let mut s = claimed_with(&[osd, "--no-blink", "--head-consoles-max=1"]);
        let pressed = turn_of_frame(&mut s, "/dev/mem0", |s| {
            press_chord(s, KBD_SMALL, RETURN);
        });
        assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "{osd}");
        assert_eq!(pressed, drawn, "{osd}");
    }
}

/// Each way vtium.conf turns the on-screen display off: `osd = off`, the
/// older `focus-mark = off`, and `off` beside an `osd-timeout` or a
/// `focus-mark` that would otherwise keep it up from the start.
const OSD_OFF: [&[&str]; 5] = [
    &["--osd=off"],
    &["--focus-mark=off"],
    &["--osd=off", "--osd-timeout=never"],
    &["--focus-mark=always", "--osd=off"],
    &["--focus-mark=off", "--osd-timeout=never"],
];

/// D5 and D7, the maintainer's decision of 2026-11-05: a console the open
/// chord opens whose login cannot start is a notice of a problem, which
/// shows whatever `osd` says: under every way of turning the OSD off and
/// under `minimal` and `full`, the head shows the notice in the notices'
/// colors and no brief line, and a key reaching the head ends it, as it
/// ends the guards' notices.
// covers: platform.heads
#[test]
fn a_console_that_cannot_start_shows_a_notice_under_every_osd() {
    let dir = Scratch::new("heads-osd-unstartable");
    let login = dir.join("login");
    let heads = CLAIMED.replace(
        "[head small]\n",
        &format!("[head small]\nlogin = {}\n", login.display()),
    );
    let modes = OSD_OFF
        .iter()
        .copied()
        .chain([&["--osd=minimal"][..], &["--osd=full"][..]]);
    for args in modes {
        script::executable(
            &login,
            "#!/bin/sh\necho ready; while read a; do echo \"got:$a\"; done\n",
        );
        let mut s = claimed_on(args, &heads);
        assert_eq!(notice(&s.app, "small"), None, "{args:?}");
        std::fs::remove_file(&login).expect("the login removed");
        press_chord(&mut s, KBD_SMALL, RETURN);
        assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "{args:?}");
        assert_eq!(
            notice(&s.app, "small").as_deref(),
            Some(vtium::notice::NO_CONSOLE_OPENED),
            "{args:?}: the notice"
        );
        assert_eq!(
            s.app.head("small").and_then(Head::status),
            None,
            "{args:?}: no brief line"
        );
        assert!(
            s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)),
            "{args:?}: drawn in the notices' colors"
        );
        s.type_on(KBD_SMALL, "on\r");
        assert!(
            s.until(BOUND, |a| head_shows(a, "small", "got:on")),
            "{args:?}: the first console's session runs on"
        );
        assert_eq!(notice(&s.app, "small"), None, "{args:?}: a key ended it");
    }
}

/// Two heads, each keyboard but `kbd-large` claimed by `small`, which
/// [`stranded`] gives no display.
const STRANDED: &str = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-large -> large
input class:keyboard -> small
";

const KBD_A: &str = "/dev/input/event4";
const KBD_B: &str = "/dev/input/event5";

/// [`STRANDED`], or `rules` in its place, booted with only the
/// large display, two keyboards claimed by `small`, each head's session
/// echoing each line it reads.
fn stranded(rules: Start) -> Seat {
    let displays = [("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large"))];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_rules(&[], Some(rules), displays, script, no_fonts());
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    s.app.add_input(Path::new(KBD_B), keyboard("kbd-b"));
    assert!(s.until(BOUND, |a| head_shows(a, "large", "ready")));
    s
}

/// D9 and D11's anti-lockout row: every keyboard claimed by a head with no
/// display types into the focus, with a notice, until a keyboard reaches a
/// lit head: here the claiming head's display arriving, after which each
/// keyboard types into its rule's head again.
#[test]
fn every_keyboard_claimed_by_a_head_with_no_display_types_into_the_focus() {
    let mut s = stranded(start(STRANDED));
    let shown = notice(&s.app, "large").unwrap_or_default();
    assert!(shown.contains(LOCKED_OUT), "a notice: {shown:?}");
    s.type_on(KBD_A, "a\r");
    s.type_on(KBD_B, "b\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:a")
            && head_shows(a, "large", "got:b")),
        "both type into the focus"
    );
    s.app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Memory::new(320, 160, "MEM-1", "mem-small")),
    );
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_A, "c\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:c")),
        "the guard let go"
    );
    assert!(!head_shows(&s.app, "large", "got:c"));
}

/// D8 and D9: a display its device cannot light, though still plugged in,
/// does not count as lit for the guards. Here `small`'s display is lit, and
/// the keyboards it claims type into it; then the device leaves it unlit.
/// No keyboard reaches a lit head, so the anti-lockout guard sends each to
/// the focus on `large`, with a notice, as when the display goes. Once the
/// device lights the display again, the keyboards type into `small` again.
#[test]
fn a_display_left_unlit_does_not_count_as_lit_for_the_guards() {
    let mut s = stranded(start(STRANDED));
    s.app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Memory::new(320, 160, "MEM-1", "mem-small")),
    );
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_A, "a\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:a")));
    s.unlight("/dev/mem0", now());
    let shown = notice(&s.app, "large").unwrap_or_default();
    assert!(shown.contains(LOCKED_OUT), "a notice: {shown:?}");
    s.type_on(KBD_A, "b\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:b")),
        "the keyboard types into the focus"
    );
    assert!(!head_shows(&s.app, "small", "got:b"));
    s.relight("/dev/mem0", now());
    s.type_on(KBD_A, "c\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:c")),
        "lit again, the guard lets go"
    );
    assert!(!head_shows(&s.app, "large", "got:c"));
}

/// D9: when the anti-lockout guard fires it moves the focus to a lit head,
/// the boot display's when lit, and the on-screen display shows on the
/// lit head; the head that lost its display shows none.
#[test]
fn the_anti_lockout_guard_moves_the_focus_to_a_lit_head() {
    let mut s = losing_with_focus();
    assert_eq!(s.app.focus_head(), Some("small"));
    s.unplug("/dev/mem0", now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"), "moved to the lit head");
    assert_eq!(marks(&s.app), [None, Some(vtium_render::Mark::Focus)]);
    s.type_on(KBD_FOCUS, "f\r");
    assert!(s.until(BOUND, |a| head_shows(a, "large", "got:f")));
}

/// Two heads, the small display the boot display, so the focus starts on
/// `small`, and one keyboard following the focus.
fn losing_with_focus() -> Seat {
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-small").lit(),
        ),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, script);
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    s
}

/// D9: under `policy = closed` the anti-lockout guard is off, so a
/// ruleset that strands every keyboard is left as it is.
// covers: platform.heads.guards
#[test]
fn under_closed_a_ruleset_stranding_every_keyboard_is_left_as_it_is() {
    let closed = STRANDED.replace("version = 1\n", "version = 1\npolicy = closed\n");
    let mut s = stranded(start(&closed));
    s.type_on(KBD_A, "a\r");
    s.until(Duration::from_millis(200), |_| false);
    assert!(!head_shows(&s.app, "large", "got:a"));
    assert_eq!(notice(&s.app, "large"), None);
}

/// D7: an open file with `version = 1` alone is valid and declares
/// nothing, so it runs the shipped default, as no file does: every display
/// lights in its one head, mirroring one grid, and nothing is shown.
// covers: platform.heads.policy
#[test]
fn a_file_with_version_1_alone_runs_the_shipped_default() {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some("version = 1\n"), displays, "echo ready; read x");
    assert!(
        s.until(BOUND, |a| a.cells().len() == 2),
        "every display lit"
    );
    assert_eq!(
        s.app.heads().iter().map(Head::name).collect::<Vec<_>>(),
        ["console"]
    );
    assert!(s.until(BOUND, |a| shows(a, "ready")), "its session runs");
    assert_eq!(notice(&s.app, "console"), None);
}

/// D7 and D9: under `policy = closed` a file that declares nothing is
/// loaded but off: the display guard lights no display, not even for a
/// notice, and nothing is shown.
// covers: platform.heads.policy
#[test]
fn under_closed_a_file_declaring_nothing_leaves_every_display_dark() {
    let displays = [(
        "/dev/mem1",
        Memory::new(640, 320, "MEM-2", "mem-large").lit(),
    )];
    let rules = start("version = 1\npolicy = closed\n");
    let mut s = Seat::with_rules(&[], Some(rules), displays, "echo ready; read x", no_fonts());
    assert!(
        s.until_memory("/dev/mem1", BOUND, |m| m.dpms == Some(Dpms::Off)),
        "the display dark"
    );
    assert_eq!(s.app.cells(), Vec::<(u32, u32)>::new());
    assert!(s.app.heads().is_empty(), "no head, not even the default");
    assert!(
        s.app
            .notice_screen()
            .is_some_and(|n| n.cells().is_empty() && n.notice().is_none()),
        "no notice either"
    );
    assert_eq!(s.memory("/dev/mem1").dpms, Some(Dpms::Off));
}

/// D9: under `policy = closed` the rescue key moves nothing, and under
/// open a head's `isolated = yes` keeps the keyboards it claims while
/// another head's move.
// covers: platform.heads.rescue-key
#[test]
fn under_closed_the_rescue_key_moves_nothing_and_an_isolated_head_keeps_its_keyboard() {
    // under closed a head isolated by default is never the active head
    let open_heads = RESCUE
        .replace("version = 1\n", "version = 1\npolicy = closed\n")
        .replace("]\n[", "]\nisolated = no\n[");
    let mut s = rescue_seat(start(&open_heads));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_alt_f(1), now());
    s.type_on(KBD_SMALL, "s\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:s")),
        "still small's"
    );
    // small keeps its keyboard; spare, a head with no display, lets its
    // keyboard go to the focus's head, large
    let kiosk = RESCUE
        .replace("[head small]", "[head small]\nisolated = yes")
        .replace("[head large]", "[head large]\n[head spare]")
        .replace(
            "input path:kbd-ignored -> ignore",
            "input path:kbd-ignored -> spare",
        );
    let file = HeadFile::new("rescue-head-off", &kiosk);
    let mut s = rescue_seat(file.rules());
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_alt_f(1), now());
    s.app
        .device_input(Path::new(KBD_IGNORED), ctrl_alt_f(1), now());
    s.type_on(KBD_SMALL, "k\r");
    s.type_on(KBD_IGNORED, "m\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:k")),
        "small's kept: isolated = yes"
    );
    assert!(
        s.until(BOUND, |a| head_shows(a, "large", "got:m")),
        "spare's moved to the focus"
    );
}

/// D9: a keyboard the rescue key moved reaches the focus's lit head, so the
/// anti-lockout guard lets go of the others, which go back to their rule:
/// here `small`, which has no display and receives nothing.
#[test]
fn a_keyboard_the_rescue_key_moved_lets_the_anti_lockout_guard_go() {
    let mut s = stranded(start(STRANDED));
    s.app.device_input(Path::new(KBD_A), ctrl_alt_f(1), now());
    s.app.turn(now());
    s.type_on(KBD_A, "a\r");
    s.type_on(KBD_B, "b\r");
    // A line on large's keyboard after b: had b reached large, its echo
    // would come before this one's.
    s.type_on(KBD_A, "c\r");
    assert!(s.until(BOUND, |a| head_shows(a, "large", "got:a")
        && head_shows(a, "large", "got:c")));
    assert!(!head_shows(&s.app, "large", "got:b"), "b is small's again");
}

/// D9: the display guard acts after every display added or removed. The
/// rules light the large display alone; once it goes the guard lights the
/// boot display, the small, in the first head, and once it is back the
/// small goes dark again.
#[test]
fn the_display_guard_lights_the_boot_display_while_the_rules_light_none() {
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-small").lit(),
        ),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ];
    let text = "version = 1\n[head desk]\n[rules]\ndisplay card:mem-large -> desk\n";
    let mut s = Seat::with_heads(&[], Some(text), displays, "echo ready; read x");
    assert!(s.until(BOUND, |a| a.grid() == (80, 20)), "the large alone");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::Off));
    s.unplug("/dev/mem1", now());
    assert!(s.until(BOUND, |a| a.grid() == (40, 10)), "the small lit");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::On));
    s.plug("/dev/mem1", now());
    assert!(s.until(BOUND, |a| a.grid() == (80, 20)), "the large again");
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::Off),
        "the small dark again"
    );
    assert_eq!(s.app.cells().len(), 1);
}

/// D11: a boot with no head file present writes nothing where the head
/// file lives, its listing the same before and after.
#[test]
fn a_boot_with_no_head_file_writes_nothing_where_it_lives() {
    let file = HeadFile::new("absent", "");
    std::fs::remove_file(&file.path).expect("no head file");
    let before = file.listing();
    let start = file.rules();
    assert_eq!(
        (&start.rules, start.line()),
        (&vtium_settings::Config::builtin(), None)
    );
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_rules(&[], Some(start), displays, "echo ready; read x", no_fonts());
    assert!(s.until(BOUND, |a| shows(a, "ready")));
    assert_eq!(file.listing(), before);
    assert!(before.is_empty());
}

/// D8: with no keyboard among the devices the start found, vtium runs and
/// its notice says no keyboard was found; with one, it says nothing.
#[test]
fn with_no_keyboard_at_start_the_notice_says_so() {
    let displays = || [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::boot(&[], displays(), "echo ready; read x");
    s.app.scanned();
    s.app.turn(now());
    let shown = notice(&s.app, "console").unwrap_or_default();
    assert!(shown.contains("No keyboard found."), "{shown:?}");
    assert!(s.until(BOUND, |a| shows(a, "ready")), "vtium runs");
    let mut s = Seat::boot(&[], displays(), "echo ready; read x");
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    s.app.scanned();
    s.app.turn(now());
    assert_eq!(notice(&s.app, "console"), None);
}

/// The operator's answers (2026-10-30) and D7: an ended head's notice
/// stays and keeps no time, so once 30 s have passed the loop still
/// sleeps, where a timer left in the past would wake it at once forever.
/// The loop turns 400 times, 1 ms apart, and is asked when it next wakes
/// at the instant of its last turn: every timer due by then has fired and
/// moved on in that turn, so a wake of zero is a timer left in the past.
/// Asked even 1 ms later, a blink timer that falls due in that gap is
/// rightly due at once.
#[test]
fn an_ended_heads_notice_leaves_the_loop_asleep() {
    let mut s = claimed_with(&["--oneshot"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(BOUND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.has(PAPER)));
    let t = now() + Duration::from_secs(31);
    let last = t + Duration::from_millis(399);
    for i in 0..400u64 {
        s.app.turn(t + Duration::from_millis(i));
    }
    let wake = s.app.next_wake(last);
    assert!(wake.is_none_or(|w| w > Duration::ZERO), "spins: {wake:?}");
    assert_eq!(notice(&s.app, "small").as_deref(), Some(ENDED));
}

/// D8 and D9: with no keyboard-class device present the anti-lockout
/// guard neither moves the focus nor shows its notice; D8's no-keyboard
/// notice says what there is to say.
#[test]
fn with_no_keyboard_the_anti_lockout_guard_does_nothing() {
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
    s.app.scanned();
    assert!(s.until(BOUND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.unplug("/dev/mem1", now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"), "not moved");
    for name in ["small", "large"] {
        let shown = notice(&s.app, name).unwrap_or_default();
        assert!(!shown.contains(LOCKED_OUT), "{name}: {shown:?}");
    }
    assert!(notice(&s.app, "small").is_some_and(|n| n.contains("No keyboard found.")));
}

/// D7 and D9: a file with `version = 1` alone runs its default head, which
/// a keyboard following the focus reaches, so the anti-lockout guard never
/// acts, through a second keyboard too, and a keyboard a rule ignores
/// stays ignored.
#[test]
fn a_file_with_no_heads_runs_its_default_head_with_no_false_lockout() {
    let displays = [(
        "/dev/mem1",
        Memory::new(640, 320, "MEM-2", "mem-large").lit(),
    )];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let text = "version = 1\n[rules]\ninput path:kbd-ignored -> ignore\n";
    let mut s = Seat::with_heads(&[], Some(text), displays, script);
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    s.app
        .add_input(Path::new(KBD_IGNORED), keyboard("kbd-ignored"));
    assert!(s.until(BOUND, |a| shows(a, "ready")));
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(notice(&s.app, "console"), None);
    s.type_on(KBD_IGNORED, "i\r");
    s.type_on(KBD_A, "a\r");
    assert!(s.until(BOUND, |a| shows(a, "got:a")), "types");
    assert!(
        !shows(&s.app, "got:i"),
        "the ignored keyboard types nowhere"
    );
    s.app.add_input(Path::new(KBD_B), keyboard("kbd-b"));
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(notice(&s.app, "console"), None);
}

/// A head's notices keep each line once and the last few: a keyboard
/// plugged and unplugged again and again shows the anti-lockout notice
/// once, and its repeats restart no time, so it ends 30 s after it is
/// first drawn.
#[test]
fn a_flapping_keyboard_shows_the_anti_lockout_notice_once() {
    let mut s = stranded(start(STRANDED));
    let first = now();
    s.app.turn(first);
    for _ in 0..20 {
        s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
        s.app.turn(now());
        s.app.remove_input(Path::new(KBD_LARGE));
        s.app.turn(now());
    }
    let shown = notice(&s.app, "large").unwrap_or_default();
    assert_eq!(shown.lines().count(), 1, "{shown:?}");
    s.app.turn(first + Duration::from_secs(31));
    assert_eq!(notice(&s.app, "large"), None, "its first time holds");
}

/// D9: the rescue lasts for the rest of the boot: the keyboard unplugged
/// and plugged again still follows the focus.
#[test]
fn a_rescued_keyboard_plugged_again_still_follows_the_focus() {
    let file = HeadFile::new("rescue-replug", RESCUE);
    let mut s = rescue_seat(file.rules());
    s.app
        .device_input(Path::new(KBD_IGNORED), ctrl_alt_f(1), now());
    s.app.remove_input(Path::new(KBD_IGNORED));
    s.app
        .add_input(Path::new(KBD_IGNORED), keyboard("kbd-ignored"));
    s.app.turn(now());
    s.type_on(KBD_IGNORED, "r\r");
    assert!(s.until(BOUND, |a| head_shows(a, "large", "got:r")));
}

/// D9: a keyboard the rescue key moved reaches no lit head while the
/// focus's head is unlit, so the anti-lockout guard still acts and moves
/// the focus to a lit head.
#[test]
fn a_rescued_keyboard_on_an_unlit_focus_leaves_the_anti_lockout_guard_on() {
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-small").lit(),
        ),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    assert_eq!(s.app.focus_head(), Some("small"));
    s.app
        .device_input(Path::new(KBD_LARGE), ctrl_alt_f(1), now());
    s.unplug("/dev/mem0", now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"), "moved to the lit head");
}

/// D9: the rescue key's notice names the head the keyboard types into
/// once the guards have run: here the anti-lockout guard moves the active
/// head away from an unlit head as the rescue lands.
#[test]
fn the_rescue_notice_names_the_head_typing_goes_to() {
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-small").lit(),
        ),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, script);
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(BOUND, |a| head_shows(a, "small", "ready")));
    s.unplug("/dev/mem0", now());
    assert_eq!(s.app.focus_head(), Some("small"), "kbd-large reaches large");
    s.app
        .device_input(Path::new(KBD_LARGE), ctrl_alt_f(1), now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"));
    let shown = notice(&s.app, "large").unwrap_or_default();
    assert!(
        shown.contains("now types into large, the active screen, until the next boot"),
        "{shown:?}"
    );
}

/// D9: with no active head the rescue key's notice says so, on every
/// head.
#[test]
fn the_rescue_notice_says_when_no_head_is_active() {
    let mut s = Seat::with_heads(&[], Some(CLAIMED), [], "read x");
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), None);
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_alt_f(1), now());
    s.app.turn(now());
    for name in ["small", "large"] {
        let shown = notice(&s.app, name).unwrap_or_default();
        assert!(
            shown.contains("but no screen is active."),
            "{name}: {shown:?}"
        );
    }
}

/// D9: only the shown VT's own key rescues: Ctrl-Alt-F2 pressed on a
/// claimed keyboard while vtium is on VT 1 moves nothing.
#[test]
fn another_vts_key_rescues_nothing() {
    let file = HeadFile::new("rescue-other-vt", RESCUE);
    let mut s = rescue_seat(file.rules());
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_alt_f(2), now());
    assert_eq!(notice(&s.app, "large"), None);
    s.type_on(KBD_SMALL, "\rs\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:s")),
        "still small's"
    );
}

/// D9: a connected display always lights. The rules light the large
/// display alone, and the small one, left dark at its bind while the large
/// was lit, is never announced; once the large goes the display guard lets
/// the small go for its device to announce and lights it, and once the
/// large is back the small goes dark again.
#[test]
fn a_display_left_dark_at_its_bind_lights_when_the_lit_one_goes() {
    let displays = [
        (
            "/dev/mem0",
            Memory::new(320, 160, "MEM-1", "mem-small").held(),
        ),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
    ];
    let text = "version = 1\n[head desk]\n[rules]\ndisplay card:mem-large -> desk\n";
    let mut s = Seat::with_heads(&[], Some(text), displays, "echo ready; read x");
    assert!(s.until(BOUND, |a| a.grid() == (80, 20)), "the large alone");
    s.unplug("/dev/mem1", now());
    assert!(s.until(BOUND, |a| a.grid() == (40, 10)), "the small lit");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::On));
    assert_eq!(s.app.cells().len(), 1);
    s.plug("/dev/mem1", now());
    assert!(s.until(BOUND, |a| a.grid() == (80, 20)), "the large again");
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::Off),
        "the small dark again"
    );
    assert_eq!(s.app.cells().len(), 1);
}

/// The head file of the session-end tests: head small, of `n` consoles,
/// on the small display, with its keyboard.
fn small_consoles(n: usize) -> String {
    format!(
        "version = 1\n[head small]\nconsoles = {n}\n[rules]\ndisplay card:mem-small -> small\ninput path:kbd-small -> small\n"
    )
}

/// What each console of the session-end tests runs: it counts its starts
/// in `dir`'s `starts.N`, where N is its console's id, and shows its pty's
/// size as it starts, as `size:ROWS COLS` and as `runK:ROWS COLS` for its
/// Kth start; asks OSC 50 for the font that the file `switch.N` names, and
/// removes the file, so the console's next session asks nothing; where
/// `exit.N` is there, removes it and exits once `go.N` appears, removing
/// that too; and otherwise echoes each line it reads, ending at `q`.
fn switching(dir: &Path) -> String {
    format!(
        "d={d}; n=$VTIUM_CONSOLE; echo x >> $d/starts.$n; \
         echo \"size:$(stty size)\"; echo \"run$(wc -l < $d/starts.$n):$(stty size)\"; \
         f=$d/switch.$n; if [ -e $f ]; then x=$(cat $f); rm $f; printf \"\\033]50;$x\\007\"; fi; \
         if [ -e $d/exit.$n ]; then rm $d/exit.$n; \
         while [ ! -e $d/go.$n ]; do sleep 0.02; done; rm $d/go.$n; exit 0; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    )
}

/// The line console `id` of head small shows for its `run`th start, once
/// it does.
fn run_line(app: &App, id: usize, run: usize) -> Option<String> {
    console_lines(app, "small", &format!("run{run}:"))
        .get(id - 1)
        .cloned()
        .flatten()
}

/// A directory of its own for `test`, holding the frits a, b and c, each
/// in its own cell, 10x20, 16x32 and 20x40, so each draws its own grid on
/// the small display at `font-size=0`, and `conf` as its vtium.conf; the
/// directory and the font sources, whose stand-in helper answers nothing.
fn reset_fixture(test: &str, conf: &str) -> (Scratch, Sources) {
    let (dir, fonts, _) = frits(test, &[]);
    for (name, cell) in [("a", "10x20"), ("b", "16x32"), ("c", "20x40")] {
        std::fs::write(
            dir.join(format!("{name}.vtfrit")),
            format!("source=kbd:x\ncell={cell}\n"),
        )
        .expect("a frit");
    }
    std::fs::write(dir.join("vtium.conf"), conf).expect("vtium.conf");
    (dir, fonts)
}

/// The seat on the small display alone, its configuration read from
/// `dir`'s vtium.conf with no `font-size` on its command line, running the
/// head file `heads` with every console running [`switching`]. At a
/// session's end the seat reads the configuration again through the same
/// reader (ADR-1201 D11); the seat and the count of those reads.
fn reset_seat(dir: &Path, heads: &str, fonts: Sources) -> (Seat, Rc<Cell<u32>>) {
    seat_running(dir, heads, fonts, &switching(dir))
}

/// [`reset_seat`], with every console running `script`.
fn seat_running(dir: &Path, heads: &str, fonts: Sources, script: &str) -> (Seat, Rc<Cell<u32>>) {
    let configdir = format!("--configdir={}", dir.display());
    let args: Vec<OsString> = [configdir.as_str(), "--login", "--", "/bin/sh", "-c", script]
        .iter()
        .map(OsString::from)
        .collect();
    let loaded = config::load(&args, &mut config::read_file);
    assert_eq!(loaded.reports, [], "the seat's configuration");
    let lp: Loop<App> = Loop::new().expect("a loop");
    let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
    let (vt, released) = CountingVt::new(FakeVt::with_number(1));
    let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), Rc::new(now), env);
    app.set_font_sources(fonts);
    let reads = Rc::new(Cell::new(0));
    let counted = Rc::clone(&reads);
    app.set_config_reader(Box::new(move || {
        counted.set(counted.get() + 1);
        config::load(&args, &mut config::read_file)
    }));
    app.set_heads(start(heads));
    app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Memory::new(320, 160, "MEM-1", "mem-small")),
    );
    app.vt.activate().expect("the fake VT");
    app.start();
    app.turn(now());
    let mut s = Seat {
        lp,
        app,
        released,
        _reap: Reap,
    };
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    (s, reads)
}

/// The `size:` line of each console of head small, in the ring's order.
fn sizes(app: &App) -> Vec<Option<String>> {
    console_lines(app, "small", "size:")
}

/// Ends the session of the console small shows: a line it echoes first,
/// then `q`, and waits for the console's next session to show its size,
/// the line gone with the hard reset of the restart. Returns that size.
fn end_shown_session(s: &mut Seat) -> String {
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(BOUND, |a| head_shows(a, "small", "got:x")));
    s.type_on(KBD_SMALL, "q\r");
    let restarted = |a: &App| {
        !head_shows(a, "small", "got:x")
            && a.head("small")
                .is_some_and(|h| h.screen_text().iter().any(|l| l.starts_with("size:")))
    };
    assert!(s.until(BOUND, restarted), "the session started again");
    let text = s
        .app
        .head("small")
        .map(Head::screen_text)
        .unwrap_or_default();
    text.into_iter()
        .find(|l| l.starts_with("size:"))
        .unwrap_or_default()
}

/// vtium/vtium#384, ADR-1200 D5: OSC 50's query reports a name OSC 50
/// takes for the font on screen, or none, never a font that is not shown.
/// The first session switches to frit b; when it ends, vtium returns the
/// head to the font file vtium.conf's `font-name` gives, which has no
/// font set's name, so the next session's query names no font, not b.
#[test]
fn after_a_return_to_a_font_file_the_query_names_no_font() {
    let path = "/usr/share/kbd/consolefonts/ter-v12n.psf.gz";
    let conf = format!("font-engine=psf\nfont-name={path}\nfont-size=16\n");
    let (dir, fonts) = reset_fixture("reset-path", &conf);
    // The first session switches to b, and the next asks which font is in
    // use; the tty echoes the reply.
    let script = format!(
        "d={d}; echo x >> $d/runs; k=$(wc -l < $d/runs); echo \"size:$(stty size)\"; \
         if [ $k = 1 ]; then printf '\\033]50;b\\007'; else printf '\\033]50;?\\007'; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    );
    let (mut s, _) = seat_running(&dir, &small_consoles(1), fonts, &script);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    end_shown_session(&mut s);
    assert_eq!(s.app.font_name(), path, "returned to the font file");
    let replied = |a: &App| head_shows(a, "small", "]50;");
    assert!(
        s.until(BOUND, replied),
        "{:?}",
        s.app.head("small").map(Head::screen_text)
    );
    let text = s
        .app
        .head("small")
        .map(Head::screen_text)
        .unwrap_or_default();
    let replies: Vec<&String> = text.iter().filter(|l| l.contains("]50;")).collect();
    assert_eq!(replies.len(), 1, "{text:?}");
    assert!(replies[0].ends_with("]50;^G"), "no font named: {replies:?}");
}

/// ADR-1401 D10, ADR-1403 D5: a look naming a frit its group can write
/// draws the built-in font, not the frit's 20x40 cell, and the start logs
/// one line that names the file and the command that mends it. The return
/// at the end of a session that switched to frit a reads the look's frit
/// again, and refuses it again in one line.
#[test]
fn a_looks_frit_others_can_change_draws_the_built_in_font_with_one_line() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, fonts) = reset_fixture("reset-untrusted-look", "font-size=0\n");
    let g = dir.join("g.vtfrit");
    std::fs::write(&g, "source=kbd:x\ncell=20x40\n").expect("the frit");
    std::fs::set_permissions(&g, std::fs::Permissions::from_mode(0o664)).expect("its mode");
    std::fs::write(dir.join("switch.1"), "a").expect("the switch");
    let heads = "version = 1\n[look g]\nfont-name = g\n[head small]\nconsoles = 1\nlook = g\n[rules]\ndisplay card:mem-small -> small\ninput path:kbd-small -> small\n";
    logged();
    let (mut s, _) = reset_seat(&dir, heads, fonts);
    let shown = g.display();
    let line = format!(
        "vtium does not read the font set g from \"{shown}\", because other users can change \"{shown}\" (mode 664). Run `chmod go-w {shown}` so that only its owner can change it. vtium draws its built-in font instead."
    );
    let about = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains("g.vtfrit"))
            .cloned()
            .collect()
    };
    let started = logged();
    assert_eq!(about(&started), std::slice::from_ref(&line), "{started:?}");
    assert!(s.until(BOUND, |a| a.font_name() == "a"), "switched to a");
    assert_eq!(s.app.cells(), [(10, 20)]);
    end_shown_session(&mut s);
    assert!(
        ["unifont", "unscii-8"].contains(&s.app.font_name()),
        "the built-in font: {}",
        s.app.font_name()
    );
    assert_ne!(s.app.cells(), [(20, 40)], "not the frit's cell");
    let returned = logged();
    assert_eq!(about(&returned), [line], "{returned:?}");
}

/// vtium/vtium#384, ADR-1200 D5: the font is the head's, so every console
/// of the head reports the font on screen. Console 1 switches the head to
/// frit b while console 2, hidden, has asked which font is in use; shown,
/// console 2 is answered b, not the font the head started in.
#[test]
fn a_hidden_consoles_query_names_the_font_another_console_switched_to() {
    let (dir, fonts) = reset_fixture("reset-hidden-query", "font-name=a\nfont-size=0\n");
    let script = "n=$VTIUM_CONSOLE; echo \"size:$(stty size)\"; \
         if [ $n = 1 ]; then printf '\\033]50;b\\007'; else printf '\\033]50;?\\007'; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done";
    let (mut s, _) = seat_running(&dir, &small_consoles(2), fonts, script);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    press_chord(&mut s, KBD_SMALL, RIGHT);
    let replied = |a: &App| head_shows(a, "small", "]50;");
    assert!(
        s.until(BOUND, replied),
        "{:?}",
        s.app.head("small").map(Head::screen_text)
    );
    let text = s
        .app
        .head("small")
        .map(Head::screen_text)
        .unwrap_or_default();
    let replies: Vec<&String> = text.iter().filter(|l| l.contains("]50;")).collect();
    assert_eq!(replies.len(), 1, "{text:?}");
    assert!(replies[0].ends_with("]50;b^G"), "{replies:?}");
}

/// vtium/vtium#384, ADR-1200 D5: a head whose display has gone keeps its
/// session, and a switch there changes the font all the same, though
/// there is nothing to lay out. The query that follows names the font
/// switched to, b, not the font the head started in.
#[test]
fn a_query_on_a_head_with_no_display_names_the_font_switched_to() {
    let (dir, fonts) = reset_fixture("reset-no-display", "font-name=a\nfont-size=0\n");
    let script = format!(
        "d={d}; echo \"size:$(stty size)\"; while [ ! -e $d/go ]; do sleep 0.02; done; \
         printf '\\033]50;b\\007\\033]50;?\\007'; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    );
    let (mut s, _) = seat_running(&dir, &small_consoles(1), fonts, &script);
    assert!(s.until(BOUND, |a| head_shows(a, "small", "size:")));
    assert_eq!(s.app.font_name(), "a");
    s.unplug("/dev/mem0", now());
    std::fs::write(dir.join("go"), "").expect("the go mark");
    let replied = |a: &App| head_shows(a, "small", "]50;");
    assert!(
        s.until(BOUND, replied),
        "{:?}",
        s.app.head("small").map(Head::screen_text)
    );
    let text = s
        .app
        .head("small")
        .map(Head::screen_text)
        .unwrap_or_default();
    let replies: Vec<&String> = text.iter().filter(|l| l.contains("]50;")).collect();
    assert_eq!(replies.len(), 1, "{text:?}");
    assert!(replies[0].ends_with("]50;b^G"), "{replies:?}");
}

/// ADR-1201 D11: an OSC 50 switch lasts for the session. When the session
/// in the console it came from ends, vtium returns the head to the font
/// vtium.conf configures, at its size, before the console's next session
/// starts: the next login's pty is the configured font's grid.
// covers: text.font-change
#[test]
fn a_sessions_end_returns_its_switch_to_the_configured_font() {
    let (dir, fonts) = reset_fixture("reset-switch", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    assert_eq!(grid(&s.app, "small"), Some((20, 5)));
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:8 32", "the next login starts in a's grid");
    assert_eq!(s.app.font_name(), "a");
    assert_eq!(s.app.cells(), [(10, 20)]);
    assert_eq!(reads.get(), 1, "vtium.conf read again once");
}

/// ADR-1201 D11: the size comes back with the font, so a zoom made in the
/// session is undone at its end, before the next login starts.
// covers: text.font-change
#[test]
fn a_sessions_end_undoes_its_zoom() {
    let (dir, fonts) = reset_fixture("reset-zoom", "font-size=16\n");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| sizes(a) == [Some("size:10 40".to_owned())]));
    let equal = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('=')
    });
    s.app.device_input(Path::new(KBD_SMALL), equal, now());
    assert_eq!(grid(&s.app, "small"), Some((20, 5)), "zoomed");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:10 40", "the next login starts at 16 points");
    assert_eq!(s.app.cells(), [(8, 16)]);
}

/// ADR-1201 D11, ADR-1302 D13: a session's end reads `font-line-spacing`
/// again with the other font keys, so a spacing saved since the start is
/// the spacing the next login draws with, at the configured size: the zoom
/// is undone back to text 16 rows high, now with 50% added, an 8x24 cell.
#[test]
fn a_sessions_end_takes_the_line_spacing_vtium_conf_names_now() {
    let (dir, fonts) = reset_fixture("reset-spacing", "font-size=16\nfont-line-spacing=25\n");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| sizes(a) == [Some("size:8 40".to_owned())]));
    assert_eq!(s.app.cells(), [(8, 20)]);
    let equal = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('=')
    });
    s.app.device_input(Path::new(KBD_SMALL), equal, now());
    assert_eq!(s.app.cells(), [(16, 40)], "zoomed, the spacing kept");
    std::fs::write(
        dir.join("vtium.conf"),
        "font-size=16\nfont-line-spacing=50\n",
    )
    .expect("the edit");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:6 40", "the next login starts with 50% added");
    assert_eq!(s.app.cells(), [(8, 24)]);
    assert_eq!(reads.get(), 1, "vtium.conf read again once");
}

/// ADR-1201 D11, ADR-1403 D5: the size a session's end returns to is the
/// head's, its look's `font-size` over vtium.conf's, so a zoom made in the
/// session is undone back to the look's size.
#[test]
fn a_sessions_end_returns_to_its_looks_size() {
    let (dir, fonts) = reset_fixture("reset-look", "font-size=16\n");
    let heads = "version = 1\n[look big]\nfont-size = 32\n[head small]\nlook = big\n\
                 [rules]\ndisplay card:mem-small -> small\ninput path:kbd-small -> small\n";
    let (mut s, reads) = reset_seat(&dir, heads, fonts);
    assert!(s.until(BOUND, |a| sizes(a) == [Some("size:5 20".to_owned())]));
    let minus = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('-')
    });
    s.app.device_input(Path::new(KBD_SMALL), minus, now());
    assert_ne!(grid(&s.app, "small"), Some((20, 5)), "zoomed");
    let size = end_shown_session(&mut s);
    assert_eq!(
        size, "size:5 20",
        "the next login starts at the look's 32 points"
    );
    assert_eq!(s.app.cells(), [(16, 32)]);
    assert_eq!(reads.get(), 1, "vtium.conf read again once");
}

/// What each console of the window tests runs: it shows its pty's size
/// as it starts, as `size:ROWS COLS`. Where the file `ask.N` in `dir`
/// holds the parameters of an XTWINOPS 8 request, N being its console's
/// id, it removes the file, sends `CSI 8 ; PARAMS t` and the size report's
/// request `CSI 18 t` in one write, and shows the reply it reads, as `r:`
/// and the bytes as od prints them, then its pty's size, as `a:ROWS COLS`.
/// It then echoes each line it reads, ending at `q`.
fn asking(dir: &Path) -> String {
    format!(
        "d={d}; n=$VTIUM_CONSOLE; echo \"size:$(stty size)\"; \
         f=$d/ask.$n; if [ -e $f ]; then x=$(cat $f); rm $f; \
         stty -echo -icanon min 0 time 5; printf \"\\033[${{x}}t\\033[18t\"; \
         r=$(dd bs=1 count=32 2>/dev/null | od -An -c | tr -d ' \\n'); \
         stty echo icanon; echo \"r:$r\"; echo \"a:$(stty size)\"; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    )
}

/// The seat of the window tests on the small display, 320x160, whose one
/// console runs [`asking`] with `conf` as its vtium.conf and `ask` as the
/// request's parameters. Unifont draws 40x10 cells of 8x16 there at 16
/// pixels, 20x5 of 16x32 at 32, and 13x3 of 24x48 at 48.
fn window_seat(test: &str, conf: &str, ask: &str) -> (Scratch, Seat) {
    let (dir, fonts) = reset_fixture(test, conf);
    std::fs::write(dir.join("ask.1"), ask).expect("the request");
    let script = asking(&dir);
    let (s, _) = seat_running(&dir, &small_consoles(1), fonts, &script);
    (dir, s)
}

/// The `r:` and `a:` lines of head small's console, once both show.
fn asked_lines(s: &mut Seat) -> (String, String) {
    let line = |a: &App, p: &str| console_lines(a, "small", p).into_iter().flatten().next();
    assert!(
        s.until(BOUND, |a| line(a, "a:").is_some()),
        "the session read the reply"
    );
    (
        line(&s.app, "r:").unwrap_or_default(),
        line(&s.app, "a:").unwrap_or_default(),
    )
}

/// ctlseqs, XTWINOPS 8: under `allow-window-ops`, a program's request for
/// 4 rows of 15 columns makes a grid of exactly that size, at the largest
/// font size whose cells the display holds 15x4 of: unifont's 32, as 48
/// holds 13x3. The pty is resized before the size report that followed in
/// the same write is answered, and the report answers the new size. A
/// zoom then fills the display again.
// covers: window.resize
#[test]
fn a_programs_request_for_a_grid_takes_the_zoom_path() {
    let (_dir, mut s) = window_seat(
        "window-allowed",
        "font-size=16\nallow-window-ops\n",
        "8;4;15",
    );
    let (reply, after) = asked_lines(&mut s);
    assert_eq!(reply, "r:033[8;4;15t", "CSI 18 t answers the new size");
    assert_eq!(after, "a:4 15", "the pty is resized");
    assert_eq!(grid(&s.app, "small"), Some((15, 4)));
    assert_eq!(s.app.cells(), [(16, 32)]);
    let minus = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('-')
    });
    s.app.device_input(Path::new(KBD_SMALL), minus, now());
    assert_eq!(grid(&s.app, "small"), Some((40, 10)), "a zoom fills again");
}

/// ctlseqs: an omitted side keeps its length and a side of 0 takes the
/// display's. 15 columns with 0 rows draws at 32, the largest size that
/// holds 15 columns, with the 5 rows the display holds there.
#[test]
fn a_side_of_zero_takes_what_the_display_holds() {
    let (_dir, mut s) = window_seat("window-zero", "font-size=16\nallow-window-ops\n", "8;0;15");
    let (reply, after) = asked_lines(&mut s);
    assert_eq!(
        (reply.as_str(), after.as_str()),
        ("r:033[8;5;15t", "a:5 15")
    );
    assert_eq!(grid(&s.app, "small"), Some((15, 5)));
}

/// With `allow-window-ops` off, its default, vtium ignores the request:
/// the grid, the font size and the pty stay, and the size report answers
/// them.
// covers: window.resize
#[test]
fn a_request_with_allow_window_ops_off_changes_nothing() {
    let (_dir, mut s) = window_seat("window-refused", "font-size=16\n", "8;4;15");
    let (reply, after) = asked_lines(&mut s);
    assert_eq!(reply, "r:033[8;10;40t");
    assert_eq!(after, "a:10 40");
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
    assert_eq!(s.app.cells(), [(8, 16)]);
}

/// A size the display cannot hold even at the smallest font size takes,
/// on that side, what the display holds there: from 20x5 at 32, 8 rows of
/// 100 columns draws at 16, unifont's smallest, with its 8 rows and the 40
/// columns the display holds.
// covers: window.resize
#[test]
fn a_size_the_display_cannot_hold_takes_what_it_holds_at_the_smallest_size() {
    let (_dir, mut s) = window_seat(
        "window-too-large",
        "font-size=32\nallow-window-ops\n",
        "8;8;100",
    );
    let (reply, after) = asked_lines(&mut s);
    assert_eq!(reply, "r:033[8;8;40t");
    assert_eq!(after, "a:8 40");
    assert_eq!(grid(&s.app, "small"), Some((40, 8)));
    assert_eq!(s.app.cells(), [(8, 16)]);
}

/// A row count far past the display, 5000000000, takes the most rows the
/// display holds, as 100 rows does: both draw at unifont's smallest size, 8
/// pixels, whose 8x8 cells give the 40 columns and 20 rows the display
/// holds.
#[test]
fn a_huge_row_count_takes_the_most_rows_the_display_holds() {
    for (test, ask) in [
        ("window-huge", "8;5000000000;80"),
        ("window-tall", "8;100;80"),
    ] {
        let (_dir, mut s) = window_seat(test, "font-size=16\nallow-window-ops\n", ask);
        let (reply, after) = asked_lines(&mut s);
        assert_eq!(
            (reply.as_str(), after.as_str()),
            ("r:033[8;20;40t", "a:20 40"),
            "{ask}"
        );
        assert_eq!(s.app.cells(), [(8, 8)], "{ask}");
    }
}

/// A side of 0 beside a side the display cannot hold fills the display at
/// the largest size that holds the most of the other side: 100 columns
/// with 0 rows from 20x5 at 32 draws 40 columns at 16, as unifont's
/// smallest, 8, holds no more of them, with the 10 rows 16 holds.
#[test]
fn a_side_of_zero_beside_one_the_display_cannot_hold_fills_at_the_largest_size() {
    let (_dir, mut s) = window_seat(
        "window-too-wide",
        "font-size=32\nallow-window-ops\n",
        "8;0;100",
    );
    let (reply, after) = asked_lines(&mut s);
    assert_eq!(
        (reply.as_str(), after.as_str()),
        ("r:033[8;10;40t", "a:10 40")
    );
    assert_eq!(s.app.cells(), [(8, 16)]);
}

/// ADR-1401 D1: the grid a program asked for is its console's own. Console
/// 2, shown after console 1 asked for 15x4, fills the display at its own
/// size, and console 1 has 15x4 again when it is shown again.
#[test]
fn a_programs_grid_stays_with_its_console() {
    let (dir, fonts) = reset_fixture("window-consoles", "font-size=16\nallow-window-ops\n");
    std::fs::write(dir.join("ask.1"), "8;4;15").expect("the request");
    let script = asking(&dir);
    let (mut s, _) = seat_running(&dir, &small_consoles(2), fonts, &script);
    let (_, after) = asked_lines(&mut s);
    assert_eq!(after, "a:4 15");
    assert_eq!(grid(&s.app, "small"), Some((15, 4)));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)), "console 2 shown");
    assert_eq!(grid(&s.app, "small"), Some((40, 10)), "console 2 fills");
    assert_eq!(s.app.cells(), [(8, 16)]);
    press_chord(&mut s, KBD_SMALL, LEFT);
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "console 1 shown");
    assert_eq!(
        grid(&s.app, "small"),
        Some((15, 4)),
        "console 1's grid again"
    );
    assert_eq!(s.app.cells(), [(16, 32)]);
}

/// ADR-1201 D11: the size a program asked for lasts for its session, so
/// the next login starts at the configured size, filling the display.
#[test]
fn a_sessions_end_undoes_a_programs_grid() {
    let (_dir, mut s) = window_seat("window-reset", "font-size=16\nallow-window-ops\n", "8;4;15");
    let (_, after) = asked_lines(&mut s);
    assert_eq!(after, "a:4 15");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:10 40", "the next login starts at 16 points");
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
}

/// ADR-1201 D11: one font for a head. A session ending in another console
/// of the head than the one the switch came from leaves the font, so it
/// never changes under another session.
#[test]
fn a_session_ending_in_another_console_leaves_the_font() {
    let (dir, fonts) = reset_fixture("reset-other", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(2), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:5 20", "console 2's next login starts in b");
    assert_eq!(s.app.font_name(), "b");
    assert_eq!(reads.get(), 0, "nothing to return, nothing read");
}

/// ADR-1201 D11: a session ending in a head's only console returns the
/// font, wherever the switch came from: here from console 2, which closed.
#[test]
fn a_session_ending_in_a_heads_only_console_returns_the_font() {
    let (dir, fonts) = reset_fixture("reset-only", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.2"), "b").expect("the switch");
    let (mut s, _) = reset_seat(&dir, &small_consoles(2), fonts);
    assert!(s.until(BOUND, |a| sizes(a).iter().all(Option::is_some)));
    // Console 2's request waits for it to be shown.
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "console 2 closed");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:8 32");
    assert_eq!(s.app.font_name(), "a");
}

/// ADR-1201 D11: vtium.conf is read again at the session's end, so a font
/// saved since the start is the font the next login draws.
#[test]
fn a_sessions_end_takes_the_font_vtium_conf_names_now() {
    let (dir, fonts) = reset_fixture("reset-edited", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    std::fs::write(dir.join("vtium.conf"), "font-name=c\nfont-size=0\n").expect("the edit");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:4 16", "the next login starts in c's grid");
    assert_eq!(s.app.font_name(), "c");
}

/// ADR-1201 D11: a font key that does not read is reported and dropped, as
/// it is at the start: `font-size=big` falls back to its default, 0, and
/// the head returns to a in a's own cell.
#[test]
fn a_font_key_that_does_not_read_at_a_sessions_end_is_reported_and_dropped() {
    let (dir, fonts) = reset_fixture("reset-unread", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    std::fs::write(dir.join("vtium.conf"), "font-name=a\nfont-size=big\n").expect("the edit");
    logged();
    let size = end_shown_session(&mut s);
    let report = format!(
        "{}:2: font-size=\"big\" is not a whole number, ignored",
        dir.join("vtium.conf").display()
    );
    let lines = logged();
    assert!(lines.iter().any(|l| l.ends_with(&report)), "{lines:?}");
    assert_eq!(size, "size:8 32");
    assert_eq!(s.app.font_name(), "a");
}

/// ADR-1201 D11, ADR-1403 D3: a vtium.conf that others can write by the
/// session's end is refused at the read again, as at the start: the log
/// says why and gives the command that mends it, and none of the file's
/// font keys apply, so the head returns to the default font.
#[test]
fn a_file_refused_at_a_sessions_end_is_reported_as_at_the_start() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, fonts) = reset_fixture("reset-untrusted", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    let file = dir.join("vtium.conf");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o664)).expect("0664");
    logged();
    end_shown_session(&mut s);
    let lines = logged();
    let mend = format!("Run `chmod go-w {}`", file.display());
    assert!(
        lines
            .iter()
            .any(|l| l.contains("(mode 664)") && l.contains(&mend)),
        "{lines:?}"
    );
    assert_eq!(reads.get(), 1, "vtium.conf read again once");
    assert_ne!(s.app.font_name(), "a", "none of the file applies");
    assert_ne!(s.app.font_name(), "b", "the switch is undone");
}

/// ADR-1201 D11: with nothing switched or zoomed, a session's end returns
/// nothing: vtium.conf is not read again, and the font stays, though the
/// file names another now.
#[test]
fn a_sessions_end_with_nothing_switched_or_zoomed_reads_and_reloads_nothing() {
    let (dir, fonts) = reset_fixture("reset-none", "font-name=a\nfont-size=0\n");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| sizes(a) == [Some("size:8 32".to_owned())]));
    std::fs::write(dir.join("vtium.conf"), "font-name=c\nfont-size=0\n").expect("the edit");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:8 32");
    assert_eq!(s.app.font_name(), "a");
    assert_eq!(reads.get(), 0, "vtium.conf not read again");
}

/// ADR-1201 D11 and ADR-1401 D8: a hang-up ends the session too. The head
/// loses its display under `when-display-lost = hangup`, and its session,
/// which switched the font, starts again in the configured font, which the
/// display draws when it comes back.
#[test]
fn a_hang_up_returns_the_font_before_the_session_starts_again() {
    let (dir, fonts) = reset_fixture("reset-hangup", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let heads = small_consoles(1).replace(
        "consoles = 1\n",
        "consoles = 1\nwhen-display-lost = hangup\n",
    );
    let (mut s, _) = reset_seat(&dir, &heads, fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    s.unplug("/dev/mem0", now());
    assert!(
        s.until(BOUND, |a| run_line(a, 1, 2).is_some()),
        "hung up and started again"
    );
    let frames = s.memory("/dev/mem0").frames;
    s.plug("/dev/mem0", now());
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "drawn again on its return"
    );
    assert_eq!(s.app.font_name(), "a");
    assert_eq!(grid(&s.app, "small"), Some((32, 8)));
}

/// The lines in `dir`'s `starts.N`: how many times console N's session
/// started.
fn starts(dir: &Path, id: u32) -> usize {
    std::fs::read_to_string(dir.join(format!("starts.{id}")))
        .map(|t| t.lines().count())
        .unwrap_or(0)
}

/// ADR-1201 D11: an OSC 50 request goes with the session that sent it. A
/// hidden console's request waits for the console to be shown, and when
/// the session ends first, its next session shows in the configured font.
#[test]
fn a_hidden_consoles_request_goes_with_its_session() {
    let (dir, fonts) = reset_fixture("reset-stale", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.2"), "b").expect("the switch");
    std::fs::write(dir.join("exit.2"), "").expect("the exit");
    let (mut s, _) = reset_seat(&dir, &small_consoles(2), fonts);
    assert!(s.until(BOUND, |a| run_line(a, 2, 1).is_some()));
    std::fs::write(dir.join("go.2"), "").expect("the go mark");
    assert!(
        s.until(BOUND, |_| starts(&dir, 2) == 2),
        "console 2 started again"
    );
    assert_eq!(s.app.font_name(), "a");
    press_chord(&mut s, KBD_SMALL, RIGHT);
    s.type_on(KBD_SMALL, "x\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:x")),
        "console 2's new session runs, shown"
    );
    assert_eq!(
        s.app.font_name(),
        "a",
        "the ended session's request is gone"
    );
}

/// ADR-1201 D11: closing a console ends its session, so closing the
/// console the switch came from returns the head to the configured font at
/// once, and nothing stays recorded against the closed console: a session
/// ending later in another console reads nothing again and starts in the
/// configured font.
#[test]
fn closing_the_console_a_switch_came_from_returns_the_font() {
    let (dir, fonts) = reset_fixture("reset-close", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.2"), "b").expect("the switch");
    let (mut s, reads) = reset_seat(&dir, &small_consoles(3), fonts);
    assert!(s.until(BOUND, |a| sizes(a).iter().all(Option::is_some)));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)), "console 3 shown");
    assert_eq!(s.app.font_name(), "a");
    assert_eq!(grid(&s.app, "small"), Some((32, 8)));
    assert_eq!(reads.get(), 1);
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:8 32");
    assert_eq!(reads.get(), 1, "nothing recorded against console 2");
}

/// ADR-1201 D11 and ADR-1200 D16: a configured `font-size` wins over a
/// frit's own cell, so when a zoom returns after vtium.conf's `font-size`
/// changed from 0 to 16, the next login draws frit a at 16 points in the
/// built-in font's 8x16 cell, as a fresh start with that file does.
#[test]
fn a_zoom_returning_after_a_font_size_edit_draws_as_a_fresh_start() {
    let (dir, fonts) = reset_fixture("reset-sized", "font-name=a\nfont-size=16\n");
    let (fresh, _) = reset_seat(&dir, &small_consoles(1), fonts.clone());
    assert_eq!(
        (grid(&fresh.app, "small"), fresh.app.cells()),
        (Some((40, 10)), vec![(8, 16)]),
        "a fresh start at 16 points"
    );
    drop(fresh);
    std::fs::write(dir.join("vtium.conf"), "font-name=a\nfont-size=0\n").expect("the edit");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert_eq!(grid(&s.app, "small"), Some((32, 8)), "a's own cell");
    let zoom = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('=')
    });
    s.app.device_input(Path::new(KBD_SMALL), zoom, now());
    assert_eq!(grid(&s.app, "small"), Some((16, 4)), "zoomed");
    std::fs::write(dir.join("vtium.conf"), "font-name=a\nfont-size=16\n").expect("the edit");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:10 40");
    assert_eq!(s.app.cells(), [(8, 16)]);
}

/// ADR-1201 D11: the switch's console need not be shown when its session
/// ends. Console 2 switches the font, console 1 is shown again, and console
/// 2's session ends while hidden: the head returns to a, and console 2's
/// next session starts in a's grid.
#[test]
fn a_hidden_console_that_switched_returns_the_font_at_its_sessions_end() {
    let (dir, fonts) = reset_fixture("reset-hidden", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.2"), "b").expect("the switch");
    std::fs::write(dir.join("exit.2"), "").expect("the exit");
    let (mut s, _) = reset_seat(&dir, &small_consoles(2), fonts);
    assert!(s.until(BOUND, |a| run_line(a, 2, 1).is_some()));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    press_chord(&mut s, KBD_SMALL, LEFT);
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)));
    std::fs::write(dir.join("go.2"), "").expect("the go mark");
    assert!(
        s.until(BOUND, |a| run_line(a, 2, 2).is_some()),
        "console 2 started again"
    );
    assert_eq!(s.app.font_name(), "a");
    assert_eq!(run_line(&s.app, 2, 2).as_deref(), Some("run2:8 32"));
}

/// ADR-1201 D11: a hidden console's return leaves the shown console's
/// zoom. Console 2, zoomed in, ends while console 1, zoomed out, is shown;
/// console 1 keeps its zoom when it is laid out again after a console
/// switch.
#[test]
fn a_hidden_consoles_return_leaves_the_shown_consoles_zoom() {
    let (dir, fonts) = reset_fixture("reset-keep-zoom", "font-size=16\n");
    std::fs::write(dir.join("exit.2"), "").expect("the exit");
    let (mut s, _) = reset_seat(&dir, &small_consoles(2), fonts);
    assert!(s.until(BOUND, |a| run_line(a, 2, 1).is_some()));
    let zoom = |c: char| {
        InputEvent::Key(KeyEvent {
            mods: Mods::CTRL,
            ..char_key(c)
        })
    };
    press_chord(&mut s, KBD_SMALL, RIGHT);
    s.app.device_input(Path::new(KBD_SMALL), zoom('='), now());
    assert_eq!(grid(&s.app, "small"), Some((20, 5)), "console 2 zoomed in");
    press_chord(&mut s, KBD_SMALL, LEFT);
    s.app.device_input(Path::new(KBD_SMALL), zoom('-'), now());
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 20)),
        "console 1 zoomed out"
    );
    std::fs::write(dir.join("go.2"), "").expect("the go mark");
    assert!(
        s.until(BOUND, |a| run_line(a, 2, 2).is_some()),
        "console 2 started again"
    );
    assert_eq!(run_line(&s.app, 2, 2).as_deref(), Some("run2:10 40"));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 10)),
        "console 2 at 16 points"
    );
    press_chord(&mut s, KBD_SMALL, LEFT);
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 20)),
        "console 1 keeps its zoom"
    );
}

/// ADR-1201 D11: the re-read takes the three font keys alone; every other
/// key of vtium.conf waits for the next start.
#[test]
fn a_sessions_end_takes_only_the_font_keys_from_vtium_conf() {
    let (dir, fonts) = reset_fixture("reset-only-font", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    assert!(s.app.config().switch(&config::keys::ALLOW_FONT_OPS));
    std::fs::write(
        dir.join("vtium.conf"),
        "font-name=c\nfont-size=0\nno-allow-font-ops\n",
    )
    .expect("the edit");
    let size = end_shown_session(&mut s);
    assert_eq!(size, "size:4 16");
    assert_eq!(s.app.font_name(), "c");
    assert!(
        s.app.config().switch(&config::keys::ALLOW_FONT_OPS),
        "allow-font-ops waits for the next start"
    );
}

/// ADR-1201 D11: the re-read reports what touches the font alone. Lines
/// about other keys, a line with no key, and the count of the reports past
/// the most kept wait for the next start, so the log never says "more
/// reports" with nothing before it.
#[test]
fn a_sessions_end_reports_only_what_touches_the_font() {
    let (dir, fonts) = reset_fixture("reset-quiet", "font-name=a\nfont-size=0\n");
    std::fs::write(dir.join("switch.1"), "b").expect("the switch");
    let (mut s, _) = reset_seat(&dir, &small_consoles(1), fonts);
    assert!(s.until(BOUND, |a| a.font_name() == "b"), "switched to b");
    let noise = "sb-size=abc\n".repeat(70);
    std::fs::write(
        dir.join("vtium.conf"),
        format!("font-name=a\nfont-size=big\n=x\n{noise}"),
    )
    .expect("the edit");
    logged();
    end_shown_session(&mut s);
    let lines = logged();
    let conf = dir.join("vtium.conf");
    let reports: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with(&conf.display().to_string()) || l.contains("more reports"))
        .collect();
    assert_eq!(
        reports,
        [&format!(
            "{}:2: font-size=\"big\" is not a whole number, ignored",
            conf.display()
        )],
        "{lines:?}"
    );
}

/// ADR-1303 D9, D10: a display the GPU path cannot set up at its size holds
/// its whole head on the CPU path while the display keeps that size, with
/// one line naming the display and each of the head's display lines again
/// naming `cpu`, and its set-up is not tried at each frame. A later size
/// that the GPU path sets up, or the display leaving the head, puts the
/// head back on the GPU path, with each display line again naming `gpu`.
#[cfg(feature = "gpu")]
mod gpu_path {
    use std::sync::atomic::Ordering::SeqCst;

    use vtium::painter::{GpuSource, SetUpFault, TestGpu};

    use super::*;

    /// Two displays of one head on the test's GPU, both drawn on the GPU
    /// path, and the lines logged since.
    fn two_on_the_gpu() -> (Seat, Arc<TestGpu>) {
        logged();
        let gpu = Arc::new(TestGpu::default());
        let source = GpuSource::Test(Arc::clone(&gpu));
        let displays = [
            ("/dev/mem0", Memory::new(320, 160, "MEM-1", "card-a")),
            ("/dev/mem1", Memory::new(320, 160, "MEM-2", "card-b")),
        ];
        let mut s = Seat::set_up(
            &["--hwaccel"],
            None,
            displays,
            MODE_SCRIPT,
            no_fonts(),
            |app| app.set_gpu_source(source),
        );
        assert!(s.until(BOUND, |a| shows(a, "ready")), "the session runs");
        assert!(
            s.until(BOUND, |_| gpu.kept.load(SeqCst) == 2),
            "both displays on the GPU path"
        );
        assert_eq!(renderers(&logged()), ["gpu", "gpu"]);
        (s, gpu)
    }

    /// The paths each display line names, in order.
    fn renderers(lines: &[String]) -> Vec<&str> {
        lines
            .iter()
            .filter(|l| l.starts_with("Display "))
            .filter_map(|l| l.split("text renderer [").nth(1)?.split(']').next())
            .collect()
    }

    /// The lines logged since the last call, once `done` holds of them,
    /// with the loop turning meanwhile.
    fn lines_until(s: &mut Seat, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let mut lines = Vec::new();
        let end = now() + BOUND;
        while !done(&lines) && now() < end {
            s.until(Duration::from_millis(20), |_| false);
            lines.extend(logged());
        }
        lines
    }

    /// Types a line on the session and waits until both displays drew it.
    fn draw_both(s: &mut Seat, text: &str) {
        let frames = (s.memory("/dev/mem0").frames, s.memory("/dev/mem1").frames);
        s.type_str(&format!("{text}\r"));
        assert!(s.until(BOUND, |a| got(a).is_some_and(|g| g.0 == text)));
        assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames.0));
        assert!(s.until_memory("/dev/mem1", BOUND, |m| m.frames > frames.1));
    }

    /// How many of `lines` hold `text`.
    fn count_of(lines: &[String], text: &str) -> usize {
        lines.iter().filter(|l| l.contains(text)).count()
    }

    /// The second display's key, as the GPU path's set-ups record it.
    fn second() -> (PathBuf, vtium_platform::display::DisplayId) {
        (PathBuf::from("/dev/mem1"), ID)
    }

    #[test]
    fn a_display_the_gpu_cannot_set_up_holds_its_head_on_the_cpu_path_while_its_size_lasts() {
        let (mut s, gpu) = two_on_the_gpu();
        let before = gpu.set_ups.lock().unwrap().len();
        *gpu.widest.lock().unwrap() = Some(400);
        s.memory("/dev/mem1").set_mode(640, 320);
        let lines = lines_until(&mut s, |l| renderers(l).len() == 2);
        let refused: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("the GPU path cannot set it up"))
            .collect();
        assert_eq!(refused.len(), 1, "{lines:#?}");
        assert!(
            refused[0].starts_with("display MEM-2: the GPU path cannot set it up at 640x320"),
            "{lines:#?}"
        );
        assert_eq!(renderers(&lines), ["cpu", "cpu"], "{lines:#?}");
        // the head's frames are the CPU path's, and the set-up is not tried
        // again at that size
        let (drawn, tries) = (gpu.drawn.load(SeqCst), gpu.set_ups.lock().unwrap().len());
        draw_both(&mut s, "a");
        draw_both(&mut s, "b");
        assert_eq!(gpu.drawn.load(SeqCst), drawn, "a frame on the GPU path");
        assert_eq!(gpu.set_ups.lock().unwrap().len(), tries, "set up again");
        assert_eq!(renderers(&logged()), Vec::<&str>::new());
        // a size the GPU path sets up puts the head back on it
        s.memory("/dev/mem1").set_mode(320, 160);
        let lines = lines_until(&mut s, |l| renderers(l).len() == 2);
        assert_eq!(renderers(&lines), ["gpu", "gpu"], "{lines:#?}");
        assert!(
            lines.iter().any(|l| l.contains("it draws on the GPU path")),
            "{lines:#?}"
        );
        draw_both(&mut s, "c");
        assert!(gpu.drawn.load(SeqCst) > drawn, "the head on the GPU path");
        // only the display that held the head was set up, refused and then
        // at its new size: the other kept its pipeline throughout
        let tries = gpu.set_ups.lock().unwrap()[before..].to_vec();
        assert_eq!(tries, [second(), second()]);
        assert_eq!(gpu.kept.load(SeqCst), 2, "both displays kept");
    }

    // A refused display whose next size meets out of memory at its set-up
    // ends the head's GPU path, and nothing says the head draws on it again
    #[test]
    fn a_refused_display_whose_next_set_up_loses_the_gpu_does_not_bring_its_head_back() {
        let (mut s, gpu) = two_on_the_gpu();
        gpu.set_up_faults
            .lock()
            .unwrap()
            .extend([SetUpFault::Limit, SetUpFault::OutOfMemory]);
        s.memory("/dev/mem1").set_mode(640, 320);
        let _ = lines_until(&mut s, |l| renderers(l).len() == 2);
        s.memory("/dev/mem1").set_mode(400, 200);
        let mut lines = lines_until(&mut s, |l| renderers(l).len() == 2);
        draw_both(&mut s, "a");
        lines.extend(logged());
        assert_eq!(count_of(&lines, "the GPU path failed"), 1, "{lines:#?}");
        assert_eq!(
            count_of(&lines, "it draws on the GPU path"),
            0,
            "{lines:#?}"
        );
        assert_eq!(renderers(&lines), ["cpu", "cpu"], "{lines:#?}");
    }

    // A display refused at one size and again at its next keeps its head on
    // the CPU path: nothing says the head is back, and nothing is drawn on
    // the GPU path
    #[test]
    fn a_display_refused_again_at_its_next_size_keeps_its_head_on_the_cpu_path() {
        let (mut s, gpu) = two_on_the_gpu();
        gpu.set_up_faults
            .lock()
            .unwrap()
            .extend([SetUpFault::Limit, SetUpFault::Compile]);
        s.memory("/dev/mem1").set_mode(640, 320);
        let _ = lines_until(&mut s, |l| renderers(l).len() == 2);
        let drawn = gpu.drawn.load(SeqCst);
        s.memory("/dev/mem1").set_mode(400, 200);
        let mut lines = lines_until(&mut s, |l| count_of(l, "cannot set it up") == 1);
        draw_both(&mut s, "a");
        draw_both(&mut s, "b");
        lines.extend(logged());
        assert_eq!(
            count_of(&lines, "it draws on the GPU path"),
            0,
            "{lines:#?}"
        );
        assert!(!renderers(&lines).contains(&"gpu"), "{lines:#?}");
        assert_eq!(gpu.drawn.load(SeqCst), drawn, "a frame on the GPU path");
    }

    // ADR-1303 D9: a head goes back to the GPU path three times in a run
    // after a display held it on the CPU path, as a display that keeps
    // re-plugging at a size the GPU cannot set up would make it; the fourth
    // hold keeps it on the CPU path until vtium restarts, which the log
    // says once
    #[test]
    fn a_head_held_on_the_cpu_path_a_fourth_time_stays_there_until_vtium_restarts() {
        let (mut s, gpu) = two_on_the_gpu();
        *gpu.widest.lock().unwrap() = Some(400);
        let mut lines = Vec::new();
        for _ in 0..3 {
            s.memory("/dev/mem1").set_mode(640, 320);
            lines.extend(lines_until(&mut s, |l| renderers(l).len() == 2));
            s.memory("/dev/mem1").set_mode(320, 160);
            lines.extend(lines_until(&mut s, |l| renderers(l).len() == 2));
        }
        assert_eq!(count_of(&lines, "cannot set it up"), 3, "{lines:#?}");
        assert_eq!(
            count_of(&lines, "it draws on the GPU path"),
            3,
            "{lines:#?}"
        );
        s.memory("/dev/mem1").set_mode(640, 320);
        let mut lines = lines_until(&mut s, |l| count_of(l, "the GPU path failed") == 1);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("refused it again after 3 trips back to it")),
            "{lines:#?}"
        );
        let drawn = gpu.drawn.load(SeqCst);
        s.memory("/dev/mem1").set_mode(320, 160);
        draw_both(&mut s, "a");
        draw_both(&mut s, "b");
        lines.extend(logged());
        assert_eq!(count_of(&lines, "the GPU path failed"), 1, "{lines:#?}");
        assert_eq!(count_of(&lines, "cannot set it up"), 0, "{lines:#?}");
        assert_eq!(
            count_of(&lines, "it draws on the GPU path"),
            0,
            "{lines:#?}"
        );
        assert!(!renderers(&lines).contains(&"gpu"), "{lines:#?}");
        assert_eq!(gpu.drawn.load(SeqCst), drawn, "a frame on the GPU path");
    }

    #[test]
    fn a_head_held_on_the_cpu_path_goes_back_to_the_gpu_when_the_display_leaves() {
        let (mut s, gpu) = two_on_the_gpu();
        gpu.set_up_faults
            .lock()
            .unwrap()
            .push_back(SetUpFault::Compile);
        s.memory("/dev/mem1").set_mode(640, 320);
        let lines = lines_until(&mut s, |l| renderers(l).len() == 2);
        assert_eq!(renderers(&lines), ["cpu", "cpu"], "{lines:#?}");
        let drawn = gpu.drawn.load(SeqCst);
        s.unplug("/dev/mem1", now());
        let frames = s.memory("/dev/mem0").frames;
        s.type_str("a\r");
        assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames));
        let lines = lines_until(&mut s, |l| !renderers(l).is_empty());
        assert_eq!(renderers(&lines), ["gpu"], "{lines:#?}");
        assert!(
            s.until(BOUND, |_| gpu.drawn.load(SeqCst) > drawn),
            "the head on the GPU path"
        );
    }
}

/// The seat of the vtium on VT `vt`, booted on vtium.conf `text` with
/// `displays`, each head's session echoing each line it reads, with a
/// keyboard at each `(node, path)` of `keyboards`.
fn on_vt(
    vt: u16,
    text: &str,
    displays: impl IntoIterator<Item = (&'static str, Memory)>,
    keyboards: &[(&str, &str)],
) -> Seat {
    on_vt_with(&[], vt, text, displays, keyboards)
}

/// [`on_vt`], with the command line `args`.
fn on_vt_with(
    args: &[&str],
    vt: u16,
    text: &str,
    displays: impl IntoIterator<Item = (&'static str, Memory)>,
    keyboards: &[(&str, &str)],
) -> Seat {
    let rules = start(text);
    assert_eq!(rules.line(), None, "a valid vtium.conf");
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_rules(
        args,
        Some(rules.on_vt(Some(vt))),
        displays,
        script,
        no_fonts(),
    );
    for (node, path) in keyboards {
        s.app.add_input(Path::new(node), keyboard(path));
    }
    s
}

/// The names of the heads the seat built, in the file's order.
fn head_names(app: &App) -> Vec<&str> {
    app.heads().iter().map(Head::name).collect()
}

/// Three heads, each on a VT of its own, that hold the same display and
/// the same keyboard.
const THREE_VTS: &str = "version = 1
[head a]
vt = 3
[head b]
vt = 4
[head c]
vt = 5
[rules]
display card:mem-one -> a
display card:mem-one -> b
display card:mem-one -> c
input path:kbd-one -> a
input path:kbd-one -> b
input path:kbd-one -> c
";

/// ADR-1403 D7, path tests 1 and 8 (ADR-1401 D2, D3 steps 3 and 2): three
/// heads on VTs 3, 4 and 5 hold one display and one keyboard. The vtium
/// on each VT builds that VT's head alone, which takes the display, and
/// the keyboard types into it.
// covers: platform.heads
#[test]
fn the_vtium_on_each_vt_builds_its_own_head_on_a_shared_display_and_keyboard() {
    for (vt, here) in [(3, "a"), (4, "b"), (5, "c")] {
        let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-one").lit())];
        let mut s = on_vt(vt, THREE_VTS, displays, &[(KBD_A, "kbd-one")]);
        assert_eq!(head_names(&s.app), [here], "the heads built on VT {vt}");
        assert!(
            s.until(BOUND, |a| head_shows(a, here, "ready")),
            "head {here} takes the display on VT {vt}"
        );
        s.type_on(KBD_A, "k\r");
        assert!(
            s.until(BOUND, |a| head_shows(a, here, "got:k")),
            "the keyboard types into head {here} on VT {vt}"
        );
    }
}

/// Two displays and two keyboards, each with a head of its own on VT 3
/// and another on VT 4.
const TWO_BY_TWO: &str = "version = 1
[head small3]
vt = 3
[head large3]
vt = 3
[head small4]
vt = 4
[head large4]
vt = 4
[rules]
display card:mem-small -> small3
display card:mem-large -> large3
display card:mem-small -> small4
display card:mem-large -> large4
input path:kbd-small -> small3
input path:kbd-large -> large3
input path:kbd-small -> small4
input path:kbd-large -> large4
";

/// ADR-1403 D7, path tests 2 and 4 (ADR-1401 D3 steps 1, 3 and 2): each
/// display and each keyboard has a head on VT 3 and another on VT 4. On
/// each VT both displays show that VT's heads, and each keyboard types
/// into its own head of that VT.
// covers: platform.heads
#[test]
fn on_each_vt_each_display_and_keyboard_reaches_its_own_head_of_that_vt() {
    for vt in [3, 4] {
        let (small, large) = (format!("small{vt}"), format!("large{vt}"));
        let displays = [
            ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
            (
                "/dev/mem1",
                Memory::new(640, 320, "MEM-2", "mem-large").lit(),
            ),
        ];
        let keyboards = [(KBD_SMALL, "kbd-small"), (KBD_LARGE, "kbd-large")];
        let mut s = on_vt(vt, TWO_BY_TWO, displays, &keyboards);
        assert_eq!(head_names(&s.app), [small.as_str(), large.as_str()]);
        assert!(s.until(BOUND, |a| head_shows(a, &small, "ready")
            && head_shows(a, &large, "ready")));
        s.type_on(KBD_SMALL, "s\r");
        s.type_on(KBD_LARGE, "l\r");
        assert!(
            s.until(BOUND, |a| head_shows(a, &small, "got:s")
                && head_shows(a, &large, "got:l")),
            "each keyboard types into its own head on VT {vt}"
        );
        assert!(!head_shows(&s.app, &small, "got:l"));
        assert!(!head_shows(&s.app, &large, "got:s"));
    }
}

/// ADR-1403 D7, path test 3 (ADR-1401 D2, D3 step 2): a head with no `vt`
/// runs on every VT, so the vtium on any VT builds it and both displays
/// show it, as before heads had a `vt`.
// covers: platform.heads
#[test]
fn a_head_with_no_vt_is_built_on_every_vt() {
    let text = "version = 1\n[head every]\n[rules]\ndisplay * -> every\n";
    for vt in [2, 7] {
        let mut s = on_vt(vt, text, two_displays(), &[]);
        assert_eq!(head_names(&s.app), ["every"]);
        assert!(
            s.until(BOUND, |a| a.cells().len() == 2),
            "both displays on VT {vt}"
        );
        assert!(s.until(BOUND, |a| head_shows(a, "every", "ready")));
    }
}

/// A keyboard whose only head runs on VT 3, a keyboard of a head on VT 4,
/// and a broad rule after both.
const TIED_TO_VT3: &str = "version = 1
[head desk]
vt = 3
[head here]
vt = 4
[rules]
display * -> here
input path:kbd-desk -> desk
input path:kbd-here -> here
input class:keyboard -> roaming
";

/// ADR-1403 D7, path tests 5 and 7 (ADR-1401 D3, the exception in step 2
/// and step 4): on VT 4, a keyboard whose only head runs on VT 3 types
/// nowhere, and the broad rule after it leaves it alone. Under the open
/// policy, vtium still watches it for the rescue key, which makes it type
/// into the active head.
// covers: platform.heads.rules
#[test]
fn a_keyboard_tied_to_another_vt_types_nowhere_until_the_rescue_key() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-one").lit())];
    let keyboards = [(KBD_A, "kbd-desk"), (KBD_B, "kbd-here")];
    let mut s = on_vt(4, TIED_TO_VT3, displays, &keyboards);
    assert_eq!(head_names(&s.app), ["here"]);
    assert!(s.until(BOUND, |a| head_shows(a, "here", "ready")));
    s.type_on(KBD_A, "d\r");
    s.type_on(KBD_B, "h\r");
    assert!(s.until(BOUND, |a| head_shows(a, "here", "got:h")));
    s.until(Duration::from_millis(100), |_| false);
    assert!(
        !head_shows(&s.app, "here", "got:d"),
        "the keyboard of VT 3's head types nowhere on VT 4"
    );
    s.app.device_input(Path::new(KBD_A), ctrl_alt_f(4), now());
    s.type_on(KBD_A, "r\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "here", "got:r")),
        "the rescue key makes it type into the active head"
    );
}

/// ADR-1403 D7, path test 6, under the maintainer's policy table
/// (ADR-1401 D3): on VT 4, under open, a display whose only head runs on
/// VT 3 shows a notice of its own that names that head, and no login
/// shows on it. The head of VT 4 has no display.
// covers: platform.heads.guards
#[test]
fn a_display_tied_to_another_vt_shows_its_notice_and_no_login() {
    let text = "version = 1
[head desk]
vt = 3
[head here]
vt = 4
[rules]
display card:mem-one -> desk
";
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-one").lit())];
    let mut s = on_vt(4, text, displays, &[]);
    assert_eq!(head_names(&s.app), ["here"]);
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0 && m.has(PAPER)));
    assert_eq!(
        s.app.display_notice("MEM-1"),
        Some((
            "MEM-1 is not configured for VT 4. It is configured for terminal desk, on VT 3.",
            true
        ))
    );
    assert!(
        s.app
            .head("here")
            .is_some_and(|h| h.cells().is_empty() && !h.started()),
        "the head of VT 4 has no display and starts no login"
    );
}

/// Two heads that both run on VT 3 alone, under the policy `policy`.
fn every_head_on_vt3(policy: &str) -> String {
    format!(
        "version = 1
policy = {policy}
[head desk]
vt = 3
[head panel]
vt = 3
[rules]
display card:mem-large -> desk
display card:mem-small -> panel
input path:kbd-desk -> desk
"
    )
}

/// The maintainer's policy table (ADR-1401 D3): under open, a file that
/// defines nothing runs the built-in default terminal on every display,
/// and every keyboard types into it.
// covers: platform.heads
#[test]
fn under_open_with_nothing_defined_every_display_shows_the_default_terminal() {
    let mut s = on_vt(4, "version = 1\n", two_displays(), &[(KBD_A, "kbd-any")]);
    assert_eq!(head_names(&s.app), ["console"]);
    assert!(s.until(BOUND, |a| a.cells().len() == 2), "both displays");
    assert!(s.until(BOUND, |a| head_shows(a, "console", "ready")));
    s.type_on(KBD_A, "k\r");
    assert!(s.until(BOUND, |a| head_shows(a, "console", "got:k")));
}

/// The maintainer's policy table (ADR-1401 D3): under open, with paths
/// defined, a VT where no head runs builds none, and each display shows
/// its own notice that names the head it is configured for.
// covers: platform.heads
#[test]
fn under_open_with_paths_on_other_vts_each_display_shows_its_own_notice() {
    let mut s = on_vt(4, &every_head_on_vt3("open"), two_displays(), &[]);
    assert_eq!(head_names(&s.app), Vec::<&str>::new());
    assert!(s.until(
        BOUND,
        |a| a.display_notice("MEM-1").is_some_and(|(_, on)| on)
            && a.display_notice("MEM-2").is_some_and(|(_, on)| on)
    ));
    assert_eq!(
        s.app.display_notice("MEM-1").map(|(t, _)| t),
        Some("MEM-1 is not configured for VT 4. It is configured for terminal panel, on VT 3.")
    );
    assert_eq!(
        s.app.display_notice("MEM-2").map(|(t, _)| t),
        Some("MEM-2 is not configured for VT 4. It is configured for terminal desk, on VT 3.")
    );
    for node in ["/dev/mem0", "/dev/mem1"] {
        assert!(
            s.until_memory(node, BOUND, |m| m.frames > 0),
            "{node} draws"
        );
    }
}

/// The maintainer's policy table (ADR-1401 D3): under open, a display no
/// rule matches shows that it is configured for no terminal, and one a
/// rule sends to `off` stays blank.
// covers: platform.heads.rules
#[test]
fn under_open_a_display_no_rule_matches_says_so_and_one_sent_off_stays_blank() {
    let text = "version = 1
[head desk]
[rules]
display card:mem-small -> off
input * -> ignore
";
    let mut s = on_vt(4, text, two_displays(), &[]);
    assert!(s.until(BOUND, |a| {
        a.display_notice("MEM-2").is_some_and(|(_, on)| on)
    }));
    assert_eq!(
        s.app.display_notice("MEM-2").map(|(t, _)| t),
        Some("MEM-2 is not configured for any terminal.")
    );
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.display_notice("MEM-1"), None);
    let m = s.memory("/dev/mem0");
    assert_eq!((m.dpms, m.frames), (Some(Dpms::Off), 0), "off stays blank");
}

/// The maintainer's policy table (ADR-1401 D3): under closed, a display
/// no path names stays blank, with paths defined or none, and no notice
/// shows on any display.
// covers: platform.heads.policy
#[test]
fn under_closed_a_display_no_path_names_stays_blank_and_no_notice_shows() {
    for text in [
        every_head_on_vt3("closed"),
        "version = 1\npolicy = closed\n".to_owned(),
    ] {
        let displays = [
            ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
            (
                "/dev/mem1",
                Memory::new(640, 320, "MEM-2", "mem-large").lit(),
            ),
        ];
        let mut s = on_vt(4, &text, displays, &[(KBD_A, "kbd-desk")]);
        assert_eq!(head_names(&s.app), Vec::<&str>::new(), "{text}");
        s.until(Duration::from_millis(200), |_| false);
        assert_eq!(s.app.display_notice("MEM-1"), None);
        assert_eq!(s.app.display_notice("MEM-2"), None);
        assert!(
            s.app.notice_screen().is_none_or(|n| n.cells().is_empty()),
            "the closed notice screen lights no display"
        );
        for node in ["/dev/mem0", "/dev/mem1"] {
            let m = s.memory(node);
            assert_eq!(
                (m.dpms, m.frames),
                (Some(Dpms::Off), 0),
                "{node} stays blank"
            );
        }
    }
}

/// The maintainer's decision of 2026-11-10 (ADR-1401 D3): a notice shows
/// for `osd-timeout` from its first frame. Then vtium parks its display,
/// which sets it off and frees its buffer. A key on this VT shows the
/// notice again. The clock the turn reads stands in for the time.
#[test]
fn a_notice_shows_for_osd_timeout_then_its_display_is_parked_until_a_key() {
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = on_vt(
        4,
        &every_head_on_vt3("open"),
        displays,
        &[(KBD_A, "kbd-any")],
    );
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    assert_eq!(s.memory("/dev/mem0").parks, 0);
    // osd-timeout is 3 s when unset
    s.app.turn(now() + Duration::from_secs(4));
    s.until(Duration::from_millis(50), |_| false);
    let m = s.memory("/dev/mem0");
    assert_eq!(m.parks, 1, "parked once its time is up");
    assert!(m.pixels.is_empty(), "its buffer is freed");
    assert_eq!(m.dpms, Some(Dpms::Off));
    let frames = m.frames;
    assert_eq!(s.app.display_notice("MEM-1").map(|(_, on)| on), Some(false));
    s.type_on(KBD_A, "x");
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "a key shows the notice again"
    );
    assert_eq!(s.app.display_notice("MEM-1").map(|(_, on)| on), Some(true));
    assert_eq!(s.memory("/dev/mem0").parks, 1, "parked once");
}

/// A seat whose only display shows a notice, then is parked, does not
/// end for want of a display: the notice's frame ended the start's
/// `display-wait`.
#[test]
fn a_parked_notice_display_does_not_end_vtium_at_display_wait() {
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = on_vt_with(
        &["--display-wait=1"],
        4,
        &every_head_on_vt3("open"),
        displays,
        &[],
    );
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    s.app.turn(now() + Duration::from_secs(4));
    s.until(Duration::from_millis(50), |_| false);
    assert_eq!(s.memory("/dev/mem0").parks, 1);
    s.app.turn(now() + Duration::from_secs(30));
    assert_eq!(s.app.finished(), None, "vtium runs on");
}

/// The maintainer's decision of 2026-11-10: `osd = off` does not hide a
/// display's notice, as it hides no notice of a problem.
#[test]
fn osd_off_still_shows_an_unused_displays_notice() {
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let mut s = on_vt_with(&["--osd=off"], 4, &every_head_on_vt3("open"), displays, &[]);
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0 && m.has(PAPER)));
    assert_eq!(s.app.display_notice("MEM-1").map(|(_, on)| on), Some(true));
}

/// Two heads on VT 3, one of them isolated, and a keyboard for each kind
/// of rule, under the policy `policy`.
fn vt_keys_on_vt3(policy: &str) -> String {
    format!(
        "version = 1
policy = {policy}
[head desk]
vt = 3
isolated = no
[head kiosk]
vt = 3
isolated = yes
[rules]
display * -> desk
input path:kbd-desk -> desk
input path:kbd-kiosk -> kiosk
input path:kbd-roam -> roaming
input path:kbd-ignored -> ignore
"
    )
}

/// Whether the VT key pressed on a keyboard at USB port `path` switches
/// the VT away from VT 4, where no head of `text` runs.
fn switches_from_vt4(text: &str, path: &str) -> bool {
    let mut s = on_vt(4, text, two_displays(), &[(KBD_A, path)]);
    assert!(s.app.vt.is_active(), "VT 4 is shown");
    s.app.device_input(Path::new(KBD_A), vt_key(), now());
    !s.app.vt.is_active()
}

/// The maintainer's decision of 2026-11-10 (ADR-1401 D3, "Who switches
/// VTs"): under open, every keyboard switches VTs on a VT where no head
/// runs, whatever its rule ties it to, but a keyboard of an isolated
/// head does not.
// covers: platform.heads.policy
#[test]
fn under_open_every_keyboard_switches_vts_on_a_vt_with_no_head() {
    let text = vt_keys_on_vt3("open");
    for (path, switches) in [
        ("kbd-desk", true),
        ("kbd-roam", true),
        ("kbd-ignored", true),
        ("kbd-other", true),
        ("kbd-kiosk", false),
    ] {
        assert_eq!(switches_from_vt4(&text, path), switches, "{path}");
    }
}

/// The maintainer's decision of 2026-11-10 (ADR-1401 D3, "Who switches
/// VTs"): under closed, on a VT where no head runs, a keyboard that a
/// path places switches VTs, and one that no path catches does not.
// covers: platform.heads.policy
#[test]
fn under_closed_only_a_keyboard_a_path_places_switches_vts() {
    let text = vt_keys_on_vt3("closed");
    for (path, switches) in [
        ("kbd-desk", true),
        ("kbd-roam", true),
        ("kbd-ignored", false),
        ("kbd-other", false),
        ("kbd-kiosk", false),
    ] {
        assert_eq!(switches_from_vt4(&text, path), switches, "{path}");
    }
}

/// The seat of VT 4 under [`every_head_on_vt3`]'s open file, with the
/// command line `args`, whose one display shows a notice, once that
/// notice has drawn.
fn showing_a_notice(args: &[&str]) -> Seat {
    let displays = [(
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    )];
    let text = every_head_on_vt3("open");
    let mut s = on_vt_with(args, 4, &text, displays, &[(KBD_A, "kbd-any")]);
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    assert_eq!(s.app.display_notice("MEM-1").map(|(_, on)| on), Some(true));
    s
}

impl Seat {
    /// The VT switches away, in a turn at `at`, and vtium lets it go.
    fn switch_away(&mut self, at: Instant) {
        let released = self.released.get();
        self.app.vt.leave().expect("the fake VT");
        self.app.turn(at);
        let count = Rc::clone(&self.released);
        assert!(
            self.until(BOUND, |_| count.get() > released),
            "vtium lets the VT go"
        );
    }

    /// The VT switches back, in a turn at `at`.
    fn switch_back(&mut self, at: Instant) {
        self.app.vt.activate().expect("the fake VT");
        self.app.turn(at);
    }
}

/// A notice's screen hides its cursor, so it has no timer of its own: the
/// loop wakes for the notice's time (ADR-1401 D3), not for the next
/// timer far later.
#[test]
fn the_loop_wakes_when_a_notices_time_is_up() {
    let mut s = showing_a_notice(&[]);
    // the turn that drew the notice may leave the guards owed, which wake
    // the loop at once; the first wait past them is the notice's
    assert!(s.until(BOUND, |a| a.next_wake(now()) != Some(Duration::ZERO)));
    let wake = s.app.next_wake(now());
    assert!(
        wake.is_some_and(|w| w <= Duration::from_secs(3)),
        "osd-timeout is 3 s when unset: {wake:?}"
    );
}

/// ADR-1401 D3: a switch back to this VT shows a parked notice again.
#[test]
fn a_parked_notice_shows_again_when_the_vt_is_switched_back() {
    let mut s = showing_a_notice(&[]);
    let start = now();
    s.app.turn(start + Duration::from_secs(4));
    assert_eq!(s.memory("/dev/mem0").parks, 1, "parked once its time is up");
    let frames = s.memory("/dev/mem0").frames;
    s.switch_away(start + Duration::from_secs(5));
    s.switch_back(start + Duration::from_secs(6));
    assert!(
        s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames),
        "the switch back shows the notice again"
    );
    assert_eq!(s.app.display_notice("MEM-1").map(|(_, on)| on), Some(true));
}

/// ADR-1401 D3: a switch back to this VT starts a notice's full
/// `osd-timeout` again, though its time ran out while the VT was away.
#[test]
fn a_switch_back_starts_a_showing_notices_time_again() {
    let mut s = showing_a_notice(&[]);
    let start = now();
    s.switch_away(start + Duration::from_secs(1));
    s.switch_back(start + Duration::from_secs(10));
    s.app.turn(start + Duration::from_secs(10));
    assert_eq!(
        s.memory("/dev/mem0").parks,
        0,
        "the first draw after the switch back shows the notice"
    );
    s.app.turn(start + Duration::from_secs(12));
    assert_eq!(s.memory("/dev/mem0").parks, 0, "within its 3 s");
    s.app.turn(start + Duration::from_secs(14));
    assert_eq!(s.memory("/dev/mem0").parks, 1, "parked after its 3 s");
}

/// ADR-1401 D3: a key on this VT starts a showing notice's full
/// `osd-timeout` again.
#[test]
fn a_key_starts_a_showing_notices_time_again() {
    let mut s = showing_a_notice(&[]);
    let start = now();
    s.app.turn(start + Duration::from_secs(2));
    s.app.device_input(
        Path::new(KBD_A),
        InputEvent::Key(char_key('x')),
        start + Duration::from_secs(2),
    );
    s.app.turn(start + Duration::from_secs(2));
    s.app.turn(start + Duration::from_secs(4));
    assert_eq!(
        s.memory("/dev/mem0").parks,
        0,
        "the key started the time again"
    );
    s.app.turn(start + Duration::from_secs(6));
    assert_eq!(s.memory("/dev/mem0").parks, 1);
}

/// ADR-1401 D3: with a terminal's display and a notice's display on one
/// VT, the notice's time puts away the notice's display alone.
#[test]
fn the_notice_cycle_parks_the_notices_display_alone() {
    let text = "version = 1
[head desk]
vt = 3
[head here]
vt = 4
[rules]
display card:mem-small -> desk
display card:mem-large -> here
";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = on_vt(4, text, displays, &[]);
    assert!(s.until(BOUND, |a| head_shows(a, "here", "ready")));
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > 0));
    s.app.turn(now() + Duration::from_secs(4));
    assert_eq!(s.memory("/dev/mem0").parks, 1, "the notice's display parks");
    assert_eq!(s.memory("/dev/mem1").parks, 0, "the terminal's does not");
    assert!(
        s.app.head("here").is_some_and(|h| !h.cells().is_empty()),
        "head here keeps its display"
    );
}

/// ADR-1401 D3: a display unplugged and plugged in again shows its notice
/// again on the screen it had, so toggling it adds no screen.
#[test]
fn a_notice_display_unplugged_and_plugged_again_adds_no_screen() {
    let mut s = showing_a_notice(&[]);
    let screens = s.app.screen_count();
    for _ in 0..3 {
        s.unplug("/dev/mem0", now());
        assert!(s.until(BOUND, |a| a.display_notice("MEM-1").is_none()));
        s.plug("/dev/mem0", now());
        assert!(s.until(BOUND, |a| {
            a.display_notice("MEM-1").is_some_and(|(_, on)| on)
        }));
    }
    assert_eq!(s.app.screen_count(), screens);
}

/// ADR-1401 D5: a notice's screen is not a terminal, so under
/// `osd = full` it shows no OSD when its display comes back.
#[test]
fn a_notice_shown_again_shows_no_osd() {
    let mut s = showing_a_notice(&["--osd=full"]);
    s.app.turn(now() + Duration::from_secs(4));
    assert_eq!(s.memory("/dev/mem0").parks, 1);
    s.type_on(KBD_A, "x");
    assert!(s.until(BOUND, |a| {
        a.display_notice("MEM-1").is_some_and(|(_, on)| on)
    }));
    let frames = s.memory("/dev/mem0").frames;
    assert!(s.until_memory("/dev/mem0", BOUND, |m| m.frames > frames));
    let head = s.app.notice_head("MEM-1").expect("the notice's screen");
    assert!(!head.shows_osd(), "the notice's screen shows no OSD");
    assert_eq!((head.status(), head.mark()), (None, None));
}

/// Under `oneshot` a session that ends gets no hard reset, since its console
/// never starts another: a hidden console's XTWINOPS 8 request still goes
/// with the session, so showing the ended console leaves the grid as the
/// display gives it.
#[test]
fn an_ended_hidden_consoles_grid_request_goes_with_its_session() {
    let (dir, fonts) = reset_fixture(
        "window-oneshot",
        "font-size=16\nallow-window-ops\noneshot\n",
    );
    std::fs::write(dir.join("ask.2"), "").expect("the ask");
    let script = format!(
        "d={d}; n=$VTIUM_CONSOLE; echo x >> $d/starts.$n; echo \"size:$(stty size)\"; \
         if [ -e $d/ask.$n ]; then rm $d/ask.$n; printf '\\033[8;4;15t'; \
         while [ ! -e $d/go.$n ]; do sleep 0.02; done; rm $d/go.$n; exit 0; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    );
    logged();
    let (mut s, _) = seat_running(&dir, &small_consoles(2), fonts, &script);
    assert!(s.until(BOUND, |_| starts(&dir, 2) == 1));
    std::fs::write(dir.join("go.2"), "").expect("the go mark");
    let mut lines = Vec::new();
    let ended = |lines: &[String]| {
        lines
            .iter()
            .any(|l| l.contains("console 2: its program ended under oneshot"))
    };
    assert!(
        s.until(BOUND, |_| {
            lines.extend(logged());
            ended(&lines)
        }),
        "console 2's session ends while it is hidden"
    );
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(
        notice(&s.app, "small").as_deref(),
        Some(ENDED_CONSOLE),
        "console 2 shown, its session ended"
    );
    s.app.turn(now());
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 10)),
        "the ended session's request is gone"
    );
}

/// A hidden console's XTWINOPS 8 request goes with its session, as an
/// OSC 50 request does (ADR-1201 D11): the next session neither waits for
/// it nor takes the ended program's grid when it is shown.
#[test]
fn a_hidden_consoles_grid_request_goes_with_its_session() {
    let (dir, fonts) = reset_fixture("review-hidden-grid", "font-size=16\nallow-window-ops\n");
    std::fs::write(dir.join("ask.2"), "").expect("the ask");
    let script = format!(
        "d={d}; n=$VTIUM_CONSOLE; echo x >> $d/starts.$n; echo \"size:$(stty size)\"; \
         if [ -e $d/ask.$n ]; then rm $d/ask.$n; printf '\\033[8;4;15t'; \
         while [ ! -e $d/go.$n ]; do sleep 0.02; done; rm $d/go.$n; exit 0; fi; \
         while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done",
        d = dir.display()
    );
    let (mut s, _) = seat_running(&dir, &small_consoles(2), fonts, &script);
    assert!(s.until(BOUND, |_| starts(&dir, 2) == 1));
    std::fs::write(dir.join("go.2"), "").expect("the go mark");
    assert!(
        s.until(BOUND, |_| starts(&dir, 2) == 2),
        "console 2 started again"
    );
    press_chord(&mut s, KBD_SMALL, RIGHT);
    s.type_on(KBD_SMALL, "x\r");
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "got:x")),
        "console 2's new session runs, shown"
    );
    assert_eq!(
        grid(&s.app, "small"),
        Some((40, 10)),
        "the ended session's request is gone"
    );
}

/// ADR-1401 D3, "Enter on a head's own keyboard": a head on VT 12 holds
/// an external display and a USB keyboard, and the laptop's own display
/// and keyboard go to a head with no `vt`. On VT 2, the external display
/// shows its notice, and Enter on the USB keyboard switches to VT 12 when
/// the head lets it.
mod enter_switch {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use vtium_platform::vt::{Switch, Vt, VtEvent};

    use vtium::app::VtiumVts;
    use vtium::console::Mode;

    use super::*;

    /// `XF86Switch_VT_1`, whose next eleven keysyms name VTs 2 to 12.
    const SWITCH_VT_1: u32 = 0x1008_fe01;

    /// A fake VT that records the VT each VT key asks for, as the kernel
    /// VT would switch to it.
    struct RecordingVt {
        vt: FakeVt,
        switched: Rc<RefCell<Vec<u16>>>,
    }

    impl Vt for RecordingVt {
        fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
            self.vt.events()
        }
        fn acknowledge_release(&mut self) -> io::Result<()> {
            self.vt.acknowledge_release()
        }
        fn refuse_release(&mut self) -> io::Result<()> {
            self.vt.refuse_release()
        }
        fn activate(&mut self) -> io::Result<Switch> {
            self.vt.activate()
        }
        fn leave(&mut self) -> io::Result<Switch> {
            self.vt.leave()
        }
        fn wait(&mut self) -> io::Result<Switch> {
            self.vt.wait()
        }
        fn is_active(&self) -> bool {
            self.vt.is_active()
        }
        /// A VT key for another VT is taken and recorded, as the kernel VT
        /// takes it while it is shown.
        fn key(&mut self, key: &KeyEvent) -> bool {
            let sym = key.keysyms.first().copied().unwrap_or_default();
            let ctrl_alt = key.mods.contains(Mods::CTRL) && key.mods.contains(Mods::ALT);
            let target = if (SWITCH_VT_1..SWITCH_VT_1 + 12).contains(&sym) {
                u16::try_from(sym - SWITCH_VT_1 + 1).ok()
            } else if ctrl_alt && (0xffbe..0xffbe + 12).contains(&sym) {
                u16::try_from(sym - 0xffbe + 1).ok()
            } else {
                None
            };
            match target {
                Some(n) if self.vt.is_active() => {
                    if Some(n) != self.vt.number() {
                        self.switched.borrow_mut().push(n);
                    }
                    true
                }
                Some(_) => false,
                None => self.vt.key(key),
            }
        }
        fn is_own_key(&self, key: &KeyEvent) -> bool {
            self.vt.is_own_key(key)
        }
        fn restore(&mut self) -> io::Result<()> {
            self.vt.restore()
        }
        fn bell(&mut self) {
            self.vt.bell();
        }
        fn number(&self) -> Option<u16> {
            self.vt.number()
        }
    }

    const USB: &str = "/dev/input/event6";
    const LAPTOP: &str = "/dev/input/event7";

    /// A laptop's file: `[head console]` with no `vt`, and
    /// `[head terminal-1]` on `vts`, with `extra` among its keys, under
    /// `policy`.
    fn laptop(policy: &str, vts: &str, extra: &str) -> String {
        format!(
            "version = 1
policy = {policy}
[head console]
[head terminal-1]
vt = {vts}
{extra}
[rules]
display card:mem-ext -> terminal-1
input path:kbd-usb -> terminal-1
display * -> console
input * -> roaming
"
        )
    }

    /// The seat of the vtium on VT 2 booted on `text`, with the laptop's
    /// display and the external one, the USB keyboard and the laptop's,
    /// where vtium runs on `running` or cannot tell; and the VTs the VT
    /// keys asked for.
    fn on_vt2(text: &str, running: Result<VtiumVts, String>) -> (Seat, Rc<RefCell<Vec<u16>>>) {
        let rules = start(text);
        assert_eq!(rules.line(), None, "a valid vtium.conf");
        let switched = Rc::new(RefCell::new(Vec::new()));
        let recorded = Rc::clone(&switched);
        let displays = [
            ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-int").lit()),
            ("/dev/mem1", Memory::new(320, 160, "MEM-2", "mem-ext").lit()),
        ];
        let script = "echo ready; while read a; do echo \"got:$a\"; done";
        let mut s = Seat::set_up(
            &[],
            Some(rules.on_vt(Some(2))),
            displays,
            script,
            no_fonts(),
            move |app| {
                app.vt = Box::new(RecordingVt {
                    vt: FakeVt::with_number(2),
                    switched: recorded,
                });
                app.set_running_vts(Box::new(move || running.clone()));
            },
        );
        s.app.add_input(Path::new(USB), keyboard("kbd-usb"));
        s.app.add_input(Path::new(LAPTOP), keyboard("kbd-laptop"));
        assert!(s.until(BOUND, |a| head_shows(a, "console", "ready")));
        (s, switched)
    }

    /// vtium runs on the VTs of `list`, and is enabled in direct mode.
    fn vts(list: &[u16]) -> Result<VtiumVts, String> {
        Ok(VtiumVts {
            running: list.iter().copied().collect(),
            mode: Mode::Direct,
        })
    }

    const NOTICE: &str =
        "MEM-2 is not configured for VT 2. It is configured for terminal terminal-1, on VT 12.";

    /// The notice of the external display, once it shows.
    fn notice_of(s: &mut Seat) -> String {
        assert!(
            s.until(BOUND, |a| a
                .display_notice("MEM-2")
                .is_some_and(|(_, on)| on)),
            "the external display shows its notice"
        );
        s.app
            .display_notice("MEM-2")
            .map(|(t, _)| t.to_owned())
            .unwrap_or_default()
    }

    // covers: platform.heads.rules
    #[test]
    fn enter_on_the_heads_own_keyboard_switches_to_its_vt_and_no_other_key_does() {
        let (mut s, switched) = on_vt2(
            &laptop("open", "12", "switch-on-enter = yes"),
            vts(&[2, 3, 12]),
        );
        assert_eq!(
            notice_of(&mut s),
            format!("{NOTICE}\nPress Enter on this keyboard to switch to VT 12.")
        );
        // any other key wakes the notice and switches nothing, Ctrl+Enter
        // among them
        s.type_on(USB, "x");
        let ctrl_enter = KeyEvent {
            mods: Mods::CTRL,
            ..char_key('\r')
        };
        s.app
            .device_input(Path::new(USB), InputEvent::Key(ctrl_enter), now());
        s.until(Duration::from_millis(50), |_| false);
        assert_eq!(*switched.borrow(), [] as [u16; 0], "a key other than Enter");
        // Enter on the laptop's keyboard types into the console
        s.type_on(LAPTOP, "l\r");
        assert!(s.until(BOUND, |a| head_shows(a, "console", "got:l")));
        assert_eq!(
            *switched.borrow(),
            [] as [u16; 0],
            "Enter on another keyboard"
        );
        assert!(
            !head_shows(&s.app, "console", "got:x"),
            "the USB keyboard types nowhere"
        );
        // Enter on the USB keyboard switches to VT 12
        s.type_on(USB, "\r");
        assert_eq!(*switched.borrow(), [12]);
    }

    // covers: platform.heads.rules
    #[test]
    fn enter_switches_to_the_first_vt_of_the_head_where_vtium_runs() {
        let (mut s, switched) = on_vt2(
            &laptop("open", "7 12", "switch-on-enter = yes"),
            vts(&[2, 12]),
        );
        assert_eq!(
            notice_of(&mut s),
            "MEM-2 is not configured for VT 2. It is configured for terminal terminal-1, on VTs 7 and 12.\nPress Enter on this keyboard to switch to VT 12."
        );
        s.type_on(USB, "\r");
        assert_eq!(*switched.borrow(), [12], "VT 7 runs no vtium");
    }

    // covers: platform.heads.rules
    #[test]
    fn where_vtium_runs_on_none_of_the_heads_vts_enter_switches_nowhere_and_the_notice_says_what_to_enable()
     {
        let (mut s, switched) = on_vt2(&laptop("open", "12", "switch-on-enter = yes"), vts(&[2]));
        assert_eq!(
            notice_of(&mut s),
            format!(
                "{NOTICE}\nvtium is not running on VT 12. To enable it there, run sudo vtium console enable --direct 12."
            )
        );
        s.type_on(USB, "\r");
        assert_eq!(*switched.borrow(), [] as [u16; 0]);
        // the command names the mode the VTs vtium is enabled on use
        let seat = VtiumVts {
            running: BTreeSet::from([2]),
            mode: Mode::Seat,
        };
        let (mut s, _) = on_vt2(&laptop("open", "12", "switch-on-enter = yes"), Ok(seat));
        assert_eq!(
            notice_of(&mut s),
            format!(
                "{NOTICE}\nvtium is not running on VT 12. To enable it there, run sudo vtium console enable --seat 12."
            )
        );
        // where vtium cannot tell, the notice keeps its text and nothing
        // switches
        let (mut s, switched) = on_vt2(
            &laptop("open", "12", "switch-on-enter = yes"),
            Err("systemctl is missing".to_owned()),
        );
        assert_eq!(notice_of(&mut s), NOTICE);
        s.type_on(USB, "\r");
        assert_eq!(*switched.borrow(), [] as [u16; 0]);
    }

    // covers: platform.heads.rules
    #[test]
    fn with_the_key_off_or_on_an_isolated_head_enter_switches_nothing_and_the_notice_keeps_its_text()
     {
        for extra in [
            "",
            "switch-on-enter = no",
            "switch-on-enter = yes\nisolated = yes",
        ] {
            let (mut s, switched) = on_vt2(&laptop("open", "12", extra), vts(&[2, 12]));
            assert_eq!(notice_of(&mut s), NOTICE, "{extra:?}");
            s.type_on(USB, "\r");
            s.until(Duration::from_millis(50), |_| false);
            assert_eq!(*switched.borrow(), [] as [u16; 0], "{extra:?}");
        }
    }

    // covers: platform.heads.rules
    #[test]
    fn under_the_closed_policy_enter_switches_nothing() {
        let text = laptop("closed", "12", "switch-on-enter = yes\nisolated = no")
            .replace("[head console]", "[head console]\nisolated = no");
        let (mut s, switched) = on_vt2(&text, vts(&[2, 12]));
        s.until(Duration::from_millis(100), |_| false);
        assert_eq!(
            s.app.display_notice("MEM-2"),
            None,
            "closed shows no notice"
        );
        s.type_on(USB, "\r");
        assert_eq!(*switched.borrow(), [] as [u16; 0]);
    }
}
