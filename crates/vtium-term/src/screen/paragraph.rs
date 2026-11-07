//! The paragraph properties of a build with the bidi feature (ADR-1004
//! D2): whether a paragraph is in implicit or explicit mode, whether it
//! runs left to right or right to left, whether its direction is detected,
//! and whether its box drawing is mirrored.
//!
//! A paragraph is the rows a wrap joins: a row whose text continues on the
//! next row ([`LineFlags::WRAPPED`]) and the row after it belong to one
//! paragraph. Each row holds its paragraph's four properties in its
//! [`LineFlags`].
//!
//! The screen keeps a current value of each property, and the Terminal
//! Working Group's recommendation, which ADR-1004 follows, says where it
//! goes. A sequence that changes a property changes its current value, and
//! sets it on every row of the cursor's paragraph only when the cursor
//! stands at the paragraph's first logical position, the first column of
//! its first row. A line feed into a new paragraph, ED's cleared rows and
//! every new row a scroll, an insert or a resize brings take the current
//! values (`Screen::new_line`). A print that wraps carries the paragraph's
//! values to the row it starts, and a rewrap gives every row it makes the
//! values of the paragraph's first row (`reflow.rs`). A row whose
//! properties change is marked changed, since its drawn order may change
//! with them (ADR-1004 D8).
//!
//! The screen draws by these properties through [`Screen::order`], which
//! finds each paragraph the view shows and has [`crate::order`] order its
//! rows, and maps the column under the pointer back to the logical column
//! it shows ([`Screen::row_order`]).

use core::ops::Range;

use super::Screen;
use crate::bounds::BIDI_PARAGRAPH_ROWS_MAX;
use crate::line::{Glyph, Line, LineFlags};
use crate::order::{self, Order, RowOrder};

impl Screen {
    /// The current values of the paragraph properties, which each new row
    /// takes: [`LineFlags::PARAGRAPH`]'s bits.
    pub fn paragraph(&self) -> LineFlags {
        self.paragraph
    }

    /// Sets the current value of one paragraph property, `flag`, one of
    /// [`LineFlags::PARAGRAPH`]'s bits, as SM, RM, SCP and SPD do. At the
    /// first logical position of the cursor's paragraph it sets the value
    /// on every row of that paragraph too, and leaves its other properties
    /// as they are.
    pub(crate) fn set_paragraph_flag(&mut self, flag: LineFlags, on: bool) {
        let flag = flag.intersection(LineFlags::PARAGRAPH);
        if on {
            self.paragraph.insert(flag);
        } else {
            self.paragraph.remove(flag);
        }
        if let Some(rows) = self.paragraph_at_cursor() {
            for y in rows {
                let Some(mut values) = self.line(y).map(|l| l.flags()) else {
                    continue;
                };
                if on {
                    values.insert(flag);
                } else {
                    values.remove(flag);
                }
                self.hold_row(y, values);
            }
        }
    }

    /// Sets the current values of every paragraph property to `values`, as
    /// DECSTR and RIS do with the defaults, and sets them on every row of
    /// the cursor's paragraph when the cursor stands at its first logical
    /// position, as SM and RM would.
    pub(crate) fn set_paragraph(&mut self, values: LineFlags) {
        self.paragraph = values.intersection(LineFlags::PARAGRAPH);
        if let Some(rows) = self.paragraph_at_cursor() {
            self.give_current(rows);
        }
    }

    /// Sets the current values of every paragraph property to `values` and
    /// sets them on every row of both screens that holds no text, as a
    /// change of `text-direction` does (ADR-1004 D5).
    pub(crate) fn set_unwritten_paragraphs(&mut self, values: LineFlags) {
        let values = values.intersection(LineFlags::PARAGRAPH);
        self.paragraph = values;
        let mut changed = false;
        for line in self.main.iter_mut().chain(self.alt.iter_mut()) {
            if line.cells().iter().all(|&c| line.glyph(c) == Glyph::Empty) {
                changed |= line.flags().intersection(LineFlags::PARAGRAPH) != values;
                hold(line.flags_mut(), values);
            }
        }
        if changed {
            self.damage.mark_all();
        }
    }

    /// Row `y`, where a line feed just put the cursor: the paragraph that
    /// starts there takes the current values, and a row the row above wraps
    /// into, the same paragraph, takes that paragraph's values.
    pub(super) fn line_fed(&mut self, y: usize) {
        let continues = y
            .checked_sub(1)
            .and_then(|a| self.line(a))
            .is_some_and(|l| l.flags().contains(LineFlags::WRAPPED));
        if continues {
            self.carry_paragraph(y);
        } else {
            self.give_current(self.paragraph_from(y));
        }
    }

    /// Gives each row of `rows` the current values: the rows ED clears
    /// whole, and the rows of a paragraph a line feed or a reset reaches.
    pub(super) fn give_current(&mut self, rows: impl IntoIterator<Item = usize>) {
        let values = self.paragraph;
        for y in rows {
            self.hold_row(y, values);
        }
    }

    /// Gives row `y`, where a print's wrap just put the cursor, the
    /// paragraph values of the row above it, which wraps into it, and so
    /// every row the paragraph now holds below it.
    pub(super) fn carry_paragraph(&mut self, y: usize) {
        let Some(above) = y.checked_sub(1).and_then(|a| self.line(a)) else {
            return;
        };
        if !above.flags().contains(LineFlags::WRAPPED) {
            return;
        }
        let values = above.flags().intersection(LineFlags::PARAGRAPH);
        let mut row = y;
        while let Some(flags) = self.line(row).map(Line::flags) {
            if flags.intersection(LineFlags::PARAGRAPH) == values {
                break;
            }
            self.hold_row(row, values);
            if !flags.contains(LineFlags::WRAPPED) {
                break;
            }
            row = row.saturating_add(1);
        }
    }

    /// The rows of the screen's paragraph that holds row `y`, when that
    /// paragraph draws in another order than it was written: it runs right
    /// to left, or one of its rows has held a right-to-left or joining
    /// character. Empty otherwise.
    pub(super) fn reordered_rows(&self, y: usize) -> Range<usize> {
        let mut first = y;
        while let Some(above) = first.checked_sub(1)
            && self
                .line(above)
                .is_some_and(|l| l.flags().contains(LineFlags::WRAPPED))
        {
            first = above;
        }
        let rows = self.paragraph_from(first);
        let reorders = rows
            .clone()
            .filter_map(|row| self.line(row))
            .any(|l| l.flags().contains(LineFlags::RTL) || l.flags().contains(LineFlags::REORDERS));
        if reorders { rows } else { y..y }
    }

    /// Gives row `y` the paragraph properties `values`, and marks it
    /// changed when they differ from those it held, since its drawn order
    /// may change with them (ADR-1004 D8).
    fn hold_row(&mut self, y: usize, values: LineFlags) {
        let values = values.intersection(LineFlags::PARAGRAPH);
        let cols = self.cols;
        let Some(line) = self.line_mut(y) else {
            return;
        };
        if line.flags().intersection(LineFlags::PARAGRAPH) == values {
            return;
        }
        hold(line.flags_mut(), values);
        self.mark_cells(y, 0..cols);
    }

    /// The rows of the cursor's paragraph when the cursor stands at its
    /// first logical position: the first column of a row that no row above
    /// wraps into. The screen's first row starts a paragraph for this, even
    /// where the scrollback's newest row wraps into it.
    fn paragraph_at_cursor(&self) -> Option<Range<usize>> {
        let y = self.cursor_y;
        let starts = self.cursor_x == 0
            && y.checked_sub(1)
                .and_then(|a| self.line(a))
                .is_none_or(|l| !l.flags().contains(LineFlags::WRAPPED));
        starts.then(|| self.paragraph_from(y))
    }

    /// The rows of the screen in use from row `y` down while a row wraps
    /// into the next.
    fn paragraph_from(&self, y: usize) -> Range<usize> {
        let lines = self.lines();
        let mut last = y;
        while last.saturating_add(1) < lines.len()
            && lines
                .get(last)
                .is_some_and(|l| l.flags().contains(LineFlags::WRAPPED))
        {
            last = last.saturating_add(1);
        }
        y..last.saturating_add(1)
    }
}

impl Screen {
    /// The order each row of the view is drawn in: each paragraph with a
    /// row in view that runs right to left, or that holds a row that has
    /// held a right-to-left or joining character, ordered once, every row
    /// of it in view taking its row's order. The bidirectional algorithm
    /// reads the paragraph whole, its rows above and below the view
    /// included, up to [`BIDI_PARAGRAPH_ROWS_MAX`] rows, once for each
    /// character's level; rule L1 and the drawing order then cost only the
    /// rows the view shows (ADR-1004 D3, D8).
    pub(crate) fn order(&self) -> Order {
        let rows = self.view().len();
        let base = self.view_place(0);
        let view = base..base.saturating_add(rows);
        let mut out: Vec<Option<RowOrder>> = vec![None; rows];
        let mut y = 0;
        while y < rows {
            let place = base.saturating_add(y);
            let (first, last, orders) = self.paragraph_orders(place, view.clone());
            let next = last
                .saturating_add(1)
                .saturating_sub(base)
                .max(y.saturating_add(1));
            let shown = first.max(base).saturating_sub(base)..next.min(rows);
            for (p, mut row) in orders.into_iter().flatten() {
                row.paragraph = shown.clone();
                if let Some(slot) = p.checked_sub(base).and_then(|at| out.get_mut(at)) {
                    *slot = Some(row);
                }
            }
            y = next;
        }
        Order::new(out)
    }

    /// The order of view row `y` alone, reading its whole paragraph, or
    /// `None` when it draws in logical order.
    pub(super) fn row_order(&self, y: usize) -> Option<RowOrder> {
        self.place_order(self.view_place(y))
    }

    /// Whether the cursor's paragraph resolves right to left: it runs
    /// right to left, or its direction is detected and its first strong
    /// character is right to left (ADR-1004 D11).
    pub(crate) fn cursor_paragraph_rtl(&self) -> bool {
        let place = self.scrollback.len().saturating_add(self.cursor_y);
        self.place_order(place).is_some_and(|r| r.rtl)
    }

    /// The order of the line at `place` in reading order alone,
    /// as [`row_order`](Self::row_order) gives a view row's.
    pub(super) fn place_order(&self, place: usize) -> Option<RowOrder> {
        let wanted = place..place.saturating_add(1);
        let (_, _, orders) = self.paragraph_orders(place, wanted);
        orders?.into_iter().next().map(|(_, row)| row)
    }

    /// The paragraph that holds the line at `place` in reading order: its
    /// first and last places, and the order of each of its rows whose place
    /// is in `wanted`, each with its place, or `None` when it draws in
    /// logical order. A paragraph of more than [`BIDI_PARAGRAPH_ROWS_MAX`]
    /// rows is drawn as in explicit mode, so it gives the line at `place`
    /// alone, ordered as explicit.
    fn paragraph_orders(
        &self,
        place: usize,
        wanted: Range<usize>,
    ) -> (usize, usize, Option<Vec<(usize, RowOrder)>>) {
        // walk at most one row past the bound each way, which tells a
        // paragraph of more than BIDI_PARAGRAPH_ROWS_MAX rows from one of
        // exactly that many
        let mut first = place;
        while place.saturating_sub(first) < BIDI_PARAGRAPH_ROWS_MAX
            && let Some(above) = first.checked_sub(1)
            && self.continues(above)
        {
            first = above;
        }
        let mut last = place;
        while last.saturating_sub(first) < BIDI_PARAGRAPH_ROWS_MAX && self.continues(last) {
            last = last.saturating_add(1);
        }
        let too_long = last.saturating_sub(first) >= BIDI_PARAGRAPH_ROWS_MAX;
        let (first, last) = if too_long {
            (place, place)
        } else {
            (first, last)
        };
        let lines: Vec<&Line> = (first..=last).filter_map(|p| self.line_at(p)).collect();
        let Some(head) = lines.first() else {
            return (first, last, None);
        };
        let draws_in_order = !head.flags().contains(LineFlags::RTL)
            && !lines
                .iter()
                .any(|l| l.flags().contains(LineFlags::REORDERS));
        if draws_in_order {
            return (first, last, None);
        }
        let rows = wanted.start.max(first).saturating_sub(first)
            ..wanted.end.min(last.saturating_add(1)).saturating_sub(first);
        let orders = order::paragraph(&lines, self.cols, too_long, rows)
            .into_iter()
            .map(|(i, row)| (first.saturating_add(i), row))
            .collect();
        (first, last, Some(orders))
    }
}

/// Replaces the paragraph properties `flags` holds with `values`, keeping
/// its other flags.
pub(crate) fn hold(flags: &mut LineFlags, values: LineFlags) {
    flags.remove(LineFlags::PARAGRAPH);
    flags.insert(values);
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use crate::Terminal;
    use crate::bounds::BIDI_PARAGRAPH_ROWS_MAX;
    use crate::order::rows_built;
    use crate::unicode::bidi::levels::chars_read;

    /// What `f` costs: the characters the algorithm reads and the rows
    /// ordered.
    fn cost(f: impl FnOnce()) -> (usize, usize) {
        let (chars, rows) = (chars_read(), rows_built());
        f();
        (chars_read() - chars, rows_built() - rows)
    }

    // ADR-1004 D3, D8: a frame reads each character of a drawn paragraph
    // once for its level, and each character of a drawn row once more for
    // rule L1, and orders only the rows it draws. A row that read the whole
    // paragraph again would cost the paragraph's length a row.
    #[test]
    fn a_frame_reads_a_paragraph_once_and_each_drawn_row_once_more() {
        let (cols, rows) = (40, 100);
        let mut t = Terminal::new(cols, rows, 0);
        let n = cols * rows - 1;
        t.input("\u{05D0}".repeat(n).as_bytes());
        let mut order = None;
        assert_eq!(cost(|| order = Some(t.screen().order())), (2 * n, rows));
        assert_eq!(order.unwrap().rows().count(), rows);
        // the pointer orders its own row: the paragraph's levels and its row
        assert_eq!(cost(|| _ = t.screen().pointer_col(3, 7)), (n + cols, 1));
    }

    // ADR-1004 D8: a paragraph that runs into the scrollback is read whole
    // for its levels, and only the rows the view shows are ordered.
    #[test]
    fn rows_above_the_view_are_read_but_not_ordered() {
        let (cols, rows, paragraph) = (40, 10, 30);
        let mut t = Terminal::new(cols, rows, 1000);
        t.screen_mut().set_scrollback_max(1000);
        let n = cols * paragraph - 1;
        t.input("\u{05D0}".repeat(n).as_bytes());
        // the last row shows 39 letters and the cursor
        let shown = cols * rows - 1;
        assert_eq!(cost(|| _ = t.screen().order()), (n + shown, rows));
    }

    /// Each row of the view as drawn, a character a cell.
    fn drawn(t: &Terminal) -> Vec<String> {
        let order = t.screen().order();
        let screen = t.screen();
        (0..screen.rows())
            .map(|y| {
                let line = screen.line(y).unwrap();
                (0..screen.cols())
                    .map(|v| {
                        let col = order.row(y).map_or(v, |r| r.logical_at(v));
                        match line.glyph(line.cell(col)) {
                            crate::line::Glyph::Char(c) => c,
                            _ => '.',
                        }
                    })
                    .collect()
            })
            .collect()
    }

    // ADR-1004 D3: a paragraph of BIDI_PARAGRAPH_ROWS_MAX rows is resolved,
    // and one of a row more is drawn as explicit, as written in its
    // left-to-right direction, with nothing read.
    #[test]
    fn a_paragraph_of_more_than_the_bound_draws_as_explicit() {
        for (rows, read, row) in [
            (BIDI_PARAGRAPH_ROWS_MAX, true, "\u{05D1}\u{05D0}"),
            (BIDI_PARAGRAPH_ROWS_MAX + 1, false, "\u{05D0}\u{05D1}"),
        ] {
            let mut t = Terminal::new(2, rows, 0);
            t.input("\u{05D0}\u{05D1}".repeat(rows).as_bytes());
            let (chars, _) = cost(|| _ = t.screen().order());
            assert_eq!(chars > 0, read, "{rows} rows");
            let d = drawn(&t);
            assert_eq!(
                (d[0].as_str(), d[rows - 1].as_str()),
                (row, row),
                "{rows} rows"
            );
        }
    }
}
