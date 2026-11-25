//! The ways to scroll, each a frame at a time into a display's buffer.
//!
//! - [`redraw`] draws every row of the grid, as vtium does today when the
//!   whole screen moves.
//! - [`Copy`] keeps the grid's rows in a ring in system memory, draws each
//!   row into the ring once, and copies the visible window into the
//!   display's buffer at each frame.
//! - [`Move`] moves the rows already in the display's buffer by the rows
//!   the view moved since that buffer was drawn, and draws the rows that
//!   came in. It reads the buffer back to do so.
//! - [`Pan`] keeps the ring inside a display buffer taller than the
//!   screen, and the plane shows a window of it. A scroll moves the
//!   window, and each frame draws only the rows that came in.
//!
//! References:
//! - ADR-1310 D6: the three ways, and the ring inside the tall buffer
//!   with each row of its first screen-height drawn twice.

use crate::pattern::Pattern;
use crate::{Counts, Geometry};

/// The ways the tool measures, by the names its rows and options use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Redraw,
    Copy,
    Move,
    Pan,
}

impl Way {
    pub const ALL: [Way; 4] = [Way::Redraw, Way::Copy, Way::Move, Way::Pan];

    pub fn name(self) -> &'static str {
        match self {
            Way::Redraw => "redraw",
            Way::Copy => "copy",
            Way::Move => "move",
            Way::Pan => "pan",
        }
    }

    pub fn parse(name: &str) -> Option<Way> {
        Way::ALL.into_iter().find(|w| w.name() == name)
    }
}

/// The byte offset of pixel line `line` in a buffer of `stride`.
fn at(line: u32, stride: usize) -> usize {
    line as usize * stride
}

/// Draws the grid's rows from `top` down into `buf`, the screen's buffer
/// with lines `stride` bytes apart.
pub fn redraw(pattern: &Pattern, g: Geometry, buf: &mut [u8], stride: usize, top: i64) -> Counts {
    let mut c = Counts::default();
    for i in 0..g.rows() {
        let dst = &mut buf[at(i * g.cell_height, stride)..];
        c.written += pattern.draw_row(dst, stride, top + i64::from(i));
        c.rows_drawn += 1;
    }
    c
}

/// The copy from a ring in system memory. The ring holds as many rows as
/// the grid, upright, a line of pixels as wide as the screen with no
/// padding, and row `r` lives in slot `r` modulo the grid's rows.
pub struct Copy {
    ring: Vec<u8>,
    slots: Vec<Option<i64>>,
}

impl Copy {
    pub fn new(g: Geometry) -> Copy {
        Copy {
            ring: vec![0; Copy::memory(g) as usize],
            slots: vec![None; g.rows() as usize],
        }
    }

    /// The bytes of the ring for `g`.
    pub fn memory(g: Geometry) -> u64 {
        g.row_bytes() * u64::from(g.rows())
    }

    /// Draws the rows the ring lacks for the view from `top`, then copies
    /// the view's rows from the ring into `buf`.
    pub fn frame(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buf: &mut [u8],
        stride: usize,
        top: i64,
    ) -> Counts {
        let mut c = Counts::default();
        let rows = i64::from(g.rows());
        let line = g.line_bytes();
        let row_bytes = g.row_bytes() as usize;
        for i in 0..rows {
            let row = top + i;
            let slot = row.rem_euclid(rows) as usize;
            if self.slots[slot] != Some(row) {
                let dst = &mut self.ring[slot * row_bytes..];
                c.system_written += pattern.draw_row(dst, line, row);
                c.rows_drawn += 1;
                self.slots[slot] = Some(row);
            }
        }
        for i in 0..g.rows() {
            let slot = (top + i64::from(i)).rem_euclid(rows) as usize;
            for y in 0..g.cell_height {
                let src = slot * row_bytes + y as usize * line;
                let dst = at(i * g.cell_height + y, stride);
                buf[dst..dst + line].copy_from_slice(&self.ring[src..src + line]);
                c.written += line as u64;
            }
        }
        c
    }
}

/// The move of rows within the display's buffer. It remembers the view's
/// top at each frame, so that a buffer of age `a`, which holds the frame
/// `a` frames back, is moved by the rows the view moved since then.
#[derive(Default)]
pub struct Move {
    tops: Vec<i64>,
}

impl Move {
    /// Draws the view from `top` into `buf`, a buffer `age` frames old: 0
    /// when its contents are unknown, as a token's age counts.
    pub fn frame(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buf: &mut [u8],
        stride: usize,
        age: u32,
        top: i64,
    ) -> Counts {
        let rows = i64::from(g.rows());
        let then = (age as usize)
            .checked_sub(1)
            .and_then(|back| self.tops.len().checked_sub(back + 1))
            .map(|i| self.tops[i]);
        self.tops.push(top);
        let Some(then) = then.filter(|t| (t - top).abs() < rows) else {
            return redraw(pattern, g, buf, stride, top);
        };
        let mut c = Counts::default();
        let line = g.line_bytes();
        // The view moved back by `shift` rows when it is positive, so the
        // rows the buffer holds move down and new rows come in at the top.
        let shift = then - top;
        let kept = (rows - shift.abs()) as u32;
        let moved_lines = kept * g.cell_height;
        let by = shift.unsigned_abs() as u32 * g.cell_height;
        if shift > 0 {
            for y in (0..moved_lines).rev() {
                let src = at(y, stride);
                buf.copy_within(src..src + line, at(y + by, stride));
            }
        } else {
            for y in 0..moved_lines {
                let src = at(y + by, stride);
                buf.copy_within(src..src + line, at(y, stride));
            }
        }
        c.written += u64::from(moved_lines) * line as u64;
        c.read += u64::from(moved_lines) * line as u64;
        let fresh: Vec<u32> = if shift > 0 {
            (0..shift as u32).collect()
        } else {
            (kept..g.rows()).collect()
        };
        for i in fresh {
            let dst = &mut buf[at(i * g.cell_height, stride)..];
            c.written += pattern.draw_row(dst, stride, top + i64::from(i));
            c.rows_drawn += 1;
        }
        c
    }
}

/// The ring inside a tall buffer: `rows` slots of text rows, and below them
/// `twins` more, each holding a second drawing of the slot `rows` above it.
/// The twins cover a screen's height, so a window that starts at any slot
/// of the ring stays inside the buffer and shows its rows in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ring {
    pub rows: u32,
    pub twins: u32,
}

impl Ring {
    /// The ring for `g` with `spare` rows beyond the grid's.
    pub fn new(g: Geometry, spare: u32) -> Ring {
        Ring {
            rows: g.rows() + spare,
            twins: g.height.div_ceil(g.cell_height),
        }
    }

    /// The tall buffer's height in pixel rows.
    pub fn buffer_height(&self, g: Geometry) -> u32 {
        (self.rows + self.twins) * g.cell_height
    }

    /// The slot of the ring that holds row `row`.
    pub fn slot(&self, row: i64) -> u32 {
        row.rem_euclid(i64::from(self.rows)) as u32
    }

    /// Where row `row` is drawn: its slot, and the slot of its twin where
    /// the slot lies in the ring's first screen-height.
    pub fn places(&self, row: i64) -> (u32, Option<u32>) {
        let slot = self.slot(row);
        (slot, (slot < self.twins).then_some(slot + self.rows))
    }
}

/// The pan, with what each of the display's buffers holds: the row in each
/// slot of its ring and twins, `None` where nothing known is drawn.
pub struct Pan {
    pub ring: Ring,
    held: Vec<Vec<Option<i64>>>,
}

impl Pan {
    pub fn new(g: Geometry, spare: u32, buffers: usize) -> Pan {
        let ring = Ring::new(g, spare);
        let slots = (ring.rows + ring.twins) as usize;
        Pan {
            ring,
            held: vec![vec![None; slots]; buffers],
        }
    }

    /// What buffer `buffer` holds in each slot.
    pub fn held(&self, buffer: usize) -> &[Option<i64>] {
        &self.held[buffer]
    }

    /// Forgets what every buffer holds, as after a modeset.
    pub fn forget(&mut self) {
        for held in &mut self.held {
            held.fill(None);
        }
    }

    /// Draws row `row` at its slot and at its twin's slot, each where
    /// buffer `buffer` does not hold it already.
    fn enter(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buffer: usize,
        buf: &mut [u8],
        stride: usize,
        row: i64,
    ) -> Counts {
        let mut c = Counts::default();
        let (slot, twin) = self.ring.places(row);
        for s in std::iter::once(slot).chain(twin) {
            if self.held[buffer][s as usize] != Some(row) {
                let dst = &mut buf[at(s * g.cell_height, stride)..];
                c.written += pattern.draw_row(dst, stride, row);
                c.rows_drawn += 1;
                self.held[buffer][s as usize] = Some(row);
            }
        }
        c
    }

    /// Draws into buffer `buffer`, `buf` with lines `stride` bytes apart,
    /// what the view from `top` needs, and returns what that cost and the
    /// pixel row the plane's window starts at, its `SRC_Y`.
    ///
    /// Each row of the view is drawn at its slot and at its twin's, each
    /// where the buffer lacks it there. The window starts at the slot of
    /// the view's top row, so each of its slots is either a row's slot or
    /// its twin's, and the window shows the view's rows in order. The
    /// strip under the grid is then drawn at the window's bottom, over
    /// whatever slot lies there, and that slot holds no row afterwards.
    pub fn frame(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buffer: usize,
        buf: &mut [u8],
        stride: usize,
        top: i64,
    ) -> (Counts, u32) {
        let mut c = Counts::default();
        let start = self.ring.slot(top);
        for i in 0..g.rows() {
            c += self.enter(pattern, g, buffer, buf, stride, top + i64::from(i));
        }
        let strip = g.strip();
        if strip > 0 {
            let s = start + g.rows();
            let dst = &mut buf[at(s * g.cell_height, stride)..];
            c.written += pattern.draw_border(dst, stride, strip);
            self.held[buffer][s as usize] = None;
        }
        (c, start * g.cell_height)
    }
}

#[cfg(test)]
mod tests;
