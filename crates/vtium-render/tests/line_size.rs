//! Double-width and double-height lines (DECDWL, DECDHL) on the CPU path.
//!
//! A line of double size draws each of its cells across two cells of the
//! display, with each pixel of the glyph doubled: twice as wide on a
//! double-width line, and twice as wide and twice as tall on a
//! double-height line, whose top line shows the glyph's upper half and
//! whose bottom line shows its lower half. These tests set each line's
//! size directly, as the escape sequences do in vtium-term.
//!
//! The first tests check the glyph's pixels against the font's own
//! pattern. The later ones check that every part of a cell, its cursor,
//! decorations, selection and colors included, draws as the same cell at
//! the normal size with each pixel doubled, and that each buffer of every
//! age gets what it lacks.

mod common;

use common::{Screen, rgb, style};
use vtium_font::{
    Cell, Doubled, Face, Font, Format, Glyph, Script, Style, Variant, Width as FontWidth,
};
use vtium_render::{Canvas, Config, DrawState, Rect, Renderer};
use vtium_term::{Age, Attrs, Color, CursorShape, LineSize, UnderlineStyle};

const CW: u32 = 8;
const CH: u32 = 16;
const FG: &str = "e0e0e0";
const BG: &str = "102030";

/// Whether the test glyph of `c` lights its pixel at `x`, `y`: a pattern
/// that differs at every pixel of a cell, so a pixel drawn from the wrong
/// place shows. `W` is a wide glyph, 16 pixels wide; a space draws nothing.
fn lit(c: char, x: u32, y: u32) -> bool {
    c != ' ' && (x * 3 + y * 5 + u32::from(c)) % 7 < 3
}

/// A bitmap face in 8x16 cells that draws [`lit`]'s pattern, with ink
/// above its cell for `A` when it `reaches`: an accent over the row above,
/// as an outline font draws one.
struct Dots {
    reaches: bool,
}

impl Face for Dots {
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
        c.is_ascii_graphic() || c == ' '
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<Glyph> {
        let c = *cluster.first()?;
        let (w, width) = if c == 'W' {
            (2 * CW, FontWidth::Wide)
        } else {
            (CW, FontWidth::Narrow)
        };
        let px = |x: u32, y: u32| if lit(c, x, y) { 255 } else { 0 };
        let own = (0..CH)
            .flat_map(|y| (0..w).map(move |x| px(x, y)))
            .collect();
        let g = Glyph::from_pixels(w, CH, Format::Mask, own)?.with_cells(width);
        if !(self.reaches && c == 'A') {
            return Some(g);
        }
        // an accent 4 rows into the row above, over the glyph's own rows
        let boxed = (0..CH + 4)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    if y < 4 {
                        255 * u8::from(x % 3 == 0)
                    } else {
                        px(x, y - 4)
                    }
                })
            })
            .collect();
        g.with_ink((0, -4), (w, CH + 4), boxed)
    }
}

/// A screen of `cols` by `rows` with an invisible cursor.
fn screen(cols: usize, rows: usize) -> Screen {
    let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    s
}

fn plain() -> vtium_term::Style {
    style(rgb(FG), rgb(BG))
}

/// Sets the size of `row`, and marks its cells changed, as vtium-term does
/// when a program sets a line's size.
fn size(s: &mut Screen, row: usize, size: LineSize) {
    s.lines[row].flags_mut().set_size(size);
    s.damage.mark_cells(row, 0..s.cols);
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s`'s display: its grid inside a border of 2 pixels across and 1 down.
fn display(s: &Screen) -> (u32, u32) {
    (s.cols as u32 * CW + 4, s.rows as u32 * CH + 2)
}

/// `s` drawn whole with `face`, with the selection `sel`.
fn whole_with(s: &Screen, face: Dots, sel: &[std::ops::Range<usize>]) -> Canvas {
    let (w, h) = display(s);
    let mut r = Renderer::new(Font::new(face), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let frame = s.frame(Age::NEVER).with_selection(sel.to_vec());
    r.draw(&frame, &mut c, 0, &state(s));
    c
}

fn whole(s: &Screen) -> Canvas {
    whole_with(s, Dots { reaches: false }, &[])
}

/// The pixel at `x`, `y` of the grid, from its top left.
fn at(c: &Canvas, x: u32, y: u32) -> u32 {
    c.pixel(x + 2, y + 1).unwrap()
}

fn px(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

/// The pixel the test glyph of `c` draws at `x`, `y` of its cells, in the
/// screen's colors.
fn glyph_px(c: char, x: u32, y: u32) -> u32 {
    if lit(c, x, y) { px(FG) } else { px(BG) }
}

/// Fails unless each pixel of grid row `row` of `got` is `want`'s for its
/// column and line within the row.
fn assert_row(got: &Canvas, cols: usize, row: u32, want: impl Fn(u32, u32) -> u32, what: &str) {
    for y in 0..CH {
        for x in 0..cols as u32 * CW {
            let g = at(got, x, row * CH + y);
            assert_eq!(g, want(x, y), "{what}: pixel {x},{y} of row {row}");
        }
    }
}

// DECDWL: each cell of a double-width line draws across two cells, each
// pixel of its glyph twice as wide and as tall as before. The cells past
// the middle of the line lie past the grid's right edge, so they show
// nothing, and the line below keeps its normal size.
// covers: text.line-size
#[test]
fn a_double_width_line_draws_each_pixel_twice_as_wide() {
    let mut s = screen(6, 2);
    s.text(0, 0, "ab", plain());
    s.text(4, 0, "zz", plain());
    s.text(0, 1, "ab", plain());
    size(&mut s, 0, LineSize::DoubleWidth);
    let c = whole(&s);
    let text = ['a', 'b', ' '];
    assert_row(
        &c,
        6,
        0,
        |x, y| glyph_px(text[(x / (2 * CW)) as usize], (x % (2 * CW)) / 2, y),
        "double width",
    );
    let normal = ['a', 'b', ' ', ' ', ' ', ' '];
    assert_row(
        &c,
        6,
        1,
        |x, y| glyph_px(normal[(x / CW) as usize], x % CW, y),
        "normal",
    );
}

// DECDHL: a double-height pair draws each glyph twice as wide and twice
// as tall, the glyph's upper half on the top line and its lower half on
// the bottom line.
// covers: text.line-size
#[test]
fn a_double_height_pair_draws_the_upper_half_above_the_lower() {
    let mut s = screen(4, 2);
    s.text(0, 0, "ab", plain());
    s.text(0, 1, "ab", plain());
    size(&mut s, 0, LineSize::DoubleHeightTop);
    size(&mut s, 1, LineSize::DoubleHeightBottom);
    let c = whole(&s);
    let cell = |x: u32| ['a', 'b'][(x / (2 * CW)) as usize];
    assert_row(
        &c,
        4,
        0,
        |x, y| glyph_px(cell(x), (x % (2 * CW)) / 2, y / 2),
        "top",
    );
    assert_row(
        &c,
        4,
        1,
        |x, y| glyph_px(cell(x), (x % (2 * CW)) / 2, (y + CH) / 2),
        "bottom",
    );
}

// A wide character on a double-width line draws across four cells.
// covers: text.line-size
#[test]
fn a_wide_character_on_a_double_width_line_spans_four_cells() {
    let mut s = screen(4, 1);
    s.put(0, 0, 'W', true, plain());
    size(&mut s, 0, LineSize::DoubleWidth);
    let c = whole(&s);
    assert_row(&c, 4, 0, |x, y| glyph_px('W', x / 2, y), "wide");
}

/// Fails unless row `row` of `doubled`, where the row has size `size`,
/// draws as row `row` of `normal` with each pixel doubled: twice as wide,
/// and for a double-height line, the upper or lower half twice as tall.
fn assert_doubled(normal: &Canvas, doubled: &Canvas, cols: usize, row: u32, size: LineSize) {
    let line = |y: u32| match size {
        LineSize::DoubleHeightTop => y / 2,
        LineSize::DoubleHeightBottom => (y + CH) / 2,
        _ => y,
    };
    let what = format!("{size:?}");
    assert_row(
        doubled,
        cols,
        row,
        |x, y| at(normal, x / 2, row * CH + line(y)),
        &what,
    );
}

/// A row of every decoration, a wide character, colors and reverse video:
/// what the doubled row must draw as its normal row doubled.
fn decorated(s: &mut Screen, row: usize) {
    let mut st = plain();
    s.text(0, row, "ab", st);
    st.attrs.set_underline(UnderlineStyle::Single);
    s.put(2, row, 'c', false, st);
    st.attrs.set_underline(UnderlineStyle::Double);
    s.put(3, row, 'd', false, st);
    st.attrs.set_underline(UnderlineStyle::Curly);
    s.put(4, row, 'e', false, st);
    st.attrs.set_underline(UnderlineStyle::Dotted);
    st.attrs.insert(Attrs::STRIKE);
    s.put(5, row, 'f', false, st);
    st.attrs.set_underline(UnderlineStyle::Dashed);
    st.attrs.insert(Attrs::OVERLINE);
    s.put(6, row, 'W', true, st);
    let mut colored = plain();
    colored.fg = Color::Rgb(rgb("ff4000"));
    colored.bg = Color::Rgb(rgb("004080"));
    s.put(8, row, 'g', false, colored);
    colored.attrs.insert(Attrs::INVERSE);
    s.put(9, row, 'h', false, colored);
}

// Each part of a cell on a line of double size draws as the cell at the
// normal size with each pixel doubled: its glyph, its underline in each
// style, its strike-through and overline, a wide character, its colors
// and reverse video.
// covers: text.line-size
#[test]
fn every_decoration_and_color_doubles_with_the_line() {
    for size_of in [
        LineSize::DoubleWidth,
        LineSize::DoubleHeightTop,
        LineSize::DoubleHeightBottom,
    ] {
        let mut normal = screen(20, 1);
        decorated(&mut normal, 0);
        let mut doubled = screen(20, 1);
        decorated(&mut doubled, 0);
        size(&mut doubled, 0, size_of);
        assert_doubled(&whole(&normal), &whole(&doubled), 20, 0, size_of);
    }
}

// The cursor in each shape, and the selection, follow the line's size:
// a block covers both display cells of its cell, an underline cursor
// lies under them, and a bar is twice as wide.
// covers: text.line-size
#[test]
fn the_cursor_and_the_selection_double_with_the_line() {
    let shapes = [
        (CursorShape::Block, None),
        (CursorShape::Block, Some(rgb("30ff30"))),
        (CursorShape::Underline, None),
        (CursorShape::Bar, None),
    ];
    for size_of in [
        LineSize::DoubleWidth,
        LineSize::DoubleHeightTop,
        LineSize::DoubleHeightBottom,
    ] {
        for (shape, color) in shapes {
            for col in [1, 6] {
                let make = |doubled: bool| {
                    let mut s = screen(20, 1);
                    decorated(&mut s, 0);
                    s.cursor.visible = true;
                    (s.cursor.col, s.cursor.row) = (col, 0);
                    s.cursor.shape = shape;
                    s.cursor.color = color;
                    if doubled {
                        size(&mut s, 0, size_of);
                    }
                    s
                };
                let normal = whole(&make(false));
                let doubled = whole(&make(true));
                assert_doubled(&normal, &doubled, 20, 0, size_of);
            }
        }
        let mut normal = screen(20, 1);
        decorated(&mut normal, 0);
        let mut doubled = screen(20, 1);
        decorated(&mut doubled, 0);
        size(&mut doubled, 0, size_of);
        // the columns selected in the one row
        let sel: Vec<_> = std::iter::once(1..4).collect();
        let n = whole_with(&normal, Dots { reaches: false }, &sel);
        let d = whole_with(&doubled, Dots { reaches: false }, &sel);
        assert_doubled(&n, &d, 20, 0, size_of);
    }
}

// A line of double size keeps its glyphs' ink in their cells: the accent
// of the `A` below draws nothing into it, where it draws into a line of
// the normal size.
// covers: text.line-size
#[test]
fn a_line_of_double_size_receives_no_ink_from_its_neighbours() {
    let make = |accent: bool, size_of: LineSize| {
        let mut s = screen(4, 2);
        s.text(0, 0, "ab", plain());
        s.text(0, 1, if accent { "A" } else { " " }, plain());
        size(&mut s, 0, size_of);
        whole_with(&s, Dots { reaches: true }, &[])
    };
    let row0 = |c: &Canvas| {
        let line = |y| (0..4 * CW).map(move |x| at(c, x, y));
        (0..CH).flat_map(line).collect::<Vec<_>>()
    };
    let doubled = LineSize::DoubleWidth;
    assert_eq!(row0(&make(true, doubled)), row0(&make(false, doubled)));
    assert_ne!(
        row0(&make(true, LineSize::Single)),
        row0(&make(false, LineSize::Single))
    );
}

// A line whose size changes draws again whole in every buffer, and so
// does the line its glyphs' ink reached; a change on a line of double
// size draws the line whole. This holds with two buffers that take turns
// and with three.
// covers: text.line-size
#[test]
fn every_buffer_gets_what_it_lacks_as_lines_change_size() {
    for n in [2, 3] {
        lines_change_size(n);
    }
}

/// The steps of [`every_buffer_gets_what_it_lacks_as_lines_change_size`],
/// drawn into `n` buffers in turn, each as old as `n` frames once drawn.
fn lines_change_size(n: usize) {
    let mut s = screen(8, 3);
    s.text(0, 0, "abcdefgh", plain());
    s.text(0, 1, "Aijklmno", plain());
    s.text(0, 2, "pqrstuvw", plain());
    let (w, h) = display(&s);
    let mut r = Renderer::new(Font::new(Dots { reaches: true }), w, h, Config::default());
    let mut buffers: Vec<Canvas> = (0..n).map(|_| Canvas::new(w, h, 0)).collect();
    let mut since = Age::NEVER;
    type Step = fn(&mut Screen);
    let steps: [(&str, Step); 8] = [
        ("the screen", |_| {}),
        ("the screen again", |_| {}),
        ("the screen a third time", |_| {}),
        ("the accent's line doubles", |s| {
            size(s, 1, LineSize::DoubleWidth)
        }),
        ("a letter on the doubled line", |s| {
            s.put(2, 1, 'z', false, plain())
        }),
        ("the cursor on the doubled line", |s| {
            s.cursor.visible = true;
            (s.cursor.col, s.cursor.row) = (1, 1);
            s.damage.mark_cell(1, 1);
        }),
        ("a double-height pair", |s| {
            size(s, 0, LineSize::DoubleHeightTop);
            size(s, 1, LineSize::DoubleHeightBottom);
        }),
        ("every line normal", |s| {
            size(s, 0, LineSize::Single);
            size(s, 1, LineSize::Single);
        }),
    ];
    for (i, (what, step)) in steps.into_iter().enumerate() {
        step(&mut s);
        let age = if i < n { 0 } else { n as u32 };
        let buffer = &mut buffers[i % n];
        let frame = s.frame(since);
        let damage = r.draw(&frame, buffer, age, &state(&s)).damage.to_vec();
        since = s.damage.checkpoint();
        let want = whole_with(&s, Dots { reaches: true }, &[]);
        assert!(
            *buffer == want,
            "{what}, {n} buffers: the buffer differs from a whole draw"
        );
        if what == "a letter on the doubled line" {
            let row = (2, 1 + CH, 2 + 8 * CW, 1 + 2 * CH);
            let holds = |d: &Rect| {
                d.x <= row.0 && d.y <= row.1 && d.x + d.width >= row.2 && d.y + d.height >= row.3
            };
            assert!(
                damage.iter().any(holds),
                "{what}: the damage {damage:?} does not hold the whole line"
            );
        }
    }
}

/// Whether the glyph of `c` that [`Rendered`] holds at a double size
/// lights its pixel at `x`, `y` of the doubled cells: a pattern of its own
/// at that size, which no doubling of [`lit`]'s pixels gives.
fn lit_doubled(c: char, x: u32, y: u32) -> bool {
    (x * 5 + y * 3 + u32::from(c)) % 11 < 4
}

/// [`Dots`], whose face also holds the glyphs of `a` and `W` rendered at
/// each double size, as the font helper renders an outline font's glyphs.
/// As the console's face does, it holds none for another character, for
/// a cluster, or for a superscript or subscript.
struct Rendered;

impl Face for Rendered {
    fn cell(&self) -> Cell {
        Dots { reaches: false }.cell()
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, c: char) -> bool {
        Dots { reaches: false }.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        Dots { reaches: false }.draw(cluster, style)
    }
    fn doubled(&self, cluster: &[char], variant: Variant, doubled: Doubled) -> Option<Glyph> {
        let &[c] = cluster else {
            return None;
        };
        if !matches!(c, 'a' | 'W') || variant.script != Script::Baseline {
            return None;
        }
        let width = if c == 'W' {
            FontWidth::Wide
        } else {
            FontWidth::Narrow
        };
        let (w, h) = (2 * CW * width.cells(), CH * doubled.rows());
        let px = |x: u32, y: u32| if lit_doubled(c, x, y) { 255 } else { 0 };
        let own = (0..h).flat_map(|y| (0..w).map(move |x| px(x, y))).collect();
        Some(Glyph::from_pixels(w, h, Format::Mask, own)?.with_cells(width))
    }
}

/// `s` drawn whole in [`Rendered`].
fn whole_rendered(s: &Screen) -> Canvas {
    let (w, h) = display(s);
    let mut r = Renderer::new(Font::new(Rendered), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    c
}

/// The pixel [`Rendered`]'s glyph of `c` at a double size draws at `x`,
/// `y` of its doubled cells, in the screen's colors.
fn doubled_px(c: char, x: u32, y: u32) -> u32 {
    if lit_doubled(c, x, y) { px(FG) } else { px(BG) }
}

// issue #61, the maintainer's direction of 2026-11-14: where the face
// holds a glyph rendered at double size, as the font helper renders an
// outline font's, a line of double size draws that glyph's pieces as they
// are. A double-width line draws each display cell from its column of
// the glyph, a wide character's across four cells, and a double-height
// pair draws the glyph's upper rows above and its lower rows below. A
// character the face holds no such glyph for doubles its pixels, as a
// bitmap font's does.
// covers: text.line-size
#[test]
fn a_glyph_rendered_at_double_size_draws_its_pieces_as_they_are() {
    let mut s = screen(8, 3);
    s.text(0, 0, "ab", plain());
    s.put(2, 0, 'W', true, plain());
    s.text(0, 1, "ab", plain());
    s.text(0, 2, "ab", plain());
    size(&mut s, 0, LineSize::DoubleWidth);
    size(&mut s, 1, LineSize::DoubleHeightTop);
    size(&mut s, 2, LineSize::DoubleHeightBottom);
    let c = whole_rendered(&s);
    let wide = |x: u32, y: u32| match x / (2 * CW) {
        0 => doubled_px('a', x, y),
        1 => glyph_px('b', (x % (2 * CW)) / 2, y),
        _ => doubled_px('W', x - 4 * CW, y),
    };
    assert_row(&c, 8, 0, wide, "double width");
    let tall = |half: u32| {
        move |x: u32, y: u32| match x / (2 * CW) {
            0 => doubled_px('a', x, y + half * CH),
            1 => glyph_px('b', (x % (2 * CW)) / 2, (y + half * CH) / 2),
            _ => px(BG),
        }
    };
    assert_row(&c, 8, 1, tall(0), "top");
    assert_row(&c, 8, 2, tall(1), "bottom");
}

/// Fails unless rows 0 and 1 of `pair`, a pair of double-height lines,
/// draw as row 0 of `normal` doubled: its upper half on row 0 and its
/// lower half on row 1.
fn assert_pair(normal: &Canvas, pair: &Canvas, cols: usize, what: &str) {
    for (row, half) in [(0, 0), (1, CH)] {
        assert_row(
            pair,
            cols,
            row,
            |x, y| at(normal, x / 2, (y + half) / 2),
            &format!("{what}, line {row}"),
        );
    }
}

// The maintainer's look at the double-height cursor screens, 2026-11-14:
// on a pair of double-height lines (ESC # 3 then ESC # 4), the cursor
// covers the whole doubled character, whichever line of the pair it
// stands on. A block covers both halves, a bar runs down both lines, and
// an underline lies under the bottom half, so the pair draws as its line
// at the normal size, cursor and all, doubled.
// covers: text.line-size
#[test]
fn the_cursor_covers_both_lines_of_a_double_height_pair() {
    let shapes = [
        (CursorShape::Block, None),
        (CursorShape::Block, Some(rgb("30ff30"))),
        (CursorShape::Underline, None),
        (CursorShape::Bar, None),
    ];
    for (shape, color) in shapes {
        // column 7 is the trailing half of the wide W at column 6, and the
        // cursor there draws over the whole character from its left cell
        for col in [1, 6, 7] {
            let mut normal = screen(20, 1);
            decorated(&mut normal, 0);
            normal.cursor.visible = true;
            (normal.cursor.col, normal.cursor.row) = (col, 0);
            normal.cursor.shape = shape;
            normal.cursor.color = color;
            let normal = whole(&normal);
            for on in [0, 1] {
                let mut s = screen(20, 2);
                decorated(&mut s, 0);
                decorated(&mut s, 1);
                size(&mut s, 0, LineSize::DoubleHeightTop);
                size(&mut s, 1, LineSize::DoubleHeightBottom);
                s.cursor.visible = true;
                (s.cursor.col, s.cursor.row) = (col, on);
                s.cursor.shape = shape;
                s.cursor.color = color;
                let what = format!("{shape:?} {color:?} at column {col} on line {on}");
                assert_pair(&normal, &whole(&s), 20, &what);
            }
        }
    }
}

// A top line with no bottom line after it, and a bottom line with no top
// line before it, draw the cursor on their own line alone: the line next
// to them draws as it does with no cursor.
// covers: text.line-size
#[test]
fn the_cursor_on_a_line_with_no_partner_covers_its_own_line() {
    for (sizes, on, other) in [
        ([LineSize::DoubleHeightTop, LineSize::Single], 0u32, 1u32),
        ([LineSize::DoubleHeightTop, LineSize::DoubleHeightTop], 0, 1),
        ([LineSize::Single, LineSize::DoubleHeightBottom], 1, 0),
        (
            [LineSize::DoubleHeightBottom, LineSize::DoubleHeightBottom],
            1,
            0,
        ),
        ([LineSize::DoubleWidth, LineSize::DoubleWidth], 0, 1),
    ] {
        let make = |cursor: bool| {
            let mut s = screen(20, 2);
            decorated(&mut s, 0);
            decorated(&mut s, 1);
            size(&mut s, 0, sizes[0]);
            size(&mut s, 1, sizes[1]);
            s.cursor.visible = cursor;
            (s.cursor.col, s.cursor.row) = (1, on as usize);
            s
        };
        let (with, without) = (whole(&make(true)), whole(&make(false)));
        assert_row(
            &with,
            20,
            other,
            |x, y| at(&without, x, other * CH + y),
            &format!("{sizes:?}, the line with no cursor"),
        );
        let line = on * CH;
        let differs =
            (0..CH).any(|y| (0..40).any(|x| at(&with, x, line + y) != at(&without, x, line + y)));
        assert!(differs, "{sizes:?}: the cursor's own line shows it");
    }
}

// As the cursor moves onto a pair, between its lines, and off it, and as
// it blinks, each frame drawn into one buffer matches a whole draw: the
// other line of the pair draws the cursor and loses it again with the
// cursor's own line.
// covers: text.line-size
#[test]
fn each_frame_of_the_cursor_on_a_pair_matches_a_whole_draw() {
    let mut s = screen(10, 4);
    for row in 0..4 {
        decorated(&mut s, row);
    }
    size(&mut s, 1, LineSize::DoubleHeightTop);
    size(&mut s, 2, LineSize::DoubleHeightBottom);
    s.cursor.visible = true;
    let (w, h) = display(&s);
    let mut r = Renderer::new(Font::new(Dots { reaches: false }), w, h, Config::default());
    let mut buffer = Canvas::new(w, h, 0);
    let mut since = Age::NEVER;
    let moves: [(&str, usize, usize, bool); 6] = [
        ("above the pair", 1, 0, true),
        ("on the top line", 1, 1, true),
        ("on the bottom line", 2, 2, true),
        ("blinked off", 2, 2, false),
        ("blinked on", 2, 2, true),
        ("below the pair", 2, 3, true),
    ];
    for (what, col, row, on) in moves {
        let old = (s.cursor.col, s.cursor.row);
        s.damage.mark_cell(old.1, old.0);
        (s.cursor.col, s.cursor.row) = (col, row);
        s.damage.mark_cell(row, col);
        let st = DrawState {
            cursor_on: on,
            ..state(&s)
        };
        // the one buffer holds the frame before, so a frame after the
        // first draws only what changed
        let age = u32::from(since != Age::NEVER);
        r.draw(&s.frame(since), &mut buffer, age, &st);
        since = s.damage.checkpoint();
        let mut want = Canvas::new(w, h, 0);
        let mut fresh = Renderer::new(Font::new(Dots { reaches: false }), w, h, Config::default());
        fresh.draw(&s.frame(Age::NEVER), &mut want, 0, &st);
        assert!(
            buffer == want,
            "{what}: the buffer differs from a whole draw"
        );
    }
}

// A block cursor on a wide character of a pair's top line covers the
// columns of that character on the bottom line too, where two narrow
// letters stand under it: both letters draw the cursor's block.
// covers: text.line-size
#[test]
fn the_cursor_covers_the_wide_characters_columns_on_the_other_line() {
    let make = |cursor: bool| {
        let mut s = screen(8, 2);
        s.put(0, 0, 'W', true, plain());
        s.text(0, 1, "ab", plain());
        size(&mut s, 0, LineSize::DoubleHeightTop);
        size(&mut s, 1, LineSize::DoubleHeightBottom);
        s.cursor.visible = cursor;
        (s.cursor.col, s.cursor.row) = (1, 0);
        s
    };
    let (with, without) = (whole(&make(true)), whole(&make(false)));
    for (letter, cell) in [('a', 0), ('b', 1)] {
        // the letter's two display cells on the bottom line
        let x0 = cell * 2 * CW;
        let differs = (0..CH)
            .all(|y| (x0..x0 + 2 * CW).any(|x| at(&with, x, CH + y) != at(&without, x, CH + y)));
        assert!(differs, "{letter} on the bottom line draws the block");
    }
    let rest =
        (0..CH).all(|y| (4 * CW..8 * CW).all(|x| at(&with, x, CH + y) == at(&without, x, CH + y)));
    assert!(rest, "the cells past the character draw no cursor");
}
