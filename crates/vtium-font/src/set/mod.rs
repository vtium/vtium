//! Vitrified sets (ADR-1200 D2): glyph bitmaps with their Unicode map,
//! cell metrics, sprite policy and provenance in the glyph chunk format,
//! the form the helper's glyphs reach the console in and the built-in
//! fonts are compiled in as.
//!
//! - [`SetBuilder`] writes a set's bytes, for the helper, the console's
//!   per-boot store and the tests. The same set, built anywhere, is the
//!   same bytes.
//! - [`Set`] reads one: [`Set::from_bytes`] a whole set in memory, as the
//!   console reads each of the helper's frames.
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
mod read;
pub(crate) mod write;

pub use builder::{SetBuilder, StrikeBuilder, WriteError};
#[cfg(feature = "unifont")]
pub(crate) use builtin::unifont;
pub use builtin::{builtin, builtin_name};
pub use face::SetFace;
pub use read::{PixelFormat, Set, SetError, StrikeInfo};

/// The format version this build writes and reads, major and minor.
pub const VERSION: (u16, u16) = (format::MAJOR, format::MINOR);
