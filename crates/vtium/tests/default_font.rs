//! The font vtium draws when no font is named: the default set `terminus`
//! where it is installed, the built-in font on a small display, and the
//! built-in font with one log line where no `terminus` set is installed.
//! Each test boots the seat on the fake VT with one display in memory,
//! with no vtium.conf, no vconsole font and no kernel command line.
//!
//! References:
//! - ADR-1204 D2: the order in which vtium chooses a font, with `terminus`
//!   after vconsole's font and before the built-in font, and the built-in
//!   font's 8×8 strike on a display where the small-display rule picks
//!   8-pixel rows.

#[path = "common/scratch.rs"]
mod scratch;

use std::any::Any;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use vtium::app::App;
use vtium::config;
use vtium::font::Sources;
use vtium::video::Video;
use vtium_platform::display::{DisplayId, Dpms, Event, Rect, SwapError, Swapped, Token};
use vtium_platform::event_loop::Loop;
use vtium_platform::vt::FakeVt;

use scratch::Scratch;

const ID: DisplayId = DisplayId(1);

/// A display in memory of `w` by `h` pixels, which wakes when the seat
/// starts and takes every frame.
struct Memory {
    w: u32,
    h: u32,
    drawn: Arc<Mutex<Vec<u8>>>,
    events: Vec<Event>,
    awake: bool,
}

impl Memory {
    fn sized(w: u32, h: u32) -> Memory {
        Memory {
            w,
            h,
            drawn: Arc::default(),
            events: Vec::new(),
            awake: false,
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
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((96.0, 96.0))
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        let memory = self.drawn.clone();
        Some(Token::new(id, (self.w, self.h), self.w * 4, 0, memory))
    }
    fn give_back(&mut self, _: Token) {}
    fn swap(&mut self, _: Token, _: &[Rect]) -> Result<Swapped, SwapError> {
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        self.awake && Arc::strong_count(&self.drawn) == 1
    }
    fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
        Ok(())
    }
    fn sleep(&mut self) {
        self.awake = false;
    }
    fn wake_up(&mut self) -> io::Result<()> {
        *self.drawn.lock().unwrap() = vec![0; (self.w * self.h * 4) as usize];
        self.awake = true;
        self.events.extend([Event::Added(ID), Event::Awake]);
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

/// The font sets in `frits`, and no vconsole font.
fn sources(frits: &Path) -> Sources {
    Sources {
        frits: frits.to_path_buf(),
        trust: vtium::font::trust().below(frits),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

/// The seat booted with no font named on a display of `w` by `h`, its font
/// sets in `frits`, and the lines it logged while it started. The font
/// helper is a path where none is, so no helper starts.
fn boot(w: u32, h: u32, frits: &Path) -> (Loop<App>, App, Vec<String>) {
    logged();
    let args: Vec<OsString> = [
        "--font-helper=/nonexistent/vtium-config",
        "--login",
        "--",
        "/bin/true",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let loaded = config::load(&args, &mut |_: &Path| Ok(None));
    let lp: Loop<App> = Loop::new().expect("a loop");
    #[expect(
        clippy::disallowed_methods,
        reason = "the seat's clock; nothing here waits on it"
    )]
    let clock = Rc::new(Instant::now);
    let mut app = App::new(
        loaded.config,
        Box::new(FakeVt::new()),
        lp.handle(),
        clock,
        Vec::new(),
    );
    app.set_font_sources(sources(frits));
    app.add_video(PathBuf::from("/dev/mem0"), Box::new(Memory::sized(w, h)));
    app.vt.activate().expect("the fake VT");
    app.start();
    #[expect(clippy::disallowed_methods, reason = "one turn of the seat")]
    app.turn(Instant::now());
    let lines = logged();
    (lp, app, lines)
}

/// A frits' directory holding the catalog's `terminus` set, which names a
/// font file that is not there.
fn with_terminus(name: &str) -> Scratch {
    let dir = Scratch::new(name);
    std::fs::create_dir_all(dir.join("console")).expect("the folder");
    std::fs::write(
        dir.join("console/terminus.vtfrit"),
        "source=bdf:/nonexistent/ter-u16n.bdf.gz\n",
    )
    .expect("the set");
    dir
}

/// The lines that name the default set.
fn about_default(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|l| l.contains("terminus")).collect()
}

/// On a display of ordinary size, with no font named, vtium draws the
/// default set `terminus` from the catalog's console folder.
#[test]
fn with_no_font_named_an_ordinary_display_draws_terminus() {
    let dir = with_terminus("default-font-ordinary");
    let (_lp, app, _) = boot(640, 480, &dir);
    assert_eq!(app.font_name(), "terminus");
}

/// On a display where the small-display rule picks 8-pixel rows, such as
/// a 320×240 panel, vtium draws the built-in font's 8×8 strike, even with
/// `terminus` installed.
#[test]
fn with_no_font_named_a_small_display_draws_the_built_in_8x8_strike() {
    let dir = with_terminus("default-font-small");
    let (_lp, app, lines) = boot(320, 240, &dir);
    assert_eq!(app.font_name(), "unscii-8");
    assert_eq!(about_default(&lines), Vec::<&String>::new(), "{lines:?}");
}

/// With no `terminus` set installed, vtium logs one line that says so and
/// draws its built-in font.
#[test]
fn with_no_terminus_installed_vtium_logs_one_line_and_draws_the_built_in_font() {
    let dir = Scratch::new("default-font-none");
    let (_lp, app, lines) = boot(640, 480, &dir);
    assert_eq!(app.font_name(), "unifont");
    assert_eq!(
        about_default(&lines),
        [
            "No font is named, and the default font set terminus is not installed. vtium draws its built-in font instead."
        ],
        "{lines:?}"
    );
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
