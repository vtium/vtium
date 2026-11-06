//! The renderer's history, cell by cell, and the damage rectangles a frame
//! hands the display.
//!
//! Each frame that draws something takes the next serial. Each cell keeps
//! the serial of the frame that last changed what it shows, the renderer's
//! own changes included: the cursor's place, shape and color, the blink
//! phase, a wide pairing, the pointer. A buffer lent at age n holds the
//! frame n serials back, so it lacks exactly the cells whose stamp is
//! newer; at age 0, or at an age older than the history, it lacks
//! everything. Nothing assumes the buffers take turns, and drawing one
//! buffer whole leaves the stamps, and so every other buffer's history, as
//! they were.
//!
//! Each cell also keeps the reach of the glyph it last drew (ADR-1306 D5):
//! a cell whose ink changes stamps the neighbours its old or new reach
//! covers, so each is composed again with the new ink or without the old.

use vtium_term::{CursorShape, Rgb};

use crate::reach::Reach;
use crate::target::Rect;

/// Damaged cells of a row at most this many cells apart share one damage
/// rectangle.
const DAMAGE_MERGE: u8 = 3;

/// The border areas the history keeps; a buffer older than the newest one
/// dropped redraws the whole border.
const BORDER_AREAS: usize = 16;

/// What the renderer last drew at a cell, for the changes the frame cannot
/// know, in 16 bytes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellState {
    /// The serial of the frame that last changed what the cell shows.
    stamp: u64,
    /// The cursor it drew: its shape and OSC 12's color.
    pub(crate) cursor: Option<(CursorShape, Option<Rgb>)>,
    /// How far the ink of the glyph it last drew reaches into the cells
    /// around it, as drawn: cut by the bound, the cursor and the blink.
    reach: Reach,
    /// The flags below, and the cells it drew over in [`SPAN`].
    bits: u8,
}

/// It drew in the text blink's off phase.
const BLINK_OFF: u8 = 1;
/// Drawn on this frame: in its damage.
const DRAWN: u8 = 1 << 1;
/// Its reach is known: read once the cell's glyph has been read since the
/// grid was reset.
const REACH_KNOWN: u8 = 1 << 2;
/// The cursor stood on it, visible, when its reach was read.
const REACH_CURSOR: u8 = 1 << 3;
/// The glyph its reach counts from spanned two cells.
const REACH_WIDE: u8 = 1 << 4;
/// Where the cells it drew over lie: 0 as a trailing half, 1 or 2.
const SPAN_SHIFT: u8 = 5;
const SPAN: u8 = 3 << SPAN_SHIFT;

impl CellState {
    fn flag(&self, f: u8) -> bool {
        self.bits & f != 0
    }

    fn set_flag(&mut self, f: u8, on: bool) {
        if on {
            self.bits |= f;
        } else {
            self.bits &= !f;
        }
    }

    /// The cells it drew over, 0 as a trailing half, 1 or 2, which `span`
    /// replaces; returns the old.
    pub(crate) fn replace_span(&mut self, span: u8) -> u8 {
        let old = (self.bits & SPAN) >> SPAN_SHIFT;
        self.bits = self.bits & !SPAN | (span.min(2) << SPAN_SHIFT);
        old
    }

    /// The cells it drew over.
    pub(crate) fn span(&self) -> u8 {
        (self.bits & SPAN) >> SPAN_SHIFT
    }

    /// Whether it drew in the text blink's off phase, which `off`
    /// replaces; returns the old.
    pub(crate) fn replace_blink_off(&mut self, off: bool) -> bool {
        let old = self.flag(BLINK_OFF);
        self.set_flag(BLINK_OFF, off);
        old
    }

    /// Records whether the visible cursor stands on the cell now, `held`,
    /// and returns whether its reach is to be read again: it was never
    /// read, or the cursor has come or gone since it was.
    pub(crate) fn reach_stale(&mut self, held: bool) -> bool {
        let stale = !self.flag(REACH_KNOWN) || self.flag(REACH_CURSOR) != held;
        self.set_flag(REACH_CURSOR, held);
        stale
    }

    /// How far the ink of the glyph it last drew reaches.
    pub(crate) fn reach(&self) -> Reach {
        self.reach
    }
}

/// The grid's cells, their history, and this frame's damage.
#[derive(Debug, Default)]
pub(crate) struct Cells {
    cols: usize,
    rows: usize,
    states: Vec<CellState>,
    /// Each row's newest stamp.
    row_stamps: Vec<u64>,
    /// How many of each row's cells send ink into another cell.
    row_reaching: Vec<u32>,
    /// The serial of the frame that last changed the whole border.
    border: u64,
    /// Areas of the border the pointer changed, with the serial of the
    /// frame that changed each, the newest last.
    border_areas: Vec<(u64, Rect)>,
    /// The newest serial among the areas dropped past [`BORDER_AREAS`].
    border_dropped: u64,
    /// This frame's serial.
    serial: u64,
    /// The serial of the frame the buffer being drawn holds; `None` when it
    /// is drawn whole.
    held: Option<u64>,
    damage: Vec<Rect>,
}

impl Cells {
    /// Starts the next frame, into a buffer `age` frames old.
    pub(crate) fn begin(&mut self, age: u32) {
        self.serial = self.serial.saturating_add(1);
        self.held = self
            .serial
            .checked_sub(u64::from(age))
            .filter(|&held| age > 0 && held > 0);
    }

    /// A grid of `cols` by `rows` whose every cell, and the border, changed
    /// on this frame.
    pub(crate) fn reset(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.states.clear();
        self.states.resize(
            cols.saturating_mul(rows),
            CellState {
                stamp: self.serial,
                ..CellState::default()
            },
        );
        self.row_stamps.clear();
        self.row_stamps.resize(rows, self.serial);
        self.row_reaching.clear();
        self.row_reaching.resize(rows, 0);
        self.border = self.serial;
        self.border_areas.clear();
    }

    fn index(&self, col: usize, row: usize) -> Option<usize> {
        if col >= self.cols {
            return None;
        }
        row.checked_mul(self.cols)?.checked_add(col)
    }

    pub(crate) fn get_mut(&mut self, col: usize, row: usize) -> Option<&mut CellState> {
        let i = self.index(col, row)?;
        self.states.get_mut(i)
    }

    pub(crate) fn get(&self, col: usize, row: usize) -> Option<&CellState> {
        let i = self.index(col, row)?;
        self.states.get(i)
    }

    /// The grid's columns and rows.
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Sets the reach of the cell at `col`, `row`, whose glyph spans
    /// `span` cells; returns the reach it had and the cells its glyph
    /// spanned.
    pub(crate) fn set_reach(
        &mut self,
        col: usize,
        row: usize,
        (reach, span): (Reach, u8),
    ) -> (Reach, u8) {
        let Some(s) = self.get_mut(col, row) else {
            return (Reach::NONE, 1);
        };
        let old = core::mem::replace(&mut s.reach, reach);
        let old_span = if s.flag(REACH_WIDE) { 2 } else { 1 };
        s.set_flag(REACH_WIDE, span == 2);
        s.set_flag(REACH_KNOWN, true);
        if let Some(n) = self.row_reaching.get_mut(row) {
            match (old.is_none(), reach.is_none()) {
                (true, false) => *n = n.saturating_add(1),
                (false, true) => *n = n.saturating_sub(1),
                _ => {}
            }
        }
        (old, old_span)
    }

    /// Forgets every cell's reach, so each is read again, and marks every
    /// cell changed on this frame.
    pub(crate) fn forget_reaches(&mut self) {
        let serial = self.serial;
        for s in &mut self.states {
            s.reach = Reach::NONE;
            s.set_flag(REACH_KNOWN | REACH_WIDE, false);
            s.stamp = serial;
        }
        self.row_reaching.fill(0);
        self.row_stamps.fill(serial);
    }

    /// Whether any cell of the rows from `first` to `last` sends ink into
    /// another cell.
    pub(crate) fn rows_reach(&self, first: usize, last: usize) -> bool {
        self.row_reaching
            .get(first..=last.min(self.rows.saturating_sub(1)))
            .is_some_and(|rows| rows.iter().any(|&n| n > 0))
    }

    /// Whether the buffer lacks something stamped `stamp`.
    fn stale(&self, stamp: u64) -> bool {
        self.held.is_none_or(|held| stamp > held)
    }

    /// The border's display area `r` changed on this frame.
    pub(crate) fn stamp_border_area(&mut self, r: Rect) {
        self.border_areas.push((self.serial, r));
        if self.border_areas.len() > BORDER_AREAS {
            let (serial, _) = self.border_areas.remove(0);
            self.border_dropped = self.border_dropped.max(serial);
        }
    }

    /// Whether the buffer lacks the whole border, or an area of it the
    /// history no longer keeps.
    pub(crate) fn border_stale(&self) -> bool {
        self.stale(self.border.max(self.border_dropped))
    }

    /// The areas of the border the buffer lacks, when it holds the rest.
    pub(crate) fn stale_border_areas(&self) -> impl Iterator<Item = Rect> + '_ {
        self.border_areas
            .iter()
            .filter(|&&(serial, _)| self.stale(serial))
            .map(|&(_, r)| r)
    }

    /// Whether the buffer may lack a cell of `row`.
    pub(crate) fn row_stale(&self, row: usize) -> bool {
        self.stale(self.row_stamps.get(row).copied().unwrap_or(u64::MAX))
    }

    /// The cell at `col`, `row` changed on this frame.
    pub(crate) fn stamp(&mut self, col: usize, row: usize) {
        let serial = self.serial;
        if let Some(s) = self.get_mut(col, row) {
            s.stamp = serial;
        }
        if let Some(r) = self.row_stamps.get_mut(row) {
            *r = serial;
        }
    }

    /// Every cell of the block from column `c0` and row `r0` to column
    /// `c1` and row `r1` that lies in the grid changed on this frame.
    pub(crate) fn stamp_block(&mut self, (c0, r0): (i64, i64), (c1, r1): (i64, i64)) {
        let clamp = |v: i64, n: usize| usize::try_from(v.max(0)).unwrap_or(usize::MAX).min(n);
        let (c0, c1) = (clamp(c0, self.cols), clamp(c1.saturating_add(1), self.cols));
        let (r0, r1) = (clamp(r0, self.rows), clamp(r1.saturating_add(1), self.rows));
        for row in r0..r1 {
            for col in c0..c1 {
                self.stamp(col, row);
            }
        }
    }

    /// Whether the buffer lacks any of `n` cells from `col` in `row`.
    pub(crate) fn stale_cells(&self, col: usize, row: usize, n: usize) -> bool {
        (col..col.saturating_add(n))
            .filter_map(|c| self.index(c, row).and_then(|i| self.states.get(i)))
            .any(|s| self.stale(s.stamp))
    }

    /// `n` cells from `col` in `row` are drawn on this frame; returns
    /// whether the first was not drawn on it before.
    pub(crate) fn mark_drawn(&mut self, col: usize, row: usize, n: usize) -> bool {
        let mut new = false;
        for c in col..col.saturating_add(n) {
            if let Some(s) = self.get_mut(c, row) {
                new |= c == col && !s.flag(DRAWN);
                s.set_flag(DRAWN, true);
            }
        }
        new
    }

    /// Clears the drawn marks into this frame's damage: `whole` after a
    /// border fill, else each row's drawn cells, those at most
    /// `DAMAGE_MERGE` cells apart in one rectangle, and the `border` areas
    /// drawn. `rect` gives a cell's display rectangle. A frame with no
    /// damage drew nothing and does not count: the next frame takes its
    /// serial.
    pub(crate) fn collect(
        &mut self,
        whole: Option<Rect>,
        border: &[Rect],
        rect: impl Fn(usize, usize) -> Rect,
    ) -> &[Rect] {
        self.damage.clear();
        for row in 0..self.rows {
            let mut gap = 0u8;
            for col in 0..self.cols {
                let Some(s) = self.get_mut(col, row) else {
                    continue;
                };
                if !s.flag(DRAWN) {
                    gap = gap.saturating_sub(1);
                    continue;
                }
                s.set_flag(DRAWN, false);
                if whole.is_some() {
                    continue;
                }
                let r = rect(col, row);
                match self.damage.last_mut() {
                    Some(last) if gap > 0 => *last = union(*last, r),
                    _ => self.damage.push(r),
                }
                gap = DAMAGE_MERGE;
            }
        }
        if let Some(w) = whole {
            self.damage.clear();
            self.damage.push(w);
        } else {
            self.damage.extend_from_slice(border);
        }
        if self.damage.is_empty() {
            self.serial = self.serial.saturating_sub(1);
        }
        &self.damage
    }
}

impl Cells {
    /// Holds `damage` as the frame's damage, which the GPU path's present
    /// gives, and gives it back.
    #[cfg(feature = "gpu")]
    pub(crate) fn set_damage(&mut self, damage: Vec<Rect>) -> &[Rect] {
        self.damage = damage;
        &self.damage
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    Rect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-1306 D4: each cell keeps its reach in one byte, and its flags
    // share one more, so the history of a cell takes the 16 bytes it took
    // before (8,160 cells at 1366x768 in 8x16 cells: 130,560 bytes)
    #[test]
    fn a_cells_history_keeps_its_reach_in_its_old_size() {
        assert_eq!(core::mem::size_of::<Reach>(), 1);
        assert_eq!(core::mem::size_of::<CellState>(), 16);
        let mut s = CellState::default();
        assert_eq!(s.replace_span(2), 0);
        assert!(!s.replace_blink_off(true));
        assert_eq!((s.span(), s.replace_blink_off(false)), (2, true));
        assert_eq!(s.replace_span(1), 2);
    }
}
