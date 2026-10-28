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

/// A face of color glyphs: every character it has is an opaque red block.
struct ColorFace;

impl vtium_font::Face for ColorFace {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, c: char) -> bool {
        c != ' '
    }
    fn draw(&self, _: &[char], _: vtium_font::Style) -> Option<vtium_font::Glyph> {
        let px = [255, 0, 0, 255].repeat(8 * 16);
        vtium_font::Glyph::from_pixels(8, 16, vtium_font::Format::Rgba, px)
    }
}

/// A character unifont has no glyph for.
fn lacks() -> char {
    ['\u{10fffd}', '\u{e0080}', '\u{f0000}']
        .into_iter()
        .find(|&c| !vtium_font::Face::has_glyph(&vtium_font::Unifont::new(16), c))
        .unwrap_or_else(|| panic!("a character unifont lacks"))
}

fn color_glyphs(depth: Depth) -> Canvas {
    let mut s = Screen::new(2, 1, rgb("c0c0c0"), rgb(BG));
    s.text(
        0,
        0,
        &format!("A{}", lacks()),
        style(rgb("c0c0c0"), rgb(BG)),
    );
    let mut r = Renderer::new(
        vtium_font::Font::new(ColorFace),
        16,
        16,
        Config {
            depth,
            ..Config::default()
        },
    );
    let mut c = Canvas::new(16, 16, 0);
    let state = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state);
    c
}

// the operator's decision on vtium/vtium#135: a color glyph on a 1-bit
// display is the built-in unifont's bitmap of the character, and where
// unifont has none, its silhouette
#[test]
fn a_color_glyph_draws_as_unifonts_bitmap_or_its_silhouette() {
    let mut u = Screen::new(1, 1, rgb("c0c0c0"), rgb(BG));
    u.text(0, 0, "A", style(rgb("c0c0c0"), rgb(BG)));
    let mut r = Renderer::new(unifont(), 8, 16, one());
    let mut want = Canvas::new(8, 16, 0);
    let state = DrawState {
        background: u.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&u.frame(Age::NEVER), &mut want, 0, &state);
    let c = color_glyphs(Depth::One);
    let a: Vec<Option<u32>> = (0..16)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y))
        .collect();
    let b: Vec<Option<u32>> = (0..16)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .map(|(x, y)| want.pixel(x, y))
        .collect();
    assert_eq!(a, b, "unifont's A");
    assert!((0..16).all(|y| (8..16).all(|x| c.pixel(x, y) == Some(WHITE))));
    let color = color_glyphs(Depth::Color);
    assert_eq!(color.pixel(0, 0), Some(0xff_0000));
}

// the operator's decision (2026-10-28): a gray panel is drawn as color,
// and the panel converts the frame itself
#[test]
fn a_gray_display_draws_as_a_color_one() {
    let color = drawn(&screen(), Config::default(), false);
    let gray = drawn(
        &screen(),
        Config {
            depth: Depth::Gray(4),
            ..Config::default()
        },
        false,
    );
    assert_eq!(gray, color);
    // the one gray exception: a color glyph is unifont's bitmap, in the
    // cell's own colors, and where unifont has none it draws in color
    let g = color_glyphs(Depth::Gray(16));
    let mut u = Screen::new(1, 1, rgb("c0c0c0"), rgb(BG));
    u.text(0, 0, "A", style(rgb("c0c0c0"), rgb(BG)));
    let mut r = Renderer::new(unifont(), 8, 16, Config::default());
    let mut want = Canvas::new(8, 16, 0);
    let state = DrawState {
        background: u.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&u.frame(Age::NEVER), &mut want, 0, &state);
    for y in 0..16 {
        for x in 0..8 {
            assert_eq!(g.pixel(x, y), want.pixel(x, y), "{x},{y}");
            assert_eq!(g.pixel(x + 8, y), Some(0xff_0000), "{},{y}", x + 8);
        }
    }
}

// the operator's decision (2026-10-28): at 1 bit the shades are ordered
// patterns, a quarter, half and three quarters lit, which join from cell to
// cell, where their coverage cut at half drew the light shade empty and the
// others solid
#[test]
fn shades_at_1_bit_are_patterns_that_join() {
    let mut s = Screen::new(6, 2, rgb("c0c0c0"), rgb(BG));
    for row in 0..2 {
        s.text(0, row, "░░▒▒▓▓", style(rgb("c0c0c0"), rgb(BG)));
    }
    let c = drawn(&s, one(), false);
    for (col, want) in [(0, 32), (1, 32), (2, 64), (3, 64), (4, 96), (5, 96)] {
        for row in 0..2 {
            assert_eq!(lit(&c, col, row), want, "cell {col},{row}");
        }
    }
    // each pair of like cells, two rows tall, repeats every 2 pixels
    for pair in 0..3u32 {
        let x0 = pair * 16 + 2;
        for y in 2..2 + 32 - 2 {
            for x in x0..x0 + 16 - 2 {
                assert_eq!(c.pixel(x, y), c.pixel(x + 2, y), "{x},{y} across");
                assert_eq!(c.pixel(x, y), c.pixel(x, y + 2), "{x},{y} down");
            }
        }
    }
    // in color the shades keep their coverage: each cell one even gray
    let color = drawn(&s, Config::default(), false);
    for col in [0, 2, 4] {
        let first = color.pixel(col * 8 + 2, 2).unwrap();
        assert!(first != 0x10_1010 && first != 0xc0_c0c0, "{first:06x}");
        for y in 2..18 {
            for x in col * 8 + 2..col * 8 + 10 {
                assert_eq!(color.pixel(x, y), Some(first), "{x},{y}");
            }
        }
    }
}

/// A face at scale 2 whose shade is even partial coverage, as an
/// antialiased strike's would be.
struct Scaled;

fn even_shade(w: u32, h: u32) -> vtium_font::Glyph {
    let px = vec![0x40; (w * h) as usize];
    vtium_font::Glyph::from_pixels(w, h, vtium_font::Format::Mask, px).unwrap()
}

impl vtium_font::Face for Scaled {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 16,
            height: 32,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, _: char) -> bool {
        true
    }
    fn draw(&self, _: &[char], _: vtium_font::Style) -> Option<vtium_font::Glyph> {
        Some(even_shade(16, 32))
    }
    fn scale(&self) -> u32 {
        2
    }
    fn draw_unscaled(&self, _: &[char], _: vtium_font::Style) -> Option<vtium_font::Glyph> {
        Some(even_shade(8, 16))
    }
}

// a face drawn at a whole scale stores its glyphs unscaled: a shade's
// pattern fills the drawn cell, a quarter lit in each quarter of it
#[test]
fn a_scaled_shade_fills_its_cell_at_1_bit() {
    use vtium_font::{Source, SpritePolicy};
    let font = vtium_font::Font::new(Scaled).with_policy(SpritePolicy::all(Source::Font));
    let bg = rgb("000000");
    let mut s = Screen::new(1, 1, rgb("ffffff"), bg);
    s.text(0, 0, "\u{2591}", style(rgb("ffffff"), bg));
    let mut r = Renderer::new(font, 16, 32, one());
    let mut c = Canvas::new(16, 32, 0);
    let state = DrawState {
        background: bg,
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state);
    let lit_in = |x0: u32, y0: u32| {
        (y0..y0 + 16)
            .flat_map(|y| (x0..x0 + 8).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) == Some(WHITE))
            .count()
    };
    let q = [lit_in(0, 0), lit_in(8, 0), lit_in(0, 16), lit_in(8, 16)];
    assert_eq!(q, [32; 4]);
}
