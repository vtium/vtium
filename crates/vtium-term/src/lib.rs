#![forbid(unsafe_code)]
//! Terminal state.
//!
//! Applies parsed actions to the screen: cells and lines, scrollback,
//! wrapping, grapheme clusters and widths, the cursor, character sets,
//! attributes (SGR), the mode registry, and the replies a program can ask
//! for (device attributes, mode reports, identification, capability
//! queries). Produces frames: what changed since a display last drew.
//!
//! The cell and line model is designed here rather than inherited: line
//! continuation (for reflow), hyperlink ids, overline and conceal are
//! budgeted from the start.
