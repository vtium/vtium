//! Text blink: in the off phase a blinking cell, SGR 5 or 6, draws as
//! `blink-text` and `blink-decorations` say, except under the cursor, which
//! shows it. By default it draws as a blank cell in its own colours, its
//! underline and strike kept. Every expected colour is a literal.

mod common;

use std::collections::BTreeSet;

use common::{Screen, rgb, style, unifont};
use vtium_render::{BlinkStyle, Canvas, Config, DrawState, Rect, Renderer, TextBlink};
use vtium_term::{Age, Attrs, Color, CursorShape, Style, UnderlineStyle};

const FG: &str = "e0e0e0";
const BG: &str = "203040";
const FG_PX: u32 = 0xe0_e0_e0;
const BG_PX: u32 = 0x20_30_40;
/// SGR 2's mix of FG and BG: e0/2 + 20/2, e0/2 + 30/2, e0/2 + 40/2.
const DIM: u32 = 0x80_88_90;
/// DIM mixed with BG again: 80/2 + 20/2, 88/2 + 30/2, 90/2 + 40/2.
const DIM2: u32 = 0x50_5c_68;
const GREEN: &str = "00ff00";
const GREEN_PX: u32 = 0x00_ff_00;
/// GREEN mixed with BG: 00/2 + 20/2, ff/2 + 30/2, 00/2 + 40/2.
const GREEN_DIM: u32 = 0x10_97_20;

fn draw(s: &Screen, cursor_on: bool, blink_hidden: bool) -> Canvas {
    draw_with(TextBlink::default(), s, cursor_on, blink_hidden)
}

fn draw_with(blink: TextBlink, s: &Screen, cursor_on: bool, blink_hidden: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let config = Config {
        blink,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        blink_hidden,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// Six cells from `cells`, `(col, char, wide, style)`, with a block cursor
/// at column 1 when `cursor`.
fn screen(cells: &[(usize, char, bool, Style)], cursor: bool) -> Screen {
    let mut s = Screen::new(6, 1, rgb("c0c0c0"), rgb("101010"));
    for &(col, c, wide, st) in cells {
        s.put(col, 0, c, wide, st);
    }
    s.cursor.visible = cursor;
    s.cursor.col = 1;
    s.cursor.shape = CursorShape::Block;
    s
}

// covers: sgr.blink
#[test]
fn blinking_text_hides_as_blank_cells_but_under_the_cursor() {
    let plain = style(rgb(FG), rgb(BG));
    let mut blink = plain;
    blink.attrs.insert(Attrs::BLINK);
    let mut rapid = plain;
    rapid.attrs.insert(Attrs::RAPID_BLINK);
    let swapped = style(rgb(BG), rgb(FG));
    let s = screen(
        &[
            (0, 'a', false, blink),
            (1, 'b', false, blink),
            (2, '中', true, rapid),
            (4, 'c', false, plain),
        ],
        true,
    );

    // shown: the text as it is, the cursor's cell swapped
    let shown = screen(
        &[
            (0, 'a', false, plain),
            (1, 'b', false, swapped),
            (2, '中', true, plain),
            (4, 'c', false, plain),
        ],
        false,
    );
    assert!(draw(&s, true, false) == draw(&shown, false, false), "shown");

    // hidden: blank cells in their own colours, but the cursor's
    let blank = screen(
        &[
            (0, ' ', false, plain),
            (1, 'b', false, swapped),
            (2, ' ', false, plain),
            (3, ' ', false, plain),
            (4, 'c', false, plain),
        ],
        false,
    );
    let hidden = draw(&s, true, true);
    assert!(hidden == draw(&blank, false, false), "hidden");
    let bg = 0x20_30_40;
    for col in [0, 2, 3] {
        for y in 0..16 {
            for x in col * 8..col * 8 + 8 {
                assert_eq!(hidden.pixel(x, y), Some(bg), "column {col} at {x},{y}");
            }
        }
    }
    let lit = |c: &Canvas, col: u32| {
        (0..16)
            .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) == Some(bg))
            .count()
    };
    assert!(lit(&hidden, 1) > 10, "the cursor's cell shows its b");

    // with the cursor off, its cell hides too
    let mut off = blank;
    off.put(1, 0, ' ', false, plain);
    assert!(
        draw(&s, false, true) == draw(&off, false, false),
        "cursor off"
    );
}

fn blink(st: Style) -> Style {
    let mut st = st;
    st.attrs.insert(Attrs::BLINK);
    st
}

/// The cell at `col`'s pixels, a row at a time.
fn cell(c: &Canvas, col: u32) -> Vec<u32> {
    (0..16)
        .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

/// The cell at `col` holds exactly the colours `want`.
fn holds(c: &Canvas, col: u32, want: &[u32], what: &str) {
    let got: BTreeSet<u32> = cell(c, col).into_iter().collect();
    let want: BTreeSet<u32> = want.iter().copied().collect();
    assert_eq!(got, want, "{what}, column {col}: {got:x?}");
}

/// Which of the cell at `col`'s pixels are in `colour`.
fn lit(c: &Canvas, col: u32, colour: u32) -> Vec<bool> {
    cell(c, col).into_iter().map(|p| p == colour).collect()
}

/// Five cells: a blinking x; one underlined and struck; one underlined in
/// SGR 58's green; one dim; and a steady x.
fn options_row() -> Screen {
    let plain = style(rgb(FG), rgb(BG));
    let mut deco = blink(plain);
    deco.attrs.set_underline(UnderlineStyle::Single);
    deco.attrs.insert(Attrs::STRIKE);
    let mut green = blink(plain);
    green.attrs.set_underline(UnderlineStyle::Single);
    green.underline_color = Color::Rgb(rgb(GREEN));
    let mut dim = blink(plain);
    dim.attrs.insert(Attrs::DIM);
    let mut s = Screen::new(5, 1, rgb("c0c0c0"), rgb("101010"));
    s.put(0, 0, 'x', false, blink(plain));
    s.put(1, 0, 'x', false, deco);
    s.put(2, 0, 'x', false, green);
    s.put(3, 0, 'x', false, dim);
    s.put(4, 0, 'x', false, plain);
    s
}

const OFF: TextBlink = TextBlink {
    style: BlinkStyle::Off,
    decorations: false,
};
const FULL: TextBlink = TextBlink {
    style: BlinkStyle::Full,
    decorations: false,
};
const PARTIAL: TextBlink = TextBlink {
    style: BlinkStyle::Partial,
    decorations: false,
};

fn with_decorations(b: TextBlink) -> TextBlink {
    TextBlink {
        decorations: true,
        ..b
    }
}

// The on phase draws the text as it is, whatever the options.
// covers: sgr.blink.style, sgr.blink.decorations
#[test]
fn the_on_phase_draws_every_option_alike() {
    let s = options_row();
    let want = draw_with(FULL, &s, false, false);
    holds(&want, 0, &[BG_PX, FG_PX], "on");
    holds(&want, 1, &[BG_PX, FG_PX], "on, underlined and struck");
    holds(&want, 2, &[BG_PX, FG_PX, GREEN_PX], "on, SGR 58");
    holds(&want, 3, &[BG_PX, DIM], "on, dim");
    for b in [OFF, FULL, PARTIAL] {
        for b in [b, with_decorations(b)] {
            assert!(draw_with(b, &s, false, false) == want, "{b:?}");
        }
    }
}

// blink-text=off: blinking text draws steady in both phases.
// covers: sgr.blink.style
#[test]
fn blink_text_off_draws_blinking_text_steady() {
    let s = options_row();
    let on = draw_with(OFF, &s, false, false);
    assert!(draw_with(OFF, &s, false, true) == on, "decorations steady");
    let off = with_decorations(OFF);
    assert!(
        draw_with(off, &s, false, true) == on,
        "decorations blinking"
    );
}

// blink-text=full: the off phase hides the glyph. With the decorations
// steady, the C reference's way, the underline and strike stay; with them
// blinking, xterm's way, the cell shows only its background.
// covers: sgr.blink.style, sgr.blink.decorations
#[test]
fn blink_text_full_hides_the_glyph_and_the_decorations_as_asked() {
    let s = options_row();
    let plain = style(rgb(FG), rgb(BG));

    let steady = draw_with(FULL, &s, false, true);
    holds(&steady, 0, &[BG_PX], "full");
    holds(&steady, 1, &[BG_PX, FG_PX], "full, decorations kept");
    holds(&steady, 2, &[BG_PX, GREEN_PX], "full, SGR 58 kept");
    holds(&steady, 3, &[BG_PX], "full, dim");
    holds(&steady, 4, &[BG_PX, FG_PX], "steady text");
    let mut blank = Screen::new(5, 1, rgb("c0c0c0"), rgb("101010"));
    let mut deco = plain;
    deco.attrs.set_underline(UnderlineStyle::Single);
    deco.attrs.insert(Attrs::STRIKE);
    let mut green = plain;
    green.attrs.set_underline(UnderlineStyle::Single);
    green.underline_color = Color::Rgb(rgb(GREEN));
    blank.put(0, 0, ' ', false, plain);
    blank.put(1, 0, ' ', false, deco);
    blank.put(2, 0, ' ', false, green);
    blank.put(3, 0, ' ', false, plain);
    blank.put(4, 0, 'x', false, plain);
    assert!(
        steady == draw(&blank, false, false),
        "blank cells, decorated"
    );

    let blinking = draw_with(with_decorations(FULL), &s, false, true);
    for col in 0..4 {
        holds(&blinking, col, &[BG_PX], "full, decorations blinking");
    }
    holds(&blinking, 4, &[BG_PX, FG_PX], "steady text");
}

// blink-text=partial: the off phase draws the glyph halfway to its
// background, as SGR 2 does, over dim text too. With the decorations
// steady they keep their colours; with them blinking they dim with it,
// SGR 58's colour included.
// covers: sgr.blink.style, sgr.blink.decorations
#[test]
fn blink_text_partial_dims_the_glyph_and_the_decorations_as_asked() {
    let s = options_row();
    let on = draw_with(PARTIAL, &s, false, false);
    let full = draw_with(FULL, &s, false, true);

    let steady = draw_with(PARTIAL, &s, false, true);
    holds(&steady, 0, &[BG_PX, DIM], "partial");
    holds(
        &steady,
        1,
        &[BG_PX, DIM, FG_PX],
        "partial, decorations kept",
    );
    holds(&steady, 2, &[BG_PX, DIM, GREEN_PX], "partial, SGR 58 kept");
    holds(&steady, 3, &[BG_PX, DIM2], "partial, dim");
    holds(&steady, 4, &[BG_PX, FG_PX], "steady text");
    // the same glyph, and the decorations where full blink keeps them
    for col in 0..3 {
        let fg = if col == 2 { GREEN_PX } else { FG_PX };
        let was: Vec<bool> = lit(&on, col, FG_PX)
            .iter()
            .zip(lit(&on, col, fg))
            .map(|(a, b)| *a || b)
            .collect();
        let now: Vec<bool> = lit(&steady, col, DIM)
            .iter()
            .zip(lit(&steady, col, fg))
            .map(|(a, b)| *a || b)
            .collect();
        assert_eq!(now, was, "column {col}: the cell's shape");
        assert_eq!(lit(&steady, col, fg), lit(&full, col, fg), "column {col}");
    }

    let blinking = draw_with(with_decorations(PARTIAL), &s, false, true);
    holds(&blinking, 0, &[BG_PX, DIM], "partial, decorations blinking");
    holds(&blinking, 1, &[BG_PX, DIM], "partial, decorations dimmed");
    holds(
        &blinking,
        2,
        &[BG_PX, DIM, GREEN_DIM],
        "partial, SGR 58 dimmed",
    );
    holds(&blinking, 3, &[BG_PX, DIM2], "partial, dim");
    holds(&blinking, 4, &[BG_PX, FG_PX], "steady text");
    // SGR 2 draws the same cells, given the colours it leaves exact
    let plain = style(rgb(FG), rgb(BG));
    let dim = |mut st: Style| {
        st.attrs.insert(Attrs::DIM);
        st
    };
    let mut deco = dim(plain);
    deco.attrs.set_underline(UnderlineStyle::Single);
    deco.attrs.insert(Attrs::STRIKE);
    let mut green = dim(plain);
    green.attrs.set_underline(UnderlineStyle::Single);
    green.underline_color = Color::Rgb(vtium_term::Rgb::new(0x10, 0x97, 0x20));
    let mut sgr2 = Screen::new(5, 1, rgb("c0c0c0"), rgb("101010"));
    sgr2.put(0, 0, 'x', false, dim(plain));
    sgr2.put(1, 0, 'x', false, deco);
    sgr2.put(2, 0, 'x', false, green);
    sgr2.put(3, 0, 'x', false, dim(style(rgb("808890"), rgb(BG))));
    sgr2.put(4, 0, 'x', false, plain);
    assert!(blinking == draw(&sgr2, false, false), "as SGR 2 draws");
}

// Under the cursor a blinking cell shows in the off phase, whatever the
// options.
// covers: sgr.blink.style, sgr.blink.decorations
#[test]
fn the_cursor_shows_a_blinking_cell_under_every_option() {
    let mut s = options_row();
    s.cursor.visible = true;
    s.cursor.shape = CursorShape::Block;
    for col in 0..4 {
        s.cursor.col = col;
        let on = draw_with(FULL, &s, true, false);
        for b in [OFF, FULL, PARTIAL] {
            for b in [b, with_decorations(b)] {
                let off = draw_with(b, &s, true, true);
                assert_eq!(
                    cell(&off, col as u32),
                    cell(&on, col as u32),
                    "{b:?} at {col}"
                );
            }
        }
    }
}

/// The damage of drawing `s` in the off phase after the on phase, into the
/// same buffer.
fn phase_damage(b: TextBlink, s: &mut Screen) -> Vec<Rect> {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let config = Config {
        blink: b,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let mut st = DrawState {
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    let since = s.damage.checkpoint();
    st.blink_hidden = true;
    r.draw(&s.frame(since), &mut c, 1, &st).damage.to_vec()
}

// With blink-text off a phase change draws nothing; with it on, the
// blinking cells redraw.
// covers: sgr.blink.style
#[test]
fn blink_text_off_redraws_nothing_at_a_phase_change() {
    let mut s = options_row();
    assert_eq!(phase_damage(OFF, &mut s), []);
    assert_eq!(phase_damage(with_decorations(OFF), &mut s), []);
    let blinking = Rect {
        x: 0,
        y: 0,
        width: 32,
        height: 16,
    };
    assert_eq!(phase_damage(FULL, &mut s), [blinking]);
    assert_eq!(phase_damage(PARTIAL, &mut s), [blinking]);
}
