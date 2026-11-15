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

use core::mem;
use core::ops::Range;
use std::collections::BTreeMap;

use crate::bounds::{
    CLUSTER_BYTES_MAX, CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, COLUMNS_MAX, PLACEHOLDER_BYTES_MAX,
    PLACEHOLDER_MARKS, compaction_limit, table_limit,
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
    /// written, or since a rewrap made it, in a build with the bidi
    /// feature. A frame draws its paragraph through the paragraph's order
    /// and redraws all of the paragraph's rows when any of them changes,
    /// so a row whose last right-to-left character was erased is redrawn
    /// in its new order too (ADR-1004 D8). A rewrap, which redraws the
    /// whole screen, sets it on each row it makes from the characters the
    /// row holds.
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

/// How one OSC 66 sequence sizes its text, with each of kitty's text
/// sizing protocol's keys inside its range.
///
/// References:
/// - ADR-1308 D2: the six keys vtium takes, their ranges and their
///   defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSize {
    /// `s`, 1 to 7: the run is this many rows high, and this many times as
    /// wide as its text's own cells.
    pub scale: u8,
    /// `w`, 0 to 7: the width in cells that the program gave the text, or
    /// 0 when vtium measured the text itself.
    pub width: u8,
    /// `n`, 0 to 15: the numerator of the fraction the text draws at.
    pub numerator: u8,
    /// `d`, 0 to 15: the denominator of that fraction, 0 when the text
    /// fills its block, and otherwise above the numerator.
    pub denominator: u8,
    /// `v`, 0 to 2: where text drawn at a fraction sits in its block, at
    /// the top (0), the bottom (1) or the middle (2).
    pub vertical: u8,
    /// `h`, 0 to 2: where text drawn at a fraction sits in its block, at
    /// the left (0), the right (1) or the middle (2).
    pub horizontal: u8,
}

impl TextSize {
    /// Each key at its default: text of the normal size.
    pub const NORMAL: Self = Self {
        scale: 1,
        width: 0,
        numerator: 0,
        denominator: 0,
        vertical: 0,
        horizontal: 0,
    };

    /// Whether the program set the text's width with `w`, so that no
    /// character that follows changes it.
    pub const fn width_set(self) -> bool {
        self.width != 0
    }

    /// Whether this size draws text just as printing it does: scale 1, no
    /// width given and no fraction. OSC 66 then makes no run.
    pub const fn is_normal(self) -> bool {
        self.scale <= 1 && self.width == 0 && self.denominator == 0
    }
}

/// One line's share of a sized run: the block of cells that one OSC 66
/// sequence writes, which every edit treats as one unit. Each line that a
/// row of the run lies on holds the row it holds, so no line refers to
/// another one.
///
/// References:
/// - ADR-1308 D3: the line's run table and what each entry holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SizedRun {
    col: u16,
    /// The rest of the entry in 27 of 32 bits, kept as two 16-bit halves
    /// so the entry aligns to 2 bytes and takes 6: the run's width in
    /// columns, 1 to 49 in 6 bits, which row of the run the line holds, 0
    /// to 6 in 3 bits, then the keys s, w, n, d, v and h in 3, 3, 4, 4, 2
    /// and 2 bits ([`SizedRun::FIELDS`]).
    packed: [u16; 2],
}

// A run's entry takes 6 bytes in its line's run table, on every target.
const _: () = assert!(size_of::<SizedRun>() == 6 && align_of::<SizedRun>() == 2);

impl SizedRun {
    /// The bit each field of the packed entry starts at, and its width in
    /// bits, in the order of [`SizedRun::fields`].
    const FIELDS: [(u32, u32); 8] = [
        (0, 6),
        (6, 3),
        (9, 3),
        (12, 3),
        (15, 4),
        (19, 4),
        (23, 2),
        (25, 2),
    ];

    /// The share of a run that starts at column `col` and is `cols`
    /// columns wide, for the run's row `row`, counted from 0 at its top.
    /// Each value keeps the bits its field has: OSC 66's keys and the run
    /// sizes they make fit them.
    ///
    /// References:
    /// - ADR-1009 D2: the 6-byte entry and the width of each field.
    pub(crate) fn new(col: usize, cols: usize, row: usize, size: TextSize) -> Self {
        let values = [
            u32::try_from(cols).unwrap_or(u32::MAX),
            u32::try_from(row).unwrap_or(u32::MAX),
            u32::from(size.scale),
            u32::from(size.width),
            u32::from(size.numerator),
            u32::from(size.denominator),
            u32::from(size.vertical),
            u32::from(size.horizontal),
        ];
        let mut bits = 0u32;
        for ((shift, width), value) in Self::FIELDS.into_iter().zip(values) {
            let max = (1u32 << width).saturating_sub(1);
            bits |= value.min(max) << shift;
        }
        let [a, b, c, d] = bits.to_le_bytes();
        Self {
            col: u16::try_from(col).unwrap_or(u16::MAX),
            packed: [u16::from_le_bytes([a, b]), u16::from_le_bytes([c, d])],
        }
    }

    /// The packed fields' values, in the order [`SizedRun::FIELDS`] lists
    /// them.
    fn fields(self) -> [u8; 8] {
        let [a, b] = self.packed[0].to_le_bytes();
        let [c, d] = self.packed[1].to_le_bytes();
        let bits = u32::from_le_bytes([a, b, c, d]);
        Self::FIELDS.map(|(shift, width)| {
            let max = (1u32 << width).saturating_sub(1);
            u8::try_from((bits >> shift) & max).unwrap_or(u8::MAX)
        })
    }

    /// The run's first column.
    pub fn col(self) -> usize {
        usize::from(self.col)
    }

    /// How many columns the run takes.
    pub fn cols(self) -> usize {
        usize::from(self.fields()[0])
    }

    /// The columns the run takes on its line.
    pub fn span(self) -> Range<usize> {
        self.col()..self.col().saturating_add(self.cols())
    }

    /// Which row of the run this line holds, 0 for its top row.
    pub fn row(self) -> usize {
        usize::from(self.fields()[1])
    }

    /// How many rows the run takes: its scale.
    pub fn rows(self) -> usize {
        usize::from(self.fields()[2].max(1))
    }

    /// The keys the run was written with.
    pub fn size(self) -> TextSize {
        let [
            _,
            _,
            scale,
            width,
            numerator,
            denominator,
            vertical,
            horizontal,
        ] = self.fields();
        TextSize {
            scale,
            width,
            numerator,
            denominator,
            vertical,
            horizontal,
        }
    }

    /// The same share, moved to column `col`.
    pub(crate) fn at(self, col: usize) -> Self {
        Self {
            col: u16::try_from(col).unwrap_or(u16::MAX),
            ..self
        }
    }
}

impl core::fmt::Debug for SizedRun {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SizedRun")
            .field("col", &self.col())
            .field("cols", &self.cols())
            .field("row", &self.row())
            .field("size", &self.size())
            .finish()
    }
}

/// A line's tables, allocated on its first styled, linked or clustered
/// cell, or its first sized run. Index `i` of a style or link names entry
/// `i - 1`; index 0 is the default style or no link.
#[derive(Clone, Debug, Default, PartialEq)]
struct Tables {
    styles: Vec<Style>,
    /// Each link's number in the link store, in 16 bits: the store holds
    /// fewer than 65,536 links ([`crate::links`]).
    ///
    /// References:
    /// - ADR-1009 D5: the line's link table keeps 16-bit numbers.
    links: Vec<u16>,
    /// The share of each sized run that crosses the line, ordered by
    /// column, the runs apart from each other.
    runs: RunTable,
    /// Each cluster as a length byte and its codepoints in UTF-8, with the
    /// counts of what the line's cells name.
    clusters: ClusterArena,
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
const ARENA_OFFSETS: usize = 1 << 21;

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
struct U24([u8; 3]);

impl U24 {
    const MAX: usize = (1 << 24) - 1;

    fn get(self) -> usize {
        let [a, b, c] = self.0;
        usize::from(a) | (usize::from(b) << 8) | (usize::from(c) << 16)
    }

    /// `n`, or the largest count when `n` is larger.
    fn saturating(n: usize) -> Self {
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
struct ClusterArena {
    bytes: Box<[u8]>,
    len: U24,
    /// The bytes of the clusters the line's cells name, length bytes
    /// included and placeholder clusters left out: what the line's
    /// cluster budget counts. It is at most the arena's length, since no
    /// two cells name one cluster, so it never reaches the largest count.
    live: U24,
    /// The cells that name a placeholder cluster, at most the line's
    /// `COLUMNS_MAX` cells, so it never reaches `u16::MAX`.
    placeholders: u16,
}

impl ClusterArena {
    /// An empty arena with room for `cap` bytes.
    fn with_capacity(cap: usize) -> Self {
        Self {
            bytes: vec![0; cap].into_boxed_slice(),
            ..Self::default()
        }
    }

    fn len(&self) -> usize {
        self.len.get()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn capacity(&self) -> usize {
        self.bytes.len()
    }

    /// The bytes the arena holds.
    fn as_slice(&self) -> &[u8] {
        self.bytes.get(..self.len()).unwrap_or_default()
    }

    /// Makes room for `more` bytes past the arena's length, to exactly
    /// that capacity when it grows, as `Vec::reserve_exact` does.
    fn reserve_exact(&mut self, more: usize) {
        if self.capacity().saturating_sub(self.len()) >= more {
            return;
        }
        self.regrow(self.len().saturating_add(more));
    }

    /// Gives back the room past the arena's length, as
    /// `Vec::shrink_to_fit` does.
    fn shrink_to_fit(&mut self) {
        if self.capacity() > self.len() {
            self.bytes = Box::from(self.as_slice());
        }
    }

    /// Makes room for `more` bytes as a line's other tables grow
    /// ([`reserve_within`]).
    fn reserve_within(&mut self, more: usize, limit: usize) {
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
    fn append(&mut self, s: &str) -> Option<u32> {
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

    fn live(&self) -> usize {
        self.live.get()
    }

    fn set_live(&mut self, live: usize) {
        self.live = U24::saturating(live);
    }

    fn placeholders(&self) -> usize {
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
struct RunTable(Option<Box<Vec<SizedRun>>>);

impl RunTable {
    fn as_slice(&self) -> &[SizedRun] {
        self.0.as_deref().map_or(&[], Vec::as_slice)
    }

    fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut SizedRun> {
        self.0.iter_mut().flat_map(|v| v.iter_mut())
    }

    fn retain(&mut self, keep: impl FnMut(&SizedRun) -> bool) {
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
    fn insert(&mut self, run: SizedRun, cells: usize) {
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

    fn shrink_to_fit(&mut self) {
        if let Some(runs) = self.0.as_mut() {
            runs.shrink_to_fit();
        }
    }

    /// The bytes the table holds on the heap, by capacity.
    fn heap_bytes(&self) -> usize {
        self.0.as_ref().map_or(0, |runs| {
            size_of::<Vec<SizedRun>>()
                .saturating_add(runs.capacity().saturating_mul(size_of::<SizedRun>()))
        })
    }
}

impl Tables {
    fn is_empty(&self) -> bool {
        self.styles.is_empty()
            && self.links.is_empty()
            && self.clusters.is_empty()
            && self.runs.is_empty()
    }

    /// The cluster at byte `offset` of the arena.
    fn cluster(&self, offset: u32) -> Option<&str> {
        cluster_at(self.clusters.as_slice(), offset)
    }

    /// The arena bytes the cluster at `offset` takes, its length byte
    /// included.
    fn cluster_size(&self, offset: u32) -> usize {
        usize::try_from(offset)
            .ok()
            .and_then(|at| self.clusters.as_slice().get(at))
            .map_or(0, |&len| usize::from(len).saturating_add(1))
    }

    /// The bytes of the line's clusters its budget counts.
    fn live(&self) -> usize {
        self.clusters.live()
    }

    /// The cells that name a placeholder cluster.
    fn placeholders(&self) -> usize {
        self.clusters.placeholders()
    }

    /// Counts the cluster `s`, just stored, as a cell now names it.
    fn count(&mut self, s: &str) {
        let arena = &mut self.clusters;
        if is_placeholder(s) {
            arena.placeholders = arena.placeholders.saturating_add(1);
        } else {
            arena.set_live(arena.live().saturating_add(s.len()).saturating_add(1));
        }
    }

    /// Gives back the cluster at `offset`, as the cell that named it goes.
    fn forget(&mut self, offset: u32) {
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

/// U+10EEEE, the kitty graphics protocol's Unicode placeholder (ADR-1100,
/// D13).
pub(crate) const PLACEHOLDER: char = '\u{10EEEE}';

/// Whether the cluster `s` is a placeholder cluster: one that starts with
/// [`PLACEHOLDER`], in a build with images. A build without images keeps
/// U+10EEEE as any private-use character, its marks within the budget.
pub(crate) fn is_placeholder(s: &str) -> bool {
    cfg!(feature = "images") && s.starts_with(PLACEHOLDER)
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

    /// The share of each sized run that crosses the line, ordered by
    /// column. A line with no run has an empty table.
    pub fn runs(&self) -> &[SizedRun] {
        self.tables.as_ref().map_or(&[], |t| t.runs.as_slice())
    }

    /// Whether the line holds a share of any sized run. Every run check on
    /// the print and edit paths asks this first, so a line without runs
    /// pays one test of its tables for them and nothing more.
    ///
    /// References:
    /// - ADR-1009 D9: a line with no run pays no time for runs.
    #[inline]
    pub(crate) fn has_runs(&self) -> bool {
        // A run marks the line wide, so a line that is not wide answers
        // from the line itself, without reading its tables.
        self.wide && self.tables.as_ref().is_some_and(|t| !t.runs.is_empty())
    }

    /// The share of the sized run that holds column `col`, if a run does.
    pub fn run_at(&self, col: usize) -> Option<SizedRun> {
        if !self.has_runs() || !self.cells.get(col).is_some_and(|c| c.in_run()) {
            return None;
        }
        self.runs()
            .iter()
            .copied()
            .find(|r| r.span().contains(&col))
    }

    /// Whether the line holds a row of a sized run taller than one row.
    /// Such a line keeps its columns at a width change.
    ///
    /// References:
    /// - ADR-1308 D6: a line that holds any row of a run taller than one
    ///   row does not rewrap.
    pub(crate) fn holds_tall_run(&self) -> bool {
        self.runs().iter().any(|r| r.rows() > 1)
    }

    /// Writes the share `run` of a sized run: `glyph`, the run's text, in
    /// its first cell, and its other cells empty, each in `style` and
    /// `link` and marked as part of the run. A run already in those cells
    /// goes from this line first. Returns false, changing nothing, when
    /// the run does not fit inside the line.
    pub(crate) fn put_run(
        &mut self,
        run: SizedRun,
        glyph: Glyph<'_>,
        style: &Style,
        link: Option<LinkId>,
    ) -> bool {
        let span = run.span();
        if run.cols() == 0 || span.end > self.cells.len() {
            return false;
        }
        self.drop_runs(span.clone());
        for col in span.clone() {
            let g = if col == span.start {
                glyph
            } else {
                Glyph::Empty
            };
            self.write(col, g, Width::Narrow, style, link);
        }
        self.mark_run(run);
        true
    }

    /// Replaces the text of the share `run` of a sized run with `glyph`,
    /// keeping the run: a codepoint that joins its text.
    pub(crate) fn set_run_text(&mut self, run: SizedRun, glyph: Glyph<'_>) {
        if !self.runs().contains(&run) {
            return;
        }
        let cell = self.cell(run.col());
        let (style, link) = (self.style(cell), self.link(cell));
        self.drop_runs(run.span());
        self.write(run.col(), glyph, Width::Narrow, &style, link);
        self.mark_run(run);
    }

    /// Marks the cells of `run` as part of it and adds it to the run
    /// table, for cells that already hold the run's text: a rewrap or a
    /// scroll inside the margins moves a run's cells, then marks them.
    pub(crate) fn mark_run(&mut self, run: SizedRun) {
        let span = run.span();
        if run.cols() == 0 || span.end > self.cells.len() {
            return;
        }
        for cell in self.cells.get_mut(span).unwrap_or_default() {
            *cell = cell.with_run(true);
        }
        self.wide = true;
        let cells = self.cells.len();
        self.tables.get_or_insert_default().runs.insert(run, cells);
    }

    /// Takes every run that holds a column of `cols` off the line: its
    /// cells keep their style and link, lose their text and their mark,
    /// and become narrow cells. A run whose cells an edit would separate,
    /// or partly overwrite, goes whole, as a wide character does. On this
    /// line that is this row of the run. The screen takes the run's other
    /// rows off their lines.
    #[inline]
    fn drop_runs(&mut self, cols: Range<usize>) {
        if self.has_runs() {
            self.drop_runs_where(|r| r.col() < cols.end && r.span().end > cols.start);
        }
    }

    /// Takes each run that `gone` holds for off the line, as
    /// [`drop_runs`](Self::drop_runs) does.
    #[cold]
    #[inline(never)]
    fn drop_runs_where(&mut self, gone: impl Fn(SizedRun) -> bool) {
        let Some(t) = self.tables.as_mut() else {
            return;
        };
        if !t.runs.as_slice().iter().any(|&r| gone(r)) {
            return;
        }
        let mut dropped = Vec::new();
        t.runs.retain(|&r| {
            let keep = !gone(r);
            if !keep {
                dropped.push(r.span());
            }
            keep
        });
        for span in dropped {
            for col in span {
                let Some(&old) = self.cells.get(col) else {
                    continue;
                };
                self.release(old);
                if let Some(cell) = self.cells.get_mut(col) {
                    *cell = Cell::new(Content::Empty, Width::Narrow, old.style(), old.link());
                }
            }
        }
    }

    /// Moves the runs of `at..end` as `insert_cells` (`insert` true) or
    /// `delete_cells` moves their cells by `n`. A run the move would cut,
    /// one across `at`, across `end`, across the deleted cells, or pushed
    /// partly past `end`, goes from the line first.
    fn shift_runs(&mut self, at: usize, n: usize, end: usize, insert: bool) {
        if !self.has_runs() {
            return;
        }
        let cut = |r: SizedRun| {
            let span = r.span();
            if span.end <= at || span.start >= end {
                return false;
            }
            if span.start < at || span.end > end {
                return true;
            }
            if insert {
                span.end.saturating_add(n) > end
            } else {
                span.start < at.saturating_add(n)
            }
        };
        self.drop_runs_where(cut);
        if let Some(t) = self.tables.as_mut() {
            for run in t.runs.iter_mut() {
                if run.col() >= at && run.col() < end {
                    let col = if insert {
                        run.col().saturating_add(n)
                    } else {
                        run.col().saturating_sub(n)
                    };
                    *run = run.at(col);
                }
            }
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
    fn placeholders(&self) -> usize {
        self.tables.as_ref().map_or(0, |t| t.placeholders())
    }

    /// Makes the line `cols` cells long, at most [`COLUMNS_MAX`]: cells
    /// past the new end are dropped, a wide character the new end cuts
    /// becomes an empty cell, and new cells are empty in the line's fill.
    /// Shrinking holds the clusters the cells left name to the budget of
    /// the shorter line, as writing it at that width would
    /// ([`hold_to_budget`](Self::hold_to_budget)), and compacts the tables,
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

    /// The arena's limit once the cluster `s` is stored: a placeholder
    /// cluster raises it by what the cell that names it adds.
    fn arena_with(&self, s: &str) -> usize {
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
    fn cluster_budget(&self) -> usize {
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
    fn limits(&self) -> (usize, usize) {
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
    fn make_room(&mut self, adds: Option<&str>) {
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

    fn shrink_tables(&mut self) {
        if let Some(t) = self.tables.as_mut() {
            t.styles.shrink_to_fit();
            t.links.shrink_to_fit();
            t.clusters.shrink_to_fit();
            t.runs.shrink_to_fit();
        }
    }

    /// Keeps the table entries a cell names, in the order cells first name
    /// them, and renumbers the cells.
    fn compact(&mut self) {
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
    fn release(&mut self, old: Cell) {
        if let (Content::Cluster(ClusterOffset(o)), Some(t)) = (old.content(), self.tables.as_mut())
        {
            t.forget(o);
        }
    }

    /// Gives back the cluster bytes the cells of `range` name, as they go.
    fn release_range(&mut self, range: Range<usize>) {
        let Some(t) = self.tables.as_mut() else {
            return;
        };
        for cell in self.cells.get(range).unwrap_or_default() {
            if let Content::Cluster(ClusterOffset(o)) = cell.content() {
                t.forget(o);
            }
        }
    }

    fn intern_style(&mut self, style: &Style) -> StyleIndex {
        if *style == Style::DEFAULT {
            return StyleIndex(0);
        }
        let (limit, _) = self.limits();
        let styles = &mut self.tables.get_or_insert_default().styles;
        StyleIndex(intern(styles, style, limit))
    }

    fn intern_link(&mut self, link: Option<LinkId>) -> LinkIndex {
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
    fn kept_cluster<'s>(&self, s: &'s str) -> Option<&'s str> {
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
    fn store_cluster(&mut self, kept: &str) -> Option<Content> {
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
    fn hold_to_budget(&mut self) {
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
    fn hold_to(&mut self, budget: usize) {
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
                let Some(content) = fit(s, room).and_then(|s| {
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

/// `cols` as a line records the width it was written at.
fn width_of(cols: usize) -> u16 {
    u16::try_from(cols.min(COLUMNS_MAX)).unwrap_or(u16::MAX)
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

/// The cluster at byte `offset` of `arena`.
fn cluster_at(arena: &[u8], offset: u32) -> Option<&str> {
    let at = usize::try_from(offset).ok()?;
    let len = usize::from(*arena.get(at)?);
    let start = at.checked_add(1)?;
    let bytes = arena.get(start..start.checked_add(len)?)?;
    core::str::from_utf8(bytes).ok()
}

/// What a line's budget keeps of the cluster `s` with `room` bytes left:
/// the longest run of its first codepoints, at most [`CLUSTER_MAX`], that
/// fits with its length byte, or `None` when that is less than two
/// codepoints, as the line then stores only the first.
fn fit(s: &str, room: usize) -> Option<&str> {
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

/// Appends the cluster `s` to `arena` as its length byte and its bytes,
/// and returns the content that names it, or `None` when the arena
/// refuses it ([`ClusterArena::append`]).
fn append_cluster(arena: &mut ClusterArena, s: &str) -> Option<Content> {
    arena
        .append(s)
        .map(|offset| Content::Cluster(ClusterOffset(offset)))
}

/// The indices a rewrap has copied from one source line's tables into the
/// line it builds: entry `i` holds the new index of old index `i + 1`, 0
/// until copied. A rewrap clears it for each pair of lines.
#[derive(Debug, Default)]
pub(crate) struct IndexMap {
    styles: Vec<u16>,
    links: Vec<u16>,
}

impl IndexMap {
    pub(crate) fn clear(&mut self) {
        self.styles.clear();
        self.links.clear();
    }

    /// Makes room for every index of `tables`.
    fn fit(&mut self, tables: &Tables) {
        if self.styles.len() < tables.styles.len() {
            self.styles.resize(tables.styles.len(), 0);
        }
        if self.links.len() < tables.links.len() {
            self.links.resize(tables.links.len(), 0);
        }
    }
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

/// Makes room for `more` entries in `table`. Its capacity doubles, as a
/// vector's does, but stops at `limit` while the entries fit inside it, so
/// a table that compacts at `limit` never holds room past it. A table
/// that passes its limit, as a row a rewrap builds may, doubles past it.
fn reserve_within<T>(table: &mut Vec<T>, more: usize, limit: usize) {
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

/// The new 1-based index of old index `i`, copying its entry into `new`
/// the first time a cell names it, `new` growing within `limit`
/// ([`reserve_within`]). `map` holds the new index of each old entry, 0
/// until it is copied.
fn remap<T: Copy>(map: &mut [u16], i: u16, old: &[T], new: &mut Vec<T>, limit: usize) -> u16 {
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

    // ADR-1001, "The line": extenders past 32 are dropped.
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
                assert_eq!(line.tables.as_ref().map_or(0, |t| t.live()), named);
                assert!(
                    line.tables.as_ref().map_or(0, |t| t.clusters.len())
                        <= compaction_limit(cols * CLUSTER_BYTES_PER_CELL)
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

    // Cells past a stored line's end read as the blank cell in the default
    // style, so an SGR under BCE does not recolor them.
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
    fn trim_keeps_cells_erased_in_a_color() {
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

    // ADR-1003, "Logical lines": a line's fill is the run of empty cells
    // of one style that ends it; empty cells before it stay as text, and
    // cells past its end read in its style.
    #[test]
    fn trim_keeps_the_fill_as_the_style_past_the_end() {
        let bce = Style {
            bg: Color::Indexed(4),
            ..Style::DEFAULT
        };
        let mut line = Line::new(LineId::new(1), 8);
        line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
        line.erase(3..8, &bce);
        line.trim();
        assert_eq!(line.len(), 3);
        assert_eq!(line.fill(), bce);
        assert_eq!(line.style(line.cell(6)), bce);
        assert!(line.cell(1).is_blank());
        // The fill survives compaction, and paints a widening.
        line.resize(10);
        assert_eq!(line.style(line.cell(9)), bce);
        assert_eq!(line.cell(9).content(), Content::Empty);
    }

    // A wrapped line's cells are all text: trimming keeps them.
    #[test]
    fn trim_keeps_every_cell_of_a_wrapped_line() {
        let mut line = Line::new(LineId::new(1), 6);
        line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
        line.flags_mut().insert(LineFlags::WRAPPED);
        line.trim();
        assert_eq!(line.len(), 6);
        line.unwrap();
        line.trim();
        assert_eq!(line.len(), 1);
    }

    // A pad belongs only in the last column of a wrapped line.
    #[test]
    fn a_pad_that_leaves_the_end_of_a_wrapped_line_is_an_empty_cell() {
        let pad = |line: &Line, col| line.cell(col).width() == Width::Pad;
        let mut line = Line::new(LineId::new(1), 4);
        line.write(3, Glyph::Empty, Width::Pad, &red(), None);
        line.flags_mut().insert(LineFlags::WRAPPED);
        let mut moved = line.clone();
        moved.delete_cells(1, 1, 4, &Style::DEFAULT);
        assert!(!pad(&moved, 2) && moved.cell(2).content() == Content::Empty);
        assert_eq!(moved.style(moved.cell(2)), red());
        line.unwrap();
        assert!(!pad(&line, 3));
        assert_eq!(line.style(line.cell(3)), red());
    }

    // A shrink cuts a wide character at the new end whole, and ends the
    // wrap of a line whose text it cut.
    #[test]
    fn shrinking_cuts_a_wide_character_and_ends_the_wrap() {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(
            1,
            Glyph::Cluster(FAMILY),
            Width::Wide,
            &Style::DEFAULT,
            None,
        );
        line.write(2, Glyph::Empty, Width::WideTail, &Style::DEFAULT, None);
        line.flags_mut().insert(LineFlags::WRAPPED);
        line.resize(2);
        assert_eq!(line.cell(1).content(), Content::Empty);
        assert_eq!(line.cell(1).width(), Width::Narrow);
        assert_eq!(line.cluster_bytes(), 0);
        assert!(!line.flags().contains(LineFlags::WRAPPED));
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
        assert_eq!(tables(&line).live(), 8);
        line.erase(2..100, &red());
        assert!(line.cell(1).is_blank());
        assert_eq!(line.style(line.cell(3)), red());
        assert_eq!(line.glyph(line.cell(3)), Glyph::Empty);
        assert_eq!(tables(&line).live(), 0);
        assert_eq!(line.len(), 4);
    }

    #[test]
    fn rewriting_one_cell_in_new_styles_keeps_the_table_bounded() {
        let mut line = Line::new(LineId::new(1), 4);
        for n in 0..10_000 {
            line.write(0, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
            let t = tables(&line).styles.len();
            assert!(t <= table_limit(4), "the style table grew to {t}");
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
            assert!(tables(&line).styles.len() <= table_limit(cols));
        }
    }

    /// A cluster of exactly `bytes` UTF-8 bytes, 2 to 128: a base of 1 to
    /// 4 bytes and marks of 4 bytes, or a mark of 1 to 3 bytes; a single
    /// letter for 1 byte.
    fn sized(bytes: usize) -> String {
        match bytes {
            1 => "x".into(),
            2 => "ea".into(),
            3 => "e\u{301}".into(),
            4 => "e\u{20D0}".into(),
            _ => {
                let rest = bytes.saturating_sub(1);
                let base = match rest % 4 {
                    0 => 'e',
                    1 => '\u{E9}',
                    2 => '\u{4E2D}',
                    _ => '\u{1F600}',
                };
                std::iter::once(base)
                    .chain(std::iter::repeat_n('\u{E0100}', rest / 4))
                    .collect()
            }
        }
    }

    // A table's capacity stops at the limit it compacts at, so a row
    // `write` fills holds no room past what `Line::bytes_max` prices. At
    // 17 columns no limit is a power of two, so a table that only doubled
    // would pass it. Each write takes a style and a link no other write
    // takes; sweeps of plain text and of small clusters, with each size of
    // cluster in the first cell, bring each table to its limit and no
    // further. `tests/memory.rs` drives rows to the whole bound.
    // The arena compacts only when the cluster a write adds would take it
    // past its limit: on a row of 10 cells, whose arena holds 90 bytes,
    // 30 overwrites of a 2-byte cluster fill it with dead clusters, and
    // the 31st compacts it.
    #[test]
    fn the_arena_compacts_on_the_bytes_a_write_adds() {
        let mut line = Line::new(LineId::new(1), 10);
        let cluster = sized(2);
        for _ in 0..30 {
            line.write(
                0,
                Glyph::Cluster(&cluster),
                Width::Narrow,
                &Style::DEFAULT,
                None,
            );
        }
        assert_eq!(tables(&line).clusters.len(), 90);
        line.write(
            0,
            Glyph::Cluster(&cluster),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(tables(&line).clusters.len(), 3);
        assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&cluster));
    }

    // A run's entry keeps every key at the most each one takes, in 6 bytes.
    #[test]
    fn a_sized_run_keeps_its_keys_in_six_bytes() {
        let size = TextSize {
            scale: 7,
            width: 7,
            numerator: 15,
            denominator: 15,
            vertical: 2,
            horizontal: 2,
        };
        let run = SizedRun::new(8191, 49, 6, size);
        assert_eq!(size_of::<SizedRun>(), 6);
        assert_eq!(
            (run.col(), run.cols(), run.row(), run.rows()),
            (8191, 49, 6, 7)
        );
        assert_eq!(run.size(), size);
        let small = SizedRun::new(
            0,
            1,
            0,
            TextSize {
                width: 1,
                ..TextSize::NORMAL
            },
        );
        assert_eq!(
            (
                small.cols(),
                small.row(),
                small.size().width,
                small.size().scale
            ),
            (1, 0, 1, 1)
        );
        assert_eq!(small.at(3).span(), 3..4);
    }

    /// [`Line::bytes_max`] less the run table at its most, for rows that
    /// hold no sized run.
    fn bytes_max_without_runs(cols: usize) -> usize {
        Line::bytes_max(cols)
            .saturating_sub(size_of::<Vec<SizedRun>>())
            .saturating_sub(cols.saturating_mul(size_of::<SizedRun>()))
    }

    #[test]
    fn the_tables_grow_no_further_than_they_compact_at() {
        let cols = 17;
        let limit = table_limit(cols);
        let (mut most, mut styles, mut links, mut arena) = (0, 0, 0, 0);
        let mut n = 0u32;
        for first in 2..=128 {
            for small in [1, 2, 5, 11] {
                let mut line = Line::new(LineId::new(1), cols);
                for col in (0..6 * cols).map(|i| i % cols) {
                    n += 1;
                    let cluster = sized(if col == 0 { first } else { small });
                    let glyph = match small {
                        1 if col > 0 => Glyph::Char('x'),
                        _ => Glyph::Cluster(&cluster),
                    };
                    line.write(col, glyph, Width::Narrow, &rgb(n), Some(LinkId::new(n)));
                    let t = tables(&line);
                    styles = styles.max(t.styles.capacity());
                    links = links.max(t.links.capacity());
                    arena = arena.max(t.clusters.capacity());
                    most = most.max(size_of::<Line>() + line.heap_bytes());
                }
            }
        }
        assert_eq!(styles, limit);
        assert_eq!(links, limit);
        assert_eq!(arena, compaction_limit(cols * CLUSTER_BYTES_PER_CELL));
        assert!(most <= bytes_max_without_runs(cols), "{most}");
    }

    /// A kitty placeholder, U+10EEEE, and `marks` 4-byte marks.
    fn placeholder(marks: usize) -> String {
        std::iter::once(PLACEHOLDER)
            .chain(std::iter::repeat_n('\u{E0100}', marks))
            .collect()
    }

    // ADR-1001, "Placeholder cells": in a build with images, a placeholder
    // cluster keeps its first three marks wherever it starts in its line,
    // after other clusters have spent the budget, and drops the marks past
    // them; the budget still cuts every other cluster.
    #[cfg(feature = "images")]
    #[test]
    fn a_placeholder_keeps_three_marks_whatever_the_budget() {
        let mut line = Line::new(LineId::new(1), 4);
        let heavy = accented(31);
        line.write(
            0,
            Glyph::Cluster(&heavy),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(line.cluster_bytes(), 24, "the budget of 4 cells is spent");
        let five = placeholder(5);
        line.write(
            1,
            Glyph::Cluster(&five),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        line.write(
            2,
            Glyph::Cluster(&heavy),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        let one = placeholder(1);
        line.write(
            3,
            Glyph::Cluster(&one),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(
            line.glyph(line.cell(1)),
            Glyph::Cluster(&placeholder(3)),
            "the placeholder keeps three of its five marks"
        );
        assert_eq!(
            line.glyph(line.cell(2)),
            Glyph::Char('e'),
            "the budget cuts"
        );
        assert_eq!(line.glyph(line.cell(3)), Glyph::Cluster(&one));
        assert_eq!(line.cluster_bytes(), 24 + 17 + 9);
        assert_eq!(tables(&line).live(), 24);
        assert_eq!(tables(&line).placeholders(), 2);
        line.erase(1..2, &Style::DEFAULT);
        assert_eq!(tables(&line).placeholders(), 1);
        assert_eq!(line.cluster_bytes(), 24 + 9);
    }

    // Holding a line to its budget rebuilds its arena and counts its
    // placeholder cells again, from the cells it has now: a line cut short
    // names fewer than it did.
    #[cfg(feature = "images")]
    #[test]
    fn holding_a_line_to_its_budget_counts_its_placeholders_again() {
        let mut line = Line::new(LineId::new(1), 8);
        let heavy = accented(31);
        let p = placeholder(3);
        for col in 0..8 {
            let glyph = if col % 2 == 0 { &heavy } else { &p };
            line.write(
                col,
                Glyph::Cluster(glyph),
                Width::Narrow,
                &Style::DEFAULT,
                None,
            );
        }
        assert_eq!(tables(&line).placeholders(), 4);
        line.cells.truncate(3);
        line.hold_to_budget();
        assert_eq!(
            tables(&line).placeholders(),
            1,
            "one placeholder cell is left"
        );
        assert_eq!(line.cluster_bytes(), 17 + tables(&line).live());
    }

    // In a build without images, U+10EEEE is a private-use character like
    // any other: a cluster that starts with it keeps only what the line's
    // budget leaves, and the line counts no placeholder.
    #[cfg(not(feature = "images"))]
    #[test]
    fn without_images_a_placeholder_cluster_keeps_only_what_the_budget_allows() {
        let mut line = Line::new(LineId::new(1), 4);
        let heavy = accented(31);
        line.write(
            0,
            Glyph::Cluster(&heavy),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        let five = placeholder(5);
        line.write(
            1,
            Glyph::Cluster(&five),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(
            line.glyph(line.cell(1)),
            Glyph::Char(PLACEHOLDER),
            "the budget is spent"
        );
        // five cells' budget, 30 bytes, holds the 25 the cluster takes
        let mut line = Line::new(LineId::new(1), 5);
        line.write(
            0,
            Glyph::Cluster(&five),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        assert_eq!(
            line.glyph(line.cell(0)),
            Glyph::Cluster(&five),
            "all five marks fit"
        );
        assert_eq!(tables(&line).placeholders(), 0);
        assert_eq!(tables(&line).live(), five.len() + 1);
        // ADR-1009 D6's figure on a 64-bit target.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(Line::bytes_max(170), 8_142);
    }

    // The arena's limit takes 17 bytes for each cell that names a
    // placeholder, so a row of placeholders, rewritten in new styles and
    // links, compacts within it and its tables reach `Line::bytes_max`.
    #[cfg(feature = "images")]
    #[test]
    fn a_row_of_placeholders_holds_to_its_raised_limit() {
        let cols = 17;
        let (mut arena, mut most, mut n) = (0, 0, 0u32);
        for first in 1..=3 {
            let mut line = Line::new(LineId::new(1), cols);
            for col in (0..6 * cols).map(|i| i % cols) {
                n += 1;
                let c = placeholder(if col == 0 { first } else { 3 });
                line.write(
                    col,
                    Glyph::Cluster(&c),
                    Width::Narrow,
                    &rgb(n),
                    Some(LinkId::new(n)),
                );
                let t = tables(&line);
                assert!(
                    t.clusters.len()
                        <= compaction_limit(
                            cols * (CLUSTER_BYTES_PER_CELL + PLACEHOLDER_BYTES_MAX)
                        )
                );
                arena = arena.max(t.clusters.capacity());
                most = most.max(size_of::<Line>() + line.heap_bytes());
            }
            assert_eq!(tables(&line).placeholders(), 17);
        }
        assert_eq!(
            arena,
            compaction_limit(cols * (CLUSTER_BYTES_PER_CELL + PLACEHOLDER_BYTES_MAX))
        );
        assert_eq!(most, bytes_max_without_runs(cols));
    }

    // A row a rewrap built names past its arena's limit. Compacting it
    // sizes the arena to what its cells name, without doubling past it.
    #[test]
    fn compacting_a_row_past_its_limit_sizes_its_arena_to_what_it_names() {
        let cols = 10;
        let mut src = Line::new(LineId::new(1), 4 * cols);
        let heavy = accented(31);
        src.write(
            0,
            Glyph::Cluster(&heavy),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        let mut row = Line::with_capacity(LineId::new(2), LineFlags::empty(), cols);
        let mut map = IndexMap::default();
        for _ in 0..cols {
            row.push_from(&src, src.cell(0), Width::Narrow, false, &mut map);
        }
        row.shrink();
        let named = row.cluster_bytes();
        assert!(named > compaction_limit(cols * CLUSTER_BYTES_PER_CELL));
        // A write compacts such a row once its style or link table fills.
        row.compact();
        let t = tables(&row);
        assert_eq!(t.clusters.len(), named);
        assert_eq!(t.clusters.capacity(), named);
    }

    /// A cluster of the most bytes a line keeps: 32 codepoints of four
    /// bytes each, 129 bytes with its length byte.
    fn heaviest() -> String {
        std::iter::repeat_n('\u{E0100}', CLUSTER_MAX).collect()
    }

    // The widest row a rewrap can build has every cell name a cluster of
    // the most bytes, as a rewrap copies each cluster whole. Its arena and
    // its count of named bytes pass 16 bits by far, and the last cluster
    // still starts at an offset a cell can name. Held to its screen's
    // limit, twice its budget, the row still counts more than 16 bits.
    #[test]
    fn a_row_full_of_the_largest_clusters_at_the_widest_width_counts_them_all() {
        let heavy = heaviest();
        assert_eq!(heavy.len() + 1, CLUSTER_BYTES_MAX);
        let mut src = Line::new(LineId::new(1), COLUMNS_MAX);
        src.write(
            0,
            Glyph::Cluster(&heavy),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
        let mut row = Line::with_capacity(LineId::new(2), LineFlags::empty(), COLUMNS_MAX);
        let mut map = IndexMap::default();
        for _ in 0..COLUMNS_MAX {
            row.push_from(&src, src.cell(0), Width::Narrow, false, &mut map);
        }
        let named = COLUMNS_MAX * CLUSTER_BYTES_MAX;
        assert_eq!(named, 1_056_768);
        assert_eq!(row.len(), COLUMNS_MAX);
        assert_eq!(tables(&row).live(), named);
        assert_eq!(tables(&row).clusters.len(), named);
        assert_eq!(row.cluster_bytes(), named);
        assert!(
            row.cells()
                .iter()
                .all(|&c| row.glyph(c) == Glyph::Cluster(&heavy))
        );
        let last = row.cell(COLUMNS_MAX - 1).content();
        assert_eq!(
            last,
            Content::Cluster(ClusterOffset(
                u32::try_from(named - CLUSTER_BYTES_MAX).unwrap()
            ))
        );

        row.paint(COLUMNS_MAX);
        // One and a half times the budget holds 571 whole clusters, and the
        // 69 bytes left keep the first 17 codepoints of the next.
        let limit = compaction_limit(COLUMNS_MAX * CLUSTER_BYTES_PER_CELL);
        let kept = limit / CLUSTER_BYTES_MAX;
        assert_eq!((limit, kept), (73_728, 571));
        assert_eq!(tables(&row).live(), limit);
        assert!(tables(&row).live() > usize::from(u16::MAX));
        assert_eq!(row.glyph(row.cell(kept - 1)), Glyph::Cluster(&heavy));
        let part: String = std::iter::repeat_n('\u{E0100}', 17).collect();
        assert_eq!(row.glyph(row.cell(kept)), Glyph::Cluster(&part));
        assert_eq!(row.glyph(row.cell(kept + 1)), Glyph::Char('\u{E0100}'));
    }

    // In a build with images, every cell of the widest line may name a
    // placeholder cluster, and the line counts each of them.
    #[cfg(feature = "images")]
    #[test]
    fn a_row_of_placeholders_at_the_widest_width_counts_every_cell() {
        let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
        let p = placeholder(PLACEHOLDER_MARKS);
        for col in 0..COLUMNS_MAX {
            assert!(line.write(
                col,
                Glyph::Cluster(&p),
                Width::Narrow,
                &Style::DEFAULT,
                None
            ));
        }
        assert_eq!(tables(&line).placeholders(), COLUMNS_MAX);
        assert_eq!(tables(&line).live(), 0);
        assert_eq!(line.cluster_bytes(), COLUMNS_MAX * PLACEHOLDER_BYTES_MAX);
        assert!(
            line.cells()
                .iter()
                .all(|&c| line.glyph(c) == Glyph::Cluster(&p))
        );
    }

    // A cell names its cluster by a 21-bit offset. The arena takes
    // clusters up to the last offset a cell can name, and refuses the
    // next one, leaving itself as it was.
    #[test]
    fn the_arena_refuses_a_cluster_past_the_offsets_a_cell_names() {
        let s = "a".repeat(255);
        let mut arena = ClusterArena::default();
        let mut last = None;
        while let Some(offset) = arena.append(&s) {
            last = Some(offset);
        }
        let last = usize::try_from(last.unwrap()).unwrap();
        assert_eq!(last, ARENA_OFFSETS - 256);
        assert_eq!(arena.len(), ARENA_OFFSETS);
        assert_eq!(arena.append("ab"), None);
        assert_eq!(arena.len(), ARENA_OFFSETS);
        let offset = u32::try_from(last).unwrap();
        assert_eq!(cluster_at(arena.as_slice(), offset), Some(s.as_str()));
        // A cluster longer than its length byte can say is refused too.
        assert_eq!(ClusterArena::default().append(&"a".repeat(256)), None);
    }

    // The arena's length and its count of named bytes are kept in 24 bits.
    // The most the arena reaches fits; a larger count is stored as the
    // largest, and the count of placeholder cells stops at the largest
    // 16-bit value, as neither may wrap to a small number.
    #[test]
    fn the_arena_counts_hold_their_largest_values() {
        for n in [0, 1, 0xFFFF, 0x1_0000, ARENA_OFFSETS + 255, U24::MAX] {
            assert_eq!(U24::saturating(n).get(), n);
        }
        assert_eq!(U24::saturating(U24::MAX + 1).get(), U24::MAX);
        assert_eq!(U24::saturating(usize::MAX).get(), U24::MAX);

        let mut t = Tables::default();
        t.clusters.set_live(U24::MAX);
        t.count("ab");
        assert_eq!(t.live(), U24::MAX);
        if cfg!(feature = "images") {
            t.clusters.placeholders = u16::MAX;
            t.count(&placeholder(PLACEHOLDER_MARKS));
            assert_eq!(t.placeholders(), usize::from(u16::MAX));
        }
    }

    // The arena reaches the capacities a vector of bytes reaches through
    // the same calls: exact reserves, appends that grow by doubling,
    // shrinking, and clones.
    #[test]
    fn the_arena_grows_and_shrinks_as_a_vector_of_bytes() {
        let mut arena = ClusterArena::default();
        let mut vec: Vec<u8> = Vec::new();
        let check = |arena: &ClusterArena, vec: &Vec<u8>| {
            assert_eq!(arena.as_slice(), vec.as_slice());
            assert_eq!(arena.capacity(), vec.capacity());
            let copy = arena.clone();
            assert_eq!(copy.capacity(), vec.clone().capacity());
            assert_eq!(&copy, arena);
        };
        for (i, n) in [1usize, 2, 7, 30, 3, 129, 0, 5, 60, 255, 2, 40]
            .into_iter()
            .enumerate()
        {
            let s = "x".repeat(n);
            match i % 4 {
                0 => {
                    arena.reserve_exact(n);
                    vec.reserve_exact(n);
                }
                1 => {
                    arena.reserve_within(n.saturating_add(1), 64);
                    reserve_within(&mut vec, n.saturating_add(1), 64);
                }
                2 => {
                    arena.shrink_to_fit();
                    vec.shrink_to_fit();
                }
                _ => {}
            }
            check(&arena, &vec);
            let offset = arena.append(&s).unwrap();
            assert_eq!(usize::try_from(offset).unwrap(), vec.len());
            vec.push(u8::try_from(n).unwrap());
            vec.extend_from_slice(s.as_bytes());
            check(&arena, &vec);
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
        assert_eq!(line.heap_bytes(), 8 + size_of::<Tables>() + 14 + 2 + 6);
        assert_eq!(line.style(line.cell(0)), rgb(0));
        assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&accented(2)));
    }

    #[test]
    fn flags_and_size_are_independent() {
        let mut f = LineFlags::empty();
        f.insert(LineFlags::WRAPPED);
        f.insert(LineFlags::PROMPT);
        f.insert(LineFlags::FIXED);
        f.set_size(LineSize::DoubleHeightBottom);
        assert!(f.contains(LineFlags::FIXED));
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

    // A double-size line holds half the screen's columns, rounded down,
    // and at least one; a single-size line holds them all (VT100 User
    // Guide, DECDWL: "The use of double-width characters reduces the
    // number of characters per line by half").
    #[test]
    fn a_double_size_line_holds_half_the_columns() {
        for size in [
            LineSize::DoubleWidth,
            LineSize::DoubleHeightTop,
            LineSize::DoubleHeightBottom,
        ] {
            assert!(size.is_double());
            assert_eq!(size.columns(80), 40);
            assert_eq!(size.columns(81), 40);
            assert_eq!(size.columns(1), 1);
        }
        assert!(!LineSize::Single.is_double());
        assert_eq!(LineSize::Single.columns(81), 81);
    }
}
