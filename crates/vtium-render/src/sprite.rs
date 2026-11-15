//! Shapes vtium draws itself, as it draws its box-drawing sprites, from
//! the cell's size alone and with no font.
//!
//! [`hollow_box`] is what vtium's own messages draw for a character that
//! its built-in font lacks: a hollow box as wide as the character's cells.
//! It says plainly that a character is there, which a blank would hide and
//! a question mark would confuse with text.
//!
//! References:
//! - ADR-1204 D1: a character the built-in font lacks draws as a hollow
//!   box as wide as its cells, which vtium draws itself.

use vtium_font::sprite::light_line;
use vtium_font::{Cell, Format, Glyph, Width};

/// A hollow box `cells` cells of `cell` wide and one cell high, a 1-bit
/// mask. Its lines are as thick as a light line of the sprites at the
/// cell's height. It keeps an eighth of the cell, at least a pixel, clear
/// on its left and right and above and below it, so two boxes side by
/// side stay apart. `None` for a cell too small to hold a box with a
/// hollow inside, or past [`Glyph::mask`]'s bound.
pub fn hollow_box(cell: Cell, cells: Width) -> Option<Glyph> {
    let width = cell.width.checked_mul(cells.cells())?;
    let height = cell.height;
    let thick = light_line(height);
    let side = (cell.width / 8).max(1);
    let over = (height / 8).max(1);
    let (left, right) = (side, width.checked_sub(side)?);
    let (top, bottom) = (over, height.checked_sub(over)?);
    // a box needs a hollow inside its two lines on each axis
    let room = |a: u32, b: u32| b.saturating_sub(a) > thick.saturating_mul(2);
    if !(room(left, right) && room(top, bottom)) {
        return None;
    }
    let mut pixels = vec![0u8; usize::try_from(width.checked_mul(height)?).ok()?];
    for y in top..bottom {
        for x in left..right {
            let edge = x < left.saturating_add(thick)
                || x >= right.saturating_sub(thick)
                || y < top.saturating_add(thick)
                || y >= bottom.saturating_sub(thick);
            if edge {
                let i = usize::try_from(y.checked_mul(width)?.checked_add(x)?).ok()?;
                *pixels.get_mut(i)? = 0xff;
            }
        }
    }
    Some(Glyph::from_pixels(width, height, Format::Mask, pixels)?.with_cells(cells))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn rows(g: &Glyph) -> Vec<String> {
        (0..g.height())
            .map(|y| {
                (0..g.width())
                    .map(|x| match g.pixel(x, y) {
                        Some([0]) => '.',
                        _ => '#',
                    })
                    .collect()
            })
            .collect()
    }

    /// An 8×8 cell's box keeps a pixel clear on each side, and its lines
    /// are a pixel thick.
    #[test]
    fn the_hollow_box_fills_its_cell_less_a_margin() {
        let g = hollow_box(
            Cell {
                width: 8,
                height: 8,
            },
            Width::Narrow,
        )
        .expect("a box");
        assert_eq!(
            rows(&g),
            [
                "........", ".######.", ".#....#.", ".#....#.", ".#....#.", ".#....#.", ".######.",
                "........",
            ]
        );
        assert_eq!(g.cells(), Width::Narrow);
    }

    /// A wide character's box spans both of its cells, and a larger cell
    /// draws thicker lines.
    #[test]
    fn a_wide_characters_box_spans_two_cells() {
        let g = hollow_box(
            Cell {
                width: 16,
                height: 32,
            },
            Width::Wide,
        )
        .expect("a box");
        assert_eq!((g.width(), g.height(), g.cells()), (32, 32, Width::Wide));
        let r = rows(&g);
        assert_eq!(r[0], ".".repeat(32));
        assert_eq!(r[4], format!("..{}..", "#".repeat(28)));
        assert_eq!(r[5], format!("..{}..", "#".repeat(28)));
        assert_eq!(r[6], format!("..##{}##..", ".".repeat(24)));
    }

    /// A cell too small for a hollow inside draws no box.
    #[test]
    fn a_cell_too_small_draws_no_box() {
        assert!(
            hollow_box(
                Cell {
                    width: 3,
                    height: 3
                },
                Width::Narrow
            )
            .is_none()
        );
        assert!(
            hollow_box(
                Cell {
                    width: 0,
                    height: 8
                },
                Width::Narrow
            )
            .is_none()
        );
    }
}
