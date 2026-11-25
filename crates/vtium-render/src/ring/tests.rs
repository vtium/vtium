//! The ring's tests: a frame shown by the pan or the copy equals the
//! frame drawn the redraw way, pixel for pixel, at every start of the
//! window, across the ring's end, at buffer ages 0 to 3 and in all four
//! orientations; a twin is drawn only when a window shows it; and a frame
//! drawn whole puts the window at rest.
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
/// The display's length along the rows: 4 rows of 16 pixels and 7 spare
/// lines, so the grid has a border strip on both sides.
const ALONG: u32 = 4 * 16 + 7;
/// Its length across them: 6 cells of 8 pixels and 5 spare columns.
const ACROSS: u32 = 6 * 8 + 5;
/// What a buffer holds before anything is drawn into it, so that a pixel
/// the ring should have drawn and did not shows.
const UNDRAWN: u32 = 0x00ab_cdef;
/// A glyph whose ink reaches 6 pixels into the row above and the row
/// below, so that the rows around a scroll trade ink.
const TALL: char = '¦';

const TURNS: [Orientation; 4] = [
    Orientation::Normal,
    Orientation::Right,
    Orientation::UpsideDown,
    Orientation::Left,
];

fn display(o: Orientation) -> (u32, u32) {
    match o.turned() {
        true => (ALONG, ACROSS),
        false => (ACROSS, ALONG),
    }
}

fn renderer(o: Orientation) -> Renderer<Unifont> {
    let (w, h) = display(o);
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
    r
}

/// A console's lines, the view over them, and what the next frame shows.
struct Term {
    lines: Vec<Line>,
    top: usize,
    palette: Palette,
    cursor: Cursor,
    pointer: Option<(u32, u32)>,
    #[cfg(feature = "images")]
    store: vtium_image::Store,
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

impl Term {
    fn new() -> Term {
        let mut colors = [Rgb::default(); 256];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = Rgb::new(i as u8, (i * 3) as u8, (i * 7) as u8);
        }
        let t = Term {
            lines: (0..ROWS).map(line).collect(),
            top: 0,
            palette: Palette::new(colors, Rgb::new(0xc0, 0xc0, 0xc0), Rgb::new(0, 0, 0x30)),
            cursor: Cursor {
                row: ROWS - 1,
                col: 2,
                visible: true,
                ..Cursor::default()
            },
            pointer: Some((13, 21)),
            #[cfg(feature = "images")]
            store: vtium_image::Store::new(),
        };
        #[cfg(feature = "images")]
        let t = {
            let mut t = t;
            t.place_picture();
            t
        };
        t
    }

    /// A picture two rows tall on the screen's second line, from its
    /// third column.
    #[cfg(feature = "images")]
    fn place_picture(&mut self) {
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
                .map(|i| 0xff00_0000 | (i * 7919) & 0x00ff_ffff)
                .collect(),
            id: 1,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = self
            .store
            .add_image(new, None, screens())
            .expect("the store takes the image");
        let place = Place {
            col: 2,
            row: 1,
            ..Place::default()
        };
        self.store
            .place(image, place, screens(), COLS)
            .expect("the store places it");
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
                    self.lines.push(line(self.lines.len()));
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
            cursor_on: true,
            background: self.palette.bg(),
            pointer: self.pointer,
            ..DrawState::default()
        }
    }
}

/// The display's pixels that the window starting at `src` shows of the
/// pan's buffer `c`.
fn window(c: &Canvas, o: Orientation, src: u32) -> Canvas {
    let (w, h) = display(o);
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
fn redrawn(o: Orientation, frame: &Frame<'_>, state: &DrawState) -> Canvas {
    let (w, h) = display(o);
    let mut c = Canvas::new(w, h, UNDRAWN);
    renderer(o).draw(frame, &mut c, 0, state);
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
/// writes in place, the cursor and the arrow moving.
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
    s
}

/// Runs `steps` through a renderer that pans over `buffers` buffers, or a
/// new buffer each frame for 0, and through one that copies into as many,
/// and checks each frame against the redraw. Returns the windows' starts.
fn run(o: Orientation, buffers: usize, steps: &[Step]) -> Vec<u32> {
    let mut term = Term::new();
    let mut pan = renderer(o);
    let mut copy = renderer(o);
    let mut pans = Buffers::new(buffers, pan.pan_buffer_size());
    let mut copies = Buffers::new(buffers, display(o));
    let mut starts = Vec::new();
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
        let want = redrawn(o, &frame, &state);

        let (i, age) = pans.take();
        let (drawn, src) =
            pan.draw_pan(&frame, &mut pans.canvases[i], age, &state, scrolled, whole);
        let damaged = !drawn.damage.is_empty();
        pans.drawn(i, damaged);
        starts.push(src);
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
        let shown = window(&pans.canvases[i], o, src);
        assert!(
            shown == want,
            "{o:?}, {buffers} buffers, step {n} {step:?}: the pan at {src} differs from the redraw: {:?}",
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
            "{o:?}, {buffers} buffers, step {n} {step:?}: the copy differs from the redraw: {:?}",
            first_difference(&copies.canvases[i], &want)
        );
    }
    starts
}

// ADR-1310, Verification, "The pan and the copy match the redraw"
#[test]
fn the_pan_and_the_copy_show_what_the_redraw_draws() {
    let steps = steps();
    for o in TURNS {
        let slots = renderer(o).ring_geometry().slots;
        for buffers in 0..=3 {
            let starts = run(o, buffers, &steps);
            let mut seen = starts.clone();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(
                seen.len(),
                slots as usize,
                "{o:?}, {buffers} buffers: every start of the window is shown: {starts:?}"
            );
        }
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
        let mut pan = renderer(o);
        let mut redraw = renderer(o);
        let mut pans = Buffers::new(2, pan.pan_buffer_size());
        let mut redraws = Buffers::new(2, display(o));
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
            let shown = window(&pans.canvases[i], o, src);
            let want = redrawn(o, &frame, &state);
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

// ADR-1310 D6, "The window at rest"
#[test]
fn a_frame_drawn_whole_puts_the_window_at_rest() {
    for o in TURNS {
        let mut term = Term::new();
        let mut pan = renderer(o);
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
                window(&buf, o, src) == redrawn(o, &frame, &state),
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
    assert_eq!(renderer(Orientation::Normal).pan_buffer_size(), (53, 151));
    assert_eq!(renderer(Orientation::Right).pan_buffer_size(), (151, 53));
    assert_eq!(renderer(Orientation::Normal).copy_ring_bytes(), 53 * 80 * 4);
}

impl<F: vtium_font::Face> Renderer<F> {
    /// The ring's shape for this renderer's display.
    fn ring_geometry(&self) -> super::Geometry {
        super::Geometry::new(&self.layout, &super::Grid::default())
    }
}
