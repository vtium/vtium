//! The ring's tests: a frame shown by the pan or the copy equals the
//! frame drawn the redraw way, pixel for pixel, at every start of the
//! window, across the ring's end, at buffer ages 0 to 6, in all four
//! orientations and with narrow and wide borders; the pan's damage holds
//! every pixel it writes; a twin is drawn only when a window shows it; a
//! scroll draws only the border it shows anew; a frame drawn whole puts
//! the window at rest; an overlay drawn over the window goes with its
//! lines, also over a frame that changes nothing and in random runs; and,
//! at 1366×768, a one-line scroll draws only the rows that
//! came in while a screen rewritten in place writes no more than the
//! redraw way.
#![expect(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking, on small numbers it picks"
)]

use vtium_font::{Font, Unifont};
use vtium_term::{
    Age, Color, Cursor, Damage, Frame, Glyph, Line, LineId, Palette, Rgb, Style, Width,
};

use crate::{Canvas, Config, DrawState, Orientation, Renderer, Target, TestInk};

const COLS: usize = 6;
const ROWS: usize = 4;

/// A display's size, along the axis the rows run down and across it.
#[derive(Clone, Copy, Debug)]
struct Shape {
    along: u32,
    across: u32,
}

/// 4 rows of 16 pixels and 7 spare lines, so the grid has a border strip
/// thinner than a row on both sides; 6 cells of 8 pixels and 5 spare
/// columns across.
const TIGHT: Shape = Shape {
    along: 4 * 16 + 7,
    across: 6 * 8 + 5,
};

/// 6 rows fit, and the grid's 4 rows leave strips of 21 lines on both
/// sides, more than a row; 20 spare columns on each side across.
const WIDE: Shape = Shape {
    along: 4 * 16 + 42,
    across: 6 * 8 + 40,
};
/// What a buffer holds before anything is drawn into it, so that a pixel
/// the ring should have drawn and did not shows.
const UNDRAWN: u32 = 0x00ab_cdef;
/// A glyph whose ink reaches 6 pixels into the row above and the row
/// below, so that the rows around a scroll trade ink.
const TALL: char = '¦';
/// A glyph whose ink reaches 3 pixels into the cells on both sides, which
/// it keeps inside its own cell while the cursor stands on it.
const SIDE: char = '¤';

const TURNS: [Orientation; 4] = [
    Orientation::Normal,
    Orientation::Right,
    Orientation::UpsideDown,
    Orientation::Left,
];

fn display(o: Orientation, s: Shape) -> (u32, u32) {
    match o.turned() {
        true => (s.along, s.across),
        false => (s.across, s.along),
    }
}

fn renderer(o: Orientation, s: Shape) -> Renderer<Unifont> {
    let (w, h) = display(o, s);
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
    let pixels = [255u8; 2 * 28];
    r.keep_glyph_for_test(
        TALL,
        TestInk {
            left: 3,
            top: -6,
            width: 2,
            height: 28,
            pixels: &pixels,
        },
    );
    let pixels = [255u8; 14 * 10];
    r.keep_glyph_for_test(
        SIDE,
        TestInk {
            left: -3,
            top: 3,
            width: 14,
            height: 10,
            pixels: &pixels,
        },
    );
    r
}

/// A console's lines, the view over them, and what the next frame shows.
struct Term {
    lines: Vec<Line>,
    /// Makes the line with the given number.
    make: fn(usize) -> Line,
    top: usize,
    palette: Palette,
    cursor: Cursor,
    /// The cursor's blink phase: false while it is off.
    cursor_on: bool,
    pointer: Option<(u32, u32)>,
    #[cfg(feature = "images")]
    store: vtium_image::Store,
    /// The pictures placed so far.
    #[cfg(feature = "images")]
    pictures: u32,
}

/// What a step of a test does to the console.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// The view moves this many lines down the output, or up when
    /// negative; new lines are written as the view passes the last.
    Scroll(i64),
    /// A cell of the screen is written again in place.
    Write(usize, usize, char),
    /// The cursor moves to a row and column.
    Cursor(usize, usize),
    /// The arrow drawn in software moves to a pixel of the grid, or hides.
    Pointer(Option<(u32, u32)>),
    /// Every cell changes, as a switch to the alternate screen does.
    Whole,
    /// The cursor's blink phase turns on or off.
    Blink(bool),
    /// A new picture two rows tall is placed on a row of the screen, from
    /// its fourth column; nothing in a build without `images`.
    Picture(usize),
    /// Nothing changes.
    Still,
}

fn line(n: usize) -> Line {
    let mut l = Line::new(LineId::new(n as u64 + 1), COLS);
    let fg = Rgb::new(0xc0, (n * 40) as u8, 0x40);
    let bg = Rgb::new((n * 23) as u8, 0x10, 0x20);
    let style = Style {
        fg: Color::Rgb(fg),
        bg: Color::Rgb(bg),
        ..Style::DEFAULT
    };
    let text: Vec<char> = format!("{n:02}{TALL}g{}Q", (b'a' + (n % 26) as u8) as char)
        .chars()
        .collect();
    for (col, &c) in text.iter().enumerate().take(COLS) {
        l.write(col, Glyph::Char(c), Width::Narrow, &style, None);
    }
    l
}

/// A line whose letters keep their ink inside their cells.
fn plain_line(n: usize) -> Line {
    let mut l = Line::new(LineId::new(n as u64 + 1), COLS);
    let style = Style {
        fg: Color::Rgb(Rgb::new(0xc0, (n * 40) as u8, 0x40)),
        bg: Color::Rgb(Rgb::new((n * 23) as u8, 0x10, 0x20)),
        ..Style::DEFAULT
    };
    for col in 0..COLS {
        let c = (b'a' + ((n + col) % 26) as u8) as char;
        l.write(col, Glyph::Char(c), Width::Narrow, &style, None);
    }
    l
}

impl Term {
    /// A console of [`line`]s with a picture, the cursor and the arrow.
    fn new() -> Term {
        let t = Term::with(line);
        #[cfg(feature = "images")]
        let t = {
            let mut t = t;
            t.place_picture(1, 2);
            t
        };
        t
    }

    /// A console of the lines `make` makes, with no picture, its cursor
    /// on the last row's third cell and the arrow over the grid.
    fn with(make: fn(usize) -> Line) -> Term {
        let mut colors = [Rgb::default(); 256];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = Rgb::new(i as u8, (i * 3) as u8, (i * 7) as u8);
        }
        Term {
            lines: (0..ROWS).map(make).collect(),
            make,
            top: 0,
            palette: Palette::new(colors, Rgb::new(0xc0, 0xc0, 0xc0), Rgb::new(0, 0, 0x30)),
            cursor: Cursor {
                row: ROWS - 1,
                col: 2,
                visible: true,
                ..Cursor::default()
            },
            cursor_on: true,
            pointer: Some((13, 21)),
            #[cfg(feature = "images")]
            store: vtium_image::Store::new(),
            #[cfg(feature = "images")]
            pictures: 0,
        }
    }

    /// A new picture two rows tall on the screen's line `row`, from its
    /// column `col`.
    #[cfg(feature = "images")]
    fn place_picture(&mut self, row: u32, col: u32) {
        use vtium_image::{NewImage, Place, Protocol, Rows, Screens};
        self.store.set_limit(1 << 24);
        let (screen, above) = self.keys();
        let screens = || Screens {
            main: Rows {
                screen: &screen,
                above: &above,
            },
            alt: Rows::default(),
        };
        let (w, h) = (16u32, 32u32);
        let new = NewImage {
            protocol: Protocol::KITTY,
            width: w,
            height: h,
            pixels: (0..w * h)
                .map(|i| 0xff00_0000 | (i * 7919 + self.pictures * 0x1234) & 0x00ff_ffff)
                .collect(),
            id: self.pictures + 1,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = self
            .store
            .add_image(new, None, screens())
            .expect("the store takes the image");
        let place = Place {
            col,
            row,
            ..Place::default()
        };
        self.store
            .place(image, place, screens(), COLS)
            .expect("the store places it");
        self.pictures += 1;
    }

    /// The keys of the lines on the screen, and of those above it, newest
    /// first.
    #[cfg(feature = "images")]
    fn keys(&self) -> (Vec<vtium_image::LineKey>, Vec<vtium_image::LineKey>) {
        let key = |i: usize| vtium_image::LineKey::new(i as u64 + 1);
        let screen = (self.top..self.top + ROWS).map(key).collect();
        let above = (0..self.top).rev().map(key).collect();
        (screen, above)
    }

    /// Applies `step`, marking in `d` what it changes; returns the lines
    /// the screen moved up and whether the frame changes every cell.
    fn apply(&mut self, step: Step, d: &mut Damage) -> (i64, bool) {
        match step {
            Step::Scroll(n) => {
                let top = (self.top as i64 + n).max(0) as usize;
                let n = top as i64 - self.top as i64;
                self.top = top;
                while self.lines.len() < self.top + ROWS {
                    self.lines.push((self.make)(self.lines.len()));
                }
                // the scroll record moves the view's ages with its lines
                // and marks the rows that came in, as the screen's view does
                let k = n.unsigned_abs() as usize;
                match n > 0 {
                    true => d.scroll_up(k),
                    false => d.scroll_down(k),
                }
                return (n, false);
            }
            Step::Write(row, col, c) => {
                let style = Style {
                    fg: Color::Rgb(Rgb::new(0xff, 0xff, 0)),
                    bg: Color::Rgb(Rgb::new(0x40, 0, (col * 30) as u8)),
                    ..Style::DEFAULT
                };
                let l = &mut self.lines[self.top + row];
                l.write(col, Glyph::Char(c), Width::Narrow, &style, None);
                d.mark_cell(row, col);
            }
            Step::Cursor(row, col) => {
                self.cursor.row = row;
                self.cursor.col = col;
            }
            Step::Pointer(p) => self.pointer = p,
            Step::Whole => {
                d.mark_all();
                return (0, true);
            }
            Step::Blink(on) => self.cursor_on = on,
            Step::Picture(row) => {
                #[cfg(feature = "images")]
                self.place_picture(row as u32, 3);
                #[cfg(not(feature = "images"))]
                let _ = row;
            }
            Step::Still => {}
        }
        (0, false)
    }

    fn frame<'a>(&'a self, d: &'a Damage, since: Age) -> Frame<'a> {
        let lines = self.lines[self.top..self.top + ROWS].iter().collect();
        let f = Frame::new(lines, d, &self.palette, since, self.cursor, false);
        #[cfg(feature = "images")]
        let f = {
            let (screen, above) = self.keys();
            let rows = vtium_image::Rows {
                screen: &screen,
                above: &above,
            };
            f.with_placements(self.store.frame(rows, COLS))
        };
        f
    }

    fn state(&self) -> DrawState {
        DrawState {
            cursor_on: self.cursor_on,
            background: self.palette.bg(),
            pointer: self.pointer,
            ..DrawState::default()
        }
    }
}

/// The display's pixels that the window starting at `src` shows of the
/// pan's buffer `c`.
fn window(c: &Canvas, o: Orientation, s: Shape, src: u32) -> Canvas {
    let (w, h) = display(o, s);
    let mut out = Canvas::new(w, h, 0);
    for y in 0..h {
        let row: Vec<u32> = (0..w)
            .map(|x| match o.turned() {
                true => c.pixel(src + x, y),
                false => c.pixel(x, src + y),
            })
            .map(|p| p.expect("the window lies inside the buffer"))
            .collect();
        out.write(0, y, &row);
    }
    out
}

/// Where `a` and `b` first differ, for a failure's message.
fn first_difference(a: &Canvas, b: &Canvas) -> Option<(u32, u32, u32, u32)> {
    (0..a.height())
        .flat_map(|y| (0..a.width()).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            let (p, q) = (a.pixel(x, y)?, b.pixel(x, y)?);
            (p != q).then_some((x, y, p, q))
        })
}

/// The frame drawn the redraw way into a new buffer.
fn redrawn(o: Orientation, s: Shape, frame: &Frame<'_>, state: &DrawState) -> Canvas {
    let (w, h) = display(o, s);
    let mut c = Canvas::new(w, h, UNDRAWN);
    renderer(o, s).draw(frame, &mut c, 0, state);
    c
}

/// A display's buffers taken in turn, and the age of each as it comes.
struct Buffers {
    canvases: Vec<Canvas>,
    held: Vec<u64>,
    frames: u64,
    next: usize,
}

impl Buffers {
    /// `n` buffers of `w` by `h`, or with `n` of 0 a new buffer for each
    /// frame, whose age is always 0.
    fn new(n: usize, (w, h): (u32, u32)) -> Buffers {
        Buffers {
            canvases: (0..n.max(1)).map(|_| Canvas::new(w, h, UNDRAWN)).collect(),
            held: vec![0; n],
            frames: 0,
            next: 0,
        }
    }

    /// The buffer to draw next and its age.
    fn take(&mut self) -> (usize, u32) {
        if self.held.is_empty() {
            let (w, h) = (self.canvases[0].width(), self.canvases[0].height());
            self.canvases[0] = Canvas::new(w, h, UNDRAWN);
            return (0, 0);
        }
        let i = self.next;
        let age = match self.held[i] {
            0 => 0,
            h => (self.frames - h + 1) as u32,
        };
        (i, age)
    }

    /// The buffer `i` was drawn, and swapped when `damaged`.
    fn drawn(&mut self, i: usize, damaged: bool) {
        if damaged && !self.held.is_empty() {
            self.frames += 1;
            self.held[i] = self.frames;
            self.next = (i + 1) % self.held.len();
        }
    }
}

/// A run of scrolls up and down, past the ring's end and back, with
/// writes in place, the cursor and the arrow moving, the cursor's blink
/// phase off, six frames that change cells in place, and pictures that
/// cross the ring's end both ways.
fn steps() -> Vec<Step> {
    let mut s = vec![Step::Write(0, 0, 'x')];
    for i in 0..12 {
        s.push(Step::Scroll(1));
        if i % 3 == 1 {
            s.push(Step::Write(i % ROWS, (i * 2) % COLS, 'W'));
        }
        if i % 4 == 2 {
            s.push(Step::Cursor(ROWS - 1, i % COLS));
            s.push(Step::Pointer(Some((
                ((i * 7) % 48) as u32,
                ((i * 11) % 64) as u32,
            ))));
        }
    }
    s.extend([
        Step::Scroll(2),
        Step::Scroll(-1),
        Step::Write(1, 3, TALL),
        Step::Scroll(-1),
        Step::Scroll(-2),
        Step::Pointer(None),
        Step::Scroll(1),
        Step::Write(2, 1, 'Z'),
        Step::Scroll(3),
        Step::Pointer(Some((40, 2))),
        Step::Scroll(-3),
        Step::Scroll(1),
        Step::Scroll(1),
    ]);
    // the cursor stands on a letter whose ink passes its sides, which
    // keeps that ink inside its cell, and its blink phase turns off; the
    // line then moves down a row, away from the cursor, while the phase
    // stays off, so only the cursor's last place in either phase says
    // that letter's ink now reaches its neighbours; seven times, so that
    // the window stands at each of its starts once, as a row whose twin
    // the window shows is drawn whole
    // the arrow hides, since the cells it covers are drawn again anyway
    s.extend([Step::Pointer(None), Step::Cursor(1, 4), Step::Blink(false)]);
    for _ in 0..7 {
        s.extend([Step::Write(1, 4, SIDE), Step::Scroll(-1), Step::Scroll(2)]);
    }
    s.push(Step::Blink(true));
    // six frames that each change a cell in place, more than the copy's
    // log keeps, so a buffer six frames old lacks changes the log forgot
    s.extend([
        Step::Write(0, 1, 'A'),
        Step::Write(1, 5, 'B'),
        Step::Write(2, 0, 'C'),
        Step::Write(3, 3, 'D'),
        Step::Write(0, 4, 'E'),
        Step::Write(2, 5, 'F'),
    ]);
    // a new picture on each of seven scrolls up, then seven scrolls down,
    // as many as the wide display's ring has slots, so that pictures show
    // while the window passes the ring's end both ways
    for _ in 0..7 {
        s.push(Step::Picture(2));
        s.push(Step::Scroll(1));
    }
    s.extend([Step::Scroll(-1); 7]);
    s
}

/// What a run saw besides the frames it compared.
struct Seen {
    /// Where each frame's window started.
    starts: Vec<u32>,
    /// Whether a frame that shows a picture scrolled up past the ring's
    /// end, and whether one scrolled down past it.
    #[cfg(feature = "images")]
    pictures_cross: (bool, bool),
}

/// A target that records each write into it.
struct Recorded<'a> {
    canvas: &'a mut Canvas,
    writes: Vec<(u32, u32, u32)>,
}

impl Target for Recorded<'_> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.writes.push((x, y, pixels.len() as u32));
        self.canvas.write(x, y, pixels);
    }
}

/// Checks that each pixel `writes` wrote lies inside `damage`.
fn inside_damage(writes: &[(u32, u32, u32)], damage: &[crate::Rect]) -> Result<(), (u32, u32)> {
    let holds = |x: u32, y: u32| {
        damage
            .iter()
            .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    };
    for &(x, y, n) in writes {
        if let Some(px) = (x..x + n).find(|&px| !holds(px, y)) {
            return Err((px, y));
        }
    }
    Ok(())
}

/// Runs `steps` through a renderer that pans over `buffers` buffers, or a
/// new buffer each frame for 0, and through one that copies into as many,
/// on a display of shape `s`. Checks each frame against the redraw, and
/// that the pan's damage holds every pixel the pan wrote.
fn run(o: Orientation, s: Shape, buffers: usize, steps: &[Step]) -> Seen {
    let mut term = Term::new();
    let mut pan = renderer(o, s);
    let mut copy = renderer(o, s);
    let mut pans = Buffers::new(buffers, pan.pan_buffer_size());
    let mut copies = Buffers::new(buffers, display(o, s));
    let mut seen = Seen {
        starts: Vec::new(),
        #[cfg(feature = "images")]
        pictures_cross: (false, false),
    };
    let mut since = Age::NEVER;
    let mut first = true;
    for (n, &step) in [Step::Whole].iter().chain(steps).enumerate() {
        let mut d = Damage::new(COLS, ROWS);
        if !first {
            since = d.checkpoint();
        }
        first = false;
        let (scrolled, whole) = term.apply(step, &mut d);
        let frame = term.frame(&d, since);
        let state = term.state();
        let want = redrawn(o, s, &frame, &state);

        let (i, age) = pans.take();
        let before = pan.ring.base;
        let mut recorded = Recorded {
            canvas: &mut pans.canvases[i],
            writes: Vec::new(),
        };
        let (drawn, src) = pan.draw_pan(&frame, &mut recorded, age, &state, scrolled, whole);
        let damaged = !drawn.damage.is_empty();
        let outside = inside_damage(&recorded.writes, drawn.damage);
        assert!(
            outside.is_ok(),
            "{o:?} {s:?}, {buffers} buffers, step {n} {step:?}: the pan wrote {outside:?} outside its damage {:?}",
            drawn.damage
        );
        pans.drawn(i, damaged);
        seen.starts.push(src);
        let after = pan.ring.base;
        #[cfg(feature = "images")]
        if !frame.placements().is_empty() {
            seen.pictures_cross.0 |= scrolled > 0 && after < before;
            seen.pictures_cross.1 |= scrolled < 0 && after > before;
        }
        #[cfg(not(feature = "images"))]
        let _ = (before, after);
        if n == 0 {
            // the frames hold what the test means them to: ink that passes
            // its cells, and a picture
            assert!(
                pan.compose.cache.reaching(),
                "{o:?}: a glyph's ink reaches past its cell"
            );
            #[cfg(feature = "images")]
            assert!(
                !frame.placements().is_empty(),
                "{o:?}: the frame shows a picture"
            );
        }
        let shown = window(&pans.canvases[i], o, s, src);
        assert!(
            shown == want,
            "{o:?} {s:?}, {buffers} buffers, step {n} {step:?}: the pan at {src} differs from the redraw: {:?}",
            first_difference(&shown, &want)
        );

        let (i, age) = copies.take();
        let drawn = copy.draw_copy(
            &frame,
            &mut copies.canvases[i],
            age,
            &state,
            scrolled,
            whole,
        );
        let damaged = !drawn.damage.is_empty();
        copies.drawn(i, damaged);
        assert!(
            copies.canvases[i] == want,
            "{o:?} {s:?}, {buffers} buffers, step {n} {step:?}: the copy differs from the redraw: {:?}",
            first_difference(&copies.canvases[i], &want)
        );
    }
    seen
}

// ADR-1310, Verification, "The pan and the copy match the redraw"
#[test]
fn the_pan_and_the_copy_show_what_the_redraw_draws() {
    let steps = steps();
    for o in TURNS {
        for s in [TIGHT, WIDE] {
            let slots = renderer(o, s).ring_geometry().slots;
            // up to 6 buffers: more than the pan remembers stale twins for,
            // and ages past the frames the copy keeps the damage of
            for buffers in 0..=6 {
                let seen = run(o, s, buffers, &steps);
                let mut starts = seen.starts.clone();
                starts.sort_unstable();
                starts.dedup();
                assert_eq!(
                    starts.len(),
                    slots as usize,
                    "{o:?} {s:?}, {buffers} buffers: every start of the window is shown: {:?}",
                    seen.starts
                );
                #[cfg(feature = "images")]
                assert_eq!(
                    seen.pictures_cross,
                    (true, true),
                    "{o:?} {s:?}, {buffers} buffers: a picture shows while the window passes the ring's end, scrolling up and down"
                );
            }
        }
    }
}

// the renderer's record of the cursor's last places moves with the lines:
// where it drew the cursor, and where the cursor stood in either phase of
// its blink
#[test]
fn the_cursor_s_last_places_move_with_the_lines() {
    for on in [true, false] {
        let o = Orientation::Normal;
        let mut term = Term::new();
        term.cursor_on = on;
        let mut pan = renderer(o, TIGHT);
        let mut buf = Canvas::new(pan.pan_buffer_size().0, pan.pan_buffer_size().1, UNDRAWN);
        let d = Damage::new(COLS, ROWS);
        let frame = term.frame(&d, Age::NEVER);
        pan.draw_pan(&frame, &mut buf, 0, &term.state(), 0, true);
        let at = (2, ROWS - 1);
        assert_eq!(pan.held_at, Some(at), "cursor on: {on}");
        assert_eq!(pan.cursor_at, on.then_some(at), "cursor on: {on}");
        let grid = pan.grid.expect("the frame placed the grid");
        pan.scroll_rows(&grid, 1);
        let moved = (2, ROWS - 2);
        assert_eq!(pan.held_at, Some(moved), "cursor on: {on}");
        assert_eq!(pan.cursor_at, on.then_some(moved), "cursor on: {on}");
    }
}

/// A target that counts the pixels written into it.
struct Counted<'a> {
    canvas: &'a mut Canvas,
    pixels: u64,
}

impl Target for Counted<'_> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.pixels += pixels.len() as u64;
        self.canvas.write(x, y, pixels);
    }
}

// ADR-1310, Verification, "Second copies only where a window reaches"
#[test]
fn a_twin_is_drawn_only_for_a_window_that_shows_it() {
    for o in TURNS {
        let mut term = Term::new();
        let mut pan = renderer(o, TIGHT);
        let mut redraw = renderer(o, TIGHT);
        let mut pans = Buffers::new(2, pan.pan_buffer_size());
        let mut redraws = Buffers::new(2, display(o, TIGHT));
        let (mut pan_px, mut redraw_px) = (0u64, 0u64);
        let mut since = Age::NEVER;
        // a screen at rest rewrites its cells in place, as `top` does, then
        // the view moves back a line, so the window passes the ring's end
        // and shows the twins of rows written while it did not
        let mut script: Vec<Step> = vec![Step::Whole, Step::Pointer(None)];
        for i in 0..8 {
            script.push(Step::Write(i % ROWS, (i * 5) % COLS, 'Ŝ'));
        }
        script.push(Step::Scroll(-1));
        for (n, &step) in script.iter().enumerate() {
            let mut d = Damage::new(COLS, ROWS);
            if n > 0 {
                since = d.checkpoint();
            }
            // the view starts a line down, so there is a line to move back to
            if n == 0 {
                term.apply(Step::Scroll(1), &mut d);
            }
            let (scrolled, whole) = term.apply(step, &mut d);
            let frame = term.frame(&d, since);
            let state = term.state();

            let (i, age) = pans.take();
            let mut counted = Counted {
                canvas: &mut pans.canvases[i],
                pixels: 0,
            };
            let (drawn, src) = pan.draw_pan(&frame, &mut counted, age, &state, scrolled, whole);
            let damaged = !drawn.damage.is_empty();
            let px = counted.pixels;
            pans.drawn(i, damaged);

            let (j, age) = redraws.take();
            let mut counted = Counted {
                canvas: &mut redraws.canvases[j],
                pixels: 0,
            };
            let damaged = !redraw
                .draw(&frame, &mut counted, age, &state)
                .damage
                .is_empty();
            let rpx = counted.pixels;
            redraws.drawn(j, damaged);

            if matches!(step, Step::Write(..)) {
                pan_px += px;
                redraw_px += rpx;
            }
            // the redraw way draws a moved row only when the frame marks
            // it, so the window is checked against a frame drawn whole
            let shown = window(&pans.canvases[i], o, TIGHT, src);
            let want = redrawn(o, TIGHT, &frame, &state);
            assert!(
                shown == want,
                "{o:?} step {n} {step:?}: the window at {src} shows what the redraw draws: {:?}",
                first_difference(&shown, &want)
            );
        }
        assert_eq!(
            pan_px, redraw_px,
            "{o:?}: a screen that rewrites its cells writes what the redraw writes"
        );
    }
}

// a scroll draws again only the border that the window shows anew: on a
// display whose border is wider than a row, a scroll by one line writes
// two bands across the screen and the row that came in, not the whole
// border
#[test]
fn a_scroll_draws_only_the_border_it_shows_anew() {
    for o in TURNS {
        let mut term = Term::with(plain_line);
        term.cursor.visible = false;
        term.pointer = None;
        let mut pan = renderer(o, WIDE);
        let (w, h) = pan.pan_buffer_size();
        let mut buf = Canvas::new(w, h, UNDRAWN);
        let mut since = Age::NEVER;
        let mut written = 0;
        for (n, step) in [Step::Whole, Step::Scroll(1)].into_iter().enumerate() {
            let mut d = Damage::new(COLS, ROWS);
            if n > 0 {
                since = d.checkpoint();
            }
            let (scrolled, whole) = term.apply(step, &mut d);
            let frame = term.frame(&d, since);
            let state = term.state();
            let mut recorded = Recorded {
                canvas: &mut buf,
                writes: Vec::new(),
            };
            let (_, src) = pan.draw_pan(&frame, &mut recorded, n as u32, &state, scrolled, whole);
            // the pixels written inside the window, each once: what the
            // screen shows changed
            let along = WIDE.along;
            written = recorded
                .writes
                .iter()
                .map(|&(x, y, len)| match o.turned() {
                    true => (x..x + len)
                        .filter(|&b| b >= src && b < src + along)
                        .count(),
                    false => match y >= src && y < src + along {
                        true => len as usize,
                        false => 0,
                    },
                })
                .sum::<usize>();
            assert!(
                window(&buf, o, WIDE, src) == redrawn(o, WIDE, &frame, &state),
                "{o:?} step {n}: the window shows the frame"
            );
        }
        // the leaving row's lines, now in the border strip, and the 16
        // lines the window shows anew, each across the display's 88 pixels,
        // and the row that came in, 48 by 16
        assert_eq!(written, 2 * 16 * 88 + 48 * 16, "{o:?}");
    }
}

// ADR-1310 D6, "The window at rest"
#[test]
fn a_frame_drawn_whole_puts_the_window_at_rest() {
    for o in TURNS {
        let mut term = Term::new();
        let mut pan = renderer(o, TIGHT);
        let rest = pan.ring_geometry().src(0);
        let mut buf = Canvas::new(pan.pan_buffer_size().0, pan.pan_buffer_size().1, UNDRAWN);
        let mut since = Age::NEVER;
        let mut age = 0;
        let script = [
            (Step::Whole, true),
            (Step::Scroll(1), false),
            (Step::Scroll(1), false),
            (Step::Whole, true),
            (Step::Scroll(2), false),
            (Step::Scroll(ROWS as i64), true),
            (Step::Scroll(1), false),
            (Step::Scroll(-(ROWS as i64) - 1), true),
        ];
        for (n, &(step, at_rest)) in script.iter().enumerate() {
            let mut d = Damage::new(COLS, ROWS);
            if n > 0 {
                since = d.checkpoint();
            }
            let (scrolled, whole) = term.apply(step, &mut d);
            let frame = term.frame(&d, since);
            let state = term.state();
            let (_, src) = pan.draw_pan(&frame, &mut buf, age, &state, scrolled, whole);
            age = 1;
            assert_eq!(
                src == rest,
                at_rest,
                "{o:?} step {n} {step:?}: the window at {src}"
            );
            assert!(
                window(&buf, o, TIGHT, src) == redrawn(o, TIGHT, &frame, &state),
                "{o:?} step {n} {step:?}: the window shows the frame"
            );
        }
        // a renderer that forgets its history draws whole, at rest too
        pan.forget_history();
        let mut d = Damage::new(COLS, ROWS);
        since = d.checkpoint();
        let (scrolled, _) = term.apply(Step::Scroll(1), &mut d);
        let frame = term.frame(&d, since);
        let (_, src) = pan.draw_pan(&frame, &mut buf, 1, &term.state(), scrolled, false);
        assert_eq!(src, rest, "{o:?}: a forgotten history rests the window");
    }
}

// the buffer the pan asks for holds the ring and a screen of twins, along
// the axis the rows run down
#[test]
fn the_pan_asks_for_the_ring_and_a_screen_more() {
    // 4 rows fit, so the ring has 5 slots of 16 lines: 80 lines, and 71
    // more for the twins
    assert_eq!(
        renderer(Orientation::Normal, TIGHT).pan_buffer_size(),
        (53, 151)
    );
    assert_eq!(
        renderer(Orientation::Right, TIGHT).pan_buffer_size(),
        (151, 53)
    );
    assert_eq!(
        renderer(Orientation::Normal, TIGHT).copy_ring_bytes(),
        53 * 80 * 4
    );
}

impl<F: vtium_font::Face> Renderer<F> {
    /// The ring's shape for this renderer's display.
    fn ring_geometry(&self) -> super::Geometry {
        super::Geometry::new(&self.layout, &super::Grid::default())
    }
}

/// A console of `cols` by `rows` cells on a display of 1366×768 or
/// 1920×1080, for the tests and figures that measure a frame at a real
/// screen's size.
struct Big {
    lines: Vec<Line>,
    cols: usize,
    rows: usize,
    top: usize,
    palette: Palette,
}

impl Big {
    fn new(cols: usize, rows: usize) -> Big {
        let mut b = Big {
            lines: Vec::new(),
            cols,
            rows,
            top: 0,
            palette: Palette::new(
                [Rgb::default(); 256],
                Rgb::new(0xc0, 0xc0, 0xc0),
                Rgb::new(0, 0, 0x30),
            ),
        };
        b.fill();
        b
    }

    /// A line of letters in a color of its own.
    fn line(&self, n: usize) -> Line {
        let mut l = Line::new(LineId::new(n as u64 + 1), self.cols);
        let style = Style {
            fg: Color::Rgb(Rgb::new(0xc0, (n * 40) as u8, 0x40)),
            bg: Color::Rgb(Rgb::new((n * 23) as u8, 0x10, 0x20)),
            ..Style::DEFAULT
        };
        for col in 0..self.cols {
            let c = (b'a' + ((n + col) % 26) as u8) as char;
            l.write(col, Glyph::Char(c), Width::Narrow, &style, None);
        }
        l
    }

    fn fill(&mut self) {
        while self.lines.len() < self.top + self.rows {
            let l = self.line(self.lines.len());
            self.lines.push(l);
        }
    }

    /// The view moves a line down the output, as a line feed on the
    /// bottom row moves it, or with `up` false a line back up the output,
    /// as a reverse index on the top row moves it.
    fn scroll(&mut self, d: &mut Damage, up: bool) {
        if up {
            self.top += 1;
            self.fill();
            d.scroll_up(1);
        } else {
            self.top -= 1;
            d.scroll_down(1);
        }
    }

    /// Writes every cell of the screen again in place with letters that
    /// differ from the ones there, as a program that redraws its screen
    /// does, all but the last `keep` rows.
    fn rewrite(&mut self, d: &mut Damage, step: usize, keep: usize) {
        for row in 0..self.rows.saturating_sub(keep) {
            self.rewrite_row(d, step, row);
        }
    }

    /// Writes every cell of `row` again in place, as [`Big::rewrite`]
    /// does.
    fn rewrite_row(&mut self, d: &mut Damage, step: usize, row: usize) {
        let style = Style {
            fg: Color::Rgb(Rgb::new(0xff, 0xff, (step * 50) as u8)),
            bg: Color::Rgb(Rgb::new(0x40, (step * 30) as u8, 0)),
            ..Style::DEFAULT
        };
        let l = &mut self.lines[self.top + row];
        for col in 0..self.cols {
            let c = (b'A' + ((row + col + step) % 26) as u8) as char;
            l.write(col, Glyph::Char(c), Width::Narrow, &style, None);
        }
        d.mark_cells(row, 0..self.cols);
    }

    fn frame<'a>(&'a self, d: &'a Damage, since: Age) -> Frame<'a> {
        let lines = self.lines[self.top..self.top + self.rows].iter().collect();
        let cursor = Cursor {
            visible: false,
            ..Cursor::default()
        };
        Frame::new(lines, d, &self.palette, since, cursor, false)
    }

    fn state(&self) -> DrawState {
        DrawState {
            cursor_on: true,
            background: self.palette.bg(),
            pointer: None,
            ..DrawState::default()
        }
    }
}

/// How a test or a figure draws a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Redraw,
    Pan,
    Copy,
}

/// A renderer at 8×16 cells on a display `w` by `h`, upright.
fn big_renderer(w: u32, h: u32) -> Renderer<Unifont> {
    Renderer::new(Font::new(Unifont::new(16)), w, h, Config::default())
}

/// Draws `frame` the way `way` says into buffer `i` of `bufs`, at `age`,
/// and returns the pixels written into the buffer and the cells drawn.
#[expect(
    clippy::too_many_arguments,
    reason = "a test helper that names each input"
)]
fn draw_way(
    way: Way,
    r: &mut Renderer<Unifont>,
    bufs: &mut [Canvas],
    i: usize,
    age: u32,
    frame: &Frame<'_>,
    state: &DrawState,
    scrolled: i64,
) -> (u64, u32) {
    let mut counted = Counted {
        canvas: &mut bufs[i],
        pixels: 0,
    };
    let cells = match way {
        Way::Redraw => r.draw(frame, &mut counted, age, state).stats.cells,
        Way::Pan => {
            r.draw_pan(frame, &mut counted, age, state, scrolled, false)
                .0
                .stats
                .cells
        }
        Way::Copy => {
            r.draw_copy(frame, &mut counted, age, state, scrolled, false)
                .stats
                .cells
        }
    };
    (counted.pixels, cells)
}

/// What each measured frame does to the console.
#[derive(Clone, Copy, Debug)]
struct Load {
    /// The view moves a line down the output.
    scroll: bool,
    /// The view moves a line back up the output instead, from a view that
    /// starts 400 lines down it.
    back: bool,
    /// The bottom row is written again in place, as a pager writes its
    /// prompt line.
    status: bool,
    /// Every row but the last `keep` is written again in place.
    rewrite: Option<usize>,
    /// Lines the view moves before the frames measured, so that the pan's
    /// window stands that many rows past the ring's start.
    moved: usize,
}

/// A one-line scroll each frame.
const SCROLL: Load = Load {
    scroll: true,
    back: false,
    status: false,
    rewrite: None,
    moved: 0,
};

/// A one-line scroll back up the output each frame.
const SCROLL_BACK: Load = Load {
    back: true,
    ..SCROLL
};

/// What `frames` frames of a console of `cols` by `rows` cells write on a
/// display `w` by `h` drawn `way` with two buffers: the pixels written and
/// the time taken and the cells drawn, per frame, and the most pixels one
/// frame wrote, each frame doing what `load` says. Two
/// frames drawn whole and two frames of the same kind come first and are
/// not counted, since each buffer then still lacks what the other buffer's
/// whole frame drew.
#[expect(
    clippy::disallowed_methods,
    reason = "the figures time each frame's draw"
)]
fn measure(
    way: Way,
    (w, h): (u32, u32),
    (cols, rows): (usize, usize),
    frames: usize,
    load: Load,
) -> (u64, std::time::Duration, u64, u64) {
    let mut big = Big::new(cols, rows);
    if load.back {
        big.top = 400;
        big.fill();
    }
    let mut r = big_renderer(w, h);
    let size = match way {
        Way::Pan => r.pan_buffer_size(),
        Way::Redraw | Way::Copy => (w, h),
    };
    let mut bufs: Vec<Canvas> = (0..2)
        .map(|_| Canvas::new(size.0, size.1, UNDRAWN))
        .collect();
    let state = big.state();
    // the first two frames draw each buffer whole
    for i in 0..2 {
        let d = Damage::new(cols, rows);
        let frame = big.frame(&d, Age::NEVER);
        draw_way(way, &mut r, &mut bufs, i, 0, &frame, &state, 0);
    }
    let (mut pixels, mut peak, mut drawn) = (0, 0, 0);
    let mut time = std::time::Duration::ZERO;
    let warm = 2 + load.moved;
    for n in 0..frames + warm {
        if n == warm {
            (pixels, time, peak, drawn) = (0, std::time::Duration::ZERO, 0, 0);
        }
        let mut d = Damage::new(cols, rows);
        let since = d.checkpoint();
        let scroll = load.scroll || n < load.moved;
        if scroll {
            big.scroll(&mut d, !load.back);
        }
        if let Some(keep) = load.rewrite.filter(|_| n >= load.moved) {
            big.rewrite(&mut d, n, keep);
        }
        if load.status {
            big.rewrite_row(&mut d, n, rows - 1);
        }
        let frame = big.frame(&d, since);
        let start = std::time::Instant::now();
        let (px, cells) = draw_way(
            way,
            &mut r,
            &mut bufs,
            n % 2,
            2,
            &frame,
            &state,
            match (scroll, load.back) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => -1,
            },
        );
        time += start.elapsed();
        pixels += px;
        peak = peak.max(px);
        drawn += u64::from(cells);
    }
    let n = frames as u64;
    (pixels / n, time / frames as u32, peak, drawn / n)
}

// a one-line scroll drawn through the ring writes the rows the buffer
// lacks, the row that came in and the row the other buffer drew, and the
// border those scrolls show anew, where the redraw way draws every row the
// scroll moved
#[test]
fn a_one_line_scroll_draws_the_rows_that_came_in() {
    let line = 1366 * 16;
    // the redraw way draws every row of the grid, 1360 pixels across on
    // the full screen and 640 on the 80×25 grid
    let redraw = measure(Way::Redraw, (1366, 768), (170, 48), 4, SCROLL).0;
    assert_eq!(redraw, 48 * 1360 * 16, "the full screen drawn again");
    let redraw_small = measure(Way::Redraw, (1366, 768), (80, 25), 4, SCROLL).0;
    assert_eq!(redraw_small, 25 * 640 * 16, "the 80×25 grid drawn again");
    // the full-screen grid leaves no border strip above or below it and
    // three pixels each side: the pan writes two rows with their margins,
    // each into both copies while the window shows the twins
    let pan = measure(Way::Pan, (1366, 768), (170, 48), 4, SCROLL).0;
    assert_eq!(pan, 2 * 2 * line, "the full screen through the pan");
    // and so it stays across the ring's end, where the window shows the
    // first copies again all at once: no frame draws the screen again
    let (_, _, peak, _) = measure(Way::Pan, (1366, 768), (170, 48), 120, SCROLL);
    assert_eq!(
        peak,
        2 * 2 * line,
        "the most one frame writes, past the ring's end"
    );
    // around the 80×25 grid the leaving rows' lines and the lines the
    // window shows anew are border across the whole display, two bands of
    // a row for each of the two frames the buffer lacks, written into both
    // copies, and the two rows the buffer lacks are written once
    let pan_small = measure(Way::Pan, (1366, 768), (80, 25), 4, SCROLL).0;
    assert_eq!(
        pan_small,
        2 * 2 * 2 * line + 2 * 640 * 16,
        "the 80×25 grid through the pan"
    );
}

// a one-line scroll down moves the window toward the ring's start, past
// which it shows every twin at once, so the rows that come in at the top
// are written into both copies, and a row written in place where the
// window shows its twin is written into the twin alone
#[test]
fn a_one_line_scroll_down_writes_the_copies_the_window_heads_for() {
    let line = 1366 * 16;
    // the window shows twins below its first rows, and the two rows the
    // buffer lacks come in at the top, each with its margins, into both
    // copies
    let pan = measure(Way::Pan, (1366, 768), (170, 48), 4, SCROLL_BACK).0;
    assert_eq!(pan, 2 * 2 * line, "a scroll down through the pan");
    // and so it stays across the ring's start: no frame draws the screen
    // again
    let (_, _, peak, _) = measure(Way::Pan, (1366, 768), (170, 48), 120, SCROLL_BACK);
    assert_eq!(
        peak,
        2 * 2 * line,
        "the most one frame writes, past the ring's start"
    );
    // a prompt line on the bottom row, written again in place on each
    // frame, lies where the window shows twins, so only the twin is
    // written: the window shows its first copy only once the row has left
    // the screen
    let load = Load {
        status: true,
        ..SCROLL_BACK
    };
    let pan = measure(Way::Pan, (1366, 768), (170, 48), 4, load).0;
    assert_eq!(
        pan,
        2 * 2 * line + 1360 * 16,
        "a scroll down with a prompt line through the pan"
    );
}

/// The display area the overlay test draws over: across the grid's edge
/// and into the border on both displays, three rows tall.
const OVER: crate::Rect = crate::Rect {
    x: 3,
    y: 9,
    width: 30,
    height: 40,
};

/// The overlay's color.
const OVER_COLOR: u32 = 0x00ff_00ff;

/// Fills `area` of `target` with the overlay's color, as a notice does.
fn draw_over(target: &mut impl Target, (w, h): (u32, u32), area: crate::Rect) {
    let x1 = (area.x + area.width).min(w);
    let row = vec![OVER_COLOR; x1.saturating_sub(area.x) as usize];
    for y in area.y..(area.y + area.height).min(h) {
        target.write(area.x, y, &row);
    }
}

/// Runs `script` through a renderer that pans over `buffers` buffers and
/// one that copies into as many, on a display of shape `s`, drawing the
/// overlay over each frame whose flag is true. Checks each frame against
/// the redraw with the overlay drawn over it, and that the pan's damage
/// with the overlay's holds every pixel the pan wrote. Returns the first
/// failure.
fn overlay_run(
    o: Orientation,
    s: Shape,
    buffers: usize,
    script: &[(Step, bool)],
) -> Result<(), String> {
    let mut term = Term::new();
    let size = display(o, s);
    let mut pan = renderer(o, s);
    let mut copy = renderer(o, s);
    // draws each frame whole into a new buffer, keeping its glyphs
    let mut redraw = renderer(o, s);
    let mut pans = Buffers::new(buffers, pan.pan_buffer_size());
    let mut copies = Buffers::new(buffers, size);
    let mut since = Age::NEVER;
    for (n, &(step, over)) in script.iter().enumerate() {
        let mut d = Damage::new(COLS, ROWS);
        if n > 0 {
            since = d.checkpoint();
        }
        let (scrolled, whole) = term.apply(step, &mut d);
        let frame = term.frame(&d, since);
        let state = term.state();
        let mut want = Canvas::new(size.0, size.1, UNDRAWN);
        redraw.draw(&frame, &mut want, 0, &state);
        if over {
            draw_over(&mut want, size, OVER);
        }
        let what = format!("{o:?} {s:?}, {buffers} buffers, step {n} {step:?}");

        let (i, age) = pans.take();
        let mut recorded = Recorded {
            canvas: &mut pans.canvases[i],
            writes: Vec::new(),
        };
        let (drawn, src) = pan.draw_pan(&frame, &mut recorded, age, &state, scrolled, whole);
        let mut damage = drawn.damage.to_vec();
        if over {
            draw_over(&mut pan.pan_overlay(&mut recorded), size, OVER);
            pan.redraw_area(OVER);
            damage.extend(pan.pan_damage(OVER));
        }
        if let Err(at) = inside_damage(&recorded.writes, &damage) {
            return Err(format!("{what}: the pan wrote {at:?} outside its damage"));
        }
        // the display presents the frame when it or the overlay has damage
        pans.drawn(i, !damage.is_empty());
        let shown = window(&pans.canvases[i], o, s, src);
        if shown != want {
            return Err(format!(
                "{what}: the pan at {src} in buffer {i} at age {age} differs: {:?}",
                first_difference(&shown, &want)
            ));
        }

        let (i, age) = copies.take();
        let drawn = copy.draw_copy(
            &frame,
            &mut copies.canvases[i],
            age,
            &state,
            scrolled,
            whole,
        );
        let damaged = !drawn.damage.is_empty();
        if over {
            draw_over(&mut copies.canvases[i], size, OVER);
            copy.redraw_area(OVER);
        }
        copies.drawn(i, damaged || over);
        if copies.canvases[i] != want {
            return Err(format!(
                "{what}: the copy differs: {:?}",
                first_difference(&copies.canvases[i], &want)
            ));
        }
    }
    Ok(())
}

// what vtium draws over the screen through the pan lands where the window
// shows it and in the line's other copy, and the cells under it are drawn
// again on the next frame wherever a scroll moved them, so no trace of it
// stays once it goes; over the copy it is drawn into the display's buffer
#[test]
fn an_overlay_drawn_over_the_pan_moves_with_its_lines_and_goes() {
    // the overlay shows over the frames that are true, before and after
    // scrolls both ways, while the window crosses the ring's end
    let mut script: Vec<(Step, bool)> = vec![(Step::Whole, true), (Step::Write(1, 2, 'q'), true)];
    for i in 0..7 {
        script.push((Step::Scroll(1), i % 3 != 2));
    }
    for i in 0..7 {
        script.push((Step::Scroll(-1), i % 2 == 0));
    }
    script.extend([
        (Step::Write(0, 0, 'z'), true),
        (Step::Scroll(1), false),
        (Step::Write(2, 2, 'y'), false),
    ]);
    // a frame that changes nothing, then one that changes nothing under
    // the overlay: the display presents the second for the overlay alone,
    // so the buffer it drew counts as holding a frame of its own
    let still: Vec<(Step, bool)> = [
        Step::Whole,
        Step::Scroll(1),
        Step::Scroll(1),
        Step::Scroll(2),
        Step::Scroll(1),
        Step::Scroll(1),
        Step::Still,
        Step::Still,
        Step::Scroll(1),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, step)| (step, n == 7))
    .collect();
    for o in TURNS {
        for s in [TIGHT, WIDE] {
            for buffers in 0..=3 {
                for script in [&script, &still] {
                    let ran = overlay_run(o, s, buffers, script);
                    assert!(ran.is_ok(), "{}", ran.err().unwrap_or_default());
                }
            }
        }
    }
}

/// A random number from `seed`, which it moves on (xorshift64*).
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    seed.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

// random runs of scrolls, writes in place and frames that change nothing,
// some under the overlay, in every orientation and on both displays with
// one to four buffers: each frame shows what the redraw draws, with the
// overlay where it shows
#[test]
fn random_frames_under_an_overlay_show_what_the_redraw_draws() {
    const RUNS: u64 = 3000;
    // the runs are shared out among threads; each run's seed is its number
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as u64;
    let failed: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|t| {
                scope.spawn(move || {
                    (t..RUNS)
                        .step_by(threads as usize)
                        .filter_map(|run| random_overlay_run(run).err())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a run panicked"))
            .collect()
    });
    assert!(
        failed.is_empty(),
        "{} of {RUNS} runs failed, the first: {}",
        failed.len(),
        failed.first().map_or("", String::as_str)
    );
}

/// The run of [`random_frames_under_an_overlay_show_what_the_redraw_draws`]
/// whose seed is `run`.
fn random_overlay_run(run: u64) -> Result<(), String> {
    let mut seed = run.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let o = TURNS[(random(&mut seed) % 4) as usize];
    let s = [TIGHT, WIDE][(random(&mut seed) % 2) as usize];
    let buffers = [1, 2, 2, 2, 3, 3, 4][(random(&mut seed) % 7) as usize];
    let mut script = vec![(Step::Whole, false)];
    for _ in 0..14 {
        let over = random(&mut seed).is_multiple_of(4);
        match random(&mut seed) % 20 {
            0..=5 => script.push((Step::Scroll(1), over)),
            6..=9 => script.push((Step::Scroll(-1), over)),
            10 => script.push((Step::Scroll(2), over)),
            11 => script.push((Step::Scroll(-2), over)),
            12..=15 => {
                let row = (random(&mut seed) % ROWS as u64) as usize;
                let col = (random(&mut seed) % COLS as u64) as usize;
                script.push((Step::Write(row, col, 'w'), over));
            }
            // a pause: a frame that changes nothing, then one that changes
            // nothing under the overlay
            _ => script.extend([(Step::Still, false), (Step::Still, true)]),
        }
    }
    overlay_run(o, s, buffers, &script).map_err(|e| format!("run {run}: {e}"))
}

// a frame whose scroll record states another scroll than the one the
// ring is told, or cannot say how far the lines moved, rests the window
// and is drawn whole, so a caller that passes the wrong count costs a
// redraw and never shows a wrong picture
#[test]
fn a_scroll_the_record_states_otherwise_draws_the_frame_whole() {
    // the lines the view moves, the rows the ring is told, and whether
    // the frame reads its changes since a frame the record knows
    let cases = [
        (2, 1, true),
        (0, 1, true),
        (0, -1, true),
        (1, 2, true),
        (1, 1, false),
    ];
    for o in TURNS {
        for (lines, told, known) in cases {
            let mut term = Term::with(plain_line);
            let mut pan = renderer(o, WIDE);
            let rest = pan.ring_geometry().src(0);
            let (w, h) = pan.pan_buffer_size();
            let mut buf = Canvas::new(w, h, UNDRAWN);
            let state = term.state();
            // the view starts a few lines down, and a scroll moves the
            // window off the ring's start
            for (n, step) in [Step::Scroll(3), Step::Whole, Step::Scroll(1)]
                .into_iter()
                .enumerate()
            {
                let mut d = Damage::new(COLS, ROWS);
                let since = d.checkpoint();
                let (scrolled, whole) = term.apply(step, &mut d);
                let frame = term.frame(&d, since);
                pan.draw_pan(&frame, &mut buf, u32::from(n > 0), &state, scrolled, whole);
            }
            let mut d = Damage::new(COLS, ROWS);
            let since = match known {
                true => d.checkpoint(),
                false => Age::NEVER,
            };
            term.apply(Step::Scroll(lines), &mut d);
            let frame = term.frame(&d, since);
            let what = format!("{o:?}: the lines moved {lines}, the ring told {told}");
            assert_eq!(frame.scrolled(), known.then_some(lines as isize), "{what}");
            let (_, src) = pan.draw_pan(&frame, &mut buf, 1, &state, told, false);
            assert_eq!(src, rest, "{what}: the window rests");
            let shown = window(&buf, o, WIDE, src);
            let want = redrawn(o, WIDE, &frame, &state);
            assert!(
                shown == want,
                "{what}: the window at {src} differs: {:?}",
                first_difference(&shown, &want)
            );
        }
    }
}

// a screen that writes most of its cells again in place, as a program
// that redraws its whole screen does, writes no more through the pan than
// the redraw way writes, wherever the pan's window stands
#[test]
fn a_screen_rewritten_in_place_writes_no_more_through_the_pan() {
    let grid = (170, 48);
    for moved in [0, 24] {
        let load = Load {
            scroll: false,
            back: false,
            status: false,
            rewrite: Some(1),
            moved,
        };
        let redraw = measure(Way::Redraw, (1366, 768), grid, 2, load).0;
        assert_eq!(redraw, 47 * 1360 * 16, "47 rows drawn again");
        let pan = measure(Way::Pan, (1366, 768), grid, 2, load).0;
        assert!(
            pan <= redraw,
            "the window {moved} rows on: the pan writes {pan}, the redraw {redraw}"
        );
    }
}

// The figures the scroll's record asks for: pixels written and time per
// frame for a one-line scroll and for a screen rewritten in place, drawn
// the redraw way, through the pan and through the copy, with two buffers
// and 8×16 cells. `cargo test --release -p vtium-render --features
// scroll-pan --lib scroll_figures -- --ignored --nocapture` prints them.
#[test]
#[ignore = "prints figures; run in a release build"]
fn scroll_figures() {
    let screens = [((1366, 768), (170, 48)), ((1920, 1080), (240, 67))];
    let loads = [
        ("one-line scroll", SCROLL),
        (
            "in place, window at rest",
            Load {
                scroll: false,
                back: false,
                status: false,
                rewrite: Some(1),
                moved: 0,
            },
        ),
        (
            "in place, window moved",
            Load {
                scroll: false,
                back: false,
                status: false,
                rewrite: Some(1),
                moved: 30,
            },
        ),
    ];
    // the pixels are those written into the display's buffer; the copy
    // also writes each cell it draws into its ring in system memory
    println!("| Display | Frame | Way | Pixels | Cells drawn | Time |\n|---|---|---|---|---|---|");
    for (display, grid) in screens {
        for (name, load) in loads {
            for way in [Way::Redraw, Way::Pan, Way::Copy] {
                let (px, t, _, cells) = measure(way, display, grid, 200, load);
                println!(
                    "| {}×{} | {name} | {way:?} | {px} | {cells} | {:.0} µs |",
                    display.0,
                    display.1,
                    t.as_secs_f64() * 1e6
                );
            }
        }
    }
}
