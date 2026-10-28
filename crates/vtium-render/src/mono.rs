//! 1-bit displays (vtium/vtium#135): each cell's colors map to lit or
//! unlit by the cell, never by brightness, and every pixel's coverage is
//! cut at half, so the display gets only pure black and white. A gray
//! panel is drawn as a color display, and the panel converts the frame
//! itself (the operator's decision, 2026-10-28), but for a color glyph,
//! which takes [`Fallback`]'s bitmap there too.
//!
//! The mapping: polarity follows the default background (the operator's
//! decision, 2026-10-28). A dark one, its luma under the kernel's cut of
//! 128, leaves the screen unlit; a light one lights it, so a light theme
//! draws dark text on a lit screen. A cell whose background differs from
//! the default flips from the screen's state; a color drawn on the cell,
//! its foreground or a decoration's, is the background's opposite, and the
//! background's own state where it equals the background, so a foreground
//! equal to its background stays invisible as it is on a color display.
//! Colors compare by value: a cell set to the default background's color
//! takes the screen's state like the default.
//!
//! Composed from black and white alone, a pixel is a gray whose level is
//! its coverage, the glyph's or the sprite's; [`Threshold`] lights it at
//! 128 and above, as the kernel's `drm_fb_xrgb8888_to_mono` lights a mono
//! panel from the same gray.
//!
//! None of this asks where a glyph came from: a vitrified set's strike,
//! the built-in unifont, a PSF file or a strike rasterised at run time
//! (vtium/vtium#123) is coverage in a cell's colors like any other, and a
//! color glyph from any of them takes [`Fallback`]'s bitmap or its
//! silhouette. Nothing here is the CPU path's alone: a hardware path
//! draws a 1-bit display by the same per-cell mapping and per-pixel cut.

use vtium_font::sprite;
use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Unifont, Width};
use vtium_term::Rgb;

use crate::blend::{div255, pixel};
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
    /// A gray panel of this many levels, drawn as [`Depth::Color`]: the
    /// panel converts the frame to its levels itself.
    Gray(u8),
}

impl Depth {
    /// Whether the display is drawn in black and white.
    fn one_bit(self) -> bool {
        self == Depth::One
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
            mapped: depth.one_bit(),
            bg,
            bg_lit: (bg != default_bg) != light(default_bg),
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

/// The border around the grid, in the default background: at 1 bit, the
/// screen's state, lit for a light default background.
pub(crate) fn border(depth: Depth, bg: Rgb) -> Rgb {
    if depth.one_bit() { ink(light(bg)) } else { bg }
}

/// Whether the default background `c` is light: its BT.601 luma at least
/// 128, the cut the kernel lights a mono panel's pixel at, so the screen
/// takes the state the panel would give the color.
fn light(c: Rgb) -> bool {
    lit(pixel(c))
}

/// The text blink at `depth`. At 1 bit the cell's colors are lit or
/// unlit, so `Partial`'s glyph halfway to its background would draw as the
/// glyph itself and never blink: its off phase hides the glyph as `Full`
/// does.
pub(crate) fn blink(depth: Depth, blink: TextBlink) -> TextBlink {
    match (depth, blink.style) {
        (Depth::One, BlinkStyle::Partial) => TextBlink {
            style: BlinkStyle::Full,
            ..blink
        },
        _ => blink,
    }
}

/// A color glyph's premultiplied pixel at 1 bit: its coverage alone, in
/// the cell's foreground `fg`, as a mask glyph draws. Its own colors would
/// light the display by their brightness. It draws the color glyphs
/// [`Fallback`] finds no bitmap for.
pub(crate) fn silhouette(src: u32, fg: Rgb) -> u32 {
    let a = src >> 24;
    let t = |v: u8| div255(u32::from(v).wrapping_mul(a));
    a << 24 | t(fg.r) << 16 | t(fg.g) << 8 | t(fg.b)
}

/// A glyph's stand-in off color, the operator's decisions on
/// vtium/vtium#135. A color glyph, on a 1-bit or gray display, is the
/// built-in unifont's bitmap of the same character, where unifont draws it
/// and its cell, at a whole scale, is the font's: the set chose the
/// character's glyph, and only its pixels change. At 1 bit, a sprite
/// the cut at half breaks, drawn with partial coverage, is vtium-font's
/// 1-bit shape of it ([`sprite::one_bit`]): the shades' ordered patterns,
/// and the wedges whose tips and thin diagonals the cut splits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fallback {
    one_bit: bool,
    unifont: Option<Unifont>,
    /// The sprites' light line, as the font draws them.
    thickness: u32,
}

impl Fallback {
    /// The stand-ins for a font of cell `cell`, whose sprites' light line
    /// is `thickness`, at `depth`: none in color.
    pub(crate) fn new(depth: Depth, cell: Cell, thickness: u32) -> Option<Fallback> {
        let unifont = Some(Unifont::new(cell.height)).filter(|u| u.cell() == cell);
        (depth != Depth::Color).then_some(Fallback {
            one_bit: depth.one_bit(),
            unifont,
            thickness,
        })
    }

    /// `glyph`'s stand-in, at the size `glyph` draws at `scale`: unifont's
    /// bitmap for a color glyph unifont has, and at 1 bit the 1-bit shape of
    /// a sprite of partial coverage, drawn at that size so it is not
    /// scaled.
    pub(crate) fn glyph(
        &self,
        glyph: &Glyph,
        scale: u32,
        text: &[char],
        width: Width,
        style: Style,
    ) -> Option<Glyph> {
        let &first = text.first()?;
        match glyph.format() {
            Format::Rgba => {
                let u = self.unifont.filter(|u| u.has_glyph(first))?;
                Font::new(u).glyph(text, width, style)
            }
            Format::Mask => {
                let partial = glyph.pixels().iter().any(|&p| p != 0 && p != 0xff);
                let one = text.len() == 1;
                let (w, h) = (
                    glyph.width().saturating_mul(scale),
                    glyph.height().saturating_mul(scale),
                );
                (self.one_bit && one && partial)
                    .then(|| sprite::one_bit(first, w, h, self.thickness))
                    .flatten()
            }
        }
    }
}

/// Whether a composed pixel is lit: its gray, BT.601's luma of it as the
/// kernel takes it, at least half.
fn lit(px: u32) -> bool {
    let (r, g, b) = (px >> 16 & 0xff, px >> 8 & 0xff, px & 0xff);
    let luma = r
        .wrapping_mul(77)
        .wrapping_add(g.wrapping_mul(150))
        .wrapping_add(b.wrapping_mul(29))
        >> 8;
    luma >= 128
}

/// A target that, at 1 bit, writes each pixel lit or unlit through `line`,
/// and otherwise writes through unchanged.
pub(crate) struct Threshold<'a, T: Target + ?Sized> {
    target: &'a mut T,
    one_bit: bool,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Threshold<'a, T> {
    pub(crate) fn new(target: &'a mut T, depth: Depth, line: &'a mut Vec<u32>) -> Self {
        Threshold {
            target,
            one_bit: depth.one_bit(),
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Threshold<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        if !self.one_bit {
            return self.target.write(x, y, pixels);
        }
        self.line.clear();
        self.line
            .extend(pixels.iter().map(|&p| if lit(p) { 0x00ff_ffff } else { 0 }));
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

    // a light default background lights the screen and leaves its text
    // unlit; a colored cell flips from the screen; the cut is luma 128
    #[test]
    fn a_light_default_background_lights_the_screen() {
        let ink = Ink::new(Depth::One, LIT, LIT);
        assert_eq!((ink.fg(BLACK), ink.bg()), (UNLIT, LIT));
        assert_eq!(border(Depth::One, LIT), LIT);
        let ink = Ink::new(Depth::One, BLUE, LIT);
        assert_eq!((ink.fg(LIT), ink.bg()), (LIT, UNLIT));
        // luma 128 is light, 127 dark
        let (l, d) = (Rgb::new(128, 128, 128), Rgb::new(127, 127, 127));
        assert_eq!(border(Depth::One, l), LIT);
        assert_eq!(border(Depth::One, d), UNLIT);
        assert_eq!(Ink::new(Depth::One, d, d).bg(), UNLIT);
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
        Threshold::new(&mut c, depth, &mut line).write(0, 0, pixels);
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
        Threshold::new(&mut c, Depth::Color, &mut line).write(0, 0, &[0x12_3456, 0x80_8080]);
        assert_eq!(
            (c.pixel(0, 0), c.pixel(1, 0)),
            (Some(0x12_3456), Some(0x80_8080))
        );
    }

    // the operator's decision (2026-10-28): a gray panel is drawn as
    // color, and converts the frame itself
    #[test]
    fn a_gray_display_draws_as_color() {
        let ink = Ink::new(Depth::Gray(16), BLUE, BLACK);
        assert_eq!((ink.fg(RED), ink.bg()), (RED, BLUE));
        assert_eq!(border(Depth::Gray(4), GREY), GREY);
        let partial = TextBlink {
            style: BlinkStyle::Partial,
            decorations: false,
        };
        assert_eq!(blink(Depth::Gray(4), partial), partial);
        let px = [0x12_3456, 0x80_8080, 0x7f_7f7f];
        assert_eq!(through_at(&px, Depth::Gray(4)), px);
        let cell = Cell {
            width: 8,
            height: 16,
        };
        // the one gray exception: a color glyph takes unifont's bitmap
        let f = Fallback::new(Depth::Gray(4), cell, 1).unwrap_or_else(|| panic!("a stand-in"));
        let a = f.glyph(&color_glyph(), 1, &['A'], Width::Narrow, Style::default());
        assert!(a.is_some_and(|g| g.format() == Format::Mask));
        // and a shade keeps its coverage
        let shade = sprite::draw('\u{2591}', 8, 16, 1).unwrap_or_else(|| panic!("the light shade"));
        assert!(
            f.glyph(&shade, 1, &['\u{2591}'], Width::Narrow, Style::default())
                .is_none()
        );
    }

    // the operator's decision (2026-10-28): at 1 bit a shade drawn as even
    // coverage is an ordered pattern; one already 1-bit, as a bitmap
    // font's, stays
    #[test]
    fn a_shade_of_partial_coverage_takes_its_pattern_at_1_bit() {
        let cell = Cell {
            width: 8,
            height: 8,
        };
        let f = Fallback::new(Depth::One, cell, 1).unwrap_or_else(|| panic!("a stand-in"));
        for (c, lit) in [('\u{2591}', 16), ('\u{2592}', 32), ('\u{2593}', 48)] {
            let even = sprite::draw(c, 8, 8, 1).unwrap_or_else(|| panic!("{c}"));
            let g = f
                .glyph(&even, 1, &[c], Width::Narrow, Style::default())
                .unwrap_or_else(|| panic!("{c}'s pattern"));
            assert_eq!(
                g.pixels().iter().filter(|&&p| p == 0xff).count(),
                lit,
                "{c}"
            );
            assert!(g.pixels().iter().all(|&p| p == 0 || p == 0xff), "{c}");
            assert!(
                f.glyph(&g, 1, &[c], Width::Narrow, Style::default())
                    .is_none(),
                "{c}"
            );
        }
        let block = sprite::draw('\u{2588}', 8, 8, 1).unwrap_or_else(|| panic!("█"));
        assert!(
            f.glyph(&block, 1, &['\u{2588}'], Width::Narrow, Style::default())
                .is_none()
        );
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
        let f = Fallback::new(Depth::One, cell, 1).unwrap_or_else(|| panic!("unifont's cell"));
        let g = f
            .glyph(&color_glyph(), 1, &['A'], Width::Narrow, Style::default())
            .unwrap_or_else(|| panic!("unifont draws A"));
        assert_ne!(g.format(), Format::Rgba);
        let want = Font::new(Unifont::new(16)).glyph(&['A'], Width::Narrow, Style::default());
        assert_eq!(Some(g), want);
        // a glyph that is no color glyph stays
        let mask = Glyph::mask(8, 16).unwrap_or_else(|| panic!("a mask"));
        assert!(
            f.glyph(&mask, 1, &['A'], Width::Narrow, Style::default())
                .is_none()
        );
        // a character unifont lacks keeps the color glyph, for its silhouette
        let lacks = ['\u{378}', '\u{10fffd}', '\u{e0080}']
            .into_iter()
            .find(|&c| !Unifont::new(16).has_glyph(c))
            .unwrap_or_else(|| panic!("a character unifont lacks"));
        let kept = f.glyph(&color_glyph(), 1, &[lacks], Width::Narrow, Style::default());
        assert!(kept.is_none());
        // no stand-in in color, nor unifont's at a cell it has no scale of
        assert!(Fallback::new(Depth::Color, cell, 1).is_none());
        let small = Cell {
            width: 8,
            height: 8,
        };
        let f = Fallback::new(Depth::One, small, 1).unwrap_or_else(|| panic!("a stand-in"));
        let g = Glyph::rgba(8, 8).unwrap_or_else(|| panic!("an 8x8 glyph"));
        assert!(
            f.glyph(&g, 1, &['A'], Width::Narrow, Style::default())
                .is_none()
        );
        assert!(
            Fallback::new(
                Depth::One,
                Cell {
                    width: 16,
                    height: 32
                },
                1
            )
            .is_some_and(|f| f.unifont.is_some())
        );
    }
}
