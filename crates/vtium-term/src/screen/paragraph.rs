//! The paragraph properties of a build with the bidi feature (ADR-1004
//! D2): whether a paragraph is in implicit or explicit mode, whether it
//! runs left to right or right to left, whether its direction is detected,
//! and whether its box drawing is mirrored.
//!
//! A paragraph is the rows a wrap joins: a row whose text continues on the
//! next row ([`LineFlags::WRAPPED`]) and the row after it belong to one
//! paragraph. Each row holds its paragraph's four properties in its
//! [`LineFlags`], and every row of a paragraph holds the same values.
//!
//! The screen keeps a current value of each property. A new row, one that
//! a line feed, a scroll, an insert or a resize brings onto the screen,
//! takes the current values (`Screen::new_line`). A sequence that changes
//! a property changes its current value and sets it on every row of the
//! cursor's paragraph. A print that wraps carries the paragraph's values to
//! the row it starts, and a rewrap gives every row it makes the values of
//! the paragraph's first row (`reflow.rs`). Drawing by these properties is
//! the frame's work, which a later part of ADR-1004 adds.

use core::ops::Range;

use super::Screen;
use crate::bounds::BIDI_PARAGRAPH_ROWS_MAX;
use crate::line::{Glyph, LineFlags};

impl Screen {
    /// The current values of the paragraph properties, which each new row
    /// takes: [`LineFlags::PARAGRAPH`]'s bits.
    pub fn paragraph(&self) -> LineFlags {
        self.paragraph
    }

    /// Sets the current value of one paragraph property, `flag`, one of
    /// [`LineFlags::PARAGRAPH`]'s bits, and sets it on every row of the
    /// cursor's paragraph, as SM, RM, SCP and SPD do.
    pub(crate) fn set_paragraph_flag(&mut self, flag: LineFlags, on: bool) {
        let mut values = self.paragraph;
        if on {
            values.insert(flag);
        } else {
            values.remove(flag);
        }
        self.set_paragraph(values);
    }

    /// Sets the current values of every paragraph property to `values` and
    /// sets them on every row of the cursor's paragraph, as DECSTR and RIS
    /// do with the defaults.
    pub(crate) fn set_paragraph(&mut self, values: LineFlags) {
        let values = values.intersection(LineFlags::PARAGRAPH);
        self.paragraph = values;
        let rows = self.cursor_paragraph();
        let reached = rows.len();
        for y in rows.clone() {
            if let Some(line) = self.line_mut(y) {
                hold(line.flags_mut(), values);
            }
        }
        // A paragraph whose first rows scrolled off the main screen goes on
        // in the scrollback, the newest row there wrapping into the
        // screen's first row.
        if rows.start == 0 && !self.flags.alternate {
            let most = BIDI_PARAGRAPH_ROWS_MAX.saturating_sub(reached);
            self.scrollback.set_paragraph_above(values, most);
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

    /// The rows of the screen in use that hold the cursor's paragraph: up
    /// from the cursor's row while the row above wraps into it, and down
    /// while a row wraps into the next.
    fn cursor_paragraph(&self) -> Range<usize> {
        let lines = self.lines();
        let y = self.cursor_y.min(lines.len().saturating_sub(1));
        let wraps = |row: usize| {
            lines
                .get(row)
                .is_some_and(|l| l.flags().contains(LineFlags::WRAPPED))
        };
        let mut first = y;
        while first > 0 && wraps(first.saturating_sub(1)) {
            first = first.saturating_sub(1);
        }
        let mut last = y;
        while last.saturating_add(1) < lines.len() && wraps(last) {
            last = last.saturating_add(1);
        }
        first..last.saturating_add(1)
    }
}

/// Replaces the paragraph properties `flags` holds with `values`, keeping
/// its other flags.
pub(crate) fn hold(flags: &mut LineFlags, values: LineFlags) {
    flags.remove(LineFlags::PARAGRAPH);
    flags.insert(values);
}
