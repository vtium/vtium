//! The composition and damage at a sideways bound of two cells (ADR-1306
//! D12), and at a bound of two rows up and down: the same checks against a
//! whole draw at every buffer age, with glyphs whose ink reaches two cells
//! or two rows past their own. The font crate's glyphs cannot reach that
//! far while its bounds are one, so these glyphs are put in the cache
//! directly, stored as the renderer stores any glyph at its ink box.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "a test fails by panicking, on small known sizes"
)]

use vtium_font::{Cell, Face, Font, Format, Glyph as FontGlyph, Style, Width as FontWidth};
use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette, Rgb, Width};

use super::{Config, DrawState, Renderer};
use crate::cache::pool_of;
use crate::reach::{Bound, InkBox};
use crate::stored::Stored;
use crate::target::{Canvas, Orientation, Rect, Target};

const CW: u32 = 8;
const CH: u32 = 16;

/// Ink from `x0`, `y0` to `x1`, `y1` from the top left of the glyph's cell.
type Ink = (i32, i32, i32, i32);

/// The test glyphs: `R` reaches two cells after its own, `L` two cells
/// before it, `U` the row above and two cells to each side, `T` two rows
/// above, `>` and `<` the two cells after and before it across the same
/// lines, 11 and 12; `x` stays in its cell.
fn shape(c: char) -> &'static [Ink] {
    const BODY: Ink = (1, 6, 7, 14);
    match c {
        'R' => &[BODY, (6, 4, 24, 6)],
        'L' => &[BODY, (-16, 8, 2, 10)],
        'U' => &[BODY, (-12, -3, 20, 0)],
        'T' => &[BODY, (2, -20, 6, 0)],
        '>' => &[BODY, (8, 11, 24, 13)],
        '<' => &[BODY, (-16, 11, 0, 13)],
        _ => &[BODY],
    }
}

/// The glyphs the cache holds at their ink boxes.
const REACHING: [char; 6] = ['R', 'L', 'U', 'T', '>', '<'];

fn cov(c: char, x: i32, y: i32) -> u8 {
    let inside = |&(x0, y0, x1, y1): &Ink| x >= x0 && x < x1 && y >= y0 && y < y1;
    if shape(c).iter().any(inside) { 255 } else { 0 }
}

/// `c` stored at its ink box, as the cache stores a glyph from the font.
fn stored(c: char) -> Stored {
    let (mut l, mut t, mut r, mut b) = (0, 0, CW as i32, CH as i32);
    for &(x0, y0, x1, y1) in shape(c) {
        (l, t, r, b) = (l.min(x0), t.min(y0), r.max(x1), b.max(y1));
    }
    let pixels: Vec<u8> = (t..b)
        .flat_map(|y| (l..r).map(move |x| cov(c, x, y)))
        .collect();
    let ink = InkBox {
        left: l,
        top: t,
        width: (r - l) as u32,
        height: (b - t) as u32,
        pixels: &pixels,
    };
    Stored::boxed(ink, (CW, CH, 1), 1, Orientation::Normal, true)
}

/// A face that draws `x`; the reaching glyphs come from the cache.
struct Plain;

impl Face for Plain {
    fn cell(&self) -> Cell {
        Cell {
            width: CW,
            height: CH,
        }
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, c: char) -> bool {
        c.is_ascii_graphic()
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<FontGlyph> {
        let c = *cluster.first()?;
        let own = (0..CH as i32)
            .flat_map(|y| (0..CW as i32).map(move |x| cov(c, x, y)))
            .collect();
        FontGlyph::from_pixels(CW, CH, Format::Mask, own)
    }
}

/// A renderer for a grid of `cols` by `rows` at `bound`, its cache holding
/// the reaching glyphs.
fn renderer(cols: usize, rows: usize, bound: Bound) -> Renderer<Plain> {
    let (w, h) = (cols as u32 * CW, rows as u32 * CH);
    let mut r = Renderer::new(Font::new(Plain), w, h, Config::default());
    r.bound = bound;
    for c in REACHING {
        let g = stored(c);
        let pool = pool_of(false, false);
        r.compose
            .cache
            .keep((&[c], FontWidth::Narrow, Style::default()), pool, g, false);
    }
    r
}

const WIDER: Bound = Bound {
    side: 2,
    rows: Bound::ROWS,
};

const TALLER: Bound = Bound { side: 2, rows: 2 };

/// The same checks on the GPU path.
#[cfg(feature = "gpu")]
mod gpu;

struct Screen {
    cols: usize,
    lines: Vec<Line>,
    damage: Damage,
    palette: Palette,
}

impl Screen {
    fn new(cols: usize, rows: usize) -> Screen {
        Screen {
            cols,
            lines: (0..rows)
                .map(|r| Line::new(LineId::new(r as u64 + 1), cols))
                .collect(),
            damage: Damage::new(cols, rows),
            palette: Palette::new(
                [Rgb::default(); 256],
                Rgb::new(200, 200, 200),
                Rgb::new(16, 16, 16),
            ),
        }
    }

    fn put(&mut self, col: usize, row: usize, c: char, fg: Rgb) {
        let style = vtium_term::Style {
            fg: vtium_term::Color::Rgb(fg),
            ..vtium_term::Style::DEFAULT
        };
        self.lines[row].write(col, Glyph::Char(c), Width::Narrow, &style, None);
        self.damage.mark_cell(row, col);
    }

    fn frame(&self, since: Age) -> Frame<'_> {
        Frame::new(
            self.lines.iter().collect(),
            &self.damage,
            &self.palette,
            since,
            Cursor::default(),
            false,
        )
    }
}

fn whole(s: &Screen, bound: Bound) -> Canvas {
    let rows = s.lines.len();
    let mut r = renderer(s.cols, rows, bound);
    let mut c = Canvas::new(s.cols as u32 * CW, rows as u32 * CH, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &DrawState::default());
    c
}

/// The cells `damage` covers.
fn cells(damage: &[Rect], s: &Screen) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for row in 0..s.lines.len() as u32 {
        for col in 0..s.cols as u32 {
            let (x, y) = (col * CW, row * CH);
            if damage
                .iter()
                .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
            {
                out.push((col, row));
            }
        }
    }
    out
}

const RED: Rgb = Rgb::new(220, 30, 30);

#[test]
fn a_sideways_bound_of_two_draws_and_damages_two_cells_beside_a_glyph() {
    let mut s = Screen::new(7, 3);
    // R at column 1 leans into columns 2 and 3 on its line 4; L at
    // column 5 into columns 4 and 3 on its line 8
    for (c, col, near, far, line) in [('R', 1, 2, 3, 4), ('L', 5, 4, 3, 8)] {
        for (bound, reached) in [(WIDER, true), (Bound::DRAWN, false)] {
            let mut r = renderer(7, 3, bound);
            let mut canvas = Canvas::new(7 * CW, 3 * CH, 0);
            r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &DrawState::default());
            let since = s.damage.checkpoint();
            s.put(col, 1, c, RED);
            let drawn = r.draw(&s.frame(since), &mut canvas, 1, &DrawState::default());
            let mut want = vec![(col as u32, 1), (near, 1)];
            if reached {
                want.push((far, 1));
            }
            want.sort_unstable();
            assert_eq!(cells(drawn.damage, &s), want, "{c} at {bound:?}");
            let at = |cell: u32| canvas.pixel(cell * CW + 3, CH + line).unwrap();
            assert_eq!(at(near), 0xdc1e1e, "{c} at {bound:?}");
            let past = if reached { 0xdc1e1e } else { 0x101010 };
            assert_eq!(at(far), past, "{c} at {bound:?}");
            let since = s.damage.checkpoint();
            s.put(col, 1, ' ', RED);
            r.draw(&s.frame(since), &mut canvas, 1, &DrawState::default());
            s.damage.checkpoint();
            assert!(canvas == whole(&s, bound), "{c} at {bound:?}");
        }
    }
}

/// Records the rectangles a frame wrote.
struct Recorder {
    canvas: Canvas,
    writes: Vec<Rect>,
}

impl Target for Recorder {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.writes.push(Rect {
            x,
            y,
            width: pixels.len() as u32,
            height: 1,
        });
        self.canvas.write(x, y, pixels);
    }
}

#[test]
fn every_frame_matches_a_whole_draw_at_a_sideways_bound_of_two() {
    for bound in [WIDER, TALLER] {
        typing(bound);
    }
}

/// A script typing and erasing the reaching glyphs at `bound`, each frame
/// checked against a whole draw, at 1, 2 and 3 buffers.
fn typing(bound: Bound) {
    let (cols, rows) = (10usize, 6usize);
    for buffers in 1..=3usize {
        let mut s = Screen::new(cols, rows);
        let mut r = renderer(cols, rows, bound);
        let mut bufs: Vec<Recorder> = (0..buffers)
            .map(|_| Recorder {
                canvas: Canvas::new(cols as u32 * CW, rows as u32 * CH, 0xdead_beef),
                writes: Vec::new(),
            })
            .collect();
        let (mut held, mut frames, mut back) = (vec![0u64; buffers], 0u64, 0usize);
        let mut since = Age::NEVER;
        let mut seed = 0x2545_f491u32 ^ buffers as u32;
        let mut rand = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed % n
        };
        for step in 0..150 {
            for _ in 0..rand(4) {
                let c = ['R', 'L', 'U', 'T', '>', '<', 'x', ' '][rand(8) as usize];
                let fg = Rgb::new(rand(256) as u8, 100, 200);
                s.put(
                    rand(cols as u32) as usize,
                    rand(rows as u32) as usize,
                    c,
                    fg,
                );
            }
            if rand(12) == 0 {
                held[back] = 0;
            }
            let age = match held[back] {
                0 => 0,
                h => (frames - h + 1) as u32,
            };
            let buf = &mut bufs[back];
            buf.writes.clear();
            let damage = r
                .draw(&s.frame(since), buf, age, &DrawState::default())
                .damage
                .to_vec();
            since = s.damage.checkpoint();
            for w in &buf.writes {
                let inside = (w.x..w.x + w.width).all(|x| {
                    damage.iter().any(|d| {
                        x >= d.x && w.y >= d.y && x < d.x + d.width && w.y < d.y + d.height
                    })
                });
                assert!(inside, "step {step}: a write {w:?} outside the damage");
            }
            assert!(
                buf.canvas == whole(&s, bound),
                "{bound:?}, step {step}, {buffers} buffers at age {age}: differs from a whole draw"
            );
            if !damage.is_empty() {
                frames += 1;
                held[back] = frames;
                back = (back + 1) % buffers;
            }
        }
    }
}

// D4's step 5 and D12: the ink of the cells beside a cell is drawn farthest
// first and, at one distance, the cell before it first, so the nearest cell
// after it lies on top where all four meet.
#[test]
fn ink_from_beside_is_drawn_farthest_first_and_the_cell_before_first() {
    let mut s = Screen::new(7, 1);
    let (far_before, near_before) = (Rgb::new(250, 0, 0), Rgb::new(0, 250, 0));
    let (near_after, far_after) = (Rgb::new(0, 0, 250), Rgb::new(250, 250, 0));
    s.put(1, 0, '>', far_before);
    s.put(2, 0, '>', near_before);
    s.put(4, 0, '<', near_after);
    s.put(5, 0, '<', far_after);
    let c = whole(&s, WIDER);
    // column 3 draws nothing of its own; all four reach its line 11
    assert_eq!(c.pixel(3 * CW + 4, 11).unwrap(), 0x0000fa);
    // where only the two farthest meet, the cell after lies on top: column
    // 3 is the one cell both reach, so remove the nearer two
    s.put(2, 0, ' ', near_before);
    s.put(4, 0, ' ', near_after);
    let c = whole(&s, WIDER);
    assert_eq!(c.pixel(3 * CW + 4, 11).unwrap(), 0xfafa00);
}

#[test]
fn a_bound_of_two_rows_draws_and_damages_two_rows_above_a_glyph() {
    let mut s = Screen::new(5, 4);
    for (bound, reached) in [(TALLER, true), (WIDER, false)] {
        let mut r = renderer(5, 4, bound);
        let mut canvas = Canvas::new(5 * CW, 4 * CH, 0);
        r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &DrawState::default());
        let since = s.damage.checkpoint();
        s.put(2, 3, 'T', RED);
        let drawn = r.draw(&s.frame(since), &mut canvas, 1, &DrawState::default());
        let mut want = vec![(2, 2), (2, 3)];
        if reached {
            want.insert(0, (2, 1));
        }
        assert_eq!(cells(drawn.damage, &s), want, "{bound:?}");
        // the ink's lines 12 to 15 of the row two above, and of the row above
        let two = canvas.pixel(2 * CW + 3, CH + 14).unwrap();
        let one = canvas.pixel(2 * CW + 3, 2 * CH + 4).unwrap();
        assert_eq!(one, 0xdc1e1e, "{bound:?}");
        assert_eq!(two, if reached { 0xdc1e1e } else { 0x101010 }, "{bound:?}");
        let since = s.damage.checkpoint();
        s.put(2, 3, ' ', RED);
        r.draw(&s.frame(since), &mut canvas, 1, &DrawState::default());
        s.damage.checkpoint();
        assert!(canvas == whole(&s, bound), "{bound:?}");
    }
}
