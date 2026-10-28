//! The scrollback, and how many lines it keeps: `sb-size`, capped by
//! default by the machine's memory, or by a cap the configuration sets (its
//! key is wired with the configuration, vtium/vtium#21).

use core::mem;
use core::ops::Range;
use std::collections::VecDeque;

use crate::bounds::{SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_MAX};
use crate::line::Line;
use crate::screen::reflow;

/// `sb-size` when the configuration does not set it, as the C reference's
/// `kmscon.conf(5)` gives it.
pub const SB_SIZE_DEFAULT: usize = 1000;

const GIB: u64 = 1 << 30;

/// The most scrollback lines `sb-size` may ask for. By default it follows
/// the machine's `ram`: 10,000 lines below 2 GiB, 30,000 from 2 GiB,
/// 100,000 from 4 GiB. A configured `cap` replaces that default, higher or
/// lower, clamped to [`SB_SIZE_MAX`].
pub fn sb_size_cap(ram: u64, cap: Option<usize>) -> usize {
    match cap {
        Some(cap) => cap.min(SB_SIZE_MAX),
        None if ram >= 4 * GIB => SB_SIZE_CAP_LARGE,
        None if ram >= 2 * GIB => SB_SIZE_CAP_MEDIUM,
        None => SB_SIZE_CAP_SMALL,
    }
}

/// The scrollback lines to keep for a requested `sb-size` on a machine
/// with `ram` bytes and an optional configured `cap`: the request, clamped
/// to the cap.
pub fn sb_size(requested: usize, ram: u64, cap: Option<usize>) -> usize {
    requested.min(sb_size_cap(ram, cap))
}

/// The lines that scrolled off the top of the main screen, oldest first,
/// and where the view stands among them.
///
/// After a resize that changed the width, the oldest rows may wait to be
/// rewrapped (ADR-1003, "Cost"): they are held as they were, whole logical
/// lines, with the rows each takes at the new width counted and the ids of
/// the rows to come reserved. They rewrap when something reaches them: the
/// view, a drop past `sb-size`, a taller screen, the newest line's wrap,
/// or [`rewrap_all`](Self::rewrap_all); the result is the one rewrapping
/// them at the resize gives. Every index here counts the rewrapped rows,
/// which follow the waiting ones.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scrollback {
    /// The rows at the current width, oldest first.
    lines: VecDeque<Line>,
    waiting: Waiting,
    /// The most rows kept: `sb-size`, at most `cap`.
    max: usize,
    cap: usize,
    /// The rewrapped row at the top of the view, or `None` while the view
    /// shows the screen.
    view: Option<usize>,
}

/// The scrollback's oldest rows, waiting to be rewrapped at `width`.
#[derive(Clone, Debug, Default, PartialEq)]
struct Waiting {
    /// The rows as they were, oldest first, whole logical lines.
    rows: VecDeque<Line>,
    /// The rows at the front already rewrapped at `width`, with their ids:
    /// what a drop left of the oldest logical line.
    ready: usize,
    /// The rows `rows` take at `width`.
    count: usize,
    width: usize,
    /// The ids reserved for the rows the rewrap makes after each logical
    /// line's first, in order: the oldest line takes them from the start,
    /// the newest from the end.
    ids: Range<u64>,
}

impl Waiting {
    /// The rows of the oldest logical line not yet rewrapped, and how many
    /// it takes at `width`.
    fn oldest(&self) -> (usize, usize) {
        let rows = self.rows.range(self.ready..);
        let len = line_len(rows.clone());
        (len, reflow::rows_at(rows.take(len), self.width))
    }

    /// The rows of the newest logical line, and how many it takes at
    /// `width`; rows already rewrapped are taken as they are.
    fn newest(&self) -> (usize, usize) {
        let end = self.rows.len();
        if end <= self.ready {
            return (end, end);
        }
        let mut from = end.saturating_sub(1);
        while from > self.ready
            && from
                .checked_sub(1)
                .and_then(|i| self.rows.get(i))
                .zip(self.rows.get(from))
                .is_some_and(|(before, row)| reflow::joins(before, row))
        {
            from = from.saturating_sub(1);
        }
        let len = end.saturating_sub(from);
        (len, reflow::rows_at(self.rows.range(from..), self.width))
    }

    /// Counts the rows each logical line takes at `width` and reserves the
    /// ids of the rows beyond each line's first from `next_id`. A row that
    /// never rewraps is cut at `width` now, as a cut loses its cells.
    fn count(&mut self, width: usize, next_id: &mut u64) {
        self.width = width;
        self.ready = 0;
        for row in &mut self.rows {
            if !reflow::rewraps(row) && row.len() > width {
                row.resize(width);
            }
        }
        let (mut count, mut extra) = (0usize, 0u64);
        let mut from = 0;
        while from < self.rows.len() {
            let len = line_len(self.rows.range(from..));
            let rows = reflow::rows_at(self.rows.range(from..).take(len), width);
            count = count.saturating_add(rows);
            extra = extra.saturating_add(u64::try_from(rows.saturating_sub(1)).unwrap_or(0));
            from = from.saturating_add(len.max(1));
        }
        self.count = count;
        self.ids = *next_id..next_id.wrapping_add(extra);
        *next_id = self.ids.end;
    }
}

/// The rows of the logical line that `rows` starts with.
fn line_len<'a>(mut rows: impl Iterator<Item = &'a Line>) -> usize {
    let Some(mut prev) = rows.next() else {
        return 0;
    };
    let mut len = 1usize;
    for row in rows {
        if !reflow::joins(prev, row) {
            break;
        }
        len = len.saturating_add(1);
        prev = row;
    }
    len
}

impl Scrollback {
    /// An empty scrollback that keeps no lines until `set_max`, which may
    /// ask for at most `cap`, itself at most [`SB_SIZE_MAX`].
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            waiting: Waiting::default(),
            max: 0,
            cap: cap.min(SB_SIZE_MAX),
            view: None,
        }
    }

    /// The rewrapped rows.
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    /// Every row, the waiting ones counted at the current width.
    pub(crate) fn total(&self) -> usize {
        self.waiting.count.saturating_add(self.lines.len())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// The rewrapped rows, oldest first.
    pub(crate) fn lines(&self) -> impl ExactSizeIterator<Item = &Line> + '_ {
        self.lines.iter()
    }

    /// The rewrapped row at `i`, oldest first.
    pub(crate) fn get(&self, i: usize) -> Option<&Line> {
        self.lines.get(i)
    }

    /// The newest row, the newest waiting line rewrapped first when no
    /// other is.
    pub(crate) fn newest_mut(&mut self) -> Option<&mut Line> {
        if self.lines.is_empty() {
            self.rewrap_newest();
        }
        self.lines.back_mut()
    }

    /// Keeps at most `max` rows, clamped to the cap; the oldest past it go.
    pub(crate) fn set_max(&mut self, max: usize) {
        self.max = max.min(self.cap);
        self.enforce_max();
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
    }

    /// Takes a line leaving the top of the screen, trimmed; the oldest row
    /// goes when the scrollback is full. With no room it gives the line
    /// back.
    pub(crate) fn push(&mut self, mut line: Line) -> Option<Line> {
        if self.max == 0 {
            return Some(line);
        }
        if self.total() >= self.max {
            self.drop_oldest(1);
        }
        line.trim();
        self.lines.push_back(line);
        None
    }

    /// Takes rows a rewrap made, trimmed, as the newest; the oldest past
    /// `sb-size` stay until [`enforce_max`](Self::enforce_max). With no
    /// room it gives the rows back.
    pub(crate) fn extend(&mut self, rows: Vec<Line>) -> Vec<Line> {
        if self.max == 0 {
            return rows;
        }
        self.lines.extend(rows);
        Vec::new()
    }

    /// Drops the oldest rows past `sb-size`.
    pub(crate) fn enforce_max(&mut self) {
        let over = self.total().saturating_sub(self.max);
        self.drop_oldest(over);
    }

    /// Drops the `n` oldest rows. A waiting logical line dropped whole is
    /// never rewrapped; one cut is rewrapped first. A view on a later line
    /// stays on it, and a view on a dropped line moves to the next.
    fn drop_oldest(&mut self, mut n: usize) {
        let w = &mut self.waiting;
        while n > 0 && w.count > 0 {
            if w.ready == 0 {
                let (len, rows) = w.oldest();
                if rows <= n {
                    w.rows.drain(..len);
                    w.count = w.count.saturating_sub(rows);
                    let extra = u64::try_from(rows.saturating_sub(1)).unwrap_or(0);
                    w.ids.start = w.ids.start.saturating_add(extra).min(w.ids.end);
                    n = n.saturating_sub(rows);
                    continue;
                }
                let mut next_id = w.ids.start;
                let mut made = Vec::with_capacity(rows);
                reflow::rewrap_line(
                    w.rows.drain(..len),
                    w.width,
                    &mut next_id,
                    &mut [],
                    &mut made,
                );
                w.ids.start = next_id.min(w.ids.end);
                w.ready = made.len();
                for row in made.into_iter().rev() {
                    w.rows.push_front(row);
                }
            }
            let k = n.min(w.ready);
            w.rows.drain(..k);
            w.ready = w.ready.saturating_sub(k);
            w.count = w.count.saturating_sub(k);
            n = n.saturating_sub(k);
        }
        if w.rows.is_empty() {
            *w = Waiting::default();
        }
        let k = n.min(self.lines.len());
        self.lines.drain(..k);
        self.view = self.view.map(|i| i.saturating_sub(k));
    }

    /// Rewraps the newest waiting logical line, which becomes the oldest
    /// rewrapped one. A view stays on its row. Returns the rows it made.
    fn rewrap_newest(&mut self) -> usize {
        let w = &mut self.waiting;
        if w.rows.is_empty() {
            return 0;
        }
        let (len, rows) = w.newest();
        let from = w.rows.len().saturating_sub(len);
        let made = if w.rows.len() <= w.ready {
            // Only rows already rewrapped are left.
            w.rows.drain(..).collect()
        } else {
            let extra = u64::try_from(rows.saturating_sub(1)).unwrap_or(0);
            let mut next_id = w.ids.end.saturating_sub(extra).max(w.ids.start);
            w.ids.end = next_id;
            let mut made = Vec::with_capacity(rows);
            reflow::rewrap_line(
                w.rows.drain(from..),
                w.width,
                &mut next_id,
                &mut [],
                &mut made,
            );
            made
        };
        w.count = w.count.saturating_sub(made.len());
        if w.rows.is_empty() {
            *w = Waiting::default();
        }
        let n = made.len();
        for row in made.into_iter().rev() {
            self.lines.push_front(row);
        }
        self.view = self.view.map(|i| i.saturating_add(n));
        n
    }

    /// Rewraps every waiting row.
    pub(crate) fn rewrap_all(&mut self) {
        while self.rewrap_newest() > 0 {}
    }

    /// Takes the newest rows for a rewrap at `width`: whole logical lines,
    /// newest first, until they take at least `margin` rows at `width` and
    /// include the row at index `reach`. The rows left wait, with what was
    /// waiting, counted at `width` and their ids reserved from `next_id`.
    /// At a width of one column, where a wide character becomes an empty
    /// cell, every row is taken, as one that waited would keep it. Returns
    /// the rows taken, oldest first, and where the rewrapped row at index
    /// `i` is among them: at `i + older - first`, as `(older, first)`.
    pub(crate) fn take_for_reflow(
        &mut self,
        width: usize,
        margin: usize,
        reach: Option<usize>,
        next_id: &mut u64,
    ) -> (Vec<Line>, (usize, usize)) {
        if width < 2 {
            let before = self.lines.len();
            self.rewrap_all();
            let older = self.lines.len().saturating_sub(before);
            let rows = mem::take(&mut self.lines);
            self.view = None;
            return (rows.into(), (older, 0));
        }
        let mut split = self.lines.len();
        let mut taken = 0usize;
        let mut older: VecDeque<Line> = VecDeque::new();
        while taken < margin || reach.is_some_and(|r| r < split) {
            if split > 0 {
                let end = split;
                split = split.saturating_sub(1);
                while let (Some(before), Some(row)) = (
                    split.checked_sub(1).and_then(|i| self.lines.get(i)),
                    self.lines.get(split),
                ) {
                    if !reflow::joins(before, row) {
                        break;
                    }
                    split = split.saturating_sub(1);
                }
                taken = taken.saturating_add(reflow::rows_at(self.lines.range(split..end), width));
                continue;
            }
            // Past the rewrapped rows, the newest waiting line.
            let w = &mut self.waiting;
            let (len, _) = w.newest();
            if len == 0 {
                break;
            }
            let from = w.rows.len().saturating_sub(len);
            taken = taken.saturating_add(reflow::rows_at(w.rows.range(from..), width));
            w.ready = w.ready.min(from);
            for row in w.rows.drain(from..).rev() {
                older.push_front(row);
            }
        }
        let rows = self.lines.split_off(split);
        let rest = mem::take(&mut self.lines);
        self.waiting.rows.extend(rest);
        self.waiting.count(width, next_id);
        if self.waiting.rows.is_empty() {
            self.waiting = Waiting::default();
        }
        self.view = None;
        let before = older.len();
        older.extend(rows);
        (older.into(), (before, split))
    }

    /// Sets the view's top to the rewrapped row at `i`, or the screen.
    pub(crate) fn set_view(&mut self, i: Option<usize>) {
        self.view = i.filter(|&i| i < self.lines.len());
    }

    /// Takes the newest row back, as a taller screen does. A view on it
    /// returns to the screen.
    pub(crate) fn pop_newest(&mut self) -> Option<Line> {
        if self.lines.is_empty() {
            self.rewrap_newest();
        }
        let line = self.lines.pop_back()?;
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
        Some(line)
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.waiting = Waiting::default();
        self.view = None;
    }

    /// How many rows the view is scrolled back.
    pub(crate) fn offset(&self) -> usize {
        self.view.map_or(0, |i| self.lines.len().saturating_sub(i))
    }

    /// The scrollback's rows in the view, top to bottom.
    pub(crate) fn in_view(&self) -> impl Iterator<Item = &Line> + '_ {
        let from = self.view.unwrap_or(self.lines.len());
        self.lines.range(from.min(self.lines.len())..)
    }

    /// The index of the row at the top of the view, or `None` while the
    /// view shows the screen.
    pub(crate) fn view_index(&self) -> Option<usize> {
        self.view
    }

    /// Scrolls the view `n` rows back, rewrapping the waiting rows it
    /// reaches.
    pub(crate) fn view_up(&mut self, n: usize) {
        if n == 0 || self.is_empty() {
            return;
        }
        while self.view.unwrap_or(self.lines.len()) < n && self.rewrap_newest() > 0 {}
        let top = self.view.unwrap_or(self.lines.len());
        self.view = Some(top.saturating_sub(n));
    }

    pub(crate) fn view_down(&mut self, n: usize) {
        if let Some(i) = self.view {
            let i = i.saturating_add(n);
            self.view = (i < self.lines.len()).then_some(i);
        }
    }

    pub(crate) fn reset_view(&mut self) {
        self.view = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_cap_steps_at_2_and_4_gib() {
        assert_eq!(sb_size_cap(512 << 20, None), 10_000);
        assert_eq!(sb_size_cap(2 * GIB - 1, None), 10_000);
        assert_eq!(sb_size_cap(2 * GIB, None), 30_000);
        assert_eq!(sb_size_cap(4 * GIB - 1, None), 30_000);
        assert_eq!(sb_size_cap(4 * GIB, None), 100_000);
        assert_eq!(sb_size_cap(u64::MAX, None), 100_000);
    }

    #[test]
    fn a_request_past_the_cap_is_clamped() {
        assert_eq!(sb_size(SB_SIZE_DEFAULT, 512 << 20, None), 1000);
        assert_eq!(sb_size(50_000, 512 << 20, None), 10_000);
        assert_eq!(sb_size(50_000, 3 * GIB, None), 30_000);
        assert_eq!(sb_size(usize::MAX, 8 * GIB, None), 100_000);
        assert_eq!(sb_size(0, 8 * GIB, None), 0);
    }

    // The operator, 2026-10-26: "10000 lines is fine but it should be
    // configurable". A configured cap wins over the memory's default in
    // both directions, and never passes SB_SIZE_MAX.
    #[test]
    fn a_configured_cap_wins_both_ways_inside_the_hard_bound() {
        assert_eq!(sb_size_cap(512 << 20, Some(50_000)), 50_000);
        assert_eq!(sb_size(50_000, 512 << 20, Some(50_000)), 50_000);
        assert_eq!(sb_size_cap(8 * GIB, Some(2_000)), 2_000);
        assert_eq!(sb_size(50_000, 8 * GIB, Some(2_000)), 2_000);
        assert_eq!(sb_size_cap(512 << 20, Some(0)), 0);
        assert_eq!(sb_size_cap(512 << 20, Some(usize::MAX)), SB_SIZE_MAX);
        assert_eq!(sb_size(usize::MAX, 8 * GIB, Some(usize::MAX)), SB_SIZE_MAX);
    }
}
