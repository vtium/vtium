//! The cursor's shapes, as the C reference drew them (libtsm's
//! `frame_cursor_cell`, kmscon's `bbulk_draw`): a block swaps the cell's
//! colours, an underline toggles a single underline in the foreground, and
//! a bar draws U+258F in place of the character. Each case is checked
//! against the same screen without a cursor, styled to look as the cursor
//! should.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, CursorShape, Rgb, Style, UnderlineStyle};

const FG: &str = "e0e0e0";
const BG: &str = "203040";

fn draw(s: &Screen, cursor_on: bool, pointer: Option<(u32, u32)>) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        pointer,
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// "ab中c" in `st`, the cursor at `col` in `shape` with `color`.
fn screen(st: Style, col: usize, shape: CursorShape, color: Option<Rgb>) -> Screen {
    let mut s = Screen::new(6, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "ab", st);
    s.put(2, 0, '中', true, st);
    s.put(4, 0, 'c', false, st);
    s.cursor.visible = true;
    s.cursor.col = col;
    s.cursor.shape = shape;
    s.cursor.color = color;
    s
}

fn swapped(st: Style) -> Style {
    Style {
        fg: st.bg,
        bg: st.fg,
        ..st
    }
}

#[test]
fn a_block_swaps_the_cells_colours() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 1, CursorShape::Block, None);
    let mut want = screen(st, 1, CursorShape::Block, None);
    want.put(1, 0, 'b', false, swapped(st));
    assert!(draw(&s, true, None) == draw(&want, false, None));
    // a hidden or blinked-off cursor draws the cell as it is
    assert!(draw(&s, false, None) == draw(&screen(st, 1, CursorShape::Block, None), false, None));
    let mut hidden = screen(st, 1, CursorShape::Block, None);
    hidden.cursor.visible = false;
    assert!(draw(&hidden, true, None) == draw(&s, false, None));
}

// On the trailing half of a wide character the cursor covers the whole
// character, from its leading cell.
#[test]
fn a_block_on_a_trailing_half_covers_the_wide_character() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 3, CursorShape::Block, None);
    let mut want = screen(st, 3, CursorShape::Block, None);
    want.put(2, 0, '中', true, swapped(st));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// OSC 12 sets the block's colour; the character keeps the cell's
// background as its colour.
#[test]
fn a_coloured_block_takes_its_colour() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Block, Some(rgb("ff8000")));
    let mut want = screen(st, 0, CursorShape::Block, None);
    want.put(0, 0, 'a', false, style(rgb(BG), rgb("ff8000")));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

#[test]
fn an_underline_cursor_toggles_a_single_underline() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 4, CursorShape::Underline, None);
    let mut want = screen(st, 4, CursorShape::Underline, None);
    let mut ul = st;
    ul.attrs.set_underline(UnderlineStyle::Single);
    want.put(4, 0, 'c', false, ul);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // on underlined text it takes the underline away
    let mut curly = st;
    curly.attrs.set_underline(UnderlineStyle::Curly);
    let s = screen(curly, 4, CursorShape::Underline, None);
    let mut want = screen(curly, 4, CursorShape::Underline, None);
    want.put(4, 0, 'c', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

#[test]
fn a_bar_replaces_the_character_with_a_left_eighth_block() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Bar, None);
    let mut want = screen(st, 0, CursorShape::Bar, None);
    want.put(0, 0, '▏', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // on a wide character: the bar in the first cell, the second empty
    let s = screen(st, 2, CursorShape::Bar, None);
    let mut want = screen(st, 2, CursorShape::Bar, None);
    want.put(2, 0, '▏', false, st);
    want.put(3, 0, ' ', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // in OSC 12's colour
    let s = screen(st, 0, CursorShape::Bar, Some(rgb("00ff00")));
    let mut want = screen(st, 0, CursorShape::Bar, None);
    want.put(0, 0, '▏', false, style(rgb("00ff00"), rgb(BG)));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// bbulk_draw_pointer: the font's I in the default colours, in a cell's box
// centred on the pointer, half a cell rounded up each way, and kept
// inside the grid
#[test]
fn the_pointer_is_an_i_beam_in_the_default_colours() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Block, None);
    let without = draw(&s, false, None);
    let mut beam = Screen::new(6, 1, rgb("c0c0c0"), rgb("101010"));
    beam.put(0, 0, 'I', false, style(rgb("c0c0c0"), rgb("101010")));
    let beam = draw(&beam, false, None);
    // (20, 9) on a grid one row high: y is kept at half a cell, so the box
    // is at (16, 0); far past the grid it is kept in the last cell
    for (pointer, bx) in [((20, 9), 16), ((10_000, 10_000), 40)] {
        let with = draw(&s, false, Some(pointer));
        for y in 0..16 {
            for x in 0..48 {
                let want = if (bx..bx + 8).contains(&x) {
                    beam.pixel(x - bx, y).unwrap()
                } else {
                    without.pixel(x, y).unwrap()
                };
                assert_eq!(with.pixel(x, y).unwrap(), want, "{pointer:?} at {x},{y}");
            }
        }
    }
}
