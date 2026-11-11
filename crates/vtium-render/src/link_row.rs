//! The overlay row that shows the hovered link's address while Ctrl is held
//! (ADR-1002, "Drawing"). The frame names the row and the address
//! ([`Frame::overlay`]). The renderer draws the address from the row's
//! first column, one byte to a character cell, in the palette's default
//! colors inverted. An address longer than the row is cut at the row's
//! last column, which shows `…` in place of the rest.
//!
//! The overlay covers only the cells the address takes. The rest of the
//! row shows what the program drew there. The renderer reads each cell the
//! overlay covers through [`LinkRow::cell`], which puts the overlay's
//! character in place of the screen's, so the CPU path and the GPU path
//! draw it the same way. Under those cells the text cursor and the
//! selection are not drawn. The pointer draws above the overlay, as it
//! draws above every cell.
//!
//! The frame marks the overlay's row changed when the overlay appears,
//! moves, changes its address or goes, so the renderer redraws the row's
//! cells then.

use vtium_term::{Attrs, Frame, FrameCell, Glyph, Rgb, Width};

/// The character that ends an address cut at the row's end.
pub(crate) const CUT: char = '\u{2026}';

/// The overlay row of one frame, fitted to the grid's columns.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkRow<'f> {
    row: usize,
    uri: &'f str,
    /// The cells the overlay covers, from column 0.
    len: usize,
    /// The address is longer than the row, so its last cell shows `…`.
    cut: bool,
    fg: Rgb,
    bg: Rgb,
}

impl<'f> LinkRow<'f> {
    /// The overlay `frame` shows, on a grid of `cols` columns and `rows`
    /// rows; `None` when the frame shows none, or when its row or the grid
    /// is empty or past the grid.
    pub(crate) fn new(frame: &Frame<'f>, cols: usize, rows: usize) -> Option<Self> {
        let overlay = frame.overlay()?;
        if overlay.row >= rows || cols == 0 || overlay.uri.is_empty() {
            return None;
        }
        let palette = frame.palette();
        Some(LinkRow {
            row: overlay.row,
            uri: overlay.uri,
            len: overlay.uri.len().min(cols),
            cut: overlay.uri.len() > cols,
            fg: palette.bg(),
            bg: palette.fg(),
        })
    }

    /// The row the overlay covers.
    pub(crate) fn row(&self) -> usize {
        self.row
    }

    /// Whether the overlay covers the cell at `col` of `row`.
    pub(crate) fn covers(&self, col: usize, row: usize) -> bool {
        row == self.row && col < self.len
    }

    /// The character the overlay shows at `col`, inside the cells it
    /// covers.
    fn char_at(&self, col: usize) -> Option<char> {
        if col >= self.len {
            return None;
        }
        if self.cut && col.saturating_add(1) == self.len {
            return Some(CUT);
        }
        // The frame's address is printable ASCII (ADR-1002, "What a link
        // may hold"), so each byte is one character.
        self.uri.as_bytes().get(col).map(|&b| char::from(b))
    }

    /// The characters the overlay shows, left to right.
    pub(crate) fn chars(&self) -> impl Iterator<Item = char> + '_ {
        (0..self.len).filter_map(|col| self.char_at(col))
    }

    /// The cell `under`, at `col` of `row`, as the frame draws it with the
    /// overlay: the overlay's character in its place where the overlay
    /// covers it, and `under` unchanged elsewhere.
    pub(crate) fn cell(&self, col: usize, row: usize, under: FrameCell<'f>) -> FrameCell<'f> {
        let Some(c) = self.char_at(col).filter(|_| row == self.row) else {
            return under;
        };
        let mut fc = under;
        fc.glyph = Glyph::Char(c);
        fc.width = Width::Narrow;
        fc.fg = self.fg;
        fc.bg = self.bg;
        fc.underline_color = None;
        // the attribute that a cell written in the default colors inverted
        // carries, which a 1-bit display reads to light the background
        fc.attrs = Attrs::INVERSE;
        fc.default_bg = false;
        fc.link = None;
        fc.hovered = false;
        fc.selected = false;
        #[cfg(feature = "bidi")]
        {
            fc.rtl = false;
            fc.joins = vtium_term::Joins::default();
            fc.mirror = None;
        }
        fc
    }
}
