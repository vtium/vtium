//! The cell: 8 bytes, with what it shows and indices into its line's
//! tables.
//!
//! A cell holds its content and width in 32 bits, the index of its style
//! in its line's style table in 16, and the index of its hyperlink in its
//! line's link table in 16. A grapheme cluster of more than one codepoint
//! lives in the line's cluster arena, and the cell holds its offset. The
//! change age is not in the cell: damage is kept by screen position
//! (`Damage`), so the scrollback carries none.

/// The bits of `Cell::content`: bits 0-20 a codepoint or a cluster offset,
/// bit 21 set for a cluster, bits 22-23 the width, bit 24 set for a cell of
/// a sized run, bits 25-31 zero.
const VALUE_MASK: u32 = (1 << 21) - 1;
const CLUSTER: u32 = 1 << 21;
const WIDTH_SHIFT: u32 = 22;
const WIDTH_MASK: u32 = 0b11 << WIDTH_SHIFT;
/// The cell is one of the cells of a sized run, the block of cells that
/// one OSC 66 sequence writes. The cell's line holds the run's place and
/// size in its run table ([`crate::Line::runs`]).
///
/// References:
/// - ADR-1308 D3: one of the content word's spare bits marks each cell of
///   a sized run.
const RUN: u32 = 1 << 24;

/// How many columns a cell's content takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Width {
    /// One column. A blank cell is narrow.
    #[default]
    Narrow,
    /// The first of two columns.
    Wide,
    /// The second column of a wide cell, which draws nothing of its own.
    WideTail,
    /// The last column of a wrapped row, left empty where a wide character
    /// did not fit and moved whole to the next row (ADR-1003). It draws as
    /// an empty cell, and a rewrap drops it.
    Pad,
}

impl Width {
    /// The columns this cell advances the cursor by.
    pub const fn columns(self) -> u8 {
        match self {
            Self::Narrow | Self::Pad => 1,
            Self::Wide => 2,
            Self::WideTail => 0,
        }
    }

    const fn bits(self) -> u32 {
        match self {
            Self::Narrow => 0,
            Self::Wide => 1,
            Self::WideTail => 2,
            Self::Pad => 3,
        }
    }
}

/// The index of a style in a line's table; 0 is `Style::DEFAULT`, which
/// the table never stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct StyleIndex(pub(crate) u16);

/// The index of a hyperlink in a line's table; 0 is no link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkIndex(pub(crate) u16);

/// The byte offset of a cluster in its line's arena, below 2^21.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterOffset(pub(crate) u32);

/// What a cell shows, as the cell stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// Nothing written, or erased.
    Empty,
    /// One codepoint.
    Char(char),
    /// A grapheme cluster of two or more codepoints, in the line's arena.
    Cluster(ClusterOffset),
}

/// One cell of the grid.
///
/// The all-zero cell is [`Cell::BLANK`]: empty, narrow, in the default
/// style, with no link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    content: u32,
    style: StyleIndex,
    link: LinkIndex,
}

const _: () = assert!(size_of::<Cell>() == 8);

impl Cell {
    /// An empty, narrow cell in the default style with no link.
    pub const BLANK: Self = Self {
        content: 0,
        style: StyleIndex(0),
        link: LinkIndex(0),
    };

    pub(crate) fn new(content: Content, width: Width, style: StyleIndex, link: LinkIndex) -> Self {
        let value = match content {
            Content::Empty => 0,
            Content::Char(c) => u32::from(c),
            Content::Cluster(ClusterOffset(i)) => CLUSTER | (i & VALUE_MASK),
        };
        Self {
            content: value | (width.bits() << WIDTH_SHIFT),
            style,
            link,
        }
    }

    pub fn content(self) -> Content {
        let value = self.content & VALUE_MASK;
        if self.content & CLUSTER != 0 {
            Content::Cluster(ClusterOffset(value))
        } else {
            // Bits 0-20 hold a codepoint written by `new`, or 0.
            char::from_u32(value)
                .filter(|&c| c != '\0')
                .map_or(Content::Empty, Content::Char)
        }
    }

    pub fn width(self) -> Width {
        match (self.content & WIDTH_MASK) >> WIDTH_SHIFT {
            1 => Width::Wide,
            2 => Width::WideTail,
            3 => Width::Pad,
            _ => Width::Narrow,
        }
    }

    pub fn style(self) -> StyleIndex {
        self.style
    }

    pub fn link(self) -> LinkIndex {
        self.link
    }

    /// Whether the cell is one of the cells of a sized run, the block of
    /// cells that one OSC 66 sequence writes. Its line's run table says
    /// which run ([`crate::Line::run_at`]).
    pub fn in_run(self) -> bool {
        self.content & RUN != 0
    }

    /// The cell, marked as a cell of a sized run when `on` is true, and
    /// unmarked otherwise.
    pub(crate) fn with_run(self, on: bool) -> Self {
        let content = if on {
            self.content | RUN
        } else {
            self.content & !RUN
        };
        Self { content, ..self }
    }

    /// Whether the cell is [`Cell::BLANK`]. Trailing blank cells are what
    /// a line drops when it is trimmed.
    pub fn is_blank(self) -> bool {
        self == Self::BLANK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blank_cell_is_all_zero_and_the_default() {
        assert_eq!(Cell::BLANK, Cell::default());
        assert_eq!(Cell::BLANK.content(), Content::Empty);
        assert_eq!(Cell::BLANK.width(), Width::Narrow);
        assert_eq!(Cell::BLANK.style(), StyleIndex(0));
        assert_eq!(Cell::BLANK.link(), LinkIndex(0));
    }

    #[test]
    fn content_width_style_and_link_round_trip() {
        let contents = [
            Content::Empty,
            Content::Char('a'),
            Content::Char('\u{10FFFF}'),
            Content::Char('\u{1F600}'),
            Content::Cluster(ClusterOffset(0)),
            Content::Cluster(ClusterOffset(VALUE_MASK)),
        ];
        for content in contents {
            for width in [Width::Narrow, Width::Wide, Width::WideTail, Width::Pad] {
                let cell = Cell::new(content, width, StyleIndex(65535), LinkIndex(7));
                assert_eq!(cell.content(), content);
                assert_eq!(cell.width(), width);
                assert_eq!(cell.style(), StyleIndex(65535));
                assert_eq!(cell.link(), LinkIndex(7));
            }
        }
    }

    #[test]
    fn the_run_mark_keeps_the_content_the_width_and_the_style() {
        let cell = Cell::new(
            Content::Char('\u{10FFFF}'),
            Width::Pad,
            StyleIndex(9),
            LinkIndex(3),
        );
        let marked = cell.with_run(true);
        assert!(marked.in_run());
        assert!(!cell.in_run());
        assert_eq!(marked.content(), Content::Char('\u{10FFFF}'));
        assert_eq!(marked.width(), Width::Pad);
        assert_eq!(marked.style(), StyleIndex(9));
        assert_eq!(marked.link(), LinkIndex(3));
        assert_eq!(marked.with_run(false), cell);
        assert!(!Cell::BLANK.with_run(true).is_blank());
    }

    #[test]
    fn a_written_cell_is_not_blank() {
        let space = Cell::new(
            Content::Char(' '),
            Width::Narrow,
            StyleIndex(0),
            LinkIndex(0),
        );
        assert!(!space.is_blank());
        let styled = Cell::new(Content::Empty, Width::Narrow, StyleIndex(1), LinkIndex(0));
        assert!(!styled.is_blank());
        assert!(Cell::BLANK.is_blank());
    }
}
