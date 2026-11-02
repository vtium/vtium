//! The keys, the pixels and the requests the store takes.

use core::fmt;
use std::sync::Arc;

use crate::rules::Protocol;

/// A line, as the terminal names it: an opaque 64-bit key, unique among
/// the terminal's lines and carrying no order (ADR-1003). vtium-term passes
/// each line's `LineId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineKey(u64);

impl LineKey {
    pub const fn new(key: u64) -> Self {
        Self(key)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An image's key: counted up by its store and never reused, so a frame
/// that still names it names no other image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImageKey(pub(crate) u64);

impl ImageKey {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A placement's key: counted up by its store and never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlacementKey(pub(crate) u64);

impl PlacementKey {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An image's pixels: premultiplied ARGB, one `u32` a pixel as
/// `0xAARRGGBB`, rows top to bottom, the color glyph format of ADR-1300,
/// for every protocol. Shared, so a frame holds them without a copy.
#[derive(Clone)]
pub struct Pixels(Arc<[u32]>);

impl Pixels {
    pub fn new(pixels: Vec<u32>) -> Self {
        Self(pixels.into())
    }

    pub fn as_slice(&self) -> &[u32] {
        &self.0
    }

    /// Whether `other` shares these pixels, as a frame's do.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for Pixels {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) || self.0 == other.0
    }
}

impl fmt::Debug for Pixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pixels({} pixels)", self.0.len())
    }
}

/// An image a protocol made, for [`Store::add_image`](crate::Store::add_image).
#[derive(Clone, Debug)]
pub struct NewImage {
    pub protocol: Protocol,
    pub width: u32,
    pub height: u32,
    /// `width` times `height` pixels, premultiplied ARGB.
    pub pixels: Vec<u32>,
    /// Kitty's image id `i`, 1 to 2^32-1, or 0 for none. An image of an id
    /// already in the buffer in use replaces it, once the new one has its
    /// bytes.
    pub id: u32,
    /// Kitty's image number `I`, or 0.
    pub number: u32,
    /// Kitty's `N=1`: evicted before other unplaced images.
    pub transient: bool,
}

/// Where a placement goes, for [`Store::place`](crate::Store::place): the
/// replay interface's `place` command and kitty's put keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    /// The screen cell of the box's top left.
    pub col: u32,
    pub row: u32,
    /// The picture's offset into that cell, in pixels, each below the cell.
    pub x: u32,
    pub y: u32,
    /// The box in cells: both 0 for the picture at its pixel size, both
    /// given for the picture fitted and centred in it, one given for the
    /// other to follow the source's aspect.
    pub cols: u32,
    pub rows: u32,
    /// The source rectangle in the image's pixels, cut to the image; a
    /// width or height of 0 reaches the image's edge.
    pub src_x: u32,
    pub src_y: u32,
    pub src_width: u32,
    pub src_height: u32,
    /// The depth a kitty placement asks for.
    pub z: i32,
    /// Kitty's placement id `p`: a placement of the same image and id is
    /// replaced.
    pub id: u32,
}

/// Why the store refused a request, each answering as a Linux errno.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A size, a position or a source rectangle out of bounds (EINVAL).
    Invalid,
    /// No image of that key in the buffer in use (ENOENT).
    NotFound,
    /// Past the budget, or a buffer's count, with nothing that may go
    /// (ENOSPC).
    NoSpace,
}

impl Refusal {
    /// The refusal's errno, negated, as the replay interface prints it.
    pub fn errno(self) -> i32 {
        match self {
            Self::Invalid => -22,
            Self::NotFound => -2,
            Self::NoSpace => -28,
        }
    }
}
