//! The edits of a line: writing and erasing cells, inserting and deleting
//! them, resizing and trimming the line, and the copies a rewrap makes from
//! one line into another.

use core::ops::Range;

#[cfg(feature = "bidi")]
use super::first_char;
use super::memory::remap;
use super::tables::{IndexMap, append_cluster};
use super::{Glyph, Line, LineFlags, LinkId, width_of};
#[cfg(doc)]
use crate::bounds::CLUSTER_MAX;
use crate::bounds::{COLUMNS_MAX, compaction_limit};
use crate::cell::{Cell, ClusterOffset, Content, LinkIndex, StyleIndex, Width};
use crate::style::Style;

impl Line {
    /// Gives cell `col` the style `style`, and keeps its text, its width,
    /// its link and its mark as a cell of a sized run. DECCARA and DECRARA
    /// change a cell's style this way, so a run whose cells they change
    /// stays whole.
    pub(crate) fn set_style(&mut self, col: usize, style: &Style) {
        if col >= self.cells.len() {
            return;
        }
        // Compaction renumbers the cells, so the style is interned after
        // it.
        self.make_room(None);
        let style = self.intern_style(style);
        if let Some(cell) = self.cells.get_mut(col) {
            *cell = cell.with_style(style);
        }
    }

    /// Writes one cell. A cluster keeps its first [`CLUSTER_MAX`]
    /// codepoints, and fewer when the line's cluster budget is spent; a
    /// cluster left with one codepoint is stored as that codepoint.
    /// Returns false, changing nothing, when `col` is past the line's end.
    pub fn write(
        &mut self,
        col: usize,
        glyph: Glyph<'_>,
        width: Width,
        style: &Style,
        link: Option<LinkId>,
    ) -> bool {
        if col >= self.cells.len() {
            return false;
        }
        self.drop_runs(col..col.saturating_add(1));
        // The overwritten cell gives back its cluster first, so that a
        // compaction keeps only the clusters other cells name.
        let old = self.cell(col);
        self.release(old);
        if let (Content::Cluster(_), Some(cell)) = (old.content(), self.cells.get_mut(col)) {
            *cell = Cell::new(Content::Empty, old.width(), old.style(), old.link());
        }
        let kept = match glyph {
            Glyph::Cluster(s) => self.kept_cluster(s),
            _ => None,
        };
        // Compaction renumbers the cells, so the style and the link are
        // interned after it.
        self.make_room(kept);
        let style = self.intern_style(style);
        let link = self.intern_link(link);
        let content = match glyph {
            Glyph::Empty => Content::Empty,
            Glyph::Char(c) => Content::Char(c),
            Glyph::Cluster(s) => kept
                .and_then(|k| self.store_cluster(k))
                .or_else(|| s.chars().next().map(Content::Char))
                .unwrap_or(Content::Empty),
        };
        if let Some(cell) = self.cells.get_mut(col) {
            *cell = Cell::new(content, width, style, link);
        }
        self.wide |= width == Width::Wide;
        #[cfg(feature = "bidi")]
        if first_char(glyph).is_some_and(crate::unicode::bidi::reorders) {
            self.flags.insert(LineFlags::REORDERS);
        }
        true
    }

    /// Erases the cells of `cols` inside the line to empty, narrow cells
    /// of `style` with no link.
    pub fn erase(&mut self, cols: Range<usize>, style: &Style) {
        let end = cols.end.min(self.cells.len());
        if cols.start >= end {
            return;
        }
        self.drop_runs(cols.start..end);
        self.make_room(None);
        let style = self.intern_style(style);
        let blank = Cell::new(Content::Empty, Width::Narrow, style, LinkIndex(0));
        for col in cols.start..end {
            if let Some(&old) = self.cells.get(col) {
                self.release(old);
            }
            if let Some(cell) = self.cells.get_mut(col) {
                *cell = blank;
            }
        }
    }

    /// Moves the cells of `at..end` right by `n`, inside the line: the
    /// cells pushed past `end` go and give back their clusters, and the `n`
    /// cells from `at` become empty, narrow cells of `style` with no link.
    /// ICH, a write in insert mode and a cell a VS16 widens move cells
    /// this way.
    pub fn insert_cells(&mut self, at: usize, n: usize, end: usize, style: &Style) {
        let end = end.min(self.cells.len());
        let n = n.min(end.saturating_sub(at));
        if n == 0 {
            return;
        }
        self.shift_runs(at, n, end, true);
        self.make_room(None);
        let style = self.intern_style(style);
        let kept = end.saturating_sub(n);
        self.release_range(kept..end);
        self.cells.copy_within(at..kept, at.saturating_add(n));
        let blank = Cell::new(Content::Empty, Width::Narrow, style, LinkIndex(0));
        if let Some(cells) = self.cells.get_mut(at..at.saturating_add(n)) {
            cells.fill(blank);
        }
    }

    /// Removes the `n` cells from `at`, which give back their clusters: the
    /// cells after them up to `end` move left by `n`, and the `n` cells
    /// before `end` become empty, narrow cells of `style` with no link.
    /// DCH moves cells this way.
    pub fn delete_cells(&mut self, at: usize, n: usize, end: usize, style: &Style) {
        let end = end.min(self.cells.len());
        let n = n.min(end.saturating_sub(at));
        if n == 0 {
            return;
        }
        self.shift_runs(at, n, end, false);
        self.make_room(None);
        let style = self.intern_style(style);
        let from = at.saturating_add(n);
        self.release_range(at..from);
        self.cells.copy_within(from..end, at);
        let blank = Cell::new(Content::Empty, Width::Narrow, style, LinkIndex(0));
        if let Some(cells) = self.cells.get_mut(end.saturating_sub(n)..end) {
            cells.fill(blank);
        }
        // A pad that left the last column is an empty cell.
        let moved = at..end.saturating_sub(n);
        for cell in self.cells.get_mut(moved).unwrap_or_default() {
            if cell.width() == Width::Pad {
                *cell = Cell::new(Content::Empty, Width::Narrow, cell.style(), cell.link());
            }
        }
    }

    /// Makes the line `cols` cells long, at most [`COLUMNS_MAX`]: cells
    /// past the new end are dropped, a wide character the new end cuts
    /// becomes an empty cell, and new cells are empty in the line's fill.
    /// Shrinking holds the clusters the cells left name to the budget of
    /// the shorter line, as writing it at that width would
    /// (`hold_to_budget`), and compacts the tables,
    /// so they hold no more than the shorter line can name.
    pub fn resize(&mut self, cols: usize) {
        let cols = cols.min(COLUMNS_MAX);
        let shrinking = cols < self.cells.len();
        if shrinking {
            self.drop_runs(cols..self.cells.len());
        }
        let fill = self.fill_cell();
        self.cells
            .reserve_exact(cols.saturating_sub(self.cells.len()));
        self.cells.resize(cols, fill);
        if shrinking {
            if let Some(last) = self.cells.last_mut()
                && last.width() == Width::Wide
            {
                if let (Content::Cluster(ClusterOffset(o)), Some(t)) =
                    (last.content(), self.tables.as_mut())
                {
                    t.forget(o);
                }
                *last = Cell::new(Content::Empty, Width::Narrow, last.style(), LinkIndex(0));
            }
            if self.flags.contains(LineFlags::WRAPPED) {
                self.unwrap();
            }
            self.cells.shrink_to_fit();
            self.hold_to_budget();
            self.compact();
            self.shrink_tables();
        } else {
            self.written_at = self.written_at.max(width_of(cols));
        }
    }

    /// Readies the line for the scrollback, as [`trim_to`](Self::trim_to)
    /// with no cell to keep.
    pub fn trim(&mut self) {
        self.trim_to(0);
    }

    /// Readies the line for the scrollback, or a rewrap, keeping at least
    /// its first `keep` cells: a wrapped line keeps every cell, as all of
    /// them are text; any other drops its fill, the run of empty, narrow,
    /// unlinked cells of one style that ends it, and keeps that style as
    /// the style of the cells past its end. It compacts its tables and
    /// gives back spare capacity.
    pub fn trim_to(&mut self, keep: usize) {
        if !self.flags.contains(LineFlags::WRAPPED) {
            let (text, fill) = self.text_and_fill();
            self.cells.truncate(text.max(keep));
            self.fill = fill.0;
        }
        self.wide = !self.runs().is_empty() || self.cells.iter().any(|c| c.width() == Width::Wide);
        self.cells.shrink_to_fit();
        self.compact();
        self.shrink_tables();
    }

    /// Where the line's text ends, and the style of its fill: the run of
    /// empty, narrow, unlinked cells of one style that ends it, or the
    /// line's own fill past its end. Text that reaches the end leaves the
    /// default style.
    fn text_and_fill(&self) -> (usize, StyleIndex) {
        let fills = |c: &Cell, style: StyleIndex| {
            c.content() == Content::Empty
                && c.width() == Width::Narrow
                && c.link() == LinkIndex(0)
                && c.style() == style
                && !c.in_run()
        };
        let Some(&last) = self.cells.last() else {
            return (0, StyleIndex(self.fill));
        };
        if !fills(&last, last.style()) {
            return (self.cells.len(), StyleIndex(0));
        }
        let start = self
            .cells
            .iter()
            .rposition(|c| !fills(c, last.style()))
            .map_or(0, |i| i.saturating_add(1));
        (start, last.style())
    }

    /// Paints the line to `cols` cells, the cells past its text in its
    /// fill, as a line takes its place on a screen `cols` wide from a
    /// rewrap or the scrollback. A line written at a wider width is held
    /// to the budget of the screen's, as writing it there would
    /// ([`hold_to_budget`](Self::hold_to_budget)). A line whose clusters
    /// were written at the screen's width or a narrower one may still name
    /// more than its budget, since a widening by a factor that is not a
    /// whole number can gather onto one row the end of one source row, the
    /// whole of the next and the start of the one after; it is held to its
    /// arena's limit, one and a half times its budget, with the same cut
    /// ([`hold_to`](Self::hold_to)). So no screen row holds more than
    /// [`bytes_max`](Self::bytes_max).
    pub(crate) fn paint(&mut self, cols: usize) {
        self.resize(cols);
        if usize::from(self.written_at) > self.cells.len() {
            self.hold_to_budget();
        } else {
            self.hold_to(compaction_limit(self.cluster_budget()));
        }
    }

    /// Appends `cell` of `src`, its style, link and cluster copied into
    /// this line's tables through `map`, which holds the indices already
    /// copied from `src` into this line; `width` replaces its width, and
    /// `empty` its content. A cluster is copied whole, past this line's
    /// own budget if need be: a rewrap keeps a logical line's clusters,
    /// and the line counts as written at the wider of its own width and
    /// the width `src` was written at. A later write is held to the line's
    /// budget, and the line itself is held where it lands on a screen
    /// ([`paint`](Self::paint)).
    pub(crate) fn push_from(
        &mut self,
        src: &Line,
        cell: Cell,
        width: Width,
        empty: bool,
        map: &mut IndexMap,
    ) {
        let (style, link) = self.map_indices(src, cell.style(), cell.link(), map);
        let content = match cell.content() {
            _ if empty => Content::Empty,
            Content::Cluster(ClusterOffset(o)) => {
                self.written_at = self.written_at.max(src.written_at);
                src.tables
                    .as_ref()
                    .and_then(|t| t.cluster(o))
                    .and_then(|s| self.copy_cluster(s))
                    .unwrap_or(Content::Empty)
            }
            other => other,
        };
        if self.cells.len() < COLUMNS_MAX {
            self.cells.push(Cell::new(content, width, style, link));
            self.wide |= width == Width::Wide;
        }
        #[cfg(feature = "bidi")]
        if !empty && first_char(src.glyph(cell)).is_some_and(crate::unicode::bidi::reorders) {
            self.flags.insert(LineFlags::REORDERS);
        }
    }

    /// Appends a pad in the background of `cell` of `src`, as printing
    /// clears the column a wide character leaves (ADR-1003).
    pub(crate) fn push_pad(&mut self, src: &Line, cell: Cell) {
        let pad = Style {
            bg: src.style(cell).bg,
            ..Style::DEFAULT
        };
        let style = self.intern_style(&pad);
        if self.cells.len() < COLUMNS_MAX {
            self.cells
                .push(Cell::new(Content::Empty, Width::Pad, style, LinkIndex(0)));
        }
    }

    /// Sets the line's fill to the fill of `src`, copied through `map`.
    pub(crate) fn set_fill_from(&mut self, src: &Line, map: &mut IndexMap) {
        let (style, _) = self.map_indices(src, StyleIndex(src.fill), LinkIndex(0), map);
        self.fill = style.0;
    }

    /// The indices in this line's tables of `style` and `link` of `src`.
    fn map_indices(
        &mut self,
        src: &Line,
        style: StyleIndex,
        link: LinkIndex,
        map: &mut IndexMap,
    ) -> (StyleIndex, LinkIndex) {
        let Some(from) = src.tables.as_ref().filter(|_| style.0 != 0 || link.0 != 0) else {
            return (StyleIndex(0), LinkIndex(0));
        };
        map.fit(from);
        let (limit, _) = self.limits();
        let to = self.tables.get_or_insert_default();
        let style = remap(
            &mut map.styles,
            style.0,
            &from.styles,
            &mut to.styles,
            limit,
        );
        let link = remap(&mut map.links, link.0, &from.links, &mut to.links, limit);
        (StyleIndex(style), LinkIndex(link))
    }

    /// Stores the cluster `s` whole.
    fn copy_cluster(&mut self, s: &str) -> Option<Content> {
        let arena = self.arena_with(s);
        let t = self.tables.get_or_insert_default();
        t.clusters.reserve_within(s.len().saturating_add(1), arena);
        let content = append_cluster(&mut t.clusters, s)?;
        t.count(s);
        Some(content)
    }

    /// Splits the line after its first `at` cells, for a rewrap that takes
    /// a long logical line's newest rows and leaves its head waiting:
    /// returns the cells from `at` on as a line with this one's id, flags
    /// and fill, and keeps the cells before as a wrapped line with no fill.
    pub(crate) fn split_text(&mut self, at: usize) -> Line {
        let at = at.min(self.cells.len());
        let mut tail =
            Line::with_capacity(self.id, self.flags, self.cells.len().saturating_sub(at));
        let mut map = IndexMap::default();
        for &cell in self.cells.get(at..).unwrap_or_default() {
            tail.push_from(self, cell, cell.width(), false, &mut map);
        }
        tail.set_fill_from(self, &mut map);
        self.drop_runs_where(|r| r.col() < at && r.span().end > at);
        for run in self.runs().to_vec() {
            if run.col() >= at {
                tail.mark_run(run.at(run.col().saturating_sub(at)));
            }
        }
        if let Some(t) = self.tables.as_mut() {
            t.runs.retain(|r| r.col() < at);
        }
        tail.shrink();
        self.cells.truncate(at);
        self.flags.insert(LineFlags::WRAPPED);
        self.fill = 0;
        self.cells.shrink_to_fit();
        self.compact();
        self.shrink_tables();
        tail
    }
}
