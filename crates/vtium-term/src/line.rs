//! The line: a row of cells, the tables its cells index, its identity and
//! its flags.
//!
//! Styles, hyperlinks and grapheme clusters live in tables owned by the
//! line, so a cell names each in 16 or 21 bits, and a line that drops out
//! of the scrollback frees everything its cells referred to. No table is
//! shared between lines, so none needs a reference count or a hash.
//!
//! A table holds only what was written to the line. Once the style or link
//! table reaches [`TABLE_GROWTH`] times the line's cells, or the cluster
//! arena that many times the line's cluster budget, the line compacts: it
//! keeps the entries a cell still names and renumbers them. A compaction,
//! like looking a style or link up in its table, costs time linear in the
//! line's width, so a write is linear in the line's width at worst.
//!
//! The cluster arena holds each cluster as a length byte and its codepoints
//! in UTF-8, and the cell holds the cluster's byte offset. The clusters a
//! line's cells name take at most [`CLUSTER_BYTES_PER_CELL`] bytes per
//! cell; a cluster that would pass that keeps the longest run of its first
//! codepoints that fits, and at least its first codepoint.

use core::ops::Range;
use std::collections::BTreeMap;

use crate::bounds::{
    CLUSTER_BYTES_MAX, CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, COLUMNS_MAX, TABLE_GROWTH,
};
use crate::cell::{Cell, ClusterOffset, Content, LinkIndex, StyleIndex, Width};
use crate::style::Style;

/// A line's identity: unique within its screen buffer for the buffer's
/// life, and never reused. A placement anchors to the line holding its top
/// row by this id (vtium/vtium#28), and prompt marks find their lines by
/// it (vtium/vtium#40).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineId(u64);

impl LineId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A hyperlink in the link store (vtium/vtium#40), as a line's link table
/// holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(u32);

impl LinkId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// DECDWL, DECDHL and DECSWL: how a line's cells are drawn
/// (vtium/vtium#61).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineSize {
    #[default]
    Single,
    DoubleWidth,
    DoubleHeightTop,
    DoubleHeightBottom,
}

/// What a line records about itself in 16 bits: the reflow flag, the
/// three shell-integration marks, the line size, and ten bits spare for
/// bidirectional state (vtium/vtium#43).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineFlags(u16);

impl LineFlags {
    /// The line's text continues on the next line: printing wrapped at the
    /// right margin. A line the program ended with a newline, or left by
    /// moving the cursor, does not carry it. Reflow (vtium/vtium#41)
    /// rejoins only lines that do.
    pub const WRAPPED: Self = Self(1 << 0);
    /// OSC 133 A: a prompt starts on this line.
    pub const PROMPT: Self = Self(1 << 1);
    /// OSC 133 B: the command's input starts on this line.
    pub const INPUT: Self = Self(1 << 2);
    /// OSC 133 C: the command's output starts on this line.
    pub const OUTPUT: Self = Self(1 << 3);

    const FLAGS: u16 = (1 << 4) - 1;
    const SIZE_SHIFT: u16 = 4;
    const SIZE_MASK: u16 = 0b11 << Self::SIZE_SHIFT;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        let flags = other.0 & Self::FLAGS;
        self.0 & flags == flags
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0 & Self::FLAGS;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !(other.0 & Self::FLAGS);
    }

    pub const fn size(self) -> LineSize {
        match (self.0 & Self::SIZE_MASK) >> Self::SIZE_SHIFT {
            0 => LineSize::Single,
            1 => LineSize::DoubleWidth,
            2 => LineSize::DoubleHeightTop,
            _ => LineSize::DoubleHeightBottom,
        }
    }

    pub fn set_size(&mut self, size: LineSize) {
        let bits: u16 = match size {
            LineSize::Single => 0,
            LineSize::DoubleWidth => 1,
            LineSize::DoubleHeightTop => 2,
            LineSize::DoubleHeightBottom => 3,
        };
        self.0 = (self.0 & !Self::SIZE_MASK) | (bits << Self::SIZE_SHIFT);
    }
}

/// What a cell shows, read through its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph<'a> {
    Empty,
    Char(char),
    /// Two or more codepoints, at most [`CLUSTER_MAX`].
    Cluster(&'a str),
}

/// A line's tables, allocated on its first styled, linked or clustered
/// cell. Index `i` of a style or link names entry `i - 1`; index 0 is the
/// default style or no link.
#[derive(Clone, Debug, Default)]
struct Tables {
    styles: Vec<Style>,
    links: Vec<LinkId>,
    /// Each cluster as a length byte and its codepoints in UTF-8.
    clusters: Vec<u8>,
    /// The bytes of `clusters` the line's cells name.
    live: usize,
}

impl Tables {
    fn is_empty(&self) -> bool {
        self.styles.is_empty() && self.links.is_empty() && self.clusters.is_empty()
    }

    /// The cluster at byte `offset` of the arena.
    fn cluster(&self, offset: u32) -> Option<&str> {
        let at = usize::try_from(offset).ok()?;
        let len = usize::from(*self.clusters.get(at)?);
        let start = at.checked_add(1)?;
        let bytes = self.clusters.get(start..start.checked_add(len)?)?;
        core::str::from_utf8(bytes).ok()
    }

    /// The arena bytes the cluster at `offset` takes, its length byte
    /// included.
    fn cluster_size(&self, offset: u32) -> usize {
        usize::try_from(offset)
            .ok()
            .and_then(|at| self.clusters.get(at))
            .map_or(0, |&len| usize::from(len).saturating_add(1))
    }
}

/// A row of cells.
///
/// A line may hold fewer cells than the grid is wide: a line trimmed for
/// the scrollback keeps no trailing blank cells. Every cell past its end
/// reads as [`Cell::BLANK`], in the default style, whatever the pen.
#[derive(Clone, Debug)]
pub struct Line {
    cells: Vec<Cell>,
    tables: Option<Box<Tables>>,
    id: LineId,
    flags: LineFlags,
}

const _: () = assert!(size_of::<Line>() == 48);

impl Line {
    /// A line of `cols` blank cells, at most [`COLUMNS_MAX`].
    pub fn new(id: LineId, cols: usize) -> Self {
        Self {
            cells: vec![Cell::BLANK; cols.min(COLUMNS_MAX)],
            tables: None,
            id,
            flags: LineFlags::empty(),
        }
    }

    pub fn id(&self) -> LineId {
        self.id
    }

    pub fn flags(&self) -> LineFlags {
        self.flags
    }

    pub fn flags_mut(&mut self) -> &mut LineFlags {
        &mut self.flags
    }

    /// The cells the line stores.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The cell at `col`, or [`Cell::BLANK`] past the line's end.
    pub fn cell(&self, col: usize) -> Cell {
        self.cells.get(col).copied().unwrap_or(Cell::BLANK)
    }

    /// The style `cell` names in this line's table.
    pub fn style(&self, cell: Cell) -> Style {
        let i = usize::from(cell.style().0);
        self.tables
            .as_ref()
            .and_then(|t| t.styles.get(i.checked_sub(1)?))
            .copied()
            .unwrap_or(Style::DEFAULT)
    }

    /// The hyperlink `cell` names in this line's table, if any.
    pub fn link(&self, cell: Cell) -> Option<LinkId> {
        let i = usize::from(cell.link().0);
        self.tables
            .as_ref()
            .and_then(|t| t.links.get(i.checked_sub(1)?))
            .copied()
    }

    /// What `cell` shows, its cluster read from this line's arena.
    pub fn glyph(&self, cell: Cell) -> Glyph<'_> {
        match cell.content() {
            Content::Empty => Glyph::Empty,
            Content::Char(c) => Glyph::Char(c),
            Content::Cluster(ClusterOffset(o)) => self
                .tables
                .as_ref()
                .and_then(|t| t.cluster(o))
                .map_or(Glyph::Empty, Glyph::Cluster),
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
        // Compaction renumbers the cells, so the old cell is read after it.
        self.make_room();
        let old = self.cell(col);
        let style = self.intern_style(style);
        let link = self.intern_link(link);
        self.release(old);
        let content = match glyph {
            Glyph::Empty => Content::Empty,
            Glyph::Char(c) => Content::Char(c),
            Glyph::Cluster(s) => self.push_cluster(s),
        };
        if let Some(cell) = self.cells.get_mut(col) {
            *cell = Cell::new(content, width, style, link);
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
        self.make_room();
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

    /// Makes the line `cols` cells long, at most [`COLUMNS_MAX`]: cells
    /// past the new end are dropped, and new cells are blank. Shrinking
    /// compacts the tables, so they hold no more than the shorter line can
    /// name.
    pub fn resize(&mut self, cols: usize) {
        let cols = cols.min(COLUMNS_MAX);
        let shrinking = cols < self.cells.len();
        self.cells.resize(cols, Cell::BLANK);
        if shrinking {
            self.cells.shrink_to_fit();
            self.compact();
            self.shrink_tables();
        }
    }

    /// Readies the line for the scrollback: drops its trailing blank
    /// cells, compacts its tables and gives back spare capacity.
    pub fn trim(&mut self) {
        let keep = self
            .cells
            .iter()
            .rposition(|c| !c.is_blank())
            .map_or(0, |i| i.saturating_add(1));
        self.cells.truncate(keep);
        self.cells.shrink_to_fit();
        self.compact();
        self.shrink_tables();
    }

    /// The bytes the line holds on the heap, by capacity.
    pub fn heap_bytes(&self) -> usize {
        let cells = self.cells.capacity().saturating_mul(size_of::<Cell>());
        let tables = self.tables.as_ref().map_or(0, |t| {
            [
                size_of::<Tables>(),
                t.styles.capacity().saturating_mul(size_of::<Style>()),
                t.links.capacity().saturating_mul(size_of::<LinkId>()),
                t.clusters.capacity(),
            ]
            .into_iter()
            .fold(0, usize::saturating_add)
        });
        cells.saturating_add(tables)
    }

    /// The cluster bytes the line's cells may name.
    fn cluster_budget(&self) -> usize {
        self.cells.len().saturating_mul(CLUSTER_BYTES_PER_CELL)
    }

    /// Compacts the tables when one more entry in any of them could pass
    /// its limit, so the indices `write` and `erase` intern stay valid.
    fn make_room(&mut self) {
        let cells = self.cells.len().max(1);
        let limit = cells.saturating_mul(TABLE_GROWTH);
        let arena = limit.saturating_mul(CLUSTER_BYTES_PER_CELL);
        let full = self.tables.as_ref().is_some_and(|t| {
            t.styles.len() >= limit
                || t.links.len() >= limit
                || (!t.clusters.is_empty()
                    && t.clusters.len().saturating_add(CLUSTER_BYTES_MAX) > arena)
        });
        if full {
            self.compact();
        }
    }

    fn shrink_tables(&mut self) {
        if let Some(t) = self.tables.as_mut() {
            t.styles.shrink_to_fit();
            t.links.shrink_to_fit();
            t.clusters.shrink_to_fit();
        }
    }

    /// Keeps the table entries a cell names, in the order cells first name
    /// them, and renumbers the cells.
    fn compact(&mut self) {
        let Some(old) = self.tables.take() else {
            return;
        };
        let mut new = Tables::default();
        let mut styles = vec![0u16; old.styles.len()];
        let mut links = vec![0u16; old.links.len()];
        let mut clusters = BTreeMap::new();
        for cell in &mut self.cells {
            let style = remap(&mut styles, cell.style().0, &old.styles, &mut new.styles);
            let link = remap(&mut links, cell.link().0, &old.links, &mut new.links);
            let content = match cell.content() {
                Content::Cluster(ClusterOffset(o)) => {
                    match remap_cluster(&mut clusters, o, &old, &mut new) {
                        Some(n) => Content::Cluster(ClusterOffset(n)),
                        None => Content::Empty,
                    }
                }
                other => other,
            };
            *cell = Cell::new(content, cell.width(), StyleIndex(style), LinkIndex(link));
        }
        new.live = new.clusters.len();
        if !new.is_empty() {
            self.tables = Some(Box::new(new));
        }
    }

    /// Gives back the cluster bytes `old` named, as it is overwritten.
    fn release(&mut self, old: Cell) {
        if let (Content::Cluster(ClusterOffset(o)), Some(t)) = (old.content(), self.tables.as_mut())
        {
            t.live = t.live.saturating_sub(t.cluster_size(o));
        }
    }

    fn intern_style(&mut self, style: &Style) -> StyleIndex {
        if *style == Style::DEFAULT {
            return StyleIndex(0);
        }
        let styles = &mut self.tables.get_or_insert_default().styles;
        StyleIndex(intern(styles, style))
    }

    fn intern_link(&mut self, link: Option<LinkId>) -> LinkIndex {
        let Some(link) = link else {
            return LinkIndex(0);
        };
        let links = &mut self.tables.get_or_insert_default().links;
        LinkIndex(intern(links, &link))
    }

    /// Stores the cluster `s`, as much of it as the budget allows.
    fn push_cluster(&mut self, s: &str) -> Content {
        let budget = self.cluster_budget();
        let mut chars = s.chars();
        let Some(first) = chars.next() else {
            return Content::Empty;
        };
        let live = self.tables.as_ref().map_or(0, |t| t.live);
        let room = budget.saturating_sub(live);
        // The longest run of whole codepoints, at most CLUSTER_MAX, that
        // fits with its length byte.
        let mut end = first.len_utf8();
        let mut count = 1usize;
        for c in chars.take(CLUSTER_MAX.saturating_sub(1)) {
            let next = end.saturating_add(c.len_utf8());
            if next.saturating_add(1) > room {
                break;
            }
            end = next;
            count = count.saturating_add(1);
        }
        if count < 2 {
            return Content::Char(first);
        }
        let t = self.tables.get_or_insert_default();
        let (Some(bytes), Ok(len), Ok(offset)) = (
            s.as_bytes().get(..end),
            u8::try_from(end),
            u32::try_from(t.clusters.len()),
        ) else {
            return Content::Char(first);
        };
        t.clusters.push(len);
        t.clusters.extend_from_slice(bytes);
        t.live = t.live.saturating_add(end).saturating_add(1);
        Content::Cluster(ClusterOffset(offset))
    }
}

/// The 1-based index of `value` in `table`, appended if absent. The table
/// holds at most [`TABLE_GROWTH`] times the line's cells (`make_room`), and
/// the newest entry is the likeliest match, so the search runs from the
/// end; it is linear in the line's width at worst. The bounds' compile-time
/// check keeps every index inside 16 bits.
fn intern<T: PartialEq + Copy>(table: &mut Vec<T>, value: &T) -> u16 {
    let i = match table.iter().rposition(|v| v == value) {
        Some(i) => i,
        None => {
            table.push(*value);
            table.len().saturating_sub(1)
        }
    };
    u16::try_from(i.saturating_add(1)).unwrap_or(u16::MAX)
}

/// The new 1-based index of old index `i`, copying its entry into `new`
/// the first time a cell names it. `map` holds the new index of each old
/// entry, 0 until it is copied.
fn remap<T: Copy>(map: &mut [u16], i: u16, old: &[T], new: &mut Vec<T>) -> u16 {
    let Some(at) = usize::from(i).checked_sub(1) else {
        return 0;
    };
    let (Some(slot), Some(value)) = (map.get_mut(at), old.get(at)) else {
        return 0;
    };
    if *slot == 0 {
        new.push(*value);
        *slot = u16::try_from(new.len()).unwrap_or(u16::MAX);
    }
    *slot
}

/// The new offset of the cluster at old offset `o`, copying it into `new`
/// the first time a cell names it.
fn remap_cluster(
    map: &mut BTreeMap<u32, u32>,
    o: u32,
    old: &Tables,
    new: &mut Tables,
) -> Option<u32> {
    if let Some(&n) = map.get(&o) {
        return Some(n);
    }
    let s = old.cluster(o)?;
    let len = u8::try_from(s.len()).ok()?;
    let n = u32::try_from(new.clusters.len()).ok()?;
    new.clusters.push(len);
    new.clusters.extend_from_slice(s.as_bytes());
    map.insert(o, n);
    Some(n)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::style::{Attrs, Color, Rgb};

    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

    fn red() -> Style {
        Style {
            fg: Color::Indexed(1),
            ..Style::DEFAULT
        }
    }

    fn rgb(n: u32) -> Style {
        let [_, r, g, b] = n.to_be_bytes();
        Style {
            fg: Color::Rgb(Rgb::new(r, g, b)),
            ..Style::DEFAULT
        }
    }

    /// `e` and `marks` combining acute accents.
    fn accented(marks: usize) -> String {
        std::iter::once('e')
            .chain(std::iter::repeat_n('\u{301}', marks))
            .collect()
    }

    fn tables(line: &Line) -> &Tables {
        line.tables.as_ref().unwrap()
    }

    #[test]
    fn a_new_line_is_blank_and_has_no_tables() {
        let line = Line::new(LineId::new(1), 80);
        assert_eq!(line.len(), 80);
        assert!(line.cells().iter().all(|c| c.is_blank()));
        assert_eq!(line.heap_bytes(), 80 * 8);
    }

    #[test]
    fn a_written_cell_reads_back_through_its_tables() {
        let mut line = Line::new(LineId::new(1), 10);
        let mut style = red();
        style.attrs.insert(Attrs::OVERLINE);
        style.attrs.insert(Attrs::CONCEAL);
        let link = LinkId::new(42);
        assert!(line.write(3, Glyph::Char('x'), Width::Narrow, &style, Some(link)));
        let cell = line.cell(3);
        assert_eq!(line.glyph(cell), Glyph::Char('x'));
        assert_eq!(line.style(cell), style);
        assert_eq!(line.link(cell), Some(link));

        assert!(line.write(4, Glyph::Cluster(FAMILY), Width::Wide, &style, None));
        let cell = line.cell(4);
        assert_eq!(line.glyph(cell), Glyph::Cluster(FAMILY));
        assert_eq!(cell.width(), Width::Wide);
        assert_eq!(line.link(cell), None);
    }

    #[test]
    fn a_cluster_of_one_codepoint_is_that_codepoint() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(0, Glyph::Cluster("e"), Width::Narrow, &Style::DEFAULT, None);
        assert_eq!(line.cell(0).content(), Content::Char('e'));
        line.write(1, Glyph::Cluster(""), Width::Narrow, &Style::DEFAULT, None);
        assert!(line.cell(1).is_blank());
        // Neither needed a table.
        assert!(line.tables.is_none());
        assert_eq!(line.heap_bytes(), 4 * 8);
    }

    // kmscon ADR 102, "Cells and clusters": extenders past 32 are dropped.
    #[test]
    fn a_cluster_keeps_its_first_cluster_max_codepoints() {
        let mut line = Line::new(LineId::new(1), 64);
        let long = accented(40);
        line.write(
            0,
            Glyph::Cluster(&long),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(
            line.glyph(line.cell(0)),
            Glyph::Cluster(&accented(CLUSTER_MAX - 1))
        );
    }

    // ADR-1001, "The line": a cluster past the line's budget keeps the
    // longest run of its first codepoints that fits, at least its first.
    #[test]
    fn a_cluster_past_the_budget_keeps_what_fits() {
        // Four cells: 24 bytes. The first cluster takes 1 + 1 + 2 * 11.
        let mut line = Line::new(LineId::new(1), 4);
        line.write(
            0,
            Glyph::Cluster(&accented(31)),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&accented(11)));
        line.write(
            1,
            Glyph::Cluster(&accented(31)),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(line.cell(1).content(), Content::Char('e'));
        // Overwriting the first gives its bytes back.
        line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
        line.write(
            1,
            Glyph::Cluster(&accented(2)),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(line.glyph(line.cell(1)), Glyph::Cluster(&accented(2)));
    }

    /// The arena bytes the line's cells name, counted from the cells.
    fn named_bytes(line: &Line) -> usize {
        line.cells()
            .iter()
            .map(|&c| match line.glyph(c) {
                Glyph::Cluster(s) => s.len().saturating_add(1),
                _ => 0,
            })
            .sum()
    }

    // Clusters of mixed sizes, maximal ones among them, written at
    // scattered columns through many compactions: the bytes the cells
    // name stay inside the budget, and `live` agrees with them.
    #[test]
    fn a_flood_of_mixed_clusters_stays_inside_the_budget() {
        for cols in [4, 21, 170] {
            let mut line = Line::new(LineId::new(1), cols);
            let mut seed = 0x9e37_79b9_u32;
            for _ in 0..20_000 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let col = (seed >> 8) as usize % cols;
                let marks = (seed >> 20) as usize % CLUSTER_MAX;
                let cluster = accented(marks);
                line.write(
                    col,
                    Glyph::Cluster(&cluster),
                    Width::Narrow,
                    &Style::DEFAULT,
                    None,
                );
                let named = named_bytes(&line);
                assert!(
                    named <= cols * CLUSTER_BYTES_PER_CELL,
                    "{named} bytes at {cols} cells"
                );
                assert_eq!(line.tables.as_ref().map_or(0, |t| t.live), named);
                assert!(
                    line.tables.as_ref().map_or(0, |t| t.clusters.len())
                        <= cols * CLUSTER_BYTES_PER_CELL * TABLE_GROWTH + CLUSTER_BYTES_MAX
                );
                match line.glyph(line.cell(col)) {
                    Glyph::Char('e') => {}
                    Glyph::Cluster(s) => assert!(cluster.starts_with(s)),
                    other => panic!("cell {col} holds {other:?}"),
                }
            }
            line.trim();
            assert!(named_bytes(&line) <= cols * CLUSTER_BYTES_PER_CELL);
        }
    }

    #[test]
    fn a_write_past_the_end_changes_nothing() {
        let mut line = Line::new(LineId::new(1), 4);
        assert!(!line.write(4, Glyph::Char('x'), Width::Narrow, &red(), None));
        assert_eq!(line.len(), 4);
        assert!(line.cells().iter().all(|c| c.is_blank()));
    }

    // libtsm#47: cells past a stored line's end were drawn with the pen,
    // so an SGR under BCE recoloured them. They read as the blank cell in
    // the default style.
    #[test]
    fn cells_past_the_end_read_blank_in_the_default_style() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(0, Glyph::Char('a'), Width::Narrow, &red(), None);
        line.trim();
        assert_eq!(line.len(), 1);
        let past = line.cell(10);
        assert!(past.is_blank());
        assert_eq!(line.style(past), Style::DEFAULT);
        assert_eq!(line.glyph(past), Glyph::Empty);
    }

    #[test]
    fn trim_keeps_cells_erased_in_a_colour() {
        let mut line = Line::new(LineId::new(1), 8);
        line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
        let bce = Style {
            bg: Color::Indexed(4),
            ..Style::DEFAULT
        };
        line.erase(2..5, &bce);
        line.trim();
        assert_eq!(line.len(), 5);
        assert_eq!(line.style(line.cell(4)).bg, Color::Indexed(4));
        assert!(line.cell(1).is_blank());
    }

    #[test]
    fn erase_clips_to_the_line_and_gives_back_clusters() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(
            3,
            Glyph::Cluster(&accented(3)),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(tables(&line).live, 8);
        line.erase(2..100, &red());
        assert!(line.cell(1).is_blank());
        assert_eq!(line.style(line.cell(3)), red());
        assert_eq!(line.glyph(line.cell(3)), Glyph::Empty);
        assert_eq!(tables(&line).live, 0);
        assert_eq!(line.len(), 4);
    }

    #[test]
    fn rewriting_one_cell_in_new_styles_keeps_the_table_bounded() {
        let mut line = Line::new(LineId::new(1), 4);
        for n in 0..10_000 {
            line.write(0, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
            let t = tables(&line).styles.len();
            assert!(t <= 4 * TABLE_GROWTH, "the style table grew to {t}");
            assert_eq!(line.style(line.cell(0)), rgb(n));
        }
    }

    #[test]
    fn compaction_keeps_what_every_cell_names() {
        let cols = 16;
        let mut line = Line::new(LineId::new(1), cols);
        let cluster = accented(1);
        // Fill every cell with its own style, link and cluster, then
        // overwrite them round after round so every table compacts.
        for round in 0..20u32 {
            for (col, c) in (0..cols).zip(0u32..) {
                let n = round * 100 + c;
                let link = LinkId::new(n);
                line.write(
                    col,
                    Glyph::Cluster(&cluster),
                    Width::Narrow,
                    &rgb(n),
                    Some(link),
                );
            }
            for (col, c) in (0..cols).zip(0u32..) {
                let n = round * 100 + c;
                let cell = line.cell(col);
                assert_eq!(line.style(cell), rgb(n));
                assert_eq!(line.link(cell), Some(LinkId::new(n)));
                assert_eq!(line.glyph(cell), Glyph::Cluster(&cluster));
            }
            assert!(tables(&line).styles.len() <= TABLE_GROWTH * cols);
        }
    }

    // A style index that passed 16 bits would collapse to another entry or
    // the default. Distinct styles across the widest line, through several
    // compactions, each read back as written.
    #[test]
    fn distinct_styles_across_the_widest_line_survive_compaction() {
        let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
        let mut n = 0u32;
        for _ in 0..3 {
            for col in 0..COLUMNS_MAX {
                n += 1;
                line.write(col, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
            }
            let first = n - u32::try_from(COLUMNS_MAX).unwrap() + 1;
            for (col, m) in (0..COLUMNS_MAX).zip(first..) {
                assert_eq!(line.style(line.cell(col)), rgb(m), "column {col}");
            }
        }
    }

    #[test]
    fn trim_frees_tables_no_cell_names() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(
            0,
            Glyph::Char('a'),
            Width::Narrow,
            &red(),
            Some(LinkId::new(1)),
        );
        line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
        line.trim();
        assert!(line.tables.is_none());
        assert_eq!(line.heap_bytes(), 8);
    }

    #[test]
    fn resize_pads_with_blank_cells_and_clamps() {
        let mut line = Line::new(LineId::new(1), 2);
        line.write(1, Glyph::Char('b'), Width::Narrow, &red(), None);
        line.resize(5);
        assert_eq!(line.len(), 5);
        assert!(line.cell(4).is_blank());
        assert_eq!(line.glyph(line.cell(1)), Glyph::Char('b'));
        line.resize(usize::MAX);
        assert_eq!(line.len(), COLUMNS_MAX);
        assert_eq!(Line::new(LineId::new(2), usize::MAX).len(), COLUMNS_MAX);
    }

    // Shrinking keeps no more tables than the shorter line can name.
    #[test]
    fn shrinking_compacts_the_tables() {
        let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
        for (col, n) in (0..COLUMNS_MAX).zip(0u32..) {
            let link = Some(LinkId::new(n));
            line.write(
                col,
                Glyph::Cluster(&accented(2)),
                Width::Narrow,
                &rgb(n),
                link,
            );
        }
        line.resize(1);
        let t = tables(&line);
        assert_eq!((t.styles.len(), t.links.len(), t.clusters.len()), (1, 1, 6));
        assert_eq!(line.heap_bytes(), 8 + size_of::<Tables>() + 14 + 4 + 6);
        assert_eq!(line.style(line.cell(0)), rgb(0));
        assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&accented(2)));
    }

    #[test]
    fn flags_and_size_are_independent() {
        let mut f = LineFlags::empty();
        f.insert(LineFlags::WRAPPED);
        f.insert(LineFlags::PROMPT);
        f.set_size(LineSize::DoubleHeightBottom);
        assert!(f.contains(LineFlags::WRAPPED));
        assert!(f.contains(LineFlags::PROMPT));
        assert!(!f.contains(LineFlags::OUTPUT));
        assert_eq!(f.size(), LineSize::DoubleHeightBottom);
        f.remove(LineFlags::WRAPPED);
        f.set_size(LineSize::Single);
        assert!(!f.contains(LineFlags::WRAPPED));
        assert!(f.contains(LineFlags::PROMPT));
        assert_eq!(f.size(), LineSize::Single);
    }
}
