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
    /// Left dark at its bind, as a DRM card leaves a display the rules
    /// leave dark while another is lit: never announced until let go.
    held: bool,
    /// Let go: the next hotplug announces it.
    released: bool,
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
            held: false,
            released: false,
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

    /// This display, left dark at its bind.
    fn held(self) -> Memory {
        Memory { held: true, ..self }
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
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake && self.dpms == Some(Dpms::On)
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
            let parsed = vtium_config::parse(text).expect("a valid head file");
            (parsed.config, None)
        });
        Seat::with_rules(args, rules, displays, script, fonts)
    }

    /// [`Seat::with_fonts`], running the head file in force and the notice
    /// of its start, as [`vtium::head::rules`] gives them, on VT 1.
    fn with_rules(
        args: &[&str],
        rules: Option<(vtium_config::Config, Option<String>)>,
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
        if let Some((rules, notice)) = rules {
            app.set_heads(rules);
            if let Some(text) = notice {
                app.notice(&text);
            }
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
/// display `node` draws the echo, within 12 turns; no turn parses more
/// than 64 KiB of flood's output, and flood's output keeps coming.
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
        match echoed {
            None if head_shows(&s.app, quiet, "got:k") => echoed = Some(frames),
            Some(at) if frames > at => break,
            _ => {}
        }
    }
    assert!(
        echoed.is_some(),
        "the key's echo reached {quiet}'s screen within 12 turns"
    );
    assert!(
        turns < 12,
        "{quiet}'s display drew the echo within 12 turns"
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
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(args, Some(CLAIMED), displays, ECHO);
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

/// vtium/vtium#167 and D5: each console's session is told its head and the
/// console's own id, distinct in every console of every head; the id is
/// not the console's place in the ring, so after a console closes the one
/// left keeps its id and a console opened next takes a new one.
// covers: platform.heads
#[test]
fn each_console_is_told_its_head_and_its_own_id_through_a_close() {
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
    s.type_on(KBD_SMALL, "q\r");
    assert!(
        s.until(SECOND, |a| consoles(a, "small") == Some((1, 0))),
        "the first console closed"
    );
    assert_eq!(
        told(&s.app, "small"),
        [line("is:small:2")],
        "2 keeps its id"
    );
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
    assert!(
        s.until(SECOND, |a| told(a, "small").iter().all(Option::is_some)
            && told(a, "small").len() == 2)
    );
    assert_eq!(
        told(&s.app, "small"),
        [line("is:small:2"), line("is:small:3")],
        "a new console takes a new id, never the place 2 left"
    );
    assert_eq!(
        told(&s.app, "large"),
        [line("is:large:1"), line("is:large:2")]
    );
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

/// The key events xkb makes of a chord pressed as a keyboard sends it:
/// Left Ctrl, then Left Super, then the key `code` (evdev's), each press
/// an event with the modifiers as they then stand; the releases make
/// none. Each event is the built-in keymap's and the system keymap's
/// alike, `unicode` the key's character, Return's and Backspace's
/// control characters.
fn pressed_chord(code: u32, keysym: u32, unicode: Option<char>) -> [InputEvent; 3] {
    let press = |code: u32, mods: Mods, keysym: u32, unicode: Option<char>| {
        InputEvent::Key(KeyEvent {
            keycode: code + 8,
            ascii: 0,
            mods,
            keysyms: vec![keysym],
            codepoints: vec![unicode],
            repeat: false,
        })
    };
    let ctrl_super = Mods::CTRL.union(Mods::LOGO);
    [
        press(29, Mods::CTRL, 0xffe3, None),
        press(125, ctrl_super, 0xffeb, None),
        press(code, ctrl_super, keysym, unicode),
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
/// asked for: each display's frame count is taken before the turn, and
/// the loop runs until the head's render thread has handed each display
/// its next frame, which [`SECOND`] bounds.
fn next_frames(s: &mut Seat, turn: impl FnOnce(&mut Seat)) -> [Vec<u8>; 2] {
    let before = MIRRORED.map(|node| s.memory(node).frames);
    turn(s);
    for (node, before) in MIRRORED.into_iter().zip(before) {
        assert!(
            s.until_memory(node, SECOND, |m| m.frames > before),
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

/// The maintainer's VT review (2026-10-30): with no head file, so one
/// head on two mirrored displays, Ctrl-Super-Right, Ctrl-Super-Left and
/// Ctrl-Super-Backspace, pressed as a keyboard sends them, switch the
/// shown console at once, and the frame of the turn that took the keys
/// draws the console now shown on both displays, pixel for pixel as a
/// seat that never switched draws that screen. The only wait after a
/// chord is for that frame to come back from the head's render thread.
#[test]
fn the_console_chords_draw_the_console_they_show() {
    const KBD: &str = "/dev/input/event0";
    let one = drawn_alone("one");
    let two = drawn_alone("two");
    assert_ne!(one, two, "the two screens draw apart");
    let mut s = Seat::boot(&["--no-blink"], two_displays(), ECHO);
    s.app.add_input(Path::new(KBD), keyboard("kbd"));
    assert!(s.until(SECOND, |a| a.cells().len() == 2 && shows(a, "ready")));
    let shown = |s: &Seat| {
        s.app
            .heads()
            .first()
            .map(|h| (h.console_count(), h.shown_console()))
    };
    let press = |s: &mut Seat, code: u32, keysym: u32| {
        let unicode = match keysym {
            RETURN => Some('\r'),
            BACKSPACE => Some('\u{8}'),
            _ => None,
        };
        for event in pressed_chord(code, keysym, unicode) {
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
    press(&mut s, 28, RETURN);
    assert_eq!(shown(&s), Some((2, 1)), "Return opens and shows");
    assert!(s.until(SECOND, |a| shows(a, "ready")));
    s.type_on(KBD, "two\r");
    assert!(s.until(SECOND, |a| answered(a, "two")));
    assert!(drawn_now(&mut s) == two, "the new console");
    let (state, frame) = chord(&mut s, 106, RIGHT);
    assert_eq!(state, Some((2, 0)), "Right wraps to the first");
    assert!(frame == one, "Right's frame draws the first console");
    let (state, frame) = chord(&mut s, 105, LEFT);
    assert_eq!(state, Some((2, 1)), "Left wraps to the second");
    assert!(frame == two, "Left's frame draws the second console");
    let (state, frame) = chord(&mut s, 14, BACKSPACE);
    assert_eq!(state, Some((1, 0)), "Backspace closes it");
    assert!(
        frame == one,
        "Backspace's frame draws the console shown next"
    );
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
    let mut s = logins(&["--oneshot"], "echo bye", "sleep 1");
    assert!(
        s.until(SECOND, |a| a.head("small").and_then(Head::notice)
            == Some(ENDED)),
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

/// A seat whose head `looping` has one console whose program keeps
/// exiting at once and whose other head has two consoles, each marking
/// its SIGHUP in `dir`. The looping program's first start waits until both
/// of the other head's consoles have set their trap, so the exit comes
/// after them; every start after the first exits at once.
fn crash_loop(dir: &Path, looping: &str) -> Seat {
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
[head small]
consoles = {small}
[head large]
consoles = {large}
[rules]
display card:mem-small -> small
display card:mem-large -> large
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
        let mut s = crash_loop(&dir, looping);
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

/// A head that ended runs no session, so Ctrl-Super-Return opens no
/// console in it.
#[test]
fn an_ended_head_opens_no_console() {
    let heads = "version = 1
[head small]
login = /bin/sh -c \"echo bye\"
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
";
    let mut s = Seat::with_heads(&["--oneshot"], Some(heads), two_displays(), "read x");
    s.app.add_input(Path::new(KBD_SMALL), keyboard("kbd-small"));
    assert!(s.until(SECOND, |a| a.head("small").and_then(Head::notice).is_some()));
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(consoles(&s.app, "small"), Some((1, 0)), "no console opened");
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
    /// A line at each session's start.
    starts: PathBuf,
}

impl Files {
    /// How many sessions have started.
    fn starts(&self) -> usize {
        std::fs::read_to_string(&self.starts).map_or(0, |t| t.lines().count())
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
            "echo >> {starts}\ntrap 'echo > {m}; exit 0' HUP\necho ready\n\
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

/// D8: under `on-loss = hangup` a head losing its last display hangs its
/// sessions up at once, as a line losing its carrier: the pty closes and
/// the session gets SIGHUP. The head then follows the respawn policy,
/// which starts its login again; the other head runs on.
#[test]
fn under_hangup_a_head_losing_its_last_display_hangs_its_sessions_up_at_once() {
    let (mut s, f) = losing("hangup", &[], "on-loss = hangup", false);
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

/// D8: under `nohup`, the default, a head losing its last display keeps
/// its session, however long the display stays away, and the keys of the
/// devices reaching it still type into it, as the kernel console's tty
/// does; a display of the head coming back draws it again.
#[test]
fn under_nohup_a_head_losing_its_last_display_runs_on_and_shows_again_on_its_return() {
    let (mut s, f) = losing("nohup", &[], "on-loss = nohup", false);
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
    let (mut s, f) = losing("hangup-30s", &[], "on-loss = hangup 30s", false);
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
    let (mut s, f) = losing("mirrored", &[], "on-loss = hangup", true);
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
    let (mut s, f) = losing("reattach", &[], "on-loss = hangup 30s", false);
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
    s.until(Duration::from_millis(100), |_| false);
    assert!(!head_shows(&s.app, "large", "never"));
    s.app
        .device_input(Path::new(KBD_LARGE), ctrl_super(RETURN), now());
    assert_eq!(consoles(&s.app, "large"), Some((1, 0)), "no console opened");
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

/// D5 and D8: the focus chord is the seat's, so it moves the focus from
/// the keyboard of a head whose display never lit too.
#[test]
fn the_focus_chord_from_a_never_lit_heads_keyboard_moves_the_focus() {
    let heads = "version = 1
[head small]
[head large]
[head unlit]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-unlit -> unlit
";
    let [small, (node, large)] = two_displays();
    let displays = [small, (node, large.lit())];
    let mut s = Seat::with_heads(&[], Some(heads), displays, "read x");
    let unlit = "/dev/input/event3";
    s.app.add_input(Path::new(unlit), keyboard("kbd-unlit"));
    assert!(s.until(SECOND, |a| a.cells().len() == 2));
    assert_eq!(s.app.focus_head(), Some("large"));
    s.app.device_input(Path::new(unlit), ctrl_super(UP), now());
    assert_eq!(s.app.take_focus_move().as_deref(), Some("small"));
}

/// D8 and the respawn policy: a hang-up is vtium's, not the session's
/// exit, so a display flapping under `hangup` brings the login back each
/// time and never gives the head up as exiting too quickly.
#[test]
fn a_display_flapping_under_hangup_brings_the_login_back_each_time() {
    let (mut s, f) = losing("flap", &[], "on-loss = hangup", false);
    for n in 1..=7 {
        s.unplug("/dev/mem0", now());
        assert!(s.hung_up(&f.mark, SECOND), "hang-up {n}");
        let _ = std::fs::remove_file(&f.mark);
        assert!(s.until(SECOND, |_| f.starts() == n + 1), "start {}", n + 1);
        s.plug("/dev/mem0", now());
        s.until(Duration::from_millis(50), |_| false);
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
    let (mut s, _files) = losing("exit-and-hangup", &[], "on-loss = hangup", false);
    for n in 1..=7 {
        s.type_on(KBD_SMALL, "q\r");
        // The loop reads the exit, and no turn takes it before the hang-up.
        for _ in 0..15 {
            s.lp.dispatch(Some(Duration::from_millis(20)), &mut s.app)
                .expect("dispatch");
        }
        let read = |a: &App| a.head("small").is_some_and(|h| h.term().exited.is_some());
        assert!(read(&s.app), "exit {n} is read, its turn not yet come");
        s.unplug("/dev/mem0", now());
        assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
        s.plug("/dev/mem0", now());
        s.until(Duration::from_millis(50), |_| false);
    }
    assert_eq!(s.app.head("small").and_then(Head::notice), None);
    assert!(s.app.head("small").and_then(Head::pty).is_some());
}

/// D8 and the operator's answers (2026-10-30): a head hung up comes back
/// with the consoles its file asks for, as at its start.
#[test]
fn a_head_hung_up_comes_back_with_the_consoles_its_file_asks_for() {
    let (mut s, f) = losing("consoles", &[], "on-loss = hangup\nconsoles = 4", false);
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((4, 0))));
    assert!(s.until(SECOND, |_| f.starts() == 4));
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, SECOND));
    assert!(s.until(SECOND, |_| f.starts() == 8), "four more");
    assert_eq!(consoles(&s.app, "small"), Some((4, 0)));
}

/// D8 and the operator's answers (2026-10-30): under `oneshot` a head
/// hung up ends with the notice that the session ended, and vtium exits once no head has
/// anything left to run.
#[test]
fn under_oneshot_a_hang_up_ends_the_head_and_then_vtium() {
    let (mut s, f) = losing("oneshot", &["--oneshot"], "on-loss = hangup", true);
    s.unplug("/dev/mem1", now());
    s.unplug("/dev/mem0", now());
    assert!(s.hung_up(&f.mark, SECOND));
    assert_eq!(s.app.head("small").and_then(Head::notice), Some(ENDED));
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

    /// The rules the start runs from the file, and the notice it shows.
    fn rules(&self) -> (vtium_config::Config, Option<String>) {
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
                std::fs::read(e.path()).expect("its bytes").hash(&mut h);
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
input class:keyboard -> focus
";

const KBD_IGNORED: &str = "/dev/input/event3";

/// Two heads booted on `rules`, the large display the boot display, so
/// the focus rests on `large`, with [`KBD_SMALL`] and [`KBD_IGNORED`].
fn rescue_seat(rules: (vtium_config::Config, Option<String>)) -> Seat {
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
    /// Whether a pixel of the frame is `bgr`.
    fn has(&self, bgr: [u8; 3]) -> bool {
        self.pixels.chunks_exact(4).any(|p| p[..3] == bgr)
    }
}

/// The notice of a head whose session ended for good under `oneshot`.
const ENDED: &str = "This session has ended and will not start again. To keep working, press Ctrl+Super+↑ to switch to another screen.";

/// The start of the anti-lockout guard's notice.
const LOCKED_OUT: &str = "No keyboard was reaching a screen that is on";

/// The text of head `name`'s notice.
fn notice(app: &App, name: &str) -> Option<String> {
    app.head(name).and_then(Head::notice).map(str::to_owned)
}

/// D7 and D11: a boot with each kind of invalid head file, each row of
/// D7's first outcome, runs the shipped rules, shows the notice naming the
/// error's line on the display, outside the session's screen, and leaves
/// the file's bytes, by their hash, and its directory as they were.
#[test]
fn a_boot_with_an_invalid_head_file_runs_the_shipped_rules_shows_the_notice_and_leaves_its_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let rules = "[rules]\ndisplay * -> desk\n";
    let invalid: [(&str, String, &str); 12] = [
        ("no-version", format!("[head desk]\n{rules}"), ":1: "),
        (
            "version",
            format!("version = 2\n[head desk]\n{rules}"),
            ":1: ",
        ),
        (
            "syntax",
            format!("version = 1\n[head desk]\n{rules}desk\n"),
            ":5: ",
        ),
        (
            "key",
            format!("version = 1\n[head desk]\ncolour = red\n{rules}"),
            ":3: ",
        ),
        (
            "undefined",
            "version = 1\n[head desk]\n[rules]\ndisplay * -> desc\n".into(),
            ":4: ",
        ),
        (
            "kind",
            "version = 1\n[head desk]\n[rules]\ninput * -> dark\n".into(),
            ":4: ",
        ),
        (
            "two-heads",
            format!("version = 1\n[head desk]\n[head desk]\n{rules}"),
            ":3: ",
        ),
        (
            "key-twice",
            format!("version = 1\n[head desk]\nconsoles = 2\nconsoles = 2\n{rules}"),
            ":4: ",
        ),
        (
            "two-rules",
            format!("version = 1\n[head desk]\n{rules}{rules}"),
            ":5: ",
        ),
        (
            "two-guards",
            format!("version = 1\n[head desk]\n{rules}[guards]\n[guards]\n"),
            ":6: ",
        ),
        (
            "range",
            format!("version = 1\n[head desk]\nfont-size = 100000\n{rules}"),
            ":3: ",
        ),
        (
            "owner",
            format!("version = 1\n[head desk]\n{rules}"),
            "can be changed by its group or by other users",
        ),
    ];
    for (test, text, says) in invalid {
        let file = HeadFile::new(&format!("invalid-{test}"), &text);
        if test == "owner" {
            std::fs::set_permissions(&file.path, std::fs::Permissions::from_mode(0o664))
                .expect("0664");
        }
        let before = file.listing();
        let start = file.rules();
        assert_eq!(
            start.0,
            vtium_config::Config::builtin(),
            "{test}: the shipped rules"
        );
        let displays = [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))];
        let mut s = Seat::with_rules(&[], Some(start), displays, "echo ready; read x", no_fonts());
        assert_eq!(
            s.app.heads().iter().map(Head::name).collect::<Vec<_>>(),
            ["console"],
            "{test}"
        );
        assert!(
            s.until(SECOND, |a| head_shows(a, "console", "ready")),
            "{test}: the session starts"
        );
        let shown = notice(&s.app, "console").unwrap_or_default();
        assert!(shown.contains(says), "{test}: {shown:?}");
        assert!(shown.contains("built-in rules"), "{test}: {shown:?}");
        assert!(
            s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)),
            "{test}: drawn"
        );
        assert!(
            !s.app.screen_text().iter().any(|l| l.contains("shipped")),
            "{test}: not in the session"
        );
        assert_eq!(file.listing(), before, "{test}: the file as it was");
    }
}

/// An invalid head file's notice, on one display.
fn noticed() -> (Seat, HeadFile) {
    let file = HeadFile::new(
        "noticed",
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desc\n",
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
/// shows on one head when that head shows another console, when its
/// number of consoles changes, and when its display returns after it had
/// none; each time it goes after `osd-timeout`, and the other head's
/// never shows. Under `osd = minimal` the dot says nothing of consoles,
/// and a console chord shows none.
// covers: platform.heads.focus
#[test]
fn the_osd_shows_on_a_head_whose_console_changes_or_whose_display_returns() {
    let mut s = claimed_with(&["--osd=full"]);
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
        s.app.turn(t + Duration::from_secs(10));
        assert_eq!(statuses(&s.app), [None, None], "{what}: gone");
    };
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
    assert!(s.until(SECOND, |a| head_shows(a, "small", "ready")));
    let line = "○ small · console 2 of 2";
    shows_then_goes(&mut s, now(), line, "a console opened");
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RIGHT), now());
    let line = "○ small · console 1 of 2";
    shows_then_goes(&mut s, now(), line, "the next console");
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((1, 0))));
    let line = "○ small · console 1 of 1";
    shows_then_goes(&mut s, now(), line, "a console's program exited");
    let t = now();
    s.unplug("/dev/mem0", t);
    s.app.turn(t + Duration::from_secs(10));
    assert_eq!(statuses(&s.app), [None, None], "nothing at the loss");
    let back = t + Duration::from_secs(11);
    s.plug("/dev/mem0", back);
    shows_then_goes(&mut s, back, line, "the display returned");
    let mut s = claimed();
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
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
    let mut s = claimed_with(&["--osd=full"]);
    s.until(Duration::from_millis(100), |_| false);
    assert_eq!(statuses(&s.app), [None, None], "none in normal use");
    s.app
        .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
    s.app.turn(now());
    assert_eq!(
        statuses(&s.app),
        [
            Some("● small · console 1 of 1".to_owned()),
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
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
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
        s.until(Duration::from_millis(50), |_| false);
        assert_eq!(
            marks(&s.app),
            [Some(Mark::NoFocus), Some(Mark::Focus)],
            "{args:?}"
        );
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
input class:keyboard -> focus
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
input class:keyboard -> focus
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
    let mut s = claimed_with(&["--osd=full"]);
    s.app
        .device_input(Path::new(KBD_SMALL), ctrl_super(RETURN), now());
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((2, 1))
        && head_shows(a, "small", "ready")));
    let t = now() + Duration::from_secs(10);
    s.unplug("/dev/mem0", t);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(SECOND, |a| consoles(a, "small") == Some((1, 0))));
    s.app.turn(t + Duration::from_secs(60));
    let line = "○ small · console 1 of 1";
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
        s.until(Duration::from_millis(200), |_| false);
        let composed = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&composed);
        let hook = move || {
            count.fetch_add(1, Ordering::SeqCst);
        };
        s.app.set_draw_hook("large", Some(Arc::new(hook)));
        s.app
            .device_input(Path::new(KBD_FOCUS), ctrl_super(UP), now());
        s.until(Duration::from_millis(200), |_| false);
        assert_eq!(composed.load(Ordering::SeqCst) > 0, drawn, "{osd}");
    }
}

/// Two heads, each keyboard but `kbd-large` claimed by `small`, which
/// [`stranded`] gives no display, with the head file's `guards`.
fn stranded_file(guards: &str) -> String {
    format!(
        "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-large -> large
input class:keyboard -> small
{guards}"
    )
}

const KBD_A: &str = "/dev/input/event4";
const KBD_B: &str = "/dev/input/event5";

/// [`stranded_file`] booted with only the large display, two keyboards
/// claimed by `small`, each head's session echoing each line it reads.
fn stranded(guards: &str) -> Seat {
    let displays = [("/dev/mem1", Memory::new(640, 320, "MEM-2", "mem-large"))];
    let script = "echo ready; while read a; do echo \"got:$a\"; done";
    let mut s = Seat::with_heads(&[], Some(&stranded_file(guards)), displays, script);
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
    let mut s = stranded("");
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

/// D9's switch: with `anti-lockout = off` a ruleset that strands every
/// keyboard is left as it is.
#[test]
fn with_anti_lockout_off_a_ruleset_stranding_every_keyboard_is_left_as_it_is() {
    let mut s = stranded("[guards]\nanti-lockout = off\n");
    s.type_on(KBD_A, "a\r");
    s.until(Duration::from_millis(200), |_| false);
    assert!(!head_shows(&s.app, "large", "got:a"));
    assert_eq!(notice(&s.app, "large"), None);
}

/// D7, D9 and D11's display-guard row: a file with `version = 1` and no
/// rules lights the boot display in a default head; every other display
/// stays dark.
#[test]
fn a_file_with_version_1_and_no_rules_lights_the_boot_display() {
    let displays = [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        (
            "/dev/mem1",
            Memory::new(640, 320, "MEM-2", "mem-large").lit(),
        ),
    ];
    let mut s = Seat::with_heads(&[], Some("version = 1\n"), displays, "echo ready; read x");
    assert!(s.until(SECOND, |a| a.cells().len() == 1), "one display lit");
    assert_eq!(s.app.grid(), (80, 20), "the boot display, the large, alone");
    assert!(s.until(SECOND, |a| shows(a, "ready")), "its session runs");
    assert_eq!(
        s.memory("/dev/mem0").dpms,
        Some(Dpms::Off),
        "the other stays dark"
    );
}

/// D9's switch: with `never-dark = off` a file with `version = 1` and no
/// rules leaves every display dark.
#[test]
fn with_never_dark_off_a_file_with_no_rules_leaves_every_display_dark() {
    let displays = [(
        "/dev/mem1",
        Memory::new(640, 320, "MEM-2", "mem-large").lit(),
    )];
    let text = "version = 1\n[guards]\nnever-dark = off\n";
    let mut s = Seat::with_heads(&[], Some(text), displays, "echo ready; read x");
    s.until(Duration::from_millis(200), |_| false);
    assert_eq!(s.app.cells(), Vec::<(u32, u32)>::new());
    assert_eq!(s.memory("/dev/mem1").dpms, Some(Dpms::Off));
}

/// D9's switches: with `rescue-key = off` the rescue key moves nothing,
/// and a head's `rescue = off` keeps the keyboards it claims while another
/// head's move.
#[test]
fn rescue_key_off_moves_nothing_and_a_heads_rescue_off_keeps_its_keyboard() {
    let off = RESCUE.replace("[rules]", "[guards]\nrescue-key = off\n[rules]");
    let file = HeadFile::new("rescue-off", &off);
    let mut s = rescue_seat(file.rules());
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
        .replace("[head small]", "[head small]\nrescue = off")
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
        "small's kept: rescue = off"
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
    let mut s = stranded("");
    s.app.device_input(Path::new(KBD_A), ctrl_alt_f(1), now());
    s.app.turn(now());
    s.type_on(KBD_A, "a\r");
    s.type_on(KBD_B, "b\r");
    assert!(s.until(SECOND, |a| head_shows(a, "large", "got:a")));
    s.until(Duration::from_millis(200), |_| false);
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

/// D9 and D11's escape row: with `vtium.heads=builtin` on the kernel
/// command line, a head file that turns every guard off is ignored for
/// that boot, and the compiled-in rules run with every guard on.
#[test]
fn vtium_heads_builtin_ignores_a_head_file_that_turns_every_guard_off() {
    let off = "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n\
               [guards]\nanti-lockout = off\nrescue-key = off\nnever-dark = off\n";
    let file = HeadFile::new("escape", off);
    let (read, _) = vtium::head::start_rules(&file.path, &file.trust(), "quiet");
    assert!(!read.guards.anti_lockout, "the file without the escape");
    let cmdline = "BOOT_IMAGE=/vmlinuz vtium.heads=builtin quiet";
    let (ran, notice) = vtium::head::start_rules(&file.path, &file.trust(), cmdline);
    assert_eq!(ran, vtium_config::Config::builtin());
    assert_eq!(
        ran.guards,
        vtium_config::Guards::default(),
        "every guard on"
    );
    assert_eq!(notice, None);
}

/// D11: a boot with no head file present writes nothing where the head
/// file lives, its listing the same before and after.
#[test]
fn a_boot_with_no_head_file_writes_nothing_where_it_lives() {
    let file = HeadFile::new("absent", "");
    std::fs::remove_file(&file.path).expect("no head file");
    let before = file.listing();
    let start = vtium::head::start_rules(&file.path, &file.trust(), "");
    assert_eq!(start, (vtium_config::Config::builtin(), None));
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
#[test]
fn an_ended_heads_notice_leaves_the_loop_asleep() {
    let mut s = claimed_with(&["--oneshot"]);
    s.type_on(KBD_SMALL, "q\r");
    assert!(s.until(SECOND, |a| notice(a, "small").as_deref() == Some(ENDED)));
    assert!(s.until_memory("/dev/mem0", SECOND, |m| m.has(PAPER)));
    let t = now() + Duration::from_secs(31);
    for i in 0..400u64 {
        s.app.turn(t + Duration::from_millis(i));
    }
    let wake = s.app.next_wake(t + Duration::from_millis(400));
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
    let mut s = stranded("");
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
