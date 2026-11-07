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
//! whatever is printed there next. The rectangle operations keep the same
//! rule (`rect.rs`).
//!
//! The left and right margins bound the editing and scrolling functions as
//! xterm's ctlseqs and the VT510 manual give them, and as xterm runs them
//! (`InsertChar`, `DeleteChar`, `InsertLine`, `DeleteLine`, `xtermScroll`,
//! `RevScroll`, `xtermColScroll`): ICH, DCH, IL and DL do nothing while the
//! cursor is outside the margins; ICH and DCH move the cells between the
//! cursor and the right margin; IL, DL, SU, SD and the scroll a line feed
//! makes move only the cells between the margins, which then do not enter
//! the scrollback; DECIC and DECDC insert and delete columns in the rows
//! between the top and bottom margins. ED, EL and ECH erase whole rows'
//! cells, whatever the margins, as the VT510 manual gives them. A partial
//! row's move ends the wrap of each row it changes the last column of, and
//! of the row above when it changes the first row's first column, the
//! scrollback's newest line's above the screen's first row.
//!
//! [`Line::erase`]: crate::Line::erase
//! [`Line::insert_cells`]: crate::Line::insert_cells
//! [`Line::delete_cells`]: crate::Line::delete_cells

use core::mem;
use core::ops::Range;

use super::Screen;
use crate::cell::Width;
use crate::line::{Line, LineFlags};
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
    /// leave half a character. At the margin there is nothing past the
    /// boundary, since no line keeps a cell past it (ADR-1003).
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
    pub(super) fn erase_region(
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
                if let Some(last) = cols.end.checked_sub(1).filter(|&l| l >= cols.start) {
                    self.images_cut(y, cols.start, last);
                }
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
    pub(super) fn unwrap_scrollback(&mut self) {
        if self.flags.alternate {
            return;
        }
        let padded = self.scrollback.unwrap_newest();
        self.images_dropped();
        // The newest scrollback row is on the screen only while the view is
        // scrolled back.
        if padded && self.scrollback.offset() != 0 {
            self.damage.mark_all();
        }
    }

    /// Ends the wrap of each row of `rows`: their text no longer continues
    /// on the next row. Ending a wrap splits the row's paragraph in two,
    /// and each part is then drawn on its own. In a build with the bidi
    /// feature, when the paragraph draws in another order than it was
    /// written, every row of it is marked changed, since the order of each
    /// part can differ from the order of the whole (ADR-1004 D8).
    pub(super) fn unwrap_rows(&mut self, rows: impl IntoIterator<Item = usize>) {
        for y in rows {
            let cols = self.cols;
            #[cfg(feature = "bidi")]
            let split = self
                .line(y)
                .is_some_and(|l| l.flags().contains(crate::line::LineFlags::WRAPPED))
                .then(|| self.reordered_rows(y));
            if self.line_mut(y).is_some_and(Line::unwrap) {
                self.mark_cells(y, cols.saturating_sub(1)..cols);
            }
            #[cfg(feature = "bidi")]
            for row in split.into_iter().flatten() {
                self.mark_cells(row, 0..cols);
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
        // The rows cleared whole take the current paragraph values.
        #[cfg(feature = "bidi")]
        self.give_current(whole.iter().copied().filter(|&row| row != y));
        self.unwrap_rows(whole.into_iter().filter(|&row| row != y));
    }

    /// ED 0: from the cursor to the end of the screen.
    pub fn erase_below(&mut self, which: Erase) {
        let (x, y) = (self.erase_column(), self.cursor_y);
        let whole = self.erase_region((x, y), (self.cols, self.rows), which);
        // The rows cleared whole take the current paragraph values.
        #[cfg(feature = "bidi")]
        self.give_current(whole.iter().copied().filter(|&row| row != y));
        self.unwrap_rows(whole);
        self.unwrap_rows(y..y.saturating_add(1));
    }

    /// ED 2: the whole screen. The rows it clears whole lose their prompt
    /// marks (ADR-1002, "Prompt marks").
    pub fn erase_screen(&mut self, which: Erase) {
        let held = (which == Erase::All).then(|| self.images_before_erase_screen());
        let whole = self.erase_region((0, 0), (self.cols, self.rows), which);
        if let Some(held) = held {
            self.images_erase_screen(held);
        }
        for &y in &whole {
            if let Some(line) = self.line_mut(y) {
                line.flags_mut().remove(LineFlags::MARKS);
            }
        }
        // The rows cleared whole take the current paragraph values.
        #[cfg(feature = "bidi")]
        self.give_current(whole.iter().copied());
        self.unwrap_rows(whole);
    }

    /// ICH: `n` blank cells at the cursor, the rest of the line up to the
    /// right margin moving right and the cells pushed past the margin
    /// going. A pending wrap inserts at the margin's column and ends.
    /// Outside the left and right margins nothing happens.
    pub fn insert_chars(&mut self, n: usize) {
        let x = self.erase_column();
        if n == 0 || !self.in_lr_margins(x) {
            return;
        }
        self.segment = None;
        self.margin_wrap = false;
        self.cursor_x = x;
        self.insert_cells_at(self.cursor_y, x, n);
    }

    /// Inserts `n` blank cells at column `x` of row `y`, inside the left
    /// and right margins: the cells from `x` to the right margin move
    /// right, and those pushed past it go.
    fn insert_cells_at(&mut self, y: usize, x: usize, n: usize) {
        let end = self.right.saturating_add(1).min(self.cols);
        let n = n.min(end.saturating_sub(x));
        let kept = end.saturating_sub(x).saturating_sub(n);
        // a picture text cuts loses the row from the cursor on: shifting
        // part of it by whole cells would tear it
        self.images_cut(y, x, end.saturating_sub(1));
        // A wide character at the insertion point, where the margin cuts
        // the moved cells, or across the margin itself goes whole.
        self.split_wide(y, x);
        self.split_wide(y, x.saturating_add(kept));
        self.split_wide(y, end);
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.insert_cells(x, n, end, &blank);
        }
        self.mark_cells(y, x..end);
    }

    /// DCH: deletes `n` cells at the cursor, the rest of the line up to
    /// the right margin moving left and blank cells filling in at the
    /// margin. Outside the left and right margins nothing happens.
    pub fn delete_chars(&mut self, n: usize) {
        let x = self.erase_column();
        if n == 0 || !self.in_lr_margins(x) {
            return;
        }
        self.segment = None;
        self.margin_wrap = false;
        self.cursor_x = x;
        self.delete_cells_at(self.cursor_y, x, n);
    }

    /// Deletes `n` cells at column `x` of row `y`, inside the left and
    /// right margins: the cells after them up to the right margin move
    /// left, blank cells fill in at the margin, and the row's wrap ends, as
    /// xterm's `ScrnDeleteChar` ends it.
    fn delete_cells_at(&mut self, y: usize, x: usize, n: usize) {
        let end = self.right.saturating_add(1).min(self.cols);
        let n = n.min(end.saturating_sub(x));
        self.images_cut(y, x, end.saturating_sub(1));
        // A wide character at either end of the deleted cells, or across
        // the margin, goes whole.
        self.split_wide(y, x);
        self.split_wide(y, x.saturating_add(n));
        self.split_wide(y, end);
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.delete_cells(x, n, end, &blank);
        }
        self.unwrap_rows(y..y.saturating_add(1));
        self.mark_cells(y, x..end);
    }

    /// DECIC: `n` blank columns at the cursor's column, in each row from
    /// the top margin to the bottom, the columns up to the right margin
    /// moving right and those pushed past it going. The cursor stays.
    /// Nothing happens while the cursor is outside the margins (xterm's
    /// `xtermColScroll`; VT510 manual, DECIC).
    pub fn insert_columns(&mut self, n: usize) {
        let x = self.erase_column();
        if n == 0 || !self.in_region(x, self.cursor_y) {
            return;
        }
        self.segment = None;
        for y in self.top..=self.bottom {
            self.insert_cells_at(y, x, n);
        }
    }

    /// DECDC: deletes `n` columns at the cursor's column, in each row from
    /// the top margin to the bottom, the columns up to the right margin
    /// moving left and blank columns filling in at the margin. The cursor
    /// stays. Nothing happens while the cursor is outside the margins
    /// (xterm's `xtermColScroll`; VT510 manual, DECDC).
    pub fn delete_columns(&mut self, n: usize) {
        let x = self.erase_column();
        if n == 0 || !self.in_region(x, self.cursor_y) {
            return;
        }
        self.segment = None;
        for y in self.top..=self.bottom {
            self.delete_cells_at(y, x, n);
        }
    }

    /// DECBI: one column left; at the left margin, with the cursor between
    /// the top and bottom margins, the region's columns move right one
    /// instead, a blank column coming in at the left margin and the column
    /// at the right margin going (VT510 manual, DECBI; xterm's
    /// `xtermColIndex`).
    pub fn back_index(&mut self) {
        let x = self.erase_column();
        if x == self.left && self.in_lr_margins(x) {
            self.insert_columns(1);
        } else {
            self.move_left(1);
        }
    }

    /// DECFI: one column right; at the right margin, with the cursor
    /// between the top and bottom margins, the region's columns move left
    /// one instead, a blank column coming in at the right margin and the
    /// column at the left margin going (VT510 manual, DECFI; xterm's
    /// `xtermColIndex`).
    pub fn forward_index(&mut self) {
        let x = self.erase_column();
        if x == self.right && self.in_lr_margins(x) {
            if self.in_region(x, self.cursor_y) {
                self.segment = None;
                for y in self.top..=self.bottom {
                    self.delete_cells_at(y, self.left, 1);
                }
            }
        } else {
            self.move_right(1);
        }
    }

    /// Whether column `x` of row `y` is inside the region: between the top
    /// and bottom margins and between the left and right margins.
    fn in_region(&self, x: usize, y: usize) -> bool {
        y >= self.top && y <= self.bottom && self.in_lr_margins(x)
    }

    /// IL: `n` blank lines at the cursor's row, the rows below it to the
    /// bottom margin moving down and those pushed past it going; between
    /// the left and right margins, only the cells between them move.
    /// Nothing happens outside the region. The cursor goes to the left
    /// margin.
    pub fn insert_lines(&mut self, n: usize) {
        let y = self.cursor_y;
        if n == 0 || !self.in_region(self.cursor_x, y) {
            return;
        }
        if !self.full_width() {
            self.scroll_in_margins(y, n, false);
            self.move_cursor(self.left, y);
            return;
        }
        self.segment = None;
        let rows = y..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let took = self.images_scroll((y, self.bottom), n, false, false);
        let gone = self.replace_lines(rows.end.saturating_sub(n)..rows.end);
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_right(n);
        }
        self.images_scrolled(took);
        self.unwrap_shifted(y);
        self.mark_rows(rows);
        self.margin_wrap = false;
        self.cursor_x = 0;
    }

    /// DL: deletes `n` lines at the cursor's row, the rows below it to the
    /// bottom margin moving up and blank lines filling in at the margin;
    /// between the left and right margins, only the cells between them
    /// move. Nothing happens outside the region. The cursor goes to the
    /// left margin.
    pub fn delete_lines(&mut self, n: usize) {
        let y = self.cursor_y;
        if n == 0 || !self.in_region(self.cursor_x, y) {
            return;
        }
        if !self.full_width() {
            self.scroll_in_margins(y, n, true);
            self.move_cursor(self.left, y);
            return;
        }
        self.segment = None;
        let rows = y..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let took = self.images_scroll((y, self.bottom), n, true, false);
        let gone = self.replace_lines(y..y.saturating_add(n));
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_left(n);
        }
        self.images_scrolled(took);
        self.unwrap_shifted(y);
        self.mark_rows(rows);
        self.margin_wrap = false;
        self.cursor_x = 0;
    }

    /// SU, and a line feed at the bottom margin: the region moves up `n`
    /// rows. On the main screen the lines leaving the region's top enter
    /// the scrollback, wherever the region starts, unwrapped when it
    /// starts below the first row; blank lines come in
    /// at the bottom. Between left and right margins only the cells
    /// between them move, and nothing enters the scrollback.
    pub fn scroll_up(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if !self.full_width() {
            return self.scroll_in_margins(self.top, n, true);
        }
        self.segment = None;
        let rows = self.top..self.bottom.saturating_add(1);
        let n = n.min(rows.len());
        let into = !self.flags.alternate && self.scrollback.keeps_lines();
        let took = self.images_scroll((self.top, self.bottom), n, true, into);
        let gone = self.replace_lines(rows.start..rows.start.saturating_add(n));
        let gone = if self.flags.alternate {
            gone
        } else {
            let below_top = rows.start > 0;
            gone.into_iter()
                .filter_map(|mut line| {
                    // The region's rows are marked below.
                    if below_top {
                        line.unwrap();
                    }
                    self.scrollback.push(line)
                })
                .collect()
        };
        self.discard_lines(gone);
        if let Some(region) = self.active_mut().get_mut(rows.clone()) {
            region.rotate_left(n);
        }
        self.images_scrolled(took);
        if let Some(above) = rows.start.checked_sub(1) {
            self.unwrap_rows(above..rows.start);
        }
        self.mark_rows(rows);
    }

    /// SD: the region moves down `n` rows; the lines leaving the bottom go,
    /// and blank lines come in at the top. Between left and right margins
    /// only the cells between them move.
    pub fn scroll_down(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if !self.full_width() {
            return self.scroll_in_margins(self.top, n, false);
        }
        let took = self.images_scroll((self.top, self.bottom), n, false, false);
        self.rotate_down(n);
        self.images_scrolled(took);
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

    /// Moves the cells between the left and right margins of the rows from
    /// `top` to the bottom margin `n` rows up, or down, as xterm's
    /// `scrollInMargins` does: the cells leaving the rows go, and blank
    /// cells come in at the other end. A wide character across a margin
    /// in any of the rows goes whole first, and a picture over the cells
    /// that move loses them, since a picture moves only with whole lines.
    fn scroll_in_margins(&mut self, top: usize, n: usize, up: bool) {
        self.segment = None;
        let bottom = self.bottom;
        let (left, right) = (self.left, self.right);
        let height = bottom.saturating_add(1).saturating_sub(top);
        let n = n.min(height);
        if n == 0 {
            return;
        }
        let cols = left..right.saturating_add(1);
        for y in top..=bottom {
            self.images_cut(y, left, right);
            self.split_wide(y, left);
            self.split_wide(y, cols.end);
        }
        let kept = height.saturating_sub(n);
        for i in 0..kept {
            let (from, to) = if up {
                (
                    top.saturating_add(i).saturating_add(n),
                    top.saturating_add(i),
                )
            } else {
                (
                    bottom.saturating_sub(i).saturating_sub(n),
                    bottom.saturating_sub(i),
                )
            };
            self.copy_cells(from, to, cols.clone());
        }
        for i in 0..n {
            let y = if up {
                bottom.saturating_sub(i)
            } else {
                top.saturating_add(i)
            };
            self.erase_cells(y, cols.clone());
        }
        self.end_partial_wraps(top, bottom, left, right);
    }

    /// Ends the wraps an edit of the cells `left..=right` of the rows `top`
    /// to `bottom` breaks: each row's when the edit reaches its last
    /// column, and the row above's when it reaches the first row's first
    /// column, the scrollback's newest line's above the screen's first
    /// row.
    fn end_partial_wraps(&mut self, top: usize, bottom: usize, left: usize, right: usize) {
        if left == 0 {
            match top.checked_sub(1) {
                Some(above) => self.unwrap_rows(above..top),
                None => self.unwrap_scrollback(),
            }
        }
        if right.saturating_add(1) >= self.cols {
            self.unwrap_rows(top..bottom.saturating_add(1));
        }
    }

    /// Copies the cells of `cols` of row `from` over those of row `to`,
    /// each with its style and hyperlink. A pad arrives as the empty cell
    /// it draws as, since it belongs only at the end of a wrapped row.
    fn copy_cells(&mut self, from: usize, to: usize, cols: Range<usize>) {
        if from == to || from >= self.rows || to >= self.rows {
            return;
        }
        let lines = self.active_mut();
        let pair = if from < to {
            let (a, b) = lines.split_at_mut(to);
            a.get(from).zip(b.first_mut())
        } else {
            let (a, b) = lines.split_at_mut(from);
            b.first().zip(a.get_mut(to))
        };
        let Some((src, dst)) = pair else {
            return;
        };
        // The target's clusters go before any is written, so a cluster
        // written early does not count against one replaced later.
        dst.erase(cols.clone(), &Style::DEFAULT);
        for x in cols.clone() {
            let cell = src.cell(x);
            let (glyph, width) = match cell.width() {
                Width::Pad => (crate::line::Glyph::Empty, Width::Narrow),
                w => (src.glyph(cell), w),
            };
            dst.write(x, glyph, width, &src.style(cell), src.link(cell));
        }
        self.mark_cells(to, cols);
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
