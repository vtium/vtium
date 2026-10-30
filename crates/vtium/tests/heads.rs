//! Heads (ADR-1401 D1, D3): the seat booted with displays in memory, on
//! the fake VT, with a real child on a real pty in each head. With no head
//! file every display mirrors one grid, as before heads; a head file gives
//! each head its own grid at its own displays' density.

use std::any::Any;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::head::Head;
use vtium::video::Video;
use vtium_config::Class;
use vtium_platform::display::{Canvas, DisplayId, Dpms, Event, Identity, Rect, SwapError, Swapped};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

const ID: DisplayId = DisplayId(1);
const SECOND: Duration = Duration::from_secs(5);

/// A device with one display, its frame buffer in memory, named for the
/// rules by its connector and its card.
struct Memory {
    w: u32,
    h: u32,
    connector: &'static str,
    card: &'static str,
    dpi: f64,
    /// Its connector was lit at start, so it is the boot display.
    lit: bool,
    pixels: Vec<u8>,
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
            connector,
            card,
            dpi: 72.0,
            lit: false,
            pixels: Vec::new(),
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
        self.connector.to_owned()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((self.w, self.h))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((self.dpi, self.dpi))
    }
    fn identity(&self, _: DisplayId) -> Option<Identity> {
        Some(Identity {
            connector: self.connector.to_owned(),
            card: Some(self.card.to_owned()),
            edid: None,
            lit_at_start: self.lit,
        })
    }
    fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
        if !self.awake || self.dpms != Some(Dpms::On) {
            return None;
        }
        Some(Canvas {
            pixels: &mut self.pixels,
            width: self.w,
            height: self.h,
            stride: self.w * 4,
            age: 0,
        })
    }
    fn swap(&mut self, id: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
        if self.dpms != Some(Dpms::On) {
            return Err(SwapError::Off);
        }
        self.frames += 1;
        self.events.push(Event::Flipped(id));
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn set_dpms(&mut self, _: DisplayId, dpms: Dpms) -> io::Result<()> {
        self.dpms = Some(dpms);
        self.dpms_sets.push(dpms);
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
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

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
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
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(now),
            env,
        );
        app.set_font_sources(fonts);
        if let Some(text) = heads {
            app.set_heads(vtium_config::parse(text).expect("a valid head file").config);
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
input class:keyboard -> focus
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
display card:mem-large -> dark
";
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        ("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large")),
        ("/dev/mem2", Memory::new(320, 240, "MEM-3", "mem-other")),
    ];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    assert!(s.until(SECOND, |a| a.cells().len() == 1));
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.cells(), [(8, 16)]);
    for node in ["/dev/mem1", "/dev/mem2"] {
        let m = s.memory(node);
        assert_eq!((m.dpms, m.frames), (Some(Dpms::Off), 0), "{node} is dark");
    }
    assert!(s.memory("/dev/mem0").frames > 0, "the small display draws");
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
/// frit, and its own `font-size` wins over kmscon.conf's; a head with the
/// default profile and no font key of its own takes kmscon.conf's, the
/// built-in font here.
#[test]
fn a_heads_profile_names_its_frit_and_its_own_font_size_wins() {
    let (fonts, starts) = frits("profile", &["t"]);
    let heads = "version = 1
[head small]
[head large]
profile = t
font-size = 32
[rules]
display card:mem-small -> small
display card:mem-large -> large
";
    let mut s = Seat::with_fonts(&[], Some(heads), two_displays(), "read x", fonts);
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(s.app.head("small").map(Head::cells), Some(vec![(8, 16)]));
    assert_eq!(s.app.head("large").map(Head::cells), Some(vec![(16, 32)]));
    assert_eq!(grid(&s.app, "large"), Some((40, 10)));
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
font-name = t
[head b]
font-name = t
[head c]
font-name = u
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
display card:mem-large -> dark
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

/// D2: one render thread per head. The slow head, first in the file,
/// takes 3 s to compose each frame once a stand-in slows it; while it
/// composes a frame, a key reaches the session of the fast head, which
/// holds the focus, and the fast head's display shows its echo, both
/// within a second. The slow head's frame comes once it is composed.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the stand-in's slow draw is a sleep"
)]
fn a_slow_head_delays_neither_another_heads_frame_nor_input() {
    let heads = "version = 1
[head slow]
[head fast]
[rules]
display card:mem-small -> slow
display card:mem-large -> fast
input class:keyboard -> focus
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
    let slow = Duration::from_secs(3);
    s.app.set_draw_hook(
        "slow",
        Some(std::sync::Arc::new(move || std::thread::sleep(slow))),
    );
    let (slow_before, fast_before) = (s.memory("/dev/mem0").frames, s.memory("/dev/mem1").frames);
    let started = now();
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    // The turn that starts the slow head's frame.
    s.until(Duration::from_millis(30), |_| false);
    s.type_str("x\r");
    assert!(
        s.until(SECOND, |a| head_shows(a, "fast", "got:x")),
        "the key reached the fast head's session"
    );
    assert!(
        s.until_memory("/dev/mem1", SECOND, |m| m.frames > fast_before),
        "the fast head drew"
    );
    let fast = now() - started;
    assert_eq!(
        s.memory("/dev/mem0").frames,
        slow_before,
        "the slow head's frame is still composing"
    );
    assert!(
        fast < Duration::from_secs(1),
        "the key and the fast head's frame waited on the slow head: {fast:?}"
    );
    assert!(
        s.until_memory("/dev/mem0", Duration::from_secs(5), |m| m.frames
            > slow_before),
        "the slow head's frame comes"
    );
    assert!(now() - started >= slow, "after its slow compose");
}

/// D2: a frame composing holds up no VT switch. While the slow head's
/// frame takes 2 s to compose, the VT's release is acknowledged within a
/// second, the device asleep; the acquire draws the display again.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the stand-in's slow draw is a sleep"
)]
fn a_frame_composing_holds_up_no_vt_release() {
    let display = (
        "/dev/mem0",
        Memory::new(320, 160, "MEM-1", "mem-small").lit(),
    );
    let mut s = Seat::boot(&[], [display], "read x");
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.frames > 0));
    let slow = Duration::from_secs(2);
    s.app.set_draw_hook(
        "console",
        Some(std::sync::Arc::new(move || std::thread::sleep(slow))),
    );
    let started = now();
    s.memory("/dev/mem0").events.push(Event::Redraw(ID));
    // The turn that starts the slow frame.
    s.until(Duration::from_millis(30), |_| false);
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
    let released = now() - started;
    assert!(
        released < Duration::from_secs(1),
        "the release waited on the frame: {released:?}"
    );
    s.app.set_draw_hook("console", None);
    let before = s.memory("/dev/mem0").frames;
    s.app.input_event(InputEvent::Key(ctrl_super_f12), now());
    assert!(s.until(SECOND, |a| a.vt.is_active()));
    assert!(
        s.until_memory("/dev/mem0", Duration::from_secs(5), |m| m.awake
            && m.frames > before),
        "the acquire draws the display again"
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
input class:keyboard -> focus
";

/// The keyboards [`claimed`] adds: one claimed by each head, and one
/// following the focus.
const KBD_SMALL: &str = "/dev/input/event0";
const KBD_LARGE: &str = "/dev/input/event1";
const KBD_FOCUS: &str = "/dev/input/event2";

/// Two heads booted on [`CLAIMED`], the large display the boot display,
/// with [`KBD_SMALL`], [`KBD_LARGE`] and [`KBD_FOCUS`], each head's
/// session echoing each line it reads, and ending at `q`.
fn claimed() -> Seat {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let script = "echo ready; while read a; do [ \"$a\" = q ] && exit; echo \"got:$a\"; done";
    let mut s = Seat::with_heads(&[], Some(CLAIMED), displays, script);
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
/// focus to the next head with a lit display, and Ctrl-Super-Down to the
/// previous. The chord moves only the devices following the focus; a
/// claimed keyboard keeps typing into its head.
#[test]
fn the_focus_chord_moves_only_the_devices_following_the_focus() {
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
        .device_input(Path::new(KBD_SMALL), ctrl_super(DOWN), now());
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

/// D5 and the operator's answer of 2026-10-30: the focus starts on the
/// boot display's head even when that display lights a turn late, until
/// the first key; after a key it stays where it is.
#[test]
fn the_focus_waits_for_a_late_boot_display_until_the_first_key() {
    let late = |key: bool| {
        let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
        let mut s = Seat::with_heads(&[], Some(TWO_HEADS), displays, "read x");
        assert!(s.until(SECOND, |a| a.cells().len() == 1));
        assert_eq!(s.app.focus_head(), Some("small"), "the only lit head");
        if key {
            s.type_str("k");
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

/// #82 and the operator's chords (2026-10-30): Ctrl-Super-Return opens a
/// console in the head the keyboard delivers to and shows it,
/// Ctrl-Super-Right and Ctrl-Super-Left show the next and the previous,
/// wrapping, and Ctrl-Super-Backspace closes the shown one and shows the
/// next. Each console is a session of its own; the other head's consoles
/// stay as they were.
#[test]
fn the_console_chords_open_switch_and_close_a_heads_consoles() {
    let mut s = claimed();
    let chord = |s: &mut Seat, keysym: u32| {
        s.app
            .device_input(Path::new(KBD_SMALL), ctrl_super(keysym), now());
        s.until(Duration::from_millis(50), |_| false);
    };
    chord(&mut s, RETURN);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)), "opened and shown");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "two\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:two")));
    chord(&mut s, RIGHT);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((2, 0)),
        "wraps to the first"
    );
    assert!(!head_shows(&s.app, "small", "got:two"), "its own screen");
    s.type_on(KBD_SMALL, "one\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:one")));
    chord(&mut s, LEFT);
    assert_eq!(consoles(&s.app, "small"), Some((2, 1)));
    assert!(head_shows(&s.app, "small", "got:two"));
    chord(&mut s, BACKSPACE);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "closed");
    assert!(head_shows(&s.app, "small", "got:one"), "the next shows");
    chord(&mut s, BACKSPACE);
    assert_eq!(
        consoles(&s.app, "small"),
        Some((1, 0)),
        "the last console is not closed"
    );
    assert_eq!(consoles(&s.app, "large"), Some((1, 0)));
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

/// The operator's answers (2026-10-30): a console whose program exits
/// while others stay open closes, and the head shows its next console.
#[test]
fn a_console_exiting_shows_the_heads_next_console() {
    let mut s = claimed();
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((2, 1))
        && head_shows(a, "small", "ready")));
    s.type_on(KBD_SMALL, "x\r");
    assert!(s.until(SECOND, |a| head_shows(a, "small", "got:x")));
    s.type_on(KBD_SMALL, "q\r");
    assert!(
        s.until(SECOND, |a| consoles(a, "small") == Some((1, 0))),
        "the console closed"
    );
    assert!(!head_shows(&s.app, "small", "got:x"), "the first shows");
    assert!(s.app.finished().is_none());
}

/// Two heads whose consoles run the head file's own `login`: small runs
/// `small`, large runs `large`, each a shell command.
fn logins(args: &[&str], small: &str, large: &str) -> Seat {
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
    Seat::with_heads(args, Some(&heads), two_displays(), "read x")
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
/// program ends shows "session ended" and stays dark; vtium exits when the
/// last head's program ends.
#[test]
fn under_oneshot_vtium_exits_when_the_last_heads_program_ends() {
    let mut s = logins(&["--oneshot"], "echo bye", "sleep 1");
    assert!(
        s.until(SECOND, |a| a.head("small").and_then(Head::notice)
            == Some("session ended")),
        "small's program ended"
    );
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.finished(), None, "large's runs on");
    assert_eq!(s.app.head("large").and_then(Head::notice), None);
    assert!(
        s.until(Duration::from_secs(3), |a| a.finished()
            == Some(vtium::app::Exit::Oneshot)),
        "the last head's program ended"
    );
}

/// The operator's answers (2026-10-30): a session restarting too fast
/// gives up on its head alone, with a notice; when every head has given
/// up vtium exits with status 1, for the getty fallback.
#[test]
fn a_head_restarting_too_fast_gives_up_alone_and_vtium_exits_after_every_head() {
    let gave_up = |a: &App, name: &str| {
        a.head(name)
            .and_then(Head::notice)
            .is_some_and(|n| n.contains("too quickly"))
    };
    let mut s = logins(&[], "exit 1", "echo large; read x");
    assert!(
        s.until(Duration::from_secs(3), |a| gave_up(a, "small")),
        "small gave up"
    );
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.finished(), None, "large's runs on");
    assert!(head_shows(&s.app, "large", "large"));
    let mut both = logins(&[], "exit 1", "exit 1");
    assert!(
        both.until(Duration::from_secs(3), |a| a.finished().is_some()),
        "every head gave up"
    );
    assert_eq!(both.app.finished(), Some(vtium::app::Exit::TooQuick));
    assert_eq!(both.app.finished().map(vtium::app::Exit::status), Some(1));
}

/// D1: a zoom changes the grid of the console shown alone; each console
/// keeps its own grid, which shows again with it.
#[test]
fn a_zoom_changes_the_grid_of_the_console_shown_alone() {
    let mut s = claimed();
    let key = |s: &mut Seat, event: InputEvent| {
        s.app.device_input(Path::new(KBD_SMALL), event, now());
        s.until(Duration::from_millis(50), |_| false);
    };
    key(&mut s, ctrl_super(RETURN));
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
/// moves on that head's displays, whichever head holds the focus.
#[test]
fn a_claimed_pointer_moves_on_its_heads_display_alone() {
    let heads = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:mouse-small -> small
input class:mouse -> focus
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
}
