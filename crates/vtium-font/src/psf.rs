//! [`Psf`]: PC Screen Fonts, PSF1 and PSF2, the Linux console's bitmap
//! fonts, and its raw headerless fonts, plain or gzipped (feature `gzip`).
//!
//! The format is kbd's, written down in Andries Brouwer's "The PSF font
//! format" (kbd's `doc/font-formats`): a header, the glyph bitmaps, and an
//! optional unicode table giving each glyph's codepoints in glyph order. A
//! glyph's entry is its single codepoints, then any sequences, each opened
//! by the start-of-sequence mark, then the terminator:
//!
//! ```text
//! <entry> := <codepoint>* <sequence>* <terminator>
//! <sequence> := <start> <codepoint>+
//! ```
//!
//! PSF1 writes each codepoint as a little-endian u16, with 0xFFFE to start a
//! sequence and 0xFFFF to end an entry; PSF2 writes UTF-8, with the bytes
//! 0xFE and 0xFF, which UTF-8 never uses.
//!
//! The behaviour follows the C reference's `font_psf.c`; the departures are
//! listed in the crate's README. That file's notice, under the MIT licence
//! whose text is in COPYING:
//!
//! Copyright (c) awsq.code <awsq.code@gmail.com>, with fragments by David
//! Herrmann <dh.herrmann@googlemail.com>

use std::fmt;
use std::io::Read;
use std::path::Path;

use crate::bounds::{
    CELL_SIDE_MAX, GZIP_BYTES_MAX, PSF_BYTES_MAX, PSF_ENTRIES_MAX, PSF_GLYPHS_MAX, PSF_HEIGHT_MAX,
    PSF_WIDTH_MAX, RAW_HEIGHT_MAX,
};
use crate::face::{Cell, Face, Style};
use crate::glyph::Glyph;

const PSF1_MAGIC: [u8; 2] = [0x36, 0x04];
const PSF1_MODE512: u8 = 0x01;
const PSF1_MODEHASTAB: u8 = 0x02;
const PSF1_MODESEQ: u8 = 0x04;
const PSF1_START: u16 = 0xfffe;
const PSF1_END: u16 = 0xffff;

const PSF2_MAGIC: [u8; 4] = [0x72, 0xb5, 0x4a, 0x86];
const PSF2_HEADER: u32 = 32;
const PSF2_HAS_TABLE: u32 = 0x01;
const PSF2_START: u8 = 0xfe;
const PSF2_END: u8 = 0xff;

/// Why a PSF font was refused.
#[derive(Debug)]
#[non_exhaustive]
pub enum PsfError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The file starts with neither PSF magic nor gzip's, and is not the
    /// size of a raw font: 256 times a height of 1 to [`RAW_HEIGHT_MAX`].
    NotPsf,
    /// The header asks for a font past a bound, or describes itself
    /// inconsistently.
    BadHeader(&'static str),
    /// The file ends before the header or the glyphs it describes.
    Truncated,
    /// The file, or what it decompresses to, passes [`PSF_BYTES_MAX`], or a
    /// gzipped file passes [`GZIP_BYTES_MAX`].
    TooLarge,
    /// The unicode table maps more than [`PSF_ENTRIES_MAX`] codepoints.
    TableTooLarge,
    /// The file is gzipped and the build leaves out the `gzip` feature.
    GzipUnsupported,
    /// The file is gzipped and its gzip framing or data is corrupt.
    Gzip(&'static str),
}

impl fmt::Display for PsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PsfError::Io(e) => write!(f, "cannot read the font: {e}"),
            PsfError::NotPsf => write!(
                f,
                "not a PSF font, and not a raw font of 256*N bytes, 0 < N <= {RAW_HEIGHT_MAX}"
            ),
            PsfError::BadHeader(why) => write!(f, "bad PSF header: {why}"),
            PsfError::Truncated => f.write_str("the font is shorter than its header says"),
            PsfError::TooLarge => write!(
                f,
                "the font is larger than {PSF_BYTES_MAX} bytes, or than {GZIP_BYTES_MAX} gzipped"
            ),
            PsfError::TableTooLarge => write!(
                f,
                "the font's unicode table maps more than {PSF_ENTRIES_MAX} codepoints"
            ),
            PsfError::GzipUnsupported => f.write_str(
                "compressed PSF fonts need the gzip feature, which this build leaves out",
            ),
            PsfError::Gzip(why) => write!(f, "bad gzip font: {why}"),
        }
    }
}

impl std::error::Error for PsfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PsfError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PsfError {
    fn from(e: std::io::Error) -> PsfError {
        PsfError::Io(e)
    }
}

/// One codepoint the unicode table maps to a glyph.
#[derive(Clone, Copy, Debug)]
struct Entry {
    code: u32,
    glyph: u16,
}

/// A loaded PSF font, drawn at a whole multiple of its size.
#[derive(Clone)]
pub struct Psf {
    count: u32,
    width: u32,
    height: u32,
    row_bytes: u32,
    glyphs: Box<[u8]>,
    /// Sorted by codepoint, one entry each; `None` when the font has no
    /// table and a codepoint is its own glyph index.
    table: Option<Box<[Entry]>>,
    scale: u32,
}

/// The font's shape, never its glyph bytes, which can run to megabytes.
impl fmt::Debug for Psf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Psf")
            .field("glyphs", &self.count)
            .field("size", &(self.width, self.height))
            .field("scale", &self.scale)
            .field("table", &self.table.as_ref().map(|t| t.len()))
            .finish()
    }
}

impl Psf {
    /// Reads and parses the font at `path`, reading at most
    /// [`PSF_BYTES_MAX`] bytes and one more to tell a larger file.
    pub fn load(path: impl AsRef<Path>) -> Result<Psf, PsfError> {
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(u64::from(PSF_BYTES_MAX).saturating_add(1))
            .read_to_end(&mut bytes)?;
        Psf::parse(&bytes)
    }

    /// Parses a PSF1 or PSF2 font, or a raw one, gzipped or not, drawn at its own size
    /// until [`Psf::set_height`] scales it.
    pub fn parse(bytes: &[u8]) -> Result<Psf, PsfError> {
        if bytes.len() > usize::try_from(PSF_BYTES_MAX).unwrap_or(usize::MAX) {
            return Err(PsfError::TooLarge);
        }
        if bytes.starts_with(&[0x1f, 0x8b]) {
            if bytes.len() > usize::try_from(GZIP_BYTES_MAX).unwrap_or(usize::MAX) {
                return Err(PsfError::TooLarge);
            }
            return Psf::parse_gzip(bytes);
        }
        Psf::parse_plain(bytes)
    }

    #[cfg(feature = "gzip")]
    fn parse_gzip(bytes: &[u8]) -> Result<Psf, PsfError> {
        use crate::gzip::{GzipError, gunzip};
        let limit = usize::try_from(PSF_BYTES_MAX).unwrap_or(usize::MAX);
        match gunzip(bytes, limit) {
            Ok(plain) => Psf::parse_plain(&plain),
            Err(GzipError::TooLarge) => Err(PsfError::TooLarge),
            Err(GzipError::Corrupt(why)) => Err(PsfError::Gzip(why)),
        }
    }

    #[cfg(not(feature = "gzip"))]
    fn parse_gzip(_: &[u8]) -> Result<Psf, PsfError> {
        Err(PsfError::GzipUnsupported)
    }

    fn parse_plain(bytes: &[u8]) -> Result<Psf, PsfError> {
        let (count, width, height, row_bytes, header, has_table, psf1) = match bytes {
            [0x36, 0x04, mode, height, ..] => {
                let count = if mode & PSF1_MODE512 != 0 { 512 } else { 256 };
                let has_table = mode & (PSF1_MODEHASTAB | PSF1_MODESEQ) != 0;
                let height = u32::from(*height);
                (count, 8, height, 1, 4, has_table, true)
            }
            [0x72, 0xb5, 0x4a, 0x86, ..] => {
                let field = |i: usize| -> Result<u32, PsfError> {
                    let at = i.saturating_mul(4);
                    match bytes.get(at..at.saturating_add(4)) {
                        Some(&[a, b, c, d]) => Ok(u32::from_le_bytes([a, b, c, d])),
                        _ => Err(PsfError::Truncated),
                    }
                };
                let header = field(2)?;
                let flags = field(3)?;
                let count = field(4)?;
                let glyph_bytes = field(5)?;
                let height = field(6)?;
                let width = field(7)?;
                let row_bytes = width.div_ceil(8);
                if height.checked_mul(row_bytes) != Some(glyph_bytes) {
                    return Err(PsfError::BadHeader(
                        "the glyph size does not match its width and height",
                    ));
                }
                if header < PSF2_HEADER {
                    return Err(PsfError::BadHeader("the header is shorter than 32 bytes"));
                }
                let has_table = flags & PSF2_HAS_TABLE != 0;
                (count, width, height, row_bytes, header, has_table, false)
            }
            _ if starts_magic(bytes, &PSF1_MAGIC) || starts_magic(bytes, &PSF2_MAGIC) => {
                return Err(PsfError::Truncated);
            }
            _ => return Psf::parse_raw(bytes),
        };
        if count == 0 || count > PSF_GLYPHS_MAX {
            return Err(PsfError::BadHeader("no glyphs, or more than 65535"));
        }
        if width == 0 || width > PSF_WIDTH_MAX || height == 0 || height > PSF_HEIGHT_MAX {
            return Err(PsfError::BadHeader(
                "a glyph 0 pixels across, wider than 32 or taller than 256",
            ));
        }
        let glyph_bytes = height.saturating_mul(row_bytes);
        let end = count
            .checked_mul(glyph_bytes)
            .and_then(|n| n.checked_add(header))
            .filter(|&n| n <= PSF_BYTES_MAX)
            .ok_or(PsfError::TooLarge)?;
        let (Ok(start), Ok(end)) = (usize::try_from(header), usize::try_from(end)) else {
            return Err(PsfError::TooLarge);
        };
        let glyphs = bytes.get(start..end).ok_or(PsfError::Truncated)?;
        let table = if has_table {
            let raw = bytes.get(end..).unwrap_or_default();
            let entries = if psf1 {
                psf1_table(raw, count)?
            } else {
                psf2_table(raw, count)?
            };
            Some(merge(entries))
        } else {
            None
        };
        Ok(Psf {
            count,
            width,
            height,
            row_bytes,
            glyphs: glyphs.into(),
            table,
            scale: 1,
        })
    }

    /// A raw (headerless) console font, as setfont and the kernel take it:
    /// 256 glyphs, one byte a row, 8 pixels across, so a file of 256 times
    /// the glyph's height in bytes. It has no unicode table.
    fn parse_raw(bytes: &[u8]) -> Result<Psf, PsfError> {
        let height = u32::try_from(bytes.len() >> 8).unwrap_or(u32::MAX);
        if bytes.len() & 0xff != 0 || height == 0 || height > RAW_HEIGHT_MAX {
            return Err(PsfError::NotPsf);
        }
        Ok(Psf {
            count: 256,
            width: 8,
            height,
            row_bytes: 1,
            glyphs: bytes.into(),
            table: None,
            scale: 1,
        })
    }

    /// Scales the font by the largest whole factor of its height that fits
    /// `height`, at least 1, as far as keeps its cell inside
    /// [`CELL_SIDE_MAX`].
    pub fn set_height(&mut self, height: u32) {
        let fit = CELL_SIDE_MAX
            .checked_div(self.width.max(self.height))
            .unwrap_or(1);
        let scale = height.checked_div(self.height).unwrap_or(1);
        self.scale = scale.clamp(1, fit.max(1));
    }

    /// How many glyphs the font holds.
    pub fn glyph_count(&self) -> u32 {
        self.count
    }

    /// A glyph's width and height in the font, before scaling.
    pub fn glyph_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The factor glyphs are drawn at.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// Whether the font carries a unicode table.
    pub fn has_table(&self) -> bool {
        self.table.is_some()
    }

    /// The glyph `c` draws: the table's entry for it, or, in a font without
    /// a table, the glyph whose index is `c`'s codepoint.
    pub fn glyph_index(&self, c: char) -> Option<u32> {
        let code = u32::from(c);
        match &self.table {
            Some(table) => table
                .binary_search_by_key(&code, |e| e.code)
                .ok()
                .and_then(|i| table.get(i))
                .map(|e| u32::from(e.glyph)),
            None => Some(code).filter(|&g| g < self.count),
        }
    }

    /// Glyph `index` drawn at the font's scale: each source pixel a square
    /// `scale` across, bold widening each row by a pixel to the right.
    pub fn render(&self, index: u32, style: Style) -> Option<Glyph> {
        self.render_at(index, style, self.scale)
    }

    /// Glyph `index` drawn at `scale`.
    fn render_at(&self, index: u32, style: Style, scale: u32) -> Option<Glyph> {
        let glyph_bytes = self.height.checked_mul(self.row_bytes)?;
        let start = usize::try_from(index.checked_mul(glyph_bytes)?).ok()?;
        let glyph = self
            .glyphs
            .get(start..start.checked_add(usize::try_from(glyph_bytes).ok()?)?)?;
        let row_bytes = usize::try_from(self.row_bytes).ok()?;
        let scale = scale.max(1);
        let mut out = Glyph::mask(
            self.width.saturating_mul(scale),
            self.height.saturating_mul(scale),
        )?;
        for (i, row) in (0u32..).zip(out.rows_mut()) {
            let k = usize::try_from(i.checked_div(scale)?).ok()?;
            let src = glyph.get(k.checked_mul(row_bytes)?..)?.get(..row_bytes)?;
            let mut bits = read_row(src, self.width);
            if style.bold {
                bits |= bits >> 1;
            }
            for (j, px) in (0u32..).zip(row.iter_mut()) {
                let col = j.checked_div(scale).unwrap_or(0);
                let bit = self.width.saturating_sub(1).saturating_sub(col);
                *px = if bits >> bit & 1 != 0 { 0xff } else { 0 };
            }
        }
        Some(out)
    }
}

/// Whether `bytes` and `magic` agree as far as both go.
fn starts_magic(bytes: &[u8], magic: &[u8]) -> bool {
    bytes.iter().zip(magic).all(|(a, b)| a == b)
}

/// A glyph row's `width` leftmost bits, the first pixel highest.
fn read_row(bytes: &[u8], width: u32) -> u32 {
    let row = bytes
        .iter()
        .fold(0u32, |acc, &b| acc.wrapping_shl(8) | u32::from(b));
    let bits = u32::try_from(bytes.len())
        .unwrap_or(0)
        .saturating_mul(8)
        .saturating_sub(width);
    row.checked_shr(bits).unwrap_or(0)
}

/// One more entry for the table, refused past [`PSF_ENTRIES_MAX`].
fn push(entries: &mut Vec<Entry>, code: u32, glyph: u16) -> Result<(), PsfError> {
    if entries.len() >= usize::try_from(PSF_ENTRIES_MAX).unwrap_or(usize::MAX) {
        return Err(PsfError::TableTooLarge);
    }
    entries.push(Entry { code, glyph });
    Ok(())
}

/// The PSF1 table's single codepoints: u16 units, an odd byte at the end
/// dropped, sequences skipped, and entries past the last glyph ignored.
fn psf1_table(raw: &[u8], count: u32) -> Result<Vec<Entry>, PsfError> {
    let mut entries = Vec::new();
    let mut glyph = 0u32;
    let mut in_sequence = false;
    for unit in raw.chunks_exact(2) {
        if glyph >= count {
            break;
        }
        let v = match unit {
            &[lo, hi] => u16::from_le_bytes([lo, hi]),
            _ => break,
        };
        match v {
            PSF1_END => {
                glyph = glyph.saturating_add(1);
                in_sequence = false;
            }
            PSF1_START => in_sequence = true,
            _ if in_sequence => {}
            _ => push(
                &mut entries,
                u32::from(v),
                u16::try_from(glyph).unwrap_or(0),
            )?,
        }
    }
    Ok(entries)
}

/// The PSF2 table's single codepoints: each glyph's UTF-8 up to its first
/// start-of-sequence mark, with any byte that is not part of a well-formed
/// character dropped, and entries past the last glyph ignored.
fn psf2_table(raw: &[u8], count: u32) -> Result<Vec<Entry>, PsfError> {
    let mut entries = Vec::new();
    let descriptions = raw.split(|&b| b == PSF2_END);
    for (glyph, desc) in (0..count).zip(descriptions) {
        let singles = desc.split(|&b| b == PSF2_START).next().unwrap_or_default();
        let glyph = u16::try_from(glyph).unwrap_or(0);
        for chunk in singles.utf8_chunks() {
            for c in chunk.valid().chars() {
                push(&mut entries, u32::from(c), glyph)?;
            }
        }
    }
    Ok(entries)
}

/// The entries sorted by codepoint, one each: where the table lists a
/// codepoint for several glyphs, the last listed wins, as the Linux console
/// loads a table (`con_insert_unipair` in drivers/tty/vt/consolemap.c
/// overwrites).
fn merge(mut entries: Vec<Entry>) -> Box<[Entry]> {
    entries.sort_by_key(|e| e.code);
    entries.dedup_by(|later, kept| {
        if later.code == kept.code {
            kept.glyph = later.glyph;
            true
        } else {
            false
        }
    });
    entries.into_boxed_slice()
}

impl Face for Psf {
    fn cell(&self) -> Cell {
        Cell {
            width: self.width.saturating_mul(self.scale),
            height: self.height.saturating_mul(self.scale),
        }
    }

    fn size_step(&self) -> u32 {
        self.height
    }

    fn has_glyph(&self, c: char) -> bool {
        self.glyph_index(c).is_some_and(|g| g < self.count)
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        let index = self.glyph_index(*cluster.first()?)?;
        self.render(index, style)
    }

    /// Glyph 0, for a font that holds neither U+25A1 nor `?`.
    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.render(0, style)
    }

    fn scale(&self) -> u32 {
        self.scale
    }

    fn draw_unscaled(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        let index = self.glyph_index(*cluster.first()?)?;
        self.render_at(index, style, 1)
    }

    fn last_resort_unscaled(&self, style: Style) -> Option<Glyph> {
        self.render_at(0, style, 1)
    }
}
