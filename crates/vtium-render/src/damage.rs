//! What each buffer still lacks, cell by cell, and the damage rectangles a
//! frame hands the display.

use crate::target::Rect;

/// Damaged cells of a row at most this many cells apart share one damage
/// rectangle.
const DAMAGE_MERGE: u8 = 3;

/// What the renderer last drew at a cell, for the changes the frame cannot
/// know.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellState {
    /// Buffers that still lack the cell's newest pixels.
    pub(crate) pending: u8,
    pub(crate) cursor: bool,
    pub(crate) hidden: bool,
    /// The cells it drew over: 0 as a trailing half, 1 or 2.
    pub(crate) span: u8,
    /// Drawn on this frame: in its damage.
    pub(crate) drawn: bool,
}

/// The grid's cells, what each buffer lacks of them, and this frame's
/// damage.
#[derive(Debug, Default)]
pub(crate) struct Cells {
    cols: usize,
    rows: usize,
    states: Vec<CellState>,
    rows_pending: Vec<bool>,
    /// Buffers whose border still lacks the background.
    borders: u8,
    damage: Vec<Rect>,
}

impl Cells {
    /// Every cell and the border pending in each of `buffers` buffers.
    pub(crate) fn reset(&mut self, cols: usize, rows: usize, buffers: u8) {
        self.cols = cols;
        self.rows = rows;
        self.states.clear();
        self.states.resize(
            cols.saturating_mul(rows),
            CellState {
                pending: buffers,
                ..CellState::default()
            },
        );
        self.rows_pending.clear();
        self.rows_pending.resize(rows, true);
        self.borders = buffers;
    }

    pub(crate) fn get_mut(&mut self, col: usize, row: usize) -> Option<&mut CellState> {
        if col >= self.cols {
            return None;
        }
        self.states
            .get_mut(row.checked_mul(self.cols)?.checked_add(col)?)
    }

    /// Whether some buffer lacks a cell of `row`.
    pub(crate) fn row_pending(&self, row: usize) -> bool {
        self.rows_pending.get(row).copied().unwrap_or(true)
    }

    pub(crate) fn set_row_pending(&mut self, row: usize, pending: bool) {
        if let Some(p) = self.rows_pending.get_mut(row) {
            *p = pending;
        }
    }

    /// Whether this buffer's border lacks the background; counts it drawn.
    pub(crate) fn take_border(&mut self) -> bool {
        let pending = self.borders > 0;
        self.borders = self.borders.saturating_sub(1);
        pending
    }

    /// Clears the drawn marks into this frame's damage: `whole` after a
    /// border fill, else each row's drawn cells, those at most
    /// `DAMAGE_MERGE` cells apart in one rectangle. `rect` gives a cell's
    /// display rectangle.
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
