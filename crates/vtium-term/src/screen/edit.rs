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
//! A sized run, the block of cells that one OSC 66 sequence writes, is one
//! unit for every edit. An edit that would move, clear or overwrite some of
//! its cells and not the others erases the whole run first, on every row
//! the screen holds, to the blank style. An edit that moves all of its
//! cells together keeps it. ECH, EL and ED erase each run the erased cells
//! touch, and a selective erase spares a run whose style is protected.
//! ICH, DCH, DECIC and DECDC keep a run only when every row of it moves
//! the same way, inside the cells that move. IL, DL and every scroll keep
//! a run only when all of its rows move together and stay inside the
//! region. Rows of a run that scrolled into the scrollback stay there as
//! they are. A line's size change erases the runs on it.
//!
//! References:
//! - ADR-1308 D4, "Erasing a run": the run is cleared on every row the
//!   screen holds, and the scrollback keeps its rows.
//! - ADR-1308 D5: the rule above and how each edit applies it.
//! - ADR-1308 D7: a line's size change erases the runs on it.
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
//! A line of double size (DECDWL, DECDHL) holds half the screen's columns
//! (`LineSize::columns`). ICH, DCH, DECIC and DECDC move its cells only up
//! to its last column. ED returns each line it erases whole to single
//! size, since the VT510 manual's ED says that erased complete lines
//! "become single-height, single-width lines". EL keeps the line's size,
//! since neither the VT510 manual's EL nor the VT100 User Guide's says
//! otherwise. IL, DL and every scroll move a line with its size, and the
//! blank lines they bring in are single size. While DECLRMM is set no line
//! is of double size (`set_line_size`), so the left and right margins
//! never meet one.
//!
//! [`Line::erase`]: crate::Line::erase
//! [`Line::insert_cells`]: crate::Line::insert_cells
//! [`Line::delete_cells`]: crate::Line::delete_cells

use core::mem;
use core::ops::{Range, RangeInclusive};

use super::Screen;
use crate::cell::Width;
use crate::line::{Line, LineFlags, LineSize, SizedRun};
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
    /// A sized run that lies on both sides of the boundary goes whole too,
    /// on every row of it.
    pub(super) fn split_wide(&mut self, y: usize, x: usize) {
        if self.row_has_runs(y) {
            self.erase_run_across(y, x);
        }
        self.split_wide_char(y, x);
    }

    /// The wide character part of [`split_wide`](Self::split_wide): a
    /// wide character across the boundary before cell `x` of row `y` goes
    /// whole, and a sized run there stays.
    pub(super) fn split_wide_char(&mut self, y: usize, x: usize) {
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

    /// Whether row `y` holds a share of any sized run. Each run check of
    /// the print and edit paths asks this first and keeps its work on the
    /// runs in a function of its own, which the compiler leaves out of
    /// line. The paths that print and edit plain text then stay as small
    /// as they are without runs. A screen that OSC 66 never wrote to
    /// answers without reading the row.
    ///
    /// References:
    /// - ADR-1009 D9: a line with no run pays no time for runs.
    #[inline]
    pub(super) fn row_has_runs(&self, y: usize) -> bool {
        self.sized_runs && self.line(y).is_some_and(Line::has_runs)
    }

    /// Whether any row of `top..=bottom` holds a share of a sized run.
    #[inline]
    fn rows_have_runs(&self, top: usize, bottom: usize) -> bool {
        self.sized_runs && (top..=bottom).any(|y| self.row_has_runs(y))
    }

    /// Erases the sized run that lies on both sides of the boundary before
    /// cell `x` of row `y`, if one does, on every row of it.
    #[cold]
    #[inline(never)]
    fn erase_run_across(&mut self, y: usize, x: usize) {
        if let Some(run) = self.line(y).and_then(|l| l.run_at(x))
            && run.col() < x
        {
            self.erase_run(y, run);
        }
    }

    /// The rows of the screen that the sized run whose share on row `y` is
    /// `run` lies on. Rows of it that scrolled into the scrollback, above
    /// the screen's first row, are not among them.
    fn run_rows(&self, y: usize, run: SizedRun) -> Range<usize> {
        let below = run.rows().saturating_sub(run.row());
        y.saturating_sub(run.row())..y.saturating_add(below).min(self.rows)
    }

    /// Erases the whole sized run whose share on row `y` is `run`: every
    /// one of its cells, on every row of it the screen holds, becomes an
    /// empty cell of the blank style.
    pub(super) fn erase_run(&mut self, y: usize, run: SizedRun) {
        for row in self.run_rows(y, run) {
            // The share on `row` belongs to the same run when it is the row
            // of the run that lies that far below or above row `y`.
            let same = self
                .line(row)
                .and_then(|l| l.run_at(run.col()))
                .filter(|r| {
                    r.span() == run.span()
                        && r.size() == run.size()
                        && r.row().saturating_add(y) == run.row().saturating_add(row)
                });
            if same.is_some() {
                self.erase_cells(row, run.span());
            }
        }
    }

    /// Erases each sized run with a cell in `cols` of row `y`, whole.
    #[inline]
    pub(super) fn erase_runs_in(&mut self, y: usize, cols: Range<usize>) {
        if self.row_has_runs(y) {
            self.erase_runs_in_row(y, cols);
        }
    }

    #[cold]
    #[inline(never)]
    fn erase_runs_in_row(&mut self, y: usize, cols: Range<usize>) {
        let runs: Vec<SizedRun> = self.line(y).map_or_else(Vec::new, |l| {
            l.runs()
                .iter()
                .copied()
                .filter(|r| r.col() < cols.end && r.span().end > cols.start)
                .collect()
        });
        for run in runs {
            self.erase_run(y, run);
        }
    }

    /// Before the cells of `x..end` of each row of `rows` move by `n`,
    /// right for an insertion (`insert` true) or left for a deletion,
    /// erases each sized run the move would cut: one across `x` or `end`,
    /// one the deleted cells touch, one pushed partly past `end`, and one
    /// among the moving cells that has a row outside `rows`, since only
    /// some of its rows would move.
    #[inline]
    pub(super) fn erase_runs_shifted(
        &mut self,
        rows: RangeInclusive<usize>,
        x: usize,
        n: usize,
        end: usize,
        insert: bool,
    ) {
        if self.rows_have_runs(*rows.start(), *rows.end()) {
            self.erase_runs_shifted_rows(rows, x, n, end, insert);
        }
    }

    #[cold]
    #[inline(never)]
    fn erase_runs_shifted_rows(
        &mut self,
        rows: RangeInclusive<usize>,
        x: usize,
        n: usize,
        end: usize,
        insert: bool,
    ) {
        let mut cut = Vec::new();
        for y in rows.clone() {
            let Some(line) = self.line(y) else {
                continue;
            };
            for &run in line.runs() {
                let span = run.span();
                if span.end <= x || span.start >= end {
                    continue;
                }
                let broken = span.start < x
                    || span.end > end
                    || if insert {
                        span.end.saturating_add(n) > end
                    } else {
                        span.start < x.saturating_add(n)
                    };
                let r = self.run_rows(y, run);
                let first = y.checked_sub(run.row());
                let whole = first.is_some_and(|f| rows.contains(&f))
                    && r.end.checked_sub(1).is_some_and(|l| rows.contains(&l))
                    && r.len() == run.rows();
                if broken || !whole {
                    cut.push((y, run));
                }
            }
        }
        for (y, run) in cut {
            self.erase_run(y, run);
        }
    }

    /// Before the rows `top..=bottom` move `n` rows up or down, erases
    /// each sized run the move would cut: one with a row outside the
    /// region, one of whose rows only some leave the region, and, with
    /// `cols` the columns that move between the left and right margins,
    /// one that a margin cuts. With `into_scrollback`, the rows that leave
    /// the region's top go into the scrollback, so a run that leaves the
    /// first row in part stays whole there and on the screen.
    #[inline]
    fn erase_runs_moved(
        &mut self,
        (top, bottom): (usize, usize),
        n: usize,
        up: bool,
        into_scrollback: bool,
        cols: Option<Range<usize>>,
    ) {
        if self.rows_have_runs(top, bottom) {
            self.erase_runs_moved_rows((top, bottom), n, up, into_scrollback, cols);
        }
    }

    #[cold]
    #[inline(never)]
    fn erase_runs_moved_rows(
        &mut self,
        (top, bottom): (usize, usize),
        n: usize,
        up: bool,
        into_scrollback: bool,
        cols: Option<Range<usize>>,
    ) {
        let kept_above = up && into_scrollback && top == 0;
        let leaves = |r: usize| {
            if up {
                r < top.saturating_add(n)
            } else {
                r.saturating_add(n) > bottom
            }
        };
        let mut cut = Vec::new();
        for y in top..=bottom.min(self.rows.saturating_sub(1)) {
            let Some(line) = self.line(y) else {
                continue;
            };
            for &run in line.runs() {
                if let Some(c) = &cols {
                    if run.span().end <= c.start || run.col() >= c.end {
                        continue;
                    }
                    if run.col() < c.start || run.span().end > c.end {
                        cut.push((y, run));
                        continue;
                    }
                }
                let first = y.checked_sub(run.row());
                let last = y.saturating_add(run.rows().saturating_sub(1).saturating_sub(run.row()));
                let above = match first {
                    Some(f) => f < top,
                    None => !kept_above,
                };
                if above || last > bottom {
                    cut.push((y, run));
                    continue;
                }
                let lo = first.unwrap_or(0).max(top);
                let some = (lo..=last).any(leaves);
                let all = (lo..=last).all(leaves);
                if some && !all && !(up && into_scrollback) {
                    cut.push((y, run));
                }
            }
        }
        for (y, run) in cut {
            self.erase_run(y, run);
        }
    }

    /// Clears the cells of `cols` in row `y` to the blank style.
    pub(super) fn erase_cells(&mut self, y: usize, cols: Range<usize>) {
        let blank = self.blank_style();
        if let Some(line) = self.line_mut(y) {
            line.erase(cols.clone(), &blank);
        }
        self.mark_cells(y, cols);
    }

    /// How many columns row `y` holds: the screen's, or half of them on a
    /// line of double size.
    pub(super) fn line_cols(&self, y: usize) -> usize {
        self.line(y)
            .map_or(self.cols, |l| l.flags().size().columns(self.cols))
    }

    /// The column after the last that an edit of row `y` moves cells up
    /// to: the right margin's, inside the row's columns.
    pub(super) fn edit_end(&self, y: usize) -> usize {
        self.right.saturating_add(1).min(self.line_cols(y))
    }

    /// DECSWL, DECDWL and DECDHL: the cursor's line takes `size`, and is
    /// drawn again whole. While DECLRMM is set the line stays single size,
    /// as the VT510 manual's DECLRMM says: "The terminal ignores any
    /// sequences to change the line attributes to double width or double
    /// height". A line that becomes double size loses its cells past its
    /// new last column, which no longer show, and its wrap, whose last
    /// column they held. The cursor keeps its column, or moves to the
    /// line's last column when it stood past it, and a pending wrap ends
    /// (VT100 User Guide, DECDWL and DECDHL: "The cursor remains over the
    /// same character position unless it would be to the right of the right
    /// margin, in which case it is moved to the right margin").
    pub(crate) fn set_line_size(&mut self, size: LineSize) {
        if size.is_double() && self.flags.lr_margins {
            return;
        }
        let (cols, y) = (self.cols, self.cursor_y);
        let Some(line) = self.line(y) else {
            return;
        };
        if line.flags().size() != size {
            self.erase_runs_in(y, 0..cols);
        }
        let kept = size.columns(cols);
        if kept < cols {
            self.split_wide(y, kept);
            self.images_cut(y, kept, cols.saturating_sub(1));
            if let Some(line) = self.line_mut(y) {
                line.erase(kept..cols, &Style::DEFAULT);
            }
            self.unwrap_rows(y..y.saturating_add(1));
        }
        if let Some(line) = self.line_mut(y) {
            line.flags_mut().set_size(size);
        }
        self.mark_cells(y, 0..cols);
        let x = self.cursor_x.min(kept.saturating_sub(1));
        self.move_cursor(x, y);
    }

    /// DECLRMM set: every line of both screens returns to single size, as
    /// the VT510 manual's DECLRMM says: "All line attributes currently in
    /// page memory for the session are set to single width, single
    /// height". Each line keeps its cells, and the lines that changed are
    /// drawn again.
    pub(crate) fn single_size_all(&mut self) {
        let rows = self.rows;
        let mut changed = Vec::new();
        for (screen, lines) in [(false, &mut self.main), (true, &mut self.alt)] {
            for (y, line) in lines.iter_mut().enumerate() {
                if line.flags().size().is_double() {
                    // A run on a double-size line is one row high, so
                    // erasing this line's share erases it whole.
                    for run in line.runs().to_vec() {
                        line.erase(run.span(), &Style::DEFAULT);
                    }
                    line.flags_mut().set_size(LineSize::Single);
                    if screen == self.flags.alternate && y < rows {
                        changed.push(y);
                    }
                }
            }
        }
        for y in changed {
            self.mark_cells(y, 0..self.cols);
        }
    }

    /// Returns each row of `rows` to single size: ED's rows erased whole.
    fn single_size(&mut self, rows: &[usize]) {
        for &y in rows {
            if let Some(line) = self.line_mut(y) {
                line.flags_mut().set_size(LineSize::Single);
            }
        }
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
            let end = if y == y_to { x_to } else { last_col };
            self.erase_touched_runs(y, x..end.saturating_add(1), which);
            let Some(line) = self.line(y) else {
                x = 0;
                continue;
            };
            let mut end = end;
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
            // A row is erased whole when the region covers its columns,
            // which on a line of double size end at half the screen.
            if x == 0 && end.saturating_add(1) >= self.line_cols(y) && runs.is_empty() {
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

    /// Erases each sized run with a cell in `cols` of row `y`, whole; a
    /// selective erase (`which` [`Erase::Unprotected`]) spares a run whose
    /// style DECSCA protected.
    fn erase_touched_runs(&mut self, y: usize, cols: Range<usize>, which: Erase) {
        if !self.row_has_runs(y) {
            return;
        }
        let runs: Vec<SizedRun> = self.line(y).map_or_else(Vec::new, |l| {
            l.runs()
                .iter()
                .copied()
                .filter(|r| r.col() < cols.end && r.span().end > cols.start)
                .filter(|r| which == Erase::All || !protected(l.style(l.cell(r.col()))))
                .collect()
        });
        for run in runs {
            self.erase_run(y, run);
        }
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
        self.single_size(&whole);
        // The rows cleared whole take the current paragraph values.
        #[cfg(feature = "bidi")]
        self.give_current(whole.iter().copied().filter(|&row| row != y));
        self.unwrap_rows(whole.into_iter().filter(|&row| row != y));
    }

    /// ED 0: from the cursor to the end of the screen.
    pub fn erase_below(&mut self, which: Erase) {
        let (x, y) = (self.erase_column(), self.cursor_y);
        let whole = self.erase_region((x, y), (self.cols, self.rows), which);
        self.single_size(&whole);
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
        self.single_size(&whole);
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
        let y = self.cursor_y;
        self.erase_runs_shifted(y..=y, x, n, self.edit_end(y), true);
        self.insert_cells_at(y, x, n);
    }

    /// Inserts `n` blank cells at column `x` of row `y`, inside the left
    /// and right margins: the cells from `x` to the right margin move
    /// right, and those pushed past it go.
    fn insert_cells_at(&mut self, y: usize, x: usize, n: usize) {
        let end = self.edit_end(y);
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
        let y = self.cursor_y;
        self.erase_runs_shifted(y..=y, x, n, self.edit_end(y), false);
        self.delete_cells_at(y, x, n);
    }

    /// Deletes `n` cells at column `x` of row `y`, inside the left and
    /// right margins: the cells after them up to the right margin move
    /// left, blank cells fill in at the margin, and the row's wrap ends, as
    /// xterm's `ScrnDeleteChar` ends it.
    fn delete_cells_at(&mut self, y: usize, x: usize, n: usize) {
        let end = self.edit_end(y);
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
        self.erase_runs_shifted(
            self.top..=self.bottom,
            x,
            n,
            self.right.saturating_add(1),
            true,
        );
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
        self.erase_runs_shifted(
            self.top..=self.bottom,
            x,
            n,
            self.right.saturating_add(1),
            false,
        );
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
                let end = self.right.saturating_add(1);
                self.erase_runs_shifted(self.top..=self.bottom, self.left, 1, end, false);
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
        self.erase_runs_moved((y, self.bottom), n, false, false, None);
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
        self.erase_runs_moved((y, self.bottom), n, true, false, None);
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
        self.erase_runs_moved((self.top, self.bottom), n, true, into, None);
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
        let rows = self.bottom.saturating_add(1).saturating_sub(self.top);
        self.erase_runs_moved((self.top, self.bottom), n.min(rows), false, false, None);
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
    /// in any of the rows goes whole first. The pictures over the rectangle
    /// move with it as their rules say (ADR-1100, D6): the part of a sixel
    /// inside it moves, and a kitty placement moves only when it lies
    /// wholly inside it.
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
        self.erase_runs_moved((top, bottom), n, up, false, Some(cols.clone()));
        for y in top..=bottom {
            self.split_wide(y, left);
            self.split_wide(y, cols.end);
        }
        let took = self.images_scroll_in((top, bottom), (left, right), n, up);
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
        self.images_scrolled(took);
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
        // A sized run inside the columns moves with its cells; one that a
        // margin would cut was erased before the move.
        for &run in src.runs() {
            if run.col() >= cols.start && run.span().end <= cols.end {
                dst.mark_run(run);
            }
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
