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
//!   between the main and alternate screens, and a move of the view
//!   through the scrollback that the scroll record cannot state mark
//!   everything;
//! - an image placement that is added, moved, clipped or removed, or whose
//!   image changes, marks the cells under its old and its new box;
//! - moving the cursor marks nothing: the frame carries the cursor, and a
//!   renderer redraws the cells it left and the cells it covers;
//! - a pen change alone (SGR) marks nothing: no stored cell changed, and
//!   cells past a line's end read in the default style, never the pen.
//!
//! # The scroll record
//!
//! Beside the marks, the damage keeps a scroll record: a signed count of
//! the rows the whole view moved, up counting positive. A display that
//! kept the pixels it drew can move them by the count and then redraw
//! only what changed in place, which [`Damage::changed_in_place`] and
//! [`Damage::row_changed_in_place`] tell it. [`Damage::scrolled`] gives
//! the count since a display's own age.
//!
//! The ages are kept for the content, not for the position: the rows'
//! and the cells' ages sit in a ring whose start moves with a scroll of
//! the whole view, so a cell written on row 66 before a line feed keeps
//! its age on row 65 after it. The rows that come into view are stamped
//! as changed in place. A scroll of the whole view also stamps one age
//! for every row, which [`Damage::changed`] and [`Damage::row_changed`]
//! read, so a renderer that ignores the record still redraws every row
//! that moved. Moving the ring's start costs nothing per row, where
//! moving every cell's age would cost a copy of the whole table on each
//! line feed.
//!
//! References:
//! - ADR-1310 D2, "A scroll record in every build": what the record counts,
//!   in both directions, and that it adds to the damage and replaces none
//!   of it.

use core::ops::Range;

use crate::bounds::{COLUMNS_MAX, ROWS_MAX};

/// A point in a screen's change history. [`Age::NEVER`] is older than
/// every mark, so a display that has never drawn sees everything changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Age(u64);

impl Age {
    pub const NEVER: Self = Self(0);
}

/// The checkpoints the scroll record remembers the count at. A checkpoint
/// takes an entry only when the count changed since the previous entry,
/// so checkpoints taken while nothing scrolls cost none. 32 entries hold
/// the ages of several displays with three buffers each through a steady
/// stream of short scrolls; an age older than the oldest entry reads no
/// count, and its display redraws every moved row, as without the record.
pub(crate) const SCROLL_LOG: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RowAge {
    /// The whole row changed in place: it was cleared whole, it came into
    /// view, or it moved in a scroll the record cannot state.
    whole: u64,
    /// The newest stamp of any one cell in the row.
    any: u64,
}

/// One entry of the scroll log: the count at a checkpoint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Logged {
    age: u64,
    count: i64,
}

/// The change ages of one screen's positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Damage {
    cols: usize,
    rows: usize,
    /// The stamp the next mark takes; always newer than every checkpoint.
    now: u64,
    all: u64,
    /// The stamp of the last scroll of the whole view, which every row
    /// reads as changed by position, though not in place.
    moved: u64,
    /// The ring slot that holds the ages of the view's first row.
    start: usize,
    /// The rows the whole view moved since the damage was made, up
    /// counting positive.
    count: i64,
    /// The count at the last checkpoints, oldest first from `log_head`.
    log: [Logged; SCROLL_LOG],
    log_head: usize,
    log_len: usize,
    row_ages: Vec<RowAge>,
    cell_ages: Vec<u64>,
}

impl Damage {
    /// Damage for a `cols` by `rows` screen, at most [`COLUMNS_MAX`] by
    /// [`ROWS_MAX`], everything changed.
    pub fn new(cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.min(COLUMNS_MAX), rows.min(ROWS_MAX));
        Self {
            cols,
            rows,
            now: 1,
            all: 1,
            moved: 0,
            start: 0,
            count: 0,
            log: [Logged::default(); SCROLL_LOG],
            log_head: 0,
            log_len: 0,
            row_ages: vec![RowAge::default(); rows],
            cell_ages: vec![0; cols.saturating_mul(rows)],
        }
    }

    /// The bytes [`new`](Self::new)`(cols, rows)` holds on the heap, as
    /// [`heap_bytes`](Self::heap_bytes) counts them; a resize holds the
    /// same for its new size. The scroll record holds nothing on the heap.
    pub const fn heap_bytes_for(cols: usize, rows: usize) -> usize {
        let cols = if cols < COLUMNS_MAX {
            cols
        } else {
            COLUMNS_MAX
        };
        let rows = if rows < ROWS_MAX { rows } else { ROWS_MAX };
        let row_ages = rows.saturating_mul(size_of::<RowAge>());
        row_ages.saturating_add(cols.saturating_mul(rows).saturating_mul(size_of::<u64>()))
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The age a display keeps after drawing: later marks are newer. The
    /// scroll log takes the count at this age when the count changed since
    /// its newest entry, dropping its oldest entry when it is full.
    pub fn checkpoint(&mut self) -> Age {
        let age = Age(self.now);
        let newest = self.log_len.checked_sub(1).and_then(|i| self.logged(i));
        if newest.is_none_or(|e| e.count != self.count) {
            let entry = Logged {
                age: self.now,
                count: self.count,
            };
            if self.log_len < SCROLL_LOG {
                let i = self.log_head.wrapping_add(self.log_len) % SCROLL_LOG;
                self.log_len = self.log_len.saturating_add(1);
                if let Some(slot) = self.log.get_mut(i) {
                    *slot = entry;
                }
            } else {
                if let Some(slot) = self.log.get_mut(self.log_head) {
                    *slot = entry;
                }
                self.log_head = self.log_head.wrapping_add(1) % SCROLL_LOG;
            }
        }
        self.now = self.now.saturating_add(1);
        age
    }

    /// The log's entry `i`, counted from its oldest.
    fn logged(&self, i: usize) -> Option<Logged> {
        if i >= self.log_len {
            return None;
        }
        self.log
            .get(self.log_head.wrapping_add(i) % SCROLL_LOG)
            .copied()
    }

    /// The ring slot that holds view row `row`'s ages.
    fn slot(&self, row: usize) -> usize {
        self.ring(self.start.wrapping_add(row))
    }

    /// `i` taken around the ring of rows; 0 for a screen with no rows. The
    /// ring's start and a row are each below the row count, so their sum
    /// never wraps.
    fn ring(&self, i: usize) -> usize {
        i.checked_rem(self.rows).unwrap_or(0)
    }

    pub fn mark_cell(&mut self, row: usize, col: usize) {
        self.mark_cells(row, col..col.saturating_add(1));
    }

    /// Marks the cells of `cols` in `row`, clipped to the screen.
    pub fn mark_cells(&mut self, row: usize, cols: Range<usize>) {
        let end = cols.end.min(self.cols);
        if cols.start >= end || row >= self.rows {
            return;
        }
        let slot = self.slot(row);
        let Some(base) = slot.checked_mul(self.cols) else {
            return;
        };
        let (Some(start), Some(stop)) = (base.checked_add(cols.start), base.checked_add(end))
        else {
            return;
        };
        let (Some(cells), Some(r)) = (
            self.cell_ages.get_mut(start..stop),
            self.row_ages.get_mut(slot),
        ) else {
            return;
        };
        cells.fill(self.now);
        r.any = self.now;
    }

    /// Marks every cell of the rows in `rows`, clipped to the screen.
    pub fn mark_rows(&mut self, rows: Range<usize>) {
        for row in rows.start..rows.end.min(self.rows) {
            let slot = self.slot(row);
            if let Some(r) = self.row_ages.get_mut(slot) {
                r.whole = self.now;
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

    /// The whole view moved `n` rows up, as a line feed on the bottom row
    /// or SU with the margins at the screen's edges moves it, or as the
    /// view moving toward newer output does. The count goes up by `n`,
    /// the ages move up with the rows, the `n` rows that come in at the
    /// bottom are changed in place, and every row is marked by position.
    pub fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.rows);
        if n == 0 {
            return;
        }
        self.start = self.ring(self.start.wrapping_add(n));
        self.scrolled_in(self.rows.saturating_sub(n)..self.rows, n, 1);
    }

    /// The whole view moved `n` rows down, as a reverse index on the top
    /// row or SD with the margins at the screen's edges moves it, or as
    /// the view moving back through the scrollback does. The count goes
    /// down by `n`, the ages move down with the rows, the `n` rows that
    /// come in at the top are changed in place, and every row is marked by
    /// position.
    pub fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.rows);
        if n == 0 {
            return;
        }
        self.start = self.ring(self.start.wrapping_add(self.rows.saturating_sub(n)));
        self.scrolled_in(0..n, n, -1);
    }

    fn scrolled_in(&mut self, rows: Range<usize>, n: usize, sign: i64) {
        let n = i64::try_from(n).unwrap_or(i64::MAX);
        self.count = self.count.saturating_add(n.saturating_mul(sign));
        self.moved = self.now;
        self.mark_rows(rows);
    }

    /// Resizes to `cols` by `rows`, at most [`COLUMNS_MAX`] by
    /// [`ROWS_MAX`], and marks everything.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.min(COLUMNS_MAX), rows.min(ROWS_MAX));
        self.cols = cols;
        self.rows = rows;
        self.start = 0;
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

    /// The rows the whole view moved after `since`, up counting positive
    /// and down negative, for a display that keeps what it drew at
    /// `since`. `None` when the record cannot say: the display never drew,
    /// its age is older than the scroll log's oldest entry, or everything
    /// changed after it, as a resize or a switch of screens marks it. The
    /// display then redraws what [`changed`](Self::changed) marks.
    pub fn scrolled(&self, since: Age) -> Option<isize> {
        if since == Age::NEVER || self.all > since.0 {
            return None;
        }
        let oldest = self.logged(0)?;
        if since.0 < oldest.age {
            return None;
        }
        let mut at = oldest.count;
        for i in 1..self.log_len {
            match self.logged(i) {
                Some(e) if e.age <= since.0 => at = e.count,
                _ => break,
            }
        }
        isize::try_from(self.count.saturating_sub(at)).ok()
    }

    /// The cell's own age and its row's, by the ring slot of view row
    /// `row`; `None` outside the screen.
    fn ages(&self, row: usize, col: usize) -> Option<(RowAge, u64)> {
        if row >= self.rows {
            return None;
        }
        let slot = self.slot(row);
        let r = *self.row_ages.get(slot)?;
        let cell = slot
            .checked_mul(self.cols)
            .and_then(|i| i.checked_add(col))
            .filter(|_| col < self.cols)
            .and_then(|i| self.cell_ages.get(i))
            .copied()
            .unwrap_or(0);
        Some((r, cell))
    }

    /// Whether the cell at `row`, `col` changed after `since`, by
    /// position: a cell that moved has changed.
    pub fn changed(&self, row: usize, col: usize, since: Age) -> bool {
        self.ages(row, col)
            .is_some_and(|(r, cell)| self.all.max(self.moved).max(r.whole).max(cell) > since.0)
    }

    /// Whether any cell of `row` changed after `since`, by position.
    pub fn row_changed(&self, row: usize, since: Age) -> bool {
        self.ages(row, 0)
            .is_some_and(|(r, _)| self.all.max(self.moved).max(r.whole).max(r.any) > since.0)
    }

    /// Whether the cell at `row`, `col` changed in place after `since`:
    /// once a display moved what it drew at `since` by
    /// [`scrolled`](Self::scrolled), the cell still differs.
    pub fn changed_in_place(&self, row: usize, col: usize, since: Age) -> bool {
        self.ages(row, col)
            .is_some_and(|(r, cell)| self.all.max(r.whole).max(cell) > since.0)
    }

    /// Whether any cell of `row` changed in place after `since`.
    pub fn row_changed_in_place(&self, row: usize, since: Age) -> bool {
        self.ages(row, 0)
            .is_some_and(|(r, _)| self.all.max(r.whole).max(r.any) > since.0)
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

    // A region scroll marks the rows that moved, and the rows outside the
    // region stay unchanged.
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
    fn sizes_clamp_to_the_bounds() {
        let mut d = Damage::new(usize::MAX, usize::MAX);
        assert_eq!((d.cols(), d.rows()), (COLUMNS_MAX, ROWS_MAX));
        d.resize(4, 3);
        d.resize(COLUMNS_MAX + 1, usize::MAX);
        assert_eq!((d.cols(), d.rows()), (COLUMNS_MAX, ROWS_MAX));
    }

    #[test]
    fn a_resize_marks_everything() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        d.resize(5, 2);
        assert_eq!(changed_cells(&d, seen).len(), 10);
    }

    fn changed_in_place(d: &Damage, since: Age) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for row in 0..d.rows() {
            for col in 0..d.cols() {
                if d.changed_in_place(row, col, since) {
                    out.push((row, col));
                }
            }
        }
        out
    }

    // A scroll of the whole view up by one moves every age up a row with
    // its line: a cell written after the display drew is still changed in
    // place one row higher, the row that came in is changed in place
    // whole, and by position every row is changed.
    #[test]
    fn a_scroll_up_moves_the_ages_with_the_rows() {
        let mut d = Damage::new(4, 3);
        d.checkpoint();
        d.mark_cell(2, 1);
        let seen = d.checkpoint();
        d.mark_cell(2, 3);
        d.scroll_up(1);
        assert_eq!(d.scrolled(seen), Some(1));
        assert_eq!(
            changed_in_place(&d, seen),
            vec![(1, 3), (2, 0), (2, 1), (2, 2), (2, 3)]
        );
        assert!(d.row_changed_in_place(1, seen));
        assert!(!d.row_changed_in_place(0, seen));
        assert_eq!(changed_cells(&d, seen).len(), 12);
    }

    // A scroll down counts down, and the row that came in at the top is
    // the one changed in place.
    #[test]
    fn a_scroll_down_counts_down() {
        let mut d = Damage::new(4, 3);
        let seen = d.checkpoint();
        d.mark_cell(0, 2);
        d.scroll_down(2);
        assert_eq!(d.scrolled(seen), Some(-2));
        let mut want: Vec<_> = (0..2).flat_map(|r| (0..4).map(move |c| (r, c))).collect();
        want.push((2, 2));
        assert_eq!(changed_in_place(&d, seen), want);
    }

    // Each display reads the count since its own age from the log.
    #[test]
    fn two_displays_read_their_own_counts() {
        let mut d = Damage::new(4, 5);
        let a = d.checkpoint();
        d.scroll_up(1);
        let b = d.checkpoint();
        d.scroll_up(2);
        d.scroll_down(1);
        assert_eq!(d.scrolled(a), Some(2));
        assert_eq!(d.scrolled(b), Some(1));
        let c = d.checkpoint();
        assert_eq!(d.scrolled(c), Some(0));
    }

    // The record says nothing to a display that never drew, nor after
    // everything was marked, as a resize marks it.
    #[test]
    fn the_record_is_silent_after_everything_changed() {
        let mut d = Damage::new(4, 3);
        assert_eq!(d.scrolled(Age::NEVER), None);
        let seen = d.checkpoint();
        d.scroll_up(1);
        d.mark_all();
        assert_eq!(d.scrolled(seen), None);
        assert_eq!(changed_in_place(&d, seen).len(), 12);
        let seen = d.checkpoint();
        d.resize(5, 2);
        assert_eq!(d.scrolled(seen), None);
    }

    // The log keeps the count at its last SCROLL_LOG checkpoints and no
    // more: an older age reads no count, however long the stream runs,
    // and the damage's size does not grow with it.
    #[test]
    fn the_scroll_log_is_bounded() {
        let mut d = Damage::new(4, 3);
        let first = d.checkpoint();
        let heap = d.heap_bytes();
        let mut ages = Vec::new();
        for _ in 0..10_000 {
            d.scroll_up(1);
            ages.push(d.checkpoint());
        }
        assert_eq!(d.scrolled(first), None);
        let kept = ages.len().saturating_sub(SCROLL_LOG);
        for (i, &age) in ages.iter().enumerate() {
            let newer = ages.len().saturating_sub(i).saturating_sub(1);
            let want = (i >= kept).then(|| isize::try_from(newer).unwrap_or(isize::MAX));
            assert_eq!(d.scrolled(age), want, "the age of checkpoint {i}");
        }
        assert_eq!(d.heap_bytes(), heap);
    }

    // Checkpoints while nothing scrolls take no entry, so they push no
    // older age out of the log.
    #[test]
    fn checkpoints_without_a_scroll_take_no_entry() {
        let mut d = Damage::new(4, 3);
        let a = d.checkpoint();
        d.scroll_up(1);
        for _ in 0..10_000 {
            d.mark_cell(0, 0);
            d.checkpoint();
        }
        assert_eq!(d.scrolled(a), Some(1));
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
