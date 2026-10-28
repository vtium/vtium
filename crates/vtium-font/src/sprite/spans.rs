//! The codepoints of each sprite [`Range`](super::Range), as first and last
//! codepoint pairs. `build.rs` includes this file to count a built-in set's
//! coverage of each range, as a set's writer does (ADR-1200 D2, `POLY`).

pub(crate) const BOX_DRAWING: &[(u32, u32)] = &[(0x2500, 0x257f)];
pub(crate) const BLOCK_ELEMENTS: &[(u32, u32)] = &[(0x2580, 0x259f)];
pub(crate) const BRAILLE: &[(u32, u32)] = &[(0x2800, 0x28ff)];
pub(crate) const SEXTANTS: &[(u32, u32)] = &[(0x1fb00, 0x1fb3b)];
pub(crate) const OCTANTS: &[(u32, u32)] = &[(0x1cd00, 0x1cde5)];
pub(crate) const LEGACY_COMPUTING: &[(u32, u32)] = &[
    (0x1cea0, 0x1ceaf),
    (0x1fb3c, 0x1fb92),
    (0x1fb94, 0x1fbaf),
    (0x1fbbd, 0x1fbbf),
    (0x1fbce, 0x1fbef),
];

/// Every range's spans, in `Range::ALL`'s order.
pub(crate) const ALL: [&[(u32, u32)]; 6] = [
    BOX_DRAWING,
    BLOCK_ELEMENTS,
    BRAILLE,
    SEXTANTS,
    OCTANTS,
    LEGACY_COMPUTING,
];
