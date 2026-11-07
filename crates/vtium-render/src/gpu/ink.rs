//! The ink grid: what the cell shader reads of the neighbours whose glyphs
//! send ink into a cell (ADR-1306 D9).
//!
//! A cell that receives ink reads, for each neighbour its glyph's ink may
//! come from, that neighbour's entry here: the reach byte the renderer
//! keeps for the cell (ADR-1306 D5), its span, the slot of its glyph among
//! the boxes of the glyphs whose ink passes their cells, and the color its
//! ink takes in a cell no mark covers. The grid is an [`Format::Rgba8`]
//! texture with two texels a cell, one above the other: the cell at column
//! `k` and row `r` has its reach byte, its span and its slot's number, low
//! byte first, at `k`, `2r`, and its ink's color at `k`, `2r + 1`.
//!
//! Each frame names the neighbours whose ink the cells it draws receive,
//! and only those: an entry named on the last frame and not on this one is
//! emptied, so no cell ever reads an entry whose glyph's slot another glyph
//! has taken since. A screen whose ink stays in its cells names no entry,
//! and its frames upload nothing here.

use vtium_term::Rgb;

use super::gl::{Error, ErrorKind, Format, Gl, Size, Texture};
use crate::reach::Reach;
use crate::target::Rect;

/// Texels a cell's entry takes, one above the other.
const TEXELS: usize = 2;

/// Bytes of one cell's entry.
const ENTRY: usize = TEXELS * 4;

/// The grid's entries, kept in memory, and its texture.
#[derive(Debug, Default)]
pub(crate) struct InkGrid {
    cols: usize,
    rows: usize,
    entries: Vec<[u8; ENTRY]>,
    /// The entries named on this frame, by their place among `entries`.
    named: Vec<usize>,
    /// The entries the last frame named, which this frame empties unless
    /// it names them again.
    last: Vec<usize>,
    /// The first and last row whose entries changed since the texture was
    /// last filled.
    dirty: Option<(usize, usize)>,
    texture: Option<(Texture, Size)>,
}

impl InkGrid {
    /// Starts a frame on a grid of `cols` by `rows` cells. A grid of
    /// another size starts empty.
    pub(crate) fn begin(&mut self, cols: usize, rows: usize) {
        self.last = core::mem::take(&mut self.named);
        if (cols, rows) != (self.cols, self.rows) {
            self.cols = cols;
            self.rows = rows;
            self.entries.clear();
            self.entries.resize(cols.saturating_mul(rows), [0; ENTRY]);
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
        let size = Size::new(
            u32::try_from(self.cols).unwrap_or(u32::MAX),
            u32::try_from(self.rows.saturating_mul(TEXELS)).unwrap_or(u32::MAX),
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
        let mut pixels = vec![0u8; stride.saturating_mul(lines).saturating_mul(TEXELS)];
        for (row, out) in
            (first..=last).zip(pixels.chunks_mut(stride.saturating_mul(TEXELS).max(1)))
        {
            let start = row.saturating_mul(self.cols);
            let cells = self.entries.get(start..start.saturating_add(self.cols));
            let (top, bottom) = out.split_at_mut(stride);
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
        }
        let rect = Rect {
            x: 0,
            y: u32::try_from(first.saturating_mul(TEXELS)).unwrap_or(u32::MAX),
            width: size.width,
            height: u32::try_from(lines.saturating_mul(TEXELS)).unwrap_or(u32::MAX),
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

    fn touch(&mut self, row: usize) {
        self.dirty = Some(match self.dirty {
            Some((a, b)) => (a.min(row), b.max(row)),
            None => (row, row),
        });
    }
}
