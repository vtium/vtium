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
//! values of the paragraph's first row (`reflow.rs`). Drawing by these
//! properties is the frame's work, which a later part of ADR-1004 adds.

use core::ops::Range;

use super::Screen;
use crate::line::{Glyph, LineFlags};

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
                if let Some(line) = self.line_mut(y) {
                    if on {
                        line.flags_mut().insert(flag);
                    } else {
                        line.flags_mut().remove(flag);
                    }
                }
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
        for line in self.main.iter_mut().chain(self.alt.iter_mut()) {
            if line.cells().iter().all(|&c| line.glyph(c) == Glyph::Empty) {
                hold(line.flags_mut(), values);
            }
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
            if let Some(line) = self.line_mut(y) {
                hold(line.flags_mut(), values);
            }
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
        while let Some(line) = self.line_mut(row) {
            if line.flags().intersection(LineFlags::PARAGRAPH) == values {
                break;
            }
            hold(line.flags_mut(), values);
            if !line.flags().contains(LineFlags::WRAPPED) {
                break;
            }
            row = row.saturating_add(1);
        }
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

/// Replaces the paragraph properties `flags` holds with `values`, keeping
/// its other flags.
pub(crate) fn hold(flags: &mut LineFlags, values: LineFlags) {
    flags.remove(LineFlags::PARAGRAPH);
    flags.insert(values);
}
