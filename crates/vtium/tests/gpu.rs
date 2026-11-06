//! The GPU path on a booted console (ADR-1303 D9, D10): the seat with
//! `hwaccel` on, its displays in memory, on the fake VT, its heads drawing
//! through Mesa's llvmpipe on their render threads, watched and steered by
//! the test's GPU, [`TestGpu`].
#![cfg(feature = "gpu")]

use std::any::Any;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::head::Start;
use vtium::painter::{GpuSource, TestGpu};
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Identity, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;
use vtium_settings::Class;

const ID: DisplayId = DisplayId(1);

/// How long a wait on a state that another thread or process brings, such
/// as a frame the render thread draws or a line a session prints, may run
/// before the test fails: only a bound, since each wait ends at the state
/// it waits for.
const BOUND: Duration = Duration::from_secs(10);

#[expect(
    clippy::disallowed_methods,
    reason = "the seat runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// A device with one display, its buffer in memory, named for the rules by
/// its connector and its card. As a framebuffer's one buffer, the buffer is
/// lent to one token at a time, a token from before a sleep or a wake-up
/// comes back too late, and a sleep clears it.
struct Memory {
    w: u32,
    h: u32,
    connector: &'static str,
    card: &'static str,
    /// What the display shows: the buffer lent, as the last swap left it.
    pixels: Vec<u8>,
    /// The buffer each token lends, drawn into on the render thread.
    drawn: Arc<Mutex<Vec<u8>>>,
    /// The buffer's age: 1 once it holds a frame, 0 when unknown.
    age: u32,
    /// Taken anew at each sleep and wake-up.
    lease: u64,
    lent: bool,
    events: Vec<Event>,
    awake: bool,
    dpms: Option<Dpms>,
    frames: u32,
}

impl Memory {
    fn new(w: u32, h: u32, connector: &'static str, card: &'static str) -> Memory {
        Memory {
            w,
            h,
            connector,
            card,
            pixels: Vec::new(),
            drawn: Arc::default(),
            age: 0,
            lease: 0,
            lent: false,
            events: Vec::new(),
            awake: false,
            dpms: None,
            frames: 0,
        }
    }

    /// A new mode in place, `w` by `h`: the display draws into new buffers
    /// at the new size, a token lent before comes back too late, and the
    /// owner is told to redraw.
    fn set_mode(&mut self, w: u32, h: u32) {
        (self.w, self.h) = (w, h);
        self.lease += 1;
        self.age = 0;
        self.pixels = vec![0; (w * h * 4) as usize];
        self.drawn = Arc::new(Mutex::new(self.pixels.clone()));
        self.events.push(Event::Redraw(ID));
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
        // the C reference's fixed density, where a point is a pixel
        Some((72.0, 72.0))
    }
    fn identity(&self, _: DisplayId) -> Option<Identity> {
        Some(Identity {
            connector: self.connector.to_owned(),
            card: Some(self.card.to_owned()),
            edid: None,
            connector_path: None,
            lit_at_start: false,
        })
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        if std::mem::take(&mut self.lent) {
            // dropped without being given back
            self.age = 0;
        }
        self.lent = true;
        let memory: Arc<Shared> = self.drawn.clone();
        Some(Token::new(id, (self.w, self.h), self.w * 4, self.age, memory).leased(self.lease))
    }
    fn give_back(&mut self, token: Token) {
        self.lent = false;
        drop(token);
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        self.lent = false;
        if token.lease() != self.lease {
            drop(token);
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
        Ok(())
    }
    fn sleep(&mut self) {
        self.lease += 1;
        self.awake = false;
        if Arc::strong_count(&self.drawn) == 1 {
            self.drawn.lock().unwrap().fill(0);
        }
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.lease += 1;
        self.age = 0;
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
        if Arc::strong_count(&self.drawn) == 1 {
            *self.drawn.lock().unwrap() = self.pixels.clone();
        }
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

/// A head for each display, by its card, and a keyboard for each head.
const TWO_HEADS: &str = "version = 1
[head small]
[head large]
[rules]
display card:mem-small -> small
display card:mem-large -> large
input path:kbd-small -> small
input path:kbd-large -> large
input class:keyboard -> roaming
";

/// The keyboard of head `small`, and of head `large`.
const KEYBOARDS: [(&str, &str, &str); 2] = [
    ("/dev/input/event0", "kbd-small", "small"),
    ("/dev/input/event1", "kbd-large", "large"),
];

/// Each display of a head of its own, by its card, and the first lit.
fn two_heads() -> [(&'static str, Memory); 2] {
    [
        ("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small")),
        ("/dev/mem1", Memory::new(320, 160, "MEM-2", "mem-large")),
    ]
}

/// Prints `ready`, then echoes each line it reads after `got:`.
const ECHO: &str = "stty -echo; printf ready; while read -r l; do printf '\\ngot:%s' \"$l\"; done";

struct Seat {
    lp: Loop<App>,
    app: App,
    /// Dropped after the app, as fields drop in order.
    reap: Reap,
}

/// Joins, as the seat drops after its app, the render threads the app went
/// on without because their GPU driver had not let go in time (ADR-1303
/// D9). No such thread is then inside the GPU driver when the test binary
/// runs its exit handlers, which would tear libEGL and Mesa down under it.
/// A test that holds a head's drop on purpose turns it off and joins the
/// threads itself once it lets go.
struct Reap(bool);

impl Drop for Reap {
    fn drop(&mut self) {
        if self.0 {
            let running = vtium::painter::wait_for_left(BOUND, &now);
            if !std::thread::panicking() {
                assert_eq!(running, 0, "a render thread left running");
            }
        }
    }
}

impl Drop for Seat {
    /// Ends the session of each head's shown console, with its process
    /// group, before the seat's app drops, so no session outlives its test.
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

/// Kills the session `pid` leads, its whole process group, and reaps the
/// leader. The pty holds `pid` until it reaps the child, so the pidfd
/// opened here names the session.
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
    let _ = rustix::process::kill_process_group(pid, Signal::KILL);
    while let Err(Errno::INTR) =
        rustix::process::waitid(WaitId::PidFd(pidfd.as_fd()), WaitIdOptions::EXITED)
    {}
}

impl Seat {
    /// The seat booted with `hwaccel` on the test's GPU `gpu`, the head
    /// file `heads`, and `displays`, each session running `script`. The
    /// cursor does not blink, since the tests compare frames drawn at
    /// different times.
    fn boot(
        gpu: &Arc<TestGpu>,
        heads: Option<&str>,
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
    ) -> Seat {
        let args: Vec<OsString> = [
            "--font-size=16",
            "--hwaccel",
            "--no-blink",
            "--login",
            "--",
            "/bin/sh",
            "-c",
            script,
        ]
        .iter()
        .map(OsString::from)
        .collect();
        let loaded = config::load(&args, &mut |_: &Path| Ok(None));
        assert_eq!(loaded.reports, [], "the seat's command line");
        let lp: Loop<App> = Loop::new().expect("a loop");
        let env = vec![(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        let vt = FakeVt::with_number(1);
        let mut app = App::new(loaded.config, Box::new(vt), lp.handle(), Rc::new(now), env);
        app.set_font_sources(no_fonts());
        if let Some(text) = heads {
            let start = Start::trusted(Path::new("heads.conf"), text.as_bytes());
            assert_eq!(start.line(), None, "a valid head file");
            app.set_heads(start);
        }
        for (node, memory) in displays {
            app.add_video(PathBuf::from(node), Box::new(memory));
        }
        app.set_gpu_source(GpuSource::Test(Arc::clone(gpu)));
        if heads.is_some() {
            for (node, path, _) in KEYBOARDS {
                app.add_input(
                    Path::new(node),
                    vtium_settings::Identity {
                        path: Some(path.to_owned()),
                        ..vtium_settings::Identity::input().with_class(Class::Keyboard)
                    },
                );
            }
        }
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat {
            lp,
            app,
            reap: Reap(true),
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
    /// at most [`BOUND`].
    fn until_memory(&mut self, node: &str, done: impl Fn(&Memory) -> bool) -> bool {
        let end = now() + BOUND;
        while !done(self.memory(node)) {
            if now() > end {
                return false;
            }
            self.until(Duration::from_millis(20), |_| false);
        }
        true
    }

    /// Types `text` on the keyboard of head `name`.
    fn type_on(&mut self, name: &str, text: &str) {
        let node = KEYBOARDS
            .iter()
            .find(|k| k.2 == name)
            .map(|k| k.0)
            .expect("the head's keyboard");
        for c in text.chars() {
            self.app
                .device_input(Path::new(node), InputEvent::Key(char_key(c)), now());
        }
    }

    /// The lines logged once `done` holds of them, with the loop turning
    /// meanwhile.
    fn lines_until(&mut self, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        let mut lines = logged();
        let end = now() + BOUND;
        while !done(&lines) && now() < end {
            self.until(Duration::from_millis(20), |_| false);
            lines.extend(logged());
        }
        lines
    }

    /// The loop's turns until no frame of any head is out and every
    /// display shows what its head last drew.
    fn settled(&mut self) {
        assert!(
            self.until(BOUND, |a| a.heads().iter().all(|h| h.frames_out() == 0)),
            "the frames out came back"
        );
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

/// Whether a row of head `name`'s screen holds `text`.
fn head_shows(app: &App, name: &str, text: &str) -> bool {
    app.head(name)
        .is_some_and(|h| h.screen_text().iter().any(|l| l.contains(text)))
}

/// The lines logged on this thread since the last call.
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

/// The display lines among `lines`, each as its connector and the path it
/// names.
fn display_lines(lines: &[String]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let rest = l.strip_prefix("Display ")?;
            let name = rest.split(' ').next()?;
            let path = rest.split("text renderer [").nth(1)?.split(']').next()?;
            Some((name.to_owned(), path.to_owned()))
        })
        .collect()
}

/// The pair a display line gives for `connector` on `path`.
fn line(connector: &str, path: &str) -> (String, String) {
    (connector.to_owned(), path.to_owned())
}

// ADR-1303 D9, vtium/vtium#244: while the VT is another session's, a head
// on the GPU path holds no texture and no frame in GL, only its context's
// compiled programs. When the VT comes back, the head draws its displays
// whole on the GPU path again, making what it gave back without setting
// any display up anew, and the display shows what it showed before
#[test]
fn a_head_on_the_gpu_path_gives_back_its_textures_while_the_vt_is_away() {
    logged();
    let gpu = Arc::new(TestGpu::default());
    let mut s = Seat::boot(
        &gpu,
        None,
        [("/dev/mem0", Memory::new(320, 160, "MEM-1", "mem-small"))],
        ECHO,
    );
    assert!(s.until(BOUND, |a| head_shows(a, "console", "ready")));
    s.settled();
    assert!(
        s.until(BOUND, |_| gpu.kept.load(SeqCst) == 1),
        "on the GPU path"
    );
    let shown = s.memory("/dev/mem0").pixels.clone();
    assert!(shown.iter().any(|&b| b != 0), "a frame shown");
    let (textures, targets, programs) = *gpu.live.lock().unwrap();
    assert!(textures > 0 && targets > 0 && programs > 0, "drawn in GL");
    let set_ups = gpu.set_ups.lock().unwrap().len();
    s.app.vt.leave().expect("the fake VT");
    assert!(
        s.until(BOUND, |a| !a.vt.is_active()),
        "the VT switched away"
    );
    assert!(
        s.until(BOUND, |_| gpu.releases.load(SeqCst) == 1),
        "the head's GPU path released"
    );
    assert_eq!(
        *gpu.live.lock().unwrap(),
        (0, 0, programs),
        "the head holds only its programs while the VT is away"
    );
    let drawn = gpu.drawn.load(SeqCst);
    s.app.vt.activate().expect("the fake VT");
    assert!(s.until_memory("/dev/mem0", |m| m.awake
        && m.frames > 0
        && m.pixels == shown));
    // the wake-up cleared the buffer, so the frame shown is drawn whole
    assert!(gpu.drawn.load(SeqCst) > drawn, "drawn on the GPU path");
    let (textures_back, targets_back, programs_back) = *gpu.live.lock().unwrap();
    assert!(textures_back > 0 && targets_back > 0, "made again");
    assert_eq!(programs_back, programs, "no program compiled again");
    assert_eq!(gpu.set_ups.lock().unwrap().len(), set_ups, "set up anew");
    let lines = logged();
    assert!(!lines.iter().any(|l| l.contains("GPU path")), "{lines:#?}");
}

// vtium/vtium#283, ADR-1303 D10: a head whose GPU set-up hangs, as a
// driver that hangs making its context would, holds neither the loop nor
// another head. The loop takes the keys for both heads' sessions, and the
// other head, whose set-up came through, draws its frames; the held head's
// display line waits for its path. Once its set-up ends, the held head
// draws on the GPU path and its line names gpu
#[test]
fn a_head_whose_gpu_set_up_hangs_holds_neither_the_loop_nor_another_head() {
    logged();
    let gpu = Arc::new(TestGpu::default());
    let gate = Arc::new(Mutex::new(()));
    *gpu.set_up_gate.lock().unwrap() = Some(Arc::clone(&gate));
    // the gate is held until the test opens it, or until long past the
    // bounds, so a loop that waits for the set-up goes on and fails the test
    // instead of hanging it
    let (holding, wait_holding) = std::sync::mpsc::channel();
    let (open, opened) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _held = gate.lock().unwrap();
        holding.send(()).unwrap();
        opened.recv_timeout(3 * BOUND).is_ok()
    });
    wait_holding.recv().unwrap();
    let mut s = Seat::boot(&gpu, Some(TWO_HEADS), two_heads(), ECHO);
    let lines = s.lines_until(|l| display_lines(l).len() == 1);
    // the first head to start its render thread took the gate; the other
    // draws on the GPU path
    let (waiting, drawing) = if display_lines(&lines) == [line("MEM-2", "gpu")] {
        (
            ("/dev/mem0", "small", "MEM-1"),
            ("/dev/mem1", "large", "MEM-2"),
        )
    } else {
        (
            ("/dev/mem1", "large", "MEM-2"),
            ("/dev/mem0", "small", "MEM-1"),
        )
    };
    assert_eq!(
        display_lines(&lines),
        [line(drawing.2, "gpu")],
        "{lines:#?}"
    );
    assert!(
        s.until(BOUND, |a| head_shows(a, "small", "ready")
            && head_shows(a, "large", "ready")),
        "both sessions run"
    );
    for round in ["a", "b"] {
        let frames = s.memory(drawing.0).frames;
        s.type_on(drawing.1, &format!("{round}\r"));
        s.type_on(waiting.1, &format!("{round}\r"));
        let got = format!("got:{round}");
        assert!(
            s.until(BOUND, |a| head_shows(a, drawing.1, &got)
                && head_shows(a, waiting.1, &got)),
            "the loop took the keys"
        );
        assert!(
            s.until_memory(drawing.0, |m| m.frames > frames),
            "the other head drew them"
        );
    }
    assert_eq!(s.memory(waiting.0).frames, 0, "a frame of the held head");
    let mut lines = lines;
    lines.extend(logged());
    assert_eq!(
        display_lines(&lines[..]).len(),
        1,
        "a line for the held head"
    );
    let told = format!(
        "head {}: waiting for the GPU driver to get ready; the head shows nothing until then",
        waiting.1
    );
    assert_eq!(
        lines.iter().filter(|l| **l == told).count(),
        1,
        "{lines:#?}"
    );
    let _ = open.send(());
    assert!(holder.join().unwrap(), "the loop waited for the set-up");
    assert!(
        s.until_memory(waiting.0, |m| m.frames > 0),
        "the held head draws once its set-up ends"
    );
    let lines = s.lines_until(|l| !display_lines(l).is_empty());
    assert_eq!(
        display_lines(&lines),
        [line(waiting.2, "gpu")],
        "{lines:#?}"
    );
    assert_eq!(gpu.set_ups.lock().unwrap().len(), 2, "each display set up");
}

// ADR-1303 D9, D10: a display whose new mode passes the GPU's largest
// renderbuffer holds its head on the CPU path, with one line naming the
// limit, while the other head stays on the GPU path; a mode within the
// limit puts the head back
#[test]
fn a_mode_past_the_gpus_largest_renderbuffer_holds_only_its_own_head_on_the_cpu_path() {
    logged();
    let gpu = Arc::new(TestGpu::default());
    *gpu.widest.lock().unwrap() = Some(400);
    let mut s = Seat::boot(&gpu, Some(TWO_HEADS), two_heads(), ECHO);
    let lines = s.lines_until(|l| display_lines(l).len() == 2);
    let mut paths: Vec<String> = display_lines(&lines).into_iter().map(|l| l.1).collect();
    paths.sort();
    assert_eq!(paths, ["gpu", "gpu"], "{lines:#?}");
    s.settled();
    logged();
    s.memory("/dev/mem1").set_mode(640, 320);
    let mut lines = Vec::new();
    assert!(
        s.until(BOUND, |_| {
            lines.extend(logged());
            !display_lines(&lines).is_empty()
        }),
        "the held head's line"
    );
    let refused: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("the GPU path cannot set it up"))
        .collect();
    assert_eq!(refused.len(), 1, "{lines:#?}");
    assert!(
        refused[0].starts_with("display MEM-2: the GPU path cannot set it up at 640x320")
            && refused[0].contains("passes the GPU's largest renderbuffer, 400,"),
        "{lines:#?}"
    );
    assert_eq!(display_lines(&lines), [line("MEM-2", "cpu")], "{lines:#?}");
    // the other head draws on the GPU path meanwhile
    let drawn = gpu.drawn.load(SeqCst);
    let frames = s.memory("/dev/mem0").frames;
    s.type_on("small", "a\r");
    assert!(s.until_memory("/dev/mem0", |m| m.frames > frames));
    assert!(
        gpu.drawn.load(SeqCst) > drawn,
        "the small head on the GPU path"
    );
    s.memory("/dev/mem1").set_mode(400, 200);
    let mut lines = Vec::new();
    assert!(
        s.until(BOUND, |_| {
            lines.extend(logged());
            !display_lines(&lines).is_empty()
        }),
        "the large head's line again"
    );
    assert_eq!(display_lines(&lines), [line("MEM-2", "gpu")], "{lines:#?}");
}

// ADR-1303 D9: at vtium's exit, heads whose GPU driver hangs destroying
// their contexts are let go of together: each head's render thread is
// told to end first, all are waited for against one deadline, and the log
// says, for each head, that the GPU driver did not let go
#[test]
fn heads_whose_driver_hangs_letting_go_hold_the_exit_one_bound_in_all() {
    logged();
    let gpu = Arc::new(TestGpu::default());
    let mut s = Seat::boot(&gpu, Some(TWO_HEADS), two_heads(), ECHO);
    let lines = s.lines_until(|l| display_lines(l).len() == 2);
    assert_eq!(display_lines(&lines).len(), 2, "{lines:#?}");
    s.settled();
    let (holding, wait_holding) = std::sync::mpsc::channel();
    let (let_go, wait_let_go) = std::sync::mpsc::channel::<()>();
    let held_by = Arc::clone(&gpu);
    // holds both heads' drops for one and a half bounds: two waits one
    // after the other outlast it, and the test fails
    let holder = std::thread::spawn(move || {
        let _held = held_by.hold_drop.lock().unwrap();
        holding.send(()).unwrap();
        wait_let_go
            .recv_timeout(Duration::from_millis(vtium::painter::LET_GO_MS * 3 / 2))
            .is_ok()
    });
    wait_holding.recv().unwrap();
    // the seat would wait for the threads the test holds
    s.reap.0 = false;
    drop(s);
    assert_eq!(
        vtium::painter::wait_for_left(Duration::ZERO, &now),
        2,
        "both threads left, still held by the driver"
    );
    let _ = let_go.send(());
    assert_eq!(
        vtium::painter::wait_for_left(BOUND, &now),
        0,
        "the threads left end once the driver lets go"
    );
    let lines = logged();
    let mut held: Vec<&str> = lines
        .iter()
        .filter(|l| {
            l.ends_with(&format!(
                ": the GPU driver did not release the head within {} ms; vtium goes on without waiting",
                vtium::painter::LET_GO_MS
            ))
        })
        .map(|l| l.split(':').next().unwrap_or_default())
        .collect();
    held.sort_unstable();
    assert_eq!(held, ["head large", "head small"], "{lines:#?}");
    assert!(
        holder.join().unwrap(),
        "the heads were waited for one after the other"
    );
}
