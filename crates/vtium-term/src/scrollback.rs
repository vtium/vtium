//! The scrollback, and how much it keeps: at most `sb-size` lines, capped
//! by default by the machine's memory or by a cap the configuration sets,
//! and at most `sb-memory` bytes, the oldest lines going past either.

use core::mem;
use core::ops::Range;
use std::collections::VecDeque;

use crate::bounds::{
    LINKS_BYTES_MAX, SB_MEMORY_MAX, SB_MEMORY_MIN, SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM,
    SB_SIZE_CAP_SMALL, SB_SIZE_MAX,
};
use crate::line::{Line, LineFlags, LineId};
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

/// The minspec machine's peak resident size, which the default scrollback
/// is held inside: the operator's round figure (vtium/vtium#131), where
/// the C reference measured 15.7 MiB with its 1,000 lines.
pub const MINSPEC_PEAK: usize = 16 << 20;

/// vtium's resident size after the first screen on the minspec machine,
/// rounded up to a KiB: the C reference's measured 10.3 MiB less its two
/// screens, with vtium's two screens in their place (ADR-1001, "Memory";
/// `tests/memory.rs` derives it).
pub const MINSPEC_FIRST_SCREEN: usize = 10_299 << 10;

/// `sb-memory` when the configuration does not set it: what
/// [`MINSPEC_PEAK`] leaves after [`MINSPEC_FIRST_SCREEN`] and a full link
/// store, rounded down to 64 KiB.
pub const SB_MEMORY_DEFAULT: usize =
    (MINSPEC_PEAK - MINSPEC_FIRST_SCREEN - LINKS_BYTES_MAX) / (64 << 10) * (64 << 10);

const _: () = assert!(SB_MEMORY_MIN <= SB_MEMORY_DEFAULT && SB_MEMORY_DEFAULT <= SB_MEMORY_MAX);

/// The most bytes `sb-memory` may set on a machine with `ram` bytes: a
/// quarter of it, from [`SB_MEMORY_MIN`] to [`SB_MEMORY_MAX`].
pub fn sb_memory_cap(ram: u64) -> usize {
    usize::try_from(ram / 4)
        .unwrap_or(usize::MAX)
        .clamp(SB_MEMORY_MIN, SB_MEMORY_MAX)
}

/// The scrollback's byte budget for a requested `sb-memory` on a machine
/// with `ram` bytes: the request inside its fence, from [`SB_MEMORY_MIN`]
/// to [`sb_memory_cap`].
pub fn sb_memory(requested: usize, ram: u64) -> usize {
    requested.clamp(SB_MEMORY_MIN, sb_memory_cap(ram))
}

/// What a row costs the scrollback's budget: its struct and its heap, by
/// capacity.
fn cost(line: &Line) -> usize {
    size_of::<Line>().saturating_add(line.heap_bytes())
}

/// What `rows` cost the scrollback's budget.
fn charge<'a>(rows: impl IntoIterator<Item = &'a Line>) -> usize {
    rows.into_iter().map(cost).fold(0, usize::saturating_add)
}

/// The lines that scrolled off the top of the main screen, oldest first,
/// and where the view stands among them. It keeps at most `sb-size` rows
/// and `sb-memory` bytes, counting each row it holds, waiting or not, by
/// [`cost`].
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
    /// What every row held costs, and the most it may: `sb-memory`.
    bytes: usize,
    bytes_max: usize,
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

/// What a resize takes from the scrollback to rewrap at once.
pub(crate) struct Taken {
    /// The rows, oldest first.
    pub(crate) rows: Vec<Line>,
    /// Where the rewrapped row at index `i` is among them: at `i + a - b`,
    /// for `(a, b)`.
    pub(crate) offset: (usize, usize),
    /// The first rows continue a line whose head still waits: their first
    /// row takes a new id, and their marks go to the head.
    pub(crate) continues: bool,
}

/// Splits the logical line starting at `start` of `rows` after `head`
/// cells of its text, a row's text ending there or the row holding it cut
/// in two. Returns the index of the first row after the split, and whether
/// a cut row's second part was inserted, moving the rows after it on by
/// one.
fn split_line(rows: &mut VecDeque<Line>, start: usize, head: usize) -> (usize, bool) {
    let mut cells = 0usize;
    let mut at = start;
    while let Some(row) = rows.get_mut(at) {
        let len = reflow::text_len(row);
        let past = head.saturating_sub(cells);
        if past == 0 {
            return (at, false);
        }
        if past < len {
            let tail = row.split_text(past);
            let next = at.saturating_add(1);
            rows.insert(next, tail);
            return (next, true);
        }
        cells = cells.saturating_add(len);
        at = at.saturating_add(1);
    }
    (at, false)
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
            bytes: 0,
            bytes_max: SB_MEMORY_DEFAULT,
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

    /// Every row, waiting and rewrapped, oldest first: what the link store
    /// sweeps.
    pub(crate) fn all_rows(&self) -> impl Iterator<Item = &Line> + '_ {
        self.waiting.rows.iter().chain(self.lines.iter())
    }

    /// What the rows held cost, waiting and rewrapped.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Changes every row, waiting and rewrapped, oldest first, with `f`
    /// until it returns false; the byte count follows what it changed.
    pub(crate) fn rows_mut_while(&mut self, mut f: impl FnMut(&mut Line) -> bool) {
        let rows = self.waiting.rows.iter_mut().chain(self.lines.iter_mut());
        for line in rows {
            let before = cost(line);
            let more = f(line);
            self.bytes = self.bytes.saturating_sub(before).saturating_add(cost(line));
            if !more {
                break;
            }
        }
    }

    /// The nearest rewrapped row above row `top` whose line `is` picks,
    /// rewrapping the waiting rows as the search reaches them; a view
    /// stays on its row.
    pub(crate) fn find_above(&mut self, top: usize, is: impl Fn(&Line) -> bool) -> Option<usize> {
        let mut end = top.min(self.lines.len());
        loop {
            if let Some(i) = self.lines.range(..end).rposition(&is) {
                return Some(i);
            }
            end = self.rewrap_newest();
            if end == 0 {
                return None;
            }
        }
    }

    /// The nearest rewrapped row below row `top` whose line `is` picks.
    pub(crate) fn find_below(&self, top: usize, is: impl Fn(&Line) -> bool) -> Option<usize> {
        let from = top.saturating_add(1);
        let at = self
            .lines
            .range(from.min(self.lines.len())..)
            .position(is)?;
        Some(from.saturating_add(at))
    }

    /// Ends the wrap of the newest row, the newest waiting line rewrapped
    /// first when no other is ([`Line::unwrap`]). Returns whether a pad
    /// at its end became an empty cell.
    pub(crate) fn unwrap_newest(&mut self) -> bool {
        if self.lines.is_empty() {
            self.rewrap_newest();
        }
        let Some(line) = self.lines.back_mut() else {
            return false;
        };
        let before = cost(line);
        let padded = line.unwrap();
        self.bytes = self.bytes.saturating_sub(before).saturating_add(cost(line));
        padded
    }

    /// Keeps at most `max` rows, clamped to the cap; the oldest past it go.
    pub(crate) fn set_max(&mut self, max: usize) {
        self.max = max.min(self.cap);
        self.enforce_max();
    }

    /// Keeps at most `bytes`; the oldest rows past it go.
    pub(crate) fn set_bytes_max(&mut self, bytes: usize) {
        self.bytes_max = bytes;
        self.enforce_max();
    }

    /// Takes a line leaving the top of the screen, trimmed; the oldest rows
    /// go to make room for it. With no room, at `sb-size` 0 or for a line
    /// that alone passes `sb-memory`, it gives the line back.
    pub(crate) fn push(&mut self, mut line: Line) -> Option<Line> {
        if self.max == 0 {
            return Some(line);
        }
        line.trim();
        let c = cost(&line);
        if c > self.bytes_max {
            return Some(line);
        }
        if self.total() >= self.max {
            self.drop_oldest(1);
        }
        while self.bytes.saturating_add(c) > self.bytes_max && self.total() > 0 {
            self.drop_oldest(1);
        }
        self.bytes = self.bytes.saturating_add(c);
        self.lines.push_back(line);
        None
    }

    /// Takes rows a rewrap made, trimmed, as the newest; the oldest past
    /// `sb-size` or `sb-memory` stay until
    /// [`enforce_max`](Self::enforce_max). With no room it gives the rows
    /// back.
    pub(crate) fn extend(&mut self, rows: Vec<Line>) -> Vec<Line> {
        if self.max == 0 {
            return rows;
        }
        self.bytes = self.bytes.saturating_add(charge(&rows));
        self.lines.extend(rows);
        Vec::new()
    }

    /// Drops the oldest rows past `sb-size`, then past `sb-memory`.
    pub(crate) fn enforce_max(&mut self) {
        let over = self.total().saturating_sub(self.max);
        self.drop_oldest(over);
        self.enforce_bytes();
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
    }

    /// Drops the oldest rows past `sb-memory`.
    fn enforce_bytes(&mut self) {
        while self.bytes > self.bytes_max && self.total() > 0 {
            self.drop_oldest(1);
        }
    }

    /// Drops the `n` oldest rows. A waiting logical line dropped whole is
    /// never rewrapped; one cut is rewrapped first. A view on a later line
    /// stays on it, and a view on a dropped line moves to the next.
    fn drop_oldest(&mut self, mut n: usize) {
        let (mut gone, mut made_bytes) = (0usize, 0usize);
        let w = &mut self.waiting;
        while n > 0 && w.count > 0 {
            if w.ready == 0 {
                let (len, rows) = w.oldest();
                gone = gone.saturating_add(charge(w.rows.range(..len)));
                if rows <= n {
                    w.rows.drain(..len);
                    w.count = w.count.saturating_sub(rows);
                    let extra = u64::try_from(rows.saturating_sub(1)).unwrap_or(0);
                    w.ids.start = w.ids.start.saturating_add(extra).min(w.ids.end);
                    n = n.saturating_sub(rows);
                    continue;
                }
                if reflow::breaks_by_length(w.rows.range(..len)) {
                    // The line's first `n` rows hold its first `n` widths
                    // of text: they go unrewrapped. What is left starts
                    // with the row the n-th id was reserved for, and its
                    // marks went with the first row.
                    let (at, inserted) = split_line(&mut w.rows, 0, n.saturating_mul(w.width));
                    w.rows.drain(..at);
                    let kept = len.saturating_add(usize::from(inserted)).saturating_sub(at);
                    made_bytes = made_bytes.saturating_add(charge(w.rows.range(..kept)));
                    let left = line_len(w.rows.iter());
                    for row in w.rows.iter_mut().take(left) {
                        row.flags_mut().remove(LineFlags::MARKS);
                    }
                    let skipped = u64::try_from(n).unwrap_or(u64::MAX);
                    if let Some(first) = w.rows.front_mut() {
                        first.set_id(LineId::new(
                            w.ids.start.saturating_add(skipped).saturating_sub(1),
                        ));
                    }
                    w.ids.start = w.ids.start.saturating_add(skipped).min(w.ids.end);
                    w.count = w.count.saturating_sub(n);
                    n = 0;
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
                made_bytes = made_bytes.saturating_add(charge(&made));
                for row in made.into_iter().rev() {
                    w.rows.push_front(row);
                }
            }
            let k = n.min(w.ready);
            gone = gone.saturating_add(charge(w.rows.range(..k)));
            w.rows.drain(..k);
            w.ready = w.ready.saturating_sub(k);
            w.count = w.count.saturating_sub(k);
            n = n.saturating_sub(k);
        }
        if w.rows.is_empty() {
            *w = Waiting::default();
        }
        let k = n.min(self.lines.len());
        gone = gone.saturating_add(charge(self.lines.range(..k)));
        self.lines.drain(..k);
        self.bytes = self.bytes.saturating_add(made_bytes).saturating_sub(gone);
        self.view = self.view.map(|i| i.saturating_sub(k));
    }

    /// Rewraps the newest waiting logical line, which becomes the oldest
    /// rewrapped one, and drops the oldest rows past `sb-memory`, which
    /// the rewrapped rows may cost more than they did waiting. A view stays
    /// on its row. Returns the rows it made that are left.
    fn rewrap_newest(&mut self) -> usize {
        let w = &mut self.waiting;
        if w.rows.is_empty() {
            return 0;
        }
        let (len, rows) = w.newest();
        let from = w.rows.len().saturating_sub(len);
        let gone = charge(w.rows.range(from..));
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
        self.bytes = self
            .bytes
            .saturating_add(charge(&made))
            .saturating_sub(gone);
        let n = made.len();
        for row in made.into_iter().rev() {
            self.lines.push_front(row);
        }
        self.view = self.view.map(|i| i.saturating_add(n));
        let before = self.lines.len();
        self.enforce_bytes();
        n.saturating_sub(before.saturating_sub(self.lines.len()))
    }

    /// Rewraps every waiting row.
    pub(crate) fn rewrap_all(&mut self) {
        while self.rewrap_newest() > 0 {}
    }

    /// Takes the newest rows for a rewrap at `width`, waiting and rewrapped
    /// alike as one run: whole logical lines, newest first, until they take
    /// at least `margin` rows at `width` and include the rewrapped row at
    /// index `reach`. A line with no wide character breaks at multiples of
    /// the width, so of a longer line only the newest rows needed are
    /// taken, and its head waits. The rows left wait, counted at `width`
    /// with their ids reserved from `next_id`. At a width of one column,
    /// where a wide character becomes an empty cell, every row is taken, as
    /// one that waited would keep it.
    pub(crate) fn take_for_reflow(
        &mut self,
        width: usize,
        margin: usize,
        reach: Option<usize>,
        next_id: &mut u64,
    ) -> Taken {
        self.view = None;
        if width < 2 {
            let before = self.lines.len();
            self.rewrap_all();
            let older = self.lines.len().saturating_sub(before);
            let rows = mem::take(&mut self.lines);
            self.bytes = 0;
            return Taken {
                rows: rows.into(),
                offset: (older, 0),
                continues: false,
            };
        }
        let w = &mut self.waiting;
        w.ready = 0;
        let mut base = w.rows.len();
        w.rows.extend(self.lines.drain(..));
        let reach = reach.map(|r| base.saturating_add(r));
        let mut from = w.rows.len();
        let mut taken = 0usize;
        let mut continues = false;
        while from > 0 && (taken < margin || reach.is_some_and(|r| r < from)) {
            let end = from;
            let mut start = end.saturating_sub(1);
            while start
                .checked_sub(1)
                .and_then(|i| w.rows.get(i))
                .zip(w.rows.get(start))
                .is_some_and(|(before, row)| reflow::joins(before, row))
            {
                start = start.saturating_sub(1);
            }
            let rows = reflow::rows_at(w.rows.range(start..end), width);
            let need = if reach.is_some_and(|r| r < start) {
                rows
            } else {
                let by_reach = reach.map_or(0, |r| {
                    rows.saturating_sub(reflow::row_of(w.rows.range(start..r), width))
                });
                margin.saturating_sub(taken).max(by_reach)
            };
            if need < rows && reflow::breaks_by_length(w.rows.range(start..end)) {
                let head = rows.saturating_sub(need).saturating_mul(width);
                let (at, inserted) = split_line(&mut w.rows, start, head);
                from = at;
                base = base.saturating_add(usize::from(inserted));
                continues = true;
                break;
            }
            from = start;
            taken = taken.saturating_add(rows);
        }
        let rows: Vec<Line> = w.rows.drain(from..).collect();
        w.count(width, next_id);
        if w.rows.is_empty() {
            *w = Waiting::default();
        }
        // Every row left waits, some cut at the new width.
        self.bytes = charge(&w.rows);
        Taken {
            rows,
            offset: (base, from),
            continues,
        }
    }

    /// Adds `marks` to the first row of the newest waiting line, the head
    /// of a line whose newest rows a resize took.
    pub(crate) fn mark_waiting_head(&mut self, marks: LineFlags) {
        if marks == LineFlags::empty() {
            return;
        }
        let (len, _) = self.waiting.newest();
        let first = self.waiting.rows.len().saturating_sub(len);
        if let Some(row) = self.waiting.rows.get_mut(first) {
            row.flags_mut().insert(marks);
        }
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
        self.bytes = self.bytes.saturating_sub(cost(&line));
        if self.view.is_some_and(|i| i >= self.lines.len()) {
            self.view = None;
        }
        Some(line)
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.waiting = Waiting::default();
        self.bytes = 0;
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
    use crate::cell::Width;
    use crate::line::Glyph;
    use crate::style::Style;

    const MIB: u64 = 1 << 20;

    /// A line of `len` cells of text, its id `id`.
    fn text(id: u64, len: usize) -> Line {
        let mut line = Line::new(LineId::new(id), len);
        for x in 0..len {
            line.write(x, Glyph::Char('x'), Width::Narrow, &Style::DEFAULT, None);
        }
        line
    }

    fn ids(sb: &Scrollback) -> Vec<u64> {
        sb.lines().map(|l| l.id().get()).collect()
    }

    // The operator's fence (vtium/vtium#131): from 1 MiB to the lesser of
    // 1 GiB and a quarter of the machine's memory, and never below 1 MiB.
    #[test]
    fn sb_memory_is_fenced_by_its_bounds_and_a_quarter_of_the_memory() {
        assert_eq!(sb_memory_cap(512 * MIB), 128 << 20);
        assert_eq!(sb_memory_cap(4 << 30), 1 << 30);
        assert_eq!(sb_memory_cap(u64::MAX), SB_MEMORY_MAX);
        assert_eq!(sb_memory_cap(0), SB_MEMORY_MIN);
        assert_eq!(sb_memory_cap(4 * MIB - 1), SB_MEMORY_MIN);
        assert_eq!(sb_memory(0, 512 * MIB), SB_MEMORY_MIN);
        assert_eq!(sb_memory(SB_MEMORY_MIN - 1, 512 * MIB), SB_MEMORY_MIN);
        assert_eq!(sb_memory(SB_MEMORY_MIN, 512 * MIB), SB_MEMORY_MIN);
        assert_eq!(sb_memory(SB_MEMORY_DEFAULT, 512 * MIB), SB_MEMORY_DEFAULT);
        assert_eq!(sb_memory(128 << 20, 512 * MIB), 128 << 20);
        assert_eq!(sb_memory((128 << 20) + 1, 512 * MIB), 128 << 20);
        assert_eq!(sb_memory(SB_MEMORY_MAX, 8 << 30), SB_MEMORY_MAX);
        assert_eq!(sb_memory(SB_MEMORY_MAX + 1, 8 << 30), SB_MEMORY_MAX);
        assert_eq!(sb_memory(usize::MAX, 8 << 30), SB_MEMORY_MAX);
    }

    // What 16 MiB leaves after the minspec first screen and a full link
    // store, about 5.69 MiB.
    #[test]
    fn the_default_budget_is_what_the_minspec_peak_leaves() {
        assert_eq!(SB_MEMORY_DEFAULT, 5_963_776);
        let rest = MINSPEC_PEAK - MINSPEC_FIRST_SCREEN - LINKS_BYTES_MAX;
        assert!((SB_MEMORY_DEFAULT..SB_MEMORY_DEFAULT + (64 << 10)).contains(&rest));
    }

    #[test]
    fn the_oldest_rows_go_past_the_byte_budget() {
        let row = cost(&{
            let mut l = text(0, 10);
            l.trim();
            l
        });
        let mut sb = Scrollback::new(100);
        sb.set_max(100);
        sb.set_bytes_max(5 * row);
        for id in 0..10 {
            assert!(sb.push(text(id, 10)).is_none());
            assert!(sb.bytes() <= 5 * row);
            assert_eq!(sb.bytes(), charge(sb.all_rows()));
        }
        assert_eq!(ids(&sb), [5, 6, 7, 8, 9]);
        // A lower budget drops the oldest now, and a view on them moves to
        // the oldest row left; the line cap still holds.
        sb.set_view(Some(1));
        sb.set_bytes_max(2 * row);
        assert_eq!(ids(&sb), [8, 9]);
        assert_eq!(sb.view_index(), Some(0));
        sb.set_bytes_max(5 * row);
        sb.set_max(1);
        assert_eq!(ids(&sb), [9]);
        assert_eq!(sb.bytes(), row);
        sb.clear();
        assert_eq!(sb.bytes(), 0);
    }

    #[test]
    fn a_line_that_alone_passes_the_budget_is_given_back() {
        let mut sb = Scrollback::new(100);
        sb.set_max(100);
        assert!(sb.push(text(0, 4)).is_none());
        let wide = text(1, 400);
        sb.set_bytes_max(cost(&wide) - 1);
        assert!(sb.push(wide).is_some());
        assert_eq!(ids(&sb), [0]);
    }

    #[test]
    fn taking_back_and_unwrapping_keep_the_count() {
        let mut sb = Scrollback::new(100);
        sb.set_max(100);
        for id in 0..4 {
            sb.push(text(id, 7));
        }
        sb.pop_newest();
        assert_eq!(sb.bytes(), charge(sb.all_rows()));
        sb.unwrap_newest();
        sb.rows_mut_while(|l| {
            l.resize(3);
            true
        });
        assert_eq!(sb.bytes(), charge(sb.all_rows()));
    }

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
