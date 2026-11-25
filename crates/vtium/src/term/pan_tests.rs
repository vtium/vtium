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

/// A display that makes the buffers a view asks for: taller than its
/// screen, unless it refuses the pan, and else the screen's size.
struct Panning {
    screen: (u32, u32),
    size: (u32, u32),
    turned: bool,
    refuses: bool,
    asked: Vec<Option<Tall>>,
    bufs: Vec<Arc<Mutex<Vec<u8>>>>,
    /// Each buffer's frame number, `None` while it holds no frame.
    drawn: Vec<Option<u32>>,
    frames: u32,
    back: usize,
    /// What the back buffer held before it was lent.
    before: Vec<u8>,
    /// The window each buffer shows.
    windows: Vec<u32>,
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
            bufs: Vec::new(),
            drawn: Vec::new(),
            frames: 0,
            back: 0,
            before: Vec::new(),
            windows: Vec::new(),
            written: Vec::new(),
        };
        p.make(buffers);
        p
    }

    fn make(&mut self, n: usize) {
        let len = (self.size.0 * self.size.1 * 4) as usize;
        self.bufs = (0..n).map(|_| Arc::new(Mutex::new(vec![0; len]))).collect();
        self.drawn = vec![None; n];
        self.windows = vec![0; n];
        self.back = 0;
    }

    fn tall(&self) -> bool {
        self.size != self.screen
    }

    /// What the screen shows: the window of the buffer swapped last.
    fn shown(&self) -> Vec<u8> {
        let front = (self.back + self.bufs.len() - 1) % self.bufs.len();
        let pixels = self.bufs[front].lock().unwrap();
        let (w, h) = self.screen;
        let stride = self.size.0 as usize * 4;
        let window = self.windows[front] as usize;
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
        Vec::new()
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
    fn lend(&mut self, _: DisplayId) -> Option<Token> {
        let age = self.drawn[self.back].map_or(0, |n| self.frames - n + 1);
        let mut pixels = self.bufs[self.back].lock().unwrap();
        self.before = pixels.clone();
        for px in pixels.chunks_mut(4) {
            px.copy_from_slice(&UNWRITTEN);
        }
        drop(pixels);
        let memory: Arc<Memory> = self.bufs[self.back].clone();
        let token = Token::new(ID, self.size, self.size.0 * 4, age, memory);
        if !self.tall() {
            return Some(token);
        }
        let mut token = token.tall(self.screen, self.turned);
        assert!(token.set_window(self.windows[self.back]));
        Some(token)
    }
    fn give_back(&mut self, _: Token) {
        let (written, _) = self.count_written(&[]);
        assert_eq!(written, 0, "a buffer given back unused");
    }
    // Every pixel a frame writes lies in its damage, as the display
    // presents only the damage.
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let (written, outside) = self.count_written(damage);
        assert_eq!(outside, 0, "pixels written outside the damage {damage:?}");
        let window = if self.tall() { token.window() } else { 0 };
        let moved = window != self.windows[self.back];
        if damage.is_empty() && !moved {
            return Ok(Swapped::Nothing);
        }
        self.frames += 1;
        self.drawn[self.back] = Some(self.frames);
        self.windows[self.back] = window;
        self.written.push((written, window));
        self.back = (self.back + 1) % self.bufs.len();
        Ok(Swapped::Frame)
    }
    fn is_flipping(&self, _: DisplayId) -> bool {
        false
    }
    fn ready_for_frame(&self, _: DisplayId) -> bool {
        true
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
    // A display holds new buffers after a change of what is asked, as a
    // modeset makes them, so each is drawn whole first.
    fn set_tall(&mut self, _: DisplayId, tall: Option<Tall>) -> io::Result<()> {
        self.asked.push(tall);
        let made = match tall {
            Some(t) if !self.refuses => (t.buffer(self.screen), t.turned),
            _ => (self.screen, false),
        };
        if made != (self.size, self.turned) {
            (self.size, self.turned) = made;
            let n = self.bufs.len();
            self.make(n);
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
