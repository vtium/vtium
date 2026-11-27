//! The scrollback, and how much it keeps: at most `sb-size` lines, capped
//! by default by the machine's memory or by a cap the configuration sets,
//! and at most `sb-memory` bytes, the oldest lines going past either.

use core::mem;
#[cfg(feature = "images")]
use std::collections::BTreeMap;
use std::collections::VecDeque;

use crate::bounds::SB_SIZE_MAX;
use crate::line::{Line, LineFlags, LineId};
use crate::screen::reflow;

mod bounds;
mod waiting;

pub use bounds::{
    MINSPEC_GRID, MINSPEC_PEAK, MINSPEC_REST, SB_MEMORY_DEFAULT, SB_MEMORY_DEFAULT_IMAGES,
    SB_MEMORY_DEFAULT_MINSPEC, SB_SIZE_DEFAULT, sb_memory, sb_memory_cap, sb_memory_default,
    sb_size, sb_size_cap, screens_bytes_max,
};
use bounds::{charge, cost};
pub(crate) use bounds::{lines_bytes_max, sb_memory_held};
#[cfg(feature = "images")]
use waiting::log_dropped;
#[cfg(feature = "images")]
pub(crate) use waiting::{Change, Move};
use waiting::{Head, RowsRead, Waiting};

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
    /// What happened to rows image placements may be anchored on, in order,
    /// since the screen last took it: rows dropped from the oldest end, and
    /// anchors a rewrap of waiting rows carried to their new rows.
    #[cfg(feature = "images")]
    log: Vec<Change>,
    /// The columns image placements are anchored at, by row, for the next
    /// [`take_for_reflow`](Self::take_for_reflow) to carry; that call
    /// empties it.
    #[cfg(feature = "images")]
    anchors: BTreeMap<LineId, Vec<u32>>,
    /// The rows of the oldest waiting logical line read to find it and to
    /// cut it, which the drops past `sb-size` and `sb-memory` read.
    rows_read: RowsRead,
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
/// in two. Returns the index of the first row after the split; whether a
/// cut row's second part was inserted, moving the rows after it on by one;
/// and what the rows from `start` to that index cost before the split, the
/// cut row whole.
fn split_line(rows: &mut VecDeque<Line>, start: usize, head: usize) -> (usize, bool, usize) {
    let mut cells = 0usize;
    let mut at = start;
    let mut before = 0usize;
    while let Some(row) = rows.get_mut(at) {
        let len = reflow::text_len(row);
        let past = head.saturating_sub(cells);
        if past == 0 {
            return (at, false, before);
        }
        before = before.saturating_add(cost(row));
        if past < len {
            let tail = row.split_text(past);
            let next = at.saturating_add(1);
            rows.insert(next, tail);
            return (next, true, before);
        }
        cells = cells.saturating_add(len);
        at = at.saturating_add(1);
    }
    (at, false, before)
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
            #[cfg(feature = "images")]
            log: Vec::new(),
            #[cfg(feature = "images")]
            anchors: BTreeMap::new(),
            rows_read: RowsRead::default(),
        }
    }

    /// The rows of the oldest waiting logical line read to find it and to
    /// cut it since the scrollback was made.
    pub(crate) fn rows_read(&self) -> usize {
        self.rows_read.get()
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
        self.waiting.head = Head::default();
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

    /// The most rows it keeps.
    pub(crate) fn max(&self) -> usize {
        self.max
    }

    /// Keeps at most `max` rows, clamped to the cap; the oldest past it go.
    pub(crate) fn set_max(&mut self, max: usize) {
        self.max = max.min(self.cap);
        self.enforce_max();
    }

    /// Keeps at most `bytes`; the oldest rows past it go.
    pub(crate) fn bytes_max(&self) -> usize {
        self.bytes_max
    }

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
        #[cfg(feature = "images")]
        let logged = self.log.len();
        if self.total() >= self.max {
            self.drop_oldest(1);
        }
        while self.bytes.saturating_add(c) > self.bytes_max && self.total() > 0 {
            self.drop_oldest(1);
        }
        // A row dropped with no row after it is followed by this line.
        #[cfg(feature = "images")]
        for change in self.log.iter_mut().skip(logged) {
            if let Change::Dropped {
                next: next @ None, ..
            } = change
            {
                *next = Some(line.id());
            }
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

    /// Drops the oldest rows past `sb-memory`, as many at once as
    /// [`rows_to_free`](Self::rows_to_free) finds, so a long waiting line
    /// is read once a drop and not once a row.
    fn enforce_bytes(&mut self) {
        while self.bytes > self.bytes_max && self.total() > 0 {
            let n = self.rows_to_free(self.bytes.saturating_sub(self.bytes_max));
            self.drop_oldest(n);
        }
    }

    /// How many of the oldest rows to drop to free `excess` bytes, at least
    /// one, reading no further than the oldest logical line still waiting:
    /// rows already rewrapped are counted by what each costs, and of a
    /// waiting line that costs more than `excess`, its share of its rows.
    fn rows_to_free(&self, excess: usize) -> usize {
        let w = &self.waiting;
        let enough = |rows: &mut dyn Iterator<Item = &Line>| {
            let mut freed = 0usize;
            let n = rows
                .take_while(|row| {
                    let more = freed < excess;
                    freed = freed.saturating_add(cost(row));
                    more
                })
                .count();
            n.max(1)
        };
        if w.count == 0 {
            return enough(&mut self.lines.iter());
        }
        if w.ready > 0 {
            return enough(&mut w.rows.range(..w.ready));
        }
        let (len, rows) = w.oldest(&self.rows_read);
        let held = charge(w.rows.range(..len));
        if held <= excess {
            return rows.max(1);
        }
        let share = u128::try_from(excess)
            .unwrap_or(u128::MAX)
            .saturating_mul(u128::try_from(rows).unwrap_or(u128::MAX))
            .div_ceil(u128::try_from(held).unwrap_or(u128::MAX).max(1));
        usize::try_from(share).unwrap_or(rows).clamp(1, rows.max(1))
    }

    /// Drops the `n` oldest rows. A waiting logical line dropped whole is
    /// never rewrapped. One cut that breaks by length loses its first `n`
    /// widths of text as they are, and keeps what the cut left of it as its
    /// [`Head`], so a cut a row at a time reads only the rows it cuts. Any
    /// other line cut is rewrapped first. A view on a later line stays on
    /// it, and a view on a dropped line moves to the next.
    fn drop_oldest(&mut self, mut n: usize) {
        let (mut gone, mut made_bytes) = (0usize, 0usize);
        let w = &mut self.waiting;
        while n > 0 && w.count > 0 {
            if w.ready == 0 {
                let (len, rows) = w.oldest(&self.rows_read);
                let cut_before = mem::take(&mut w.head).0.is_some();
                let extra = u64::try_from(rows.saturating_sub(1)).unwrap_or(0);
                if w.take_fresh(0) {
                    // Held rewrapped already: its rows are ready as they
                    // are, and the ids reserved for them go unused.
                    w.ready = len;
                    w.ids.start = w.ids.start.saturating_add(extra).min(w.ids.end);
                    continue;
                }
                if rows <= n {
                    gone = gone.saturating_add(charge(w.rows.range(..len)));
                    #[cfg(feature = "images")]
                    let dropped: Vec<LineId> = w.rows.range(..len).map(Line::id).collect();
                    w.rows.drain(..len);
                    w.count = w.count.saturating_sub(rows);
                    w.ids.start = w.ids.start.saturating_add(extra).min(w.ids.end);
                    n = n.saturating_sub(rows);
                    #[cfg(feature = "images")]
                    {
                        let next = w.rows.front().or(self.lines.front()).map(Line::id);
                        log_dropped(&mut self.log, &dropped, true, rows, next);
                    }
                    continue;
                }
                if cut_before || reflow::breaks_by_length(w.rows.range(..len)) {
                    // The line's first `n` rows hold its first `n` widths
                    // of text: they go unrewrapped, and the cut reads only
                    // the rows it passes. What is left starts with the row
                    // the n-th id was reserved for, and its marks went with
                    // the first row, cleared once, at the line's first cut.
                    let (at, inserted, cut) = split_line(&mut w.rows, 0, n.saturating_mul(w.width));
                    self.rows_read.add(at);
                    gone = gone.saturating_add(cut);
                    #[cfg(feature = "images")]
                    let dropped: Vec<LineId> = w.rows.range(..at).map(Line::id).collect();
                    w.rows.drain(..at);
                    if inserted {
                        made_bytes = made_bytes.saturating_add(w.rows.front().map_or(0, cost));
                    }
                    let left = len.saturating_add(usize::from(inserted)).saturating_sub(at);
                    if !cut_before {
                        self.rows_read.add(left);
                        for row in w.rows.iter_mut().take(left) {
                            row.flags_mut().remove(LineFlags::MARKS);
                        }
                    }
                    let skipped = u64::try_from(n).unwrap_or(u64::MAX);
                    if let Some(first) = w.rows.front_mut() {
                        first.set_id(LineId::new(
                            w.ids.start.saturating_add(skipped).saturating_sub(1),
                        ));
                    }
                    w.ids.start = w.ids.start.saturating_add(skipped).min(w.ids.end);
                    w.count = w.count.saturating_sub(n);
                    w.head = Head(Some((left, rows.saturating_sub(n))));
                    #[cfg(feature = "images")]
                    {
                        let next = w.rows.front().map(Line::id);
                        log_dropped(&mut self.log, &dropped, true, n, next);
                    }
                    n = 0;
                    continue;
                }
                gone = gone.saturating_add(charge(w.rows.range(..len)));
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
            #[cfg(feature = "images")]
            let dropped: Vec<LineId> = w.rows.range(..k).map(Line::id).collect();
            w.rows.drain(..k);
            w.ready = w.ready.saturating_sub(k);
            w.count = w.count.saturating_sub(k);
            n = n.saturating_sub(k);
            #[cfg(feature = "images")]
            {
                let next = w.rows.front().or(self.lines.front()).map(Line::id);
                log_dropped(&mut self.log, &dropped, false, 1, next);
            }
        }
        if w.rows.is_empty() {
            *w = Waiting::default();
        }
        let k = n.min(self.lines.len());
        gone = gone.saturating_add(charge(self.lines.range(..k)));
        #[cfg(feature = "images")]
        let dropped: Vec<LineId> = self.lines.range(..k).map(Line::id).collect();
        self.lines.drain(..k);
        #[cfg(feature = "images")]
        {
            let next = self.lines.front().map(Line::id);
            log_dropped(&mut self.log, &dropped, false, 1, next);
        }
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
        w.head = Head::default();
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
            if w.take_fresh(from) {
                // Held rewrapped already: its rows are taken as they are,
                // and the ids reserved for them go unused.
                w.rows.drain(from..).collect()
            } else {
                let mut made = Vec::with_capacity(rows);
                reflow::rewrap_line(
                    w.rows.drain(from..),
                    w.width,
                    &mut next_id,
                    &mut [],
                    &mut made,
                );
                made
            }
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
    ///
    /// A logical line an image placement is anchored on, by the anchors
    /// [`set_anchors`](Self::set_anchors) gave, is taken whole or left to
    /// wait whole, and one left to wait is rewrapped at `width` at once,
    /// its anchors carried. An anchor past a row's text or on a row that
    /// does not rewrap lands where each width in turn puts it, so carrying
    /// it straight from an older width to the newest could land it
    /// elsewhere (ADR-1003, "Images").
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
            #[cfg(feature = "images")]
            self.anchors.clear();
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
            #[cfg(feature = "images")]
            let anchored = w
                .rows
                .range(start..end)
                .any(|l| self.anchors.contains_key(&l.id()));
            #[cfg(not(feature = "images"))]
            let anchored = false;
            if need < rows && !anchored && reflow::breaks_by_length(w.rows.range(start..end)) {
                let head = rows.saturating_sub(need).saturating_mul(width);
                let (at, inserted, _) = split_line(&mut w.rows, start, head);
                from = at;
                base = base.saturating_add(usize::from(inserted));
                continues = true;
                break;
            }
            from = start;
            taken = taken.saturating_add(rows);
        }
        let rows: Vec<Line> = w.rows.drain(from..).collect();
        #[cfg(feature = "images")]
        {
            w.fresh.clear();
            let anchors = mem::take(&mut self.anchors);
            // The rows anchors are on that wait: the search ends once it
            // has found them all, or at the newest row when one is on no
            // row left.
            let held = |l: &Line| anchors.contains_key(&l.id());
            let mut left = anchors
                .len()
                .saturating_sub(rows.iter().filter(|l| held(l)).count());
            let mut at = 0;
            while left > 0 && at < w.rows.len() {
                let len = line_len(w.rows.range(at..));
                let end = at.saturating_add(len.max(1));
                let found = w.rows.range(at..end).filter(|l| held(l)).count();
                if found == 0 {
                    at = end;
                    continue;
                }
                left = left.saturating_sub(found);
                let cols = |id| anchors.get(&id).cloned().unwrap_or_default();
                let (end, moves) = w.rewrap_at(at, width, next_id, cols);
                self.log.push(Change::Carried(moves));
                at = end;
            }
        }
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
        self.waiting.head = Head::default();
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

    /// What happened to rows since the last call, in order.
    #[cfg(feature = "images")]
    pub(crate) fn take_changes(&mut self) -> Vec<Change> {
        mem::take(&mut self.log)
    }

    /// Gives the columns image placements are anchored at, by scrollback
    /// row, for the next [`take_for_reflow`](Self::take_for_reflow) to
    /// carry; it stops looking once it has found every row given.
    #[cfg(feature = "images")]
    pub(crate) fn set_anchors(&mut self, anchors: BTreeMap<LineId, Vec<u32>>) {
        self.anchors = anchors;
    }

    /// Image placements, anchored at columns `cols`, went on to row `id`
    /// as the rows above it dropped. When `id` waits on a logical line not
    /// held rewrapped, that line is rewrapped at the width now, as a
    /// rewrap at each resize would have it, and held so. The line's first
    /// row keeps its id and the placements on it their column; anchors on
    /// a later row are carried. Returns where each carried anchor went.
    #[cfg(feature = "images")]
    pub(crate) fn note_anchor(&mut self, id: LineId, cols: &[u32], next_id: &mut u64) -> Vec<Move> {
        let w = &mut self.waiting;
        let Some(i) = w.rows.range(w.ready..).position(|l| l.id() == id) else {
            return Vec::new();
        };
        let i = i.saturating_add(w.ready);
        let mut at = i;
        while at > w.ready
            && at
                .checked_sub(1)
                .and_then(|b| w.rows.get(b))
                .zip(w.rows.get(at))
                .is_some_and(|(before, row)| reflow::joins(before, row))
        {
            at = at.saturating_sub(1);
        }
        if w.rows.get(at).is_some_and(|l| w.fresh.contains(&l.id())) {
            return Vec::new();
        }
        let len = line_len(w.rows.range(at..));
        let before = charge(w.rows.range(at..at.saturating_add(len)));
        let first = i == at;
        let carry = |row| {
            if row == id && !first {
                cols.to_vec()
            } else {
                Vec::new()
            }
        };
        let (end, moves) = w.rewrap_at(at, w.width, next_id, carry);
        self.bytes = self
            .bytes
            .saturating_sub(before)
            .saturating_add(charge(w.rows.range(at..end)));
        moves
    }

    /// Whether a line leaving the screen's top enters the scrollback.
    pub(crate) fn keeps_lines(&self) -> bool {
        self.max > 0
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
mod tests;
