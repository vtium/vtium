//! The ink grid's tests: the line bits it sets for known boxes and
//! layouts, and where they lie in its texture.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]

use super::*;

const CELL: Size = Size::new(8, 16);
const BOUND: Bound = Bound { side: 1, rows: 1 };

fn job(col: usize, row: usize, cells: usize, down: bool) -> ReceivingJob {
    ReceivingJob {
        col,
        row,
        cells,
        down,
    }
}

impl InkGrid {
    /// The line bits of the cell at `col`, `row`, as the lines set.
    fn lines_set(&self, col: usize, row: usize) -> Vec<usize> {
        let at = self.index(col, row).unwrap() * self.words;
        (0..self.words * BITS)
            .filter(|&n| self.lines[at + n / BITS] >> (n % BITS) & 1 != 0)
            .collect()
    }
}

/// The box of a glyph whose first cell is at column 2 of row 1, in a
/// slot of four cells by three rows, its own cell the second of the
/// middle row: an accent in its own column from line 13 to 15 of the
/// row above, and a stroke from line 10 to 11 of the cell above and to
/// the right of it, its columns 0 and 1.
fn accent() -> BoxLines {
    let slot = Size::new(4 * 8, 3 * 16);
    let mut px = vec![0u8; 32 * 48];
    for y in 13..16 {
        for x in 8 + 3..8 + 5 {
            px[y * 32 + x] = 200;
        }
    }
    for y in 10..12 {
        for x in 16..18 {
            px[y * 32 + x] = 90;
        }
    }
    // ink inside the glyph's own cell sets no line of another
    px[(16 + 5) * 32 + 8 + 4] = 255;
    BoxLines::new(&px, slot, CELL)
}

/// A grid of 5 by 3 cells whose glyph at column 2 of row 1 sends ink one
/// row up and one cell to the right, its slot 7, settled.
fn grid() -> InkGrid {
    let mut g = InkGrid::default();
    g.begin(5, 3, CELL);
    g.name((2, 1), 7, Rgb::new(1, 2, 3));
    g.settle(|k, r| match (k, r) {
        (2, 1) => (Reach::new(0, 1, 1, 0), 1),
        _ => (Reach::NONE, 0),
    });
    g
}

// ADR-1306 D9: a job's line bits name the display lines its neighbours'
// ink reaches, across its row, and no others.
#[test]
fn the_lines_a_neighbours_ink_reaches_are_set_and_no_others() {
    let boxes = accent();
    let lines = |slot: u32| (slot == 7).then_some(&boxes);
    let mut g = grid();
    g.mark(job(2, 0, 1, false), (BOUND, CELL), &lines);
    assert_eq!(g.lines_set(2, 0), [13, 14, 15], "above the glyph");
    g.mark(job(3, 0, 1, false), (BOUND, CELL), &lines);
    assert_eq!(g.lines_set(3, 0), [10, 11], "above and to the right");
    g.mark(job(1, 0, 1, false), (BOUND, CELL), &lines);
    assert!(g.lines_set(1, 0).is_empty(), "left of the reach");
    g.mark(job(3, 1, 1, false), (BOUND, CELL), &lines);
    assert!(g.lines_set(3, 1).is_empty(), "beside it, where no ink lies");
    // a job of two cells: a line any of its cells receives ink on, in
    // each of its cells
    g.mark(job(2, 0, 2, false), (BOUND, CELL), &lines);
    assert_eq!(g.lines_set(2, 0), [10, 11, 13, 14, 15]);
    assert_eq!(g.lines_set(3, 0), [10, 11, 13, 14, 15]);
}

// ADR-1306 D9: on a display turned a quarter, a display line runs down
// a column of the upright frame, and each cell holds the bits of its
// own columns.
#[test]
fn turned_a_quarter_each_cell_names_its_own_columns() {
    let boxes = accent();
    let lines = |slot: u32| (slot == 7).then_some(&boxes);
    let mut g = grid();
    g.mark(job(2, 0, 2, true), (BOUND, CELL), &lines);
    assert_eq!(g.lines_set(2, 0), [3, 4]);
    assert_eq!(g.lines_set(3, 0), [0, 1]);
}

// A glyph whose reach names no row above sends no ink there, whatever
// its box holds; nor does a neighbour the frame emptied.
#[test]
fn the_reach_and_the_named_entries_bound_the_lines() {
    let boxes = accent();
    let lines = |slot: u32| (slot == 7).then_some(&boxes);
    let mut g = InkGrid::default();
    g.begin(5, 3, CELL);
    g.name((2, 1), 7, Rgb::new(1, 2, 3));
    g.settle(|_, _| (Reach::new(0, 1, 0, 1), 1));
    g.mark(job(2, 0, 1, false), (BOUND, CELL), &lines);
    assert!(g.lines_set(2, 0).is_empty(), "no reach up");
    let mut g = grid();
    g.begin(5, 3, CELL);
    g.settle(|_, _| (Reach::new(0, 1, 1, 0), 1));
    g.mark(job(2, 0, 1, false), (BOUND, CELL), &lines);
    assert!(g.lines_set(2, 0).is_empty(), "an entry the frame emptied");
}

// The texture holds each cell's line bits after its two texels, the
// first line in the lowest bit of the red channel.
#[test]
fn the_line_bits_lie_after_the_source_texels() {
    use crate::gpu::StandIn;
    let boxes = accent();
    let lines = |slot: u32| (slot == 7).then_some(&boxes);
    let mut g = grid();
    g.mark(job(2, 0, 1, false), (BOUND, CELL), &lines);
    let mut gl = StandIn::new();
    g.upload(&mut gl).unwrap();
    assert!(g.texture().is_some());
    // the stand-in's first object, the grid's texture
    let (format, size, px) = gl.texture_pixels(1).unwrap();
    assert_eq!(format, Format::Rgba8);
    // three texels a cell: two, and one for 16 lines
    assert_eq!(size, Size::new(5, 9));
    let texel = |k: usize, y: usize| &px[(y * 5 + k) * 4..(y * 5 + k) * 4 + 4];
    assert_eq!(texel(2, 2), [0, 0xe0, 0, 0], "lines 13, 14 and 15");
    assert_eq!(
        texel(2, 3),
        [0x14, 1, 7, 0],
        "the source's reach, span, slot"
    );
    assert_eq!(texel(2, 4), [1, 2, 3, 0], "its color");
    assert_eq!(texel(2, 5), [0; 4], "its line bits");
}

/// The box of a wide glyph whose first cell is at column 1 of row 0, in
/// a slot of four cells by three rows, its two cells the second and
/// third of the middle row: a tail in the slot's last column of cells
/// and its last row, the cell below and to the right of the glyph's,
/// from line 2 to 4, its columns 6 and 7.
fn wide_tail() -> BoxLines {
    let slot = Size::new(4 * 8, 3 * 16);
    let mut px = vec![0u8; 32 * 48];
    for y in 32 + 2..32 + 5 {
        for x in 24 + 6..24 + 8 {
            px[y * 32 + x] = 180;
        }
    }
    BoxLines::new(&px, slot, CELL)
}

// ADR-1306 D9: a wide glyph above and to the left of a cell sends its
// ink there from the slot's last column of cells. The cell lies past
// the glyph's two cells, so the neighbours read for it start one cell
// further left than the bound alone.
#[test]
fn a_wide_glyphs_ink_below_and_past_its_cells_sets_the_lines_it_reaches() {
    let boxes = wide_tail();
    let lines = |slot: u32| (slot == 3).then_some(&boxes);
    for (down, want) in [(false, [2, 3, 4].as_slice()), (true, &[6, 7])] {
        let mut g = InkGrid::default();
        g.begin(5, 3, CELL);
        g.name((1, 0), 3, Rgb::new(1, 2, 3));
        g.settle(|k, r| match (k, r) {
            (1, 0) => (Reach::new(0, 1, 0, 1), 2),
            _ => (Reach::NONE, 0),
        });
        g.mark(job(3, 1, 1, down), (BOUND, CELL), &lines);
        assert_eq!(g.lines_set(3, 1), want, "down {down}");
    }
}

// A cell whose job received ink on the last frame and receives none on
// this one, or is not drawn, has its line bits cleared, so no job that
// reads them later finds the last frame's lines.
#[test]
fn a_cell_no_job_marks_this_frame_has_its_lines_cleared() {
    let boxes = accent();
    let lines = |slot: u32| (slot == 7).then_some(&boxes);
    let mut g = grid();
    g.mark_jobs([job(2, 0, 2, false)].into_iter(), (BOUND, CELL), lines);
    assert_eq!(g.lines_set(3, 0), [10, 11, 13, 14, 15]);
    g.begin(5, 3, CELL);
    g.name((2, 1), 7, Rgb::new(1, 2, 3));
    g.settle(|k, r| match (k, r) {
        (2, 1) => (Reach::new(0, 1, 1, 0), 1),
        _ => (Reach::NONE, 0),
    });
    g.mark_jobs([job(2, 0, 1, false)].into_iter(), (BOUND, CELL), lines);
    assert_eq!(g.lines_set(2, 0), [13, 14, 15]);
    assert!(g.lines_set(3, 0).is_empty(), "a cell no job marked");
}
