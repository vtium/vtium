//! How far a glyph's ink may reach past its own cells (ADR-1302 D1,
//! ADR-1306 D1 and D12). The set's reader and writer, the glyph type, the
//! glyph helper and the renderer read the bound from these two constants
//! and from nowhere else, so widening it is a change of a constant.

/// How far a glyph's ink may reach past its own cells to the left and to
/// the right, in cells. Ink past it is cut.
pub const REACH_CELLS: u32 = 1;

/// How far a glyph's ink may reach above and below its own row, in rows.
/// Ink past it is cut.
pub const REACH_ROWS: u32 = 1;
