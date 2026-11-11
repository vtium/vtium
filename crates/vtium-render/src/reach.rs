//! How far a glyph's ink reaches past its own cells (ADR-1302 D1, ADR-1306
//! D1, D5 and D12).
//!
//! A glyph from an outline font may draw ink past its own cells: up to
//! [`Bound::SIDE`] cells to the left and to the right, and up to
//! [`Bound::ROWS`] rows above and below. The block of cells around a glyph
//! that its ink may draw in is its reach. Each cell remembers the reach of
//! the glyph it last drew in one byte, [`Reach`], so that a change to the
//! glyph damages the neighbours its old or new ink lies in, and so that a
//! cell being drawn finds the neighbours whose ink it receives by reading
//! their bytes.
//!
//! Everything that depends on the bound reads it from [`Bound`], which
//! takes the two constants of the font crate, so that widening the bound
//! is a change of those constants.

use vtium_font::{Glyph, Ink};

/// The bound on a glyph's reach: how many cells to each side and how many
/// rows above and below its ink may draw in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bound {
    /// Cells to the left and to the right of the glyph's own.
    pub(crate) side: u8,
    /// Rows above and below the glyph's own.
    pub(crate) rows: u8,
}

impl Bound {
    /// The bound to each side, from the font crate's constant.
    pub(crate) const SIDE: u8 = clamp(vtium_font::REACH_CELLS);

    /// The bound above and below, from the font crate's constant.
    pub(crate) const ROWS: u8 = clamp(vtium_font::REACH_ROWS);

    /// The bound vtium draws with.
    pub(crate) const DRAWN: Bound = Bound {
        side: Bound::SIDE,
        rows: Bound::ROWS,
    };
}

/// A bound in cells as a reach count holds it: at most [`Reach::MAX`].
const fn clamp(n: u32) -> u8 {
    match n {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => Reach::MAX,
    }
}

/// How many cells a glyph's ink reaches to the left, to the right, up and
/// down, two bits each in one byte (ADR-1306 D5, D12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reach(u8);

impl Reach {
    /// The most one count holds.
    pub(crate) const MAX: u8 = 3;

    /// Ink that stays in its own cells.
    pub(crate) const NONE: Reach = Reach(0);

    /// The counts, each cut to [`Reach::MAX`].
    pub(crate) fn new(left: u8, right: u8, up: u8, down: u8) -> Reach {
        let c = |n: u8| n.min(Reach::MAX);
        Reach(c(left) | c(right) << 2 | c(up) << 4 | c(down) << 6)
    }

    pub(crate) fn left(self) -> u8 {
        self.0 & 3
    }

    pub(crate) fn right(self) -> u8 {
        self.0 >> 2 & 3
    }

    pub(crate) fn up(self) -> u8 {
        self.0 >> 4 & 3
    }

    pub(crate) fn down(self) -> u8 {
        self.0 >> 6 & 3
    }

    /// The byte that holds the four counts: left in its lowest two bits,
    /// then right, up and down.
    #[cfg(feature = "gpu")]
    pub(crate) fn byte(self) -> u8 {
        self.0
    }

    /// The reach whose counts `byte` holds, as [`Reach::byte`] gives them.
    #[cfg(feature = "gpu")]
    pub(crate) fn from_byte(byte: u8) -> Reach {
        Reach(byte)
    }

    /// Whether the ink stays in its own cells.
    pub(crate) fn is_none(self) -> bool {
        self.0 == 0
    }

    /// The counts cut to `bound`: ink past it is cut.
    pub(crate) fn within(self, bound: Bound) -> Reach {
        Reach::new(
            self.left().min(bound.side),
            self.right().min(bound.side),
            self.up().min(bound.rows),
            self.down().min(bound.rows),
        )
    }

    /// The counts above and below alone: the letter under the cursor keeps
    /// its ink beside its cells inside them (ADR-1302 D8, ADR-1306 D6).
    pub(crate) fn upright(self) -> Reach {
        Reach::new(0, 0, self.up(), self.down())
    }

    /// The block of cells it covers for a glyph whose first cell is at
    /// `col`, `row` and which spans `span` cells: the first column and row
    /// and the last, which may lie past the grid's edges.
    pub(crate) fn block(self, col: usize, row: usize, span: usize) -> Block {
        let (col, row) = (signed(col), signed(row));
        let last = col.saturating_add(signed(span.max(1))).saturating_sub(1);
        Block {
            c0: col.saturating_sub(i64::from(self.left())),
            r0: row.saturating_sub(i64::from(self.up())),
            c1: last.saturating_add(i64::from(self.right())),
            r1: row.saturating_add(i64::from(self.down())),
        }
    }
}

fn signed(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A block of cells, from column `c0` and row `r0` to column `c1` and row
/// `r1`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) c0: i64,
    pub(crate) r0: i64,
    pub(crate) c1: i64,
    pub(crate) r1: i64,
}

impl Block {
    /// Whether it holds any of `n` cells from `col` in `row`.
    pub(crate) fn meets(&self, col: usize, row: usize, n: usize) -> bool {
        let (col, row) = (signed(col), signed(row));
        let last = col.saturating_add(signed(n.max(1))).saturating_sub(1);
        row >= self.r0 && row <= self.r1 && last >= self.c0 && col <= self.c1
    }
}

/// A glyph's ink box as the renderer reads it (ADR-1306 D3): `width` by
/// `height` pixels from `left`, `top`, offsets from the top left of the
/// glyph's first cell, 0 or negative, in the glyph's format.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InkBox<'a> {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: &'a [u8],
}

/// The ink box of `glyph`, when its ink passes its cells. This is the one
/// place the renderer reads the font's box.
pub(crate) fn ink_box(glyph: &Glyph) -> Option<InkBox<'_>> {
    glyph.ink().map(|ink: &Ink| InkBox {
        left: ink.left(),
        top: ink.top(),
        width: ink.width(),
        height: ink.height(),
        pixels: ink.pixels(),
    })
}

/// How many cells a box reaches past a glyph's cells on each side: the
/// box is `left`, `top`, `width` by `height` pixels against a glyph
/// `glyph_w` by `glyph_h` pixels that spans `cells` cells, all at one
/// size.
pub(crate) fn of_box(
    (left, top, width, height): (i64, i64, i64, i64),
    (glyph_w, glyph_h): (i64, i64),
    cells: i64,
) -> Reach {
    let cw = glyph_w.checked_div(cells.max(1)).unwrap_or(0).max(1);
    let ch = glyph_h.max(1);
    // the cells past an edge that `past` pixels reach into
    let count = |past: i64, side: i64| {
        let n = past
            .max(0)
            .saturating_add(side.saturating_sub(1))
            .checked_div(side)
            .unwrap_or(0);
        u8::try_from(n.min(i64::from(Reach::MAX))).unwrap_or(Reach::MAX)
    };
    Reach::new(
        count(left.saturating_neg(), cw),
        count(left.saturating_add(width).saturating_sub(glyph_w), cw),
        count(top.saturating_neg(), ch),
        count(top.saturating_add(height).saturating_sub(glyph_h), ch),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reach_keeps_four_counts_in_one_byte() {
        let r = Reach::new(1, 2, 3, 0);
        assert_eq!((r.left(), r.right(), r.up(), r.down()), (1, 2, 3, 0));
        assert_eq!(Reach::new(9, 9, 9, 9), Reach::new(3, 3, 3, 3));
        assert_eq!(core::mem::size_of::<Reach>(), 1);
        assert!(Reach::NONE.is_none());
        assert_eq!(r.upright(), Reach::new(0, 0, 3, 0));
        let b = Bound { side: 1, rows: 1 };
        assert_eq!(r.within(b), Reach::new(1, 1, 1, 0));
    }

    #[test]
    fn the_bound_is_the_font_crates_constants() {
        assert_eq!(u32::from(Bound::SIDE), vtium_font::REACH_CELLS);
        assert_eq!(u32::from(Bound::ROWS), vtium_font::REACH_ROWS);
    }

    #[test]
    fn a_block_spans_the_glyphs_cells_and_its_reach() {
        let b = Reach::new(1, 1, 1, 0).block(5, 2, 2);
        assert_eq!(
            b,
            Block {
                c0: 4,
                r0: 1,
                c1: 7,
                r1: 2
            }
        );
        assert!(b.meets(7, 1, 1));
        assert!(b.meets(3, 2, 2));
        assert!(!b.meets(8, 1, 1));
        assert!(!b.meets(5, 3, 1));
        // the corner is in the block whenever both counts reach it
        assert!(b.meets(4, 1, 1));
    }

    #[test]
    fn a_box_reaches_each_cell_it_passes_into() {
        // a glyph 8 by 16 in one cell
        let g = (8, 16);
        assert_eq!(of_box((0, 0, 8, 16), g, 1), Reach::NONE);
        assert_eq!(of_box((0, -2, 8, 18), g, 1), Reach::new(0, 0, 1, 0));
        assert_eq!(of_box((-1, 0, 10, 16), g, 1), Reach::new(1, 1, 0, 0));
        assert_eq!(of_box((0, 0, 8, 17), g, 1), Reach::new(0, 0, 0, 1));
        assert_eq!(of_box((-9, -17, 26, 34), g, 1), Reach::new(2, 2, 2, 1));
        // a wide glyph's cells are half its width
        assert_eq!(of_box((-3, 0, 19, 16), (16, 16), 2), Reach::new(1, 0, 0, 0));
    }
}
