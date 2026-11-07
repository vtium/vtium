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

/// Where a glyph's box holds ink, by the cells of its slot (ADR-1306 D3):
/// for each cell of the slot, which of its rows of pixels and which of its
/// columns hold any coverage. A cell's line bits follow from it, so they
/// are worked out once, when the glyph takes its box.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoxLines {
    /// The slot's cells across.
    across: usize,
    /// The cell's width and height.
    cell: (usize, usize),
    /// For each cell of the slot, a row at a time, whether each of its
    /// rows of pixels holds ink.
    rows: Vec<bool>,
    /// For each cell of the slot, whether each of its columns holds ink.
    columns: Vec<bool>,
}

impl BoxLines {
    /// The lines of the box whose coverage is `pixels`, `slot` in size, a
    /// line after the other, in cells of `cell`.
    pub(crate) fn new(pixels: &[u8], slot: Size, cell: Size) -> BoxLines {
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let (cw, ch) = (n(cell.width), n(cell.height));
        let across = n(slot.width).checked_div(cw).unwrap_or(0);
        let down = n(slot.height).checked_div(ch).unwrap_or(0);
        let cells = across.saturating_mul(down);
        let mut lines = BoxLines {
            across,
            cell: (cw, ch),
            rows: vec![false; cells.saturating_mul(ch)],
            columns: vec![false; cells.saturating_mul(cw)],
        };
        let stride = n(slot.width);
        for (y, line) in pixels.chunks(stride.max(1)).enumerate() {
            let (by, py) = (y.checked_div(ch), y.checked_rem(ch));
            let (Some(by), Some(py)) = (by, py) else {
                continue;
            };
            for (x, &a) in line.iter().enumerate() {
                let (Some(bx), Some(px)) = (x.checked_div(cw), x.checked_rem(cw)) else {
                    continue;
                };
                if a == 0 {
                    continue;
                }
                let i = by.saturating_mul(across).saturating_add(bx);
                if let Some(r) = lines.rows.get_mut(i.saturating_mul(ch).saturating_add(py)) {
                    *r = true;
                }
                if let Some(c) = lines
                    .columns
                    .get_mut(i.saturating_mul(cw).saturating_add(px))
                {
                    *c = true;
                }
            }
        }
        lines
    }

    /// Whether the slot's cell at `bx`, `by` holds ink on each of its
    /// display lines, its rows of pixels or, `down`, its columns, ORed
    /// into `out`; nothing for a cell outside the slot.
    fn or_into(&self, (bx, by): (i64, i64), down: bool, out: &mut [bool]) {
        let (Ok(bx), Ok(by)) = (usize::try_from(bx), usize::try_from(by)) else {
            return;
        };
        if bx >= self.across {
            return;
        }
        let (lines, n) = match down {
            true => (&self.columns, self.cell.0),
            false => (&self.rows, self.cell.1),
        };
        let at = by
            .saturating_mul(self.across)
            .saturating_add(bx)
            .saturating_mul(n);
        let Some(cell) = lines.get(at..at.saturating_add(n)) else {
            return;
        };
        for (o, &inked) in out.iter_mut().zip(cell) {
            *o |= inked;
        }
    }
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
    /// A job's line bits while [`InkGrid::mark`] works them out.
    scratch: Vec<bool>,
}

impl InkGrid {
    /// Starts a frame on a grid of `cols` by `rows` cells of `cell`. A grid
    /// of another size starts empty.
    pub(crate) fn begin(&mut self, cols: usize, rows: usize, cell: Size) {
        self.last = core::mem::take(&mut self.named);
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

    /// Sets the line bits of the job whose first cell is at `col`, `row`
    /// and which spans `cells` cells of `cell`, once the frame is settled:
    /// a bit for each display line through the job that ink from the
    /// entries reaches, which the cell shader's `received` would find ink
    /// on. The neighbours are the ones the shader reads, at `bound`, and
    /// each one's ink is its glyph's box as `boxes` gives it by its slot,
    /// cut to the block of cells its reach names. A display line runs
    /// across the job, the same in each of its cells, or, `down`, down one
    /// of its cells' columns.
    pub(crate) fn mark<'b>(
        &mut self,
        (col, row, cells): (usize, usize, usize),
        (down, bound, cell): (bool, Bound, Size),
        boxes: impl Fn(u32) -> Option<&'b BoxLines>,
    ) {
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let lines = if down { n(cell.width) } else { n(cell.height) };
        let mut job = core::mem::take(&mut self.scratch);
        job.clear();
        job.resize(lines.saturating_mul(cells.max(1)), false);
        let int = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        let (side, up) = (i64::from(bound.side), i64::from(bound.rows));
        let (c, r, end) = (int(col), int(row), int(col.saturating_add(cells)));
        for nr in r.saturating_sub(up)..=r.saturating_add(up) {
            for k in c.saturating_sub(side).saturating_sub(1)..end.saturating_add(side) {
                if nr == r && k >= c && k < end {
                    continue;
                }
                let Some(e) = self.entry(k, nr) else {
                    continue;
                };
                let (reach, span) = (Reach::from_byte(e[0]), i64::from(e[1]));
                if reach.is_none() || span == 0 {
                    continue;
                }
                // the shader takes a glyph to the left in the job's row
                // only from fewer than the bound's cells away
                let gap = c.saturating_sub(k.saturating_add(span));
                if nr == r && k < c && (gap < 0 || gap >= side) {
                    continue;
                }
                let c0 = k.saturating_sub(i64::from(reach.left()));
                let c1 = k
                    .saturating_add(span)
                    .saturating_sub(1)
                    .saturating_add(i64::from(reach.right()));
                let r0 = nr.saturating_sub(i64::from(reach.up()));
                let r1 = nr.saturating_add(i64::from(reach.down()));
                if r < r0 || r > r1 {
                    continue;
                }
                let Some(lines_of) = boxes(u32::from(u16::from_le_bytes([e[2], e[3]]))) else {
                    continue;
                };
                // the box's cell under each of the job's cells inside the
                // block, the glyph's first cell `side` cells and `up` rows
                // from the slot's top left; a display line across the job
                // gathers the ink of all its cells in the first one's bits
                let by = r.saturating_sub(nr).saturating_add(up);
                for (j, jc) in (c..end).enumerate() {
                    if jc < c0 || jc > c1 {
                        continue;
                    }
                    let at = if down { j.saturating_mul(lines) } else { 0 };
                    if let Some(out) = job.get_mut(at..at.saturating_add(lines)) {
                        let bx = jc.saturating_sub(k).saturating_add(side);
                        lines_of.or_into((bx, by), down, out);
                    }
                }
            }
        }
        for j in 0..cells {
            let at = if down { j.saturating_mul(lines) } else { 0 };
            if let Some(bits) = job.get(at..at.saturating_add(lines)) {
                self.set_lines(col.saturating_add(j), row, bits);
            }
        }
        self.scratch = job;
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    const CELL: Size = Size::new(8, 16);
    const BOUND: Bound = Bound { side: 1, rows: 1 };

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
        g.mark((2, 0, 1), (false, BOUND, CELL), lines);
        assert_eq!(g.lines_set(2, 0), [13, 14, 15], "above the glyph");
        g.mark((3, 0, 1), (false, BOUND, CELL), lines);
        assert_eq!(g.lines_set(3, 0), [10, 11], "above and to the right");
        g.mark((1, 0, 1), (false, BOUND, CELL), lines);
        assert!(g.lines_set(1, 0).is_empty(), "left of the reach");
        g.mark((3, 1, 1), (false, BOUND, CELL), lines);
        assert!(g.lines_set(3, 1).is_empty(), "beside it, where no ink lies");
        // a job of two cells: a line any of its cells receives ink on, in
        // each of its cells
        g.mark((2, 0, 2), (false, BOUND, CELL), lines);
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
        g.mark((2, 0, 2), (true, BOUND, CELL), lines);
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
        g.mark((2, 0, 1), (false, BOUND, CELL), lines);
        assert!(g.lines_set(2, 0).is_empty(), "no reach up");
        let mut g = grid();
        g.begin(5, 3, CELL);
        g.settle(|_, _| (Reach::new(0, 1, 1, 0), 1));
        g.mark((2, 0, 1), (false, BOUND, CELL), lines);
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
        g.mark((2, 0, 1), (false, BOUND, CELL), lines);
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
}
