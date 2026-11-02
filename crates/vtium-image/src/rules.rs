//! The protocol tag and the one function that holds every protocol's rules
//! (ADR-1100, D3).
//!
//! Every image and placement carries a [`Protocol`]: the protocol whose
//! rules it follows. Its variants are private to this module, and
//! [`rules`] is the only place that maps one to its rules. The store and
//! vtium-term's screen ask the table; the frame entry carries what the
//! rules resolved, so the compositor reads no tag. `Protocol` has no
//! equality, so code outside this module can neither match on it nor
//! compare it with a constant:
//!
//! ```compile_fail
//! use vtium_image::Protocol;
//! fn is_sixel(p: Protocol) -> bool {
//!     matches!(p, Protocol::SIXEL)
//! }
//! ```
//!
//! ```compile_fail
//! use vtium_image::Protocol;
//! fn is_sixel(p: Protocol) -> bool {
//!     p == Protocol::SIXEL
//! }
//! ```
//!
//! What a protocol does is read from its rules instead:
//!
//! ```
//! use vtium_image::{Protocol, rules};
//! assert_eq!(rules(Protocol::SIXEL).name, "sixel");
//! assert!(rules(Protocol::SIXEL).cut_by_text);
//! assert!(!rules(Protocol::KITTY).cut_by_text);
//! ```
//!
//! A third protocol adds a variant, a constant and one arm of [`rules`].

/// The protocol whose rules an image or a placement follows. The iTerm2
/// compatibility handler places its pictures as [`SIXEL`](Self::SIXEL).
#[derive(Clone, Copy, Debug)]
pub struct Protocol(Tag);

#[derive(Clone, Copy, Debug)]
enum Tag {
    Sixel,
    Kitty,
}

impl Protocol {
    /// Sixel's rules: cell content, cut by text and erases.
    pub const SIXEL: Self = Self(Tag::Sixel);
    /// The kitty graphics protocol's rules: a layer text never touches.
    pub const KITTY: Self = Self(Tag::Kitty);
}

/// The layers a frame draws placements in, from the bottom (D4). Cell
/// backgrounds that are not the default draw between the first two, and
/// glyphs between the last two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Below cell backgrounds: kitty z below -1,073,741,824.
    BelowBackground,
    /// Below text: kitty z from -1,073,741,824 to -1.
    BelowText,
    /// Above text: kitty z of 0 or more, and every sixel.
    AboveText,
}

impl Layer {
    /// The layer's name in the replay interface's `placement` line.
    pub fn name(self) -> &'static str {
        match self {
            Self::BelowBackground => "below-bg",
            Self::BelowText => "below-text",
            Self::AboveText => "above-text",
        }
    }

    /// The layer kitty's z chooses: the spec's bands.
    fn of_z(z: i32) -> Self {
        if z < Z_BELOW_BACKGROUND {
            Self::BelowBackground
        } else if z < 0 {
            Self::BelowText
        } else {
            Self::AboveText
        }
    }
}

/// A kitty z below this draws under cell backgrounds.
const Z_BELOW_BACKGROUND: i32 = -1_073_741_824;

/// How a region scroll (margins, IL, DL) moves a placement it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionScroll {
    /// The part inside the moving band moves with it, the placement splits
    /// at the band's edges, and parts pushed out of the region are clipped.
    Split,
    /// A placement wholly inside the band moves and is clipped at its
    /// edges; one that crosses an edge keeps its place on the screen.
    WholeOnly,
}

/// One protocol's rules: the columns of D3's table.
#[derive(Debug)]
pub struct Rules {
    /// The kind's printed name.
    pub name: &'static str,
    /// A character written into a cell it covers, the erases (ED 0, ED 1,
    /// EL, ECH, DECSED, DECSEL, DECERA, DECSERA, DECFRA), DECCRA's
    /// destination, ICH, DCH and insert-mode printing cut the part under
    /// those cells.
    pub cut_by_text: bool,
    /// A new picture of the same tag over it removes, cell by cell, the
    /// parts under each cell the new picture covers fully with opaque
    /// pixels, and then each piece under it that draws nothing
    /// (`store/sixel.rs`).
    pub covered_by_same: bool,
    /// ED 2 removes it from the screen's own lines, as kitty's spec asks
    /// of the clear screen code. A placement text cuts loses those cells to
    /// the erase itself instead, keeping any part in the scrollback.
    pub erased_by_clear_screen: bool,
    /// What a region scroll does to it.
    pub region_scroll: RegionScroll,
    /// It may be evicted while placed on the screen's own lines, oldest
    /// transmission first.
    pub evict_on_screen: bool,
    /// Its image goes with its last placement, since nothing can name it
    /// again; otherwise an image stays until it is deleted or evicted.
    pub freed_with_last_placement: bool,
    /// Kitty's delete commands reach it.
    pub deleted_by_kitty: bool,
    /// Its images carry kitty's ids: one image to an id in each buffer, an
    /// image of an id already there replacing it, and in a stack the lower
    /// id below between two placements of different ids.
    pub ids: bool,
    /// The z and the layer a placement asked for at `z` takes.
    pub depth: fn(i32) -> (i32, Layer),
}

const SIXEL: Rules = Rules {
    name: "sixel",
    cut_by_text: true,
    covered_by_same: true,
    erased_by_clear_screen: false,
    region_scroll: RegionScroll::Split,
    evict_on_screen: false,
    freed_with_last_placement: true,
    deleted_by_kitty: false,
    ids: false,
    depth: |_| (0, Layer::AboveText),
};

const KITTY: Rules = Rules {
    name: "kitty",
    cut_by_text: false,
    covered_by_same: false,
    erased_by_clear_screen: true,
    region_scroll: RegionScroll::WholeOnly,
    evict_on_screen: true,
    freed_with_last_placement: false,
    deleted_by_kitty: true,
    ids: true,
    depth: |z| (z, Layer::of_z(z)),
};

/// The rules a tag follows: the one place each protocol's rules live.
pub fn rules(protocol: Protocol) -> &'static Rules {
    match protocol.0 {
        Tag::Sixel => &SIXEL,
        Tag::Kitty => &KITTY,
    }
}

/// Whether two tags are one protocol, for D3's "a new picture of the same
/// tag over it".
pub(crate) fn same(a: Protocol, b: Protocol) -> bool {
    matches!(
        (a.0, b.0),
        (Tag::Sixel, Tag::Sixel) | (Tag::Kitty, Tag::Kitty)
    )
}

/// A tag the store keeps, comparable so a store can be compared whole; the
/// comparison stays inside the crate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Kept(pub(crate) Protocol);

impl PartialEq for Kept {
    fn eq(&self, other: &Self) -> bool {
        same(self.0, other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // covers: image.pipeline
    #[test]
    fn each_tag_follows_its_own_column_of_the_table() {
        let six = rules(Protocol::SIXEL);
        let kit = rules(Protocol::KITTY);
        assert_eq!((six.name, kit.name), ("sixel", "kitty"));
        assert!(six.cut_by_text && !kit.cut_by_text);
        assert!(!six.erased_by_clear_screen && kit.erased_by_clear_screen);
        assert_eq!(six.region_scroll, RegionScroll::Split);
        assert_eq!(kit.region_scroll, RegionScroll::WholeOnly);
        assert!(!six.evict_on_screen && kit.evict_on_screen);
        assert!(six.freed_with_last_placement && !kit.freed_with_last_placement);
        assert!(!six.deleted_by_kitty && kit.deleted_by_kitty);
        assert!(!six.ids && kit.ids);
        assert!(same(Protocol::SIXEL, Protocol::SIXEL));
        assert!(!same(Protocol::SIXEL, Protocol::KITTY));
    }

    #[test]
    fn a_sixel_draws_above_text_at_z_0_and_kitty_by_its_bands() {
        let six = rules(Protocol::SIXEL).depth;
        let kit = rules(Protocol::KITTY).depth;
        assert_eq!(six(-5), (0, Layer::AboveText));
        assert_eq!(kit(0), (0, Layer::AboveText));
        assert_eq!(kit(-1), (-1, Layer::BelowText));
        assert_eq!(kit(-1_073_741_824), (-1_073_741_824, Layer::BelowText));
        assert_eq!(
            kit(-1_073_741_825),
            (-1_073_741_825, Layer::BelowBackground)
        );
        assert!(Layer::BelowBackground < Layer::BelowText);
        assert!(Layer::BelowText < Layer::AboveText);
    }
}
