//! Change tracking: what a display must redraw since it last drew.
//!
//! Damage is kept by screen position, not in cells or lines: a renderer
//! redraws positions, and a line that moves has changed at every position
//! it passes through. The scrollback therefore carries no ages, and a cell
//! costs none.
//!
//! Each mark stamps the current age. A display takes a [`Age`] from
//! [`Damage::checkpoint`] when it draws, and a position has changed for
//! it when its stamp is newer. Several displays each keep their own age,
//! so one display drawing never hides a change from another.
//!
//! What marks what (the screen applies the rule):
//!
//! - writing or erasing cells marks those cells;
//! - scrolling marks the rows that moved: inside margins, the region's
//!   rows only; the whole screen, every row;
//! - a resize, a palette change, reverse video (DECSCNM), switching
//!   between the main and alternate screens, and moving the view through
//!   the scrollback mark everything;
//! - an image placement that is added, moved, clipped or removed, or whose
//!   image changes, marks the cells under its old and its new box;
//! - moving the cursor marks nothing: the frame carries the cursor, and a
//!   renderer redraws the cells it left and the cells it covers;
//! - a pen change alone (SGR) marks nothing: no stored cell changed, and
//!   cells past a line's end read in the default style, never the pen.

use core::ops::Range;

/// A point in a screen's change history. [`Age::NEVER`] is older than
/// every mark, so a display that has never drawn sees everything changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Age(u64);

impl Age {
    pub const NEVER: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Default)]
struct RowAge {
    /// The whole row changed: it moved, or was cleared whole.
    whole: u64,
    /// The newest stamp of any one cell in the row.
    any: u64,
}

/// The change ages of one screen's positions.
#[derive(Clone, Debug)]
pub struct Damage {
    cols: usize,
    rows: usize,
    /// The stamp the next mark takes; always newer than every checkpoint.
    now: u64,
    all: u64,
    row_ages: Vec<RowAge>,
    cell_ages: Vec<u64>,
}

impl Damage {
    /// Damage for a `cols` by `rows` screen, everything changed.
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            now: 1,
            all: 1,
            row_ages: vec![RowAge::default(); rows],
            cell_ages: vec![0; cols.saturating_mul(rows)],
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The age a display keeps after drawing: later marks are newer.
    pub fn checkpoint(&mut self) -> Age {
        let age = Age(self.now);
        self.now = self.now.saturating_add(1);
        age
    }

    pub fn mark_cell(&mut self, row: usize, col: usize) {
        self.mark_cells(row, col..col.saturating_add(1));
    }

    /// Marks the cells of `cols` in `row`, clipped to the screen.
    pub fn mark_cells(&mut self, row: usize, cols: Range<usize>) {
        let end = cols.end.min(self.cols);
        if cols.start >= end {
            return;
        }
        let Some(base) = row.checked_mul(self.cols) else {
            return;
        };
        let (Some(start), Some(stop)) = (base.checked_add(cols.start), base.checked_add(end))
        else {
            return;
        };
        let (Some(cells), Some(r)) = (
            self.cell_ages.get_mut(start..stop),
            self.row_ages.get_mut(row),
        ) else {
            return;
        };
        cells.fill(self.now);
        r.any = self.now;
    }

    /// Marks every cell of the rows in `rows`, clipped to the screen.
    pub fn mark_rows(&mut self, rows: Range<usize>) {
        let end = rows.end.min(self.rows);
        if let Some(r) = self.row_ages.get_mut(rows.start..end) {
            for row in r {
                row.whole = self.now;
            }
        }
    }

    /// Marks the cells of a box, as an image placement covers them.
    pub fn mark_rect(&mut self, rows: Range<usize>, cols: Range<usize>) {
        for row in rows.start..rows.end.min(self.rows) {
            self.mark_cells(row, cols.clone());
        }
    }

    pub fn mark_all(&mut self) {
        self.all = self.now;
    }

    /// Resizes to `cols` by `rows` and marks everything.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.row_ages = vec![RowAge::default(); rows];
        self.cell_ages = vec![0; cols.saturating_mul(rows)];
        self.mark_all();
    }

    /// The bytes the ages hold on the heap, by capacity.
    pub fn heap_bytes(&self) -> usize {
        let rows = self.row_ages.capacity().saturating_mul(size_of::<RowAge>());
        let cells = self.cell_ages.capacity().saturating_mul(size_of::<u64>());
        rows.saturating_add(cells)
    }

    /// Whether the cell at `row`, `col` changed after `since`.
    pub fn changed(&self, row: usize, col: usize, since: Age) -> bool {
        let Some(r) = self.row_ages.get(row) else {
            return false;
        };
        let cell = row
            .checked_mul(self.cols)
            .and_then(|i| i.checked_add(col))
            .filter(|_| col < self.cols)
            .and_then(|i| self.cell_ages.get(i))
            .copied()
            .unwrap_or(0);
        self.all.max(r.whole).max(cell) > since.0
    }

    /// Whether any cell of `row` changed after `since`.
    pub fn row_changed(&self, row: usize, since: Age) -> bool {
        self.row_ages
            .get(row)
            .is_some_and(|r| self.all.max(r.whole).max(r.any) > since.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed_cells(d: &Damage, since: Age) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for row in 0..d.rows() {
            for col in 0..d.cols() {
                if d.changed(row, col, since) {
                    out.push((row, col));
                }
            }
        }
        out
    }

    #[test]
    fn a_display_that_never_drew_sees_everything() {
        let d = Damage::new(4, 3);
        assert_eq!(changed_cells(&d, Age::NEVER).len(), 12);
    }

    #[test]
    fn after_a_checkpoint_nothing_has_changed() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        assert!(changed_cells(&d, seen).is_empty());
        assert!((0..3).all(|r| !d.row_changed(r, seen)));
    }

    #[test]
    fn a_write_marks_its_cells_only() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        d.mark_cells(1, 1..3);
        assert_eq!(changed_cells(&d, seen), vec![(1, 1), (1, 2)]);
        assert!(d.row_changed(1, seen));
        assert!(!d.row_changed(0, seen));
    }

    // libtsm#34: a region scroll flagged every cell. It marks the rows
    // that moved, and the rows outside the region stay unchanged.
    #[test]
    fn a_region_scroll_marks_only_the_region() {
        let mut d = Damage::new(4, 6);
        let seen = d.checkpoint();
        d.mark_rows(2..5);
        let changed = changed_cells(&d, seen);
        assert_eq!(changed.len(), 12);
        assert!(changed.iter().all(|&(r, _)| (2..5).contains(&r)));
        assert!(!d.row_changed(0, seen));
        assert!(!d.row_changed(5, seen));
    }

    #[test]
    fn a_placement_marks_its_box() {
        let mut d = Damage::new(6, 4);
        let seen = d.checkpoint();
        d.mark_rect(1..3, 2..4);
        assert_eq!(
            changed_cells(&d, seen),
            vec![(1, 2), (1, 3), (2, 2), (2, 3)]
        );
    }

    #[test]
    fn marks_clip_to_the_screen() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        d.mark_cells(1, 3..100);
        d.mark_cells(7, 0..4);
        d.mark_rows(2..100);
        d.mark_rect(2..9, 0..1);
        d.mark_cell(usize::MAX, usize::MAX);
        assert_eq!(
            changed_cells(&d, seen),
            vec![(1, 3), (2, 0), (2, 1), (2, 2), (2, 3)]
        );
        assert!(!d.changed(9, 9, Age::NEVER));
    }

    #[test]
    fn a_resize_marks_everything() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        d.resize(5, 2);
        assert_eq!(changed_cells(&d, seen).len(), 10);
    }

    #[test]
    fn each_display_keeps_its_own_age() {
        let mut d = Damage::new(4, 3);
        let a = d.checkpoint();
        d.mark_cell(0, 0);
        let b = d.checkpoint();
        d.mark_cell(2, 3);
        assert_eq!(changed_cells(&d, a), vec![(0, 0), (2, 3)]);
        assert_eq!(changed_cells(&d, b), vec![(2, 3)]);
    }
}
