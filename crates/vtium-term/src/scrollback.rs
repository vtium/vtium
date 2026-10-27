//! The scrollback, and how many lines it keeps: `sb-size`, capped by
//! default by the machine's memory, or by a cap the configuration sets (its
//! key is wired with the configuration, vtium/vtium#21).

use std::collections::VecDeque;

use crate::bounds::{SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_MAX};
use crate::line::Line;

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
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scrollback {
    lines: VecDeque<Line>,
    /// The most lines kept: `sb-size`, at most `cap`.
    max: usize,
    cap: usize,
    /// The scrollback line at the top of the view, or `None` while the view
    /// shows the screen.
    view: Option<usize>,
}

impl Scrollback {
    /// An empty scrollback that keeps no lines until `set_max`, which may
    /// ask for at most `cap`, itself at most [`SB_SIZE_MAX`].
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max: 0,
            cap: cap.min(SB_SIZE_MAX),
            view: None,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub(crate) fn lines(&self) -> impl ExactSizeIterator<Item = &Line> + '_ {
        self.lines.iter()
    }

    /// Keeps at most `max` lines, clamped to the cap; the oldest past it go.
    pub(crate) fn set_max(&mut self, max: usize) {
        self.max = max.min(self.cap);
        while self.lines.len() > self.max {
            self.drop_oldest();
        }
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
    }

    /// Takes a line leaving the top of the screen, trimmed; the oldest line
    /// goes when the scrollback is full. With no room it gives the line
    /// back.
    pub(crate) fn push(&mut self, mut line: Line) -> Option<Line> {
        if self.max == 0 {
            return Some(line);
        }
        if self.lines.len() >= self.max {
            self.drop_oldest();
        }
        line.trim();
        self.lines.push_back(line);
        None
    }

    /// Drops the oldest line. A view on a later line stays on it, and a
    /// view on the dropped line moves to the next.
    fn drop_oldest(&mut self) {
        self.lines.pop_front();
        self.view = self.view.map(|i| i.saturating_sub(1));
    }

    /// Takes the newest line back, as a taller screen does. A view on it
    /// returns to the screen.
    pub(crate) fn pop_newest(&mut self) -> Option<Line> {
        let line = self.lines.pop_back()?;
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
        Some(line)
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.view = None;
    }

    /// How many lines the view is scrolled back.
    pub(crate) fn offset(&self) -> usize {
        self.view.map_or(0, |i| self.lines.len().saturating_sub(i))
    }

    /// The scrollback's lines in the view, top to bottom.
    pub(crate) fn in_view(&self) -> impl Iterator<Item = &Line> + '_ {
        let from = self.view.unwrap_or(self.lines.len());
        self.lines.range(from.min(self.lines.len())..)
    }

    /// The index of the scrollback line at the top of the view, or `None`
    /// while the view shows the screen.
    pub(crate) fn view_index(&self) -> Option<usize> {
        self.view
    }

    pub(crate) fn view_up(&mut self, n: usize) {
        if n == 0 || self.lines.is_empty() {
            return;
        }
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
