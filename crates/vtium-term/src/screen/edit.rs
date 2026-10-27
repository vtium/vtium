//! Erasing, inserting and deleting, and scrolling.
//!
//! Every cell these clear or push off a line gives back its cluster bytes
//! through the line ([`Line::erase`], [`Line::insert_cells`],
//! [`Line::delete_cells`]), and a line that scrolls off takes its tables
//! with it, so the cluster arena's bound holds on every path. Cleared and
//! inserted cells take the blank style: the pen's background only.
//!
//! A wide character an edit cuts goes whole, on either side of the cut.
//!
//! An edit that clears a row's last column ends the row's wrap: its text no
//! longer continues on the next row. EL 0, EL 2, ECH and DCH end the
//! cursor's row's, as does ED on the cursor's row from the cursor; ED and
//! DECSED end the wrap of each other row they clear whole, a row where
//! DECSED leaves a protected cell keeping it. This is xterm's rule
//! (`ClearRight`, `ScrnDeleteChar`, `ClearBufRows`), with EL 2 as Ghostty's
//! `eraseLine` has it, since xterm keeps the wrap of a row EL 2 clears.
//! IL, DL and SD end the wrap of each row they shift and of the row above
//! them, whose next row changes, as Ghostty's `insertLines` and
//! `deleteLines` do in a region as wide as the screen; xterm moves the
//! flags, joining a row to an unrelated one. A line feed's scroll, and SU,
//! move lines with their flags, since a wrapped line moves whole, and end
//! the wrap of the row above the region. A line a region below the first
//! row scrolls into the scrollback enters it unwrapped, since its next row
//! stays on the screen. An erase that clears the main screen's first row
//! whole, and IL, DL and SD there, end the wrap of the scrollback's newest
//! line, whose next row it was; xterm keeps that wrap, joining the line to
//! whatever is printed there next.
//!
//! [`Line::erase`]: crate::Line::erase
//! [`Line::insert_cells`]: crate::Line::insert_cells
//! [`Line::delete_cells`]: crate::Line::delete_cells

use core::mem;
use core::ops::Range;

use super::Screen;
use crate::cell::Width;
use crate::line::LineFlags;
use crate::style::{Attrs, Style};

/// Which cells of a region an erase clears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Erase {
    /// Every cell (ED, EL, ECH).
    #[default]
    All,
    /// Every cell but those DECSCA protected (DECSED, DECSEL).
    Unprotected,
}

impl Screen {
    /// A wide character whose halves lie on both sides of the boundary
    /// before cell `x` of row `y` goes whole: an edit on one side would
    /// leave half a character. The boundary may be the margin: a line keeps
    /// its cells past it after the screen narrows, and a character there
    /// keeps both halves until one side is edited.
    pub(super) fn split_wide(&mut self, y: usize, x: usize) {
        let Some(line) = self.line(y) else {
            return;
        };
        let Some(lead) = x.checked_sub(1) else {
            return;
        };
        if x >= line.len()
            || line.cell(x).width() != Width::WideTail
            || line.cell(lead).width() != Width::Wide
        {
            return;
        }
        self.erase_cells(y, lead..x.saturating_add(1));
    }

    /// Clears the cells of `cols` in row `y` to the blank style.
    pub(super) fn erase_cells(&mut self, y: usize, cols: Range<usize>) {
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.erase(cols.clone(), &blank);
        }
        self.mark_cells(y, cols);
    }

    /// Clears from `from` to `to` inclusive, each a column and a row, in
    /// reading order; `which` says whether protected cells stay. Returns
    /// the rows it cleared whole. Clearing the screen's first row whole
    /// ends the wrap of the scrollback's newest line.
    fn erase_region(
        &mut self,
        from: (usize, usize),
        to: (usize, usize),
        which: Erase,
    ) -> Vec<usize> {
        self.segment = None;
        let last_col = self.cols.saturating_sub(1);
        let (mut x, first_row) = from;
        let x_to = to.0.min(last_col);
        let y_to = to.1.min(self.rows.saturating_sub(1));
        let mut whole = Vec::new();
        for y in first_row..=y_to {
            let Some(line) = self.line(y) else {
                x = 0;
                continue;
            };
            let mut end = if y == y_to { x_to } else { last_col };
            // A wide character cut by the region goes whole.
            if x > 0
                && line.cell(x).width() == Width::WideTail
                && line.cell(x.saturating_sub(1)).width() == Width::Wide
            {
                x = x.saturating_sub(1);
            }
            let after = end.saturating_add(1);
            if after < line.len()
                && line.cell(end).width() == Width::Wide
                && line.cell(after).width() == Width::WideTail
            {
                end = after;
            }
            let mut runs = Vec::new();
            let mut run = x;
            for col in x..=end {
                if which == Erase::Unprotected && protected(line.style(line.cell(col))) {
                    runs.push(run..col);
                    run = col.saturating_add(1);
                }
            }
            if x == 0 && end >= last_col && runs.is_empty() {
                whole.push(y);
            }
            runs.push(run..end.saturating_add(1));
            for cols in runs {
                self.erase_cells(y, cols);
            }
            x = 0;
        }
        if whole.first() == Some(&0) {
            self.unwrap_scrollback();
        }
        whole
    }

    /// Ends the wrap of each row from `y` to the bottom margin, which an
    /// edit shifted, and of the row above `y`, whose next row changed: the
    /// scrollback's newest line above the first row.
    fn unwrap_shifted(&mut self, y: usize) {
        match y.checked_sub(1) {
            Some(above) => self.unwrap_rows(above..y),
            None => self.unwrap_scrollback(),
        }
        self.unwrap_rows(y..self.bottom.saturating_add(1));
    }

    /// Ends the wrap of the scrollback's newest line, when the main
    /// screen's first row, its next row, is cleared or replaced.
    fn unwrap_scrollback(&mut self) {
        if self.flags.alternate {
            return;
        }
        if let Some(line) = self.scrollback.newest_mut() {
            line.flags_mut().remove(LineFlags::WRAPPED);
        }
    }

    /// Ends the wrap of each row of `rows`: their text no longer continues
    /// on the next row.
    fn unwrap_rows(&mut self, rows: impl IntoIterator<Item = usize>) {
        for y in rows {
            if let Some(line) = self.line_mut(y) {
                line.flags_mut().remove(LineFlags::WRAPPED);
            }
        }
    }

    /// The cursor's column for an erase: a pending wrap erases the last
    /// column.
    fn erase_column(&self) -> usize {
        self.cursor_x.min(self.cols.saturating_sub(1))
    }

    /// ECH: `n` cells from the cursor.
    pub fn erase_chars(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let x = self.erase_column();
        let (y, end) = (self.cursor_y, x.saturating_add(n).saturating_sub(1));
        self.erase_region((x, y), (end, y), Erase::All);
        self.unwrap_rows(y..y.saturating_add(1));
    }

    /// EL 0: from the cursor to the end of its line.
    pub fn erase_line_right(&mut self, which: Erase) {
        let y = self.cursor_y;
        let x = self.erase_column();
        self.erase_region((x, y), (self.cols, y), which);
        self.unwrap_rows(y..y.saturating_add(1));
    }

    /// EL 1: from the start of the cursor's line to the cursor.
    pub fn erase_line_left(&mut self, which: Erase) {
        let y = self.cursor_y;
        self.erase_region((0, y), (self.cursor_x, y), which);
    }

    /// EL 2: the cursor's line.
    pub fn erase_line(&mut self, which: Erase) {
        let y = self.cursor_y;
        self.erase_region((0, y), (self.cols, y), which);
        self.unwrap_rows(y..y.saturating_add(1));
    }

    /// ED 1: from the top of the screen to the cursor.
    pub fn erase_above(&mut self, which: Erase) {
        let y = self.cursor_y;
        let whole = self.erase_region((0, 0), (self.cursor_x, y), which);
        self.unwrap_rows(whole.into_iter().filter(|&row| row != y));
    }

    /// ED 0: from the cursor to the end of the screen.
    pub fn erase_below(&mut self, which: Erase) {
        let (x, y) = (self.erase_column(), self.cursor_y);
        let whole = self.erase_region((x, y), (self.cols, self.rows), which);
        self.unwrap_rows(whole);
        self.unwrap_rows(y..y.saturating_add(1));
    }

    /// ED 2: the whole screen.
    pub fn erase_screen(&mut self, which: Erase) {
        let whole = self.erase_region((0, 0), (self.cols, self.rows), which);
        self.unwrap_rows(whole);
    }

    /// ICH: `n` blank cells at the cursor, the rest of the line moving
    /// right and the cells pushed past the margin going. A pending wrap
    /// inserts at the last column and ends.
    pub fn insert_chars(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.segment = None;
        self.cursor_x = self.erase_column();
        let (x, y, cols) = (self.cursor_x, self.cursor_y, self.cols);
        let n = n.min(cols.saturating_sub(x));
        let kept = cols.saturating_sub(x).saturating_sub(n);
        // A wide character at the insertion point, where the margin cuts
        // the moved cells, or across the margin itself goes whole.
        self.split_wide(y, x);
        self.split_wide(y, x.saturating_add(kept));
        self.split_wide(y, cols);
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.insert_cells(x, n, cols, &blank);
        }
        self.mark_cells(y, x..cols);
    }

    /// DCH: deletes `n` cells at the cursor, the rest of the line moving
    /// left and blank cells filling in at the margin.
    pub fn delete_chars(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.segment = None;
        self.cursor_x = self.erase_column();
        let (x, y, cols) = (self.cursor_x, self.cursor_y, self.cols);
        let n = n.min(cols.saturating_sub(x));
        // A wide character at either end of the deleted cells, or across
        // the margin, goes whole.
        self.split_wide(y, x);
        self.split_wide(y, x.saturating_add(n));
        self.split_wide(y, cols);
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.delete_cells(x, n, cols, &blank);
        }
        self.unwrap_rows(y..y.saturating_add(1));
        self.mark_cells(y, x..cols);
    }

    /// IL: `n` blank lines at the cursor's row, the rows below it to the
    /// bottom margin moving down and those pushed past it going. Nothing
    /// happens outside the region. The cursor goes to the first column.
    pub fn insert_lines(&mut self, n: usize) {
        let y = self.cursor_y;
        if n == 0 || y < self.top || y > self.bottom {
            return;
        }
        self.segment = None;
        let rows = y..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let gone = self.replace_lines(rows.end.saturating_sub(n)..rows.end);
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_right(n);
        }
        self.unwrap_shifted(y);
        self.mark_rows(rows);
        self.cursor_x = 0;
    }

    /// DL: deletes `n` lines at the cursor's row, the rows below it to the
    /// bottom margin moving up and blank lines filling in at the margin.
    /// Nothing happens outside the region. The cursor goes to the first
    /// column.
    pub fn delete_lines(&mut self, n: usize) {
        let y = self.cursor_y;
        if n == 0 || y < self.top || y > self.bottom {
            return;
        }
        self.segment = None;
        let rows = y..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let gone = self.replace_lines(y..y.saturating_add(n));
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_left(n);
        }
        self.unwrap_shifted(y);
        self.mark_rows(rows);
        self.cursor_x = 0;
    }

    /// SU, and a line feed at the bottom margin: the region moves up `n`
    /// rows. On the main screen the lines leaving the region's top enter
    /// the scrollback, wherever the region starts, as in the C reference,
    /// unwrapped when it starts below the first row; blank lines come in
    /// at the bottom.
    pub fn scroll_up(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.segment = None;
        let rows = self.top..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let gone = self.replace_lines(rows.start..rows.start.saturating_add(n));
        let gone = if self.flags.alternate {
            gone
        } else {
            let below_top = rows.start > 0;
            gone.into_iter()
                .filter_map(|mut line| {
                    if below_top {
                        line.flags_mut().remove(LineFlags::WRAPPED);
                    }
                    self.scrollback.push(line)
                })
                .collect()
        };
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_left(n);
        }
        if let Some(above) = rows.start.checked_sub(1) {
            self.unwrap_rows(above..rows.start);
        }
        self.mark_rows(rows);
    }

    /// SD: the region moves down `n` rows; the lines leaving the bottom go,
    /// and blank lines come in at the top.
    pub fn scroll_down(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.rotate_down(n);
        self.unwrap_shifted(self.top);
    }

    /// Moves the region down `n` rows, blank lines coming in at the top.
    pub(super) fn rotate_down(&mut self, n: usize) {
        self.segment = None;
        let rows = self.top..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let gone = self.replace_lines(rows.end.saturating_sub(n)..rows.end);
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_right(n);
        }
        self.mark_rows(rows);
    }

    /// Replaces the lines of `rows` with new blank lines of the blank
    /// style, and returns the lines replaced, for the scrollback or for
    /// `discard_lines`.
    fn replace_lines(&mut self, rows: Range<usize>) -> Vec<crate::Line> {
        let blank = self.blank_style();
        let cols = self.cols;
        let mut gone = Vec::with_capacity(rows.len());
        for row in rows {
            let line = self.new_line(cols, &blank);
            if let Some(slot) = self.active_mut().get_mut(row) {
                gone.push(mem::replace(slot, line));
            }
        }
        gone
    }
}

/// Whether DECSCA protected a cell of `style` from a selective erase.
fn protected(style: Style) -> bool {
    style.attrs.contains(Attrs::PROTECTED)
}
