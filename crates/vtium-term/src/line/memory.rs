//! A line's memory: the cluster budget its cells write within, the limits
//! its tables compact at, the compaction itself, and what the line keeps of
//! a cluster that would pass its budget.

use core::mem;
use core::ops::Range;
use std::collections::BTreeMap;

use super::tables::{ClusterArena, Tables, append_cluster, cluster_at, reserve_within};
use super::{Line, LinkId, is_placeholder, width_of};
use crate::bounds::{
    CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, PLACEHOLDER_BYTES_MAX, PLACEHOLDER_MARKS,
    compaction_limit, table_limit,
};
use crate::cell::{Cell, ClusterOffset, Content, LinkIndex, StyleIndex};
use crate::style::Style;
use crate::unicode::Segmenter;

impl Line {
    /// The bytes of the cluster arena the line's cells name, length bytes
    /// included: at most [`CLUSTER_BYTES_PER_CELL`] for each cell of the
    /// width the line was written at, and in a build with images up to
    /// 17 more for each cell that names a
    /// placeholder cluster, which the budget does not count. A trimmed line
    /// keeps what its cells name, which may pass the budget of its fewer
    /// cells.
    pub fn cluster_bytes(&self) -> usize {
        let Some(t) = self.tables.as_ref() else {
            return 0;
        };
        let held: usize = self
            .cells
            .iter()
            .filter_map(|c| match c.content() {
                Content::Cluster(ClusterOffset(o)) if t.cluster(o).is_some_and(is_placeholder) => {
                    Some(t.cluster_size(o))
                }
                _ => None,
            })
            .sum();
        t.live().saturating_add(held)
    }

    /// The cells that name a placeholder cluster.
    pub(super) fn placeholders(&self) -> usize {
        self.tables.as_ref().map_or(0, |t| t.placeholders())
    }

    /// The arena's limit once the cluster `s` is stored: a placeholder
    /// cluster raises it by what the cell that names it adds.
    pub(super) fn arena_with(&self, s: &str) -> usize {
        self.arena_limit(
            self.placeholders()
                .saturating_add(usize::from(is_placeholder(s))),
        )
    }

    /// The most bytes the cluster arena holds before the line compacts it,
    /// while `placeholders` of its cells name a placeholder cluster: one
    /// and a half times the line's budget and [`PLACEHOLDER_BYTES_MAX`]
    /// for each of them.
    fn arena_limit(&self, placeholders: usize) -> usize {
        let held = placeholders.saturating_mul(PLACEHOLDER_BYTES_MAX);
        let budget = self
            .cells
            .len()
            .max(1)
            .saturating_mul(CLUSTER_BYTES_PER_CELL);
        compaction_limit(budget.saturating_add(held))
    }

    /// Gives back spare capacity, once a rewrap has filled the line.
    pub(crate) fn shrink(&mut self) {
        self.cells.shrink_to_fit();
        self.shrink_tables();
    }

    /// The bytes the line holds on the heap, by capacity.
    pub fn heap_bytes(&self) -> usize {
        let cells = self.cells.capacity().saturating_mul(size_of::<Cell>());
        let tables = self.tables.as_ref().map_or(0, |t| {
            [
                size_of::<Tables>(),
                t.styles.capacity().saturating_mul(size_of::<Style>()),
                t.links.capacity().saturating_mul(size_of::<u16>()),
                t.clusters.capacity(),
                t.runs.heap_bytes(),
            ]
            .into_iter()
            .fold(0, usize::saturating_add)
        });
        cells.saturating_add(tables)
    }

    /// The cluster bytes the line's cells may name.
    pub(super) fn cluster_budget(&self) -> usize {
        self.cells.len().saturating_mul(CLUSTER_BYTES_PER_CELL)
    }

    /// The most entries the style and the link table each hold, and the
    /// most bytes the cluster arena holds, before the line compacts them:
    /// one and a half times what the line's cells can name, which for the
    /// arena is its budget and [`PLACEHOLDER_BYTES_MAX`] for each cell that
    /// names a placeholder cluster ([`table_limit`], [`compaction_limit`]).
    /// A table's capacity grows by doubling, as a vector's does, but stops
    /// at its limit ([`reserve_within`]), so a line's tables never hold
    /// room past what they compact at.
    ///
    /// References:
    /// - ADR-1009 D3: one and a half times, and the floor of the style and
    ///   link tables.
    pub(super) fn limits(&self) -> (usize, usize) {
        (
            table_limit(self.cells.len().max(1)),
            self.arena_limit(self.placeholders()),
        )
    }

    /// Compacts the tables when a style or a link table is full, or when
    /// the cluster `adds`, which a write is about to store, would take the
    /// arena past its limit. So the indices `write` and `erase` intern
    /// stay valid, and a write compacts the arena only when its own
    /// cluster does not fit.
    ///
    /// References:
    /// - ADR-1009 D4: the arena compacts on the bytes a write adds.
    pub(super) fn make_room(&mut self, adds: Option<&str>) {
        let (limit, _) = self.limits();
        let arena = adds.map_or(0, |s| self.arena_with(s));
        let bytes = adds.map_or(0, |s| s.len().saturating_add(1));
        let full = self.tables.as_ref().is_some_and(|t| {
            t.styles.len() >= limit
                || t.links.len() >= limit
                || (bytes > 0 && t.clusters.len().saturating_add(bytes) > arena)
        });
        if full {
            self.compact();
        }
    }

    pub(super) fn shrink_tables(&mut self) {
        if let Some(t) = self.tables.as_mut() {
            t.styles.shrink_to_fit();
            t.links.shrink_to_fit();
            t.clusters.shrink_to_fit();
            t.runs.shrink_to_fit();
        }
    }

    /// Keeps the table entries a cell names, in the order cells first name
    /// them, and renumbers the cells.
    pub(super) fn compact(&mut self) {
        let (limit, arena) = self.limits();
        let Some(old) = self.tables.take() else {
            return;
        };
        let mut new = Tables::default();
        // A row a rewrap built may name more than its arena's limit; its
        // arena is sized once, to what its cells name, so that compacting
        // it never doubles past what it holds.
        let named = old
            .placeholders()
            .saturating_mul(PLACEHOLDER_BYTES_MAX)
            .saturating_add(old.live());
        if named > arena {
            new.clusters.reserve_exact(named);
        }
        let mut styles = vec![0u16; old.styles.len()];
        let mut links = vec![0u16; old.links.len()];
        let mut clusters = BTreeMap::new();
        for cell in &mut self.cells {
            let style = remap(
                &mut styles,
                cell.style().0,
                &old.styles,
                &mut new.styles,
                limit,
            );
            let link = remap(&mut links, cell.link().0, &old.links, &mut new.links, limit);
            let content = match cell.content() {
                Content::Cluster(ClusterOffset(o)) => {
                    match remap_cluster(&mut clusters, o, &old, &mut new, arena) {
                        Some(n) => {
                            if new.cluster(n).is_some_and(is_placeholder) {
                                let arena = &mut new.clusters;
                                arena.placeholders = arena.placeholders.saturating_add(1);
                            }
                            Content::Cluster(ClusterOffset(n))
                        }
                        None => Content::Empty,
                    }
                }
                other => other,
            };
            *cell = Cell::new(content, cell.width(), StyleIndex(style), LinkIndex(link))
                .with_run(cell.in_run());
        }
        new.runs = old.runs;
        let fill = remap(&mut styles, self.fill, &old.styles, &mut new.styles, limit);
        self.fill = fill;
        let live = budgeted_bytes(new.clusters.as_slice());
        new.clusters.set_live(live);
        if !new.is_empty() {
            self.tables = Some(Box::new(new));
        }
    }

    /// Gives back the cluster bytes `old` named, as it is overwritten.
    pub(super) fn release(&mut self, old: Cell) {
        if let (Content::Cluster(ClusterOffset(o)), Some(t)) = (old.content(), self.tables.as_mut())
        {
            t.forget(o);
        }
    }

    /// Gives back the cluster bytes the cells of `range` name, as they go.
    pub(super) fn release_range(&mut self, range: Range<usize>) {
        let Some(t) = self.tables.as_mut() else {
            return;
        };
        for cell in self.cells.get(range).unwrap_or_default() {
            if let Content::Cluster(ClusterOffset(o)) = cell.content() {
                t.forget(o);
            }
        }
    }

    pub(super) fn intern_style(&mut self, style: &Style) -> StyleIndex {
        if *style == Style::DEFAULT {
            return StyleIndex(0);
        }
        let (limit, _) = self.limits();
        let styles = &mut self.tables.get_or_insert_default().styles;
        StyleIndex(intern(styles, style, limit))
    }

    pub(super) fn intern_link(&mut self, link: Option<LinkId>) -> LinkIndex {
        // A link's number fits 16 bits, since the store holds fewer links
        // than that (`links.rs` checks it when it compiles).
        let Some(link) = link.and_then(|l| u16::try_from(l.get()).ok()) else {
            return LinkIndex(0);
        };
        let (limit, _) = self.limits();
        let links = &mut self.tables.get_or_insert_default().links;
        LinkIndex(intern(links, &link, limit))
    }

    /// What the line keeps of the cluster `s` when a cell names it: as much
    /// of it as the budget allows, or `None` when that is its first
    /// codepoint alone. A placeholder cluster keeps its head whatever the
    /// budget.
    pub(super) fn kept_cluster<'s>(&self, s: &'s str) -> Option<&'s str> {
        let first = s.chars().next()?;
        if is_placeholder(s) {
            Some(placeholder_head(s)).filter(|k| k.len() > first.len_utf8())
        } else {
            let live = self.tables.as_ref().map_or(0, |t| t.live());
            fit(s, self.cluster_budget().saturating_sub(live))
        }
    }

    /// Stores `kept`, what the line keeps of a cluster
    /// ([`kept_cluster`](Self::kept_cluster)), and returns the content that
    /// names it, or `None` when the arena refuses it.
    pub(super) fn store_cluster(&mut self, kept: &str) -> Option<Content> {
        let arena = self.arena_with(kept);
        let t = self.tables.get_or_insert_default();
        t.clusters
            .reserve_within(kept.len().saturating_add(1), arena);
        let content = append_cluster(&mut t.clusters, kept)?;
        t.count(kept);
        Some(content)
    }

    /// Holds the clusters the line's cells name to the budget of its
    /// width, as writing the line at that width keeps them
    /// ([`hold_to`](Self::hold_to)). The line then counts as written at
    /// its width.
    pub(super) fn hold_to_budget(&mut self) {
        self.written_at = width_of(self.cells.len());
        self.hold_to(self.cluster_budget());
    }

    /// Holds the clusters the line's cells name to `budget` bytes, length
    /// bytes included, placeholder clusters left out: from the first cell
    /// on, a cluster that would pass what is left of `budget` keeps the
    /// longest run of its first codepoints that fits, and at least its
    /// first codepoint, and a placeholder cluster is kept whole. A line
    /// inside `budget` is left as it is; one past it has its arena
    /// rebuilt, in time linear in its width.
    pub(super) fn hold_to(&mut self, budget: usize) {
        if self.tables.as_ref().is_none_or(|t| t.live() <= budget) {
            return;
        }
        let Some(mut t) = self.tables.take() else {
            return;
        };
        let old = mem::take(&mut t.clusters);
        let held = old.placeholders().saturating_mul(PLACEHOLDER_BYTES_MAX);
        let mut kept = ClusterArena::with_capacity(budget.saturating_add(held));
        let (mut live, mut placeholders) = (0usize, 0u16);
        for cell in &mut self.cells {
            let Content::Cluster(ClusterOffset(o)) = cell.content() else {
                continue;
            };
            let room = budget.saturating_sub(live);
            let content = cluster_at(old.as_slice(), o).map_or(Content::Empty, |s| {
                let first = s.chars().next().map_or(Content::Empty, Content::Char);
                if is_placeholder(s) {
                    let content = append_cluster(&mut kept, s);
                    placeholders = placeholders.saturating_add(u16::from(content.is_some()));
                    return content.unwrap_or(first);
                }
                // A run's text keeps whole grapheme clusters.
                let held = if cell.in_run() {
                    Some(run_fit(s, room)).filter(|k| k.chars().nth(1).is_some())
                } else {
                    fit(s, room)
                };
                let Some(content) = held.and_then(|s| {
                    live = live.saturating_add(s.len()).saturating_add(1);
                    append_cluster(&mut kept, s)
                }) else {
                    return first;
                };
                content
            });
            *cell =
                Cell::new(content, cell.width(), cell.style(), cell.link()).with_run(cell.in_run());
        }
        kept.shrink_to_fit();
        kept.set_live(live);
        kept.placeholders = placeholders;
        t.clusters = kept;
        if !t.is_empty() {
            self.tables = Some(t);
        }
    }
}

/// What a line keeps of the placeholder cluster `s`: the placeholder and
/// its first [`PLACEHOLDER_MARKS`] codepoints after it.
fn placeholder_head(s: &str) -> &str {
    let end = s
        .char_indices()
        .nth(PLACEHOLDER_MARKS.saturating_add(1))
        .map_or(s.len(), |(i, _)| i);
    s.get(..end).unwrap_or(s)
}

/// The bytes of the clusters in `arena` a line's budget counts, length
/// bytes included: every cluster but the placeholder clusters.
fn budgeted_bytes(arena: &[u8]) -> usize {
    let mut at = 0usize;
    let mut bytes = 0usize;
    while let Some(&len) = arena.get(at) {
        let size = usize::from(len).saturating_add(1);
        let placeholder = u32::try_from(at)
            .ok()
            .and_then(|o| cluster_at(arena, o))
            .is_some_and(is_placeholder);
        if !placeholder {
            bytes = bytes.saturating_add(size);
        }
        at = at.saturating_add(size);
    }
    bytes
}

/// What a line's budget keeps of the cluster `s` with `room` bytes left:
/// the longest run of its first codepoints, at most [`CLUSTER_MAX`], that
/// fits with its length byte, or `None` when that is less than two
/// codepoints, as the line then stores only the first.
pub(super) fn fit(s: &str, room: usize) -> Option<&str> {
    let mut chars = s.chars();
    let mut end = chars.next()?.len_utf8();
    let mut count = 1usize;
    for c in chars.take(CLUSTER_MAX.saturating_sub(1)) {
        let next = end.saturating_add(c.len_utf8());
        if next.saturating_add(1) > room {
            break;
        }
        end = next;
        count = count.saturating_add(1);
    }
    s.get(..end).filter(|_| count >= 2)
}

/// The longest run of whole grapheme clusters at the start of `text` that
/// fits `budget` bytes with a length byte and keeps within [`CLUSTER_MAX`]
/// codepoints, and at least the first cluster, whatever its size.
pub(crate) fn whole_clusters(text: &str, budget: usize) -> &str {
    let fits = |bytes: usize, codepoints: usize| {
        bytes.saturating_add(1) <= budget && codepoints <= CLUSTER_MAX
    };
    let mut state: Option<Segmenter> = None;
    let (mut kept, mut codepoints) = (0usize, 0usize);
    for (i, c) in text.char_indices() {
        if !state.as_mut().is_some_and(|s| s.continues(c)) {
            // A cluster ends at `i`: the first one stays whatever its size.
            if i > 0 {
                if kept > 0 && !fits(i, codepoints) {
                    return text.get(..kept).unwrap_or(text);
                }
                kept = i;
            }
            state = Some(Segmenter::start(c));
        }
        codepoints = codepoints.saturating_add(1);
    }
    if kept > 0 && !fits(text.len(), codepoints) {
        return text.get(..kept).unwrap_or(text);
    }
    text
}

/// What a line with `room` bytes of its cluster budget left keeps of a
/// sized run's text: as many whole grapheme clusters as fit
/// ([`whole_clusters`]). When not even the first cluster fits, the line
/// keeps that cluster as it keeps printed text, the longest run of its
/// first codepoints that fits, and at least its first codepoint, since the
/// line's budget bounds its memory.
///
/// References:
/// - ADR-1308 D3, "The text's bound": whole clusters, and what a line
///   whose budget is spent keeps.
/// - ADR-1009 D1: the cluster budget bounds a line's memory.
pub(crate) fn run_fit(text: &str, room: usize) -> &str {
    let kept = whole_clusters(text, room);
    let mut chars = kept.chars();
    let first = chars.next().map_or(0, char::len_utf8);
    if chars.next().is_none()
        || (kept.len().saturating_add(1) <= room && kept.chars().count() <= CLUSTER_MAX)
    {
        return kept;
    }
    fit(kept, room).unwrap_or_else(|| kept.get(..first).unwrap_or(kept))
}

/// The 1-based index of `value` in `table`, appended if absent. The table
/// holds at most [`table_limit`] of the line's cells (`make_room`), and
/// the newest entry is the likeliest match, so the search runs from the
/// end; it is linear in the line's width at worst. The bounds' compile-time
/// check keeps every index inside 16 bits.
fn intern<T: PartialEq + Copy>(table: &mut Vec<T>, value: &T, limit: usize) -> u16 {
    let i = match table.iter().rposition(|v| v == value) {
        Some(i) => i,
        None => {
            reserve_within(table, 1, limit);
            table.push(*value);
            table.len().saturating_sub(1)
        }
    };
    u16::try_from(i.saturating_add(1)).unwrap_or(u16::MAX)
}

/// The new 1-based index of old index `i`, copying its entry into `new`
/// the first time a cell names it, `new` growing within `limit`
/// ([`reserve_within`]). `map` holds the new index of each old entry, 0
/// until it is copied.
pub(super) fn remap<T: Copy>(
    map: &mut [u16],
    i: u16,
    old: &[T],
    new: &mut Vec<T>,
    limit: usize,
) -> u16 {
    let Some(at) = usize::from(i).checked_sub(1) else {
        return 0;
    };
    let (Some(slot), Some(value)) = (map.get_mut(at), old.get(at)) else {
        return 0;
    };
    if *slot == 0 {
        reserve_within(new, 1, limit);
        new.push(*value);
        *slot = u16::try_from(new.len()).unwrap_or(u16::MAX);
    }
    *slot
}

/// The new offset of the cluster at old offset `o`, copying it into `new`
/// the first time a cell names it, the arena growing within `arena`
/// ([`reserve_within`]).
fn remap_cluster(
    map: &mut BTreeMap<u32, u32>,
    o: u32,
    old: &Tables,
    new: &mut Tables,
    arena: usize,
) -> Option<u32> {
    if let Some(&n) = map.get(&o) {
        return Some(n);
    }
    let s = old.cluster(o)?;
    u8::try_from(s.len()).ok()?;
    new.clusters
        .reserve_within(s.len().saturating_add(1), arena);
    let n = new.clusters.append(s)?;
    map.insert(o, n);
    Some(n)
}
