//! 1-bit displays (vtium/vtium#135): each cell's colors map to lit or
//! unlit by the cell, never by brightness, and every pixel's coverage is
//! thresholded at half, so the display gets only pure black and white.
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
//! its coverage, the glyph's or the sprite's; [`Threshold`] lights it at
//! 128 and above, as the kernel's `drm_fb_xrgb8888_to_mono` lights a DRM
//! mono panel from the same gray.

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
}

const LIT: Rgb = Rgb::new(255, 255, 255);
const UNLIT: Rgb = Rgb::new(0, 0, 0);

/// A pixel's coverage on a 1-bit display: [`Threshold`]'s cut.
const HALF: u32 = 128;

fn ink(lit: bool) -> Rgb {
    if lit { LIT } else { UNLIT }
}

/// One cell's colors at `depth`: the identity in color.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ink {
    one: bool,
    bg: Rgb,
    bg_lit: bool,
}

impl Ink {
    /// The colors of a cell on background `bg`, the palette's default
    /// background being `default_bg`.
    pub(crate) fn new(depth: Depth, bg: Rgb, default_bg: Rgb) -> Ink {
        Ink {
            one: depth == Depth::One,
            bg,
            bg_lit: bg != default_bg,
        }
    }

    pub(crate) fn bg(&self) -> Rgb {
        if self.one { ink(self.bg_lit) } else { self.bg }
    }

    /// A color drawn on the cell: its foreground or a decoration's.
    pub(crate) fn fg(&self, c: Rgb) -> Rgb {
        if !self.one {
            return c;
        }
        ink(if c == self.bg {
            self.bg_lit
        } else {
            !self.bg_lit
        })
    }
}

/// The border around the grid, in the default background: unlit at 1 bit.
pub(crate) fn border(depth: Depth, bg: Rgb) -> Rgb {
    match depth {
        Depth::Color => bg,
        Depth::One => UNLIT,
    }
}

/// The text blink at `depth`. A 1-bit display cannot show `Partial`'s
/// glyph halfway to its background, which would draw as the glyph itself
/// and never blink, so its off phase hides the glyph as `Full` does.
pub(crate) fn blink(depth: Depth, blink: TextBlink) -> TextBlink {
    match (depth, blink.style) {
        (Depth::One, BlinkStyle::Partial) => TextBlink {
            style: BlinkStyle::Full,
            ..blink
        },
        _ => blink,
    }
}

/// A color glyph's premultiplied pixel at 1 bit: its coverage alone, in the
/// cell's foreground `fg`, as a mask glyph draws. Its own colors would
/// light the display by their brightness.
pub(crate) fn silhouette(src: u32, fg: Rgb) -> u32 {
    let a = src >> 24;
    let t = |v: u8| div255(u32::from(v).wrapping_mul(a));
    a << 24 | t(fg.r) << 16 | t(fg.g) << 8 | t(fg.b)
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
    luma >= HALF
}

/// A target that, at 1 bit, writes each pixel lit or unlit through `line`,
/// and at color writes through unchanged.
pub(crate) struct Threshold<'a, T: Target + ?Sized> {
    target: &'a mut T,
    one: bool,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Threshold<'a, T> {
    pub(crate) fn new(target: &'a mut T, depth: Depth, line: &'a mut Vec<u32>) -> Self {
        Threshold {
            target,
            one: depth == Depth::One,
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Threshold<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        if !self.one {
            return self.target.write(x, y, pixels);
        }
        self.line.clear();
        self.line
            .extend(pixels.iter().map(|&p| if lit(p) { 0x00ff_ffff } else { 0 }));
        self.target.write(x, y, self.line);
    }
}

#[cfg(test)]
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
        let mut c = crate::Canvas::new(u32::try_from(pixels.len()).unwrap_or(0), 1, 7);
        let mut line = Vec::new();
        Threshold::new(&mut c, Depth::One, &mut line).write(0, 0, pixels);
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
}
