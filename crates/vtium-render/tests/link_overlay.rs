//! The overlay row (ADR-1002, "Drawing"): while Ctrl is held over a link,
//! the frame names a row and the link's address, and the renderer draws
//! the address from the row's first column in the default colors inverted,
//! cut at the row's end with `…` when it is longer. The overlay covers only
//! the cells the address takes. The rest of the row draws as the program
//! wrote it, and the text cursor and the selection are not drawn under the
//! overlay. Each drawing is compared with the same screen written with the
//! address's characters in the inverted default style and no overlay.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, Depth, DrawState, Renderer};
use vtium_term::{Age, Attrs, Cursor, CursorShape, Style};

const COLS: usize = 12;
const ROWS: usize = 3;

/// The style a cell takes in the default colors inverted.
fn inverted() -> Style {
    let mut st = Style::DEFAULT;
    st.attrs.insert(Attrs::INVERSE);
    st
}

/// Three rows of text in a style of their own.
fn screen() -> Screen {
    let mut s = Screen::new(COLS, ROWS, rgb("c0c0c0"), rgb("101010"));
    let text = style(rgb("ff8040"), rgb("21a0c3"));
    for row in 0..ROWS {
        s.text(0, row, "programtext!", text);
    }
    s
}

/// Draws `frame` of `s` on a fresh renderer at `depth`.
fn draw(s: &Screen, frame: &vtium_term::Frame<'_>, depth: Depth) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let config = Config {
        depth,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(frame, &mut c, 0, &st);
    c
}

/// Fails with `what` and the count of pixels that differ, unless `a` and
/// `b` are the same pixels.
fn same(a: &[u32], b: &[u32], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: the sizes differ");
    let differ = a.iter().zip(b).filter(|(x, y)| x != y).count();
    assert_eq!(differ, 0, "{what}: {differ} pixels differ");
}

// covers: osc.8
#[test]
fn the_overlay_draws_the_address_inverted_from_the_rows_first_column() {
    for depth in [Depth::Color, Depth::One] {
        for row in [0, ROWS - 1] {
            let s = screen();
            let drawn = draw(
                &s,
                &s.frame(Age::NEVER).with_hover(None, Some((row, "a:b/c"))),
                depth,
            );
            let mut want = screen();
            want.text(0, row, "a:b/c", inverted());
            let wanted = draw(&want, &want.frame(Age::NEVER), depth);
            same(
                drawn.pixels(),
                wanted.pixels(),
                &format!("{depth:?}, row {row}"),
            );
        }
    }
}

// covers: osc.8
#[test]
fn an_address_longer_than_the_row_is_cut_with_an_ellipsis() {
    let uri = "https://example.com/a-long-path";
    for depth in [Depth::Color, Depth::One] {
        let s = screen();
        let drawn = draw(
            &s,
            &s.frame(Age::NEVER).with_hover(None, Some((ROWS - 1, uri))),
            depth,
        );
        let mut want = screen();
        want.text(0, ROWS - 1, "https://exa\u{2026}", inverted());
        let wanted = draw(&want, &want.frame(Age::NEVER), depth);
        same(drawn.pixels(), wanted.pixels(), &format!("{depth:?}"));
    }
}

// The address exactly as wide as the row shows whole, with no `…`.
#[test]
fn an_address_as_wide_as_the_row_shows_whole() {
    let uri = "x://12345678";
    assert_eq!(uri.len(), COLS);
    let s = screen();
    let drawn = draw(
        &s,
        &s.frame(Age::NEVER).with_hover(None, Some((0, uri))),
        Depth::Color,
    );
    let mut want = screen();
    want.text(0, 0, uri, inverted());
    let wanted = draw(&want, &want.frame(Age::NEVER), Depth::Color);
    same(drawn.pixels(), wanted.pixels(), "the row");
}

// The cursor and the selection draw past the overlay, and not under it.
// covers: osc.8
#[test]
fn the_cursor_and_the_selection_are_not_drawn_under_the_overlay() {
    let row = ROWS - 1;
    for col in [1, 7] {
        let mut s = screen();
        s.cursor = Cursor {
            row,
            col,
            visible: true,
            shape: CursorShape::Block,
            ..Cursor::default()
        };
        let selection = vec![0..0, 0..0, 0..COLS];
        let frame = s
            .frame(Age::NEVER)
            .with_selection(selection)
            .with_hover(None, Some((row, "a:b/c")));
        let drawn = draw(&s, &frame, Depth::Color);
        let mut want = screen();
        want.text(0, row, "a:b/c", inverted());
        want.cursor = s.cursor;
        want.cursor.visible = col >= 5;
        let frame = want
            .frame(Age::NEVER)
            .with_selection(vec![0..0, 0..0, 5..COLS]);
        let wanted = draw(&want, &frame, Depth::Color);
        same(
            drawn.pixels(),
            wanted.pixels(),
            &format!("the cursor at {col}"),
        );
    }
}

// A wide character whose first half the overlay covers draws its second
// half as a blank cell in its own colors, so no cell keeps old pixels.
#[test]
fn a_wide_character_the_overlay_cuts_leaves_a_blank_cell() {
    let text = style(rgb("ff8040"), rgb("21a0c3"));
    let mut s = screen();
    s.put(4, 0, '\u{4e2d}', true, text);
    let drawn = draw(
        &s,
        &s.frame(Age::NEVER).with_hover(None, Some((0, "a:b/c"))),
        Depth::Color,
    );
    let mut want = screen();
    want.text(0, 0, "a:b/c", inverted());
    want.put(5, 0, ' ', false, text);
    let wanted = draw(&want, &want.frame(Age::NEVER), Depth::Color);
    same(drawn.pixels(), wanted.pixels(), "the row");
}

// A frame with no overlay, its row marked changed as the screen marks it
// when Ctrl is released, draws the row as the program wrote it.
// covers: osc.8
#[test]
fn the_overlay_goes_with_its_frame() {
    let mut s = screen();
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let since = s.damage.checkpoint();
    r.draw(
        &s.frame(Age::NEVER).with_hover(None, Some((0, "a:b/c"))),
        &mut c,
        0,
        &st,
    );
    let shown = c.pixels().to_vec();
    s.damage.mark_rows(0..1);
    r.draw(&s.frame(since), &mut c, 1, &st);
    assert!(c.pixels() != &shown[..], "the overlay was drawn");
    let plain = draw(&s, &s.frame(Age::NEVER), Depth::Color);
    same(c.pixels(), plain.pixels(), "after the overlay");
}
