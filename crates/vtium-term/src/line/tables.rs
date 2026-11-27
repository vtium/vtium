//! A line's tables: the styles, links and sized runs its cells index, the
//! cluster arena that holds its grapheme clusters, and the map a rewrap
//! copies indices through.

use super::is_placeholder;
use super::runs::SizedRun;
use crate::bounds::{CLUSTER_BYTES_MAX, COLUMNS_MAX};
use crate::cell::{ClusterOffset, Content};
use crate::style::Style;

/// A line's tables, allocated on its first styled, linked or clustered
/// cell, or its first sized run. Index `i` of a style or link names entry
/// `i - 1`; index 0 is the default style or no link.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Tables {
    pub(super) styles: Vec<Style>,
    /// Each link's number in the link store, in 16 bits: the store holds
    /// fewer than 65,536 links ([`crate::links`]).
    ///
    /// References:
    /// - ADR-1009 D5: the line's link table keeps 16-bit numbers.
    pub(super) links: Vec<u16>,
    /// The share of each sized run that crosses the line, ordered by
    /// column, the runs apart from each other.
    pub(super) runs: RunTable,
    /// Each cluster as a length byte and its codepoints in UTF-8, with the
    /// counts of what the line's cells name.
    pub(super) clusters: ClusterArena,
}

// A line's tables are as large with the run table as they were without
// it: 80 bytes on a 64-bit target and 44 on a 32-bit one. The cluster
// arena and its counts take one pointer's width less than a vector and two
// 32-bit counts did, which pays for the run table's pointer.
// `Line::bytes_max` counts this size, so the memory budget's figures hold
// only while it stays here.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Tables>() == 80 && size_of::<ClusterArena>() == 24);
#[cfg(target_pointer_width = "32")]
const _: () = assert!(size_of::<Tables>() == 44 && size_of::<ClusterArena>() == 16);

/// How many offsets a cell can name: a cell holds its cluster's offset in
/// 21 bits, so the arena refuses a cluster that would start at this offset
/// or past it.
pub(super) const ARENA_OFFSETS: usize = 1 << 21;

// The arena of the widest row a rewrap can build, every cell naming a
// cluster of the most bytes, stays inside the offsets a cell can name. A
// row that `write` fills compacts well before that, as `bounds.rs` checks.
// The arena's length and its count of named bytes are kept in 24 bits,
// which hold the most the arena takes: its last offset and one cluster of
// up to 256 bytes with its length byte.
const _: () = assert!(COLUMNS_MAX * CLUSTER_BYTES_MAX <= ARENA_OFFSETS);
const _: () = assert!(ARENA_OFFSETS + 256 <= U24::MAX);
// A line has at most `COLUMNS_MAX` cells, so its count of the cells that
// name a placeholder cluster fits in 16 bits.
const _: () = assert!(COLUMNS_MAX <= u16::MAX as usize);

/// A count kept in three bytes, from 0 to 2^24 - 1. A larger value is
/// stored as the largest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct U24([u8; 3]);

impl U24 {
    pub(super) const MAX: usize = (1 << 24) - 1;

    pub(super) fn get(self) -> usize {
        let [a, b, c] = self.0;
        usize::from(a) | (usize::from(b) << 8) | (usize::from(c) << 16)
    }

    /// `n`, or the largest count when `n` is larger.
    pub(super) fn saturating(n: usize) -> Self {
        let n = u32::try_from(n.min(Self::MAX)).unwrap_or(u32::MAX);
        let [a, b, c, _] = n.to_le_bytes();
        Self([a, b, c])
    }
}

/// A line's cluster arena: each cluster as a length byte and its
/// codepoints in UTF-8, one after another, with the counts the line's
/// cluster budget reads.
///
/// The arena keeps its bytes in a boxed slice whose length is the arena's
/// capacity, and its own length in 24 bits, so it takes two pointers' width
/// and 8 bytes, one pointer's width less than a vector and two 32-bit
/// counts. It grows and
/// shrinks as a vector would, to the same capacities: `reserve_exact`,
/// `shrink_to_fit` and a clone each leave the capacity a vector leaves.
/// The bytes past its length are zero and never read.
#[derive(Default)]
pub(super) struct ClusterArena {
    bytes: Box<[u8]>,
    len: U24,
    /// The bytes of the clusters the line's cells name, length bytes
    /// included and placeholder clusters left out: what the line's
    /// cluster budget counts. It is at most the arena's length, since no
    /// two cells name one cluster, so it never reaches the largest count.
    live: U24,
    /// The cells that name a placeholder cluster, at most the line's
    /// `COLUMNS_MAX` cells, so it never reaches `u16::MAX`.
    pub(super) placeholders: u16,
}

impl ClusterArena {
    /// An empty arena with room for `cap` bytes.
    pub(super) fn with_capacity(cap: usize) -> Self {
        Self {
            bytes: vec![0; cap].into_boxed_slice(),
            ..Self::default()
        }
    }

    pub(super) fn len(&self) -> usize {
        self.len.get()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn capacity(&self) -> usize {
        self.bytes.len()
    }

    /// The bytes the arena holds.
    pub(super) fn as_slice(&self) -> &[u8] {
        self.bytes.get(..self.len()).unwrap_or_default()
    }

    /// Makes room for `more` bytes past the arena's length, to exactly
    /// that capacity when it grows, as `Vec::reserve_exact` does.
    pub(super) fn reserve_exact(&mut self, more: usize) {
        if self.capacity().saturating_sub(self.len()) >= more {
            return;
        }
        self.regrow(self.len().saturating_add(more));
    }

    /// Gives back the room past the arena's length, as
    /// `Vec::shrink_to_fit` does.
    pub(super) fn shrink_to_fit(&mut self) {
        if self.capacity() > self.len() {
            self.bytes = Box::from(self.as_slice());
        }
    }

    /// Makes room for `more` bytes as a line's other tables grow
    /// ([`reserve_within`]).
    pub(super) fn reserve_within(&mut self, more: usize, limit: usize) {
        if let Some(target) = grown_within(self.len(), self.capacity(), more, limit) {
            self.reserve_exact(target.saturating_sub(self.len()));
        }
    }

    /// Moves the arena's bytes to a buffer of `cap` bytes, `cap` at least
    /// its length.
    fn regrow(&mut self, cap: usize) {
        let mut bytes = vec![0; cap.max(self.len())].into_boxed_slice();
        if let Some(to) = bytes.get_mut(..self.len()) {
            to.copy_from_slice(self.as_slice());
        }
        self.bytes = bytes;
    }

    /// Appends the cluster `s` as its length byte and its bytes, and
    /// returns the offset it starts at. It refuses a cluster longer than
    /// a length byte can say, and one that would start at an offset no
    /// cell can name. A line reserves the room first; an arena without
    /// room grows as a vector's `push` and `extend_from_slice` would.
    pub(super) fn append(&mut self, s: &str) -> Option<u32> {
        let len = u8::try_from(s.len()).ok()?;
        let at = self.len();
        if at >= ARENA_OFFSETS {
            return None;
        }
        let offset = u32::try_from(at).ok()?;
        self.push(&[len]);
        self.push(s.as_bytes());
        Some(offset)
    }

    /// Appends `bytes`, first doubling the capacity, to at least 8 bytes
    /// and at least what they need, when they do not fit: a vector's
    /// growth.
    fn push(&mut self, bytes: &[u8]) {
        let at = self.len();
        let end = at.saturating_add(bytes.len());
        if end > self.capacity() {
            self.regrow(self.capacity().saturating_mul(2).max(end).max(8));
        }
        if let Some(to) = self.bytes.get_mut(at..end) {
            to.copy_from_slice(bytes);
            self.len = U24::saturating(end);
        }
    }

    pub(super) fn live(&self) -> usize {
        self.live.get()
    }

    pub(super) fn set_live(&mut self, live: usize) {
        self.live = U24::saturating(live);
    }

    pub(super) fn placeholders(&self) -> usize {
        usize::from(self.placeholders)
    }
}

impl Clone for ClusterArena {
    /// A copy whose capacity is its length, as a vector's clone has.
    fn clone(&self) -> Self {
        Self {
            bytes: Box::from(self.as_slice()),
            ..*self
        }
    }
}

impl PartialEq for ClusterArena {
    /// Arenas are equal when they hold the same bytes and counts, whatever
    /// their capacity, as vectors are.
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
            && self.live == other.live
            && self.placeholders == other.placeholders
    }
}

impl core::fmt::Debug for ClusterArena {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClusterArena")
            .field("bytes", &self.as_slice())
            .field("live", &self.live())
            .field("placeholders", &self.placeholders)
            .finish()
    }
}

/// A line's run table, behind one pointer that stays empty while the line
/// holds no run, so that it takes one word of a line's tables.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(
    clippy::box_collection,
    reason = "one thin pointer takes a word of a line's tables, where a Vec takes three"
)]
pub(super) struct RunTable(pub(super) Option<Box<Vec<SizedRun>>>);

impl RunTable {
    pub(super) fn as_slice(&self) -> &[SizedRun] {
        self.0.as_deref().map_or(&[], Vec::as_slice)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = &mut SizedRun> {
        self.0.iter_mut().flat_map(|v| v.iter_mut())
    }

    pub(super) fn retain(&mut self, keep: impl FnMut(&SizedRun) -> bool) {
        if let Some(runs) = self.0.as_mut() {
            runs.retain(keep);
            if runs.is_empty() {
                self.0 = None;
            }
        }
    }

    /// Adds `run`, keeping the table ordered by column, on a line of
    /// `cells` cells. The runs on a line never overlap and each takes at
    /// least one column, so the table holds at most one run for each cell.
    /// Its capacity grows by doubling, as the line's other tables do, and
    /// stops at the cells. A table that already holds a run for each cell
    /// means an edit let runs overlap: a debug build stops there, and the
    /// memory tests find the line past its bound.
    ///
    /// References:
    /// - ADR-1009 D1: the line's cells are the sanity cap on its runs.
    pub(super) fn insert(&mut self, run: SizedRun, cells: usize) {
        let runs = self.0.get_or_insert_default();
        debug_assert!(
            runs.len() < cells,
            "a line of {cells} cells holds {} sized runs and gains one more",
            runs.len()
        );
        reserve_within(runs, 1, cells);
        let at = runs.partition_point(|r| r.col() < run.col());
        runs.insert(at, run);
    }

    pub(super) fn shrink_to_fit(&mut self) {
        if let Some(runs) = self.0.as_mut() {
            runs.shrink_to_fit();
        }
    }

    /// The bytes the table holds on the heap, by capacity.
    pub(super) fn heap_bytes(&self) -> usize {
        self.0.as_ref().map_or(0, |runs| {
            size_of::<Vec<SizedRun>>()
                .saturating_add(runs.capacity().saturating_mul(size_of::<SizedRun>()))
        })
    }
}

impl Tables {
    pub(super) fn is_empty(&self) -> bool {
        self.styles.is_empty()
            && self.links.is_empty()
            && self.clusters.is_empty()
            && self.runs.is_empty()
    }

    /// The cluster at byte `offset` of the arena.
    pub(super) fn cluster(&self, offset: u32) -> Option<&str> {
        cluster_at(self.clusters.as_slice(), offset)
    }

    /// The arena bytes the cluster at `offset` takes, its length byte
    /// included.
    pub(super) fn cluster_size(&self, offset: u32) -> usize {
        usize::try_from(offset)
            .ok()
            .and_then(|at| self.clusters.as_slice().get(at))
            .map_or(0, |&len| usize::from(len).saturating_add(1))
    }

    /// The bytes of the line's clusters its budget counts.
    pub(super) fn live(&self) -> usize {
        self.clusters.live()
    }

    /// The cells that name a placeholder cluster.
    pub(super) fn placeholders(&self) -> usize {
        self.clusters.placeholders()
    }

    /// Counts the cluster `s`, just stored, as a cell now names it.
    pub(super) fn count(&mut self, s: &str) {
        let arena = &mut self.clusters;
        if is_placeholder(s) {
            arena.placeholders = arena.placeholders.saturating_add(1);
        } else {
            arena.set_live(arena.live().saturating_add(s.len()).saturating_add(1));
        }
    }

    /// Gives back the cluster at `offset`, as the cell that named it goes.
    pub(super) fn forget(&mut self, offset: u32) {
        if self.cluster(offset).is_some_and(is_placeholder) {
            let arena = &mut self.clusters;
            arena.placeholders = arena.placeholders.saturating_sub(1);
        } else {
            let size = self.cluster_size(offset);
            let arena = &mut self.clusters;
            arena.set_live(arena.live().saturating_sub(size));
        }
    }
}

/// The cluster at byte `offset` of `arena`.
pub(super) fn cluster_at(arena: &[u8], offset: u32) -> Option<&str> {
    let at = usize::try_from(offset).ok()?;
    let len = usize::from(*arena.get(at)?);
    let start = at.checked_add(1)?;
    let bytes = arena.get(start..start.checked_add(len)?)?;
    core::str::from_utf8(bytes).ok()
}

/// Appends the cluster `s` to `arena` as its length byte and its bytes,
/// and returns the content that names it, or `None` when the arena
/// refuses it ([`ClusterArena::append`]).
pub(super) fn append_cluster(arena: &mut ClusterArena, s: &str) -> Option<Content> {
    arena
        .append(s)
        .map(|offset| Content::Cluster(ClusterOffset(offset)))
}

/// The indices a rewrap has copied from one source line's tables into the
/// line it builds: entry `i` holds the new index of old index `i + 1`, 0
/// until copied. A rewrap clears it for each pair of lines.
#[derive(Debug, Default)]
pub(crate) struct IndexMap {
    pub(super) styles: Vec<u16>,
    pub(super) links: Vec<u16>,
}

impl IndexMap {
    pub(crate) fn clear(&mut self) {
        self.styles.clear();
        self.links.clear();
    }

    /// Makes room for every index of `tables`.
    pub(super) fn fit(&mut self, tables: &Tables) {
        if self.styles.len() < tables.styles.len() {
            self.styles.resize(tables.styles.len(), 0);
        }
        if self.links.len() < tables.links.len() {
            self.links.resize(tables.links.len(), 0);
        }
    }
}

/// Makes room for `more` entries in `table`. Its capacity doubles, as a
/// vector's does, but stops at `limit` while the entries fit inside it, so
/// a table that compacts at `limit` never holds room past it. A table
/// that passes its limit, as a row a rewrap builds may, doubles past it.
pub(super) fn reserve_within<T>(table: &mut Vec<T>, more: usize, limit: usize) {
    if let Some(target) = grown_within(table.len(), table.capacity(), more, limit) {
        table.reserve_exact(target.saturating_sub(table.len()));
    }
}

/// The capacity [`reserve_within`] grows a table of `len` entries and
/// `capacity` room to, for `more` entries within `limit`, or `None` when
/// the room is there.
fn grown_within(len: usize, capacity: usize, more: usize, limit: usize) -> Option<usize> {
    let need = len.saturating_add(more);
    if need <= capacity {
        return None;
    }
    let doubled = capacity.saturating_mul(2).max(4);
    Some(if need <= limit {
        doubled.min(limit).max(need)
    } else {
        doubled.max(need)
    })
}
