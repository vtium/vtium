//! The line: a row of cells, the tables its cells index, its identity and
//! its flags.
//!
//! Styles, hyperlinks and grapheme clusters live in tables owned by the
//! line, so a cell names each in 16 or 21 bits, and a line that drops out
//! of the scrollback frees everything its cells referred to. No table is
//! shared between lines, so none needs a reference count or a hash.
//!
//! A table holds only what was written to the line. Once a table reaches
//! twice the line's cells, the line compacts: it keeps the entries a cell
//! still names and renumbers them. Each compaction follows at least as
//! many writes as the line has cells, so writes stay amortised constant
//! time, and a table never holds more than twice the line's cells.

use core::ops::Range;

use crate::bounds::{CLUSTER_MAX, COLUMNS_MAX};
use crate::cell::{Cell, ClusterIndex, Content, LinkIndex, StyleIndex, Width};
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
    Cluster(&'a [char]),
}

/// Where a cluster's codepoints sit in the line's arena.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: u32,
    len: u32,
}

/// A line's tables, allocated on its first styled, linked or clustered
/// cell. Index `i` of a style or link names entry `i - 1`; index 0 is the
/// default style or no link.
#[derive(Clone, Debug, Default)]
struct Tables {
    styles: Vec<Style>,
    links: Vec<LinkId>,
    clusters: Vec<Span>,
    chars: Vec<char>,
}

impl Tables {
    fn is_empty(&self) -> bool {
        self.styles.is_empty() && self.links.is_empty() && self.clusters.is_empty()
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

    /// What `cell` shows, its cluster read from this line's table.
    pub fn glyph(&self, cell: Cell) -> Glyph<'_> {
        match cell.content() {
            Content::Empty => Glyph::Empty,
            Content::Char(c) => Glyph::Char(c),
            Content::Cluster(ClusterIndex(i)) => self
                .tables
                .as_ref()
                .and_then(|t| cluster_chars(t, i))
                .map_or(Glyph::Empty, Glyph::Cluster),
        }
    }

    /// Writes one cell. A cluster of one codepoint is stored as that
    /// codepoint, and one past [`CLUSTER_MAX`] keeps its first codepoints.
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
        self.make_room();
        let style = self.intern_style(style);
        let link = self.intern_link(link);
        let content = match glyph {
            Glyph::Empty | Glyph::Cluster([]) => Content::Empty,
            Glyph::Char(c) | Glyph::Cluster(&[c]) => Content::Char(c),
            Glyph::Cluster(chars) => self.push_cluster(chars),
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
        if let Some(cells) = self.cells.get_mut(cols.start..end) {
            cells.fill(blank);
        }
    }

    /// Makes the line `cols` cells long, at most [`COLUMNS_MAX`]: cells
    /// past the new end are dropped, and new cells are blank.
    pub fn resize(&mut self, cols: usize) {
        self.cells.resize(cols.min(COLUMNS_MAX), Cell::BLANK);
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
        if let Some(t) = self.tables.as_mut() {
            t.styles.shrink_to_fit();
            t.links.shrink_to_fit();
            t.clusters.shrink_to_fit();
            t.chars.shrink_to_fit();
        }
    }

    /// The bytes the line holds on the heap, by capacity.
    pub fn heap_bytes(&self) -> usize {
        let cells = self.cells.capacity().saturating_mul(size_of::<Cell>());
        let tables = self.tables.as_ref().map_or(0, |t| {
            [
                size_of::<Tables>(),
                t.styles.capacity().saturating_mul(size_of::<Style>()),
                t.links.capacity().saturating_mul(size_of::<LinkId>()),
                t.clusters.capacity().saturating_mul(size_of::<Span>()),
                t.chars.capacity().saturating_mul(size_of::<char>()),
            ]
            .into_iter()
            .fold(0, usize::saturating_add)
        });
        cells.saturating_add(tables)
    }

    /// The most entries a table holds before the line compacts it.
    fn table_limit(&self) -> usize {
        self.cells.len().max(1).saturating_mul(2)
    }

    /// Compacts the tables when one more entry in any of them could pass
    /// its limit, so the indices `write` and `erase` intern stay valid.
    fn make_room(&mut self) {
        let limit = self.table_limit();
        let full = self.tables.as_ref().is_some_and(|t| {
            t.styles.len() >= limit
                || t.links.len() >= limit
                || t.clusters.len() >= limit
                || t.chars.len().saturating_add(CLUSTER_MAX) > limit.saturating_mul(CLUSTER_MAX)
        });
        if full {
            self.compact();
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
        let mut clusters = vec![None::<u32>; old.clusters.len()];
        for cell in &mut self.cells {
            let style = remap(&mut styles, cell.style().0, &old.styles, &mut new.styles);
            let link = remap(&mut links, cell.link().0, &old.links, &mut new.links);
            let content = match cell.content() {
                Content::Cluster(ClusterIndex(i)) => {
                    match remap_cluster(&mut clusters, i, &old, &mut new) {
                        Some(j) => Content::Cluster(ClusterIndex(j)),
                        None => Content::Empty,
                    }
                }
                other => other,
            };
            *cell = Cell::new(content, cell.width(), StyleIndex(style), LinkIndex(link));
        }
        if !new.is_empty() {
            self.tables = Some(Box::new(new));
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

    fn push_cluster(&mut self, chars: &[char]) -> Content {
        let chars = chars.get(..CLUSTER_MAX).unwrap_or(chars);
        let t = self.tables.get_or_insert_default();
        let (Ok(start), Ok(len), Ok(index)) = (
            u32::try_from(t.chars.len()),
            u32::try_from(chars.len()),
            u32::try_from(t.clusters.len()),
        ) else {
            return Content::Empty;
        };
        t.chars.extend_from_slice(chars);
        t.clusters.push(Span { start, len });
        Content::Cluster(ClusterIndex(index))
    }
}

/// The 1-based index of `value` in `table`, appended if absent. The table
/// is short (`make_room` keeps it within twice the line's cells) and the
/// newest entry is the likeliest match, so the search runs from the end.
fn intern<T: PartialEq + Copy>(table: &mut Vec<T>, value: &T) -> u16 {
    let i = match table.iter().rposition(|v| v == value) {
        Some(i) => i,
        None => {
            table.push(*value);
            table.len().saturating_sub(1)
        }
    };
    u16::try_from(i.saturating_add(1)).unwrap_or(0)
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
        *slot = u16::try_from(new.len()).unwrap_or(0);
    }
    *slot
}

/// The new index of old cluster `i`, copying its codepoints into `new`
/// the first time a cell names it.
fn remap_cluster(map: &mut [Option<u32>], i: u32, old: &Tables, new: &mut Tables) -> Option<u32> {
    let at = usize::try_from(i).ok()?;
    let slot = map.get_mut(at)?;
    if slot.is_none() {
        let chars = cluster_chars(old, i)?;
        let start = u32::try_from(new.chars.len()).ok()?;
        let len = u32::try_from(chars.len()).ok()?;
        let index = u32::try_from(new.clusters.len()).ok()?;
        new.chars.extend_from_slice(chars);
        new.clusters.push(Span { start, len });
        *slot = Some(index);
    }
    *slot
}

fn cluster_chars(t: &Tables, i: u32) -> Option<&[char]> {
    let span = t.clusters.get(usize::try_from(i).ok()?)?;
    let start = usize::try_from(span.start).ok()?;
    let end = start.checked_add(usize::try_from(span.len).ok()?)?;
    t.chars.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Attrs, Color, Rgb};

    fn red() -> Style {
        Style {
            fg: Color::Indexed(1),
            ..Style::DEFAULT
        }
    }

    fn rgb(n: u16) -> Style {
        let [hi, lo] = n.to_be_bytes();
        Style {
            fg: Color::Rgb(Rgb::new(hi, lo, 0)),
            ..Style::DEFAULT
        }
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

        let family = [
            '\u{1F468}',
            '\u{200D}',
            '\u{1F469}',
            '\u{200D}',
            '\u{1F467}',
        ];
        assert!(line.write(4, Glyph::Cluster(&family), Width::Wide, &style, None));
        let cell = line.cell(4);
        assert_eq!(line.glyph(cell), Glyph::Cluster(&family));
        assert_eq!(cell.width(), Width::Wide);
        assert_eq!(line.link(cell), None);
    }

    #[test]
    fn a_cluster_of_one_codepoint_is_that_codepoint() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(
            0,
            Glyph::Cluster(&['e']),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(line.cell(0).content(), Content::Char('e'));
        line.write(1, Glyph::Cluster(&[]), Width::Narrow, &Style::DEFAULT, None);
        assert!(line.cell(1).is_blank());
        // Neither needed a table.
        assert_eq!(line.heap_bytes(), 4 * 8);
    }

    // kmscon ADR 102, "Cells and clusters": extenders past 32 are dropped.
    #[test]
    fn a_cluster_keeps_its_first_cluster_max_codepoints() {
        let mut line = Line::new(LineId::new(1), 4);
        let mut long = vec!['e'];
        long.extend(std::iter::repeat_n('\u{301}', 40));
        line.write(
            0,
            Glyph::Cluster(&long),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(
            line.glyph(line.cell(0)),
            Glyph::Cluster(long.get(..CLUSTER_MAX).unwrap_or_default())
        );
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
    fn erase_clips_to_the_line() {
        let mut line = Line::new(LineId::new(1), 4);
        line.erase(2..100, &red());
        assert!(line.cell(1).is_blank());
        assert_eq!(line.style(line.cell(3)), red());
        assert_eq!(line.len(), 4);
    }

    #[test]
    fn rewriting_one_cell_in_new_styles_keeps_the_table_bounded() {
        let mut line = Line::new(LineId::new(1), 4);
        for n in 0..10_000u16 {
            line.write(0, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
            let t = line.tables.as_ref().map_or(0, |t| t.styles.len());
            assert!(t <= 8, "the style table grew to {t}");
            assert_eq!(line.style(line.cell(0)), rgb(n));
        }
    }

    #[test]
    fn compaction_keeps_what_every_cell_names() {
        let cols = 16;
        let mut line = Line::new(LineId::new(1), cols);
        let family = ['\u{1F468}', '\u{200D}', '\u{1F469}'];
        // Fill every cell with its own style, link and cluster, then
        // overwrite them round after round so every table compacts.
        for round in 0..20u16 {
            for (col, c) in (0..cols).zip(0u16..) {
                let n = round.wrapping_mul(100).wrapping_add(c);
                let link = LinkId::new(u32::from(n));
                line.write(
                    col,
                    Glyph::Cluster(&family),
                    Width::Wide,
                    &rgb(n),
                    Some(link),
                );
            }
            for (col, c) in (0..cols).zip(0u16..) {
                let n = round.wrapping_mul(100).wrapping_add(c);
                let cell = line.cell(col);
                assert_eq!(line.style(cell), rgb(n));
                assert_eq!(line.link(cell), Some(LinkId::new(u32::from(n))));
                assert_eq!(line.glyph(cell), Glyph::Cluster(&family));
            }
            let t = line.tables.as_ref().map_or(0, |t| t.styles.len());
            assert!(t <= 2 * cols);
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
