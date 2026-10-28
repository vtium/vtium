//! Vitrified sets (ADR-1200): fonts baked ahead of time into one portable
//! file of glyph bitmaps, with their Unicode map, cell metrics, sprite
//! policy and provenance, which is the only form a font reaches the console
//! in.
//!
//! - [`SetBuilder`] writes a set's bytes, for the tests and for
//!   `vtium-vitrify`. The same set, built anywhere, is the same bytes.
//! - [`Set`] reads one: [`Set::from_bytes`] a whole file in memory, as
//!   `vtium-vitrify install` checks a portable set, and [`Store::load`] a
//!   set from the root-owned store, as the console does.
//! - [`Store`] is the store's check: the directory, the manifest and the
//!   set file opened without following a link, each owned by the expected
//!   owner and writable by no one else, and the set's SHA-256 matched to
//!   its manifest row.
//! - [`SetFace`] draws one cell size of a set as a [`Face`](crate::Face),
//!   holding in memory only the chunks of glyphs it has drawn.
//! - [`builtin`] gives the built-in sets, `unifont` and `unscii-8`, and
//!   [`builtin_name`] the one ADR-1200 D7 draws for a cell height and
//!   locale.
//!
//! The format is in `format.rs`, the bounds in `src/bounds.rs`, and the
//! crate's README describes both.

mod builder;
mod builtin;
mod face;
pub(crate) mod format;
mod manifest;
mod read;
mod store;
pub(crate) mod write;

pub use builder::{SetBuilder, StrikeBuilder, WriteError};
pub(crate) use builtin::unifont;
pub use builtin::{builtin, builtin_name};
pub use face::SetFace;
pub use manifest::{Manifest, ManifestError, ManifestRow};
pub use read::{PixelFormat, Set, SetError, StrikeInfo};
pub use store::{Store, StoreError};

/// The format version this build writes and reads, major and minor.
pub const VERSION: (u16, u16) = (format::MAJOR, format::MINOR);
