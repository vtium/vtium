//! Synthetic italic for bitmap glyphs (ADR-1302 D4): a bitmap font with no
//! italic face draws italic by shearing its glyph by 0.2 about the
//! baseline, one pixel row at a time. The rows move right in bands of
//! five, each band one pixel further than the band below it. Where the
//! bands sit depends on the font's height at its own size
//! ([`shear_shift`]).
//!
//! A font 16 pixels tall or taller keeps a still band at the baseline: the
//! three rows above the baseline and every row below it do not move, so a
//! letter's last two rows of ink and the row above them move together. In
//! an 8x16 console font whose baseline lies under row 11, rows 0 to 3 move
//! 2 pixels, rows 4 to 8 move 1 pixel, and rows 9 to 15 do not move. A
//! font under 16 pixels tall moves each row by 0.2 times the height of the
//! row's middle above the baseline, rounded to the nearest pixel, so its
//! rows below the baseline's still band move 1 pixel left.
//!
//! The shear runs at the font's own size, before the glyph is scaled by
//! its whole factor, so each sheared pixel becomes a square of the scale.
//! vtium-font shears the faces the console draws itself, the built-in
//! unifont and Unscii and a PSF font, and the glyph helper shears a frit's
//! bitmap sources by the same rule ([`shear`]), so both draw the same
//! glyph.

use crate::bounds::SET_CHUNK_BYTES;
use crate::glyph::Glyph;

/// The height at its own size from which a bitmap font keeps a still band
/// of the three rows above the baseline and every row below it
/// (ADR-1302 D4).
pub const STILL_BAND_FROM: u32 = 16;

/// How far row `row` of a bitmap glyph `height` rows tall, at its font's
/// own size, moves right when it is sheared about `baseline`, the row under
/// the baseline counted from the glyph's top. A negative shift moves the
/// row left.
///
/// Counting `d` rows up from the baseline, `baseline - row`, 1 for the row
/// just above it: from [`STILL_BAND_FROM`] rows tall, a row moves right by
/// `(d + 1) / 5` rounded down, and a row on or below the baseline does not
/// move. Under that height, a row moves right by 0.2 times the height of
/// its middle above the baseline, `d - 0.5`, rounded to the nearest pixel
/// with a half rounding right, which is `(d + 2) / 5` rounded down.
pub fn shear_shift(row: u32, baseline: u32, height: u32) -> i32 {
    let d = i64::from(baseline).saturating_sub(i64::from(row));
    let shift = if height >= STILL_BAND_FROM {
        d.max(0).saturating_add(1).div_euclid(5)
    } else {
        d.saturating_add(2).div_euclid(5)
    };
    i32::try_from(shift).unwrap_or(0)
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
    let shifts: Vec<i32> = (0..height)
        .map(|y| shear_shift(y, baseline, height))
        .collect();
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

    /// The shift of each row of a glyph `height` rows tall, from bands
    /// given as their first row, their last row and how far they move.
    fn bands(height: u32, bands: &[(u32, u32, i32)]) -> Vec<i32> {
        let rows: Vec<i32> = bands
            .iter()
            .flat_map(|&(from, to, by)| (from..=to).map(move |_| by))
            .collect();
        assert_eq!(rows.len(), height as usize, "the bands cover the glyph");
        rows
    }

    // ADR-1302 D4's bands at each height, each font sheared about the row
    // under the lowest ink of its H: under 16 pixels the rule of
    // 2026-11-01, and from 16 pixels the table of 2026-11-07, measured on
    // kbd's Terminus fonts, ter-v14n to ter-v32n
    #[test]
    fn the_bands_at_each_height_are_adr_1302_d4s() {
        let cases: [(u32, u32, Vec<i32>); 7] = [
            // an 8x8 font whose baseline lies under its last row
            (8, 8, bands(8, &[(0, 0, 2), (1, 5, 1), (6, 7, 0)])),
            // ter-v14n, 8x14, baseline under row 11
            (14, 12, bands(14, &[(0, 4, 2), (5, 9, 1), (10, 13, 0)])),
            // ter-v16n, 8x16, baseline under row 11
            (16, 12, bands(16, &[(0, 3, 2), (4, 8, 1), (9, 15, 0)])),
            // ter-v18n, 10x18, baseline under row 14
            (
                18,
                15,
                bands(18, &[(0, 1, 3), (2, 6, 2), (7, 11, 1), (12, 17, 0)]),
            ),
            // ter-v20n, 10x20, baseline under row 15
            (
                20,
                16,
                bands(20, &[(0, 2, 3), (3, 7, 2), (8, 12, 1), (13, 19, 0)]),
            ),
            // ter-v24n, 12x24, baseline under row 18
            (
                24,
                19,
                bands(
                    24,
                    &[(0, 0, 4), (1, 5, 3), (6, 10, 2), (11, 15, 1), (16, 23, 0)],
                ),
            ),
            // ter-v32n, 16x32, baseline under row 25
            (
                32,
                26,
                bands(
                    32,
                    &[
                        (0, 2, 5),
                        (3, 7, 4),
                        (8, 12, 3),
                        (13, 17, 2),
                        (18, 22, 1),
                        (23, 31, 0),
                    ],
                ),
            ),
        ];
        for (height, baseline, want) in cases {
            let got: Vec<i32> = (0..height)
                .map(|y| shear_shift(y, baseline, height))
                .collect();
            assert_eq!(got, want, "{height} rows, baseline {baseline}");
        }
    }

    // from 16 pixels, a letter that stands on the baseline moves its last
    // two rows and the row above them together, and a descender does not
    // move, at every height and baseline (ADR-1302 D4)
    #[test]
    fn from_16_pixels_no_band_edge_falls_on_the_last_rows() {
        for height in 16..=64 {
            for baseline in 3..=height {
                for y in baseline - 3..height {
                    assert_eq!(
                        shear_shift(y, baseline, height),
                        0,
                        "row {y} of {height}, baseline {baseline}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_baseline_is_under_hs_lowest_ink_else_the_bottom() {
        let mut h = [false; 16];
        h[2..12].fill(true);
        assert_eq!(bitmap_baseline(Some(&h), 16), 12);
        assert_eq!(bitmap_baseline(Some(&[false; 16]), 16), 16);
        assert_eq!(bitmap_baseline(None, 16), 16);
    }

    /// A narrow glyph `width` by `height` whose column `x` is inked in
    /// every row.
    fn column(width: u32, height: u32, x: u32) -> Glyph {
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).map(|at| if at == x { 255 } else { 0 }))
            .collect();
        Glyph::from_pixels(width, height, crate::glyph::Format::Mask, pixels)
            .expect("a glyph")
            .with_cells(crate::face::Width::Narrow)
    }

    /// The first inked column of each row of `rows`, each `width` wide.
    fn first_ink(rows: &[u8], width: u32) -> Vec<Option<usize>> {
        rows.chunks(width as usize)
            .map(|row| row.iter().position(|&p| p != 0))
            .collect()
    }

    // a column at the right edge of an 8x16 glyph leans into a stair past
    // the cell: rows 0 to 3 move 2, rows 4 to 8 move 1 and rows 9 to 15
    // stay, so the glyph keeps the rows that stay and its ink box holds the
    // rest
    #[test]
    fn a_column_leans_and_its_ink_box_holds_what_passes_the_cell() {
        let g = column(8, 16, 7);
        let s = sheared(&g, 12, 1).expect("sheared");
        let cut = first_ink(s.pixels(), 8);
        assert_eq!((cut[8], cut[9], cut[15]), (None, Some(7), Some(7)));
        let ink = s.ink().expect("a box");
        assert_eq!((ink.left(), ink.width()), (0, 10));
        let boxed = first_ink(ink.pixels(), 10);
        assert_eq!(
            (boxed[0], boxed[3], boxed[4], boxed[8], boxed[9]),
            (Some(9), Some(9), Some(8), Some(8), Some(7))
        );
        // at twice the size each row moves twice as far
        let s2 = sheared(&g, 12, 2).expect("sheared at 2");
        let ink = s2.ink().expect("a box");
        assert_eq!((ink.left(), ink.width()), (0, 20));
        let boxed = first_ink(ink.pixels(), 20);
        assert_eq!(
            (boxed[0], boxed[7], boxed[8], boxed[17], boxed[18]),
            (Some(18), Some(18), Some(16), Some(16), Some(14))
        );
    }

    // a font under 16 pixels tall moves a row 3 under its baseline 1
    // pixel left: in an 8x8 glyph sheared about row 4, a column at the
    // left edge passes the cell's left edge in its last row
    #[test]
    fn under_16_pixels_a_low_row_leans_past_the_left_edge() {
        let g = column(8, 8, 0);
        let s = sheared(&g, 4, 1).expect("sheared");
        let cut = first_ink(s.pixels(), 8);
        assert_eq!(
            cut,
            [
                Some(1),
                Some(1),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                None
            ],
            "the last row's pixel lies left of the cell"
        );
        let ink = s.ink().expect("a box");
        assert_eq!((ink.left(), ink.width()), (-1, 9));
        assert_eq!(first_ink(ink.pixels(), 9)[7], Some(0));
    }
}
