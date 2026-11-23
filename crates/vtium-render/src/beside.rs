//! The ink beside a piece of a magnified character, which the piece's
//! underline skips as it skips the piece's own ink.
//!
//! A character on a line of double size, and a sized run, draws as
//! pieces: one for each display cell, each drawn as a cell of its own
//! ([`crate::renderer::double`]). A cell's underline leaves out each pixel
//! within a gap of its glyph's ink along the cell's row, and in a
//! magnified cell the gap is as wide as the magnification. A piece holds
//! only its own part of the character's ink, so a descender that ends
//! near the edge between two pieces would leave the gap short on the
//! other side of that edge.
//!
//! So each piece carries a [`Beside`]: for each row of its cell, how many
//! pixels from its left edge and how many from its right edge lie within
//! the gap of the ink in the character's pieces beside it. The glyph cache
//! works it out once, when it cuts the piece from the whole magnified
//! character and can read the pieces beside it. The CPU path's underline
//! leaves those pixels out with the ones near the piece's own ink, and the
//! GPU path hands the same rows to the cell shader. A magnified cell then
//! skips the pixels that the normal cell, magnified, skips.

use vtium_font::{Format, Glyph};

use crate::skip;
use crate::target::Orientation;

/// A piece's glyph as its ink is read: the glyph, the color layer drawn
/// over it in a sized run that mixes text with color glyphs, and how many
/// display pixels across each of its pixels is drawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ink<'a> {
    pub(crate) glyph: &'a Glyph,
    pub(crate) layer: Option<&'a Glyph>,
    pub(crate) scale: u32,
}

impl Ink<'_> {
    /// Whether it has ink an underline skips at the display pixel `x`, `y`
    /// of its cell, upright: a coverage above 0 in a text glyph's pixels,
    /// and when `color` is set, an alpha at the threshold in a color glyph
    /// or the color layer ([`skip::inked`]). Its pixels are its ink cut to
    /// its cell, so ink past its cell does not count.
    fn at(&self, x: u32, y: u32, color: bool) -> bool {
        let s = self.scale.max(1);
        let (gx, gy) = (x.checked_div(s).unwrap_or(0), y.checked_div(s).unwrap_or(0));
        core::iter::once(self.glyph)
            .chain(self.layer)
            .any(|g| skip::inked(g.format() == Format::Rgba, coverage(g, gx, gy), color))
    }
}

/// The coverage of `g` at its pixel `x`, `y`: the mask's byte or the color
/// glyph's alpha, 0 outside it.
fn coverage(g: &Glyph, x: u32, y: u32) -> u8 {
    if x >= g.width() || y >= g.height() {
        return 0;
    }
    let bpp = match g.format() {
        Format::Mask => 1,
        Format::Rgba => 4,
    };
    let i = u64::from(y)
        .saturating_mul(u64::from(g.width()))
        .saturating_add(u64::from(x))
        .saturating_mul(bpp)
        .saturating_add(bpp.saturating_sub(1));
    usize::try_from(i)
        .ok()
        .and_then(|i| g.pixels().get(i))
        .copied()
        .unwrap_or(0)
}

/// For each row of a piece's cell, upright, from the top: how many pixels
/// from the cell's left edge, and how many from its right edge, lie within
/// the gap of the ink in the pieces beside it. An underline leaves those
/// pixels out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Beside {
    /// The cell's width, upright.
    width: u32,
    rows: Box<[[u8; 2]]>,
}

impl Beside {
    /// The pixels near the ink beside a piece whose cell is `cell`, upright,
    /// for a gap of `gap` display pixels: `left` holds the pieces to its
    /// left, the nearest first, and `right` the pieces to its right, the
    /// nearest first, each `None` where the character has no piece. A
    /// pixel `a` pixels from the edge lies within the gap of ink `d`
    /// pixels past the edge, both counted from 1, when `a + d - 1` is at
    /// most the gap, so the nearest ink keeps `gap + 1 - d` pixels clear.
    /// A color glyph's ink counts only where the cell's height says the
    /// underline skips it ([`skip::skips_color`]). `None` when no ink
    /// beside the piece comes that near.
    pub(crate) fn of(
        (width, height): (u32, u32),
        gap: u32,
        left: &[Option<Ink<'_>>],
        right: &[Option<Ink<'_>>],
    ) -> Option<Beside> {
        if width == 0 || gap == 0 {
            return None;
        }
        let color = skip::skips_color(height);
        // the pixel `d` pixels past an edge, counted from 1, lies in the
        // piece `(d - 1) / width` further on, at `(d - 1) % width` pixels
        // from its own edge on this side
        let near = |pieces: &[Option<Ink<'_>>], y: u32, from_right: bool| {
            (1..=gap)
                .find(|&d| {
                    let past = d.saturating_sub(1);
                    let (k, off) = (
                        past.checked_div(width).unwrap_or(0),
                        past.checked_rem(width).unwrap_or(0),
                    );
                    let x = if from_right {
                        width.saturating_sub(1).saturating_sub(off)
                    } else {
                        off
                    };
                    usize::try_from(k)
                        .ok()
                        .and_then(|k| pieces.get(k).copied().flatten())
                        .is_some_and(|p| p.at(x, y, color))
                })
                .map_or(0, |d| {
                    u8::try_from(gap.saturating_add(1).saturating_sub(d)).unwrap_or(u8::MAX)
                })
        };
        let rows: Box<[[u8; 2]]> = (0..height)
            .map(|y| [near(left, y, true), near(right, y, false)])
            .collect();
        rows.iter()
            .any(|&[l, r]| l != 0 || r != 0)
            .then_some(Beside { width, rows })
    }

    /// The pixels kept clear from the left and from the right edge of each
    /// row of the cell, upright, from the top.
    #[cfg(feature = "gpu")]
    pub(crate) fn rows(&self) -> &[[u8; 2]] {
        &self.rows
    }

    /// Bytes it holds.
    pub(crate) fn bytes(&self) -> usize {
        self.rows.len().saturating_mul(2)
    }

    /// Whether the display pixel `x`, `y` of the piece's cell, counted
    /// from the top left of the cell as the display shows it, lies within
    /// the gap of the ink beside the piece, on a display turned by `o`.
    /// The cell's upright column and row under that pixel are the ones the
    /// glyph cache turns there (`stored.rs`'s `turn`).
    pub(crate) fn skips(&self, o: Orientation, (x, y): (i64, i64)) -> bool {
        let w = i64::from(self.width);
        let h = i64::try_from(self.rows.len()).unwrap_or(0);
        let last = |n: i64, v: i64| n.saturating_sub(1).saturating_sub(v);
        let (u, v) = match o {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(w, x), last(h, y)),
            Orientation::Right => (y, last(h, x)),
            Orientation::Left => (last(w, y), x),
        };
        if u < 0 || u >= w {
            return false;
        }
        let Some(&[l, r]) = usize::try_from(v).ok().and_then(|v| self.rows.get(v)) else {
            return false;
        };
        u < i64::from(l) || u >= w.saturating_sub(i64::from(r))
    }
}

/// Where the display shows the pixel at column `u`, row `v` of a cell `w`
/// by `h` pixels upright, from the top left of the cell as the display
/// shows it, on a display turned by `o`: the place the glyph cache turns
/// the pixel to (`stored.rs`'s `turn`).
#[cfg(feature = "gpu")]
pub(crate) fn shown_at(o: Orientation, (u, v): (i64, i64), (w, h): (i64, i64)) -> (i64, i64) {
    let last = |n: i64, v: i64| n.saturating_sub(1).saturating_sub(v);
    match o {
        Orientation::Normal => (u, v),
        Orientation::UpsideDown => (last(w, u), last(h, v)),
        Orientation::Right => (last(h, v), u),
        Orientation::Left => (v, last(w, u)),
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "a test fails by panicking, on small known sizes"
)]
mod tests {
    use super::*;

    /// A 4 by 2 mask glyph with ink at the pixels `lit` names.
    fn glyph(lit: &[(u32, u32)]) -> Glyph {
        let px = (0..2u32)
            .flat_map(|y| (0..4u32).map(move |x| (x, y)))
            .map(|p| if lit.contains(&p) { 255 } else { 0 })
            .collect();
        Glyph::from_pixels(4, 2, Format::Mask, px).unwrap()
    }

    fn ink(g: &Glyph) -> Option<Ink<'_>> {
        Some(Ink {
            glyph: g,
            layer: None,
            scale: 1,
        })
    }

    // Ink in the last column of the piece to the left keeps a gap of 2
    // clear from the left edge on its row; ink one pixel further off keeps
    // one pixel clear; and ink on the other row keeps nothing clear there.
    #[test]
    fn the_ink_beside_keeps_the_gap_clear_from_the_edge() {
        let left = glyph(&[(3, 0), (2, 1)]);
        let b = Beside::of((4, 2), 2, &[ink(&left)], &[None]).unwrap();
        assert_eq!(&*b.rows, &[[2, 0], [1, 0]]);
        let right = glyph(&[(0, 1)]);
        let b = Beside::of((4, 2), 2, &[None], &[ink(&right)]).unwrap();
        assert_eq!(&*b.rows, &[[0, 0], [0, 2]]);
        assert!(Beside::of((4, 2), 2, &[ink(&glyph(&[(1, 0)]))], &[None]).is_none());
    }

    // A gap wider than a cell reaches into the piece after the next one
    #[test]
    fn a_gap_wider_than_a_cell_reads_the_piece_after_the_next() {
        let blank = glyph(&[]);
        let far = glyph(&[(3, 0)]);
        let b = Beside::of((4, 2), 6, &[ink(&blank), ink(&far)], &[]).unwrap();
        assert_eq!(&*b.rows, &[[2, 0], [0, 0]]);
    }

    // A pixel of the display lies in the gap where the turned cell puts
    // the upright pixel that lies in it
    #[test]
    fn the_gap_turns_with_the_display() {
        let left = glyph(&[(3, 0)]);
        let b = Beside::of((4, 2), 1, &[ink(&left)], &[None]).unwrap();
        // upright, only the pixel at column 0 of row 0 lies in the gap
        assert!(b.skips(Orientation::Normal, (0, 0)));
        assert!(!b.skips(Orientation::Normal, (0, 1)));
        assert!(!b.skips(Orientation::Normal, (1, 0)));
        // turned half round, it lies at the bottom right of the 4 by 2 cell
        assert!(b.skips(Orientation::UpsideDown, (3, 1)));
        assert!(!b.skips(Orientation::UpsideDown, (0, 0)));
        // turned a quarter, the cell is 2 wide and 4 tall
        assert!(b.skips(Orientation::Right, (1, 0)));
        assert!(!b.skips(Orientation::Right, (0, 0)));
        assert!(b.skips(Orientation::Left, (0, 3)));
        assert!(!b.skips(Orientation::Left, (1, 3)));
    }
}
