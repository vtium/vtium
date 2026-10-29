//! The vitrified set's layout (ADR-1200 D2): the magic, the version, the
//! section tags and every record's codes and widths, shared by the writer
//! and the reader. `build.rs` includes this file to pack the built-in sets.
//!
//! A set is little-endian throughout, every field of fixed width:
//!
//! ```text
//! header   magic "VTGLASS\0", major u16, minor u16, section count u32,
//!          then each section: tag [u8; 4], offset u64, length u64
//! META     UTF-8 lines "key=value\n"
//! STRK     count u32, record size u32, then each strike:
//!            style u8, format u8, 0 u16, cell width u16, cell height u16,
//!            baseline u16, underline u16, underline thickness u16,
//!            strike u16, strike thickness u16, 0 u16, glyph count u32
//! CMAP     strike count u32, record size u32, then for each strike:
//!            run count u32, then each run:
//!              first codepoint u32, length u32, first glyph u32, flags u32
//! POLY     each sprite range's source [u8; 6], 0 u16, strike count u32,
//!          record size u32, then for each strike and each range:
//!            covered u32, total u32
//! GLYF     chunk count u32, record size u32, then each chunk:
//!            offset u64, stored length u32, decoded length u32, flags u32
//!          then the chunks' bytes
//! ```
//!
//! `STRK`'s strikes are sorted by cell height, width, style and format,
//! no two alike, so one cell may hold a 1-bit, an R8 and an RGBA strike
//! of each style, their codepoints apart, and every cell size has a
//! regular strike.
//!
//! The sections follow the header in the order the table lists them, with
//! no byte between them or after the last. A record size is at least the
//! size this version writes; a reader of this major version reads the
//! fields it knows and skips the rest, so a minor version can add fields at
//! a record's end and sections a reader skips. Reserved fields are 0.

/// The file's first eight bytes.
pub(crate) const MAGIC: [u8; 8] = *b"VTGLASS\0";
/// The format's major version: a reader refuses any other. Version 2
/// dropped the chunk record's SHA-256: the format is the helper's wire
/// format, written to no file (ADR-1200 D2).
pub(crate) const MAJOR: u16 = 2;
/// The format's minor version: a reader takes any.
pub(crate) const MINOR: u16 = 0;

pub(crate) const META: [u8; 4] = *b"META";
pub(crate) const STRK: [u8; 4] = *b"STRK";
pub(crate) const CMAP: [u8; 4] = *b"CMAP";
pub(crate) const POLY: [u8; 4] = *b"POLY";
pub(crate) const GLYF: [u8; 4] = *b"GLYF";

/// The magic, the two versions and the section count.
pub(crate) const PRELUDE_BYTES: u32 = 16;
/// A section table entry: tag, offset, length.
pub(crate) const SECTION_BYTES: u32 = 20;
/// A table's count and record size.
pub(crate) const TABLE_HEAD_BYTES: u32 = 8;

pub(crate) const STRIKE_RECORD: u32 = 24;
pub(crate) const RUN_RECORD: u32 = 16;
/// Six ranges' covered and total counts.
pub(crate) const COVERAGE_RECORD: u32 = 48;
pub(crate) const CHUNK_RECORD: u32 = 20;

/// A run's flag: its glyphs are two cells wide.
pub(crate) const RUN_WIDE: u32 = 1;
/// A chunk's flag: it is stored as a raw deflate stream (RFC 1951).
pub(crate) const CHUNK_DEFLATED: u32 = 1;

/// Pixel formats. A 1-bit row is padded to a byte, its first pixel the
/// high bit; R8 is a byte of coverage a pixel; RGBA is four bytes a pixel,
/// the colors premultiplied by alpha. Format 3 is reserved for ADR-1300's
/// subpixel strikes, and a reader of this version refuses it.
pub(crate) const FORMAT_BITS: u8 = 0;
pub(crate) const FORMAT_R8: u8 = 1;
pub(crate) const FORMAT_RGBA: u8 = 2;

/// Style bits: 0 is regular, 3 bold italic.
pub(crate) const STYLE_BOLD: u8 = 1;
pub(crate) const STYLE_ITALIC: u8 = 2;

/// A sprite range's source in `POLY`, in `Range::ALL`'s order.
pub(crate) const SOURCE_FONT: u8 = 0;
pub(crate) const SOURCE_SPRITES: u8 = 1;
pub(crate) const SOURCE_AUTO: u8 = 2;

/// The deflate level the writer uses: fixed, so a set's bytes depend only
/// on its inputs and the tool's version.
pub(crate) const DEFLATE_LEVEL: u8 = 9;

/// The bytes one glyph takes as stored, `cells` cells of `width` by
/// `height` in `format`; `None` for an unknown format or past `u32`.
pub(crate) fn glyph_bytes(format: u8, width: u32, height: u32, cells: u32) -> Option<u32> {
    let w = width.checked_mul(cells)?;
    let row = match format {
        FORMAT_BITS => w.div_ceil(8),
        FORMAT_R8 => w,
        FORMAT_RGBA => w.checked_mul(4)?,
        _ => return None,
    };
    row.checked_mul(height)
}

/// Whether `name` is a set name ADR-1200 D1 admits: 1 to 64 characters of
/// `A-Z a-z 0-9 . _ + -`, not starting with `.`.
pub(crate) fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}
