//! The ink grid: what the cell shader reads of the neighbours whose glyphs
//! send ink into a cell, and of the display lines that receive it (ADR-1306
//! D9).
//!
//! A cell that receives ink reads, for each neighbour its glyph's ink may
//! come from, that neighbour's entry here: the reach byte the renderer
//! keeps for the cell (ADR-1306 D5), its span, the slot of its glyph among
//! the boxes of the glyphs whose ink passes their cells, and the color its
//! ink takes in a cell no mark covers. Each cell also holds a bit for each
//! display line through it, set when ink reaches that line of the job the
//! cell belongs to, so the shader looks for the ink a pixel receives only
//! on the lines that receive some. A display line runs across the upright
//! frame's rows, or down its columns on a display turned a quarter, so a
//! cell holds as many bits as the larger of its width and its height.
//!
//! The grid is an [`Format::Rgba8`] texture. Each cell takes `t` texels,
//! one above the other: two for the glyph it sends ink from, then enough
//! for its line bits, 32 bits to a texel. The cell at column `k` and row
//! `r` has its reach byte, its span and its slot's number, low byte first,
//! at `k`, `t·r`; its ink's color at `k`, `t·r + 1`; and its line bits from
//! `k`, `t·r + 2` on, the first line in the lowest bit of the red channel.
//!
//! Each frame names the neighbours whose ink the cells it draws receive,
//! and only those: an entry named on the last frame and not on this one is
//! emptied, so no cell ever reads an entry whose glyph's slot another glyph
//! has taken since. The line bits of a cell are set again for each job the
//! frame draws with received ink, and only such a job reads them. A screen
//! whose ink stays in its cells names no entry, and its frames upload
//! nothing here.

use vtium_term::Rgb;

use super::box_lines::BoxLines;
use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use crate::reach::{Bound, Reach};
use crate::target::Rect;

/// Texels of a cell's entry for the glyph it sends ink from: its reach,
/// span and slot, then its ink's color.
const SOURCE: usize = 2;

/// Bytes of one cell's source entry.
const ENTRY: usize = SOURCE * 4;

/// Display lines whose bits one texel holds.
const BITS: usize = 32;

/// Texels of line bits a cell of `cell` holds: enough for a bit for each
/// display line through it, as many as the larger of its width and its
/// height. The cell shader works the same number out from its `cell`
/// input.
fn words(cell: Size) -> usize {
    usize::try_from(cell.width.max(cell.height))
        .unwrap_or(usize::MAX)
        .div_ceil(BITS)
}

/// A job of the frame that receives ink from its neighbours' glyphs: the
/// column and row of its first cell, how many cells it spans, and whether
/// its display lines run down the upright frame's columns, on a display
/// turned a quarter, rather than across its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReceivingJob {
    pub(crate) col: usize,
    pub(crate) row: usize,
    pub(crate) cells: usize,
    pub(crate) down: bool,
}

/// A neighbour whose glyph sends ink into a job: the column and row of the
/// glyph's first cell, the first and last column of the block of cells its
/// reach names, and its glyph's slot among the boxes.
#[derive(Clone, Copy, Debug)]
struct Sender {
    col: i64,
    row: i64,
    c0: i64,
    c1: i64,
    slot: u32,
}

/// The grid's entries and line bits, kept in memory, and its texture.
#[derive(Debug, Default)]
pub(crate) struct InkGrid {
    cols: usize,
    rows: usize,
    /// Texels of line bits each cell holds.
    words: usize,
    entries: Vec<[u8; ENTRY]>,
    /// Each cell's line bits, `words` to a cell.
    lines: Vec<u32>,
    /// The entries named on this frame, by their place among `entries`.
    named: Vec<usize>,
    /// The entries the last frame named, which this frame empties unless
    /// it names them again.
    last: Vec<usize>,
    /// The first and last row whose entries or line bits changed since
    /// the texture was last filled.
    dirty: Option<(usize, usize)>,
    texture: Option<(Texture, Size)>,
    /// The cells whose line bits this frame set, by their place among
    /// `entries`.
    marked: Vec<usize>,
    /// The cells whose line bits the last frame set, which this frame
    /// clears unless it sets them again.
    last_marked: Vec<usize>,
    /// A job's line bits while [`InkGrid::mark`] works them out.
    scratch: Vec<bool>,
}

impl InkGrid {
    /// Starts a frame on a grid of `cols` by `rows` cells of `cell`. A grid
    /// of another size starts empty.
    pub(crate) fn begin(&mut self, cols: usize, rows: usize, cell: Size) {
        self.last = core::mem::take(&mut self.named);
        self.last_marked = core::mem::take(&mut self.marked);
        let words = words(cell);
        if (cols, rows, words) != (self.cols, self.rows, self.words) {
            self.cols = cols;
            self.rows = rows;
            self.words = words;
            let cells = cols.saturating_mul(rows);
            self.entries.clear();
            self.entries.resize(cells, [0; ENTRY]);
            self.lines.clear();
            self.lines.resize(cells.saturating_mul(words), 0);
            self.last.clear();
            self.last_marked.clear();
            self.dirty = rows.checked_sub(1).map(|last| (0, last));
        }
    }

    /// Names the glyph of the cell at `col`, `row` as a source of ink this
    /// frame: its glyph's slot among the boxes and the color of its ink in
    /// a cell no mark covers. Its reach and span follow from
    /// [`InkGrid::settle`], once the frame has read every cell's reach.
    pub(crate) fn name(&mut self, (col, row): (usize, usize), slot: u32, color: Rgb) {
        let Some(i) = self.index(col, row) else {
            return;
        };
        let [lo, hi] = u16::try_from(slot).unwrap_or(u16::MAX).to_le_bytes();
        let entry = [0, 0, lo, hi, color.r, color.g, color.b, 0];
        if let Some(e) = self.entries.get_mut(i)
            && *e != entry
        {
            *e = entry;
            self.touch(row);
        }
        self.named.push(i);
    }

    /// Gives each entry named this frame the reach and span that `reach`
    /// says its cell has now. The renderer reads a cell's reach when it
    /// scans the cell's row, which can come after a row that receives its
    /// ink was drawn, so the entries take it once the frame is scanned.
    /// An entry the last frame named and this one did not is emptied.
    pub(crate) fn settle(&mut self, mut reach: impl FnMut(usize, usize) -> (Reach, u8)) {
        self.named.sort_unstable();
        self.named.dedup();
        for i in core::mem::take(&mut self.last) {
            if self.named.binary_search(&i).is_err()
                && let Some(e) = self.entries.get_mut(i)
                && *e != [0; ENTRY]
            {
                *e = [0; ENTRY];
                let row = i.checked_div(self.cols).unwrap_or(0);
                self.touch(row);
            }
        }
        for k in 0..self.named.len() {
            let Some(&i) = self.named.get(k) else {
                continue;
            };
            let (col, row) = (
                i.checked_rem(self.cols).unwrap_or(0),
                i.checked_div(self.cols).unwrap_or(0),
            );
            let (r, span) = reach(col, row);
            if let Some(e) = self.entries.get_mut(i)
                && (e[0], e[1]) != (r.byte(), span)
            {
                e[0] = r.byte();
                e[1] = span;
                self.touch(row);
            }
        }
    }

    /// Sets the line bits of each of `jobs`, the frame's jobs that receive
    /// ink, once the frame is settled, and clears the bits of every cell the
    /// last frame set and this one did not. The neighbours are those the
    /// shader reads at `bound`, and `boxes` gives the ink of each one's
    /// glyph by its slot among the boxes ([`InkGrid::mark`]).
    pub(crate) fn mark_jobs<'b>(
        &mut self,
        jobs: impl Iterator<Item = ReceivingJob>,
        (bound, cell): (Bound, Size),
        boxes: impl Fn(u32) -> Option<&'b BoxLines>,
    ) {
        for job in jobs {
            self.mark(job, (bound, cell), &boxes);
        }
        self.marked.sort_unstable();
        self.marked.dedup();
        for i in core::mem::take(&mut self.last_marked) {
            if self.marked.binary_search(&i).is_err() {
                let at = i.saturating_mul(self.words);
                if let Some(words) = self.lines.get_mut(at..at.saturating_add(self.words))
                    && words.iter().any(|&w| w != 0)
                {
                    words.fill(0);
                    self.touch(i.checked_div(self.cols).unwrap_or(0));
                }
            }
        }
    }

    /// Sets the line bits of `job`, in cells of `cell`: a bit for each
    /// display line through the job that ink from the entries reaches,
    /// which the cell shader's `received` would find ink on. Each
    /// neighbour's ink is its glyph's box as `boxes` gives it, cut to the
    /// block of cells its reach names. A display line runs across the job,
    /// the same in each of its cells, or down one of its cells' columns.
    fn mark<'b>(
        &mut self,
        job: ReceivingJob,
        (bound, cell): (Bound, Size),
        boxes: &impl Fn(u32) -> Option<&'b BoxLines>,
    ) {
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let lines = if job.down {
            n(cell.width)
        } else {
            n(cell.height)
        };
        let mut bits = core::mem::take(&mut self.scratch);
        bits.clear();
        bits.resize(lines.saturating_mul(job.cells.max(1)), false);
        let int = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        let (side, up) = (i64::from(bound.side), i64::from(bound.rows));
        let (c, r) = (int(job.col), int(job.row));
        let end = int(job.col.saturating_add(job.cells));
        for row in r.saturating_sub(up)..=r.saturating_add(up) {
            for col in c.saturating_sub(side).saturating_sub(1)..end.saturating_add(side) {
                if let Some(s) = self.sender(job, (col, row), side)
                    && let Some(ink) = boxes(s.slot)
                {
                    gather(job, s, ink, (side, up), (lines, &mut bits));
                }
            }
        }
        for j in 0..job.cells {
            let at = if job.down { j.saturating_mul(lines) } else { 0 };
            if let Some(cell_bits) = bits.get(at..at.saturating_add(lines)) {
                self.set_lines(job.col.saturating_add(j), job.row, cell_bits);
            }
        }
        self.scratch = bits;
    }

    /// The neighbour at `col`, `row` when its glyph sends ink into `job` as
    /// the cell shader reads it at a bound of `side` cells: its entry is
    /// named, and its reach covers the job's row. A glyph in the job's own
    /// row counts from beside the job only, and from the left only when it
    /// ends fewer than `side` cells before the job, as the shader takes it.
    fn sender(&self, job: ReceivingJob, (col, row): (i64, i64), side: i64) -> Option<Sender> {
        let int = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        let (c, r) = (int(job.col), int(job.row));
        let end = int(job.col.saturating_add(job.cells));
        if row == r && col >= c && col < end {
            return None;
        }
        let e = self.entry(col, row)?;
        let (reach, span) = (Reach::from_byte(e[0]), i64::from(e[1]));
        if reach.is_none() || span == 0 {
            return None;
        }
        let gap = c.saturating_sub(col.saturating_add(span));
        if row == r && col < c && (gap < 0 || gap >= side) {
            return None;
        }
        let (r0, r1) = (
            row.saturating_sub(i64::from(reach.up())),
            row.saturating_add(i64::from(reach.down())),
        );
        (r >= r0 && r <= r1).then(|| Sender {
            col,
            row,
            c0: col.saturating_sub(i64::from(reach.left())),
            c1: col
                .saturating_add(span)
                .saturating_sub(1)
                .saturating_add(i64::from(reach.right())),
            slot: u32::from(u16::from_le_bytes([e[2], e[3]])),
        })
    }

    /// The texture, once an entry has been named.
    pub(crate) fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref().map(|(t, _)| t)
    }

    /// Fills the texture's rows that changed, making it first at the
    /// grid's size when it has none of that size. Gives how many calls of
    /// the frame's batch of uploads it made.
    pub(crate) fn upload<G: Gl>(&mut self, gl: &mut G) -> Result<usize, Error> {
        if self.dirty.is_none() {
            return Ok(0);
        }
        let mut calls = 0usize;
        let texels = SOURCE.saturating_add(self.words);
        let size = Size::new(
            u32::try_from(self.cols).unwrap_or(u32::MAX),
            u32::try_from(self.rows.saturating_mul(texels)).unwrap_or(u32::MAX),
        );
        if self.texture.as_ref().is_none_or(|&(_, s)| s != size) {
            if self.named.is_empty() && self.texture.is_none() {
                // nothing to read, and nothing made yet
                self.dirty = None;
                return Ok(0);
            }
            if size.width == 0 || size.height == 0 {
                self.dirty = None;
                return Ok(0);
            }
            if !gl.limits().fits_texture(size) {
                return Err(Error::new(
                    ErrorKind::Limit,
                    format!(
                        "the ink grid of {}x{} cells passes the largest texture",
                        self.cols, self.rows
                    ),
                ));
            }
            if let Some((old, _)) = self.texture.take() {
                gl.free_texture(old)?;
                calls = calls.saturating_add(1);
            }
            self.texture = Some((gl.create_texture(Format::Rgba8, size)?, size));
            calls = calls.saturating_add(1);
            self.dirty = self.rows.checked_sub(1).map(|last| (0, last));
        }
        let Some((first, last)) = self.dirty.take() else {
            return Ok(calls);
        };
        let Some((texture, _)) = &self.texture else {
            return Ok(calls);
        };
        let stride = self.cols.saturating_mul(4);
        let lines = last.saturating_sub(first).saturating_add(1);
        let block = stride.saturating_mul(texels);
        let mut pixels = vec![0u8; block.saturating_mul(lines)];
        for (row, out) in (first..=last).zip(pixels.chunks_mut(block.max(1))) {
            let start = row.saturating_mul(self.cols);
            let cells = self.entries.get(start..start.saturating_add(self.cols));
            let mut texel_rows = out.chunks_mut(stride.max(1));
            let (Some(top), Some(bottom)) = (texel_rows.next(), texel_rows.next()) else {
                continue;
            };
            for (k, e) in cells.unwrap_or_default().iter().enumerate() {
                let at = k.saturating_mul(4);
                let (t0, t1) = e.split_at(4);
                if let (Some(t), Some(b)) = (
                    top.get_mut(at..at.saturating_add(4)),
                    bottom.get_mut(at..at.saturating_add(4)),
                ) {
                    t.copy_from_slice(t0);
                    b.copy_from_slice(t1);
                }
            }
            for (w, out) in texel_rows.enumerate() {
                for (k, texel) in out.chunks_mut(4).enumerate() {
                    let i = start
                        .saturating_add(k)
                        .saturating_mul(self.words)
                        .saturating_add(w);
                    let bits = self.lines.get(i).copied().unwrap_or(0);
                    texel.copy_from_slice(&bits.to_le_bytes());
                }
            }
        }
        let rect = Rect {
            x: 0,
            y: u32::try_from(first.saturating_mul(texels)).unwrap_or(u32::MAX),
            width: size.width,
            height: u32::try_from(lines.saturating_mul(texels)).unwrap_or(u32::MAX),
        };
        gl.fill_texture(texture, rect, &pixels, stride)?;
        Ok(calls.saturating_add(1))
    }

    /// Frees the texture.
    pub(crate) fn free<G: Gl>(self, gl: &mut G) -> Result<(), Error> {
        match self.texture {
            Some((t, _)) => gl.free_texture(t),
            None => Ok(()),
        }
    }

    fn index(&self, col: usize, row: usize) -> Option<usize> {
        (col < self.cols && row < self.rows)
            .then(|| row.saturating_mul(self.cols).saturating_add(col))
    }

    /// The entry of the cell at `col`, `row`, which may lie off the grid.
    fn entry(&self, col: i64, row: i64) -> Option<&[u8; ENTRY]> {
        let (col, row) = (usize::try_from(col).ok()?, usize::try_from(row).ok()?);
        self.entries.get(self.index(col, row)?)
    }

    /// Sets the line bits of the cell at `col`, `row` to `bits`.
    fn set_lines(&mut self, col: usize, row: usize, bits: &[bool]) {
        let Some(i) = self.index(col, row) else {
            return;
        };
        self.marked.push(i);
        let at = i.saturating_mul(self.words);
        let Some(words) = self.lines.get_mut(at..at.saturating_add(self.words)) else {
            return;
        };
        let mut changed = false;
        for (word, chunk) in words.iter_mut().zip(bits.chunks(BITS)) {
            let packed = chunk
                .iter()
                .enumerate()
                .fold(0u32, |w, (b, &on)| w | u32::from(on) << b);
            changed |= *word != packed;
            *word = packed;
        }
        if changed {
            self.touch(row);
        }
    }

    fn touch(&mut self, row: usize) {
        self.dirty = Some(match self.dirty {
            Some((a, b)) => (a.min(row), b.max(row)),
            None => (row, row),
        });
    }
}

/// ORs into `bits`, `lines` bits a cell of `job`, the ink of `sender`'s
/// box (`ink`) in each of the job's cells inside the sender's block. The
/// sender's first cell lies `side` cells and `up` rows from its slot's top
/// left. A display line across the job gathers the ink of all its cells in
/// the first cell's bits; a line down a column has bits of its own cell.
fn gather(
    job: ReceivingJob,
    sender: Sender,
    ink: &BoxLines,
    (side, up): (i64, i64),
    (lines, bits): (usize, &mut [bool]),
) {
    let int = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
    let row = int(job.row);
    let by = row.saturating_sub(sender.row).saturating_add(up);
    let first = int(job.col);
    for (j, col) in (first..first.saturating_add(int(job.cells))).enumerate() {
        if col < sender.c0 || col > sender.c1 {
            continue;
        }
        let at = if job.down { j.saturating_mul(lines) } else { 0 };
        if let Some(out) = bits.get_mut(at..at.saturating_add(lines)) {
            let bx = col.saturating_sub(sender.col).saturating_add(side);
            ink.or_into((bx, by), job.down, out);
        }
    }
}

#[cfg(test)]
mod tests;
