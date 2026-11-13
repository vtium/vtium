//! One grid across two displays (ADR-1401, D1's mirrored head), with a
//! picture each display draws (ADR-1100 D5): the seat booted with two
//! displays in memory, on the fake VT, with a real child on a real pty.
//! The child places a kitty picture, and the tests read what each display
//! shows and count the frames each one draws.
//!
//! Each display keeps the scaled copies of the pictures it draws in a
//! cache of its own, bounded by its own screen, and a copy past that bound
//! is scaled for each frame that draws it (ADR-1100 D5). So what one
//! display draws never decides when another display can draw a picture,
//! and no display waits on another display's frame or on input.
#![cfg(feature = "images")]

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
use vtium::video::Video;
use vtium_platform::display::{
    DisplayId, Dpms, Event, Identity, Memory as Shared, Rect, SwapError, Swapped, Token,
};
use vtium_platform::event_loop::Loop;
use vtium_platform::vt::FakeVt;

const ID: DisplayId = DisplayId(5);

/// How long a wait on the child or a frame may run before the test fails:
/// only a bound, since each wait ends at the state it waits for.
const BOUND: Duration = Duration::from_secs(10);

/// Pure red, the color of the picture the child places.
const RED: u32 = 0x00ff_0000;

/// A device with one display, its frame buffer in memory. It holds the
/// seat to what a framebuffer holds it to: one token lent at a time, a
/// token from before a sleep or a wake-up comes back too late and is
/// dropped, a dropped token leaves the buffer at age 0, and a frame that
/// changed nothing is not shown.
struct Memory {
    w: u32,
    h: u32,
    connector: &'static str,
    card: &'static str,
    /// What the display shows: the buffer lent, as the last swap left it.
    pixels: Vec<u8>,
    /// The buffer each token lends, drawn into on the render thread.
    drawn: Arc<Mutex<Vec<u8>>>,
    events: Vec<Event>,
    awake: bool,
    dpms: Option<Dpms>,
    /// The frames the display has shown.
    frames: u32,
    age: u32,
    lease: u64,
    lent: bool,
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
            events: Vec::new(),
            awake: false,
            dpms: None,
            frames: 0,
            age: 0,
            lease: 0,
            lent: false,
        }
    }

    /// The pixels the display shows of `rgb`.
    fn count(&self, rgb: u32) -> usize {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]) & 0x00ff_ffff == rgb)
            .count()
    }

    /// The display's pixels.
    fn area(&self) -> usize {
        (self.w * self.h) as usize
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
        self.connector.into()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some((self.w, self.h))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        // 72 DPI, where a point is a pixel
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
        self.dpms = Some(dpms);
        if dpms == Dpms::On {
            self.age = 0;
            self.events.push(Event::Redraw(ID));
        }
        Ok(())
    }
    fn sleep(&mut self) {
        self.lease += 1;
        self.awake = false;
        self.events.push(Event::Asleep);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        self.lease += 1;
        let first = self.pixels.is_empty();
        self.pixels = vec![0; (self.w * self.h * 4) as usize];
        if Arc::strong_count(&self.drawn) == 1 {
            *self.drawn.lock().unwrap() = self.pixels.clone();
        }
        self.awake = true;
        self.dpms = Some(Dpms::On);
        self.age = 0;
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

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// No frits and no vconsole font, so the seat draws its built-in font.
fn no_fonts() -> Sources {
    Sources {
        frits: PathBuf::from("/nonexistent/vtium-test-frits"),
        trust: vtium::font::trust(),
        helper: None,
        vconsole_conf: PathBuf::from("/dev/null"),
        kernel_cmdline: PathBuf::from("/dev/null"),
    }
}

/// The small display and the large one, each its own device. With no head
/// file they mirror one grid of 40×10 cells: 8×16 pixels on the small
/// display and 16×32 on the large one.
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
    /// The seat booted with `args` at 16 points, 16-pixel cells at 72 DPI,
    /// on `displays` with no head file, its session running `script`.
    fn boot(
        args: &[&str],
        displays: impl IntoIterator<Item = (&'static str, Memory)>,
        script: &str,
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
        let mut app = App::new(
            loaded.config,
            Box::new(FakeVt::new()),
            lp.handle(),
            Rc::new(now),
            env,
        );
        app.set_font_sources(no_fonts());
        for (node, memory) in displays {
            app.add_video(PathBuf::from(node), Box::new(memory));
        }
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat { lp, app }
    }

    /// One turn of the loop: what is ready is dispatched, then the turn.
    fn turn(&mut self) {
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

    /// Turns the loop until `done` holds, at most [`BOUND`].
    fn until(&mut self, mut done: impl FnMut(&mut Seat) -> bool) -> bool {
        let end = now() + BOUND;
        while now() < end {
            if done(self) {
                return true;
            }
            self.turn();
        }
        done(self)
    }

    fn memory(&mut self, node: &str) -> &mut Memory {
        self.app
            .videos
            .get_mut(Path::new(node))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("a memory device")
    }

    /// The frames each display has shown.
    fn frames(&mut self) -> [u32; 2] {
        [
            self.memory("/dev/mem0").frames,
            self.memory("/dev/mem1").frames,
        ]
    }

    /// Whether `node` shows the picture over the whole display.
    fn all_red(&mut self, node: &str) -> bool {
        let m = self.memory(node);
        m.count(RED) == m.area()
    }
}

/// The kitty picture the child places: 4×2 pixels of pure red, scaled to
/// a box of 80×20 cells from the top left, with the cursor left where it
/// was. vtium keeps a kitty picture's shape in its box, and the picture
/// and the box are both twice as wide as they are tall, so the picture
/// fills the box. The box passes the 40×10 grid's right and bottom edges,
/// so the picture covers every cell.
const PICTURE: &str =
    r"\033_Ga=T,f=32,s=4,v=2,c=80,r=20,C=1,q=2;/wAA//8AAP//AAD//wAA//8AAP//AAD//wAA//8AAP8=\033\\";

/// The bytes the whole picture takes at its drawn size on a display whose
/// character cell is `w` by `h` pixels: the box of 80×20 cells, at 4 bytes
/// a pixel.
fn picture_bytes((w, h): (u64, u64)) -> u64 {
    80 * w * 20 * h * 4
}

/// The bytes the scaled copies of a display `w` by `h` pixels may hold
/// (ADR-1100 D5).
fn cache_bytes(w: u64, h: u64) -> u64 {
    w * h * 4 * vtium_render::PICTURE_CACHE_SCREENS as u64
}

/// ADR-1401 D1, ADR-1100 D5: two displays mirror one grid, each at its own
/// cell size, and a picture the program places shows on both of them with
/// no key pressed. The picture's scaled copy does not fit in either
/// display's cache. So each display scales the picture again for every
/// frame that draws it, and no display waits for room in a cache. Both
/// displays show the whole picture after the frames the program's output
/// draws. Then both displays stop drawing: neither display's picture makes
/// vtium draw another frame on either display.
// covers: platform.multi-monitor
#[test]
fn each_display_of_a_mirrored_head_draws_a_picture_past_its_cache_with_no_input() {
    // the copy is past the cache of both displays
    assert!(picture_bytes((8, 16)) > cache_bytes(320, 160));
    assert!(picture_bytes((16, 32)) > cache_bytes(640, 320));
    let script = format!("stty -echo; printf '\\033[?25l{PICTURE}'; read x");
    let mut s = Seat::boot(&["--no-blink"], two_displays(), &script);
    assert!(s.until(|s| s.app.cells().len() == 2), "both displays lit");
    assert_eq!(s.app.grid(), (40, 10));
    assert_eq!(s.app.cells(), [(8, 16), (16, 32)]);
    assert!(
        s.until(|s| s.all_red("/dev/mem0") && s.all_red("/dev/mem1")),
        "the small display shows {} red pixels of {}, the large one {} of {}",
        s.memory("/dev/mem0").count(RED),
        s.memory("/dev/mem0").area(),
        s.memory("/dev/mem1").count(RED),
        s.memory("/dev/mem1").area(),
    );
    // Quiet: with no input and no output, ten turns of the loop send no
    // frame and show no frame on either display.
    assert!(
        s.until(|s| {
            let frames = s.frames();
            s.app.heads()[0].frames_out() == 0
                && (0..10).all(|_| {
                    s.turn();
                    s.app.heads()[0].frames_out() == 0 && s.frames() == frames
                })
        }),
        "the displays settle"
    );
    assert!(s.all_red("/dev/mem0") && s.all_red("/dev/mem1"));
}
