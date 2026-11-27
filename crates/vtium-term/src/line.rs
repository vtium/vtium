//! The line: a row of cells, the tables its cells index, its identity and
//! its flags.
//!
//! Styles, hyperlinks and grapheme clusters live in tables owned by the
//! line, so a cell names each in 16 or 21 bits, and a line that drops out
//! of the scrollback frees everything its cells referred to. No table is
//! shared between lines, so none needs a reference count or a hash.
//!
//! A table holds only what was written to the line. Once the style or link
//! table reaches one and a half times the line's cells, or the cluster
//! arena one and a half times the line's cluster budget, the line compacts:
//! it keeps the entries a cell still names and renumbers them
//! ([`compaction_limit`]). A compaction,
//! like looking a style or link up in its table, costs time linear in the
//! line's width, so a write is linear in the line's width at worst.
//!
//! The cluster arena holds each cluster as a length byte and its codepoints
//! in UTF-8, and the cell holds the cluster's byte offset. The clusters a
//! line's cells name take at most [`CLUSTER_BYTES_PER_CELL`] bytes per
//! cell of the width the line is written at; a cluster that would pass that
//! keeps the longest run of its first codepoints that fits, and at least
//! its first codepoint.
//!
//! In a build with images, a cluster that starts with U+10EEEE, the kitty
//! graphics protocol's Unicode placeholder, sits outside that budget
//! (ADR-1001, "Placeholder cells"): it keeps the placeholder and its first [`PLACEHOLDER_MARKS`]
//! codepoints after it, at most [`PLACEHOLDER_BYTES_MAX`] bytes, whatever
//! the other clusters take, since those marks carry its picture's row, its
//! column and the top byte of its image id. A line counts the cells that
//! name a placeholder cluster, and its arena compacts at one and a half
//! times its budget and [`PLACEHOLDER_BYTES_MAX`] for each of them.
//!
//! References:
//! - ADR-1009 D3: the tables compact at one and a half times what the
//!   line's cells can name.

use crate::bounds::{
    CLUSTER_BYTES_PER_CELL, COLUMNS_MAX, PLACEHOLDER_BYTES_MAX, compaction_limit, table_limit,
};
#[cfg(doc)]
use crate::bounds::{CLUSTER_MAX, PLACEHOLDER_MARKS};
use crate::cell::{Cell, ClusterOffset, Content, LinkIndex, StyleIndex, Width};
use crate::style::Style;

mod edit;
mod memory;
mod runs;
mod tables;

pub(crate) use memory::{run_fit, whole_clusters};
pub use runs::{SizedRun, TextSize};
pub(crate) use tables::IndexMap;
use tables::Tables;

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

/// A hyperlink in the link store (vtium/vtium#40, ADR-1002), as a line's
/// link table holds it. Outside the terminal an id is valid only until the
/// next [`Terminal::input`](crate::Terminal::input): a sweep of the store
/// renumbers the links it keeps.
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
/// (vtium/vtium#61). A line of double size draws each character twice as
/// wide, so it holds half as many columns as the screen: the cursor, the
/// right margin, printing and editing count those columns only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineSize {
    #[default]
    Single,
    DoubleWidth,
    DoubleHeightTop,
    DoubleHeightBottom,
}

impl LineSize {
    /// Whether the line draws each character twice as wide: every size
    /// but single. The two halves of a double-height line are double width
    /// too.
    pub const fn is_double(self) -> bool {
        !matches!(self, Self::Single)
    }

    /// How many columns a line of this size holds on a screen `cols`
    /// columns wide: all of them at single size, and half of them at
    /// double size. A double-size line on an odd width leaves the last
    /// screen column empty, since no character fits there at twice the
    /// width. A screen one column wide still gives a double-size line one
    /// column, so the cursor has a place on it.
    pub const fn columns(self, cols: usize) -> usize {
        if self.is_double() {
            let half = cols / 2;
            if half == 0 { 1 } else { half }
        } else {
            cols
        }
    }
}

/// What a line records about itself in 16 bits: the reflow flag, the
/// three shell-integration marks, the line size, the mark of mode 2028
/// reset, the four properties of the paragraph the line belongs to
/// (ADR-1004 D2), whether it has held a character that draws its paragraph
/// in another order (ADR-1004 D8), and four bits spare.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineFlags(u16);

impl LineFlags {
    /// The line's text continues on the next line: printing wrapped at the
    /// right margin. A line the program ended with a newline, or left by
    /// moving the cursor, does not carry it. Reflow (ADR-1003) rejoins
    /// only lines that do.
    pub const WRAPPED: Self = Self(1 << 0);
    /// OSC 133 A: a prompt starts on this line.
    pub const PROMPT: Self = Self(1 << 1);
    /// OSC 133 B: the command's input starts on this line.
    pub const INPUT: Self = Self(1 << 2);
    /// OSC 133 C: the command's output starts on this line.
    pub const OUTPUT: Self = Self(1 << 3);
    /// Written while mode 2028 was reset: a resize never rewraps the line,
    /// and cuts it at a narrower margin (ADR-1003).
    pub const FIXED: Self = Self(1 << 6);
    /// The shell-integration marks.
    pub const MARKS: Self = Self(Self::PROMPT.0 | Self::INPUT.0 | Self::OUTPUT.0);
    /// The paragraph is in explicit mode: BDSM was reset (`CSI 8 l`), so
    /// vtium draws its cells in the order they were written, in the
    /// paragraph's direction. Without it the paragraph is in implicit mode,
    /// BDSM's default (ADR-1004 D3, D4).
    pub const EXPLICIT: Self = Self(1 << 7);
    /// The paragraph runs right to left: SCP 2, SPD 3, or vtium.conf's
    /// `text-direction = rtl`. Without it the paragraph runs left to right
    /// (ADR-1004 D5).
    pub const RTL: Self = Self(1 << 8);
    /// The paragraph's direction is detected from its first strong
    /// character: mode 2501, or `text-direction = auto` (ADR-1004 D5).
    pub const DETECT: Self = Self(1 << 9);
    /// Box-drawing characters in a right-to-left paragraph draw mirrored:
    /// mode 2500 (ADR-1004 D7).
    pub const BOX_MIRROR: Self = Self(1 << 10);
    /// The paragraph's four properties. Every row of a paragraph holds the
    /// same values of them (ADR-1004 D2).
    pub const PARAGRAPH: Self =
        Self(Self::EXPLICIT.0 | Self::RTL.0 | Self::DETECT.0 | Self::BOX_MIRROR.0);
    /// The line has held a right-to-left or joining character since it was
    /// written, or since a rewrap made or kept it, in a build with the bidi
    /// feature. A frame draws its paragraph through the paragraph's order
    /// and redraws all of the paragraph's rows when any of them changes,
    /// so a row whose last right-to-left character was erased is redrawn
    /// in its new order too (ADR-1004 D8). A rewrap, which redraws the
    /// whole screen, sets or clears it on each row it makes or keeps, from
    /// the characters the row holds.
    pub(crate) const REORDERS: Self = Self(1 << 11);

    const FLAGS: u16 = ((1 << 4) - 1) | Self::FIXED.0 | Self::PARAGRAPH.0 | Self::REORDERS.0;
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

    /// The flags of `other` this one holds.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0 & Self::FLAGS)
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

impl core::ops::BitOr for LineFlags {
    type Output = Self;

    fn bitor(mut self, other: Self) -> Self {
        self.insert(other);
        self
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

/// The first codepoint of `glyph`, the one the bidirectional algorithm
/// reads for its cell (ADR-1004 D3).
#[cfg(feature = "bidi")]
fn first_char(glyph: Glyph<'_>) -> Option<char> {
    match glyph {
        Glyph::Empty => None,
        Glyph::Char(c) => Some(c),
        Glyph::Cluster(s) => s.chars().next(),
    }
}

/// U+10EEEE, the kitty graphics protocol's Unicode placeholder (ADR-1100,
/// D13).
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';

/// Whether the cluster `s` is a placeholder cluster: one that starts with
/// [`PLACEHOLDER`], in a build with images. A build without images keeps
/// U+10EEEE as any private-use character, its marks within the budget.
pub(crate) fn is_placeholder(s: &str) -> bool {
    cfg!(feature = "images") && s.starts_with(PLACEHOLDER)
}

/// A row of cells.
///
/// A line may hold fewer cells than the grid is wide: a line trimmed for
/// the scrollback keeps its text and drops its fill, the run of empty
/// cells of one style that ended it at the margin, keeping the fill's
/// style. Every cell past its end reads as an empty cell in that style,
/// whatever the pen: the default style unless the program erased the
/// line's end in a color (ADR-1003).
///
/// [`Width::Pad`] cells sit only at the end of a wrapped line: one in the
/// last column where a wide character moved to the next row, and up to a
/// sized run's columns where a rewrap moved the run. An edit that ends the
/// wrap, or moves such a cell, makes it an empty narrow cell.
///
/// A line's run table holds its share of each sized run that crosses it
/// ([`runs`](Self::runs)). Every edit of the line keeps the table and the
/// cells' run marks in step: an edit that would cut a run's cells on this
/// line takes the run off the line, and a move of all of them moves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    cells: Vec<Cell>,
    tables: Option<Box<Tables>>,
    id: LineId,
    flags: LineFlags,
    /// The style index of the cells past the line's end.
    fill: u16,
    /// The line may hold a wide character: set by any write of one, and
    /// made exact when the line is trimmed, so a rewrap's count reads the
    /// lengths of lines without one (ADR-1003, "Cost").
    wide: bool,
    /// The width, in cells, whose cluster budget the clusters the line's
    /// cells name were written within: the width the line was written at,
    /// or the widest of the rows a rewrap built it from. A row that lands
    /// on a narrower screen is held to that screen's budget
    /// ([`paint`](Self::paint)).
    written_at: u16,
}

// The Vec and the Box are four pointer-sized words, so a Line is 48 bytes
// on 64-bit targets and 32 on armv7; the fill, the wide mark and the width
// the line was written at take 5 bytes of the padding.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Line>() == 48);
#[cfg(target_pointer_width = "32")]
const _: () = assert!(size_of::<Line>() == 32);

impl Line {
    /// A line of `cols` blank cells, at most [`COLUMNS_MAX`].
    pub fn new(id: LineId, cols: usize) -> Self {
        Self {
            cells: vec![Cell::BLANK; cols.min(COLUMNS_MAX)],
            tables: None,
            id,
            flags: LineFlags::empty(),
            fill: 0,
            wide: false,
            written_at: width_of(cols),
        }
    }

    /// The most bytes a screen row of `cols` cells holds, `cols` at least 1
    /// and at most [`COLUMNS_MAX`]: its struct, its cells, and its style
    /// table, link table and cluster arena each at the limit it compacts
    /// at, by capacity. A row that `write` and `erase` fill reaches it; a
    /// row a rewrap built and a later write compacted holds fewer styles
    /// and links and stays under it. `tests/memory.rs` drives rows on both
    /// paths and holds each to it. In a build with images, every cell may
    /// name a placeholder cluster ([`bytes_max_in`](Self::bytes_max_in)).
    pub const fn bytes_max(cols: usize) -> usize {
        Self::bytes_max_in(cols, cfg!(feature = "images"))
    }

    /// [`bytes_max`](Self::bytes_max) in a build with images (`images`
    /// true) or without: with images, every cell may name a placeholder
    /// cluster, which raises what the arena compacts at by 17 bytes a
    /// cell before the one and a half times. The bound counts the run
    /// table at its most, one entry for each cell, since the runs on a
    /// line never overlap.
    ///
    /// References:
    /// - ADR-1001, "Placeholder cells": the 17 bytes a placeholder cell
    ///   adds outside the budget.
    /// - ADR-1009 D1: the bound counts the run table.
    /// - ADR-1009 D6: the figures this gives.
    pub const fn bytes_max_in(cols: usize, images: bool) -> usize {
        let cols = if cols == 0 {
            1
        } else if cols > COLUMNS_MAX {
            COLUMNS_MAX
        } else {
            cols
        };
        let placeholder = if images { PLACEHOLDER_BYTES_MAX } else { 0 };
        let entries =
            table_limit(cols).saturating_mul(size_of::<Style>().saturating_add(size_of::<u16>()));
        let arena = compaction_limit(
            cols.saturating_mul(CLUSTER_BYTES_PER_CELL.saturating_add(placeholder)),
        );
        let runs =
            size_of::<Vec<SizedRun>>().saturating_add(cols.saturating_mul(size_of::<SizedRun>()));
        size_of::<Self>()
            .saturating_add(cols.saturating_mul(size_of::<Cell>()))
            .saturating_add(size_of::<Tables>())
            .saturating_add(entries)
            .saturating_add(arena)
            .saturating_add(runs)
    }

    /// An empty line with room for `cols` cells, for a rewrap to fill.
    pub(crate) fn with_capacity(id: LineId, flags: LineFlags, cols: usize) -> Self {
        Self {
            cells: Vec::with_capacity(cols.min(COLUMNS_MAX)),
            tables: None,
            id,
            flags,
            fill: 0,
            wide: false,
            written_at: width_of(cols),
        }
    }

    pub fn id(&self) -> LineId {
        self.id
    }

    /// Gives the line the id of a row a rewrap made.
    pub(crate) fn set_id(&mut self, id: LineId) {
        self.id = id;
    }

    pub fn flags(&self) -> LineFlags {
        self.flags
    }

    pub fn flags_mut(&mut self) -> &mut LineFlags {
        &mut self.flags
    }

    /// Ends the line's wrap: its text no longer continues on the next
    /// line. The pads that end it become empty narrow cells, and it
    /// returns whether any did, a change a frame draws.
    pub fn unwrap(&mut self) -> bool {
        self.flags.remove(LineFlags::WRAPPED);
        // A wide character leaves one pad, and a sized run that moved to
        // the next row at a rewrap may leave several.
        let mut changed = false;
        for last in self.cells.iter_mut().rev() {
            if last.width() != Width::Pad {
                break;
            }
            *last = Cell::new(Content::Empty, Width::Narrow, last.style(), last.link());
            changed = true;
        }
        changed
    }

    /// Sets the line's reorder mark from the characters it holds now, and
    /// clears the mark when none of them reorders. A rewrap does this for a
    /// row it keeps as it is, as it does for each row it makes, so the mark
    /// after a rewrap depends only on the row's characters. A row that
    /// waited through several widths ends with the same mark it would have
    /// after a rewrap at each width.
    ///
    /// References:
    /// - ADR-1004 D8: what the mark records and why a frame reads it.
    #[cfg(feature = "bidi")]
    pub(crate) fn mark_reorders_from_text(&mut self) {
        let reorders = self
            .cells
            .iter()
            .any(|&c| first_char(self.glyph(c)).is_some_and(crate::unicode::bidi::reorders));
        if reorders {
            self.flags.insert(LineFlags::REORDERS);
        } else {
            self.flags.remove(LineFlags::REORDERS);
        }
    }

    /// Whether the line may hold a wide character; false only when it
    /// holds none.
    pub(crate) fn holds_wide(&self) -> bool {
        self.wide
    }

    /// The style of the cells past the line's end.
    pub fn fill(&self) -> Style {
        self.style(self.fill_cell())
    }

    fn fill_cell(&self) -> Cell {
        Cell::new(
            Content::Empty,
            Width::Narrow,
            StyleIndex(self.fill),
            LinkIndex(0),
        )
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

    /// The cell at `col`, or past the line's end an empty cell in its
    /// fill.
    pub fn cell(&self, col: usize) -> Cell {
        self.cells
            .get(col)
            .copied()
            .unwrap_or_else(|| self.fill_cell())
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
            .map(|&n| LinkId::new(u32::from(n)))
    }

    /// The links this line's link table holds, each once: those its cells
    /// name, and those overwritten since it last compacted.
    pub(crate) fn link_table(&self) -> impl Iterator<Item = LinkId> + '_ {
        self.tables
            .iter()
            .flat_map(|t| t.links.iter().map(|&n| LinkId::new(u32::from(n))))
    }

    /// Renames each link of the table to `rename` of it, as a sweep of the
    /// link store renumbers them; a link it gives no new name for leaves
    /// the cells that named it unlinked.
    pub(crate) fn rename_links(&mut self, rename: impl Fn(LinkId) -> Option<LinkId>) {
        let Some(t) = self.tables.as_mut() else {
            return;
        };
        // The table indices, 1-based, of links with no new name; empty,
        // and unallocated, on a line that loses none.
        let mut gone = Vec::new();
        for (i, link) in t.links.iter_mut().enumerate() {
            match rename(LinkId::new(u32::from(*link)))
                .and_then(|new| u16::try_from(new.get()).ok())
            {
                Some(new) => *link = new,
                None => gone.push(i.saturating_add(1)),
            }
        }
        if gone.is_empty() {
            return;
        }
        for cell in &mut self.cells {
            if gone.contains(&usize::from(cell.link().0)) {
                *cell = Cell::new(cell.content(), cell.width(), cell.style(), LinkIndex(0))
                    .with_run(cell.in_run());
            }
        }
        self.compact();
        self.shrink_tables();
    }

    /// Unlinks every cell and empties the link table, as the link store
    /// strips the scrollback's oldest lines. Returns whether the table held
    /// any link.
    pub(crate) fn strip_links(&mut self) -> bool {
        if self.link_table().next().is_none() {
            return false;
        }
        for cell in &mut self.cells {
            *cell = Cell::new(cell.content(), cell.width(), cell.style(), LinkIndex(0))
                .with_run(cell.in_run());
        }
        self.compact();
        self.shrink_tables();
        true
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
}

/// `cols` as a line records the width it was written at.
fn width_of(cols: usize) -> u16 {
    u16::try_from(cols.min(COLUMNS_MAX)).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests;
