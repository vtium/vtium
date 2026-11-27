//! A font set whose first source is a bitmap font with strikes, such as
//! the default set `terminus`, draws the strike of the height that a
//! configured font size or a zoom asks for. vtium reads no font itself, so
//! it asks the font helper for the cell at each height it draws at. The
//! helper is vtium-config's real serve mode, and the set's fonts are BDF
//! files the test writes, whose letter A fills its whole cell, so the ink
//! that vtium draws for A shows which strike drew it and where.
//!
//! References:
//! - ADR-1204 D2: the cell of the tallest strike no taller than the height,
//!   the next smaller strike centred in a cell between two strikes, and the
//!   32-pixel strike centred above 32 pixels.
//! - ADR-1200 D17: a configured font size and a zoom set the cell's height.

#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/helper.rs"]
mod stand_in;

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
use vtium_platform::display::{DisplayId, Dpms, Event, Rect, SwapError, Swapped, Token};
use vtium_platform::event_loop::Loop;
use vtium_platform::input::{InputEvent, KeyEvent, Mods};
use vtium_platform::vt::FakeVt;

use scratch::Scratch;
use stand_in::real_helper;

const ID: DisplayId = DisplayId(1);
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

/// How long a wait on the helper's answers and the render thread's frames
/// may run before the test fails instead of hanging.
const BOUND: Duration = Duration::from_secs(60);

/// The strikes of the test's set, as Terminus has them: width and height.
const STRIKES: [(u32, u32); 9] = [
    (6, 12),
    (7, 14),
    (8, 16),
    (10, 18),
    (10, 20),
    (11, 22),
    (12, 24),
    (14, 28),
    (16, 32),
];

#[expect(
    clippy::disallowed_methods,
    reason = "the harness runs the loop in real time"
)]
fn now() -> Instant {
    Instant::now()
}

/// A display in memory of [`WIDTH`] by [`HEIGHT`] pixels at 72 DPI, where
/// a point is a pixel, which wakes when the seat starts and takes every
/// frame.
struct Memory {
    drawn: Arc<Mutex<Vec<u8>>>,
    events: Vec<Event>,
    awake: bool,
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
        Some((WIDTH, HEIGHT))
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        Some(60_000)
    }
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        Some((72.0, 72.0))
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(id) {
            return None;
        }
        let memory = self.drawn.clone();
        Some(Token::new(id, (WIDTH, HEIGHT), WIDTH * 4, 0, memory))
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
        *self.drawn.lock().expect("the frame") = vec![0; (WIDTH * HEIGHT * 4) as usize];
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

/// A BDF font of `width`×`height` cells whose letter A lights every pixel
/// of its cell, its baseline one row above the cell's bottom.
fn full_bdf(width: u32, height: u32) -> String {
    let bytes = width.div_ceil(8);
    let lit = (1u64 << width) - 1;
    let row = format!(
        "{:0digits$X}\n",
        lit << (bytes * 8 - width),
        digits = (bytes * 2) as usize
    );
    format!(
        "STARTFONT 2.1\nFONT -test-\nFONTBOUNDINGBOX {width} {height} 0 -1\n\
         STARTPROPERTIES 3\nFONT_ASCENT {}\nFONT_DESCENT 1\n\
         CHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 1\n\
         STARTCHAR A\nENCODING 65\nDWIDTH {width} 0\nBBX {width} {height} 0 -1\nBITMAP\n{}ENDCHAR\nENDFONT\n",
        height - 1,
        row.repeat(height as usize)
    )
}

/// A frits' directory holding the set `t`, shaped as `terminus` is: the
/// 16-pixel strike as its source, then the eight other strikes.
fn struck_set(name: &str) -> Scratch {
    let dir = Scratch::new(name);
    let mut text = String::new();
    for (key, (w, h)) in std::iter::once(("source", (8, 16))).chain(
        STRIKES
            .iter()
            .filter(|&&(_, h)| h != 16)
            .map(|&s| ("strike", s)),
    ) {
        let path = dir.join(format!("{h}.bdf"));
        std::fs::write(&path, full_bdf(w, h)).expect("the strike");
        text.push_str(&format!("{key}=bdf:{}\n", path.display()));
    }
    std::fs::write(dir.join("t.vtfrit"), text).expect("the set");
    dir
}

struct Seat {
    lp: Loop<App>,
    app: App,
    _dir: Scratch,
}

impl Seat {
    /// The seat drawing the set `t` at `--font-size=<size>`, its session
    /// printing one A at the top left. Its first turn has run.
    fn boot(test: &str, size: u32) -> Seat {
        Seat::boot_with(test, size, &[], "printf A; exec sleep 30")
    }

    /// The seat drawing the set `t` at `--font-size=<size>`, with the
    /// options `more`, its session running `script` under `/bin/sh`. Its
    /// first turn has run.
    fn boot_with(test: &str, size: u32, more: &[&str], script: &str) -> Seat {
        let dir = struck_set(test);
        let kinds = dir.join("kinds");
        let fonts = Sources {
            frits: dir.to_path_buf(),
            trust: vtium::font::trust().below(&dir),
            helper: Some(real_helper(&kinds, None)),
            vconsole_conf: PathBuf::from("/dev/null"),
            kernel_cmdline: PathBuf::from("/dev/null"),
        };
        let size = format!("--font-size={size}");
        let args: Vec<OsString> = [size.as_str(), "--font-name=t"]
            .iter()
            .chain(more)
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
        app.set_font_sources(fonts);
        let memory = Memory {
            drawn: Arc::default(),
            events: Vec::new(),
            awake: false,
        };
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(memory));
        app.vt.activate().expect("the fake VT");
        app.start();
        app.turn(now());
        Seat { lp, app, _dir: dir }
    }

    /// Turns the loop until `done` holds, at most [`BOUND`].
    fn until(&mut self, mut done: impl FnMut(&mut Seat) -> bool) -> bool {
        let end = now() + BOUND;
        while now() < end {
            if done(self) {
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
        done(self)
    }

    /// The box the ink of the grid's top-left cell covers, in a grid of
    /// cells `cell` centred on the display, as its left column and top row
    /// within the cell, and its width and height in pixels. The background
    /// is the display's bottom-right pixel, which no text reaches.
    fn ink(&mut self, cell: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
        let memory = self
            .app
            .videos
            .get_mut(Path::new("/dev/mem0"))
            .and_then(|v| v.as_any().downcast_mut::<Memory>())
            .expect("the memory device");
        let drawn = memory.drawn.lock().expect("the frame");
        let pixel = |x: u32, y: u32| {
            let at = ((y * WIDTH + x) * 4) as usize;
            drawn.get(at..at + 4).map(<[u8]>::to_vec)
        };
        let paper = pixel(WIDTH - 1, HEIGHT - 1)?;
        let (x0, y0) = ((WIDTH % cell.0) / 2, (HEIGHT % cell.1) / 2);
        let lit: Vec<(u32, u32)> = (0..cell.1)
            .flat_map(|y| (0..cell.0).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(x0 + x, y0 + y).is_some_and(|p| p != paper))
            .collect();
        let left = lit.iter().map(|p| p.0).min()?;
        let top = lit.iter().map(|p| p.1).min()?;
        let right = lit.iter().map(|p| p.0).max()?;
        let bottom = lit.iter().map(|p| p.1).max()?;
        Some((left, top, right - left + 1, bottom - top + 1))
    }

    /// The helper's answer to the set's cell query at `height`, as the
    /// strike's width and height, else why there is none.
    fn answer(&self, height: u32) -> Option<Result<(u32, u32), String>> {
        let answers = self.app.frit_cells("t");
        let (_, answer) = answers.iter().find(|(at, _)| *at == height)?;
        Some(match answer {
            Ok(vtium::helper::OwnCell::Bitmap(c)) => Ok((c.width, c.height)),
            other => Err(format!("{other:?}")),
        })
    }

    /// Waits until the head draws in `cell` and its top-left A covers
    /// `ink`, and fails naming what it drew instead.
    fn draws(&mut self, cell: (u32, u32), ink: (u32, u32, u32, u32), what: &str) {
        let mut seen = (Vec::new(), None);
        let drawn = self.until(|s| {
            seen = (s.app.cells(), s.ink(cell));
            seen.0 == [cell] && seen.1 == Some(ink)
        });
        assert!(
            drawn,
            "{what}: want the cell {cell:?} and A's ink {ink:?}, got {seen:?}"
        );
    }
}

/// The key `key` with `mods` held: Ctrl-= zooms in, and Ctrl-- zooms out.
fn zoom(mods: Mods, key: char) -> InputEvent {
    InputEvent::Key(KeyEvent {
        keycode: 0,
        ascii: 0,
        unshifted: 0,
        shifted: 0,
        base: None,
        mods,
        keysyms: vec![u32::from(key)],
        codepoints: vec![None],
        repeat: false,
    })
}

/// A configured font size of 24 pixels gives the set a 12×24 cell, which
/// its 24-pixel strike fills. With 20 pixels the cell is 10×20 and the
/// 20-pixel strike fills it. The console asked the helper for the cell at
/// the configured height.
#[test]
fn a_font_size_draws_the_strike_of_its_height() {
    for (size, (w, h)) in [(24, (12, 24)), (20, (10, 20))] {
        let mut s = Seat::boot(&format!("strike-size-{size}"), size);
        s.draws((w, h), (0, 0, w, h), &format!("font-size={size}"));
        assert_eq!(s.answer(size), Some(Ok((w, h))), "the strike's cell");
    }
}

/// A configured font size of 40 pixels, taller than the tallest strike,
/// gives the set a cell 40 pixels high and as wide as the 32-pixel strike,
/// which draws at factor 1 centred in it, 4 rows down. The console asked
/// the helper for the cell at 40 pixels, and the helper answered with the
/// 32-pixel strike's cell.
#[test]
fn a_font_size_above_the_tallest_strike_draws_it_centred() {
    let mut s = Seat::boot("strike-size-40", 40);
    s.draws((16, 40), (0, 4, 16, 32), "font-size=40");
    assert_eq!(s.answer(40), Some(Ok((16, 32))), "the strike's cell");
}

/// From a font size of 16 pixels, and from the automatic size, which is
/// 16 pixels on this display, each zoom in steps to the next strike's
/// height, 18, 20, 22 and 24, and the 24-pixel strike fills the 12×24 cell
/// it reaches. A zoom out steps back to the 22-pixel strike.
#[test]
fn a_zoom_steps_through_the_strikes() {
    for size in [16, 0] {
        let mut s = Seat::boot(&format!("strike-zoom-{size}"), size);
        s.draws((8, 16), (0, 0, 8, 16), &format!("font-size={size}"));
        for (w, h) in [(10, 18), (10, 20), (11, 22), (12, 24)] {
            s.app.input_event(zoom(Mods::CTRL, '='), now());
            s.draws((w, h), (0, 0, w, h), &format!("{size}, zoomed in to {h}"));
        }
        s.app.input_event(zoom(Mods::CTRL, '-'), now());
        s.draws(
            (11, 22),
            (0, 0, 11, 22),
            &format!("{size}, zoomed out to 22"),
        );
    }
}

/// ctlseqs, XTWINOPS 8: a program asks for a text area in rows and
/// columns, and vtium meets the request at the largest height whose grid
/// holds it. For a set with strikes, those heights are the strikes'
/// heights. On the 640×480 display, 21 rows of 58 columns are the grid of
/// the 22-pixel strike's 11×22 cell, so a request from 16 pixels draws
/// that strike. 26 rows of 64 columns are the grid of the 18-pixel
/// strike's 10×18 cell, so a request from 24 pixels steps down to it.
// covers: window.resize
#[test]
fn a_text_area_request_draws_the_strike_whose_grid_holds_it() {
    for (size, rows, cols, (w, h)) in [(16, 21, 58, (11, 22)), (24, 26, 64, (10, 18))] {
        let script = format!("printf 'A\\033[8;{rows};{cols}t'; exec sleep 30");
        let mut s = Seat::boot_with(
            &format!("strike-window-{size}"),
            size,
            &["--allow-window-ops"],
            &script,
        );
        let what = format!("{rows} rows of {cols} columns from {size} pixels");
        s.draws((w, h), (0, 0, w, h), &what);
        assert_eq!(s.app.grid(), (cols, rows), "{what}: the grid");
    }
}

/// Above the tallest strike, a zoom in steps to the next whole multiple of
/// the 32-pixel strike, which draws at factor 2 and fills the 32×64 cell.
/// vtium asks the helper once for that step, at 64 pixels, and not at each
/// height from 33 to 64.
#[test]
fn a_zoom_above_the_tallest_strike_asks_the_helper_once() {
    let mut s = Seat::boot("strike-zoom-once", 32);
    s.draws((16, 32), (0, 0, 16, 32), "font-size=32");
    let asked = s.app.frit_cells("t").len();
    s.app.input_event(zoom(Mods::CTRL, '='), now());
    s.draws((32, 64), (0, 0, 32, 64), "32, zoomed in to 64");
    let heights: Vec<u32> = s.app.frit_cells("t").iter().map(|(at, _)| *at).collect();
    assert_eq!(
        heights.len(),
        asked + 1,
        "one ask for the step: {heights:?}"
    );
    assert!(
        heights.contains(&64),
        "the ask is at 64 pixels: {heights:?}"
    );
}

/// A program asks for a text area of one row of one column, which the
/// 640×480 display holds at every height. vtium climbs from 16 pixels
/// through each strike, two pixels apart, and then through the whole
/// multiples of the 32-pixel strike up to 160 pixels, the first step above
/// the largest height a zoom in starts from. Each step asks the helper at
/// the next whole multiple of the strike in use and at the heights just
/// above `from`, 26 asks in all, where asking at each pixel from 17 to 160
/// takes 145.
#[test]
fn a_text_area_request_asks_the_helper_at_few_heights() {
    let mut s = Seat::boot_with(
        "strike-window-few",
        16,
        &["--allow-window-ops"],
        "printf 'A\\033[8;1;1t'; exec sleep 30",
    );
    assert!(s.until(|s| s.app.grid() == (1, 1)), "the grid");
    let heights: Vec<u32> = s.app.frit_cells("t").iter().map(|(at, _)| *at).collect();
    assert!(
        heights.len() <= 26 && heights.contains(&160),
        "{} asks, at {heights:?}",
        heights.len()
    );
}
