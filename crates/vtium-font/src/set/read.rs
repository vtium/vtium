//! The set reader (ADR-1200 D2, D13): a set's header and every section
//! but its glyph chunks read and checked once, and each chunk inflated the
//! first time a glyph is drawn from it, then kept as stored; a 1-bit glyph
//! is expanded to an 8-bit mask as it is drawn.

use std::fmt;
use std::sync::OnceLock;

use crate::bounds::{
    SET_BYTES_MAX, SET_CHUNKS_MAX, SET_META_BYTES_MAX, SET_RECORD_BYTES_MAX, SET_SECTIONS_MAX,
};
use crate::face::{Cell, Decorations, Style, Width};
use crate::glyph::{Format, Glyph};
use crate::policy::{Coverage, RangeCoverage, SpritePolicy};
use crate::set::format::{
    CHUNK_RECORD, CMAP, FORMAT_BITS, FORMAT_R8, FORMAT_RGBA, GLYF, MAGIC, MAJOR, META, POLY,
    PRELUDE_BYTES, SECTION_BYTES, STRK, TABLE_HEAD_BYTES, glyph_bytes,
};
use crate::sprite::Range;

mod parse;

use parse::{parse_cmap, parse_index, parse_meta, parse_poly, parse_strikes};

/// Why a set was refused, or a chunk of it.
#[derive(Debug)]
#[non_exhaustive]
pub enum SetError {
    /// The set does not start with `VTGLASS\0`.
    NotSet,
    /// The set's major format version is not one this build reads.
    UnknownMajor(u16),
    /// The set ends before what its header or a table describes.
    Truncated,
    /// A count or size passes the named bound.
    Bound(&'static str),
    /// A section or record contradicts the format or itself.
    Malformed(&'static str),
    /// A strike's pixel format is one this build does not draw.
    UnknownFormat(u8),
    /// The chunk is not a deflate stream of exactly its decoded length.
    ChunkLength(usize),
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::NotSet => f.write_str("not a vitrified set"),
            SetError::UnknownMajor(v) => write!(
                f,
                "the set's format version {v} is not {MAJOR}, the version this vtium reads"
            ),
            SetError::Truncated => f.write_str("the set is shorter than its header says"),
            SetError::Bound(name) => write!(f, "the set passes {name}"),
            SetError::Malformed(why) => write!(f, "malformed set: {why}"),
            SetError::UnknownFormat(v) => write!(f, "a strike in pixel format {v}, not drawn"),
            SetError::ChunkLength(i) => {
                write!(f, "glyph chunk {i} does not decode to its length")
            }
        }
    }
}

impl std::error::Error for SetError {}

/// How a strike's glyphs are stored in the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// A bit a pixel, rows padded to a byte, expanded to an 8-bit mask
    /// as each glyph is drawn. Bitmap fonts.
    Bits,
    /// A byte of coverage a pixel. Outline fonts.
    R8,
    /// Four bytes a pixel, premultiplied. Color glyphs.
    Rgba,
}

impl PixelFormat {
    pub(crate) fn code(self) -> u8 {
        match self {
            PixelFormat::Bits => FORMAT_BITS,
            PixelFormat::R8 => FORMAT_R8,
            PixelFormat::Rgba => FORMAT_RGBA,
        }
    }

    fn from_code(code: u8) -> Result<PixelFormat, SetError> {
        match code {
            FORMAT_BITS => Ok(PixelFormat::Bits),
            FORMAT_R8 => Ok(PixelFormat::R8),
            FORMAT_RGBA => Ok(PixelFormat::Rgba),
            other => Err(SetError::UnknownFormat(other)),
        }
    }
}

/// One strike's description: what `STRK` holds for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StrikeInfo {
    pub style: Style,
    pub format: PixelFormat,
    pub cell: Cell,
    /// The baseline's row from the cell's top.
    pub baseline: u32,
    pub decorations: Decorations,
    pub glyph_count: u32,
}

/// A run of codepoints whose glyphs are consecutive and of one width.
#[derive(Clone, Copy, Debug)]
struct Run {
    first: u32,
    len: u32,
    glyph: u32,
}

/// A run that brought glyphs into the strike: where they lie in its glyph
/// bytes.
#[derive(Clone, Copy, Debug)]
struct Span {
    glyph: u32,
    len: u32,
    wide: bool,
    stored: u64,
}

struct Strike {
    info: StrikeInfo,
    runs: Vec<Run>,
    spans: Vec<Span>,
    /// The bytes of its glyphs as stored.
    stored: u64,
    coverage: Coverage,
    /// The strike's chunks, `[first, end)` of the set's.
    chunks: (usize, usize),
}

/// A chunk's index entry, with where it starts in its strike's glyph
/// bytes.
struct Chunk {
    offset: u64,
    stored: u32,
    decoded: u32,
    deflated: bool,
    strike: usize,
    start: u64,
}

/// Where the chunks' bytes are.
enum Bytes {
    /// Compiled in: a built-in set.
    Static(&'static [u8]),
    /// The whole set, held in memory.
    Owned(Vec<u8>),
}

/// A chunk once read: its bytes in memory, or `None` when it did not
/// decode.
type Loaded = OnceLock<Option<Box<[u8]>>>;

/// A vitrified set: its provenance, strikes, maps and sprite policy, held
/// in memory once checked, and its glyph chunks, each decoded the first
/// time a glyph is drawn from it and kept.
pub struct Set {
    version: (u16, u16),
    meta: String,
    policy: SpritePolicy,
    strikes: Vec<Strike>,
    chunks: Vec<Chunk>,
    bytes: Bytes,
    loaded: Box<[Loaded]>,
    fault: OnceLock<SetError>,
}

impl fmt::Debug for Set {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Set")
            .field("name", &self.name())
            .field("version", &self.version)
            .field("strikes", &self.strikes.len())
            .field("chunks", &self.chunks.len())
            .finish()
    }
}

/// Little-endian fields read from the front of a slice.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SetError> {
        if n > self.0.len() {
            return Err(SetError::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], SetError> {
        self.take(N)?.try_into().map_err(|_| SetError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, SetError> {
        Ok(u8::from_le_bytes(self.array()?))
    }

    fn u16(&mut self) -> Result<u16, SetError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, SetError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, SetError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// A table's count, checked against `bound`, and its record size,
    /// at least `known`: the count and each record as a cursor holding
    /// exactly the fields this version knows.
    fn table(
        &mut self,
        bound: u32,
        bound_name: &'static str,
        known: u32,
    ) -> Result<(u32, usize), SetError> {
        let count = self.u32()?;
        let size = self.u32()?;
        if count > bound {
            return Err(SetError::Bound(bound_name));
        }
        if size < known {
            return Err(SetError::Malformed("a record smaller than its fields"));
        }
        if size > SET_RECORD_BYTES_MAX {
            return Err(SetError::Bound("SET_RECORD_BYTES_MAX"));
        }
        Ok((count, usize_of(size)?))
    }

    /// The next record of `size` bytes, as a cursor over its fields.
    fn record(&mut self, size: usize) -> Result<Cursor<'a>, SetError> {
        Ok(Cursor(self.take(size)?))
    }

    fn end(&self) -> Result<(), SetError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(SetError::Malformed("bytes after a section's last record"))
        }
    }
}

fn usize_of(n: impl TryInto<usize>) -> Result<usize, SetError> {
    n.try_into().map_err(|_| SetError::Bound("usize"))
}

fn add(a: u64, b: u64) -> Result<u64, SetError> {
    a.checked_add(b)
        .ok_or(SetError::Malformed("an offset past u64"))
}

fn mul(a: u64, b: u64) -> Result<u64, SetError> {
    a.checked_mul(b)
        .ok_or(SetError::Malformed("an offset past u64"))
}

/// The header: versions and the section table.
struct Header {
    version: (u16, u16),
    sections: Vec<([u8; 4], u64, u64)>,
}

impl Header {
    /// The header's length from its first 16 bytes, checking the magic
    /// and the major version.
    fn len(prelude: &[u8]) -> Result<usize, SetError> {
        let mut c = Cursor(prelude);
        if c.array::<8>().ok() != Some(MAGIC) {
            return Err(SetError::NotSet);
        }
        let major = c.u16()?;
        if major != MAJOR {
            return Err(SetError::UnknownMajor(major));
        }
        let _minor = c.u16()?;
        let count = c.u32()?;
        if count > SET_SECTIONS_MAX {
            return Err(SetError::Bound("SET_SECTIONS_MAX"));
        }
        usize_of(PRELUDE_BYTES.saturating_add(count.saturating_mul(SECTION_BYTES)))
    }

    /// The header, from bytes that hold it, of a file `file_len` long:
    /// the sections must follow it in order and fill the file.
    fn parse(bytes: &[u8], file_len: u64) -> Result<Header, SetError> {
        let len = Header::len(bytes)?;
        let mut c = Cursor(bytes);
        c.take(8)?;
        let version = (c.u16()?, c.u16()?);
        let count = c.u32()?;
        let mut at = u64::try_from(len).map_err(|_| SetError::Truncated)?;
        let mut sections = Vec::new();
        for _ in 0..count {
            let tag = c.array::<4>()?;
            let (offset, length) = (c.u64()?, c.u64()?);
            if offset != at {
                return Err(SetError::Malformed(
                    "sections not packed in order after the header",
                ));
            }
            at = add(at, length)?;
            if sections.iter().any(|&(t, _, _)| t == tag) {
                return Err(SetError::Malformed("a section listed twice"));
            }
            sections.push((tag, offset, length));
        }
        if at < file_len {
            return Err(SetError::Malformed("bytes after the last section"));
        }
        if at > file_len {
            return Err(SetError::Truncated);
        }
        for tag in [META, STRK, CMAP, POLY, GLYF] {
            if !sections.iter().any(|&(t, _, _)| t == tag) {
                return Err(SetError::Malformed("a required section missing"));
            }
        }
        Ok(Header { version, sections })
    }

    fn section(&self, tag: [u8; 4]) -> Result<(u64, u64), SetError> {
        self.sections
            .iter()
            .find(|&&(t, _, _)| t == tag)
            .map(|&(_, o, l)| (o, l))
            .ok_or(SetError::Malformed("a required section missing"))
    }
}

/// The sections a set is checked from: every one but `GLYF`'s chunk
/// bytes, which stay where [`Bytes`] says.
struct Parts<'a> {
    version: (u16, u16),
    meta: &'a [u8],
    strk: &'a [u8],
    cmap: &'a [u8],
    poly: &'a [u8],
    /// `GLYF`'s head and chunk index.
    index: &'a [u8],
    glyf: (u64, u64),
}

/// The length of `GLYF`'s head and index, from its first 8 bytes.
fn index_len(head: &[u8]) -> Result<u64, SetError> {
    let mut c = Cursor(head);
    let (count, size) = c.table(SET_CHUNKS_MAX, "SET_CHUNKS_MAX", CHUNK_RECORD)?;
    add(
        u64::from(TABLE_HEAD_BYTES),
        mul(
            u64::from(count),
            u64::try_from(size).map_err(|_| SetError::Truncated)?,
        )?,
    )
}

impl Set {
    /// Reads and checks a whole set held in memory: every bound, every
    /// table and the chunk index. Each chunk is decoded when a glyph is
    /// first drawn from it, or by [`Set::check_chunks`].
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Set, SetError> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > u64::from(SET_BYTES_MAX) {
            return Err(SetError::Bound("SET_BYTES_MAX"));
        }
        let mut set = {
            let parts = Set::split(&bytes)?;
            Set::build(&parts)?
        };
        set.bytes = Bytes::Owned(bytes);
        Ok(set)
    }

    /// A built-in set, compiled into the binary.
    #[cfg_attr(not(any(feature = "unifont", feature = "unscii")), allow(dead_code))]
    pub(crate) fn from_static(bytes: &'static [u8]) -> Result<Set, SetError> {
        let parts = Set::split(bytes)?;
        let mut set = Set::build(&parts)?;
        set.bytes = Bytes::Static(bytes);
        Ok(set)
    }

    /// The sections of a whole set in memory.
    fn split(bytes: &[u8]) -> Result<Parts<'_>, SetError> {
        let file_len = u64::try_from(bytes.len()).map_err(|_| SetError::Truncated)?;
        let header = Header::parse(bytes, file_len)?;
        let get = |tag| -> Result<&[u8], SetError> {
            let (offset, len) = header.section(tag)?;
            let start = usize_of(offset)?;
            let end = usize_of(add(offset, len)?)?;
            bytes.get(start..end).ok_or(SetError::Truncated)
        };
        let glyf = get(GLYF)?;
        let index = glyf
            .get(..usize_of(index_len(glyf)?)?)
            .ok_or(SetError::Truncated)?;
        Ok(Parts {
            version: header.version,
            meta: get(META)?,
            strk: get(STRK)?,
            cmap: get(CMAP)?,
            poly: get(POLY)?,
            index,
            glyf: header.section(GLYF)?,
        })
    }

    /// Checks every section and builds the set, its chunks not yet read.
    fn build(parts: &Parts<'_>) -> Result<Set, SetError> {
        if parts.meta.len() > usize_of(SET_META_BYTES_MAX)? {
            return Err(SetError::Bound("SET_META_BYTES_MAX"));
        }
        let meta = parse_meta(parts.meta)?;
        let mut strikes = parse_strikes(parts.strk)?;
        parse_cmap(parts.cmap, &mut strikes)?;
        let policy = parse_poly(parts.poly, &mut strikes)?;
        let chunks = parse_index(parts.index, parts.glyf, &mut strikes)?;
        let loaded = chunks.iter().map(|_| OnceLock::new()).collect();
        Ok(Set {
            version: parts.version,
            meta,
            policy,
            strikes,
            chunks,
            bytes: Bytes::Static(&[]),
            loaded,
            fault: OnceLock::new(),
        })
    }

    /// The set's name, from its provenance.
    pub fn name(&self) -> &str {
        self.meta_values("name").next().unwrap_or_default()
    }

    /// The provenance's `key=value` lines, in order.
    pub fn meta(&self) -> impl Iterator<Item = (&str, &str)> {
        self.meta.lines().filter_map(|l| l.split_once('='))
    }

    /// Every value of `key` in the provenance, in order.
    pub fn meta_values<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.meta().filter(move |(k, _)| *k == key).map(|(_, v)| v)
    }

    /// The format's major and minor version the set was written in.
    pub fn version(&self) -> (u16, u16) {
        self.version
    }

    /// The sprite policy the set was built with.
    pub fn policy(&self) -> SpritePolicy {
        self.policy
    }

    /// Each strike, sorted by cell height, then width, then style, then
    /// format.
    pub fn strikes(&self) -> impl Iterator<Item = StrikeInfo> + '_ {
        self.strikes.iter().map(|s| s.info)
    }

    /// The distinct cell sizes, smallest first.
    pub fn cells(&self) -> Vec<Cell> {
        let mut cells: Vec<Cell> = Vec::new();
        for s in self.regulars() {
            if cells.last() != Some(&s.info.cell) {
                cells.push(s.info.cell);
            }
        }
        cells
    }

    /// How many glyph chunks are held in memory.
    pub fn loaded_chunks(&self) -> usize {
        self.loaded
            .iter()
            .filter(|l| l.get().is_some_and(Option::is_some))
            .count()
    }

    /// The first chunk that did not decode since the set was read, if
    /// any: its glyphs draw as missing ones.
    pub fn fault(&self) -> Option<&SetError> {
        self.fault.get()
    }

    /// Decodes every chunk without keeping it: the whole set's check.
    pub fn check_chunks(&self) -> Result<(), SetError> {
        (0..self.chunks.len()).try_for_each(|i| self.decode(i).map(drop))
    }

    /// Chunk `i` read and inflated.
    fn decode(&self, i: usize) -> Result<Box<[u8]>, SetError> {
        let chunk = self
            .chunks
            .get(i)
            .ok_or(SetError::Malformed("no such chunk"))?;
        let stored = usize_of(chunk.stored)?;
        let read_slice = |all: &[u8]| -> Result<Vec<u8>, SetError> {
            let start = usize_of(chunk.offset)?;
            all.get(start..start.saturating_add(stored))
                .map(<[u8]>::to_vec)
                .ok_or(SetError::Truncated)
        };
        let bytes = match &self.bytes {
            Bytes::Static(all) => read_slice(all)?,
            Bytes::Owned(all) => read_slice(all)?,
        };
        let decoded = usize_of(chunk.decoded)?;
        let bytes = if chunk.deflated {
            miniz_oxide::inflate::decompress_to_vec_with_limit(&bytes, decoded)
                .ok()
                .filter(|d| d.len() == decoded)
                .ok_or(SetError::ChunkLength(i))?
        } else {
            bytes
        };
        Ok(bytes.into_boxed_slice())
    }

    /// Chunk `i` in memory, read the first time it is asked for.
    fn chunk(&self, i: usize) -> Option<&[u8]> {
        self.loaded
            .get(i)?
            .get_or_init(|| match self.decode(i) {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    let _ = self.fault.set(e);
                    None
                }
            })
            .as_deref()
    }

    /// The strike that draws `cp` at `cell` in `style`, and whether bold
    /// is to be drawn by widening: the style's own strike, else for bold
    /// italic the bold one, then the italic one widened, else the regular
    /// one, widened for bold. Of a style's strikes, one a pixel format,
    /// the one that maps `cp` draws it. A strike lacking `cp` passes it
    /// on, so a style drawn from fewer sources than the regular falls back
    /// to the regular glyphs (ADR-1200 D1, `style=`).
    fn strike_for(&self, cell: Cell, style: Style, cp: u32) -> Option<(&Strike, bool)> {
        let find = |bold: bool, italic: bool| {
            self.strikes.iter().find(|k| {
                k.info.cell == cell && k.info.style == Style { bold, italic } && k.run(cp).is_some()
            })
        };
        let Style { bold, italic } = style;
        find(bold, italic)
            .map(|s| (s, false))
            .or_else(|| {
                (bold && italic)
                    .then(|| find(true, false))
                    .flatten()
                    .map(|s| (s, false))
            })
            .or_else(|| {
                italic
                    .then(|| find(false, true))
                    .flatten()
                    .map(|s| (s, bold))
            })
            .or_else(|| find(false, false).map(|s| (s, bold)))
    }

    /// Every regular strike, in order.
    fn regulars(&self) -> impl Iterator<Item = &Strike> {
        self.strikes
            .iter()
            .filter(|k| k.info.style == Style::default())
    }

    /// The regular strikes for `cell`, one a pixel format, their
    /// codepoints apart: 1-bit glyphs from bitmap sources, R8 from outline
    /// fonts, RGBA from color fonts.
    fn regular(&self, cell: Cell) -> impl Iterator<Item = &Strike> {
        self.regulars().filter(move |k| k.info.cell == cell)
    }

    /// Whether the set maps `c` at `cell`.
    pub(crate) fn has(&self, cell: Cell, c: char) -> bool {
        self.regular(cell).any(|s| s.run(u32::from(c)).is_some())
    }

    /// The regular strikes' coverage of `range` at `cell`, summed.
    pub(crate) fn coverage(&self, cell: Cell, range: Range) -> Option<RangeCoverage> {
        self.regular(cell)
            .map(|s| s.coverage.get(range))
            .reduce(|a, b| RangeCoverage {
                covered: a.covered.saturating_add(b.covered).min(a.total),
                total: a.total,
            })
    }

    /// The first regular strike's decorations at `cell`.
    pub(crate) fn decorations(&self, cell: Cell) -> Option<Decorations> {
        Some(self.regular(cell).next()?.info.decorations)
    }

    /// The glyph for `c` at `cell`, drawn at `scale`; `None` when the set
    /// lacks it or its chunk does not decode.
    pub(crate) fn draw(&self, cell: Cell, c: char, style: Style, scale: u32) -> Option<Glyph> {
        let (strike, widen) = self.strike_for(cell, style, u32::from(c))?;
        let index = strike.index(u32::from(c))?;
        self.draw_index(strike, index, widen, scale)
    }

    /// Glyph 0 of the first regular strike at `cell` that holds a glyph,
    /// drawn when the set holds neither U+25A1 nor `?`.
    pub(crate) fn last_resort(&self, cell: Cell, style: Style, scale: u32) -> Option<Glyph> {
        let strike = self.regular(cell).find(|s| s.info.glyph_count > 0)?;
        self.draw_index(strike, 0, style.bold, scale)
    }

    /// Glyph `glyph` of `strike` drawn at `scale`, a 1-bit glyph expanded
    /// to a mask of 0 and 255 (ADR-1200 D13), bold widened when `widen`
    /// asks: a pixel set when it or its left neighbour in the same cell is,
    /// as vtium-font's bitmap faces widen.
    fn draw_index(&self, strike: &Strike, glyph: u32, widen: bool, scale: u32) -> Option<Glyph> {
        let at = strike.locate(&self.chunks, glyph)?;
        let chunk = self.chunk(at.chunk)?;
        let start = usize_of(at.offset).ok()?;
        let info = strike.info;
        let cells = if at.wide { 2 } else { 1 };
        let w = info.cell.width.checked_mul(cells)?;
        let h = info.cell.height;
        let len = usize_of(strike.size(at.wide)?).ok()?;
        let src = chunk.get(start..start.checked_add(len)?)?;
        let row = len.checked_div(usize_of(h).ok()?)?;
        let scale = scale.max(1);
        let out_w = w.checked_mul(scale)?;
        let out_h = h.checked_mul(scale)?;
        let format = match info.format {
            PixelFormat::Rgba => Format::Rgba,
            PixelFormat::Bits | PixelFormat::R8 => Format::Mask,
        };
        // the pixel at `x` of `line` in the strike's format, as mask or RGBA
        // bytes
        let pixel = |line: &[u8], x: u32| -> Option<[u8; 4]> {
            let x = usize_of(x).ok()?;
            match info.format {
                PixelFormat::Bits => {
                    let byte = line.get(x / 8)?;
                    let on = byte >> 7usize.saturating_sub(x % 8) & 1 != 0;
                    Some([if on { 0xff } else { 0 }; 4])
                }
                PixelFormat::R8 => Some([*line.get(x)?; 4]),
                PixelFormat::Rgba => line.get(x.checked_mul(4)?..)?.first_chunk().copied(),
            }
        };
        let bpp = usize_of(format.bytes_per_pixel()).ok()?;
        let mut pixels = Vec::with_capacity(
            usize_of(out_w.saturating_mul(out_h))
                .ok()?
                .saturating_mul(bpp),
        );
        for y in 0..out_h {
            let sy = usize_of(y.checked_div(scale)?).ok()?;
            let line = src.get(sy.checked_mul(row)?..)?.get(..row)?;
            for x in 0..out_w {
                let sx = x.checked_div(scale)?;
                let p = pixel(line, sx)?;
                if format == Format::Mask {
                    let left = (widen && sx.checked_rem(info.cell.width)? != 0)
                        .then(|| pixel(line, sx.checked_sub(1)?))
                        .flatten();
                    pixels.push(left.map_or(p[0], |l| p[0].max(l[0])));
                } else {
                    pixels.extend_from_slice(&p);
                }
            }
        }
        let glyph = Glyph::from_pixels(out_w, out_h, format, pixels)?;
        Some(glyph.with_cells(if at.wide { Width::Wide } else { Width::Narrow }))
    }
}

/// Where a glyph lies: its chunk and its offset in the chunk.
struct Located {
    chunk: usize,
    offset: u64,
    wide: bool,
}

impl Strike {
    /// The run holding codepoint `cp`.
    fn run(&self, cp: u32) -> Option<&Run> {
        let i = self
            .runs
            .partition_point(|r| r.first <= cp)
            .checked_sub(1)?;
        self.runs
            .get(i)
            .filter(|r| cp.wrapping_sub(r.first) < r.len)
    }

    /// The glyph codepoint `cp` draws.
    fn index(&self, cp: u32) -> Option<u32> {
        let r = self.run(cp)?;
        r.glyph.checked_add(cp.checked_sub(r.first)?)
    }

    /// Where glyph `glyph` lies among `chunks`, the set's.
    fn locate(&self, chunks: &[Chunk], glyph: u32) -> Option<Located> {
        let i = self
            .spans
            .partition_point(|s| s.glyph <= glyph)
            .checked_sub(1)?;
        let span = self.spans.get(i)?;
        let k = u64::from(glyph.checked_sub(span.glyph)?);
        if k >= u64::from(span.len) {
            return None;
        }
        let size = self.size(span.wide)?;
        let at = span.stored.checked_add(k.checked_mul(size)?)?;
        let (first, end) = self.chunks;
        let mine = chunks.get(first..end)?;
        let c = mine.partition_point(|c| c.start <= at).checked_sub(1)?;
        let chunk = mine.get(c)?;
        Some(Located {
            chunk: first.checked_add(c)?,
            offset: at.checked_sub(chunk.start)?,
            wide: span.wide,
        })
    }

    /// A glyph's bytes as stored, one cell or two wide.
    fn size(&self, wide: bool) -> Option<u64> {
        let info = self.info;
        let cells = if wide { 2 } else { 1 };
        glyph_bytes(info.format.code(), info.cell.width, info.cell.height, cells).map(u64::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::{SET_RECORD_BYTES_MAX, SET_RUNS_MAX, SET_STRIKES_MAX};

    // The map a set can make the reader hold: 36 bytes a run, a run and
    // the glyph span it may bring in, so at most 9 MiB at SET_RUNS_MAX, and
    // CMAP's bytes while they are read, at most 16,777,352 (the crate's
    // README, Bounds)
    #[test]
    fn the_map_stays_within_its_figures() {
        let per_run = size_of::<Run>() + size_of::<Span>();
        assert_eq!(per_run, 36);
        let runs = usize::try_from(SET_RUNS_MAX).unwrap_or(usize::MAX);
        assert_eq!(per_run * runs, 9 << 20);
        let cmap = 8 + 4 * SET_STRIKES_MAX + SET_RUNS_MAX * SET_RECORD_BYTES_MAX;
        assert_eq!(cmap, 16_777_352);
    }
}
