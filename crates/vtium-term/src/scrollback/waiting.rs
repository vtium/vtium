//! The scrollback's oldest rows, waiting to be rewrapped after a width
//! change, the count of rows read to find them, and with images, the record
//! of what happened to the rows a placement may be anchored on.

use core::ops::Range;
#[cfg(feature = "images")]
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::line_len;
use crate::line::Line;
#[cfg(feature = "images")]
use crate::line::LineId;
use crate::screen::reflow;

/// A count of rows read, which a test reads to check that a drop reads
/// each row a bounded number of times. It counts through a shared
/// reference, as [`Scrollback::rows_to_free`] reads, and two scrollbacks
/// compare equal whatever their counts.
#[derive(Debug, Default)]
pub(super) struct RowsRead(AtomicUsize);

impl RowsRead {
    pub(super) fn add(&self, rows: usize) {
        self.0.fetch_add(rows, Ordering::Relaxed);
    }

    pub(super) fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl Clone for RowsRead {
    fn clone(&self) -> Self {
        RowsRead(AtomicUsize::new(self.get()))
    }
}

impl PartialEq for RowsRead {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The scrollback's oldest rows, waiting to be rewrapped at `width`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Waiting {
    /// The rows as they were, oldest first, whole logical lines.
    pub(super) rows: VecDeque<Line>,
    /// The rows at the front already rewrapped at `width`, with their ids:
    /// what a drop left of the oldest logical line.
    pub(super) ready: usize,
    /// The rows `rows` take at `width`.
    pub(super) count: usize,
    pub(super) width: usize,
    /// The ids reserved for the rows the rewrap makes after each logical
    /// line's first, in order: the oldest line takes them from the start,
    /// the newest from the end.
    pub(super) ids: Range<u64>,
    /// The first rows of the logical lines held already rewrapped at
    /// `width`, by id. An image placement anchored on a waiting line keeps
    /// it rewrapped at each width, as a rewrap at each resize would, and
    /// something that reaches such a line takes its rows as they are
    /// (ADR-1003, "Images").
    #[cfg(feature = "images")]
    pub(super) fresh: BTreeSet<LineId>,
    /// What a cut by length left of the oldest logical line, while nothing
    /// else has changed the rows since.
    pub(super) head: Head,
}

/// The rows of the oldest waiting logical line and the rows it takes at
/// the width, kept after a cut by length so that the next cut need not
/// read the line again. A line kept here breaks by length, its marks are
/// cleared, and no row before it is ready. Anything else that changes the
/// waiting rows forgets it. It is derived from the rows, so two waiting
/// sets compare equal whether or not they keep it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Head(pub(super) Option<(usize, usize)>);

impl PartialEq for Head {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Waiting {
    /// The rows of the oldest logical line not yet rewrapped, and how many
    /// it takes at `width`, with the line's rows counted in `read` when
    /// they are read and not kept from a cut.
    pub(super) fn oldest(&self, read: &RowsRead) -> (usize, usize) {
        if let Some(kept) = self.head.0.filter(|_| self.ready == 0) {
            return kept;
        }
        let rows = self.rows.range(self.ready..);
        let len = line_len(rows.clone());
        read.add(len);
        (len, reflow::rows_at(rows.take(len), self.width))
    }

    /// The rows of the newest logical line, and how many it takes at
    /// `width`; rows already rewrapped are taken as they are.
    pub(super) fn newest(&self) -> (usize, usize) {
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
    pub(super) fn count(&mut self, width: usize, next_id: &mut u64) {
        self.width = width;
        self.ready = 0;
        self.head = Head::default();
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

    /// Whether the logical line that starts at row `at` is held rewrapped
    /// at `width`; it is then no longer recorded as held so.
    pub(super) fn take_fresh(&mut self, at: usize) -> bool {
        #[cfg(feature = "images")]
        {
            let Some(id) = self.rows.get(at).map(Line::id) else {
                return false;
            };
            self.fresh.remove(&id)
        }
        #[cfg(not(feature = "images"))]
        {
            let _ = at;
            false
        }
    }

    /// Rewraps the logical line that starts at row `at` in place at
    /// `width`, the rows after its first taking their ids from `next_id`,
    /// and records it as held rewrapped. Each anchor `cols` gives for one
    /// of its rows is carried. Returns the index of the row after the line,
    /// and where each anchor went.
    #[cfg(feature = "images")]
    pub(super) fn rewrap_at(
        &mut self,
        at: usize,
        width: usize,
        next_id: &mut u64,
        cols: impl Fn(LineId) -> Vec<u32>,
    ) -> (usize, Vec<Move>) {
        self.head = Head::default();
        let len = line_len(self.rows.range(at..));
        let end = at.saturating_add(len);
        let (mut places, from) = anchor_places(self.rows.range(at..end), cols);
        let mut made = Vec::with_capacity(len);
        reflow::rewrap_line(
            self.rows.drain(at..end),
            width,
            next_id,
            &mut places,
            &mut made,
        );
        let moves = moves(&places, &from, &made);
        if let Some(first) = made.first() {
            self.fresh.insert(first.id());
        }
        let end = at.saturating_add(made.len());
        for (i, row) in made.into_iter().enumerate() {
            self.rows.insert(at.saturating_add(i), row);
        }
        (end, moves)
    }
}

/// What happened to rows an image placement may be anchored on.
#[cfg(feature = "images")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Row `line` left the scrollback's oldest end, standing for `rows`
    /// rows at the width: a waiting line dropped whole as it was held
    /// stands for the rows it takes at the width, and every other row for
    /// one. The placements anchored on it lose that many of their rows and
    /// go on from `next`, the oldest row left after it, or the line a push
    /// appends after it; with neither, from the screen's first row.
    Dropped {
        line: LineId,
        rows: usize,
        next: Option<LineId>,
    },
    /// A rewrap of waiting rows carried these anchors, all at once.
    Carried(Vec<Move>),
}

/// A rewrap carried the cell at `col` of row `from` to `to_col` of row
/// `to`.
#[cfg(feature = "images")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub(crate) from: LineId,
    pub(crate) col: u32,
    pub(crate) to: LineId,
    pub(crate) to_col: u32,
}

/// The places of the anchors `cols` gives on `rows`, which are about to
/// rewrap, each with the row and column it starts from.
#[cfg(feature = "images")]
fn anchor_places<'a>(
    rows: impl Iterator<Item = &'a Line>,
    cols: impl Fn(LineId) -> Vec<u32>,
) -> (Vec<reflow::Place>, Vec<(LineId, u32)>) {
    let mut places = Vec::new();
    let mut from = Vec::new();
    for (i, row) in rows.enumerate() {
        for col in cols(row.id()) {
            places.push(reflow::Place::at(
                i,
                usize::try_from(col).unwrap_or(usize::MAX),
            ));
            from.push((row.id(), col));
        }
    }
    (places, from)
}

/// Where the rewrap that made `made` carried each anchor of `from`.
#[cfg(feature = "images")]
fn moves(places: &[reflow::Place], from: &[(LineId, u32)], made: &[Line]) -> Vec<Move> {
    places
        .iter()
        .zip(from)
        .filter_map(|(place, &(id, col))| {
            let row = made.get(place.row)?;
            Some(Move {
                from: id,
                col,
                to: row.id(),
                to_col: u32::try_from(place.col).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

/// Logs the rows `ids` dropped from the oldest end, `next` being the
/// oldest row left. Rows already rewrapped (`line` false) each stand for
/// one row and go on to the next of them. The rows of one logical line
/// held as it was (`line` true) go on to `next` together, its first row
/// standing for the `rows` rows the line takes at the width.
#[cfg(feature = "images")]
pub(super) fn log_dropped(
    log: &mut Vec<Change>,
    ids: &[LineId],
    line: bool,
    rows: usize,
    next: Option<LineId>,
) {
    for (i, &id) in ids.iter().enumerate() {
        let (rows, next) = match line {
            true if i == 0 => (rows, next),
            true => (1, next),
            false => (1, ids.get(i.saturating_add(1)).copied().or(next)),
        };
        log.push(Change::Dropped {
            line: id,
            rows,
            next,
        });
    }
}
