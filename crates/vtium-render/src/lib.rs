#![forbid(unsafe_code)]
//! Rendering.
//!
//! Draws a terminal frame into a display buffer: cells through the glyph
//! cache, decorations and block/box sprites, images in their bands, the
//! cursor, and damage tracking so an unchanged frame costs nothing. The
//! CPU compositor is the reference; a GPU path, behind a feature, must
//! draw the same pixels.
