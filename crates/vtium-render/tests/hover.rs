//! The link under the pointer (ADR-1002, "Drawing"): every cell of it the
//! frame marks hovered draws a dashed underline in place of its own
//! underline style, in the cell's underline color, SGR 58's where set and
//! the foreground otherwise. A link that is not hovered draws as the same
//! text with no link. Each hovered cell is compared with the same cell
//! written with the dashed style and no link, which `src/deco.rs` holds to
//! its shape.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, Depth, DrawState, Renderer};
use vtium_term::{Age, Glyph, LinkId, Rgb, Style, UnderlineStyle, Width};

const LINK: LinkId = LinkId::new(7);

fn draw(s: &Screen, hovered: Option<LinkId>, depth: Depth) -> Canvas {
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
    r.draw(
        &s.frame(Age::NEVER).with_hover(hovered, None),
        &mut c,
        0,
        &st,
    );
    c
}

/// The pixels of the cell at `col`, row 0.
fn cell(c: &Canvas, col: u32) -> Vec<u32> {
    (0..16)
        .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

fn underlined(mut st: Style, u: UnderlineStyle, color: Option<Rgb>) -> Style {
    st.attrs.set_underline(u);
    if let Some(c) = color {
        st.underline_color = vtium_term::Color::Rgb(c);
    }
    st
}

/// The styles a link's cells take: plain, under each underline style, and
/// in an SGR 58 color.
fn styles() -> Vec<Style> {
    let plain = style(rgb("ff8040"), rgb("21a0c3"));
    vec![
        plain,
        underlined(plain, UnderlineStyle::Single, None),
        underlined(plain, UnderlineStyle::Double, None),
        underlined(plain, UnderlineStyle::Curly, None),
        underlined(plain, UnderlineStyle::Dotted, None),
        underlined(plain, UnderlineStyle::Dashed, None),
        underlined(plain, UnderlineStyle::Curly, Some(rgb("00ff00"))),
    ]
}

/// Row 0 holds `g` in each of the styles, as a link when `link`; with
/// `dashed`, each cell takes the dashed style in place of its own.
fn screen(link: bool, dashed: bool) -> Screen {
    let all = styles();
    let mut s = Screen::new(all.len(), 1, rgb("c0c0c0"), rgb("101010"));
    for (col, st) in all.into_iter().enumerate() {
        let st = if dashed {
            let mut st = st;
            st.attrs.set_underline(UnderlineStyle::Dashed);
            st
        } else {
            st
        };
        s.lines[0].write(
            col,
            Glyph::Char('g'),
            Width::Narrow,
            &st,
            link.then_some(LINK),
        );
        s.damage.mark_cell(0, col);
    }
    s
}

// covers: osc.8
#[test]
fn a_hovered_link_draws_dashed_in_its_cells_underline_color() {
    for depth in [Depth::Color, Depth::One] {
        let hovered = draw(&screen(true, false), Some(LINK), depth);
        let want = draw(&screen(false, true), None, depth);
        for col in 0..styles().len() as u32 {
            assert_eq!(
                cell(&hovered, col),
                cell(&want, col),
                "{depth:?}, cell {col}"
            );
        }
    }
}

#[test]
fn a_link_not_hovered_draws_as_its_text() {
    for hovered in [None, Some(LinkId::new(8))] {
        let link = draw(&screen(true, false), hovered, Depth::Color);
        let text = draw(&screen(false, false), None, Depth::Color);
        assert_eq!(link.pixels(), text.pixels(), "{hovered:?}");
    }
}

// A frame the hover leaves redraws the link's cells, which the frame marks
// changed, without the dashes.
#[test]
fn the_dashes_go_with_the_hover() {
    let s = screen(true, false);
    let (w, h) = (s.cols as u32 * 8, 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(
        &s.frame(Age::NEVER).with_hover(Some(LINK), None),
        &mut c,
        0,
        &st,
    );
    r.draw(&s.frame(Age::NEVER).with_hover(None, None), &mut c, 1, &st);
    assert_eq!(
        c.pixels(),
        draw(&screen(false, false), None, Depth::Color).pixels()
    );
}
