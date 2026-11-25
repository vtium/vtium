//! The ring's tests: a frame shown by the pan or the copy equals the
//! frame drawn the redraw way, pixel for pixel, at every start of the
//! window, across the ring's end, at buffer ages 0 to 6, in all four
//! orientations and with narrow and wide borders; the pan's damage holds
//! every pixel it writes; a twin is drawn only when a window shows it; a
//! scroll draws only the border it shows anew; and a frame drawn whole
//! puts the window at rest.
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
                let k = n.unsigned_abs() as usize;
                match n > 0 {
                    true => d.mark_rows(ROWS.saturating_sub(k)..ROWS),
                    false => d.mark_rows(0..k),
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
