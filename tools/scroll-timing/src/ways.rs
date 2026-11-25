//! The ways to scroll, each a frame at a time into a display's buffer.
//!
//! The redraw, the copy and the pan take the view's place as a pixel line
//! (see the crate's documentation), so they scroll by whole rows or by a
//! few pixel lines at a time. The move takes the top row, and scrolls only
//! by whole rows.
//!
//! - [`redraw`] draws every row of the grid, as vtium does today when the
//!   whole screen moves.
//! - [`Copy`] keeps the grid's pixel lines in a ring in system memory,
//!   draws each line into the ring once, and copies the visible window into
//!   the display's buffer at each frame.
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
use crate::{Counts, Geometry, Unit};

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

/// The text row that holds pixel line `line`, and the line's place inside
/// that row, from 0 at the row's top.
fn row_of(line: i64, g: Geometry) -> (i64, u32) {
    let ch = i64::from(g.cell_height);
    (line.div_euclid(ch), line.rem_euclid(ch) as u32)
}

/// Draws the grid's pixel lines from line `pos` of the scrollback down into
/// `buf`, the screen's buffer with lines `stride` bytes apart. Where `pos`
/// falls inside a row, the grid's first and last rows are drawn in part.
pub fn redraw(pattern: &Pattern, g: Geometry, buf: &mut [u8], stride: usize, pos: i64) -> Counts {
    let mut c = Counts::default();
    let lines = g.rows() * g.cell_height;
    let mut y = 0;
    while y < lines {
        let (row, first) = row_of(pos + i64::from(y), g);
        let count = (g.cell_height - first).min(lines - y);
        let dst = &mut buf[at(y, stride)..];
        c.written += pattern.draw_lines(dst, stride, row, first, count);
        c.rows_drawn += 1;
        y += count;
    }
    c
}

/// The copy from a ring in system memory. The ring holds as many pixel
/// lines as the grid, upright, each as wide as the screen with no padding,
/// and line `l` of the scrollback lives in the ring's line `l` modulo the
/// grid's lines. A text row's lines therefore stay together in the ring, at
/// slot `r` modulo the grid's rows, and a view that stands inside a row
/// finds the lines of its first and last rows apart, at the ring's two
/// ends.
pub struct Copy {
    ring: Vec<u8>,
    /// The scrollback's line that each line of the ring holds.
    lines: Vec<Option<i64>>,
}

impl Copy {
    pub fn new(g: Geometry) -> Copy {
        Copy {
            ring: vec![0; Copy::memory(g) as usize],
            lines: vec![None; (g.rows() * g.cell_height) as usize],
        }
    }

    /// The bytes of the ring for `g`.
    pub fn memory(g: Geometry) -> u64 {
        g.row_bytes() * u64::from(g.rows())
    }

    /// Draws the pixel lines the ring lacks for the view from line `pos`,
    /// then copies the view's lines from the ring into `buf`.
    pub fn frame(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buf: &mut [u8],
        stride: usize,
        pos: i64,
    ) -> Counts {
        let mut c = Counts::default();
        let n = g.rows() * g.cell_height;
        let line = g.line_bytes();
        let ring_line = |l: i64| l.rem_euclid(i64::from(n)) as usize;
        // Each text row the view shows, in whole or in part, and the run of
        // its lines that lie in the view: the lines it lacks among them are
        // drawn in one pass of the pattern, since a row's lines lie
        // together in the ring.
        let mut y = 0;
        while y < n {
            let (row, first) = row_of(pos + i64::from(y), g);
            let count = (g.cell_height - first).min(n - y);
            let from = pos + i64::from(y);
            let missing: Vec<u32> = (0..count)
                .filter(|&k| {
                    self.lines[ring_line(from + i64::from(k))] != Some(from + i64::from(k))
                })
                .collect();
            if let (Some(&lo), Some(&hi)) = (missing.first(), missing.last()) {
                let start = ring_line(from + i64::from(lo));
                let dst = &mut self.ring[start * line..];
                c.system_written += pattern.draw_lines(dst, line, row, first + lo, hi - lo + 1);
                c.rows_drawn += 1;
                for k in lo..=hi {
                    let l = from + i64::from(k);
                    self.lines[ring_line(l)] = Some(l);
                }
            }
            y += count;
        }
        for y in 0..n {
            let src = ring_line(pos + i64::from(y)) * line;
            let dst = at(y, stride);
            buf[dst..dst + line].copy_from_slice(&self.ring[src..src + line]);
            c.written += line as u64;
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
            return redraw(pattern, g, buf, stride, top * i64::from(g.cell_height));
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
    /// The ring for `g` with `spare` rows beyond the grid's, for a view
    /// that scrolls by `unit`. A view that stands inside a row shows one
    /// row more than the grid holds, the last in part, so the ring for
    /// pixel steps holds that row too: without it, the first and the last
    /// row shown would share a slot.
    pub fn new(g: Geometry, spare: u32, unit: Unit) -> Ring {
        let partial = u32::from(unit == Unit::Pixels);
        Ring {
            rows: g.rows() + partial + spare,
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
    pub fn new(g: Geometry, spare: u32, unit: Unit, buffers: usize) -> Pan {
        let ring = Ring::new(g, spare, unit);
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
    /// what the view from pixel line `pos` needs, and returns what that
    /// cost and the pixel line the plane's window starts at, its `SRC_Y`.
    ///
    /// Each row the view shows, in whole or in part, is drawn whole at its
    /// slot and at its twin's, each where the buffer lacks it there. The
    /// window starts at the slot of the view's top row, as far down it as
    /// the view stands inside that row, so each of its slots is either a
    /// row's slot or its twin's, and the window shows the view's lines in
    /// order. The strip under the grid is then drawn at the window's
    /// bottom, over whatever slots lie there, and those slots hold no row
    /// afterwards.
    pub fn frame(
        &mut self,
        pattern: &Pattern,
        g: Geometry,
        buffer: usize,
        buf: &mut [u8],
        stride: usize,
        pos: i64,
    ) -> (Counts, u32) {
        let mut c = Counts::default();
        let (top, offset) = row_of(pos, g);
        assert!(
            offset == 0 || self.ring.rows > g.rows(),
            "a ring with no room for a row in part cannot stand inside a row"
        );
        let start = self.ring.slot(top);
        let shown = g.rows() + u32::from(offset > 0);
        for i in 0..shown {
            c += self.enter(pattern, g, buffer, buf, stride, top + i64::from(i));
        }
        let src_y = start * g.cell_height + offset;
        let strip = g.strip();
        if strip > 0 {
            let from = src_y + g.rows() * g.cell_height;
            let dst = &mut buf[at(from, stride)..];
            c.written += pattern.draw_border(dst, stride, strip);
            for s in from / g.cell_height..=(from + strip - 1) / g.cell_height {
                self.held[buffer][s as usize] = None;
            }
        }
        (c, src_y)
    }
}

#[cfg(test)]
mod tests;
