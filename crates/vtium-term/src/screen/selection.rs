//! The selection: text the user marks with the pointer to copy, on the
//! screen and in the scrollback, which the host drives.
//!
//! The selection holds each end by its line's [`LineId`] and a column, so
//! it moves with its text as the screen scrolls. A line the screen discards
//! (IL, DL, SD, RI, a scroll on the alternate screen) takes the selection
//! with it, so a copy never reaches into other text. Only an end whose line
//! the scrollback dropped when full leaves the selection standing: copying
//! then starts at the oldest line, as in the C reference.
//! Changing the selection marks everything, since its cells draw inverted.

use core::ops::Range;

use super::Screen;
use crate::cell::Width;
use crate::line::{Glyph, Line, LineId};

/// One end of the selection: a column of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Anchor {
    line: LineId,
    x: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    active: bool,
    /// Where the pointer started.
    begin: Option<Anchor>,
    /// The first and last cells, in reading order.
    start: Option<Anchor>,
    end: Option<Anchor>,
}

impl Selection {
    pub(super) fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether an end of the selection, or where it started, is on one of
    /// `lines`.
    fn holds_any(&self, lines: &[Line]) -> bool {
        self.active
            && [self.begin, self.start, self.end]
                .into_iter()
                .flatten()
                .any(|a| lines.iter().any(|l| l.id() == a.line))
    }
}

impl Screen {
    /// Drops lines the screen discards; one that holds the selection takes
    /// it with it.
    pub(super) fn discard_lines(&mut self, lines: Vec<Line>) {
        if self.selection.holds_any(&lines) {
            self.selection.reset();
            self.damage.mark_all();
        }
    }

    /// Starts a selection at column `x` of view row `y`.
    pub fn select_start(&mut self, x: usize, y: usize) {
        let Some(anchor) = self.anchor(x, y) else {
            return;
        };
        self.selection = Selection {
            active: true,
            begin: Some(anchor),
            start: Some(anchor),
            end: Some(anchor),
        };
        self.damage.mark_all();
    }

    /// Extends the selection from where it started to column `x` of view
    /// row `y`, whichever comes first in reading order.
    pub fn select_to(&mut self, x: usize, y: usize) {
        if !self.selection.active {
            return;
        }
        let Some(target) = self.anchor(x, y) else {
            return;
        };
        let begin = self.selection.begin;
        let (start, end) = if begin.is_some_and(|b| self.before(b, target)) {
            (begin, Some(target))
        } else {
            (Some(target), begin)
        };
        self.selection.start = start;
        self.selection.end = end;
        self.damage.mark_all();
    }

    /// Selects the word at column `x` of view row `y`: the run of cells
    /// around it up to a space, or nothing on a space.
    pub fn select_word(&mut self, x: usize, y: usize) {
        let Some(anchor) = self.anchor(x, y) else {
            return;
        };
        self.damage.mark_all();
        let view = self.view();
        let Some(line) = view.get(y) else {
            return;
        };
        let is_space = |col: usize| line.glyph(line.cell(col)) == Glyph::Char(' ');
        if x >= line.len() || is_space(x) {
            self.selection.reset();
            return;
        }
        let start = (0..=x)
            .rev()
            .find(|&c| is_space(c))
            .map_or(0, |c| c.saturating_add(1));
        let end = (x..line.len())
            .find(|&c| is_space(c) || line.glyph(line.cell(c)) == Glyph::Empty)
            .map_or(line.len(), |c| c.saturating_sub(1));
        self.selection = Selection {
            active: true,
            begin: Some(anchor),
            start: Some(Anchor {
                line: anchor.line,
                x: start,
            }),
            end: Some(Anchor {
                line: anchor.line,
                x: end,
            }),
        };
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selection.reset();
        self.damage.mark_all();
    }

    /// The selected text, each line ended by a newline but the last, each
    /// cell's whole cluster, an empty cell a space, and nothing for the
    /// tail of a wide character; `None` when nothing is selected.
    pub fn selection_text(&self) -> Option<String> {
        let (from, to) = self.selected_span()?;
        let mut out = String::new();
        for (i, line) in self
            .all_lines()
            .enumerate()
            .take(to.0.saturating_add(1))
            .skip(from.0)
        {
            let start = if i == from.0 { from.1 } else { 0 };
            let end = if i == to.0 {
                to.1.saturating_add(1)
            } else {
                self.cols
            };
            let Some(text) = self.line_text(line, start, end) else {
                continue;
            };
            out.push_str(&text);
            out.push('\n');
        }
        out.pop();
        Some(out)
    }

    /// The text of `line` from column `start` to before `end`, up to its
    /// last cell that holds something; `None` when `start` is past that.
    fn line_text(&self, line: &Line, start: usize, end: usize) -> Option<String> {
        let len = line
            .cells()
            .iter()
            .rposition(|c| line.glyph(*c) != Glyph::Empty)
            .map_or(0, |i| i.saturating_add(1))
            .min(self.cols);
        if start > len {
            return None;
        }
        let mut text = String::new();
        for col in start..end.min(len) {
            let cell = line.cell(col);
            match line.glyph(cell) {
                Glyph::Empty if cell.width() == Width::WideTail => {}
                Glyph::Empty => text.push(' '),
                Glyph::Char(c) => text.push(c),
                Glyph::Cluster(s) => text.push_str(s),
            }
        }
        Some(text)
    }

    /// The columns of each view row the selection covers, for the frame.
    pub(super) fn selected_columns(&self, view: &[&Line]) -> Vec<Range<usize>> {
        let Some((from, to)) = self.selected_span() else {
            return Vec::new();
        };
        let base = self
            .scrollback
            .view_index()
            .unwrap_or(self.scrollback.len());
        (0..view.len())
            .map(|row| {
                let i = base.saturating_add(row);
                if i < from.0 || i > to.0 {
                    return 0..0;
                }
                let start = if i == from.0 { from.1 } else { 0 };
                let end = if i == to.0 {
                    to.1.saturating_add(1)
                } else {
                    self.cols
                };
                start..end.min(self.cols)
            })
            .collect()
    }

    /// The selection's first and last cells, each as its line's place in
    /// reading order and a column. A first cell whose line has gone is the
    /// oldest line's first; a last cell whose line has gone is the end of
    /// the screen.
    fn selected_span(&self) -> Option<((usize, usize), (usize, usize))> {
        let sel = &self.selection;
        if !sel.active {
            return None;
        }
        let place = |a: Option<Anchor>| a.and_then(|a| Some((self.place(a.line)?, a.x)));
        let (start, end) = (place(sel.start), place(sel.end));
        if start.is_none() && end.is_none() {
            return None;
        }
        let last = self
            .scrollback
            .len()
            .saturating_add(self.rows)
            .saturating_sub(1);
        Some((start.unwrap_or((0, 0)), end.unwrap_or((last, self.cols))))
    }

    /// The scrollback's lines, then the screen's, in reading order.
    fn all_lines(&self) -> impl Iterator<Item = &Line> + '_ {
        self.scrollback.lines().chain(self.lines())
    }

    /// The place of the line `id` in reading order.
    fn place(&self, id: LineId) -> Option<usize> {
        self.all_lines().position(|l| l.id() == id)
    }

    /// Whether `a` comes before `b` in reading order.
    fn before(&self, a: Anchor, b: Anchor) -> bool {
        if a.line == b.line {
            return a.x < b.x;
        }
        match (self.place(a.line), self.place(b.line)) {
            (Some(pa), Some(pb)) => pa < pb,
            _ => true,
        }
    }

    /// Column `x` of view row `y`, inside the screen.
    fn anchor(&self, x: usize, y: usize) -> Option<Anchor> {
        if x >= self.cols || y >= self.rows {
            return None;
        }
        let line = *self.view().get(y)?;
        Some(Anchor { line: line.id(), x })
    }
}
