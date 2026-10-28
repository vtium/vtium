#![forbid(unsafe_code)]
// A font file is untrusted input (docs/architecture.md, "Determinism"):
// nothing a crafted font holds can panic the loader or the drawing.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! Glyph sources.
//!
//! Loads fonts and draws glyph bitmaps for the renderer: vitrified sets
//! ([`set`], ADR-1200), the form every font reaches the console in, and
//! the root-owned store they are checked and loaded from; PSF fonts; the
//! built-in unifont, itself a set; and the sprites that draw box
//! drawing, block elements, Braille, sextants, octants and the rest of
//! Symbols for Legacy Computing's block shapes to fill their cell.
//! Outline fonts with shaping and fallback come later, behind a feature,
//! as another [`Face`].
//!
//! ```
//! use vtium_font::{Face, Font, Style, Width};
//! # #[cfg(feature = "unifont")] {
//! let font = Font::new(vtium_font::Unifont::new(16));
//! assert_eq!((font.cell().width, font.cell().height), (8, 16));
//! let a = font.glyph(&['A'], Width::Narrow, Style::default()).unwrap();
//! assert_eq!((a.width(), a.height()), (8, 16));
//! // a box-drawing character fills its cell, whatever the font holds
//! let line = font.glyph(&['│'], Width::Wide, Style::default()).unwrap();
//! assert_eq!((line.width(), line.height()), (16, 16));
//! # }
//! ```
//!
//! A [`Face`] draws a character at its own width; a [`Font`] puts a face
//! and the [`sprite`]s together and draws any cluster exactly into its
//! cells. A character the face lacks draws as its sprite where the policy
//! draws one, else as the font's [`Missing`] glyph: U+25A1, `?` or the
//! face's last resort, or the codepoint in hex.
//! Its [`SpritePolicy`] chooses, for each [`sprite::Range`], between the
//! face's glyphs and the sprites, and [`Font::coverage`] counts how much
//! of each range the face holds.
//! Every glyph is a [`Glyph`]: an 8-bit coverage mask, or premultiplied
//! RGBA for a color face. The bitmap faces and the sprites draw masks,
//! and the same font, size and character give the same bytes on every
//! host.
//!
//! The behaviour is the C reference's (kmscon's `font_psf.c`,
//! `font_unifont.c`, `font_box.c` and `font.c`), the floor for vtium; the
//! crate's README lists where vtium departs from it and why.
//!
//! # Bounds
//!
//! A font file or a set is parsed with every size bounded, and a malformed
//! one is refused with a [`PsfError`] or a [`set::SetError`] rather than a
//! panic. Each bound is defined once in `src/bounds.rs` with its source and
//! tabulated in the crate's README: [`PSF_GLYPHS_MAX`], [`PSF_WIDTH_MAX`],
//! [`PSF_HEIGHT_MAX`], [`PSF_BYTES_MAX`], [`PSF_ENTRIES_MAX`],
//! [`GZIP_BYTES_MAX`], [`CELL_SIDE_MAX`], [`SET_BYTES_MAX`],
//! [`SET_SECTIONS_MAX`], [`SET_META_BYTES_MAX`], [`SET_STRIKES_MAX`],
//! [`SET_GLYPHS_MAX`], [`SET_RUNS_MAX`], [`SET_CHUNKS_MAX`],
//! [`SET_CHUNK_BYTES`], [`SET_RECORD_BYTES_MAX`], [`MANIFEST_BYTES_MAX`] and
//! [`MANIFEST_ROWS_MAX`].

mod bounds;
mod face;
mod glyph;
#[cfg(feature = "gzip")]
mod gzip;
mod hex;
mod policy;
mod psf;
pub mod set;
mod sha256;
pub mod sprite;
#[cfg(feature = "unifont")]
mod unifont;

pub use bounds::{
    CELL_SIDE_MAX, GZIP_BYTES_MAX, MANIFEST_BYTES_MAX, MANIFEST_ROWS_MAX, PSF_BYTES_MAX,
    PSF_ENTRIES_MAX, PSF_GLYPHS_MAX, PSF_HEIGHT_MAX, PSF_WIDTH_MAX, SET_BYTES_MAX, SET_CHUNK_BYTES,
    SET_CHUNKS_MAX, SET_GLYPHS_MAX, SET_META_BYTES_MAX, SET_RECORD_BYTES_MAX, SET_RUNS_MAX,
    SET_SECTIONS_MAX, SET_STRIKES_MAX,
};
pub use face::{Cell, Decorations, Face, Font, REPLACEMENT, Style, Width};
pub use glyph::{Format, Glyph};
pub use policy::{Coverage, Missing, RangeCoverage, Source, SpritePolicy};
pub use psf::{Psf, PsfError};
pub use sha256::{Sha256, hex as sha256_hex};
#[cfg(feature = "unifont")]
pub use unifont::Unifont;
