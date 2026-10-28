//! 1-bit and gray displays (vtium/vtium#135): each cell's colors map to
//! lit or unlit by the cell, never by brightness, and every pixel's
//! coverage is cut to the display's levels: at half on a 1-bit display,
//! so it gets only pure black and white, and to the nearest of a gray
//! panel's levels, so a glyph's edges keep whole-pixel antialiasing.
//!
//! The mapping: the default background is unlit, and a cell whose
//! background differs from it is lit; a color drawn on the cell, its
//! foreground or a decoration's, is the background's opposite, and the
//! background's own state where it equals the background, so a foreground
//! equal to its background stays invisible as it is on a color display.
//! Colors compare by value: a cell set to the default background's color
//! is unlit like the default.
//!
//! Composed from black and white alone, a pixel is a gray whose level is
//! its coverage, the glyph's or the sprite's; [`Quantize`] cuts it to the
//! display's levels by the kernel's own rounding, so a DRM panel's
//! conversion keeps each level exact: 1-bit at 128 and above, as
//! `drm_fb_xrgb8888_to_mono` lights a mono panel.
//!
//! None of this asks where a glyph came from: a vitrified set's strike,
//! the built-in unifont, a PSF file or a strike rasterised at run time
//! (vtium/vtium#123) is coverage in a cell's colors like any other, and a
//! color glyph from any of them takes [`Fallback`]'s bitmap or its
//! silhouette.

use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Unifont, Width};
use vtium_term::Rgb;

use crate::blend::div255;
use crate::renderer::{BlinkStyle, TextBlink};
use crate::target::Target;

/// The display's depth: `display-depth`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth {
    /// Colors drawn as they are.
    #[default]
    Color,
    /// One bit a pixel: lit or unlit.
    One,
    /// A gray panel of this many levels, 4 or 16 on the panels vtium knows,
    /// each a whole step from unlit to lit.
    Gray(u8),
}

impl Depth {
    /// The levels a pixel takes, `None` in color.
    pub fn levels(self) -> Option<u32> {
        match self {
            Depth::Color => None,
            Depth::One => Some(2),
            Depth::Gray(n) => Some(u32::from(n).max(2)),
        }
    }
}

const LIT: Rgb = Rgb::new(255, 255, 255);
const UNLIT: Rgb = Rgb::new(0, 0, 0);

fn ink(lit: bool) -> Rgb {
    if lit { LIT } else { UNLIT }
}

/// One cell's colors at `depth`: the identity in color.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ink {
    mapped: bool,
    bg: Rgb,
    bg_lit: bool,
}

impl Ink {
    /// The colors of a cell on background `bg`, the palette's default
    /// background being `default_bg`.
    pub(crate) fn new(depth: Depth, bg: Rgb, default_bg: Rgb) -> Ink {
        Ink {
            mapped: depth != Depth::Color,
            bg,
            bg_lit: bg != default_bg,
        }
    }

    pub(crate) fn bg(&self) -> Rgb {
        if self.mapped {
            ink(self.bg_lit)
        } else {
            self.bg
        }
    }

    /// A color drawn on the cell: its foreground or a decoration's.
    pub(crate) fn fg(&self, c: Rgb) -> Rgb {
        if !self.mapped {
            return c;
        }
        ink(if c == self.bg {
            self.bg_lit
        } else {
            !self.bg_lit
        })
    }
}

/// The border around the grid, in the default background: unlit on a
/// 1-bit or gray display.
pub(crate) fn border(depth: Depth, bg: Rgb) -> Rgb {
    match depth {
        Depth::Color => bg,
        Depth::One | Depth::Gray(_) => UNLIT,
    }
}

/// The text blink at `depth`. Off color, the cell's colors are lit or
/// unlit, so `Partial`'s glyph halfway to its background would draw as the
/// glyph itself and never blink: its off phase hides the glyph as `Full`
/// does.
pub(crate) fn blink(depth: Depth, blink: TextBlink) -> TextBlink {
    match (depth, blink.style) {
        (Depth::One | Depth::Gray(_), BlinkStyle::Partial) => TextBlink {
            style: BlinkStyle::Full,
            ..blink
        },
        _ => blink,
    }
}

/// A color glyph's premultiplied pixel off color: its coverage alone, in
/// the cell's foreground `fg`, as a mask glyph draws. Its own colors would
/// light the display by their brightness. It draws the color glyphs
/// [`Fallback`] finds no bitmap for.
pub(crate) fn silhouette(src: u32, fg: Rgb) -> u32 {
    let a = src >> 24;
    let t = |v: u8| div255(u32::from(v).wrapping_mul(a));
    a << 24 | t(fg.r) << 16 | t(fg.g) << 8 | t(fg.b)
}

/// A color glyph's stand-in off color (the operator's decision on
/// vtium/vtium#135): the built-in unifont's bitmap of the same character,
/// where unifont draws it and its cell, at a whole scale, is the font's.
/// The set chose the character's glyph; only its pixels change.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fallback {
    unifont: Unifont,
}

impl Fallback {
    /// The stand-in for a font of cell `cell` at `depth`: none in color, or
    /// where unifont has no scale of that cell.
    pub(crate) fn new(depth: Depth, cell: Cell) -> Option<Fallback> {
        let unifont = Unifont::new(cell.height);
        (depth != Depth::Color && unifont.cell() == cell).then_some(Fallback { unifont })
    }

    /// `glyph`'s stand-in when it is a color glyph unifont has a bitmap for.
    pub(crate) fn glyph(
        &self,
        glyph: &Glyph,
        text: &[char],
        width: Width,
        style: Style,
    ) -> Option<Glyph> {
        if glyph.format() != Format::Rgba || !self.unifont.has_glyph(*text.first()?) {
            return None;
        }
        Font::new(self.unifont).glyph(text, width, style)
    }
}

/// `v`, a byte, cut to the nearest of `levels` steps from 0 to 255, by
/// the kernel's rounding: `drm_fb_gray8_to_gray2_line`'s
/// `(v * 3 + 127) / 255` at 4 levels, which keeps a 1-bit cut at 128.
fn quantize(v: u32, levels: u32) -> u32 {
    let steps = levels.saturating_sub(1).max(1);
    let q = v.wrapping_mul(steps).wrapping_add(127) / 255;
    q.wrapping_mul(255).checked_div(steps).unwrap_or(0)
}

/// A composed pixel's gray at `levels`: BT.601's luma of it, as the kernel
/// takes it, quantized.
fn gray(px: u32, levels: u32) -> u32 {
    let (r, g, b) = (px >> 16 & 0xff, px >> 8 & 0xff, px & 0xff);
    let luma = r
        .wrapping_mul(77)
        .wrapping_add(g.wrapping_mul(150))
        .wrapping_add(b.wrapping_mul(29))
        >> 8;
    let v = quantize(luma, levels);
    v << 16 | v << 8 | v
}

/// A target that, off color, writes each pixel at the display's levels
/// through `line`, and in color writes through unchanged.
pub(crate) struct Quantize<'a, T: Target + ?Sized> {
    target: &'a mut T,
    levels: Option<u32>,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Quantize<'a, T> {
    pub(crate) fn new(target: &'a mut T, depth: Depth, line: &'a mut Vec<u32>) -> Self {
        Quantize {
            target,
            levels: depth.levels(),
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Quantize<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let Some(levels) = self.levels else {
            return self.target.write(x, y, pixels);
        };
        self.line.clear();
        self.line.extend(pixels.iter().map(|&p| gray(p, levels)));
        self.target.write(x, y, self.line);
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::blend::{mask, over_bg, tint_over};

    const BLACK: Rgb = Rgb::new(0, 0, 0);
    const RED: Rgb = Rgb::new(255, 0, 0);
    const GREY: Rgb = Rgb::new(200, 200, 200);
    const BLUE: Rgb = Rgb::new(0, 0, 238);

    // vtium/vtium#135: the default background is off; text on it is on,
    // whatever its brightness, pure red's 30% included
    #[test]
    fn text_on_the_default_background_is_lit() {
        let ink = Ink::new(Depth::One, BLACK, BLACK);
        assert_eq!(ink.bg(), UNLIT);
        for fg in [GREY, RED, BLUE, Rgb::new(1, 1, 1)] {
            assert_eq!(ink.fg(fg), LIT, "{fg:?}");
        }
    }

    // a background other than the default, reverse video and the
    // selection included, is lit with its glyph unlit
    #[test]
    fn a_cell_off_the_default_background_is_lit_with_its_glyph_unlit() {
        for (fg, bg) in [(BLACK, GREY), (GREY, BLUE), (BLUE, RED), (RED, LIT)] {
            let ink = Ink::new(Depth::One, bg, BLACK);
            assert_eq!((ink.fg(fg), ink.bg()), (UNLIT, LIT), "{fg:?} on {bg:?}");
        }
    }

    #[test]
    fn a_foreground_equal_to_its_background_stays_invisible() {
        for bg in [BLACK, RED] {
            let ink = Ink::new(Depth::One, bg, BLACK);
            assert_eq!(ink.fg(bg), ink.bg(), "{bg:?}");
        }
    }

    // a light default background is off too: its text is lit
    #[test]
    fn the_default_background_is_unlit_whatever_its_color() {
        let ink = Ink::new(Depth::One, LIT, LIT);
        assert_eq!((ink.fg(BLACK), ink.bg()), (LIT, UNLIT));
        assert_eq!(border(Depth::One, LIT), UNLIT);
    }

    #[test]
    fn color_maps_nothing() {
        let ink = Ink::new(Depth::Color, BLUE, BLACK);
        assert_eq!((ink.fg(RED), ink.bg()), (RED, BLUE));
        assert_eq!(border(Depth::Color, GREY), GREY);
        let partial = TextBlink {
            style: BlinkStyle::Partial,
            decorations: true,
        };
        assert_eq!(blink(Depth::Color, partial), partial);
        assert_eq!(blink(Depth::One, partial).style, BlinkStyle::Full);
        assert!(blink(Depth::One, partial).decorations);
    }

    /// Writes `pixels` through a 1-bit threshold and returns them.
    fn through(pixels: &[u32]) -> Vec<u32> {
        through_at(pixels, Depth::One)
    }

    fn through_at(pixels: &[u32], depth: Depth) -> Vec<u32> {
        let mut c = crate::Canvas::new(u32::try_from(pixels.len()).unwrap_or(0), 1, 7);
        let mut line = Vec::new();
        Quantize::new(&mut c, depth, &mut line).write(0, 0, pixels);
        (0..pixels.len())
            .filter_map(|x| c.pixel(u32::try_from(x).ok()?, 0))
            .collect()
    }

    // coverage at half and above is lit, in the mask, the layers and a
    // color glyph's silhouette alike
    #[test]
    fn coverage_is_thresholded_at_half() {
        let px: Vec<u32> = [0u8, 1, 127, 128, 254, 255]
            .iter()
            .map(|&a| mask(LIT, UNLIT, a))
            .collect();
        let want = [0, 0, 0, 0xff_ffff, 0xff_ffff, 0xff_ffff];
        assert_eq!(through(&px), want);
        let layered: Vec<u32> = [0u8, 1, 127, 128, 254, 255]
            .iter()
            .map(|&a| over_bg(UNLIT, tint_over(0, a, LIT)))
            .collect();
        assert_eq!(through(&layered), want);
        // an opaque red pixel and a half-covered blue one, lit by coverage
        let emoji = [0xff_ff_00_00, 0x80_00_00_80, 0x7f_7f_7f_7f];
        let shown: Vec<u32> = emoji
            .iter()
            .map(|&s| over_bg(UNLIT, silhouette(s, LIT)))
            .collect();
        assert_eq!(through(&shown), [0xff_ffff, 0xff_ffff, 0]);
        // drawn on a lit cell in its unlit foreground, the glyph is dark
        let dark: Vec<u32> = emoji
            .iter()
            .map(|&s| over_bg(LIT, silhouette(s, UNLIT)))
            .collect();
        assert_eq!(through(&dark), [0, 0, 0xff_ffff]);
    }

    #[test]
    fn color_writes_through() {
        let mut c = crate::Canvas::new(2, 1, 7);
        let mut line = Vec::new();
        Quantize::new(&mut c, Depth::Color, &mut line).write(0, 0, &[0x12_3456, 0x80_8080]);
        assert_eq!(
            (c.pixel(0, 0), c.pixel(1, 0)),
            (Some(0x12_3456), Some(0x80_8080))
        );
    }

    // the kernel's gray conversions keep each level: ssd130x's SSD132x
    // takes the high nibble, st7586 the top two bits, and gray2
    // `(v * 3 + 127) / 255`
    #[test]
    fn gray_coverage_takes_the_nearest_level_the_kernel_keeps() {
        let ramp: Vec<u32> = (0..=255u8).map(|a| mask(LIT, UNLIT, a)).collect();
        for (levels, step) in [(4u8, 85u32), (16, 17)] {
            let out = through_at(&ramp, Depth::Gray(levels));
            for (a, &p) in out.iter().enumerate() {
                let v = p & 0xff;
                assert_eq!(p, v << 16 | v << 8 | v, "gray");
                assert_eq!(v % step, 0, "{a} at {levels} levels is {v}");
                let a = u32::try_from(a).unwrap_or(0);
                assert!(v.abs_diff(a) <= step / 2 + 1, "{a} is {v} at {levels}");
                let q = v / step;
                if levels == 16 {
                    assert_eq!(v >> 4, q, "the high nibble");
                } else {
                    assert_eq!(v >> 6, q, "the top two bits");
                    assert_eq!((v * 3 + 127) / 255, q, "gray2");
                }
            }
        }
        assert_eq!(
            through_at(&[0x7f_7f7f, 0x80_8080], Depth::Gray(2)),
            [0, 0xff_ffff]
        );
    }

    #[test]
    fn a_gray_display_maps_colors_as_a_1_bit_one_does() {
        let ink = Ink::new(Depth::Gray(16), BLUE, BLACK);
        assert_eq!((ink.fg(RED), ink.bg()), (UNLIT, LIT));
        assert_eq!(border(Depth::Gray(4), GREY), UNLIT);
        let partial = TextBlink {
            style: BlinkStyle::Partial,
            decorations: false,
        };
        assert_eq!(blink(Depth::Gray(4), partial).style, BlinkStyle::Full);
    }

    fn color_glyph() -> Glyph {
        Glyph::rgba(8, 16).unwrap_or_else(|| panic!("an 8x16 glyph"))
    }

    // the operator's decision on vtium/vtium#135: a color glyph off color
    // is the built-in unifont's bitmap where unifont has one at the cell
    #[test]
    fn a_color_glyph_falls_back_to_unifonts_bitmap() {
        let cell = Cell {
            width: 8,
            height: 16,
        };
        let f = Fallback::new(Depth::One, cell).unwrap_or_else(|| panic!("unifont's cell"));
        let g = f
            .glyph(&color_glyph(), &['A'], Width::Narrow, Style::default())
            .unwrap_or_else(|| panic!("unifont draws A"));
        assert_ne!(g.format(), Format::Rgba);
        let want = Font::new(Unifont::new(16)).glyph(&['A'], Width::Narrow, Style::default());
        assert_eq!(Some(g), want);
        // a glyph that is no color glyph stays
        let mask = Glyph::mask(8, 16).unwrap_or_else(|| panic!("a mask"));
        assert!(
            f.glyph(&mask, &['A'], Width::Narrow, Style::default())
                .is_none()
        );
        // a character unifont lacks keeps the color glyph, for its silhouette
        let lacks = ['\u{378}', '\u{10fffd}', '\u{e0080}']
            .into_iter()
            .find(|&c| !Unifont::new(16).has_glyph(c))
            .unwrap_or_else(|| panic!("a character unifont lacks"));
        let kept = f.glyph(&color_glyph(), &[lacks], Width::Narrow, Style::default());
        assert!(kept.is_none());
        // no stand-in in color, or at a cell unifont has no scale of
        assert!(Fallback::new(Depth::Color, cell).is_none());
        let small = Cell {
            width: 8,
            height: 8,
        };
        assert!(Fallback::new(Depth::One, small).is_none());
        assert!(
            Fallback::new(
                Depth::Gray(4),
                Cell {
                    width: 16,
                    height: 32
                }
            )
            .is_some()
        );
    }
}
