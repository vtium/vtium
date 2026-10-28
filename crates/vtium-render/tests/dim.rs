//! SGR 2, dim: the foreground draws halfway to the background, each
//! channel the sum of the two halved, as the C reference drew it (libtsm's
//! `color_dim`). Dim follows reverse video and comes before the cursor, and
//! the decorations that follow the foreground follow it dimmed. Every
//! expected color is a literal.

mod common;

use std::collections::BTreeSet;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs, Color, CursorShape, Rgb, Style, UnderlineStyle};

const FG: &str = "ff8040";
const BG: &str = "21a0c3";
/// ff/2 + 21/2, 80/2 + a0/2, 40/2 + c3/2: 7f + 10, 40 + 50, 20 + 61. A
/// rounded mean, (ff + 21) / 2, would give 90 in red.
const DIM: u32 = 0x8f_90_81;
const FG_PX: u32 = 0xff_80_40;
const BG_PX: u32 = 0x21_a0_c3;

fn draw(s: &Screen, cursor_on: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// The colors of the cell at `col` and how many of its pixels each takes.
fn colors(c: &Canvas, col: u32) -> Vec<(u32, usize)> {
    let px: Vec<u32> = (0..16)
        .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect();
    let set: BTreeSet<u32> = px.iter().copied().collect();
    set.into_iter()
        .map(|p| (p, px.iter().filter(|&&q| q == p).count()))
        .collect()
}

/// The cell at `col` holds exactly `want`, each on at least one pixel.
fn holds(c: &Canvas, col: u32, want: &[u32], what: &str) {
    let got: Vec<u32> = colors(c, col).into_iter().map(|(p, _)| p).collect();
    let mut want = want.to_vec();
    want.sort_unstable();
    assert_eq!(got, want, "{what}: {:x?}", colors(c, col));
}

/// The color most of the cell at `col` takes: its background.
fn ground(c: &Canvas, col: u32) -> u32 {
    colors(c, col).into_iter().max_by_key(|p| p.1).unwrap().0
}

fn dim(mut st: Style) -> Style {
    st.attrs.insert(Attrs::DIM);
    st
}

/// One row of `n` cells, the cursor hidden.
fn row(n: usize) -> Screen {
    Screen::new(n, 1, rgb("c0c0c0"), rgb("101010"))
}

// covers: sgr.dim
#[test]
fn dim_text_draws_halfway_to_its_background() {
    let plain = style(rgb(FG), rgb(BG));
    let mut s = row(5);
    s.put(0, 0, 'x', false, plain);
    s.put(1, 0, 'x', false, dim(plain));
    // the underline and the strike take the foreground as drawn
    let mut deco = dim(plain);
    deco.attrs.set_underline(UnderlineStyle::Single);
    deco.attrs.insert(Attrs::STRIKE);
    s.put(2, 0, 'x', false, deco);
    // a color SGR 58 set is exact
    let mut set = deco;
    set.underline_color = Color::Rgb(rgb("00ff00"));
    s.put(3, 0, 'x', false, set);
    // a blank cell shows only its background
    s.put(4, 0, ' ', false, dim(plain));
    let c = draw(&s, false);
    holds(&c, 0, &[BG_PX, FG_PX], "plain");
    holds(&c, 1, &[BG_PX, DIM], "dim");
    holds(&c, 2, &[BG_PX, DIM], "dim, underlined and struck");
    holds(&c, 3, &[BG_PX, DIM, 0x00_ff_00], "dim, SGR 58's underline");
    holds(&c, 4, &[BG_PX], "dim blank");
    // the underline row is lit in the dim color across the cell
    let ul = (0..16)
        .find(|&y| (16..24).all(|x| c.pixel(x, y) == Some(DIM)))
        .expect("an underline row in the dim color");
    assert!((16..24).all(|x| c.pixel(x, ul) == Some(DIM)));
}

// Bold keeps its glyph, and the glyph takes the dim color.
// covers: sgr.dim
#[test]
fn bold_and_dim_draw_the_bold_glyph_dimmed() {
    let mut bold = style(rgb(FG), rgb(BG));
    bold.attrs.insert(Attrs::BOLD);
    let mut s = row(2);
    s.put(0, 0, 'x', false, bold);
    s.put(1, 0, 'x', false, dim(bold));
    let c = draw(&s, false);
    holds(&c, 0, &[BG_PX, FG_PX], "bold");
    holds(&c, 1, &[BG_PX, DIM], "bold and dim");
    let lit = |col: u32, color: u32| -> Vec<(u32, u32)> {
        (0..16)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixel(col * 8 + x, y) == Some(color))
            .collect()
    };
    assert_eq!(lit(0, FG_PX), lit(1, DIM), "the same bold glyph");
}

// Dim mixes the colors as reverse video swapped them: the text draws in
// the mix over the old foreground.
// covers: sgr.dim
#[test]
fn reverse_video_dims_the_swapped_foreground() {
    let mut inverse = style(rgb(FG), rgb(BG));
    inverse.attrs.insert(Attrs::INVERSE);
    let mut s = row(2);
    s.put(0, 0, 'x', false, inverse);
    s.put(1, 0, 'x', false, dim(inverse));
    let c = draw(&s, false);
    holds(&c, 0, &[FG_PX, BG_PX], "inverse");
    assert_eq!(ground(&c, 0), FG_PX, "inverse: the old foreground behind");
    holds(&c, 1, &[FG_PX, DIM], "inverse and dim");
    assert_eq!(
        ground(&c, 1),
        FG_PX,
        "inverse and dim: the old foreground behind"
    );
}

/// A dim 'x' at column 0, the cursor on it in `shape` with `color`.
fn under_cursor(shape: CursorShape, color: Option<Rgb>) -> Canvas {
    let mut s = row(1);
    s.put(0, 0, 'x', false, dim(style(rgb(FG), rgb(BG))));
    s.cursor.visible = true;
    s.cursor.shape = shape;
    s.cursor.color = color;
    draw(&s, true)
}

// The cursor draws over the cell as dim left it: a block swaps the dimmed
// foreground with the background; OSC 12's color replaces the block's
// background; an underline and a bar take the dimmed foreground or OSC
// 12's color.
// covers: sgr.dim
#[test]
fn the_cursor_draws_over_the_dimmed_cell() {
    let block = under_cursor(CursorShape::Block, None);
    holds(&block, 0, &[DIM, BG_PX], "block");
    assert_eq!(ground(&block, 0), DIM, "block: the mix behind");

    let colored = under_cursor(CursorShape::Block, Some(rgb("00ff00")));
    holds(&colored, 0, &[0x00_ff_00, BG_PX], "colored block");
    assert_eq!(
        ground(&colored, 0),
        0x00_ff_00,
        "colored block: its color behind"
    );

    let underline = under_cursor(CursorShape::Underline, None);
    holds(&underline, 0, &[BG_PX, DIM], "underline");
    assert!(
        (0..16).any(|y| (0..8).all(|x| underline.pixel(x, y) == Some(DIM))),
        "underline: a full row in the mix"
    );
    let underline = under_cursor(CursorShape::Underline, Some(rgb("00ff00")));
    holds(
        &underline,
        0,
        &[BG_PX, DIM, 0x00_ff_00],
        "colored underline",
    );

    holds(
        &under_cursor(CursorShape::Bar, None),
        0,
        &[BG_PX, DIM],
        "bar",
    );
    holds(
        &under_cursor(CursorShape::Bar, Some(rgb("00ff00"))),
        0,
        &[BG_PX, 0x00_ff_00],
        "colored bar",
    );
}
