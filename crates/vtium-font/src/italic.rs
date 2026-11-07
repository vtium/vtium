//! Synthetic italic for bitmap glyphs (ADR-1302 D4): a bitmap font with no
//! italic face draws italic by shearing its glyph by 0.2 about the
//! baseline, one pixel row at a time. Each row moves right by 0.2 times
//! the height of the row's middle above the baseline, rounded to the
//! nearest pixel, a half rounding right. In an 8x16 console font whose
//! baseline lies under row 11, the top five rows move 2 pixels, the next
//! five 1 pixel, the five around the baseline none, and the last row 1
//! pixel left.
//!
//! The shear runs at the font's own size, before the glyph is scaled by
//! its whole factor, so each sheared pixel becomes a square of the scale.
//! vtium-font shears the faces the console draws itself, the built-in
//! unifont and Unscii and a PSF font, and the glyph helper shears a frit's
//! bitmap sources by the same rule ([`shear_shift`]), so both draw the
//! same glyph.

use crate::bounds::SET_CHUNK_BYTES;
use crate::glyph::Glyph;

/// How far row `row` of a bitmap glyph moves right when it is sheared
/// about `baseline`, the row under the baseline counted from the glyph's
/// top: 0.2 times the height of the row's middle above the baseline,
/// `baseline - row - 0.5`, rounded to the nearest pixel with a half
/// rounding right. That is `(baseline - row + 2) / 5` rounded down, which
/// this computes in whole numbers. A negative shift moves the row left.
pub fn shear_shift(row: u32, baseline: u32) -> i32 {
    let above = i64::from(baseline)
        .saturating_sub(i64::from(row))
        .saturating_add(2);
    i32::try_from(above.div_euclid(5)).unwrap_or(0)
}

/// The baseline a bitmap font is sheared about, as a row counted from the
/// glyph's top: the row under the lowest ink of the font's `H`, whose
/// rows `h_rows` gives from the top, each `true` where the row holds ink.
/// A font with no `H`, or a blank one, takes its glyph's bottom edge,
/// `height`. A font that states its own baseline, as a set's strike or a
/// BDF font's ascent does, is sheared about that one instead.
pub fn bitmap_baseline(h_rows: Option<&[bool]>, height: u32) -> u32 {
    h_rows
        .and_then(|rows| rows.iter().rposition(|&ink| ink))
        .and_then(|last| u32::try_from(last).ok())
        .map_or(height, |last| last.saturating_add(1).min(height))
}

/// A glyph's rows sheared at its own size: `width` columns from `left`,
/// the offset of the first column from the left edge of the glyph's
/// cells, over the glyph's height. The columns span the glyph's cells and
/// every column a row moves into, so no ink is cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheared {
    pub left: i32,
    pub width: u32,
    pub height: u32,
    /// One byte a pixel, row by row: the pixel's coverage.
    pub pixels: Vec<u8>,
}

/// The pixels of a glyph `width` by `height`, one byte each, row by row,
/// sheared about `baseline` ([`shear_shift`]). `None` when `pixels` is not
/// that long.
pub fn shear(pixels: &[u8], width: u32, height: u32, baseline: u32) -> Option<Sheared> {
    let w = usize::try_from(width).ok()?;
    if pixels.len() != w.checked_mul(usize::try_from(height).ok()?)? {
        return None;
    }
    let shifts: Vec<i32> = (0..height).map(|y| shear_shift(y, baseline)).collect();
    let left = shifts.iter().copied().min().unwrap_or(0).min(0);
    let right = shifts.iter().copied().max().unwrap_or(0).max(0);
    let out_w = width
        .checked_add(right.unsigned_abs())?
        .checked_add(left.unsigned_abs())?;
    let stride = usize::try_from(out_w).ok()?;
    let mut out = vec![0u8; stride.checked_mul(usize::try_from(height).ok()?)?];
    for ((row, line), shift) in pixels
        .chunks_exact(w.max(1))
        .zip(out.chunks_exact_mut(stride.max(1)))
        .zip(&shifts)
    {
        let at = usize::try_from(shift.checked_sub(left)?).ok()?;
        line.get_mut(at..at.checked_add(w)?)?.copy_from_slice(row);
    }
    Some(Sheared {
        left,
        width: out_w,
        height,
        pixels: out,
    })
}

/// `glyph`, a mask drawn at its font's own size, sheared about `baseline`
/// and then scaled by `scale`, each pixel a square `scale` across: its
/// pixels the sheared ink cut to its cells, and, where the ink passes its
/// cells, its ink box (ADR-1302 D6), the columns its cells and its ink
/// cover cut to its reach, one cell past its cells on each side. A box
/// whose 1-bit rows would pass a set's chunk ([`SET_CHUNK_BYTES`]) is left
/// out and the glyph is cut to its cells, as the glyph helper leaves it out
/// of a set. `None`
/// for a color glyph, which never leans, or past the glyph's bounds.
pub(crate) fn sheared(glyph: &Glyph, baseline: u32, scale: u32) -> Option<Glyph> {
    if glyph.format() != crate::glyph::Format::Mask {
        return None;
    }
    let scale = scale.max(1);
    let s = shear(glyph.pixels(), glyph.width(), glyph.height(), baseline)?;
    let cells = glyph.cells();
    let span = glyph.width().checked_mul(scale)?;
    let cell = span.checked_div(cells.cells())?;
    let height = glyph.height().checked_mul(scale)?;
    let src = usize::try_from(s.width).ok()?;
    // the sheared pixel under the scaled column `x` from the cells' left
    // edge and the scaled row `y`
    let at = |x: i64, y: u32| -> u8 {
        let sx = x
            .div_euclid(i64::from(scale))
            .saturating_sub(i64::from(s.left));
        let sy = y.checked_div(scale).unwrap_or(0);
        usize::try_from(sx)
            .ok()
            .filter(|&sx| sx < src)
            .and_then(|sx| {
                let row = usize::try_from(sy).ok()?.checked_mul(src)?;
                s.pixels.get(row.checked_add(sx)?).copied()
            })
            .unwrap_or(0)
    };
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| (0..span).map(move |x| at(i64::from(x), y)))
        .collect();
    let out =
        Glyph::from_pixels(span, height, crate::glyph::Format::Mask, pixels)?.with_cells(cells);
    // the columns that hold ink, from the cells' left edge, cut to the
    // reach
    let across = i64::from(cell).saturating_mul(i64::from(crate::REACH_CELLS));
    let reach = (
        across.saturating_neg(),
        i64::from(span).saturating_add(across),
    );
    let inked = |x: i64| (0..height).any(|y| at(x, y) != 0);
    let first = (reach.0..reach.1).find(|&x| inked(x));
    let last = (reach.0..reach.1).rev().find(|&x| inked(x));
    let (Some(first), Some(last)) = (first, last) else {
        return Some(out);
    };
    let (from, to) = (first.min(0), last.saturating_add(1).max(i64::from(span)));
    if (from, to) == (0, i64::from(span)) {
        return Some(out);
    }
    let box_pixels: Vec<u8> = (0..height)
        .flat_map(|y| (from..to).map(move |x| at(x, y)))
        .collect();
    let width = u32::try_from(to.saturating_sub(from)).ok()?;
    let bytes = u64::from(width.div_ceil(8)).saturating_mul(u64::from(height));
    if bytes > u64::from(SET_CHUNK_BYTES) {
        return Some(out);
    }
    let left = i32::try_from(from).ok()?;
    // a bitmap glyph's shear moves rows sideways only, so its box is its
    // row's height (ADR-1302 D1)
    out.clone()
        .with_ink((left, 0), (width, height), box_pixels)
        .or(Some(out))
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    // an 8x16 console font whose baseline lies under row 11: rows 0 to 4
    // move 2, 5 to 9 move 1, 10 to 14 stay, and row 15 moves 1 left
    // (ADR-1302 D4)
    #[test]
    fn an_8x16_fonts_rows_move_2_1_0_and_back_1() {
        let shifts: Vec<i32> = (0..16).map(|y| shear_shift(y, 12)).collect();
        assert_eq!(shifts, [2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, -1]);
    }

    #[test]
    fn the_baseline_is_under_hs_lowest_ink_else_the_bottom() {
        let mut h = [false; 16];
        h[2..12].fill(true);
        assert_eq!(bitmap_baseline(Some(&h), 16), 12);
        assert_eq!(bitmap_baseline(Some(&[false; 16]), 16), 16);
        assert_eq!(bitmap_baseline(None, 16), 16);
    }

    // a full column at the glyph's left edge leans into a stair: each row
    // lies at its shift, and the last row's pixel passes the left edge
    #[test]
    fn a_column_leans_and_its_ink_box_holds_what_passes_the_cell() {
        let mut pixels = vec![0u8; 8 * 16];
        for y in 0..16 {
            pixels[y * 8] = 255;
        }
        let g = Glyph::from_pixels(8, 16, crate::glyph::Format::Mask, pixels)
            .expect("a glyph")
            .with_cells(crate::face::Width::Narrow);
        let s = sheared(&g, 12, 1).expect("sheared");
        let column = |row: &[u8]| row.iter().position(|&p| p != 0);
        let cols: Vec<Option<usize>> = s.rows().map(column).collect();
        assert_eq!(cols[0], Some(2));
        assert_eq!(cols[9], Some(1));
        assert_eq!(cols[14], Some(0));
        assert_eq!(cols[15], None, "the last row's pixel lies left of the cell");
        let ink = s.ink().expect("a box");
        assert_eq!((ink.left(), ink.width()), (-1, 9));
        assert_eq!(ink.pixels()[15 * 9], 255);
        // at twice the size each row moves twice as far
        let s2 = sheared(&g, 12, 2).expect("sheared at 2");
        let cols: Vec<Option<usize>> = s2.rows().map(column).collect();
        assert_eq!(
            (cols[0], cols[1], cols[18], cols[28]),
            (Some(4), Some(4), Some(2), Some(0))
        );
        assert_eq!(s2.ink().map(|i| (i.left(), i.width())), Some((-2, 18)));
    }
}
