//! The renderer's history, cell by cell, and the damage rectangles a frame
//! hands the display.
//!
//! Each frame that draws something takes the next serial. Each cell keeps
//! the serial of the frame that last changed what it shows, the renderer's
//! own changes included: the cursor's place, shape and colour, the blink
//! phase, a wide pairing, the pointer. A buffer lent at age n holds the
//! frame n serials back, so it lacks exactly the cells whose stamp is
//! newer; at age 0, or at an age older than the history, it lacks
//! everything. Nothing assumes the buffers take turns, and drawing one
//! buffer whole leaves the stamps, and so every other buffer's history, as
//! they were.

use vtium_term::{CursorShape, Rgb};

use crate::target::Rect;

/// Damaged cells of a row at most this many cells apart share one damage
/// rectangle.
const DAMAGE_MERGE: u8 = 3;

/// What the renderer last drew at a cell, for the changes the frame cannot
/// know.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellState {
    /// The serial of the frame that last changed what the cell shows.
    stamp: u64,
    /// The cursor it drew: its shape and OSC 12's colour.
    pub(crate) cursor: Option<(CursorShape, Option<Rgb>)>,
    pub(crate) hidden: bool,
    /// The cells it drew over: 0 as a trailing half, 1 or 2.
    pub(crate) span: u8,
    /// Drawn on this frame: in its damage.
    drawn: bool,
}

/// The grid's cells, their history, and this frame's damage.
#[derive(Debug, Default)]
pub(crate) struct Cells {
    cols: usize,
    rows: usize,
    states: Vec<CellState>,
    /// Each row's newest stamp.
    row_stamps: Vec<u64>,
    /// The serial of the frame that last changed the border.
    border: u64,
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
        self.border = self.serial;
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

    /// Whether the buffer lacks something stamped `stamp`.
    fn stale(&self, stamp: u64) -> bool {
        self.held.is_none_or(|held| stamp > held)
    }

    /// The border changed on this frame.
    pub(crate) fn stamp_border(&mut self) {
        self.border = self.serial;
    }

    /// Whether the buffer lacks the border.
    pub(crate) fn border_stale(&self) -> bool {
        self.stale(self.border)
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

    /// Whether the buffer lacks any of `n` cells from `col` in `row`.
    pub(crate) fn stale_cells(&self, col: usize, row: usize, n: usize) -> bool {
        (col..col.saturating_add(n))
            .filter_map(|c| self.index(c, row).and_then(|i| self.states.get(i)))
            .any(|s| self.stale(s.stamp))
    }

    /// `n` cells from `col` in `row` are drawn on this frame.
    pub(crate) fn mark_drawn(&mut self, col: usize, row: usize, n: usize) {
        for c in col..col.saturating_add(n) {
            if let Some(s) = self.get_mut(c, row) {
                s.drawn = true;
            }
        }
    }

    /// Clears the drawn marks into this frame's damage: `whole` after a
    /// border fill, else each row's drawn cells, those at most
    /// `DAMAGE_MERGE` cells apart in one rectangle. `rect` gives a cell's
    /// display rectangle. A frame with no damage drew nothing and does not
    /// count: the next frame takes its serial.
    pub(crate) fn collect(
        &mut self,
        whole: Option<Rect>,
        rect: impl Fn(usize, usize) -> Rect,
    ) -> &[Rect] {
        self.damage.clear();
        for row in 0..self.rows {
            let mut gap = 0u8;
            for col in 0..self.cols {
                let Some(s) = self.get_mut(col, row) else {
                    continue;
                };
                if !s.drawn {
                    gap = gap.saturating_sub(1);
                    continue;
                }
                s.drawn = false;
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
        }
        if self.damage.is_empty() {
            self.serial = self.serial.saturating_sub(1);
        }
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
