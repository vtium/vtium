//! The cell: 8 bytes, with what it shows and indices into its line's
//! tables.
//!
//! A cell holds its content and width in 32 bits, the index of its style
//! in its line's style table in 16, and the index of its hyperlink in its
//! line's link table in 16. A grapheme cluster of more than one codepoint
//! lives in the line's cluster table, and the cell holds its index. The
//! change age is not in the cell: damage is kept by screen position
//! (`Damage`), so the scrollback carries none.

/// The bits of `Cell::content`: bits 0-20 a codepoint or a cluster index,
/// bit 21 set for a cluster, bits 22-23 the width, bits 24-31 zero.
const VALUE_MASK: u32 = (1 << 21) - 1;
const CLUSTER: u32 = 1 << 21;
const WIDTH_SHIFT: u32 = 22;
const WIDTH_MASK: u32 = 0b11 << WIDTH_SHIFT;

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
}

impl Width {
    /// The columns this cell advances the cursor by.
    pub const fn columns(self) -> u8 {
        match self {
            Self::Narrow => 1,
            Self::Wide => 2,
            Self::WideTail => 0,
        }
    }

    const fn bits(self) -> u32 {
        match self {
            Self::Narrow => 0,
            Self::Wide => 1,
            Self::WideTail => 2,
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

/// The index of a cluster in a line's table, below 2^21.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterIndex(pub(crate) u32);

/// What a cell shows, as the cell stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// Nothing written, or erased.
    Empty,
    /// One codepoint.
    Char(char),
    /// A grapheme cluster of two or more codepoints, in the line's table.
    Cluster(ClusterIndex),
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
            Content::Cluster(ClusterIndex(i)) => CLUSTER | (i & VALUE_MASK),
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
            Content::Cluster(ClusterIndex(value))
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
            _ => Width::Narrow,
        }
    }

    pub fn style(self) -> StyleIndex {
        self.style
    }

    pub fn link(self) -> LinkIndex {
        self.link
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
            Content::Cluster(ClusterIndex(0)),
            Content::Cluster(ClusterIndex(VALUE_MASK)),
        ];
        for content in contents {
            for width in [Width::Narrow, Width::Wide, Width::WideTail] {
                let cell = Cell::new(content, width, StyleIndex(65535), LinkIndex(7));
                assert_eq!(cell.content(), content);
                assert_eq!(cell.width(), width);
                assert_eq!(cell.style(), StyleIndex(65535));
                assert_eq!(cell.link(), LinkIndex(7));
            }
        }
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
