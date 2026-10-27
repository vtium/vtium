//! The missing glyph `Missing::Hex` draws: a character's codepoint in hex
//! digits (vtium/vtium#85). The C reference had no counterpart; the look is
//! the operator's.
//!
//! The design is 8x16 for a narrow cell and 16x16 for a wide one, scaled by
//! the largest whole factor the cell holds and centred in it.
//! - Narrow: the cell in the text colour with the digits cut out, two rows
//!   of two 3x5 digits for a codepoint up to U+FFFF, three rows of two 3x4
//!   digits, with lower-case b, c and d, past it.
//! - Wide: a box outline with the digits inside in the text colour, 3x5,
//!   two rows of two up to U+FFFF and two rows of three past it.

use crate::face::{Cell, Width};
use crate::glyph::Glyph;

/// The hex digits 3 pixels wide and 5 high, each row three bits, the left
/// pixel highest, the top row first. Every two differ by at least two
/// pixels, as do every two of [`DIGITS_3X4`].
const DIGITS_3X5: [[u8; 5]; 16] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b100, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b010, 0b101, 0b010, 0b101, 0b010], // 8
    [0b111, 0b101, 0b111, 0b001, 0b110], // 9
    [0b010, 0b101, 0b111, 0b101, 0b101], // A
    [0b110, 0b101, 0b110, 0b101, 0b110], // B
    [0b111, 0b100, 0b100, 0b100, 0b111], // C
    [0b110, 0b101, 0b101, 0b101, 0b110], // D
    [0b111, 0b100, 0b111, 0b100, 0b111], // E
    [0b111, 0b100, 0b111, 0b100, 0b100], // F
];

/// The hex digits 3 pixels wide and 4 high, b, c and d in lower case so
/// they differ from 8, e and 0.
const DIGITS_3X4: [[u8; 4]; 16] = [
    [0b111, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b111], // 1
    [0b110, 0b001, 0b010, 0b111], // 2
    [0b110, 0b011, 0b001, 0b110], // 3
    [0b101, 0b101, 0b111, 0b001], // 4
    [0b111, 0b110, 0b001, 0b110], // 5
    [0b011, 0b110, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010], // 7
    [0b111, 0b010, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b110], // 9
    [0b010, 0b101, 0b111, 0b101], // a
    [0b100, 0b111, 0b101, 0b111], // b
    [0b000, 0b111, 0b100, 0b111], // c
    [0b001, 0b111, 0b101, 0b111], // d
    [0b111, 0b110, 0b100, 0b111], // e
    [0b111, 0b100, 0b110, 0b100], // f
];

/// `c`'s codepoint as a glyph exactly `cells` of `cell` wide, or `None`
/// when the cell is smaller than the design.
pub(crate) fn draw(c: char, cell: Cell, cells: Width) -> Option<Glyph> {
    let design = design(u32::from(c), cells)?;
    let width = cell.width.saturating_mul(cells.cells());
    let n = width
        .checked_div(design.width())?
        .min(cell.height.checked_div(design.height())?);
    if n == 0 {
        return None;
    }
    let mut out = Glyph::mask(width, cell.height)?.with_cells(cells);
    if cells == Width::Narrow {
        out.fill(0, 0, width, cell.height, 0xff);
    }
    let x0 = width.saturating_sub(design.width().saturating_mul(n)) / 2;
    let y0 = cell
        .height
        .saturating_sub(design.height().saturating_mul(n))
        / 2;
    for (y, row) in (0u32..).zip(design.rows()) {
        for (x, &alpha) in (0u32..).zip(row) {
            let left = x0.saturating_add(x.saturating_mul(n));
            let top = y0.saturating_add(y.saturating_mul(n));
            out.fill(
                left,
                top,
                left.saturating_add(n),
                top.saturating_add(n),
                alpha,
            );
        }
    }
    Some(out)
}

/// The glyph at its design size: 8x16 narrow, 16x16 wide.
fn design(cp: u32, cells: Width) -> Option<Glyph> {
    let astral = cp > 0xffff;
    let digits: u32 = if astral { 6 } else { 4 };
    let digit = |i: u32| -> usize {
        let shift = 4u32.saturating_mul(digits.saturating_sub(1).saturating_sub(i));
        usize::try_from(cp.checked_shr(shift).unwrap_or(0) & 0xf).unwrap_or(0)
    };
    match cells {
        Width::Narrow => {
            let mut g = Glyph::mask(8, 16)?;
            g.fill(0, 0, 8, 16, 0xff);
            for i in 0..digits {
                let x = at(1, 4, i % 2);
                if astral {
                    put(&mut g, DIGITS_3X4.get(digit(i))?, x, at(1, 5, i / 2), 0);
                } else {
                    put(&mut g, DIGITS_3X5.get(digit(i))?, x, at(2, 7, i / 2), 0);
                }
            }
            Some(g)
        }
        Width::Wide => {
            let mut g = Glyph::mask(16, 16)?;
            g.fill(0, 0, 16, 1, 0xff);
            g.fill(0, 15, 16, 16, 0xff);
            g.fill(0, 0, 1, 16, 0xff);
            g.fill(15, 0, 16, 16, 0xff);
            let (per_row, left, step) = if astral { (3, 3, 4) } else { (2, 4, 5) };
            for i in 0..digits {
                let x = at(left, step, i.checked_rem(per_row).unwrap_or(0));
                let y = at(2, 7, i.checked_div(per_row).unwrap_or(0));
                put(&mut g, DIGITS_3X5.get(digit(i))?, x, y, 0xff);
            }
            Some(g)
        }
    }
}

/// `base` plus `step` times `k`: the `k`th digit's place.
fn at(base: u32, step: u32, k: u32) -> u32 {
    base.saturating_add(step.saturating_mul(k))
}

/// Sets the pixels of a 3-wide digit whose top left is `x`, `y` to `alpha`.
fn put(g: &mut Glyph, rows: &[u8], x: u32, y: u32, alpha: u8) {
    for (dy, &bits) in (0u32..).zip(rows) {
        for dx in 0..3u32 {
            if bits & 0b100u8.checked_shr(dx).unwrap_or(0) != 0
                && let Some(p) = g.coverage_mut(x.saturating_add(dx), y.saturating_add(dy))
            {
                *p = alpha;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DIGITS_3X4, DIGITS_3X5};

    /// How many pixels two digits differ by.
    fn apart(a: &[u8], b: &[u8]) -> u32 {
        a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
    }

    // every pair of digits differs by at least two pixels, so no one stray
    // pixel turns one into another
    #[test]
    fn every_digit_is_two_pixels_from_the_others() {
        for (i, a) in DIGITS_3X5.iter().enumerate() {
            for (j, b) in DIGITS_3X5.iter().enumerate().skip(i).skip(1) {
                assert!(apart(a, b) >= 2, "3x5 {i:X} and {j:X}");
            }
        }
        for (i, a) in DIGITS_3X4.iter().enumerate() {
            for (j, b) in DIGITS_3X4.iter().enumerate().skip(i).skip(1) {
                assert!(apart(a, b) >= 2, "3x4 {i:x} and {j:x}");
            }
        }
    }
}
