//! The ways a view draws a scroll, `scroll-draw`'s pan, copy and redraw,
//! on a stand-in display that makes the buffers a view asks for, counts
//! the pixels each frame writes, and shows the window each swap commits.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "a test's sizes are small, its clock is the host's, and a test fails by panicking"
)]

use std::any::Any;
use std::io;
use std::sync::Mutex;
use std::time::Duration;

use vtium_font::Unifont;
use vtium_platform::display::{Dpms, Event, Memory, Rect, Tall};

use super::*;

const ID: DisplayId = DisplayId(1);
/// What a lent buffer holds before the frame, so that the pixels the frame
/// writes are the ones that no longer hold it.
const UNWRITTEN: [u8; 4] = [0x5a, 0xa5, 0x3c, 0xc3];
/// The cell of the built-in GNU Unifont at 16 pixels.
const CELL: (u32, u32) = (8, 16);

/// A display that makes the buffers a view asks for, as drm2d does: taller
/// than its screen, unless it refuses the pan, and else the screen's size.
/// Like drm2d it keeps one window for all its buffers: the window the last
/// frame swapped carries, and the window the screen shows once that frame
/// is sent. It lends the buffers it holds as tall ones only while they are
/// the buffers last asked for.
struct Panning {
    screen: (u32, u32),
    /// The size of the buffers it holds, and whether their window moves
    /// across them.
    size: (u32, u32),
    turned: bool,
    refuses: bool,
    /// Each ask, in order.
    asked: Vec<Option<Tall>>,
    /// The ask in force.
    ask: Option<Tall>,
    /// It keeps the buffers it holds after an ask until [`Panning::arrive`],
    /// as drm2d does while it is asleep, blanked or waiting on a flip, or
    /// after a modeset that failed.
    defers: bool,
    /// Unlike drm2d, it lends the buffers it holds as tall ones until the
    /// new ones arrive, which [`Video::set_tall`] allows a display to do.
    stale_as_tall: bool,
    /// Each frame it sends starts a flip that lasts until
    /// [`Panning::flip_done`]. While a flip is in flight a swap is refused as
    /// busy and its frame kept, and with two buffers it lends no buffer.
    flips: bool,
    flipping: bool,
    /// A frame drawn and kept that waits to be sent.
    fresh: bool,
    bufs: Vec<Arc<Mutex<Vec<u8>>>>,
    /// Each buffer's frame number, `None` while it holds no frame.
    drawn: Vec<Option<u32>>,
    frames: u32,
    back: usize,
    /// Counts the buffers made: a token lent before the last were made is
    /// dropped at its swap.
    lease: u64,
    /// What the back buffer held before it was lent.
    before: Vec<u8>,
    /// The window the last frame swapped carries, and the one the screen
    /// shows.
    window: u32,
    shown: u32,
    events: Vec<Event>,
    /// The pixels each frame swapped wrote, and its window.
    written: Vec<(usize, u32)>,
}

impl Panning {
    fn new(screen: (u32, u32), buffers: usize) -> Panning {
        let mut p = Panning {
            screen,
            size: screen,
            turned: false,
            refuses: false,
            asked: Vec::new(),
            ask: None,
            defers: false,
            stale_as_tall: false,
            flips: false,
            flipping: false,
            fresh: false,
            bufs: Vec::new(),
            drawn: Vec::new(),
            frames: 0,
            back: 0,
            lease: 0,
            before: Vec::new(),
            window: 0,
            shown: 0,
            events: Vec::new(),
            written: Vec::new(),
        };
        p.make(buffers);
        p
    }

    fn make(&mut self, n: usize) {
        let len = (self.size.0 * self.size.1 * 4) as usize;
        self.bufs = (0..n).map(|_| Arc::new(Mutex::new(vec![0; len]))).collect();
        self.drawn = vec![None; n];
        self.back = 0;
        self.lease += 1;
        self.window = 0;
        self.shown = 0;
        self.fresh = false;
    }

    /// The tall buffers it makes: those asked for, unless it refuses them.
    fn making(&self) -> Option<Tall> {
        self.ask.filter(|t| t.ring > 0 && !self.refuses)
    }

    /// Whether the buffers it holds are the tall buffers last asked for.
    fn holds_tall(&self) -> bool {
        self.making()
            .is_some_and(|t| self.size == t.buffer(self.screen))
    }

    fn tall(&self) -> bool {
        self.size != self.screen
    }

    /// Makes the buffers last asked for, as drm2d's modeset does: the
    /// window goes back to the buffer's start, and the owner hears that it
    /// redraws.
    fn arrive(&mut self) {
        let made = match self.making() {
            Some(t) => (t.buffer(self.screen), t.turned),
            None => (self.screen, false),
        };
        if made != (self.size, self.turned) {
            (self.size, self.turned) = made;
            let n = self.bufs.len();
            self.make(n);
            self.events.push(Event::Redraw(ID));
        }
    }

    /// The flip in flight ends.
    fn flip_done(&mut self) {
        self.flipping = false;
    }

    /// Sends the frame kept, as drm2d's commit does: refused as busy while
    /// a flip is in flight.
    fn send(&mut self) -> Result<Swapped, SwapError> {
        if self.flipping {
            return Err(SwapError::Busy);
        }
        if !self.fresh {
            return Ok(Swapped::Nothing);
        }
        self.fresh = false;
        self.shown = self.window;
        self.back = (self.back + 1) % self.bufs.len();
        self.flipping = self.flips;
        Ok(Swapped::Frame)
    }

    /// What the screen shows: the window of the buffer sent last.
    fn shown(&self) -> Vec<u8> {
        let front = (self.back + self.bufs.len() - 1) % self.bufs.len();
        let pixels = self.bufs[front].lock().unwrap();
        let (w, h) = self.screen;
        let stride = self.size.0 as usize * 4;
        let window = if self.tall() { self.shown as usize } else { 0 };
        let (x0, y0) = if self.turned {
            (window, 0)
        } else {
            (0, window)
        };
        (0..h as usize)
            .flat_map(|y| {
                let at = (y0 + y) * stride + x0 * 4;
                pixels[at..at + w as usize * 4].to_vec()
            })
            .collect()
    }

    /// Puts back what the back buffer held where the frame wrote nothing,
    /// and gives the pixels it wrote and those of them that lie outside
    /// `damage`, in the buffer's coordinates.
    fn count_written(&mut self, damage: &[Rect]) -> (usize, usize) {
        let width = self.size.0 as usize;
        let mut pixels = self.bufs[self.back].lock().unwrap();
        let (mut written, mut outside) = (0, 0);
        for (i, (px, old)) in pixels.chunks_mut(4).zip(self.before.chunks(4)).enumerate() {
            if px == UNWRITTEN {
                px.copy_from_slice(old);
                continue;
            }
            written += 1;
            let (x, y) = ((i % width) as u32, (i / width) as u32);
            let inside = damage
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
            outside += usize::from(!inside);
        }
        (written, outside)
    }
}

impl Video for Panning {
    fn backend(&self) -> &'static str {
        "panning"
    }
    fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    fn displays(&self) -> Vec<DisplayId> {
        vec![ID]
    }
    fn name(&self, _: DisplayId) -> String {
        "PAN-1".to_owned()
    }
    fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
        Some(self.screen)
    }
    fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
        None
    }
    // As drm2d, it lends no buffer while the one to draw into waits for a
    // flip: with two buffers, the back one is on screen until the flip
    // ends, and a frame kept waits for its flip whatever the buffers.
    fn lend(&mut self, _: DisplayId) -> Option<Token> {
        if !self.ready_for_frame(ID) {
            return None;
        }
        let age = self.drawn[self.back].map_or(0, |n| self.frames - n + 1);
        let mut pixels = self.bufs[self.back].lock().unwrap();
        self.before = pixels.clone();
        for px in pixels.chunks_mut(4) {
            px.copy_from_slice(&UNWRITTEN);
        }
        drop(pixels);
        let memory: Arc<Memory> = self.bufs[self.back].clone();
        let token = Token::new(ID, self.size, self.size.0 * 4, age, memory).leased(self.lease);
        if !(self.holds_tall() || (self.stale_as_tall && self.tall())) {
            return Some(token);
        }
        let mut token = token.tall(self.screen, self.turned);
        assert!(token.set_window(self.window.min(token.last_window())));
        Some(token)
    }
    fn give_back(&mut self, token: Token) {
        if token.lease() != self.lease {
            return;
        }
        let (written, _) = self.count_written(&[]);
        assert_eq!(written, 0, "a buffer given back unused");
    }
    // Every pixel a frame writes lies in its damage, as the display
    // presents only the damage. A token lent before the buffers were made
    // again is dropped, and every buffer is then age 0.
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        if token.lease() != self.lease {
            self.drawn.fill(None);
            return Err(SwapError::Dropped);
        }
        let (written, outside) = self.count_written(damage);
        assert_eq!(outside, 0, "pixels written outside the damage {damage:?}");
        let tall = token.size() != token.screen();
        if tall {
            self.window = token.window().min(token.last_window());
        }
        let moved = tall && self.window != self.shown;
        if !damage.is_empty() || moved {
            self.frames += 1;
            self.drawn[self.back] = Some(self.frames);
            self.fresh = true;
            self.written.push((written, self.window));
        }
        self.send()
    }
    fn commit(&mut self, _: DisplayId) -> Result<Swapped, SwapError> {
        self.send()
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        self.flipping
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        !self.flipping || (self.bufs.len() != 2 && !self.fresh)
    }
    fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
        Ok(())
    }
    fn sleep(&mut self) {}
    fn wake_up(&mut self) -> io::Result<()> {
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
    // As drm2d, it takes the ask at once, so the buffers it holds stop
    // being lent as tall ones, and makes the new buffers at once unless it
    // defers them.
    fn set_tall(&mut self, _: DisplayId, tall: Option<Tall>) -> io::Result<()> {
        self.asked.push(tall);
        if self.ask == tall {
            return Ok(());
        }
        self.ask = tall;
        if !self.defers {
            self.arrive();
        }
        Ok(())
    }
}

/// A terminal of `screen`'s size in cells and a view of it that draws a
/// scroll as `scroll` says, turned as `orientation` says.
fn view(screen: (u32, u32), scroll: ScrollDraw, orientation: Orientation) -> (Term, View) {
    let turned = matches!(orientation, Orientation::Right | Orientation::Left);
    let (across, down) = if turned { (screen.1, screen.0) } else { screen };
    let (cols, rows) = ((across / CELL.0) as usize, (down / CELL.1) as usize);
    let clock: Clock = Rc::new(Instant::now);
    let term = Term::new(
        Terminal::new(cols, rows, 100),
        Sync::new(Duration::ZERO, 0),
        Blink::new(false),
        clock,
    );
    let face: DisplayFace = Box::new(Unifont::new(16));
    let mut view = View::new(
        ID,
        Font::new(face),
        screen,
        None,
        RenderConfig {
            orientation,
            ..RenderConfig::default()
        },
        false,
        PointerFile::default(),
    );
    view.scroll = scroll;
    (term, view)
}

fn feed(term: &mut Term, bytes: &[u8]) {
    term.parse(bytes, &mut |_| {});
}

fn draw(term: &mut Term, view: &mut View, video: &mut dyn Video) {
    view.draw(term, video, &DrawState::default(), &Instant::now);
}

/// The way the view's renderer drew its last frame.
fn way(view: &View) -> Option<Way> {
    view.drawing.as_ref().and_then(|d| d.way)
}

/// A screen full of numbered lines, the cursor on the bottom row.
fn fill(term: &mut Term, rows: usize) {
    for n in 0..rows {
        let sep = if n + 1 == rows { "" } else { "\r\n" };
        feed(term, format!("line {n} of the screen{sep}").as_bytes());
    }
}

// With the pan, a line feed on the bottom row writes the row that came in
// and moves the window by a row: one row into a buffer one frame old, and
// the two frames' rows into one two frames old, each row in its two copies
// where it has two. Drawn again, every row is written.
//
// References:
// - ADR-1310 D6: the pan over a tall buffer draws only new rows.
#[test]
fn a_line_feed_on_the_bottom_row_draws_one_row_over_the_pan() {
    let screen = (160, 96);
    let (row, cell) = ((screen.0 * CELL.1) as usize, (CELL.0 * CELL.1) as usize);
    let whole = (screen.0 * screen.1) as usize;
    for buffers in [1, 2] {
        let mut frames = Vec::new();
        for scroll in [ScrollDraw::Auto, ScrollDraw::Redraw] {
            let (mut term, mut view) = view(screen, scroll, Orientation::Normal);
            let mut video = Panning::new(screen, buffers);
            fill(&mut term, 6);
            // each buffer drawn whole once
            for _ in 0..buffers {
                feed(&mut term, b"\x1b[1;1HL\x1b[6;20H");
                draw(&mut term, &mut view, &mut video);
            }
            for _ in 0..3 {
                feed(&mut term, b"\r\nnext");
                draw(&mut term, &mut view, &mut video);
            }
            frames.push((video.written.split_off(buffers), video.tall(), way(&view)));
        }
        let [(pan, tall, panned), (redraw, screen_sized, redrew)] = &frames[..] else {
            panic!("{frames:?}");
        };
        assert!(*tall && !screen_sized);
        assert_eq!((*panned, *redrew), (Some(Way::Pan), Some(Way::Redraw)));
        // a row has a second copy where the window can reach it, and the
        // cursor's cell where it was is drawn again
        let most = 2 * buffers * row + 2 * cell;
        // the first line feed's row has no second copy, and the buffer
        // lacks only that row
        assert_eq!(pan.first().map(|f| f.0), Some(row), "{pan:?}");
        assert!(pan.iter().all(|&(px, _)| px <= most), "{buffers}: {pan:?}");
        assert!(pan.windows(2).all(|w| w[1].1 == w[0].1 + CELL.1), "{pan:?}");
        assert!(
            redraw.iter().all(|&(px, w)| px == whole && w == 0),
            "{redraw:?}"
        );
    }
}

// The way a view draws: `auto` and `pan` pan where the display makes the
// tall buffer and copy where it refuses it; `copy` asks for no tall
// buffer and copies; `redraw` asks for none and redraws.
//
// References:
// - ADR-1310 D6, "Where the driver allows it": the copy where the pan is
//   refused.
#[test]
fn a_refused_pan_copies_and_each_way_draws_as_the_key_says() {
    let screen = (160, 96);
    let turned = false;
    let ring = |v: &View| {
        let r = &v.drawing.as_ref().unwrap().renderer;
        tall_for(ScrollDraw::Pan, r, screen, Orientation::Normal)
    };
    for (scroll, refuses, wants_tall, drawn) in [
        (ScrollDraw::Auto, false, true, Way::Pan),
        (ScrollDraw::Pan, false, true, Way::Pan),
        (ScrollDraw::Auto, true, true, Way::Copy),
        (ScrollDraw::Pan, true, true, Way::Copy),
        (ScrollDraw::Copy, false, false, Way::Copy),
        (ScrollDraw::Redraw, false, false, Way::Redraw),
    ] {
        let (mut term, mut view) = view(screen, scroll, Orientation::Normal);
        let mut video = Panning::new(screen, 2);
        video.refuses = refuses;
        let tall = ring(&view);
        fill(&mut term, 6);
        draw(&mut term, &mut view, &mut video);
        feed(&mut term, b"\r\nnext");
        draw(&mut term, &mut view, &mut video);
        let what = format!("{scroll:?}, refused {refuses}");
        let asked = if wants_tall { tall } else { None };
        assert_eq!(video.asked, [asked], "{what}");
        assert_eq!(tall.map(|t| t.turned), Some(turned), "{what}");
        assert_eq!(way(&view), Some(drawn), "{what}");
        assert_eq!(video.tall(), drawn == Way::Pan, "{what}");
    }
}

// A head that moves to the GPU path asks for buffers the screen's size
// again, and the renderer forgets what it drew over the pan, so the next
// frame draws whole into the new buffers.
//
// References:
// - ADR-1310 D6, "Where the driver allows it": a display on the GPU path
//   keeps buffers the screen's size.
#[test]
fn a_change_of_way_asks_for_new_buffers_and_draws_whole() {
    let screen = (160, 96);
    let (mut term, mut view) = view(screen, ScrollDraw::Auto, Orientation::Normal);
    let mut video = Panning::new(screen, 2);
    fill(&mut term, 6);
    draw(&mut term, &mut view, &mut video);
    assert!(video.tall());
    view.scroll = ScrollDraw::Redraw;
    feed(&mut term, b"\r\nnext");
    draw(&mut term, &mut view, &mut video);
    assert!(!video.tall());
    assert_eq!(video.asked.last(), Some(&None));
    assert_eq!(way(&view), Some(Way::Redraw));
    let whole = (screen.0 * screen.1) as usize;
    assert_eq!(video.written.last().map(|w| w.0), Some(whole));
}

/// What `steps` show, drawn on a display of `buffers` buffers turned as
/// `orientation` says, under `scroll`, after each step. A notice is drawn
/// over every frame from the third step on.
fn shows(
    screen: (u32, u32),
    scroll: ScrollDraw,
    orientation: Orientation,
    buffers: usize,
    steps: &[&[u8]],
) -> (Vec<Vec<u8>>, Option<Way>) {
    shows_switching(screen, &[scroll], orientation, buffers, steps)
}

/// What [`shows`] gives, with the view drawing each step the way of
/// `scrolls` in turn.
fn shows_switching(
    screen: (u32, u32),
    scrolls: &[ScrollDraw],
    orientation: Orientation,
    buffers: usize,
    steps: &[&[u8]],
) -> (Vec<Vec<u8>>, Option<Way>) {
    let (mut term, mut view) = view(screen, ScrollDraw::Redraw, orientation);
    let mut video = Panning::new(screen, buffers);
    let mut shown = Vec::new();
    for (n, step) in steps.iter().enumerate() {
        view.scroll = scrolls[n % scrolls.len()];
        if n == 2 {
            view.overlay = Overlay::new(
                Some("a notice over the scroll"),
                &[],
                None,
                OverlayColors::default(),
            );
        }
        feed(&mut term, step);
        draw(&mut term, &mut view, &mut video);
        shown.push(video.shown());
    }
    (shown, way(&view))
}

/// The steps the pictures are compared after.
const STEPS: &[&[u8]] = &[
    b"line 0\r\nline 1\r\nline 2\r\nline 3\r\nline 4\r\nline 5",
    b"\r\nline 6",
    b"\r\nline 7\r\nline 8",
    b"\x1b[H\x1bMat the top",
    b"\x1b[6;1H\r\nline 9\r\nline 10\r\nline 11",
    b"\x1b[3;3Hin place",
    b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\nfar down",
    b"\x1b[2Jcleared",
    b"\x1b[?1049halternate\r\n\r\n\r\n\r\n\r\n\r\n\r\nscrolled",
    b"\x1b[?1049l\r\nback on the main screen",
    b"\r\none more",
    b"\x1b[H\x1bM\x1bMtwo rows down",
    b"\r\nand the last",
];

// Every frame the pan and the copy show is what the redraw shows: line
// feeds, a reverse index on the top row, a scroll of several lines, a
// notice that stays over moving lines, a clear, the alternate screen and
// back, on two and three buffers, upright and turned, with a border
// around the grid and without one.
//
// References:
// - ADR-1310 D6: the pan and the copy show what a redraw shows.
#[test]
fn the_pan_and_the_copy_show_what_the_redraw_shows() {
    for screen in [(160, 96), (171, 103)] {
        for orientation in [Orientation::Normal, Orientation::Right] {
            let screen = match orientation {
                Orientation::Right => (screen.1, screen.0),
                _ => screen,
            };
            for buffers in [2, 3] {
                let (want, _) = shows(screen, ScrollDraw::Redraw, orientation, buffers, STEPS);
                for (scroll, drawn) in [(ScrollDraw::Pan, Way::Pan), (ScrollDraw::Copy, Way::Copy)]
                {
                    let what = format!("{scroll:?} {orientation:?} {screen:?} {buffers} buffers");
                    let (got, way) = shows(screen, scroll, orientation, buffers, STEPS);
                    assert_eq!(way, Some(drawn), "{what}");
                    for (n, (g, w)) in got.iter().zip(&want).enumerate() {
                        assert!(g == w, "{what}: step {n}");
                    }
                }
            }
        }
    }
}

// A view whose way changes from frame to frame, as a head's does when it
// moves to the GPU path and back, shows what the redraw shows: the
// renderer forgets what one way drew before it draws another way.
//
// References:
// - ADR-1310 D6: the pan and the copy show what a redraw shows.
#[test]
fn a_view_that_changes_its_way_shows_what_the_redraw_shows() {
    let screen = (160, 96);
    let (want, _) = shows(screen, ScrollDraw::Redraw, Orientation::Normal, 2, STEPS);
    for scrolls in [
        [ScrollDraw::Copy, ScrollDraw::Redraw],
        [ScrollDraw::Pan, ScrollDraw::Redraw],
        [ScrollDraw::Pan, ScrollDraw::Copy],
    ] {
        let (got, _) = shows_switching(screen, &scrolls, Orientation::Normal, 2, STEPS);
        for (n, (g, w)) in got.iter().zip(&want).enumerate() {
            assert!(g == w, "{scrolls:?}: step {n}");
        }
    }
}

// A frame drawn whole because the display lost its picture, as after a
// wake-up, puts the window back at the ring's start, where no row needs a
// second copy, even when the scroll record could state the scroll.
//
// References:
// - ADR-1310 D6, "The window at rest": a frame drawn whole moves the
//   window to the ring's start.
#[test]
fn a_frame_drawn_whole_puts_the_window_at_rest() {
    let screen = (160, 96);
    let (mut term, mut view) = view(screen, ScrollDraw::Pan, Orientation::Normal);
    let mut video = Panning::new(screen, 1);
    fill(&mut term, 6);
    draw(&mut term, &mut view, &mut video);
    feed(&mut term, b"\r\nnext");
    draw(&mut term, &mut view, &mut video);
    assert_eq!(video.written.last().map(|w| w.1), Some(CELL.1));
    view.lost = true;
    feed(&mut term, b"x");
    draw(&mut term, &mut view, &mut video);
    let whole = (screen.0 * screen.1) as usize;
    assert_eq!(video.written.last(), Some(&(whole, 0)));
}

/// Draws a frame as the app does: the view first hears of the buffers the
/// display made since. On a display whose frames flip, the frame meets the
/// flip of the frame before still in flight: the display keeps it as busy,
/// or with two buffers lends none. The flip then ends, and the frame kept
/// goes out, or the frame held back is drawn, which starts the next flip.
fn draw_through(term: &mut Term, view: &mut View, video: &mut Panning) {
    hear(view, video);
    draw(term, view, video);
    if video.flips && (view.kept.is_some() || view.pending) {
        video.flip_done();
        view.send_kept(term, video);
        if view.pending {
            draw(term, view, video);
        }
    }
}

/// What the app does with the display's events: a modeset's redraw makes
/// the view draw whole.
fn hear(view: &mut View, video: &mut Panning) {
    for e in video.events() {
        if matches!(e, Event::Redraw(_)) {
            view.lost = true;
        }
    }
}

/// Draws with a font of `height` pixels from here on, the terminal resized
/// to the grid that fits, as a change of the head's font does.
fn refont(term: &mut Term, view: &mut View, height: u32, orientation: Orientation) {
    let face: DisplayFace = Box::new(Unifont::new(height));
    let config = RenderConfig {
        orientation,
        ..RenderConfig::default()
    };
    view.rebuild(Font::new(face), height, config);
    let (cols, rows) = view.drawing.as_ref().unwrap().renderer.grid_size();
    term.terminal.resize(cols, rows);
}

/// A change to how the stand-in display behaves.
type SetUp = fn(&mut Panning);

/// The step before which the font goes from 16 to 32 pixels.
const REFONT: usize = 3;
/// The step at which the display makes the buffers asked for.
const ARRIVE: usize = 7;

/// What a display shows after each of [`STEPS`], drawn under `scroll` on
/// `buffers` buffers turned as `orientation` says: before step [`REFONT`]
/// `set_up` changes how the display behaves and the font goes from 16 to
/// 32 pixels, and the display makes the new buffers at step [`ARRIVE`],
/// while a frame of that step composes.
fn shows_refont(
    screen: (u32, u32),
    scroll: ScrollDraw,
    orientation: Orientation,
    buffers: usize,
    set_up: SetUp,
) -> (Vec<Vec<u8>>, Option<Way>) {
    let (mut term, mut view) = view(screen, scroll, orientation);
    let mut video = Panning::new(screen, buffers);
    let key = (PathBuf::from("/dev/test"), ID);
    let mut shown = Vec::new();
    for (n, step) in STEPS.iter().enumerate() {
        if n == REFONT {
            set_up(&mut video);
            refont(&mut term, &mut view, 32, orientation);
        }
        feed(&mut term, step);
        if n == ARRIVE {
            // the buffers arrive while the step's frame composes: a token
            // lent before is dropped at its swap
            hear(&mut view, &mut video);
            let prepared = view.prepare(
                &key,
                &mut term,
                &mut None,
                &mut video,
                &DrawState::default(),
                None,
                &Instant::now,
            );
            video.arrive();
            if let Prepared::Job(job) = prepared {
                let composed = job.compose(None);
                view.present(composed, &mut term, &mut video, true, &Instant::now);
            }
        }
        draw_through(&mut term, &mut view, &mut video);
        shown.push(video.shown());
    }
    (shown, way(&view))
}

// A font change while the display keeps the buffers it holds, as drm2d
// does while it is asleep, blanked or waiting on a flip, shows no wrong
// picture. drm2d stops lending its tall buffers as tall ones once the
// view asks for buffers of another size, so the view draws nothing until
// the new buffers arrive, and the screen keeps the picture it had. A
// display that lends the buffers it holds as tall ones, as `set_tall`
// allows, gets each frame drawn whole in the buffer's first screen, with
// the window at the buffer's start. Once the new buffers arrive, every
// frame is what the redraw shows. This holds on one to three buffers,
// upright and turned, with flips that make the display keep a frame and
// a token that comes back after the buffers were made again.
//
// References:
// - ADR-1310 D6: the pan and the copy show what a redraw shows.
// - ADR-1401 D2, "A buffer taller than the screen": the display makes the
//   buffers asked for at its next modeset.
#[test]
fn a_font_change_shows_what_the_redraw_shows_until_and_after_the_new_buffers() {
    fn drm2d(video: &mut Panning) {
        video.defers = true;
    }
    fn drm2d_flipping(video: &mut Panning) {
        (video.defers, video.flips) = (true, true);
    }
    fn stale_as_tall(video: &mut Panning) {
        (video.defers, video.stale_as_tall) = (true, true);
    }
    fn stale_as_tall_flipping(video: &mut Panning) {
        (video.defers, video.stale_as_tall, video.flips) = (true, true, true);
    }
    let set_ups: [(&str, SetUp); 4] = [
        ("drm2d", drm2d),
        ("drm2d flipping", drm2d_flipping),
        ("stale as tall", stale_as_tall),
        ("stale as tall flipping", stale_as_tall_flipping),
    ];
    for orientation in [Orientation::Normal, Orientation::Right] {
        let screen = match orientation {
            Orientation::Right => (96, 160),
            _ => (160, 96),
        };
        for buffers in [1, 2, 3] {
            let (want, _) = shows_refont(screen, ScrollDraw::Redraw, orientation, buffers, |_| {});
            for (name, set_up) in set_ups {
                let what = format!("{name} {orientation:?} {buffers} buffers");
                let (got, way) =
                    shows_refont(screen, ScrollDraw::Pan, orientation, buffers, set_up);
                assert_eq!(way, Some(Way::Pan), "{what}");
                for (n, (g, w)) in got.iter().zip(&want).enumerate() {
                    let kept = name.starts_with("drm2d") && (REFONT..ARRIVE).contains(&n);
                    if kept {
                        assert!(g == &want[REFONT - 1], "{what}: step {n} keeps its picture");
                    } else {
                        assert!(g == w, "{what}: step {n}");
                    }
                }
            }
        }
    }
}

// The GPU path is offered only frames drawn the `redraw` way: a frame over
// the pan or from the copy is drawn on the CPU path, and the GPU path
// neither sets the display up nor draws.
//
// References:
// - ADR-1310 D6, "Where the driver allows it": a display on the GPU path
//   draws a scroll the `redraw` way.
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_path_is_offered_no_frame_of_the_pan_or_the_copy() {
    use crate::gpu::head::HeadGpu;
    use crate::painter::{GpuSource, TestGpu};
    let screen = (160, 96);
    let key = (PathBuf::from("/dev/test"), ID);
    for (scroll, drawn) in [
        (ScrollDraw::Pan, Way::Pan),
        (ScrollDraw::Copy, Way::Copy),
        (ScrollDraw::Redraw, Way::Redraw),
    ] {
        let test = Arc::new(TestGpu::default());
        let mut gpu = HeadGpu::new(&GpuSource::Test(Arc::clone(&test))).unwrap();
        let (mut term, mut view) = view(screen, scroll, Orientation::Normal);
        let mut video = Panning::new(screen, 2);
        fill(&mut term, 6);
        for _ in 0..3 {
            feed(&mut term, b"\r\nnext");
            let Prepared::Job(job) = view.prepare(
                &key,
                &mut term,
                &mut None,
                &mut video,
                &DrawState::default(),
                None,
                &Instant::now,
            ) else {
                panic!("a job");
            };
            let composed = job.compose(Some(&mut gpu));
            view.present(composed, &mut term, &mut video, true, &Instant::now);
        }
        assert_eq!(way(&view), Some(drawn));
        let set_ups = test.set_ups.lock().unwrap().len();
        let on_gpu = test.drawn.load(Ordering::SeqCst);
        if drawn == Way::Redraw {
            assert!(on_gpu > 0, "the GPU path draws the redraw");
        } else {
            assert_eq!((set_ups, on_gpu), (0, 0), "{scroll:?}");
        }
    }
}

// A display whose driver refuses the pan copies under `auto` and `pan`,
// and the log says so once. Under `pan` the line is a warning, since the
// key asked for the pan; under `auto` the copy is one of the ways the key
// allows.
//
// References:
// - ADR-1310 D6, "Where the driver allows it": vtium logs once when the
//   driver refuses an asked-for pan, and copies.
#[test]
fn a_refused_pan_is_logged_once_and_warned_of_under_pan() {
    let screen = (160, 96);
    for (scroll, line) in [
        (
            ScrollDraw::Auto,
            "Display PAN-1 scrolls by copying: vtium keeps the drawn lines in memory and copies the part that the screen shows.",
        ),
        (
            ScrollDraw::Pan,
            "Display PAN-1 cannot pan, because its driver does not allow it. The display scrolls by copying instead.",
        ),
    ] {
        let (mut term, mut view) = view(screen, scroll, Orientation::Normal);
        let mut video = Panning::new(screen, 2);
        video.refuses = true;
        crate::getty::tests::logged();
        fill(&mut term, 6);
        for _ in 0..3 {
            feed(&mut term, b"\r\nnext");
            draw(&mut term, &mut view, &mut video);
        }
        assert_eq!(crate::getty::tests::logged(), [line], "{scroll:?}");
    }
}

/// A terminal of 4 by 2 cells with 20 lines of history, for views on
/// displays taller than its grid.
fn term_with_history() -> Term {
    let clock: Clock = Rc::new(Instant::now);
    let mut term = Term::new(
        Terminal::new(4, 2, 100),
        Sync::new(Duration::ZERO, 0),
        Blink::new(false),
        clock,
    );
    term.terminal.screen_mut().set_scrollback_max(100);
    for i in 0..20 {
        feed(&mut term, format!("{i:02}xx\r\n").as_bytes());
    }
    term
}

/// One pass of the head's drawing loop over `views`, in order, as
/// app/drawing.rs runs it: the first view to prepare takes the snapshot
/// that every view of the pass draws. Returns that snapshot.
fn pass(term: &mut Term, views: &mut [(&mut View, &mut Panning)]) -> Arc<Snapshot> {
    let mut snapshot = None;
    for (view, video) in views.iter_mut() {
        let key = (PathBuf::new(), view.id);
        let prepared = view.prepare(
            &key,
            term,
            &mut snapshot,
            *video,
            &DrawState::default(),
            None,
            &Instant::now,
        );
        let Prepared::Job(job) = prepared else {
            panic!("the view takes a frame");
        };
        view.present(job.compose(None), term, *video, true, &Instant::now);
    }
    let Some((taken, _)) = snapshot else {
        panic!("a view took the snapshot");
    };
    taken
}

/// How many rows above the screen `snapshot` holds.
fn held(snapshot: &Snapshot) -> usize {
    snapshot.above.lines.len()
}

/// A view of `screen`'s pixels, which draws a scroll the redraw way.
fn display(screen: (u32, u32)) -> (View, Panning) {
    let (_, view) = view(screen, ScrollDraw::Redraw, Orientation::Normal);
    (view, Panning::new(screen, 2))
}

/// 2 spare lines above the grid of 4 by 2 cells: the overscan's 4 rows.
const SHORT: (u32, u32) = (32, 2 * 16 + 2);
/// 130 spare lines above the grid of 4 by 2 cells, 8 rows and 2 lines:
/// 9 rows.
const TALL: (u32, u32) = (32, 2 * 16 + 8 * 16 + 2);

// One snapshot serves every view of a pass, and the first view to prepare
// takes it. When a view that asks for more rows above the screen prepares
// after it, that view draws its frame and is pending, and the next pass
// draws it again from a snapshot that holds the rows it asks for, though
// nothing changed.
//
// References:
// - ADR-1310 D10, "A strip taller than a row": as many rows of history as fit fill the spare lines.
#[test]
fn a_taller_display_that_prepares_second_draws_again_with_its_rows() {
    let mut term = term_with_history();
    let (mut short, mut short_video) = display(SHORT);
    let (mut tall, mut tall_video) = display(TALL);
    let mut views = [(&mut short, &mut short_video), (&mut tall, &mut tall_video)];
    let first = pass(&mut term, &mut views);
    assert_eq!((first.above.asked, held(&first)), (4, 5), "the first pass");
    assert!(views[1].0.pending, "the tall display draws again");
    assert!(!views[0].0.pending);
    let tall = &mut views[1];
    assert!(
        tall.0
            .draws(&term, &*tall.1, false, false, false, Instant::now()),
        "nothing changed, and the tall display still draws"
    );
    let second = pass(&mut term, &mut views);
    assert_eq!(
        (second.above.asked, held(&second)),
        (9, 10),
        "the second pass"
    );
    assert!(!views[1].0.pending, "the tall display has its rows");
}

// The snapshot asks for the most rows above the screen that a view of the
// term still asks for. The count goes down when the view that asked for
// the most goes away or asks for fewer, and the screen's rewrap margin
// goes down with it.
//
// References:
// - ADR-1310 D10, "A strip taller than a row": the snapshot captures as many rows above the screen as the spare lines show.
#[test]
fn the_rows_a_snapshot_holds_follow_the_views_that_ask() {
    let mut term = term_with_history();
    let (mut tall, mut tall_video) = display(TALL);
    let (mut short, mut short_video) = display(SHORT);
    assert_eq!(
        held(&pass(&mut term, &mut [(&mut tall, &mut tall_video)])),
        10
    );
    let mut both = [(&mut tall, &mut tall_video), (&mut short, &mut short_video)];
    assert_eq!(held(&pass(&mut term, &mut both)), 10);
    // the tall display goes away
    drop(tall);
    assert_eq!(
        held(&pass(&mut term, &mut [(&mut short, &mut short_video)])),
        5
    );
    // a tall display whose grid grows until its spare lines show 1 row,
    // with history enough that the rows the screen takes back leave more
    // above it than a frame asks for
    for i in 20..50 {
        feed(&mut term, format!("{i:02}xx\r\n").as_bytes());
    }
    let (mut tall, mut tall_video) = display(TALL);
    assert_eq!(
        held(&pass(&mut term, &mut [(&mut tall, &mut tall_video)])),
        10
    );
    term.terminal.screen_mut().resize(4, 10);
    assert_eq!(
        held(&pass(&mut term, &mut [(&mut tall, &mut tall_video)])),
        5
    );
    // and a change of width rewraps the screen's 10 rows and those 5
    // alone, where the tall grid's frames held 10
    let palette = term.terminal.palette().clone();
    term.terminal.screen_mut().resize(8, 10);
    let more = vtium_term::Reach {
        above: 30,
        ..vtium_term::Reach::default()
    };
    let frame = term
        .terminal
        .screen()
        .frame_reaching(&palette, Age::NEVER, more);
    assert_eq!(frame.rows_above(), 10 + 5);
}
