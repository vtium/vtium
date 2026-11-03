//! A kitty Unicode placeholder cell (ADR-1100 D13): a cell whose cluster
//! starts with U+10EEEE draws its background and no glyph, underline or
//! strike-through, since the picture its virtual placement names draws
//! there instead. The cursor and the selection draw over it as over any
//! cell. Each case is checked against a screen that draws what the cell
//! should look like with ordinary characters. Without `images`, U+10EEEE
//! is a private-use character like any other.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
#[cfg(feature = "images")]
use vtium_term::CursorShape;
use vtium_term::{Age, Attrs, Glyph, LinkId, Style, UnderlineStyle, Width};

const FG: &str = "e0a040";
const BG: &str = "203040";
/// The screen's default foreground.
const DEFAULT_FG: &str = "c0c0c0";

/// U+10EEEE with the row and column diacritics for row 0 and column 1:
/// the first and second marks of kitty's `rowcolumn-diacritics.txt`.
const PLACEHOLDER: &str = "\u{10EEEE}\u{305}\u{30D}";

fn draw(s: &Screen, cursor_on: bool) -> Canvas {
    draw_hovered(s, cursor_on, None)
}

/// Draws `s` with the link `hovered` under the pointer.
fn draw_hovered(s: &Screen, cursor_on: bool, hovered: Option<LinkId>) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(
        &s.frame(Age::NEVER).with_hover(hovered, None),
        &mut c,
        0,
        &st,
    );
    c
}

/// A colored foreground with an underline and a strike-through.
fn decorated() -> Style {
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.set_underline(UnderlineStyle::Single);
    st.attrs.insert(Attrs::STRIKE);
    st
}

/// "a", then `glyph` in `st` at column 1, then "b".
fn screen(glyph: Glyph<'_>, st: Style) -> Screen {
    linked(glyph, st, None)
}

/// The same screen, with column 1 in `link`.
fn linked(glyph: Glyph<'_>, st: Style, link: Option<LinkId>) -> Screen {
    let mut s = Screen::new(3, 1, rgb(DEFAULT_FG), rgb("101010"));
    s.text(0, 0, "a", style(rgb(FG), rgb(BG)));
    s.lines[0].write(1, glyph, Width::Narrow, &st, link);
    s.damage.mark_cell(0, 1);
    s.text(2, 0, "b", style(rgb(FG), rgb(BG)));
    s
}

/// The same screen with a blank cell in the background alone at column 1.
fn background_only() -> Screen {
    screen(Glyph::Char(' '), style(rgb(FG), rgb(BG)))
}

// covers: image.kitty-placeholders
#[cfg(feature = "images")]
#[test]
fn a_placeholder_cell_draws_only_its_background() {
    let want = draw(&background_only(), false);
    for glyph in [Glyph::Cluster(PLACEHOLDER), Glyph::Char('\u{10EEEE}')] {
        let s = screen(glyph, decorated());
        assert!(
            draw(&s, false) == want,
            "{glyph:?} draws more than its background"
        );
    }
}

// Only a cluster that starts with U+10EEEE is a placeholder: one that holds
// it after another character is text, drawn with its decorations.
// covers: image.kitty-placeholders
#[cfg(feature = "images")]
#[test]
fn a_cluster_that_holds_the_placeholder_later_draws_as_text() {
    let text = Glyph::Cluster("a\u{10EEEE}");
    let plain = draw(&screen(text, style(rgb(FG), rgb(BG))), false);
    let decorated = draw(&screen(text, decorated()), false);
    assert!(
        plain != draw(&background_only(), false),
        "the glyph is not drawn"
    );
    assert!(
        decorated != plain,
        "the underline and strike-through are not drawn"
    );
}

// covers: image.kitty-placeholders
#[cfg(feature = "images")]
#[test]
fn the_cursor_draws_over_a_placeholder_cell() {
    let at_placeholder = |shape| {
        let mut s = screen(Glyph::Cluster(PLACEHOLDER), decorated());
        s.cursor.visible = true;
        s.cursor.col = 1;
        s.cursor.shape = shape;
        s
    };
    let blank = |st: Style| screen(Glyph::Char(' '), st);
    // The cell's foreground names its image, so vtium-term resolves it as
    // the default foreground (ADR-1100 D13), and the cursor draws as over a
    // blank cell in the default foreground.
    let plain = style(rgb(DEFAULT_FG), rgb(BG));

    // a block swaps the background with the default foreground
    let s = at_placeholder(CursorShape::Block);
    assert!(draw(&s, true) == draw(&blank(style(rgb(BG), rgb(DEFAULT_FG))), false));

    // an underline cursor draws its single line, though the cell's own
    // underline is not drawn
    let mut ul = plain;
    ul.attrs.set_underline(UnderlineStyle::Single);
    let s = at_placeholder(CursorShape::Underline);
    assert!(draw(&s, true) == draw(&blank(ul), false));

    // a bar draws its left eighth block
    let s = at_placeholder(CursorShape::Bar);
    assert!(draw(&s, true) == draw(&screen(Glyph::Char('\u{258F}'), plain), false));
}

// The pointer over a link of placeholder cells draws no dashed underline
// there, as the cell draws no underline of its own.
// covers: image.kitty-placeholders
#[cfg(feature = "images")]
#[test]
fn a_hovered_link_draws_no_dash_on_a_placeholder_cell() {
    let link = LinkId::new(7);
    let s = linked(Glyph::Cluster(PLACEHOLDER), decorated(), Some(link));
    let hovered = draw_hovered(&s, false, Some(link));
    assert!(hovered == draw(&background_only(), false));
}

// Without `images`, U+10EEEE draws as another private-use character does,
// with its decorations.
#[cfg(not(feature = "images"))]
#[test]
fn without_images_the_placeholder_is_text() {
    let other = draw(&screen(Glyph::Char('\u{10EEED}'), decorated()), false);
    let placeholder = draw(&screen(Glyph::Char('\u{10EEEE}'), decorated()), false);
    assert!(placeholder == other);
    let plain = draw(
        &screen(Glyph::Cluster(PLACEHOLDER), style(rgb(FG), rgb(BG))),
        false,
    );
    assert!(
        plain != draw(&background_only(), false),
        "the glyph is not drawn"
    );
    let decorated = draw(&screen(Glyph::Cluster(PLACEHOLDER), decorated()), false);
    assert!(
        decorated != plain,
        "the underline and strike-through are not drawn"
    );
}
