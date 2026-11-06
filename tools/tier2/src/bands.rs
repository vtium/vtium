//! The check that a screen drawn with line spacing keeps its text as it
//! is drawn without it (ADR-1302 D13).
//!
//! Line spacing makes each row taller, and the console adds the rows
//! above and below the row's text band: half each, the odd row above.
//! A glyph that keeps its size, as Latin letters and emoji do, draws in
//! the text band exactly as it draws in a row with no spacing, and its
//! underline and strike move down with the band. So each cell of the
//! spaced drawing, taken from the rows the spacing adds above it, must
//! equal the same cell of the unspaced drawing, but for a cell holding a
//! sprite, which stretches to the whole taller cell. A spacing that the
//! console rounds or splits other than the screen states moves every band
//! and fails the check.

use crate::draw::Spacing;
use crate::ppm::Image;

/// Checks `spaced`, a grid of `cols` by `rows` cells drawn with
/// `spacing`, against `plain`, the same grid drawn without it: `spaced`
/// is as wide as `plain` and `spacing.above + spacing.below` pixel rows
/// taller in each row, and each cell's text band, starting
/// `spacing.above` rows into the cell, holds the pixels of the same cell
/// of `plain`. A cell for which `stretched(col, row)` is true is not
/// compared. The error names the first cell that differs.
pub fn check(
    spaced: &Image,
    plain: &Image,
    (cols, rows): (usize, usize),
    spacing: Spacing,
    stretched: impl Fn(usize, usize) -> bool,
) -> Result<(), String> {
    let (cols32, rows32) = match (u32::try_from(cols), u32::try_from(rows)) {
        (Ok(c), Ok(r)) if c > 0 && r > 0 => (c, r),
        _ => return Err(format!("a grid of {cols} by {rows} cells")),
    };
    if !plain.width().is_multiple_of(cols32) || !plain.height().is_multiple_of(rows32) {
        return Err(format!(
            "the unspaced drawing, {}x{}, is not {cols} by {rows} whole cells",
            plain.width(),
            plain.height()
        ));
    }
    let width = plain.width() / cols32;
    let text = plain.height() / rows32;
    let cell = text + spacing.above + spacing.below;
    if spaced.width() != plain.width() || spaced.height() != cell * rows32 {
        return Err(format!(
            "the spaced drawing is {}x{}, where {} added rows above and {} below make it {}x{}",
            spaced.width(),
            spaced.height(),
            spacing.above,
            spacing.below,
            plain.width(),
            cell * rows32
        ));
    }
    let bytes = |col: u32| (col * width * 3) as usize..((col + 1) * width * 3) as usize;
    for row in 0..rows32 {
        for col in 0..cols32 {
            if stretched(col as usize, row as usize) {
                continue;
            }
            for y in 0..text {
                let s = &spaced.row(row * cell + spacing.above + y)[bytes(col)];
                let p = &plain.row(row * text + y)[bytes(col)];
                if s != p {
                    return Err(format!(
                        "the cell at column {col}, row {row} does not hold its unspaced pixels {} rows down, its row {y} differing",
                        spacing.above
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// A grid of 2 by 2 cells of 2x3 pixels, each pixel row of each cell
    /// a gray of its own.
    fn plain() -> Image {
        let mut rgb = Vec::new();
        for y in 0..6u8 {
            for x in 0..4u8 {
                rgb.extend([y * 40 + x, y * 40 + x, y * 40 + x]);
            }
        }
        Image::new(4, 6, rgb).unwrap()
    }

    /// `plain` with `above` black rows added above each row of cells and
    /// `below` under it, as line spacing adds them.
    fn spaced(plain: &Image, above: u32, below: u32) -> Image {
        let mut rgb = Vec::new();
        let blank = vec![0u8; 4 * 3];
        for row in 0..2 {
            (0..above).for_each(|_| rgb.extend(&blank));
            for y in 0..3 {
                rgb.extend(plain.row(row * 3 + y));
            }
            (0..below).for_each(|_| rgb.extend(&blank));
        }
        Image::new(4, 6 + 2 * (above + below), rgb).unwrap()
    }

    const fn spacing(above: u32, below: u32) -> Spacing {
        Spacing {
            percent: 0,
            above,
            below,
        }
    }

    // ADR-1302 D13: the text band sits the rows added above it down, and
    // holds the unspaced drawing's pixels
    #[test]
    fn a_band_at_its_rows_passes() {
        let p = plain();
        for (above, below) in [(1, 1), (3, 2), (0, 0)] {
            let s = spaced(&p, above, below);
            assert_eq!(
                check(&s, &p, (2, 2), spacing(above, below), |_, _| false),
                Ok(()),
                "{above} above, {below} below"
            );
        }
    }

    // the odd row placed below, where D13 puts it above, moves each band
    // a row up, and the check names the first cell
    #[test]
    fn the_odd_row_below_fails() {
        let p = plain();
        let s = spaced(&p, 2, 3);
        let e = check(&s, &p, (2, 2), spacing(3, 2), |_, _| false).unwrap_err();
        assert!(
            e.starts_with(
                "the cell at column 0, row 0 does not hold its unspaced pixels 3 rows down"
            ),
            "{e}"
        );
    }

    // a spacing rounded to other rows than the screen states makes the
    // drawing another height
    #[test]
    fn a_spacing_rounded_otherwise_fails() {
        let p = plain();
        let s = spaced(&p, 1, 1);
        let e = check(&s, &p, (2, 2), spacing(2, 1), |_, _| false).unwrap_err();
        assert_eq!(
            e,
            "the spaced drawing is 4x10, where 2 added rows above and 1 below make it 4x12"
        );
    }

    // a sprite stretches to the taller cell, so its cell is not compared,
    // and every other cell still is
    #[test]
    fn a_stretched_cell_is_not_compared() {
        let p = plain();
        let mut rgb = spaced(&p, 1, 1).rgb().to_vec();
        // a pixel of the band of the cell at column 1, row 0
        rgb[(4 + 2) * 3] = 255;
        let s = Image::new(4, 10, rgb).unwrap();
        assert_eq!(
            check(&s, &p, (2, 2), spacing(1, 1), |col, row| (col, row)
                == (1, 0)),
            Ok(())
        );
        let e = check(&s, &p, (2, 2), spacing(1, 1), |_, _| false).unwrap_err();
        assert!(e.starts_with("the cell at column 1, row 0 "), "{e}");
    }
}
