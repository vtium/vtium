//! The selection: text the user marks with the pointer to copy, on the
//! screen and in the scrollback, which the host drives.
//!
//! The selection holds each end by its line's [`LineId`] and a column, so
//! it moves with its text as the screen scrolls, and a resize that rewraps
//! the text carries each end to the cell now holding it (ADR-1003). A line the screen discards
//! (IL, DL, SD, RI, a scroll on the alternate screen, a scroll with no
//! room in the scrollback, the hidden rows a taller screen drops) takes the
//! selection with it, so a copy never reaches into other text. Only an end
//! whose line left the scrollback (dropped when it was full, or emptied by
//! ED 3) leaves the selection standing: copying then starts at the oldest
//! line, as in the C reference.
//! A line selection's copy ends with a line end, as xterm's does with
//! `cutNewline`, its default.
//! A line that wrapped is one line to the selection, as in xterm: a word
//! goes on across the wrap, a line selection takes each row of it, and the
//! copy has no line end where it wrapped. The alternate screen's first row
//! does not continue the scrollback, which holds the main screen's lines.
//! Changing the selection marks everything, since its cells draw inverted.

use core::ops::Range;

use super::Screen;
use crate::cell::Width;
use crate::line::{Glyph, Line, LineFlags, LineId};

/// One end of the selection: a column of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Anchor {
    pub(super) line: LineId,
    pub(super) x: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    active: bool,
    /// Where the pointer started.
    begin: Option<Anchor>,
    /// The first and last cells, in reading order.
    start: Option<Anchor>,
    end: Option<Anchor>,
    /// A whole line, whose copy ends with a line end.
    line: bool,
}

impl Selection {
    pub(super) fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Where the selection started and its ends, while it is active.
    pub(super) fn anchors(&self) -> Vec<Anchor> {
        if !self.active {
            return Vec::new();
        }
        let mut anchors: Vec<Anchor> = Vec::with_capacity(3);
        for a in [self.begin, self.start, self.end].into_iter().flatten() {
            if !anchors.contains(&a) {
                anchors.push(a);
            }
        }
        anchors
    }

    /// Moves each anchor of `moved` to where a rewrap carried it; one whose
    /// cell went takes the selection with it.
    pub(super) fn carry(&mut self, moved: &[(Anchor, Option<Anchor>)]) {
        let mut gone = false;
        for end in [&mut self.begin, &mut self.start, &mut self.end] {
            let Some(a) = *end else {
                continue;
            };
            if let Some(&(_, to)) = moved.iter().find(|(from, _)| *from == a) {
                *end = to;
                gone |= to.is_none();
            }
        }
        if gone {
            self.reset();
        }
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
            line: false,
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
    /// around it up to a space, or nothing on a space. Going right, an
    /// empty cell ends it too, and a wide character is one cell of it. A
    /// word that reaches the end of a wrapped row goes on at the start of
    /// the next.
    pub fn select_word(&mut self, x: usize, y: usize) {
        let Some(anchor) = self.anchor(x, y) else {
            return;
        };
        self.damage.mark_all();
        let at = self.view_place(y);
        let Some(line) = self.line_at(at) else {
            return;
        };
        if x >= line.len() || is_space(line, x) {
            self.selection.reset();
            return;
        }
        let (first, start) = self.word_start(at, x);
        let (last, end) = self.word_end(at, x);
        self.selection = Selection {
            active: true,
            begin: Some(anchor),
            start: self.anchor_at(first, start),
            end: self.anchor_at(last, end),
            line: false,
        };
    }

    /// Where the word holding column `x` of the line at `at` starts: after
    /// the space before it, on this row or a row it wrapped from. A wrapped
    /// row's empty cells at its end are passed over, as `word_end` passes
    /// them, and a wrapped row with no text ends the word, as it does
    /// going forward.
    fn word_start(&self, mut at: usize, x: usize) -> (usize, usize) {
        let Some(line) = self.line_at(at) else {
            return (at, x);
        };
        if let Some(c) = (0..=x).rev().find(|&c| is_space(line, c)) {
            return (at, c.saturating_add(1));
        }
        while let Some(prev) = at.checked_sub(1).filter(|&p| self.continues(p)) {
            let Some(line) = self.line_at(prev) else {
                break;
            };
            let len = text_len(line).min(self.cols);
            match (0..len).rev().find(|&c| is_space(line, c)) {
                _ if len == 0 => break,
                Some(c) if c.saturating_add(1) == len => break,
                Some(c) => return (prev, c.saturating_add(1)),
                None => at = prev,
            }
        }
        (at, 0)
    }

    /// Where the word holding column `x` of the line at `at` ends: before
    /// the space or empty cell after it, on this row or a row it wrapped
    /// onto. A wrapped row's empty cells at its end, which a wide character
    /// that wrapped whole leaves, do not end the word.
    fn word_end(&self, mut at: usize, mut x: usize) -> (usize, usize) {
        loop {
            let Some(line) = self.line_at(at) else {
                return (at, x);
            };
            let len = line.len().min(self.cols);
            let stop = (x..len).find(|&c| ends_word(line, c));
            let runs_on = self.continues(at)
                && stop.is_none_or(|c| (c..len).all(|c| line.glyph(line.cell(c)) == Glyph::Empty))
                && self
                    .line_at(at.saturating_add(1))
                    .is_some_and(|next| !ends_word(next, 0));
            if !runs_on {
                let end = stop.map_or(len, |c| c.max(x.saturating_add(1)));
                return (at, end.saturating_sub(1));
            }
            at = at.saturating_add(1);
            x = 0;
        }
    }

    /// Selects the whole line at view row `y`, `x` any column of it: each
    /// row of it, when it wrapped.
    pub fn select_line(&mut self, x: usize, y: usize) {
        let Some(anchor) = self.anchor(x, y) else {
            return;
        };
        let mut first = self.view_place(y);
        while let Some(prev) = first.checked_sub(1).filter(|&p| self.continues(p)) {
            first = prev;
        }
        let mut last = self.view_place(y);
        while self.continues(last) {
            last = last.saturating_add(1);
        }
        self.selection = Selection {
            active: true,
            begin: Some(anchor),
            start: self.anchor_at(first, 0),
            end: self.anchor_at(last, self.cols.saturating_sub(1)),
            line: true,
        };
        self.damage.mark_all();
    }

    /// Whether anything is selected.
    pub fn has_selection(&self) -> bool {
        self.selection.active
    }

    /// Clears the selection; with none, nothing changes.
    pub fn clear_selection(&mut self) {
        if self.selection.active {
            self.selection.reset();
            self.damage.mark_all();
        }
    }

    /// The selected text, each line ended by a newline but a row that
    /// wrapped and the last of a selection of characters, each cell's whole
    /// cluster, an empty cell a space, and nothing for the tail of a wide
    /// character; `None` when nothing is selected.
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
            if i == to.0 || !self.continues(i) {
                out.push('\n');
            }
        }
        if !self.selection.line {
            out.pop();
        }
        Some(out)
    }

    /// The text of `line` from column `start` to before `end`, up to its
    /// last cell that holds something; `None` when `start` is past that.
    fn line_text(&self, line: &Line, start: usize, end: usize) -> Option<String> {
        let len = text_len(line).min(self.cols);
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

    /// The line at `place` in reading order.
    fn line_at(&self, place: usize) -> Option<&Line> {
        match place.checked_sub(self.scrollback.len()) {
            None => self.scrollback.get(place),
            Some(row) => self.lines().get(row),
        }
    }

    /// The place in reading order of view row `y`.
    fn view_place(&self, y: usize) -> usize {
        self.scrollback
            .view_index()
            .unwrap_or(self.scrollback.len())
            .saturating_add(y)
    }

    /// Whether the line at `place` in reading order continues on the next:
    /// it wrapped, and the next is not the alternate screen's first row.
    fn continues(&self, place: usize) -> bool {
        let next = place.saturating_add(1);
        let wrapped = self
            .line_at(place)
            .is_some_and(|l| l.flags().contains(LineFlags::WRAPPED));
        wrapped
            && self.line_at(next).is_some()
            && !(self.flags.alternate && next == self.scrollback.len())
    }

    /// Column `x` of the line at `place` in reading order.
    fn anchor_at(&self, place: usize, x: usize) -> Option<Anchor> {
        let line = self.line_at(place)?;
        Some(Anchor { line: line.id(), x })
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

/// The cells of `line` up to its last that holds something.
fn text_len(line: &Line) -> usize {
    line.cells()
        .iter()
        .rposition(|c| line.glyph(*c) != Glyph::Empty)
        .map_or(0, |i| i.saturating_add(1))
}

/// Whether column `x` of `line` holds a space.
fn is_space(line: &Line, x: usize) -> bool {
    line.glyph(line.cell(x)) == Glyph::Char(' ')
}

/// Whether column `x` of `line` ends a word going right: a space, or an
/// empty cell that is not a wide character's tail.
fn ends_word(line: &Line, x: usize) -> bool {
    let cell = line.cell(x);
    is_space(line, x) || (line.glyph(cell) == Glyph::Empty && cell.width() != Width::WideTail)
}
