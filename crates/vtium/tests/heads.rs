//! Heads (ADR-1401 D1, D3): the seat booted with displays in memory, on
//! the fake VT, with a real child on a real pty in each head. With no head
//! file every display mirrors one grid, as before heads; a head file gives
//! each head its own grid at its own displays' density.

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
use vtium::head::{Head, Start};
use vtium::video::Video;
use vtium_config::Class;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Identity, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

#[path = "common/helper.rs"]
mod stand_in;
use stand_in::real_helper;

const ID: DisplayId = DisplayId(1);
const SECOND: Duration = Duration::from_secs(5);

/// How long a wait on a frame, drawn by a head's render thread, may run
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
}

impl Memory {
    /// At 72 DPI, the C reference's fixed density, where a point is a
    /// pixel.
    fn new(w: u32, h: u32, connector: &'static str, card: &'static str) -> Memory {
        Memory {
            w,
            h,
            refresh_mhz: 60_000,
            connector,
            card,
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
        }
    }

    /// This display, measuring `dpi`.
    fn at(self, dpi: f64) -> Memory {
        Memory { dpi, ..self }
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
            edid: None,
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
        self.awake && self.dpms == Some(Dpms::On) && Arc::strong_count(&self.drawn) == 1
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
    /// to, as [`vtium::head::rules`] gives it, on VT 1.
    fn with_rules(
        args: &[&str],
        rules: Option<Start>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
        fonts: Sources,
    ) -> Seat {
        let args: Vec<OsString> = ["--font-size=16"]
            .iter()
            .chain(args)
            .chain(&["--login", "--", "/bin/sh", "-c", script])
            .map(OsString::from)
            .collect();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the seat's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::with_number(1)),
            lp.handle(),
            Rc::new(now),
            env,
        );
        app.set_font_sources(fonts);
        if let Some(start) = rules {
            app.set_heads(start);
        }
        for (node, memory) in displays {
            app.add_video(PathBuf::from(node), Box::new(memory));
        }
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat { lp, app }
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
fn keyboard(path: &str) -> vtium_config::Identity {
    vtium_config::Identity {
        path: Some(path.to_owned()),
        ..vtium_config::Identity::input().with_class(Class::Keyboard)
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
        s.until(SECOND, |a| a.cells().len() == 2),
        "both displays lit"
    );
    // 320x160 holds 40x10 cells of 8x16; 640x320 holds 40x10 of 16x32.
    assert_eq!(s.app.grid(), (40, 10));
    assert_eq!(s.app.cells(), [(8, 16), (16, 32)]);
    s.type_str("a\r");
    assert!(
        s.until(SECOND, |a| shows(a, "one:a")),
        "the first keyboard types"
    );
    s.type_str("b\r");
    assert!(
        s.until(SECOND, |a| shows(a, "two:b")),
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
        s.until(SECOND, |a| a.cells().len() == 2),
        "both displays lit"
    );
    assert_eq!(grid(&s.app, "small"), Some((40, 10)));
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(8, 16)]));
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(8, 16)]));
    assert!(s.until(SECOND, |a| size(a, "small").is_some()
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
    assert!(one.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(one.app.grid(), (40, 10));
    assert_eq!(one.app.cells(), [(8, 16), (16, 32)]);
    let mut two = Seat::with_heads(&[], Some(TWO_HEADS), displays(), "read x");
    assert!(two.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(grid(&two.app, "small"), Some((40, 10)));
    assert_eq!(grid(&two.app, "large"), Some((40, 10)));
    assert_eq!(two.app.cells(), [(8, 16), (16, 32)]);
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
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    s.type_str("k\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "large", "got:k")),
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
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
        s.until(SECOND, |a| head_shows(a, "large", "ready")),
        "its first display starts it"
    );
    assert_eq!(grid(&s.app, "large"), Some((80, 20)));
}

/// The frits `names`, each of one built-in source, in a directory of its
/// own, and a stand-in helper that writes its pid to the directory's
/// `starts` as it starts and answers nothing; the sources and the file.
fn frits(test: &str, names: &[&str]) -> (Sources, PathBuf) {
    let dir = std::env::temp_dir().join(format!("vtium-heads-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the frits' directory");
    for name in names {
        std::fs::write(
            dir.join(format!("{name}.vtfrit")),
            "source=builtin:unifont\n",
        )
        .expect("a frit");
    }
    let starts = dir.join("starts");
    let helper = vtium::helper::Command {
        program: PathBuf::from("/bin/sh"),
        args: ["-c", "echo $$ >> \"$1\"; exec sleep 30", "helper"]
            .into_iter()
            .map(OsString::from)
            .chain([starts.clone().into_os_string()])
            .collect(),
        account: None,
    };
    let fonts = Sources {
        frits: dir,
        helper: Some(helper),
        ..no_fonts()
    };
    (fonts, starts)
}

/// D3 and D6: the more specific setting wins. A head's profile names its
/// frit, over vtium.conf's font; a head with the default profile takes
/// vtium.conf's, the built-in font here. A head has no font key of its
/// own.
#[test]
fn a_heads_profile_names_its_frit_over_vtium_confs_font() {
    let (fonts, starts) = frits("profile", &["t"]);
    let heads = "version = 1
[head small]
[head large]
profile = t
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(&[], Some(heads), two_displays(), "read x", fonts);
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
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
        s.until(SECOND, |_| starts.exists()),
        "the large head's frit started its helper"
    );
    drop(s);
    if let Some(dir) = starts.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// ADR-1200 D17, binary-recheck's finding 4: a bitmap frit asks for its
/// fonts' own cell at an OSC 50 switch. Switched away and back while
/// another head still draws it, a frit whose last ask had no answer in
/// time asks again; one the helper refused keeps the refusal and asks
/// nothing. The helper is the real one, so each query gets one answer:
/// its frit names a kbd font no machine has, which it refuses, and held,
/// it answers nothing until the test opens its go FIFO, which it never
/// does here.
// covers: text.font-change
#[test]
fn an_osc_50_switch_back_asks_again_after_a_timeout_but_not_a_refusal() {
    for (case, held, again) in [("timeout", true, 1), ("refused", false, 0)] {
        let dir = scratch_dir(&format!("osc50-{case}"));
        std::fs::write(dir.join("t.vtfrit"), "source=kbd:vtium-no-such-font\n").expect("the frit");
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
            frits: dir.clone(),
            helper: Some(real_helper(&kinds, held.then_some(go.as_path()))),
            ..no_fonts()
        };
        let heads = "version = 1
[head a]
profile = t
[head b]
profile = t
[rules]
display card:mem-small -> a
display card:mem-large -> b
input class:keyboard -> roaming
";
        let [(small_node, small), large] = two_displays();
        let displays = [(small_node, small.lit()), large];
        let script = "echo ready; read a; printf '\\033]50;unifont\\007'; echo away; \
                      read b; printf '\\033]50;t\\007'; echo back; read c";
        logged();
        let mut s = Seat::with_fonts(&["--font-size=0"], Some(heads), displays, script, fonts);
        assert!(s.until(SECOND, |a| head_shows(a, "a", "ready")), "{case}");
        let unknown = |lines: &[String]| {
            lines
                .iter()
                .filter(|l| l.contains("own cell is not known"))
                .count()
        };
        assert_eq!(
            unknown(&logged()),
            1,
            "{case}: the start asked once for both heads"
        );
        s.type_str("x\r");
        assert!(s.until(SECOND, |a| head_shows(a, "a", "away")), "{case}");
        assert_eq!(s.app.font_name(), "unifont", "{case}");
        s.type_str("y\r");
        assert!(s.until(SECOND, |a| head_shows(a, "a", "back")), "{case}");
        assert_eq!(s.app.font_name(), "t", "{case}");
        assert_eq!(unknown(&logged()), again, "{case}: the switch back");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// D6: with no head file the seat runs the compiled-in configuration,
/// the parse of the template: its one `console` head.
#[test]
fn with_no_head_file_the_seat_runs_the_compiled_in_heads() {
    let s = Seat::boot(&[], two_displays(), "read x");
    let names: Vec<&str> = s.app.heads().iter().map(Head::name).collect();
    let builtin = vtium_config::Config::builtin();
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
    let (fonts, starts) = frits("share", &["t", "u"]);
    let heads = "version = 1
[head a]
profile = t
[head b]
profile = t
[head c]
profile = u
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
    assert!(s.until(SECOND, |a| a.cells().len() == 3));
    assert_eq!(s.app.font_name(), "t");
    let started = || {
        std::fs::read_to_string(&starts)
            .map(|t| t.lines().count())
            .unwrap_or(0)
    };
    assert!(s.until(SECOND, |_| started() >= 2), "the helpers started");
    s.until(Duration::from_millis(300), |_| false);
    assert_eq!(started(), 2, "one helper for t, one for u");
    let pids: Vec<String> = std::fs::read_to_string(&starts)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    drop(s);
    let gone = |pid: &String| !Path::new(&format!("/proc/{pid}")).exists();
    let end = now() + SECOND;
    while now() < end && !pids.iter().all(gone) {
        std::thread::yield_now();
    }
    assert!(
        pids.iter().all(gone),
        "each helper killed and reaped: {pids:?}"
    );
    if let Some(dir) = starts.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
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
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
    assert!(
        s.until_memory("/dev/mem0", Duration::from_secs(4), |m| m.dpms
            == Some(Dpms::Off)),
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
        s.until(SECOND, |a| got(a).is_some_and(|g| g.0 == long)),
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
    assert!(s.until(SECOND, |a| shows(a, "ready")), "the session runs");
    assert!(
        s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0),
        "the display draws"
    );
    // 320x160 holds 40x10 cells of 8x16.
    assert_eq!(s.app.grid(), (40, 10));
    s.type_str("a\r");
    assert!(s.until(SECOND, |a| got(a).is_some_and(|g| g.0 == "a")));
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
    assert!(s.until(SECOND, |a| shows(a, "ready")), "the session runs");
    assert!(
        s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0),
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
    assert!(s.until(SECOND, |a| shows(a, "ready")), "the session runs");
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
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
    assert!(s.until(SECOND, |a| shows(a, "ready")), "the session runs");
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2), "both lit");
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
    assert!(s.until_memory("/dev/mem1", SECOND, |m| m.frames > 0));
    assert!(
        s.until(SECOND, |a| a.head("fast").is_some_and(Head::started)),
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
        s.until(SECOND, |a| head_shows(a, "fast", "got:x")),
        "the key reached the fast head's session"
    );
    assert!(
        s.until_memory("/dev/mem1", SECOND, |m| m.frames > fast_before),
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
    assert!(
        s.until(SECOND, |a| head_shows(a, quiet, "ready")
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
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
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
        s.until_memory("/dev/mem0", SECOND, |m| !m.awake),
        "asleep for the release"
    );
    assert!(
        s.until(SECOND, |a| !a.vt.is_active()),
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
    assert!(s.until(SECOND, |a| a.vt.is_active()));
    assert!(
        s.until_memory("/dev/mem0", Duration::from_secs(5), |m| m.awake
            && m.frames > before),
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2), "both lit");
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
    assert!(s.until_memory("/dev/mem1", SECOND, |m| m.frames > 0));
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
        s.until_memory("/dev/mem0", SECOND, |m| !m.awake),
        "the slow head's display asleep for the release"
    );
    assert!(!s.memory("/dev/mem1").awake, "both asleep");
    assert!(!s.app.vt.is_active(), "the release acknowledged");
    s.app.set_draw_hook("slow", None);
    let fast = s.memory("/dev/mem1").frames;
    s.app.input_event(InputEvent::Key(ctrl_super_f12), now());
    assert!(s.until(SECOND, |a| a.vt.is_active()), "the acquire");
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
        s.until_memory("/dev/mem0", Duration::from_secs(5), |m| m.shown.len()
            > shown
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
        s.until_memory("/dev/mem1", SECOND, |m| m.frames > fast),
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")
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
        s.until(SECOND, |a| head_shows(a, "small", "got:c")),
        "the claimed keyboard types into its head"
    );
    s.type_on(KBD_FOCUS, "f\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "large", "got:f")),
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
        s.until(SECOND, |a| head_shows(a, "small", "got:f")
            && head_shows(a, "large", "got:c")),
        "the focus's keyboard types into small, the claimed one into large"
    );
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(DOWN), now());
    assert_eq!(s.app.take_focus_move().as_deref(), Some("large"));
    s.type_on(KBD_FOCUS, "g\r");
    assert!(s.until(SECOND, |a| head_shows(a, "large", "got:g")));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")
        && head_shows(a, "large", "ready")));
    let (focus_in, focus_out) = ("1b 5b 49", "1b 5b 4f");
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "large", "k:"), "no keyboard yet");
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(
        s.until(SECOND, |a| head_shows(a, "large", focus_in)),
        "the keyboard gives the boot display's head the focus"
    );
    assert!(!head_shows(&s.app, "small", "k:"));
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    assert!(
        s.until(SECOND, |a| head_shows(a, "small", focus_in)
            && head_shows(a, "large", focus_out)),
        "the chord moves it"
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
        s.until(SECOND, |a| told(a, "small").iter().all(Option::is_some)
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
    assert!(s.until(SECOND, typed));
    s.type_on(KBD_SMALL, "q\r");
    // The line typed goes only with the hard reset of the restart.
    assert!(
        s.until(SECOND, |a| !typed(a)
            && head_shows(a, "small", "is:small:1")),
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
    assert!(s.until(SECOND, |a| ready(a).iter().all(Option::is_some)));
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
        s.until(SECOND, |a| seen(a) == [lines(&[focus_in]), lines(&[])]),
        "the shown console has the focus: {:?}",
        seen(&s.app)
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert!(
        s.until(SECOND, |a| seen(a)
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
        assert!(s.until(SECOND, |a| a.cells().len() == 1));
        assert_eq!(s.app.focus_head(), Some("small"), "the only lit head");
        if key {
            s.type_on(KBD_FOCUS, "k");
        }
        s.app.add_video(
            PathBuf::from("/dev/mem1"),
            Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
        );
        assert!(s.until(SECOND, |a| a.cells().len() == 2));
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
    let chord = |s: &mut Seat, keysym: u32| {
        s.app
            .device_input(Path::new(KBD_SMALL), ctrl_super(keysym), now());
        s.until(Duration::from_millis(50), |_| false);
    };
    s.type_on(KBD_SMALL, "one\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:one")));
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    assert!(!head_shows(&s.app, "small", "got:one"), "its own screen");
    s.type_on(KBD_SMALL, "two\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:two")));
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 2)));
    s.type_on(KBD_SMALL, "three\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:three")));
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
    assert!(s.until(SECOND, |a| told_ids(a, "small") == is(&["1", "2", "3"])));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert_eq!(consoles(&s.app, "small"), Some((3, 1)));
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((4, 2)),
        "opened after the shown console, and shown"
    );
    assert!(
        s.until(SECOND, |a| told_ids(a, "small")
            == is(&["1", "2", "4", "3"])),
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
        s.until(SECOND, |a| told_ids(a, "small") == is(&["1", "5", "2"])),
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
            s.until(SECOND, |a| head_shows(a, "small", &format!("got:end{id}"))),
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
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vtium-heads-close-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("a marker directory");
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
    assert!(s.until(SECOND, |a| told_ids(a, "small").iter().all(Option::is_some)));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:on")));
    let _ = std::fs::remove_dir_all(&d);
}

/// D5 and binary-review's finding 2: a session the open chord cannot
/// start leaves the run alive, as the C reference ignored a terminal that
/// would not open (`src/terminal.c:1771-1772`, `src/seat.c:624-628`). The
/// console that could not start leaves the head, which shows the console it
/// showed before, and the top of the head says so; the other sessions run
/// on. The head's login here is a script removed once the first console's
/// session has started, so the second cannot start.
// covers: platform.heads
#[test]
fn a_console_the_open_chord_cannot_start_leaves_and_the_run_goes_on() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vtium-heads-unstartable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let login = dir.join("login");
    script_file(
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
    let line = Some("No console opened: its login could not start.");
    assert!(
        s.until(SECOND, |a| a.head("small").and_then(Head::status) == line),
        "the line at the top of the head: {:?}",
        s.app.head("small").and_then(Head::status)
    );
    s.type_on(KBD_SMALL, "on\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "small", "got:on")),
        "the first console's session runs on"
    );
    s.type_on(KBD_LARGE, "on\r");
    assert!(s.until(SECOND, |a| head_shows(a, "large", "got:on")));
    assert_eq!(s.app.finished(), None, "vtium runs on");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A login script at `dir/login` that turns on mode 1004, prints its
/// console's id, then echoes each line it reads through `cat -v`, so a
/// focus report it was sent shows on its screen.
fn focus_login(dir: &Path) -> PathBuf {
    let login = dir.join("login");
    script_file(
        &login,
        "#!/bin/sh\nprintf '\\033[?1004h'; echo \"is:$VTIUM_CONSOLE\"; while read a; do echo \"got:$a\" | cat -v; done\n",
    );
    login
}

/// Writes `text` as a script at `path` that a head's login starts. This
/// process writes the text to a file beside it, and `cp` writes the
/// script, so this process never holds the script open to write. A child
/// that another test forks while this process holds a file open to write
/// holds that descriptor until its own exec, and starting the script
/// while it does fails with ETXTBSY.
fn script_file(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    let staged = path.with_extension("text");
    std::fs::write(&staged, text).expect("the script's text");
    let copied = std::process::Command::new("cp")
        .arg(&staged)
        .arg(path)
        .status()
        .expect("cp");
    assert!(
        copied.success(),
        "cp {} {}",
        staged.display(),
        path.display()
    );
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("its mode");
}

/// A scratch directory for test `name`, on disk under the build's own.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("vtium-heads-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
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
    assert!(s.until(SECOND, |a| told_ids(a, "small").iter().all(Option::is_some)));
    (s, login)
}

/// D5, binary-recheck's finding 2: a failed open never shows its console,
/// so the console shown before is told nothing: no focus-out and focus-in
/// reach its program before the line typed next. The focus-in the head's
/// keyboard gave it at the start may come before, whichever came first of
/// the keyboard and the program's mode 1004.
// covers: platform.heads
#[test]
fn a_failed_open_tells_the_console_shown_nothing() {
    let dir = scratch_dir("failed-open-focus");
    let (mut s, login) = three_focus_consoles(&dir);
    std::fs::rename(&login, dir.join("gone")).expect("the login moved");
    press_chord(&mut s, KBD_SMALL, RETURN);
    assert_eq!(consoles(&s.app, "small"), Some((3, 0)));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:")));
    let got = console_lines(&s.app, "small", "got:")
        .first()
        .cloned()
        .flatten();
    assert!(
        got.as_deref()
            .is_some_and(|l| l.ends_with('x') && !l.contains("^[[O")),
        "console 1's program got no focus-out before the line: {got:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
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
        s.until(SECOND, |a| told_ids(a, "small")
            == is(&["1", "2", "5", "3"])),
        "id 4 is never given again, and 5 opens after console 2: {:?}",
        told_ids(&s.app, "small")
    );
    assert_eq!(s.app.finished(), None);
    let _ = std::fs::remove_dir_all(&dir);
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
        s.until(SECOND, |a| head_shows(a, "small", "got:xyz")),
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
        s.until(SECOND, |a| head_shows(a, "small", "got:abc")),
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
            Some("No more consoles: small has 2, its most."),
        ),
        (
            &["--consoles-max=3"][..],
            Some("No more consoles: every head together has 3, the most."),
        ),
        (&["--head-consoles-max=2", "--osd=off"][..], None),
    ];
    for (args, line) in cases {
        let mut s = claimed_on(args, &claimed_consoles(2));
        assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "{args:?}");
        press_chord(&mut s, KBD_SMALL, RETURN);
        assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "{args:?}");
        assert!(
            s.until(SECOND, |a| a.head("small").and_then(Head::status) == line),
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
        s.until(SECOND, |a| status(a).as_deref()
            == Some("\u{25cb} small \u{b7} console 2 of 2")),
        "{:?}",
        status(&s.app)
    );
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert!(
        s.until(SECOND, |a| status(a).as_deref()
            == Some("\u{25cb} small \u{b7} console 1 of 1")),
        "{:?}",
        status(&s.app)
    );
}

/// D5: closing the shown console gives mode 1004's focus to the console
/// shown next. Turning mode 1004 on reports nothing, as in the C
/// reference, so the keyboard comes, and the switch is pressed, only once
/// both sessions have printed their ids, which they print after they turn
/// the mode on: a focus given before then is never reported.
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
        s.until(SECOND, |a| told_ids(a, "small").iter().all(Option::is_some)),
        "both sessions turned mode 1004 on"
    );
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    let (focus_in, focus_out) = ("1b 5b 49", "1b 5b 4f");
    assert!(s.until(SECOND, |a| head_shows(a, "small", focus_in)));
    press_chord(&mut s, KBD_SMALL, RIGHT);
    assert!(s.until(SECOND, |a| head_shows(a, "small", focus_in)
        && head_shows(a, "small", "is:2")));
    press_chord(&mut s, KBD_SMALL, BACKSPACE);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)));
    assert!(
        s.until(SECOND, |a| {
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
        s.until(SECOND, both),
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "see link")
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

/// ADR-1002, "What a link does", binary-review's finding 4: after a VT
/// switch away and back, the keyboards start afresh holding no Ctrl, so
/// the overlay Ctrl showed before the switch ends at the return, with no
/// key pressed after it.
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
    assert!(s.until_memory("/dev/mem1", SECOND, |m| m.awake));
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2 && shows(a, "ready")));
    s.type_str(&format!("{line}\r"));
    assert!(s.until(SECOND, |a| answered(a, line)));
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

/// The maintainer's VT review (2026-10-30): one head of two consoles on
/// two mirrored displays. Ctrl-Super-Right and Ctrl-Super-Left, pressed
/// as a keyboard sends them, switch the shown console at once, and the
/// frame of the turn that took the keys draws the console now shown on
/// both displays, pixel for pixel as a seat of one console that never
/// switched draws that screen. The only wait after a chord is for that
/// frame to come back from the head's render thread.
#[test]
fn the_console_chords_draw_the_console_they_show() {
    const KBD: &str = "/dev/input/event0";
    let one = drawn_alone("one");
    let two = drawn_alone("two");
    assert_ne!(one, two, "the two screens draw apart");
    let mut s = Seat::with_heads(&["--no-blink"], Some(MIRRORED_TWO), two_displays(), ECHO);
    s.app.add_input(Path::new(KBD), keyboard("kbd"));
    assert!(s.until(SECOND, |a| a.cells().len() == 2 && shows(a, "ready")));
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
    assert!(s.until(SECOND, |a| answered(a, "one")));
    assert!(drawn_now(&mut s) == one, "the first console");
    press(&mut s, 106, RIGHT);
    assert_eq!(shown(&s), Some((2, 1)), "Right shows the second");
    assert!(s.until(SECOND, |a| shows(a, "ready")));
    s.type_on(KBD, "two\r");
    assert!(s.until(SECOND, |a| answered(a, "two")));
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
    assert!(s.until(SECOND, |a| a.cells().len() == 3));
    let counts = |a: &App| -> Vec<usize> { a.heads().iter().map(Head::console_count).collect() };
    assert_eq!(counts(&s.app), [4, 3, 1], "a's cap, then the total's");
    assert!(s.until(SECOND, |a| a.heads().iter().all(Head::started)));
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
    let dir = std::env::temp_dir().join(format!("vtium-heads-hidden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the marks' directory");
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
    let started = s.until(Duration::from_secs(3), |_| starts().len() == 4);
    let seen = starts();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(started, "small's two, large's one, and 2 again: {seen:?}");
    assert_eq!(seen, ["1", "1", "2", "2"], "2 started again as 2");
    assert_eq!(
        consoles(&s.app, "small"),
        Some((2, 0)),
        "the first still shown"
    );
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:x")));
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
        s.until(SECOND, |a| notice(a, "small").as_deref()
            == Some(ENDED_CONSOLE)),
        "the shown console's session ended: {:?}",
        notice(&s.app, "small")
    );
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)), "in its place");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert_eq!(notice(&s.app, "small"), None, "the second runs");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:x")));
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
    assert!(s.until(SECOND, |a| notice(a, "small").as_deref() == Some(ENDED)));
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
        s.until(SECOND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
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
    assert!(s.until(SECOND, |a| notice(a, "large").as_deref() == Some(ENDED)));
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(SECOND, |a| notice(a, "small").is_some()));
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.finished(), None, "small's second console runs");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    s.type_on(KBD_SMALL, "q\r");
    assert!(
        s.until(SECOND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
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
    assert!(s.until(SECOND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    s.type_on(KBD_SMALL, "x\r");
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)));
    assert_eq!(notice(&s.app, "small").as_deref(), Some(ENDED));
    assert!(!head_shows(&s.app, "small", "got:x"));
    assert_eq!(s.app.finished(), None, "large runs on");
}

/// D1 and D5: a hidden console's program starts again at that console's
/// own size. Console 2, zoomed to 20 by 5, is hidden behind console 1 when
/// its program exits, and the program started again reads 5 rows of 20.
// covers: platform.heads
#[test]
fn a_hidden_console_starts_again_at_its_own_size() {
    let dir = std::env::temp_dir().join(format!("vtium-heads-own-size-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the marks' directory");
    let script = format!(
        "d={d}; echo \"size:$(stty size)\"; \
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
    assert!(s.until(SECOND, |a| sizes(a).iter().all(Option::is_some)));
    let key = |s: &mut Seat, event: InputEvent| {
        s.app.device_input(Path::new(KBD_SMALL), event, now());
        s.until(Duration::from_millis(50), |_| false);
    };
    key(&mut s, ctrl_super(RIGHT));
    let equal = InputEvent::Key(KeyEvent {
        mods: Mods::CTRL,
        ..char_key('=')
    });
    key(&mut s, equal);
    assert_eq!(grid(&s.app, "small"), Some((20, 5)), "console 2 zoomed");
    key(&mut s, ctrl_super(LEFT));
    assert_eq!(consoles(&s.app, "small"), Some((2, 0)));
    std::fs::write(dir.join("go"), "").expect("the go mark");
    let again = s.until(Duration::from_secs(3), |a| {
        dir.join("done").exists() && sizes(a).get(1) == Some(&Some("size:5 20".to_owned()))
    });
    let seen = sizes(&s.app);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(again, "console 2 started again at 5 rows of 20: {seen:?}");
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
        s.until(Duration::from_secs(3), |a| a.finished().is_some()),
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
    let marker = std::env::temp_dir().join(format!("vtium-heads-respawn-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let small = format!(
        "if [ -e {m} ]; then echo again; read x; else : > {m}; echo first; fi",
        m = marker.display()
    );
    let mut s = logins(&[], &small, "echo large; read x");
    assert!(
        s.until(Duration::from_secs(3), |a| head_shows(a, "small", "again")),
        "small's login came back"
    );
    assert!(head_shows(&s.app, "large", "large"), "large runs on");
    assert!(!head_shows(&s.app, "large", "again"));
    let _ = std::fs::remove_file(&marker);
}

/// The operator's answers (2026-10-30): under oneshot a head whose
/// program ends shows that the session ended and stays dark; vtium exits when the
/// last head's program ends.
#[test]
fn under_oneshot_vtium_exits_when_the_last_heads_program_ends() {
    // Large's program ends once the test writes this file.
    let go = std::env::temp_dir().join(format!("vtium-oneshot-go-{}", std::process::id()));
    std::fs::remove_file(&go).ok();
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
    std::fs::remove_file(&go).ok();
}

/// A seat whose head `looping` has one console whose program keeps
/// exiting at once and whose other head has two consoles, each marking
/// its SIGHUP in `dir`. The looping program's first start waits until both
/// of the other head's consoles have set their trap, so the exit comes
/// after them; every start after the first exits at once.
fn crash_loop(dir: &Path, looping: &str, policy: &str) -> Seat {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).expect("the marks' directory");
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
        let dir = std::env::temp_dir().join(format!(
            "vtium-heads-crash-loop-{}-{looping}",
            std::process::id()
        ));
        let mut s = crash_loop(&dir, looping, "open");
        let finished = s.until(Duration::from_secs(3), |a| a.finished().is_some());
        let marks = [dir.join("hup-1"), dir.join("hup-2")];
        let hung_up = finished && s.until(SECOND, |_| marks.iter().all(|m| m.exists()));
        let marked = marks.iter().map(|m| m.exists()).collect::<Vec<_>>();
        let _ = std::fs::remove_dir_all(&dir);
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
    let dir = std::env::temp_dir().join(format!(
        "vtium-heads-crash-loop-keys-{}",
        std::process::id()
    ));
    let mut s = crash_loop(&dir, "small", "closed");
    let roaming = "/dev/input/event6";
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    s.app.add_input(Path::new(roaming), keyboard("kbd-roaming"));
    let stopped = vtium::notice::stopped();
    let shown = s.until(Duration::from_secs(4), |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    let _ = std::fs::remove_dir_all(&dir);
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
    let dir = std::env::temp_dir().join(format!(
        "vtium-heads-crash-loop-closed-{}",
        std::process::id()
    ));
    let mut s = crash_loop(&dir, "small", "closed");
    s.app.add_input(Path::new(KBD_A), keyboard("kbd-a"));
    let marks = [dir.join("hup-1"), dir.join("hup-2")];
    let hung_up = s.until(Duration::from_secs(4), |_| marks.iter().all(|m| m.exists()));
    let stopped = vtium::notice::stopped();
    let shown = s.until(SECOND, |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    s.until(Duration::from_millis(300), |_| false);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(hung_up, "large's consoles hung up, shown and hidden");
    assert!(shown, "the notice on the notice screen");
    assert_eq!(s.app.finished(), None, "vtium holds its VT");
    assert!(s.app.vt.is_active());
    assert_eq!(
        s.app.notice_screen().map(|n| n.cells().len()),
        Some(1),
        "the boot display alone"
    );
    assert!(
        s.app
            .heads()
            .iter()
            .all(|h| h.cells().is_empty() && h.pty().is_none()),
        "every head dark, no session"
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
    let dir = std::env::temp_dir().join(format!(
        "vtium-heads-crash-loop-vt-switch-{}",
        std::process::id()
    ));
    let mut s = crash_loop(&dir, "small", "closed");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    let stopped = vtium::notice::stopped();
    let shown = s.until(Duration::from_secs(4), |a| {
        a.notice_screen().and_then(Head::notice) == Some(stopped.as_str())
    });
    let _ = std::fs::remove_dir_all(&dir);
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
        s.until_memory("/dev/mem0", SECOND, |m| m.awake)
            && s.until_memory("/dev/mem1", SECOND, |m| m.awake),
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
    let key = |s: &mut Seat, event: InputEvent| {
        s.app.device_input(Path::new(KBD_SMALL), event, now());
        s.until(Duration::from_millis(50), |_| false);
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    let mouse = vtium_config::Identity {
        path: Some("mouse-small".to_owned()),
        ..vtium_config::Identity::input().with_class(Class::Mouse)
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
        once.until(Duration::from_secs(3), |a| a.finished().is_some()),
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
    let script =
        "echo ready; read a; sleep 0.3; printf '\\033]50;unscii-8\\007'; echo asked; read b";
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(heads), displays, script);
    assert!(s.until(SECOND, |a| head_shows(a, "one", "ready")));
    let chord = |s: &mut Seat, keysym: u32| {
        s.app.input_event(ctrl_super(keysym), now());
        s.until(Duration::from_millis(50), |_| false);
    };
    chord(&mut s, RIGHT);
    assert!(s.until(SECOND, |a| head_shows(a, "one", "ready")));
    s.type_str("x\r");
    chord(&mut s, RIGHT);
    assert_eq!(consoles(&s.app, "one"), Some((2, 0)), "the second hidden");
    s.until(Duration::from_millis(800), |_| false);
    assert_eq!(s.app.font_name(), "unifont", "the request waits");
    chord(&mut s, RIGHT);
    assert!(
        s.until(SECOND, |a| a.font_name() == "unscii-8"),
        "shown, it switches the font"
    );
    assert!(s.until(SECOND, |a| head_shows(a, "one", "asked")));
}

/// The files a [`losing`] seat's sessions write, removed when dropped,
/// whether the test passed or not.
struct Files {
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
    fn drop(&mut self) {
        for path in [&self.script, &self.mark, &self.starts] {
            let _ = std::fs::remove_file(path);
        }
    }
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
    let dir = std::env::temp_dir();
    let path = |ext: &str| dir.join(format!("vtium-heads-{test}-{}.{ext}", std::process::id()));
    let files = Files {
        script: path("sh"),
        mark: path("hup"),
        starts: path("starts"),
    };
    let _ = std::fs::remove_file(&files.mark);
    let _ = std::fs::remove_file(&files.starts);
    std::fs::write(
        &files.script,
        format!(
            "echo $VTIUM_CONSOLE >> {starts}\ntrap 'echo > {m}; exit 0' HUP\necho ready\n\
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    // A frame on a screen ends the start's wait, which a turn far ahead
    // would otherwise run out.
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
    s.until(Duration::from_millis(50), |_| false);
    (s, files)
}

impl Seat {
    /// The display of the memory device at `node` goes, in a turn at `at`.
    fn unplug(&mut self, node: &str, at: Instant) {
        self.memory(node).events.push(Event::Removed(ID));
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
    assert!(s.hung_up(&f.mark, SECOND), "SIGHUP at once");
    assert!(
        s.until(SECOND, |a| a.head("small").and_then(Head::pty).is_some()),
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
        s.until(SECOND, |a| head_shows(a, "small", "got:x")),
        "the keys still reach it"
    );
    let frames = s.memory("/dev/mem0").frames;
    s.plug("/dev/mem0", t + Duration::from_secs(3601));
    assert!(
        s.until_memory("/dev/mem0", SECOND, |m| m.frames > frames),
        "drawn again on its return"
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
    assert!(s.hung_up(&f.mark, SECOND), "after it, SIGHUP");
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
    assert!(s.hung_up(&f.mark, SECOND), "the last went");
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
    assert!(s.hung_up(&f.mark, SECOND), "its timer ran out");
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"), "the boot display's");
    s.unplug("/dev/mem1", now());
    s.app.add_video(
        PathBuf::from("/dev/mem2"),
        Box::new(Memory::new(320, 160, "MEM-3", "mem-small")),
    );
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"), "it left");
    s.plug("/dev/mem1", now());
    assert!(s.until(SECOND, |a| a.cells().len() == 3));
    assert_eq!(s.app.focus_head(), Some("large"), "it came back");
}

/// D8: a head whose display never lit receives nothing, even from a
/// keyboard its rule claims.
#[test]
fn a_head_whose_display_never_lit_receives_no_key() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, "read x");
    s.app.add_input(Path::new(KBD_LARGE), keyboard("kbd-large"));
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
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
/// lighting late takes it even after the focus's head lost its display
/// (found by the review of H3c).
#[test]
fn a_late_boot_display_takes_the_focus_after_the_focus_heads_display_left() {
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
    assert_eq!(s.app.focus_head(), Some("small"));
    s.unplug("/dev/mem0", now());
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
    );
    assert!(s.until(SECOND, |a| a.head("large").map(|h| h.cells().len())
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
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
    assert_eq!(s.app.focus_head(), Some("small"));
    s.type_on(KBD_LARGE, "k");
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large").lit()),
    );
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.app.device_input(Path::new(unlit), ctrl_super(UP), now());
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move(), None, "nothing moved");
    assert_eq!(s.app.focus_head(), Some("large"));
    assert!(
        s.until(SECOND, |a| head_shows(a, "small", "k: 1b")),
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.app.device_input(Path::new(KBD_SMALL), vt_key(), now());
    assert!(s.app.vt.is_active(), "the VT stays");
    assert!(
        s.until(SECOND, |a| head_shows(a, "small", "k: 1b")),
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
        assert!(s.until(SECOND, |a| head_shows(a, "large", "ready")));
        assert_eq!(s.app.focus_head(), Some("large"), "closed: {closed}");
        s.type_on(unknown, "u\r");
        if closed {
            s.until(Duration::from_millis(200), |_| false);
            assert!(!head_shows(&s.app, "large", "got:u"), "nothing typed");
        } else {
            assert!(
                s.until(SECOND, |a| head_shows(a, "large", "got:u")),
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
        assert!(s.until(SECOND, |a| head_shows(a, "large", "ready")));
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
                s.until(SECOND, |a| head_shows(a, "large", "got:u")),
                "it roams"
            );
        }
        s.app.drained_input(drained(vt_key()), now());
        assert_eq!(s.app.vt.is_active(), closed, "closed: {closed}: the VT key");
    }
}

/// D5: a roaming keyboard asks the active head, and with none, since under
/// `policy = closed` every head is isolated unless it says otherwise, it
/// delivers nothing, the VT's keys included.
// covers: platform.heads.isolated
#[test]
fn a_roaming_keyboard_with_no_active_head_switches_no_vt() {
    let heads = TWO_HEADS.replace("version = 1\n", "version = 1\npolicy = closed\n");
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(&heads), displays, BYTES);
    s.app.add_input(Path::new(KBD_FOCUS), keyboard("elsewhere"));
    assert!(s.until(SECOND, |a| head_shows(a, "large", "ready")));
    assert_eq!(s.app.focus_head(), None, "no head can be the active head");
    s.app.device_input(Path::new(KBD_FOCUS), vt_key(), now());
    assert!(s.app.vt.is_active(), "the VT stays");
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
        assert!(s.hung_up(&f.mark, SECOND), "hang-up {n}");
        let _ = std::fs::remove_file(&f.mark);
        assert!(s.until(SECOND, |_| f.starts() == n + 1), "start {}", n + 1);
        s.plug("/dev/mem0", now());
        assert!(
            s.until(SECOND, |a| a
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
        assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
        s.plug("/dev/mem0", now());
        assert!(
            s.until(SECOND, |a| a
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
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((4, 0))));
    assert!(s.until(SECOND, |_| f.starts() == 4));
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, SECOND));
    assert!(s.until(SECOND, |_| f.starts() == 8), "four more");
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
    assert!(s.hung_up(&f.mark, SECOND));
    let shown = notice(&s.app, "small").unwrap_or_default();
    assert_eq!(shown.lines().next(), Some(ENDED), "{shown}");
    assert!(
        s.until(SECOND, |a| a.finished() == Some(vtium::app::Exit::Oneshot)),
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
        let dir =
            std::env::temp_dir().join(format!("vtium-heads-{test}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("0700");
        let path = dir.join("heads.conf");
        std::fs::write(&path, text).expect("the head file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("0644");
        HeadFile { dir, path }
    }

    /// The trust the start reads the file under, with the test's user in
    /// root's place.
    fn trust(&self) -> vtium_config::Trust {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata(&self.dir).expect("the directory").uid();
        vtium_config::Trust::owner(uid).below(&self.dir)
    }

    /// What the start runs for the file.
    fn rules(&self) -> Start {
        vtium::head::rules(&self.path, &self.trust())
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:c")));
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
        s.until(SECOND, |a| head_shows(a, "large", "got:s")
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
const ENDED: &str = "This session has ended and will not start again. To keep working, press Ctrl+Super+↑ to switch to another screen.";

/// The same notice while another console of its head still runs.
const ENDED_CONSOLE: &str = "This session has ended and will not start again. To keep working, press Ctrl+Super+→ to show another console on this screen.";

/// The start of the anti-lockout guard's notice.
const LOCKED_OUT: &str = "No keyboard was reaching a screen that is on";

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
const SKIPPED: &str = "invalid config skipped, using builtin\nRun vtium check-config /etc/vtium/heads.conf to see why.";
const PARTLY: &str = "config partly invalid, running what's valid\nRun vtium check-config /etc/vtium/heads.conf to see why.";
const CLOSED_OFF: &str = "vtium: config invalid, policy closed: input off\nNothing on this console takes input. Run vtium check-config /etc/vtium/heads.conf over ssh, or from other boot media, for the full report.";
const NO_DISPLAY: &str = "vtium: no display for any head\nConnect a display that a rule in /etc/vtium/heads.conf sends to a head.";

/// The panic screen of a broken closed file whose one error, on line 4,
/// is the key `focs`, read as `heads.conf` (D7): the console line, that
/// no head is left, the error with its file and line, and the next step,
/// each naming the file the start read.
const PANIC: &str = concat!(
    "vtium: config invalid, policy closed: input off\n",
    "The errors in heads.conf, listed below, leave no head that can run.\n",
    "heads.conf:4: This section takes no key named \"focs\". Check the key's spelling, or remove the line.\n",
    "Nothing on this console takes input. Run vtium check-config heads.conf over ssh, or from other boot media, for the full report.",
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
    s.until(Duration::from_millis(300), |_| false);
    match row.heads.first() {
        Some(&first) => {
            assert!(
                s.until(SECOND, |a| head_shows(a, first, "ready")),
                "{test}: its session starts"
            );
            assert_eq!(notice(&s.app, first).as_deref(), row.line, "{test}");
            let lit = s.app.head(first).map(|h| h.cells().len());
            assert_eq!(lit, Some(row.lit), "{test}: the displays it lights");
            let lit = s.app.notice_screen().is_some_and(|n| !n.cells().is_empty());
            assert!(!lit, "{test}: a head holds the display");
        }
        None => {
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
                        "This section takes no key named \"focs\". Check the key's spelling, or remove the line.",
                    ),
                    // the policy line vtium cannot read, at its line
                    "unreadable-policy" | "unreadable-policy-no-head" => at(
                        ":2",
                        "`policy` cannot be \"maybe\", so vtium cannot read this line and counts the file as closed and broken. Change it to one of `open`, `closed`.",
                    ),
                    _ => at(
                        "",
                        "The file needs `version = 1` above its first section. Add that line at the top.",
                    ),
                };
                let want = [
                    "vtium: config invalid, policy closed: input off".to_owned(),
                    format!("The errors in {path}, listed below, leave no head that can run."),
                    error,
                    format!(
                        "Nothing on this console takes input. Run vtium check-config {path} over ssh, or from other boot media, for the full report."
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
            s.until_memory("/dev/mem1", SECOND, |m| m.has(PAPER)),
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
            s.until(SECOND, |a| head_shows(a, first, "got:c")),
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
    assert!(s.until(SECOND, |a| head_shows(a, "desk", "ready")));
    assert_eq!(s.app.focus_head(), Some("desk"), "not isolated");
    s.type_on(KBD_A, "t\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "desk", "got:t")),
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
    assert!(s.until(SECOND, |a| head_shows(a, "desk", "ready")));
    assert_eq!(notice(&s.app, "desk").as_deref(), Some(PARTLY));
    s.type_on(KBD_A, "x\r");
    s.type_on(KBD_B, "d\r");
    assert!(s.until(SECOND, |a| head_shows(a, "desk", "got:d")));
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
        s.until_memory("/dev/mem1", SECOND, |m| m.has(PAPER)),
        "drawn on the boot display"
    );
    // the start logged each error with its line, so the seat logs the
    // console line alone, never the report
    let lines = logged();
    let line = "vtium: config invalid, policy closed: input off";
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
/// the boot display for `vtium: no display for any head` alone, which
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
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)));
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
        s.until(SECOND, |a| head_shows(a, "desk", "ready")),
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
    assert!(s.until(SECOND, |a| a.finished().is_some()));
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
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)));
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
        s.until_memory("/dev/mem0", SECOND, |m| !m.has(PAPER)),
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
    assert!(s.until(SECOND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert!(
        s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)),
        "drawn"
    );
    assert!(!s.memory("/dev/mem1").has(PAPER), "large runs on");
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
        s.until_memory("/dev/mem0", SECOND, |m| m.has(STATUS_PAPER)),
        "drawn on small"
    );
    assert!(
        s.until_memory("/dev/mem1", SECOND, |m| m.has(STATUS_PAPER)),
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:x")));
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(SECOND, |a| !head_shows(a, "small", "got:x")
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
        s.until_memory("/dev/mem0", SECOND, |m| m.has(STATUS_PAPER)
            && !m.has(PAPER)),
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
    assert!(s.until(SECOND, |a| notice(a, "small").is_some()));
    assert!(
        s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)),
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.app.add_video(
        PathBuf::from("/dev/mem1"),
        Box::new(Memory::new(640, 320, "MEM-2", "mem-large")),
    );
    assert!(s.until(SECOND, |a| consoles(a, "large") == Some((2, 0))
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
    assert!(s.until(SECOND, |a| a.cells().len() == 3));
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
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
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

/// D5: under `osd = off` the active head's move composes no frame, since
/// it leaves the picture as it was; under `minimal` it composes one with
/// the dot, the control.
#[test]
fn under_osd_off_a_move_composes_no_frame() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    for (osd, drawn) in [("--osd=off", false), ("--osd=minimal", true)] {
        let mut s = claimed_with(&[osd, "--no-blink"]);
        // The large display shows "ready", past the cursor's first cell:
        // its last frame of the start is drawn and no other is on its way.
        assert!(
            s.until_memory("/dev/mem1", BOUND, |m| m.inked(0, 1..5)),
            "the start drawn"
        );
        assert!(
            s.until(BOUND, |a| a
                .head("large")
                .is_some_and(|h| h.frames_out() == 0)),
            "no frame of the start composing"
        );
        let composed = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&composed);
        let hook = move || {
            count.fetch_add(1, Ordering::SeqCst);
        };
        s.app.set_draw_hook("large", Some(Arc::new(hook)));
        s.app
            .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
        if drawn {
            assert!(
                s.until(BOUND, |_| composed.load(Ordering::SeqCst) > 0),
                "{osd}: a frame composed"
            );
        } else {
            // No frame comes to wait for: the window only gives one the
            // chance to.
            s.until(Duration::from_millis(200), |_| false);
            assert_eq!(composed.load(Ordering::SeqCst), 0, "{osd}");
        }
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
    assert!(s.until(SECOND, |a| head_shows(a, "large", "ready")));
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
        s.until(SECOND, |a| head_shows(a, "large", "got:a")
            && head_shows(a, "large", "got:b")),
        "both type into the focus"
    );
    s.app.add_video(
        PathBuf::from("/dev/mem0"),
        Box::new(Memory::new(320, 160, "MEM-1", "mem-small")),
    );
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_A, "c\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "small", "got:c")),
        "the guard let go"
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
    assert!(s.until(SECOND, |a| head_shows(a, "large", "got:f")));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")
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
        s.until(SECOND, |a| a.cells().len() == 2),
        "every display lit"
    );
    assert_eq!(
        s.app.heads().iter().map(Head::name).collect::<Vec<_>>(),
        ["console"]
    );
    assert!(s.until(SECOND, |a| shows(a, "ready")), "its session runs");
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
    s.until(Duration::from_millis(200), |_| false);
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
        s.until(SECOND, |a| head_shows(a, "small", "got:s")),
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
        s.until(SECOND, |a| head_shows(a, "small", "got:k")),
        "small's kept: isolated = yes"
    );
    assert!(
        s.until(SECOND, |a| head_shows(a, "large", "got:m")),
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
    assert!(s.until(SECOND, |a| a.grid() == (80, 20)), "the large alone");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::Off));
    s.unplug("/dev/mem1", now());
    assert!(s.until(SECOND, |a| a.grid() == (40, 10)), "the small lit");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::On));
    s.plug("/dev/mem1", now());
    assert!(s.until(SECOND, |a| a.grid() == (80, 20)), "the large again");
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
        (&vtium_config::Config::builtin(), None)
    );
    let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
    let mut s = Seat::with_rules(&[], Some(start), displays, "echo ready; read x", no_fonts());
    assert!(s.until(SECOND, |a| shows(a, "ready")));
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
    assert!(shown.contains("No keyboard was found."), "{shown:?}");
    assert!(s.until(SECOND, |a| shows(a, "ready")), "vtium runs");
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
    assert!(s.until(SECOND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)));
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
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.unplug("/dev/mem1", now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"), "not moved");
    for name in ["small", "large"] {
        let shown = notice(&s.app, name).unwrap_or_default();
        assert!(!shown.contains(LOCKED_OUT), "{name}: {shown:?}");
    }
    assert!(notice(&s.app, "small").is_some_and(|n| n.contains("No keyboard was found.")));
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
    assert!(s.until(SECOND, |a| shows(a, "ready")));
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(notice(&s.app, "console"), None);
    s.type_on(KBD_IGNORED, "i\r");
    s.type_on(KBD_A, "a\r");
    assert!(s.until(SECOND, |a| shows(a, "got:a")), "types");
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
    assert!(s.until(SECOND, |a| head_shows(a, "large", "got:r")));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
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
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.unplug("/dev/mem0", now());
    assert_eq!(s.app.focus_head(), Some("small"), "kbd-large reaches large");
    s.app
        .device_input(Path::new(KBD_LARGE), ctrl_alt_f(1), now());
    s.app.turn(now());
    assert_eq!(s.app.focus_head(), Some("large"));
    let shown = notice(&s.app, "large").unwrap_or_default();
    assert!(
        shown.contains("It types into the active screen, large, until the next boot."),
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
            shown.contains("no screen is active now."),
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
        s.until(SECOND, |a| head_shows(a, "small", "got:s")),
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
    assert!(s.until(SECOND, |a| a.grid() == (80, 20)), "the large alone");
    s.unplug("/dev/mem1", now());
    assert!(s.until(SECOND, |a| a.grid() == (40, 10)), "the small lit");
    assert_eq!(s.memory("/dev/mem0").dpms, Some(Dpms::On));
    assert_eq!(s.app.cells().len(), 1);
    s.plug("/dev/mem1", now());
    assert!(s.until(SECOND, |a| a.grid() == (80, 20)), "the large again");
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::Off),
        "the small dark again"
    );
    assert_eq!(s.app.cells().len(), 1);
}
