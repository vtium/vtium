//! The pointer's image: an arrow a cell tall, white with a dark outline,
//! its hotspot at the tip, turned with the display; or, under
//! `pointer-char`, a glyph of the font in the same colors, its hotspot at
//! its top left, which the renderer puts on the corner of the cell under the
//! pointer.
//!
//! One image serves both ways of showing the pointer: the display's cursor
//! plane takes it as it is, and the renderer blends it over the cells it
//! draws on a display without one. It is as tall as the C reference's
//! hardware I-beam, the cell's height kept between 8 and
//! [`POINTER_SIDE_MAX`] pixels, in its colors, white on black at 220 of
//! 255 (`src/terminal.c:220-301` at kmscon b1f7a7c5). Its outline is one
//! pixel wide and one more for each 32 of its height, half the I-beam's,
//! which outlined a stroke one pixel wide.

use vtium_font::{Format, Glyph};
use vtium_term::Rgb;

use crate::blend::over_bg;
use crate::bounds::POINTER_SIDE_MAX;
use crate::compose::{Area, int};
use crate::target::{Orientation, Target};

/// The opaque white the arrow fills with.
const FILL: u32 = 0xffff_ffff;

/// The dark outline around it, premultiplied: black at 220 of 255.
const OUTLINE: u32 = 0xdc00_0000;

/// The arrow's outline as a polygon in 18ths of its height, tip first: the
/// left edge, the notch, the tail and the right wing.
const SHAPE: [(i64, i64); 7] = [
    (0, 0),
    (0, 16),
    (4, 12),
    (7, 18),
    (9, 17),
    (6, 11),
    (11, 11),
];

/// The units [`SHAPE`] is drawn in, to one height.
const UNITS: i64 = 18;

/// The arrow's widest point, in [`UNITS`].
const SHAPE_WIDTH: i64 = 11;

/// The pointer's image: premultiplied ARGB8888, `width` pixels a row,
/// with the pixel the pointer's position names at `hot`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrow {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    hot: (u32, u32),
}

impl Arrow {
    /// The arrow for cells `cell_height` pixels tall on a display turned
    /// as `orientation` says: pointing at the text's top left.
    pub fn new(cell_height: u32, orientation: Orientation) -> Arrow {
        Arrow::upright(cell_height).turned(orientation)
    }

    /// `glyph` as the pointer on a display turned as `orientation` says:
    /// each pixel it covers at least half filled, outlined as the arrow is
    /// for the glyph's height, with its top left at the hotspot.
    pub fn glyph(glyph: &Glyph, orientation: Orientation) -> Arrow {
        let (gw, gh) = (i64::from(glyph.width()), i64::from(glyph.height()));
        let thk = (gh.clamp(8, i64::from(POINTER_SIDE_MAX)) / 32).saturating_add(1);
        let covered = |x: i64, y: i64| {
            let (x, y) = (x.saturating_sub(thk), y.saturating_sub(thk));
            if x < 0 || y < 0 || x >= gw || y >= gh {
                return false;
            }
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                return false;
            };
            let at = match glyph.format() {
                Format::Mask => 0,
                Format::Rgba => 3,
            };
            glyph
                .pixel(x, y)
                .and_then(|p| p.get(at))
                .is_some_and(|&c| c >= 128)
        };
        Arrow::filled(gw, gh, thk, covered, (thk, thk)).turned(orientation)
    }

    /// Every pixel, row by row.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixel the pointer's position names: the tip.
    pub fn hot(&self) -> (u32, u32) {
        self.hot
    }

    /// The pixel at `x`, `y`, transparent outside.
    pub(crate) fn pixel(&self, x: i64, y: i64) -> u32 {
        if x < 0 || y < 0 || x >= i64::from(self.width) {
            return 0;
        }
        y.checked_mul(i64::from(self.width))
            .and_then(|i| i.checked_add(x))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.pixels.get(i).copied())
            .unwrap_or(0)
    }

    /// The arrow pointing up and left.
    fn upright(cell_height: u32) -> Arrow {
        let side = i64::from(POINTER_SIDE_MAX);
        let h = i64::from(cell_height).clamp(8, side);
        let thk = (h / 32).saturating_add(1);
        let shape_h = h.saturating_sub(thk.saturating_mul(2));
        let shape_w = SHAPE_WIDTH
            .saturating_mul(shape_h)
            .saturating_add(UNITS.saturating_sub(1))
            / UNITS;
        let w = shape_w.min(side.saturating_sub(thk.saturating_mul(2)));
        let inside = |x: i64, y: i64| {
            let (x, y) = (x.saturating_sub(thk), y.saturating_sub(thk));
            x >= 0 && y >= 0 && x < shape_w && y < shape_h && in_shape(x, y, shape_h)
        };
        Arrow::filled(w, shape_h, thk, inside, (thk, thk))
    }

    /// A shape `w` by `h` with an outline `thk` wide all round: white
    /// where `inside` holds, the outline where it holds within `thk`,
    /// clear elsewhere, with the hotspot at `hot`.
    fn filled(
        w: i64,
        h: i64,
        thk: i64,
        inside: impl Fn(i64, i64) -> bool,
        hot: (i64, i64),
    ) -> Arrow {
        let (w, h) = (
            w.saturating_add(thk.saturating_mul(2)),
            h.saturating_add(thk.saturating_mul(2)),
        );
        let near = |x: i64, y: i64| {
            (y.saturating_sub(thk)..=y.saturating_add(thk))
                .any(|ny| (x.saturating_sub(thk)..=x.saturating_add(thk)).any(|nx| inside(nx, ny)))
        };
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.push(if inside(x, y) {
                    FILL
                } else if near(x, y) {
                    OUTLINE
                } else {
                    0
                });
            }
        }
        let side = |v: i64| u32::try_from(v).unwrap_or(0);
        Arrow {
            pixels,
            width: side(w),
            height: side(h),
            hot: (side(hot.0), side(hot.1)),
        }
    }

    /// This arrow on a display turned as `o` says, which draws the grid's
    /// pixel `x`, `y` where [`Orientation`] puts it.
    fn turned(self, o: Orientation) -> Arrow {
        if o == Orientation::Normal {
            return self;
        }
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let last = |n: i64, v: i64| n.saturating_sub(1).saturating_sub(v);
        let place = |x: i64, y: i64| match o {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(w, x), last(h, y)),
            Orientation::Right => (last(h, y), x),
            Orientation::Left => (y, last(w, x)),
        };
        let (tw, th) = if o.turned() { (h, w) } else { (w, h) };
        let mut pixels = vec![0; self.pixels.len()];
        for y in 0..h {
            for x in 0..w {
                let (tx, ty) = place(x, y);
                let at = ty.saturating_mul(tw).saturating_add(tx);
                if let Some(p) = usize::try_from(at).ok().and_then(|i| pixels.get_mut(i)) {
                    *p = self.pixel(x, y);
                }
            }
        }
        let (hx, hy) = place(i64::from(self.hot.0), i64::from(self.hot.1));
        let side = |v: i64| u32::try_from(v).unwrap_or(0);
        Arrow {
            pixels,
            width: side(tw),
            height: side(th),
            hot: (side(hx), side(hy)),
        }
    }
}

/// A target that blends the arrow, placed at `at` on the display, over
/// every run of pixels written beneath it: the pointer drawn over the
/// cells and the border as they are drawn, never read back.
pub(crate) struct Over<'a, T: Target + ?Sized> {
    target: &'a mut T,
    arrow: Option<(&'a Arrow, Area)>,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Over<'a, T> {
    /// Writes through to `target`, with `arrow` at its area when set;
    /// `line` is the scratch a run under it is blended in.
    pub(crate) fn new(
        target: &'a mut T,
        arrow: Option<(&'a Arrow, Area)>,
        line: &'a mut Vec<u32>,
    ) -> Over<'a, T> {
        Over {
            target,
            arrow,
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Over<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let Some((arrow, at)) = self.arrow else {
            return self.target.write(x, y, pixels);
        };
        let (x0, y0) = (int(x), int(y));
        let end = x0.saturating_add(int(pixels.len()));
        let (from, to) = (x0.max(at.x), end.min(at.x.saturating_add(at.w)));
        if y0 < at.y || y0 >= at.bottom() || from >= to {
            return self.target.write(x, y, pixels);
        }
        self.line.clear();
        self.line.extend_from_slice(pixels);
        let ay = y0.saturating_sub(at.y);
        for px in from..to {
            let src = arrow.pixel(px.saturating_sub(at.x), ay);
            let i = usize::try_from(px.saturating_sub(x0)).unwrap_or(usize::MAX);
            if let Some(p) = self.line.get_mut(i).filter(|_| src != 0) {
                let [_, r, g, b] = p.to_be_bytes();
                *p = over_bg(Rgb::new(r, g, b), src);
            }
        }
        self.target.write(x, y, self.line);
    }
}

/// Whether the centre of pixel `x`, `y` lies in [`SHAPE`] drawn `h` pixels
/// tall: the even-odd rule along the pixel's row, in whole numbers.
fn in_shape(x: i64, y: i64, h: i64) -> bool {
    // the centre and the corners, scaled so that both are whole
    let (px, py) = (
        x.saturating_mul(2).saturating_add(1).saturating_mul(UNITS),
        y.saturating_mul(2).saturating_add(1).saturating_mul(UNITS),
    );
    let scale = h.saturating_mul(2);
    let corner = |(cx, cy): (i64, i64)| (cx.saturating_mul(scale), cy.saturating_mul(scale));
    let mut inside = false;
    let mut prev = SHAPE.last().copied().map(corner).unwrap_or_default();
    for &v in &SHAPE {
        let (ax, ay) = prev;
        let (bx, by) = corner(v);
        prev = (bx, by);
        if (ay > py) == (by > py) {
            continue;
        }
        // the edge crosses the row at x = ax + (py - ay) (bx - ax) / (by - ay);
        // the centre lies left of it when px (by - ay) < that, signs kept
        let dy = by.saturating_sub(ay);
        let lhs = px.saturating_sub(ax).saturating_mul(dy);
        let rhs = py.saturating_sub(ay).saturating_mul(bx.saturating_sub(ax));
        if (dy > 0 && lhs < rhs) || (dy < 0 && lhs > rhs) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_font::Format;

    /// The arrow as text: `#` fill, `o` outline, `.` clear.
    fn picture(a: &Arrow) -> Vec<String> {
        a.pixels()
            .chunks(usize::try_from(a.width()).unwrap_or(1))
            .map(|row| {
                row.iter()
                    .map(|&p| match p {
                        FILL => '#',
                        OUTLINE => 'o',
                        0 => '.',
                        _ => '?',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_arrow_is_a_cell_tall_with_its_tip_at_the_hotspot() {
        let a = Arrow::new(16, Orientation::Normal);
        assert_eq!((a.width(), a.height(), a.hot()), (11, 16, (1, 1)));
        let want = [
            "...........",
            "ooo........",
            "o#oo.......",
            "o##oo......",
            "o###oo.....",
            "o####oo....",
            "o#####oo...",
            "o######oo..",
            "o#######oo.",
            "o########o.",
            "o#####oooo.",
            "o##oo##o...",
            "o#ooo##oo..",
            "ooo.oo##o..",
            ".....o#oo..",
            ".....ooo...",
        ];
        assert_eq!(picture(&a), want);
        // the tip is outlined
        assert_eq!(a.pixel(1, 1), OUTLINE);
    }

    // A glyph as the pointer: its covered pixels filled, outlined as the
    // arrow is, its top left at the hotspot, so that it sits on its cell.
    #[test]
    fn a_glyph_is_filled_and_outlined_with_its_corner_at_the_hotspot() {
        let pixels = [
            [255, 0, 0, 0],
            [255, 0, 0, 0],
            [255, 0, 0, 0],
            [255, 255, 255, 0],
        ]
        .concat();
        let g = Glyph::from_pixels(4, 4, Format::Mask, pixels).expect("a glyph");
        let a = Arrow::glyph(&g, Orientation::Normal);
        assert_eq!((a.width(), a.height(), a.hot()), (6, 6, (1, 1)));
        let want = [
            "ooo...", //
            "o#o...", //
            "o#o...", //
            "o#ooo.", //
            "o###o.", //
            "ooooo.",
        ];
        assert_eq!(picture(&a), want);
        // turned with the display, as the arrow is
        let r = Arrow::glyph(&g, Orientation::Right);
        assert_eq!((r.width(), r.height()), (6, 6));
        assert_eq!(r.pixel(5 - 1, 1), FILL);
        assert_eq!(r.hot(), (4, 1));
    }

    #[test]
    fn the_arrow_keeps_between_8_and_64_pixels_tall() {
        for (cell, h) in [(1, 8), (8, 8), (48, 48), (64, 64), (200, 64)] {
            let a = Arrow::new(cell, Orientation::Normal);
            assert_eq!(a.height(), h, "cell {cell}");
            assert!(a.width() <= POINTER_SIDE_MAX && a.width() < a.height());
            let len = usize::try_from(a.width() * a.height()).unwrap_or(0);
            assert_eq!(a.pixels().len(), len);
        }
    }

    #[test]
    fn the_arrow_turns_with_the_display() {
        let a = Arrow::new(16, Orientation::Normal);
        let (w, h) = (i64::from(a.width()), i64::from(a.height()));
        for (o, turned) in [
            (Orientation::UpsideDown, false),
            (Orientation::Right, true),
            (Orientation::Left, true),
        ] {
            let t = Arrow::new(16, o);
            if turned {
                assert_eq!((t.width(), t.height()), (a.height(), a.width()));
            } else {
                assert_eq!((t.width(), t.height()), (a.width(), a.height()));
            }
            for y in 0..h {
                for x in 0..w {
                    let (tx, ty) = match o {
                        Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                        Orientation::Right => (h - 1 - y, x),
                        Orientation::Left => (y, w - 1 - x),
                        Orientation::Normal => (x, y),
                    };
                    assert_eq!(t.pixel(tx, ty), a.pixel(x, y), "{o:?} {x},{y}");
                }
            }
            let (hx, hy) = (i64::from(a.hot().0), i64::from(a.hot().1));
            let want = match o {
                Orientation::UpsideDown => (w - 1 - hx, h - 1 - hy),
                Orientation::Right => (h - 1 - hy, hx),
                Orientation::Left => (hy, w - 1 - hx),
                Orientation::Normal => (hx, hy),
            };
            let got = t.hot();
            assert_eq!((i64::from(got.0), i64::from(got.1)), want, "{o:?}");
        }
    }
}
