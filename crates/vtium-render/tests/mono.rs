//! A 1-bit display (vtium/vtium#135): every pixel drawn is black or
//! white, each cell lit or unlit by its colors, never by their brightness.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{BlinkStyle, Canvas, Config, Depth, DrawState, Renderer, TextBlink};
use vtium_term::{Age, Attrs, Color, UnderlineStyle};

const WHITE: u32 = 0xff_ffff;

fn one() -> Config {
    Config {
        depth: Depth::One,
        ..Config::default()
    }
}

/// `s` drawn whole on a display 4 pixels wider and taller than its grid,
/// with `config`.
fn drawn(s: &Screen, config: Config, blink_hidden: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8 + 4, s.rows as u32 * 16 + 4);
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0x12_3456);
    let state = DrawState {
        background: s.palette.bg(),
        blink_hidden,
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state);
    c
}

/// The lit pixels of the cell at `col`, `row`, inside the 2-pixel border.
fn lit(c: &Canvas, col: u32, row: u32) -> usize {
    (0..16)
        .flat_map(|y| (0..8).map(move |x| (col * 8 + 2 + x, row * 16 + 2 + y)))
        .filter(|&(x, y)| c.pixel(x, y) == Some(WHITE))
        .count()
}

fn bits_only(c: &Canvas) {
    for y in 0..c.height() {
        for x in 0..c.width() {
            let p = c.pixel(x, y).unwrap();
            assert!(p == 0 || p == WHITE, "{p:06x} at {x},{y}");
        }
    }
}

const BG: &str = "101010";

fn screen() -> Screen {
    let bg = rgb(BG);
    let mut s = Screen::new(8, 4, rgb("c0c0c0"), bg);
    // pure red, about 30% bright: lit all the same
    s.text(0, 0, "H", style(rgb("ff0000"), bg));
    // dark blue on the default background
    s.text(1, 0, "H", style(rgb("00002a"), bg));
    // a colored background: lit, its glyph unlit
    s.text(2, 0, "H", style(rgb("ffff00"), rgb("0000ff")));
    // a foreground equal to its background: invisible
    s.text(3, 0, "H", style(bg, bg));
    s.text(4, 0, "H", style(rgb("00ff00"), rgb("00ff00")));
    // reverse video
    let mut rev = style(rgb("c0c0c0"), bg);
    rev.attrs = Attrs::INVERSE;
    s.text(5, 0, "H", rev);
    // a cell set to the default background's color is unlit
    s.text(6, 0, " ", style(rgb("c0c0c0"), bg));
    // a curly underline in red under nothing, and struck spaces
    let mut curl = style(rgb("c0c0c0"), bg);
    curl.attrs.set_underline(UnderlineStyle::Curly);
    curl.underline_color = Color::Rgb(rgb("ff0000"));
    s.text(0, 1, "    ", curl);
    let mut struck = style(rgb("c0c0c0"), bg);
    struck.attrs = Attrs::STRIKE;
    s.text(0, 2, " ", struck);
    s
}

#[test]
fn every_pixel_is_black_or_white() {
    bits_only(&drawn(&screen(), one(), false));
}

#[test]
fn colors_map_to_lit_or_unlit_by_cell() {
    let c = drawn(&screen(), one(), false);
    let color = drawn(&screen(), Config::default(), false);
    let glyph = (0..16)
        .flat_map(|y| (0..8).map(move |x| (x + 2, y + 2)))
        .filter(|&(x, y)| color.pixel(x, y) == Some(0xff_0000))
        .count();
    assert!(glyph > 10, "the H covers {glyph} pixels");
    // text on the default background, red and dark blue alike
    assert_eq!(lit(&c, 0, 0), glyph);
    assert_eq!(lit(&c, 1, 0), glyph);
    // a colored background lit, its glyph unlit
    assert_eq!(lit(&c, 2, 0), 128 - glyph);
    // a foreground equal to its background: none, or all on a colored one
    assert_eq!(lit(&c, 3, 0), 0);
    assert_eq!(lit(&c, 4, 0), 128);
    // reverse video is lit with its glyph unlit
    assert_eq!(lit(&c, 5, 0), 128 - glyph);
    assert_eq!(lit(&c, 6, 0), 0);
    // the border is unlit
    for (x, y) in [(0, 0), (1, 40), (c.width() - 1, c.height() - 1)] {
        assert_eq!(c.pixel(x, y), Some(0), "{x},{y}");
    }
}

// the decorations are lit like the glyph, the curl's coverage cut at half
#[test]
fn decorations_are_lit_like_the_glyph() {
    let c = drawn(&screen(), one(), false);
    for col in 0..4 {
        assert!(lit(&c, col, 1) > 0, "the curl in column {col}");
    }
    assert!(lit(&c, 0, 2) > 0, "the strike");
    assert_eq!(lit(&c, 0, 3), 0);
}

// a 1-bit display cannot show Partial's half-way glyph, so its off phase
// hides the glyph
#[test]
fn partial_blink_hides_the_glyph_at_one_bit() {
    let mut s = Screen::new(2, 1, rgb("c0c0c0"), rgb(BG));
    let mut blink = style(rgb("c0c0c0"), rgb(BG));
    blink.attrs = Attrs::BLINK;
    s.text(0, 0, "H", blink);
    let config = Config {
        blink: TextBlink {
            style: BlinkStyle::Partial,
            decorations: false,
        },
        ..one()
    };
    assert!(lit(&drawn(&s, config, false), 0, 0) > 0);
    assert_eq!(lit(&drawn(&s, config, true), 0, 0), 0);
    let color = Config {
        depth: Depth::Color,
        ..config
    };
    let partial = drawn(&s, color, true);
    assert!((0..8).any(|x| (0..16).any(|y| partial.pixel(x + 2, y + 2) != Some(0x10_1010))));
}

// the depth is kept for the owner, which builds the next renderer with it
#[test]
fn a_renderer_reports_its_depth() {
    let r = Renderer::new(unifont(), 16, 16, one());
    assert_eq!(r.depth(), Depth::One);
    let r = Renderer::new(unifont(), 16, 16, Config::default());
    assert_eq!(r.depth(), Depth::Color);
}
