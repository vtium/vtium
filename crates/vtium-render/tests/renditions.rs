//! SGR's conceal (8), overline (53) and their decorations as drawn. A
//! concealed cell draws its background alone, and an overline draws the
//! cell's top rows, as thick as the underline, over the glyph in the
//! strike's color. Every expected color is a literal.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{
    BlinkStyle, Canvas, Config, DecoColor, DecorationColors, DrawState, Renderer, TextBlink,
};
use vtium_term::{Age, Attrs, CursorShape, Style, UnderlineStyle};

const FG: &str = "e1e1e1";
const BG: &str = "213141";
const FG_PX: u32 = 0xe1_e1_e1;
const BG_PX: u32 = 0x21_31_41;
const GREEN: &str = "00ff00";
const GREEN_PX: u32 = 0x00_ff_00;

fn draw_config(config: Config, s: &Screen, blink_hidden: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on: true,
        blink_hidden,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

fn draw(s: &Screen) -> Canvas {
    draw_config(Config::default(), s, false)
}

/// Four cells from `cells`, `(col, text, style)`, with a cursor of `shape`
/// at column 3 when it is given.
fn screen(cells: &[(usize, &str, Style)], cursor: Option<CursorShape>) -> Screen {
    let mut s = Screen::new(4, 1, rgb("c0c0c0"), rgb("101010"));
    for &(col, text, st) in cells {
        s.text(col, 0, text, st);
    }
    if let Some(shape) = cursor {
        s.cursor.visible = true;
        s.cursor.col = 3;
        s.cursor.shape = shape;
    }
    s
}

fn with(mut st: Style, attrs: &[Attrs]) -> Style {
    for &a in attrs {
        st.attrs.insert(a);
    }
    st
}

/// The colors of cell `col`'s pixels, row by row.
fn cell(c: &Canvas, col: u32) -> Vec<Vec<u32>> {
    (0..16)
        .map(|y| {
            (col * 8..col * 8 + 8)
                .map(|x| c.pixel(x, y).unwrap())
                .collect()
        })
        .collect()
}

// ECMA-48 8.3.117 and ctlseqs: a concealed cell draws as its background,
// its glyph, underline, strike and overline hidden; a block cursor on it
// shows the cursor and no glyph, and SGR 28's revealed text draws.
// covers: sgr.conceal
#[test]
fn a_concealed_cell_draws_its_background_alone() {
    let plain = style(rgb(FG), rgb(BG));
    let mut lined = with(plain, &[Attrs::STRIKE, Attrs::OVERLINE]);
    lined.attrs.set_underline(UnderlineStyle::Curly);
    let hidden = with(lined, &[Attrs::CONCEAL]);
    let s = screen(
        &[
            (0, "W", hidden),
            (1, "g", hidden),
            (2, "W", lined),
            (3, "x", hidden),
        ],
        Some(CursorShape::Block),
    );
    let c = draw(&s);
    for col in [0, 1] {
        for (y, row) in cell(&c, col).iter().enumerate() {
            assert!(
                row.iter().all(|&p| p == BG_PX),
                "cell {col}, row {y}: {row:x?}"
            );
        }
    }
    // revealed, the same cell draws its glyph and its lines
    let revealed = cell(&c, 2);
    assert!(revealed.iter().flatten().any(|&p| p == FG_PX));
    // the block cursor swaps the colors and shows no glyph
    for (y, row) in cell(&c, 3).iter().enumerate() {
        assert!(row.iter().all(|&p| p == FG_PX), "cursor row {y}: {row:x?}");
    }
    // a concealed cell draws what a blank cell in its colors draws
    let blank = screen(
        &[(0, "  ", plain), (3, " ", plain)],
        Some(CursorShape::Block),
    );
    let mut blank = blank;
    blank.text(2, 0, "W", lined);
    assert!(c == draw(&blank), "concealed cells draw as blank ones");
}

// ECMA-48 8.3.117: an overlined cell draws a line along its top, one
// thickness of the underline (a pixel in a 16-pixel cell), the cell's
// width, in the foreground; SGR 55 draws none.
// covers: sgr.overline
#[test]
fn an_overline_draws_the_cells_top_row() {
    let plain = style(rgb(FG), rgb(BG));
    let over = with(plain, &[Attrs::OVERLINE]);
    let s = screen(&[(0, " ", over), (1, " ", plain)], None);
    let c = draw(&s);
    let lined = cell(&c, 0);
    assert_eq!(lined[0], vec![FG_PX; 8], "the top row");
    for (y, row) in lined.iter().enumerate().skip(1) {
        assert_eq!(*row, vec![BG_PX; 8], "row {y}");
    }
    assert!(
        cell(&c, 1).iter().flatten().all(|&p| p == BG_PX),
        "no overline"
    );
}

// The overline draws with the strike, over the glyph, in the strike's
// color: the configured strike-color, else the foreground.
// covers: sgr.overline
#[test]
fn an_overline_and_a_strike_draw_together_in_the_strikes_color() {
    let plain = style(rgb(FG), rgb(BG));
    let both = with(plain, &[Attrs::OVERLINE, Attrs::STRIKE]);
    let strike = with(plain, &[Attrs::STRIKE]);
    let s = screen(&[(0, " ", both), (1, " ", strike)], None);
    let config = Config {
        decorations: DecorationColors {
            strike: DecoColor::Rgb(rgb(GREEN)),
            ..DecorationColors::default()
        },
        ..Config::default()
    };
    let c = draw_config(config, &s, false);
    let lined = cell(&c, 0);
    let struck = cell(&c, 1);
    assert_eq!(lined[0], vec![GREEN_PX; 8], "the overline");
    for y in 1..16 {
        assert_eq!(lined[y], struck[y], "row {y} is the strike's");
    }
    assert_eq!(struck[0], vec![BG_PX; 8]);
    assert!(struck.iter().any(|r| *r == vec![GREEN_PX; 8]), "the strike");
}

// blink-decorations: with `blink-text=full`, an overline of blinking text
// hides with the glyph in the off phase, as the strike does.
// covers: sgr.overline
#[test]
fn an_overline_blinks_with_the_decorations() {
    let plain = style(rgb(FG), rgb(BG));
    let blink = with(plain, &[Attrs::OVERLINE, Attrs::BLINK]);
    let s = screen(&[(0, " ", blink)], None);
    let full = |decorations| Config {
        blink: TextBlink {
            style: BlinkStyle::Full,
            decorations,
        },
        ..Config::default()
    };
    let top = |c: &Canvas| cell(c, 0)[0].clone();
    assert_eq!(top(&draw_config(full(true), &s, false)), vec![FG_PX; 8]);
    assert_eq!(top(&draw_config(full(true), &s, true)), vec![BG_PX; 8]);
    assert_eq!(top(&draw_config(full(false), &s, true)), vec![FG_PX; 8]);
}
