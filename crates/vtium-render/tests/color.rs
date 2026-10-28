//! Color glyphs through the renderer (vtium/vtium#23): a premultiplied
//! RGBA glyph from a vitrified set composes over its cell's background
//! with `src + bg (255 - a) / 255`, takes no color from the foreground,
//! bold or reverse but the background reverse gives it, lies over the
//! underline and under the strike, and draws the same at a whole scale
//! however the cache stores it. Each expected pixel is computed here,
//! apart from the renderer's blend.

mod common;

use std::sync::Arc;

use common::{Screen, rgb, style};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::{Cell, Decorations, Font, Width};
use vtium_render::{Budgets, Canvas, Config, DrawState, Profile, Renderer, Storage};
use vtium_term::{Age, Attrs, Style, UnderlineStyle};

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};
const SMILE: char = '\u{1f600}';
const UNDERLINE_ROW: u32 = 14;
const STRIKE_ROW: u32 = 8;

const FG: &str = "40ff80";
const BG: &str = "2050a0";

/// The glyph, 16x16 premultiplied: column `x`'s alpha `17 x`, from clear
/// to opaque, its red the alpha, its green the alpha's share by row.
fn glyph_px(x: u32, y: u32) -> [u8; 4] {
    let a = x * 17;
    [a, a * y / 15, a / 3, a].map(|v| v as u8)
}

fn set() -> Arc<Set> {
    let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, vtium_font::Style::default())
        .decorations(Decorations {
            underline: UNDERLINE_ROW,
            underline_thickness: 1,
            strike: STRIKE_ROW,
            strike_thickness: 1,
        });
    let px = (0..16)
        .flat_map(|y| (0..16).flat_map(move |x| glyph_px(x, y)))
        .collect();
    let g = color.glyph(Width::Wide, px);
    color.map(SMILE, g);
    let bytes = SetBuilder::new("color-test")
        .strike(color)
        .write()
        .expect("the set writes");
    Arc::new(Set::from_bytes(bytes).expect("the set reads"))
}

fn font(height: u32) -> Font<SetFace> {
    Font::new(SetFace::new(set(), height).expect("a face"))
}

/// The premultiplied `src` over the opaque `bg`, each channel
/// `src + bg (255 - a) / 255` rounded to the nearest.
fn over(src: [u8; 4], bg: u32) -> u32 {
    let a = u32::from(src[3]);
    let ch = |s: u8, shift: u32| {
        let d = (bg >> shift) & 0xff;
        (u32::from(s) + (d * (255 - a) * 2 + 255) / 510) << shift
    };
    ch(src[0], 16) | ch(src[1], 8) | ch(src[2], 0)
}

fn px(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

/// One smile at the start of a row of 3 cells in `st`, drawn with `config`
/// at a cell `height` pixels high.
fn draw(st: Style, height: u32, config: Config) -> Canvas {
    let mut s = Screen::new(3, 1, rgb("c0c0c0"), rgb("000000"));
    s.put(0, 0, SMILE, true, st);
    let f = font(height);
    let scale = height / CELL.height;
    let (w, h) = (3 * CELL.width * scale, CELL.height * scale);
    let mut r = Renderer::new(f, w, h, config);
    let mut c = Canvas::new(w, h, 0x00de_ad00);
    let state = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state);
    c
}

/// Every pixel of the glyph's two cells.
fn cells(c: &Canvas, scale: u32) -> Vec<u32> {
    (0..16 * scale)
        .flat_map(|y| (0..16 * scale).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

// covers: text.emoji
#[test]
fn a_color_glyph_composes_over_its_background_in_its_own_colors() {
    let plain = style(rgb(FG), rgb(BG));
    let c = draw(plain, 16, Config::default());
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(c.pixel(x, y), Some(over(glyph_px(x, y), px(BG))), "{x},{y}");
        }
    }
    // the third cell, unwritten, is the screen's default background
    assert_eq!(c.pixel(20, 3), Some(0));

    // another foreground and bold draw the same pixels
    let mut other = style(rgb("ff0000"), rgb(BG));
    other.attrs.insert(Attrs::BOLD);
    assert_eq!(cells(&draw(other, 16, Config::default()), 1), cells(&c, 1));

    // reverse composes over the foreground, which is now the background
    let mut rev = plain;
    rev.attrs.insert(Attrs::INVERSE);
    let c = draw(rev, 16, Config::default());
    for (x, y) in [(0, 0), (5, 9), (15, 15)] {
        assert_eq!(c.pixel(x, y), Some(over(glyph_px(x, y), px(FG))), "{x},{y}");
    }
}

// covers: text.emoji
#[test]
fn a_color_glyph_lies_over_the_underline_and_under_the_strike() {
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.set_underline(UnderlineStyle::Single);
    st.attrs.insert(Attrs::STRIKE);
    let c = draw(st, 16, Config::default());
    // clear: the underline and the strike in the foreground
    assert_eq!(c.pixel(0, UNDERLINE_ROW), Some(px(FG)));
    assert_eq!(c.pixel(0, STRIKE_ROW), Some(px(FG)));
    // opaque: the glyph hides the underline, the strike crosses the glyph
    let opaque = |y| glyph_px(15, y);
    let solid = |p: [u8; 4]| u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
    assert_eq!(
        c.pixel(15, UNDERLINE_ROW),
        Some(solid(opaque(UNDERLINE_ROW)))
    );
    assert_eq!(c.pixel(15, STRIKE_ROW), Some(px(FG)));
    // rows with neither are the glyph over the background
    assert_eq!(c.pixel(7, 3), Some(over(glyph_px(7, 3), px(BG))));
}

// covers: text.emoji
#[test]
fn a_scaled_color_glyph_draws_alike_however_the_cache_stores_it() {
    let st = style(rgb(FG), rgb(BG));
    let one = draw(st, 16, Config::default());
    let configs = [
        Config::default(),
        Config {
            budgets: Some(Budgets::glyphs(0)),
            ..Config::default()
        },
        Config {
            profile: Profile::Performance,
            ..Config::default()
        },
        Config {
            storage: Storage {
                bits: false,
                unscaled: false,
            },
            ..Config::default()
        },
    ];
    let want: Vec<u32> = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .map(|(x, y)| one.pixel(x / 2, y / 2).unwrap())
        .collect();
    for config in configs {
        let two = draw(st, 32, config);
        assert_eq!(cells(&two, 2), want, "{config:?}");
    }
}
