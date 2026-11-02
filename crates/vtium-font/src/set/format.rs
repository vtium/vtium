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
//! CLUS     optional: cluster count u32, record size u32, key pool length
//!          u32, chunk count u32, chunk record size u32, then each
//!          cluster, sorted by strike and then by its key:
//!            strike u16, key length u16, key u32, chunk u32, offset u32,
//!            flags u32
//!          then each scalar value of the key pool u32, then each chunk as
//!          GLYF's, its offset from CLUS's start, then the chunks' bytes
//! INKX     optional: box count u32, record size u32, chunk count u32,
//!          chunk record size u32, then each ink box, sorted by strike,
//!          then kind, then glyph:
//!            strike u16, kind u16 (0 a glyph of GLYF, by its number in its
//!            strike; 1 a cluster of CLUS, by its record's number), glyph
//!            u32, left i32, width u32, chunk u32, offset u32
//!          then each chunk as GLYF's, its offset from INKX's start, then
//!          the chunks' bytes
//! ```
//!
//! `INKX` holds the ink box of each glyph whose ink passes its cells
//! (ADR-1200 D2, ADR-1302 D6). A box is `width` columns across from
//! `left`, the offset of its first column from the left edge of the
//! glyph's first cell, which is 0 or negative, over the cell's full
//! height, in its strike's pixel format. A box spans its glyph's cells and
//! the ink past them, and stays inside the glyph's reach, its cells and
//! one cell more on each side: `left` is at least minus one cell's width,
//! and the box ends no further than one cell past the glyph's last cell.
//! A glyph keeps its pixels cut to its cells in `GLYF` or `CLUS` as before
//! and its whole box in an `INKX` chunk, so a reader that does not know
//! `INKX` skips it and draws every glyph cut to its cells. Only R8 and
//! 1-bit strikes hold boxes; a color strike's glyphs fit their cells. A
//! glyph whose box would pass `SET_CHUNK_BYTES` holds none, and is drawn
//! cut to its cells.
//!
//! `CLUS` maps clusters, short sequences of scalar values, to glyphs drawn
//! for them whole: a cluster's key is `key length` scalars of the pool
//! from `key`, and its glyph lies `offset` bytes into its chunk, one or two
//! cells wide by `RUN_WIDE`, in its strike's cell and pixel format. A set
//! with no cluster has no `CLUS`, and a reader that does not know the
//! section skips it, so the format stays 2.0, as it does for `INKX`.
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
pub(crate) const CLUS: [u8; 4] = *b"CLUS";
pub(crate) const INKX: [u8; 4] = *b"INKX";

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
pub(crate) const CLUSTER_RECORD: u32 = 20;
pub(crate) const INK_RECORD: u32 = 24;

/// An ink box's kind: a glyph of `GLYF`, or a cluster of `CLUS`.
pub(crate) const INK_GLYPH: u16 = 0;
pub(crate) const INK_CLUSTER: u16 = 1;

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

/// Whether a box `width` columns from `left` spans a glyph `span` pixels
/// wide, of cells `cell` pixels wide, passes it on one side at least, and
/// reaches no further than one cell past it on either side (ADR-1302 D1).
pub(crate) fn in_reach(left: i64, width: i64, cell: i64, span: i64) -> bool {
    let right = left.saturating_add(width);
    left <= 0
        && left.saturating_add(cell) >= 0
        && right >= span
        && right <= span.saturating_add(cell)
        && (left, right) != (0, span)
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
