#![forbid(unsafe_code)]
//! Glyph sources.
//!
//! Loads fonts and produces glyph bitmaps for the renderer: PSF and the
//! built-in unifont for the minspec build; outline fonts with shaping and
//! fallback behind a feature. Font files are admin-supplied but still
//! parsed with bounds, since a crafted font must not corrupt memory.
