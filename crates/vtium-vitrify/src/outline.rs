//! Outline fonts (ADR-1200 D1, D13, D14): TrueType and OpenType fonts and
//! their collections, rasterised by swash in the parser's child at each
//! cell a set is built at, into R8 coverage for outlines and premultiplied
//! RGBA for color glyphs (COLR and CBDT).
//!
//! The parent sends an [`OutlineRequest`]: the font's bytes, the family to
//! take from a collection, the cells and the codepoints wanted. The child
//! answers with an [`Outline`], every glyph already placed in its cell,
//! which the parent checks field by field before it trusts any:
//!
//! ```text
//! request  magic "VTVOUT3\0", font length u32, the font, family length u32,
//!          the family, cell count u32, each cell's width u32 (0: the font
//!          sets it) and height u32, range count u32, each range's first
//!          and last codepoint u32, cluster count u32, each cluster's kind
//!          u8, length u8 and scalar values u32, baseline count u32 (0, or
//!          the cell count), each baseline u32
//! answer   magic "VTVOUTL3", family length u32, the family, format length
//!          u32, the format, licence length u32, the licence, strike count
//!          u32, then each strike: cell width u32, cell height u32, baseline
//!          u32, glyph count u32, map length u32, cluster count u32, each
//!          glyph's flags u8 (1 two cells wide, 2 RGBA, 4 an ink box) and
//!          its rows, cut to its cells, then for an ink box its left i32,
//!          width u32 and R8 rows, then
//!          the map: each (codepoint u32, glyph u32), sorted, then the
//!          clusters: each length u8, scalar values u32 and glyph u32,
//!          sorted
//! ```
//!
//! Little-endian throughout.
//!
//! Each cell's font size follows the C reference's flow (`font_freetype.c`,
//! a cell request for half a pixel under the height): the font's ascent
//! and descent span the cell's height less half a pixel, the baseline sits
//! the ascent below the top, rounded up, and rises by what the lowest ink
//! of the font's printable ASCII passes the cell's bottom (ADR-1302 D3),
//! and the font that sets the cell sets its width
//! from `W`'s advance or ink, whichever is wider. A glyph is placed by its
//! font (ADR-1302 D2): its pen at the cell's left edge, moved by half of
//! what its cells exceed its advance rounded, rounded down, and its ink at
//! its bearing from the pen on the baseline; its pixels are cut at its
//! cells' edges, and the ink past them goes to its ink box, cut one cell
//! past its cells. A glyph advancing more
//! than one and a half cells is two cells wide. A color glyph larger than
//! its cells is drawn again at the size that fits them, centred across
//! them and kept inside the cell's height. Outlines are hinted; color
//! glyphs are not, and a COLR glyph's layers are laid over each other here,
//! premultiplied with the renderer's rounding.
//!
//! The clusters asked for ([`crate::shaping`]) are shaped once with
//! HarfRust and drawn at each cell: each glyph the shaper gives at its
//! position, laid over the others, and the whole placed as one glyph is.

use std::sync::Arc;

use crate::frit::Cover;
use crate::shaping::{Ask, Kind};
use vtium_font::{CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, SET_CHUNK_BYTES};

/// The most bytes an outline font file holds; Noto Sans CJK's collection
/// is about 20 MB.
pub const OUTLINE_BYTES_MAX: u64 = 64 << 20;
/// The most bytes an outline answer holds: a strike of GB 2312's 7,445
/// ideographs at 48 pixels is about 34 MB.
pub const OUTLINE_ANSWER_BYTES_MAX: usize = 128 << 20;
/// The most cells one request draws at: a frit's `size=` lines.
pub const OUTLINE_CELLS_MAX: usize = 3;
/// The most ranges one request names: a frit's `cover=` lines.
const RANGES_MAX: usize = 4096;
/// The most bytes a family's or a format's name takes.
const NAME_MAX: usize = 256;
/// The most bytes of a font's licence description kept.
const LICENSE_MAX: usize = 1024;
/// The most codepoints a strike's map names: every Unicode codepoint.
const MAP_MAX: usize = 0x11_0000;
/// The most glyphs a strike holds: a font's glyph ids are 16 bits.
const GLYPHS_MAX: usize = 65536;
/// The most clusters one request asks for: every cluster
/// [`crate::shaping::asks`] lists is under half of it.
pub const CLUSTERS_MAX: usize = 1 << 14;

pub(crate) const REQUEST_MAGIC: &[u8; 8] = b"VTVOUT3\0";
const ANSWER_MAGIC: &[u8; 8] = b"VTVOUTL3";

const GLYPH_WIDE: u8 = 1;
const GLYPH_COLOR: u8 = 2;
/// The glyph's ink box follows its pixels: its left i32, its width u32
/// and its R8 pixels.
const GLYPH_INK: u8 = 4;

/// What the parser is given to rasterise an outline font.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutlineRequest {
    /// Shared, so the parent holds one copy of a font however many
    /// requests and pipes carry it.
    pub font: Arc<[u8]>,
    /// The family to take: in a collection, its first face of that family;
    /// a single font must be of it. Empty takes the first face.
    pub family: String,
    /// Each cell to draw at, width and height; a width of 0 lets the font
    /// set the cell's width, as the set's first source does.
    pub cells: Vec<(u32, u32)>,
    /// The codepoints to draw, as ranges; empty for every one the font
    /// maps.
    pub cover: Vec<Cover>,
    /// The clusters to shape and draw whole.
    pub clusters: Vec<Ask>,
    /// The baseline to draw each cell's glyphs on, one for each cell, as
    /// [`baseline`] gives it and the helper keeps it for the source and the
    /// cell's height; empty, the rasteriser finds each itself.
    pub baselines: Vec<u32>,
}

/// One glyph placed in its cells: R8 coverage or premultiplied RGBA, row
/// by row, cut to its cells, and the ink box of an R8 glyph whose ink
/// passes them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Placed {
    pub wide: bool,
    pub color: bool,
    pub pixels: Vec<u8>,
    pub ink: Option<InkBox>,
}

/// The columns an R8 glyph's ink covers past its cells (ADR-1302 D1,
/// D6): `width` columns from `left`, the offset of the box's first column
/// from the left edge of the glyph's first cell, 0 or negative, over the
/// cell's height, R8. The box spans the glyph's cells and its ink, cut to
/// its reach, one cell past its cells on each side.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InkBox {
    pub left: i32,
    pub width: u32,
    pub pixels: Vec<u8>,
}

/// Whether a box `width` columns from `left` spans a glyph `span` pixels
/// wide in cells `cell` pixels wide, passes it on one side at least, and
/// reaches no further than one cell past it on either side: the rule
/// vtium-font's reader holds a set's boxes to.
pub fn in_reach(left: i32, width: u32, cell: u32, span: u32) -> bool {
    let (left, cell, span) = (i64::from(left), i64::from(cell), i64::from(span));
    let right = left.saturating_add(i64::from(width));
    left <= 0
        && left.saturating_add(cell) >= 0
        && right >= span
        && right <= span.saturating_add(cell)
        && (left, right) != (0, span)
}

/// An outline font's glyphs at one cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strike {
    pub width: u32,
    pub height: u32,
    /// The baseline's row from the cell's top.
    pub baseline: u32,
    pub glyphs: Vec<Placed>,
    /// Each codepoint and its glyph, sorted by codepoint.
    pub map: Vec<(u32, u32)>,
    /// Each cluster the font shaped as asked, and its glyph, sorted by
    /// cluster.
    pub clusters: Vec<(Vec<u32>, u32)>,
}

/// The parser's answer: the face it took and its glyphs at each cell
/// asked for, in the order asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outline {
    /// The face's family, as its name table gives it.
    pub family: String,
    /// What was read: `outline`, or `outline+color` with color glyphs.
    pub format: String,
    /// The licence the face's name table describes, its blanks run
    /// together into single spaces; empty when it gives none.
    pub license: String,
    pub strikes: Vec<Strike>,
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn put_key(out: &mut Vec<u8>, key: &[u32]) {
    out.push(u8::try_from(key.len()).unwrap_or(u8::MAX));
    for &c in key {
        put32(out, c);
    }
}

/// Little-endian fields read from the front of a slice.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        if n > self.0.len() {
            return Err("cut short");
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, &'static str> {
        self.take(1)?.first().copied().ok_or("cut short")
    }

    fn u32(&mut self) -> Result<u32, &'static str> {
        let b = self.take(4)?;
        let a: [u8; 4] = b.try_into().map_err(|_| "cut short")?;
        Ok(u32::from_le_bytes(a))
    }

    /// A cluster's key: a length u8 of 1 to [`CLUSTER_SCALARS_MAX`], then
    /// that many scalar values.
    fn key(&mut self) -> Result<Vec<u32>, &'static str> {
        let n = self.u8()?;
        if n == 0 || u32::from(n) > CLUSTER_SCALARS_MAX {
            return Err("a cluster empty or past CLUSTER_SCALARS_MAX");
        }
        (0..n)
            .map(|_| {
                let c = self.u32()?;
                char::from_u32(c)
                    .map(|_| c)
                    .ok_or("a cluster holding a codepoint that is not a character")
            })
            .collect()
    }

    fn count(&mut self, max: usize) -> Result<usize, &'static str> {
        let n = usize::try_from(self.u32()?).map_err(|_| "a count past usize")?;
        if n > max {
            return Err("a count past its bound");
        }
        Ok(n)
    }

    /// A name: UTF-8 of at most `max` bytes, no control character.
    fn name(&mut self, max: usize) -> Result<String, &'static str> {
        let n = self.count(max)?;
        std::str::from_utf8(self.take(n)?)
            .ok()
            .filter(|s| !s.chars().any(char::is_control))
            .map(str::to_owned)
            .ok_or("a name that is not UTF-8 text")
    }
}

impl OutlineRequest {
    pub fn encode(&self) -> Vec<u8> {
        self.pieces()
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect()
    }

    /// The request as written to the parser: its head, the font itself,
    /// not copied, and the rest.
    pub(crate) fn pieces(&self) -> [Arc<[u8]>; 3] {
        let mut head = REQUEST_MAGIC.to_vec();
        put32(&mut head, len32(self.font.len()));
        let mut out = Vec::new();
        put32(&mut out, len32(self.family.len()));
        out.extend_from_slice(self.family.as_bytes());
        put32(&mut out, len32(self.cells.len()));
        for &(w, h) in &self.cells {
            put32(&mut out, w);
            put32(&mut out, h);
        }
        put32(&mut out, len32(self.cover.len()));
        for &(a, b) in &self.cover {
            put32(&mut out, a);
            put32(&mut out, b);
        }
        put32(&mut out, len32(self.clusters.len()));
        for ask in &self.clusters {
            out.push(ask.kind.code());
            put_key(&mut out, &ask.key);
        }
        put32(&mut out, len32(self.baselines.len()));
        for &b in &self.baselines {
            put32(&mut out, b);
        }
        [head.into(), Arc::clone(&self.font), out.into()]
    }

    pub fn decode(bytes: &[u8]) -> Result<OutlineRequest, &'static str> {
        let mut r = Reader(bytes);
        if r.take(8)? != REQUEST_MAGIC {
            return Err("not an outline request");
        }
        let n = r.count(usize::try_from(OUTLINE_BYTES_MAX).unwrap_or(usize::MAX))?;
        let font = Arc::from(r.take(n)?);
        let family = r.name(NAME_MAX)?;
        let mut cells = Vec::new();
        for _ in 0..r.count(OUTLINE_CELLS_MAX)? {
            let (w, h) = (r.u32()?, r.u32()?);
            if w > CELL_SIDE_MAX || !(1..=CELL_SIDE_MAX).contains(&h) {
                return Err("a cell past CELL_SIDE_MAX");
            }
            cells.push((w, h));
        }
        let mut cover = Vec::new();
        for _ in 0..r.count(RANGES_MAX)? {
            cover.push((r.u32()?, r.u32()?));
        }
        let mut clusters = Vec::new();
        for _ in 0..r.count(CLUSTERS_MAX)? {
            let kind = Kind::from_code(r.u8()?).ok_or("an unknown cluster kind")?;
            clusters.push(Ask {
                key: r.key()?,
                kind,
            });
        }
        let mut baselines = Vec::new();
        for _ in 0..r.count(OUTLINE_CELLS_MAX)? {
            baselines.push(r.u32()?);
        }
        if !baselines.is_empty()
            && (baselines.len() != cells.len()
                || baselines.iter().zip(&cells).any(|(&b, &(_, h))| b > h))
        {
            return Err("a baseline for no cell, or below its cell");
        }
        if !r.0.is_empty() {
            return Err("bytes after the request");
        }
        Ok(OutlineRequest {
            font,
            family,
            cells,
            cover,
            clusters,
            baselines,
        })
    }

    fn covers(&self, c: u32) -> bool {
        self.cover.is_empty() || self.cover.iter().any(|&(a, b)| (a..=b).contains(&c))
    }
}

impl Placed {
    /// The bytes a glyph of `cells` cells takes at `width` by `height`.
    fn bytes(wide: bool, color: bool, width: u32, height: u32) -> Option<usize> {
        let n = width
            .checked_mul(if wide { 2 } else { 1 })?
            .checked_mul(height)?
            .checked_mul(if color { 4 } else { 1 })?;
        usize::try_from(n).ok()
    }
}

impl Outline {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = ANSWER_MAGIC.to_vec();
        for name in [&self.family, &self.format, &self.license] {
            put32(&mut out, len32(name.len()));
            out.extend_from_slice(name.as_bytes());
        }
        put32(&mut out, len32(self.strikes.len()));
        for s in &self.strikes {
            for v in [
                s.width,
                s.height,
                s.baseline,
                len32(s.glyphs.len()),
                len32(s.map.len()),
                len32(s.clusters.len()),
            ] {
                put32(&mut out, v);
            }
            for g in &s.glyphs {
                let flags = if g.wide { GLYPH_WIDE } else { 0 }
                    | if g.color { GLYPH_COLOR } else { 0 }
                    | if g.ink.is_some() { GLYPH_INK } else { 0 };
                out.push(flags);
                out.extend_from_slice(&g.pixels);
                if let Some(ink) = &g.ink {
                    out.extend_from_slice(&ink.left.to_le_bytes());
                    put32(&mut out, ink.width);
                    out.extend_from_slice(&ink.pixels);
                }
            }
            for &(c, g) in &s.map {
                put32(&mut out, c);
                put32(&mut out, g);
            }
            for (key, g) in &s.clusters {
                put_key(&mut out, key);
                put32(&mut out, *g);
            }
        }
        out
    }

    /// Reads and checks an answer to `request`: a strike for each cell
    /// asked for, of its height and, where asked, its width, each inside
    /// [`CELL_SIDE_MAX`] with its baseline inside it; every glyph exactly
    /// as long as its flags say and no larger than a set's chunk, RGBA
    /// premultiplied; every codepoint a scalar value in order, one the
    /// request covers, and every glyph one the strike holds; every cluster
    /// one the request asked for, in order, its glyph one the strike holds;
    /// nothing after the last strike.
    pub fn decode(bytes: &[u8], request: &OutlineRequest) -> Result<Outline, &'static str> {
        if bytes.len() > OUTLINE_ANSWER_BYTES_MAX {
            return Err("larger than OUTLINE_ANSWER_BYTES_MAX");
        }
        let mut r = Reader(bytes);
        if r.take(8)? != ANSWER_MAGIC {
            return Err("not an outline answer");
        }
        let family = r.name(NAME_MAX)?;
        let format = r.name(NAME_MAX)?;
        let license = r.name(LICENSE_MAX)?;
        let count = r.count(OUTLINE_CELLS_MAX)?;
        if count != request.cells.len() {
            return Err("not a strike for each cell asked for");
        }
        let mut asked: Vec<&[u32]> = request.clusters.iter().map(|a| a.key.as_slice()).collect();
        asked.sort_unstable();
        let mut strikes = Vec::with_capacity(count);
        for &(want_w, want_h) in &request.cells {
            let (width, height, baseline) = (r.u32()?, r.u32()?, r.u32()?);
            if height != want_h || (want_w != 0 && width != want_w) {
                return Err("a strike at a cell not asked for");
            }
            if !(1..=CELL_SIDE_MAX).contains(&width) || baseline > height {
                return Err("a cell past CELL_SIDE_MAX, or a baseline below it");
            }
            let glyph_count = r.count(usize::MAX)?;
            let map_len = r.count(MAP_MAX)?;
            let cluster_count = r.count(usize::MAX)?;
            if cluster_count > asked.len() {
                return Err("more clusters than the request asked for");
            }
            // every glyph is one the map or a cluster names, so a strike
            // holds no more glyphs than those, nor than a set's strike holds
            if glyph_count > map_len.saturating_add(cluster_count).min(GLYPHS_MAX) {
                return Err("more glyphs than the map and clusters name, or than GLYPHS_MAX");
            }
            let mut glyphs = Vec::new();
            for _ in 0..glyph_count {
                let flags = r.u8()?;
                if flags & !(GLYPH_WIDE | GLYPH_COLOR | GLYPH_INK) != 0 {
                    return Err("an unknown glyph flag");
                }
                let (wide, color) = (flags & GLYPH_WIDE != 0, flags & GLYPH_COLOR != 0);
                let n = Placed::bytes(wide, color, width, height).ok_or("a glyph past usize")?;
                if n > usize::try_from(SET_CHUNK_BYTES).unwrap_or(0) {
                    return Err("a glyph larger than a set's chunk");
                }
                let pixels = r.take(n)?.to_vec();
                if color
                    && pixels.chunks_exact(4).any(|p| match p {
                        &[cr, cg, cb, a] => cr > a || cg > a || cb > a,
                        _ => true,
                    })
                {
                    return Err("an RGBA glyph that is not premultiplied");
                }
                let ink = if flags & GLYPH_INK == 0 {
                    None
                } else {
                    if color {
                        return Err("an ink box on a color glyph");
                    }
                    let left = i32::from_le_bytes(r.take(4)?.try_into().map_err(|_| "cut short")?);
                    let box_width = r.u32()?;
                    let span = if wide { width.saturating_mul(2) } else { width };
                    if !in_reach(left, box_width, width, span) {
                        return Err("an ink box past its glyph's reach, or not past its cells");
                    }
                    let n = Placed::bytes(false, false, box_width, height)
                        .ok_or("an ink box past usize")?;
                    if n > usize::try_from(SET_CHUNK_BYTES).unwrap_or(0) {
                        return Err("an ink box larger than a set's chunk");
                    }
                    Some(InkBox {
                        left,
                        width: box_width,
                        pixels: r.take(n)?.to_vec(),
                    })
                };
                glyphs.push(Placed {
                    wide,
                    color,
                    pixels,
                    ink,
                });
            }
            if map_len > r.0.len() / 8 {
                return Err("an answer cut short: its map is longer than the bytes left");
            }
            let mut map = Vec::with_capacity(map_len);
            for _ in 0..map_len {
                let (c, g) = (r.u32()?, r.u32()?);
                if char::from_u32(c).is_none() {
                    return Err("a codepoint that is not a Unicode scalar value");
                }
                if !request.covers(c) {
                    return Err("a codepoint the request does not cover");
                }
                if usize::try_from(g).map_or(true, |g| g >= glyphs.len()) {
                    return Err("a glyph past the count");
                }
                if map.last().is_some_and(|&(p, _)| p >= c) {
                    return Err("the map is not sorted, or names a codepoint twice");
                }
                map.push((c, g));
            }
            // a cluster's length, one scalar value and its glyph at least
            if cluster_count > r.0.len() / 9 {
                return Err("an answer cut short: its clusters are longer than the bytes left");
            }
            let mut clusters: Vec<(Vec<u32>, u32)> = Vec::with_capacity(cluster_count);
            for _ in 0..cluster_count {
                let key = r.key()?;
                if asked.binary_search(&key.as_slice()).is_err() {
                    return Err("a cluster the request did not ask for");
                }
                if clusters.last().is_some_and(|(p, _)| *p >= key) {
                    return Err("the clusters are not sorted, or name one twice");
                }
                let g = r.u32()?;
                if usize::try_from(g).map_or(true, |g| g >= glyphs.len()) {
                    return Err("a cluster's glyph past the count");
                }
                clusters.push((key, g));
            }
            strikes.push(Strike {
                width,
                height,
                baseline,
                glyphs,
                map,
                clusters,
            });
        }
        if !r.0.is_empty() {
            return Err("bytes after the last strike");
        }
        Ok(Outline {
            family,
            format,
            license,
            strikes,
        })
    }
}

/// Lowercase with blanks, `-` and `_` dropped: a family as fontconfig
/// compares it, and as a font's file name spells it.
pub fn family_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

pub use raster::{baseline, cell_width, decorations, rasterise};

/// The child's side: swash, on bytes no one has checked. A panic here ends
/// the child, which fails the source and nothing else (ADR-1200 D14).
pub(crate) mod raster {
    use super::{
        InkBox, LICENSE_MAX, Outline, OutlineRequest, Placed, SET_CHUNK_BYTES, Strike, family_key,
    };
    use swash::scale::image::{Content, Image};
    use swash::scale::{Render, ScaleContext, Source, StrikeWith};
    use swash::zeno::{Mask, Origin, Placement};

    use crate::resample::{self, METHOD};
    use swash::{FontDataRef, FontRef, StringId};

    /// The face `family` names in `data`, or its first face for an empty
    /// family; with the family its name table gives and its index in a
    /// collection.
    fn face<'a>(data: &'a [u8], family: &str) -> Result<(FontRef<'a>, String, u32), String> {
        let fonts = FontDataRef::new(data).ok_or("not a TrueType or OpenType font")?;
        let want = family_key(family);
        let mut first = None;
        for (index, font) in (0u32..).zip(fonts.fonts()) {
            let strings = font.localized_strings();
            let names: Vec<String> = [StringId::TypographicFamily, StringId::Family]
                .into_iter()
                .filter_map(|id| strings.find_by_id(id, Some("en")))
                .map(|s| s.to_string())
                .collect();
            let name = names.first().cloned().unwrap_or_default();
            if want.is_empty() || names.iter().any(|n| family_key(n) == want) {
                return Ok((font, name, index));
            }
            first.get_or_insert(name);
        }
        Err(match first {
            Some(found) => format!("the font is of family {found}, not {family}"),
            None => "the font holds no face".to_owned(),
        })
    }

    /// The pixels of `image` from row `y`, column `x` of its own: one byte
    /// a pixel for a mask, four premultiplied for color.
    pub(crate) fn pixel(image: &Image, x: u32, y: u32, color: bool) -> [u8; 4] {
        let w = image.placement.width as usize;
        let (x, y) = (x as usize, y as usize);
        if color {
            let at = (y * w + x) * 4;
            match image.data.get(at..at + 4) {
                Some(&[r, g, b, a]) => [r.min(a), g.min(a), b.min(a), a],
                _ => [0; 4],
            }
        } else {
            let a = image.data.get(y * w + x).copied().unwrap_or(0);
            [a; 4]
        }
    }

    /// Copies `image` into a `bw` by `bh` box at `left`, `top` of the box,
    /// cut at its edges.
    fn copy(image: &Image, color: bool, bw: u32, bh: u32, left: i32, top: i32) -> Vec<u8> {
        let bpp = if color { 4 } else { 1 };
        let mut out = vec![0u8; (bw * bh) as usize * bpp];
        for y in 0..image.placement.height {
            let ty = top + y as i32;
            if ty < 0 || ty >= bh as i32 {
                continue;
            }
            for x in 0..image.placement.width {
                let tx = left + x as i32;
                if tx < 0 || tx >= bw as i32 {
                    continue;
                }
                let at = (ty as usize * bw as usize + tx as usize) * bpp;
                let p = pixel(image, x, y, color);
                if let (Some(dst), Some(src)) = (out.get_mut(at..at + bpp), p.get(..bpp)) {
                    dst.copy_from_slice(src);
                }
            }
        }
        out
    }

    /// The color a COLR layer takes when it names the text's color, which
    /// a set cannot know: swash's own default, a mid grey.
    pub(crate) const FOREGROUND: [u8; 4] = [128, 128, 128, 255];

    /// `t / 255` rounded, for `t` up to 255 * 255.
    pub(crate) fn div255(t: u32) -> u32 {
        (t + 127) / 255
    }

    /// Draws one face's glyphs: color glyphs unhinted, as COLR layers or
    /// bitmaps, and outlines hinted.
    pub(crate) struct Drawer<'a> {
        pub(crate) font: FontRef<'a>,
        context: ScaleContext,
    }

    impl Drawer<'_> {
        /// Glyph `g` at `ppem`: its color form if it has one, else its
        /// outline as coverage. A color bitmap is scaled from its largest
        /// strike to `ppem`, and further to fit `fit`, by this build's
        /// [`METHOD`].
        pub(crate) fn draw(&mut self, g: u16, ppem: f32, fit: (u32, u32)) -> Option<Image> {
            self.draw_stretched(g, ppem, fit, 1.0)
        }

        /// [`Drawer::draw`], an outline scaled sideways by `sx` about its
        /// origin before it is rasterised (ADR-1302 D2's joining stretch);
        /// a color glyph is drawn as it is.
        pub(crate) fn draw_stretched(
            &mut self,
            g: u16,
            ppem: f32,
            fit: (u32, u32),
            sx: f32,
        ) -> Option<Image> {
            let mut scaler = self.context.builder(self.font).size(ppem).build();
            if scaler.has_color_outlines() {
                if let Some(layers) = scaler.scale_color_outline(g) {
                    return Some(self.compose(&layers));
                }
            }
            if scaler.has_color_bitmaps() {
                if let Some(image) = self.bitmap(g, ppem, fit) {
                    return Some(image);
                }
            }
            let mut scaler = self
                .context
                .builder(self.font)
                .size(ppem)
                .hint(true)
                .build();
            let stretch = (sx != 1.0).then(|| swash::zeno::Transform::scale(sx, 1.0));
            Render::new(&[Source::Outline])
                .transform(stretch)
                .render(&mut scaler, g)
        }

        /// A color bitmap from the font's largest strike holding `g`, its
        /// colors premultiplied, scaled to `ppem` or as far under as fits
        /// `fit`, by [`METHOD`] (ADR-1200 D13; the operator's choice on
        /// vtium/vtium#23).
        fn bitmap(&mut self, g: u16, ppem: f32, fit: (u32, u32)) -> Option<Image> {
            let strike = self.font.color_strikes().find_by_largest_ppem(g)?.ppem();
            // a scaler of no size draws the strike as it is stored
            let mut native = self.context.builder(self.font).build();
            let image = native.scale_color_bitmap(g, StrikeWith::LargestSize)?;
            let (w, h) = (image.placement.width, image.placement.height);
            if strike == 0 || w == 0 || h == 0 {
                return None;
            }
            let premultiplied = image
                .data
                .chunks_exact(4)
                .flat_map(|p| match p {
                    &[r, g, b, a] => {
                        let m = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
                        [m(r), m(g), m(b), a]
                    }
                    _ => [0; 4],
                })
                .collect();
            let src = resample::Image {
                width: w,
                height: h,
                pixels: premultiplied,
            };
            let s = (f64::from(ppem) / f64::from(strike))
                .min(f64::from(fit.0) / f64::from(w))
                .min(f64::from(fit.1) / f64::from(h));
            let size = |n: u32| ((f64::from(n) * s).round() as u32).max(1);
            let out = resample::resize(&src, size(w), size(h), METHOD);
            Some(Image {
                source: Source::ColorBitmap(StrikeWith::LargestSize),
                content: Content::Color,
                placement: Placement {
                    left: (f64::from(image.placement.left) * s).round() as i32,
                    top: (f64::from(image.placement.top) * s).round() as i32,
                    width: out.width,
                    height: out.height,
                },
                data: out.pixels,
            })
        }

        /// A COLR glyph's layers, each filled in its palette color and
        /// laid over those below it, premultiplied, with exact rounding.
        fn compose(&self, layers: &swash::scale::outline::Outline) -> Image {
            let palette = self.font.color_palettes().next();
            let b = layers.bounds();
            let (x0, y1) = (b.min.x.floor() as i32, b.max.y.ceil() as i32);
            let w = (b.max.x.ceil() as i32 - x0).max(0) as u32;
            let h = (y1 - b.min.y.floor() as i32).max(0) as u32;
            let mut data = vec![0u8; (w * h * 4) as usize];
            for i in 0..layers.len() {
                let Some(layer) = layers.get(i) else { continue };
                let [r, g, b, a] = layer
                    .color_index()
                    .and_then(|c| palette.map(|p| p.get(c)))
                    .unwrap_or(FOREGROUND);
                let mut mask = Mask::new(layer.path());
                mask.origin(Origin::BottomLeft).inspect(|_, _, _| {});
                let (cover, at) = mask.render();
                for (row, line) in cover.chunks(at.width.max(1) as usize).enumerate() {
                    let y = y1 - at.top + row as i32;
                    for (col, &c) in line.iter().enumerate() {
                        let x = at.left - x0 + col as i32;
                        if c == 0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                            continue;
                        }
                        let i = ((y as u32 * w + x as u32) * 4) as usize;
                        let Some(dst) = data.get_mut(i..i + 4) else {
                            continue;
                        };
                        let alpha = div255(u32::from(c) * u32::from(a));
                        for (d, s) in dst.iter_mut().zip([r, g, b, 255]) {
                            let src = div255(u32::from(s) * alpha);
                            *d = (src + div255(u32::from(*d) * (255 - alpha))) as u8;
                        }
                    }
                }
            }
            Image {
                source: Source::ColorOutline(0),
                content: Content::Color,
                placement: Placement {
                    left: x0,
                    top: y1,
                    width: w,
                    height: h,
                },
                data,
            }
        }
    }

    /// The font's metrics, refused when they give no size to draw at.
    fn metrics(font: FontRef<'_>) -> Result<swash::Metrics, String> {
        let metrics = font.metrics(&[]);
        let span = metrics.ascent + metrics.descent.abs();
        if metrics.units_per_em == 0 || span <= 0.0 {
            return Err("the font gives no ascent, descent or units per em".to_owned());
        }
        Ok(metrics)
    }

    /// The size a face is drawn at in a cell `h` pixels high, in pixels to
    /// the em: its ascent and descent span the height less half a pixel,
    /// as the C reference asked FreeType for a cell half a pixel under the
    /// height (`request_size`).
    fn ppem(metrics: &swash::Metrics, h: u32) -> f32 {
        let span = metrics.ascent + metrics.descent.abs();
        (h as f32 - 0.5) * f32::from(metrics.units_per_em) / span
    }

    /// The W rule (ADR-1200 D17): the cell's width is the wider of `W`'s
    /// advance, rounded, and the columns its ink covers at `ppem`, as the
    /// C reference's `font_get_width` took it; a face with no `W` takes
    /// `M`'s, then `0`'s, and a face with none of them half the height
    /// `h`. The ink is measured on the outline as it is, not hinted: the
    /// C reference loaded it with FreeType's light hinting, which in a
    /// TrueType font moves only rows, so its ink kept the outline's width
    /// across. DejaVu Sans
    /// Mono's `W` at 16 pixels spans 8.02 pixels, 9 columns, where full
    /// hinting snaps it to 8. A glyph with no outline, a color or bitmap
    /// one, is measured as it is drawn.
    fn w_width(drawer: &mut Drawer<'_>, ppem: f32, h: u32) -> u32 {
        let font = drawer.font;
        let (charmap, advances) = (font.charmap(), font.glyph_metrics(&[]).scale(ppem));
        ['W', 'M', '0']
            .into_iter()
            .map(|c| charmap.map(c))
            .find(|&g| g != 0)
            .map(|g| {
                let mut scaler = drawer.context.builder(font).size(ppem).build();
                let ink = Render::new(&[Source::Outline])
                    .render(&mut scaler, g)
                    .or_else(|| drawer.draw(g, ppem, (u32::MAX, u32::MAX)))
                    .map_or(0, |i| i.placement.width);
                // vtium rounds the advance once, from a float; the C
                // reference took FreeType's advance, already rounded in 26.6
                // fixed point. They can differ when the fraction is near a
                // half, as DejaVu Sans Mono's W at 15 pixels advances
                // 7.4995, which vtium rounds to 7 and FreeType to 8. That
                // changes the cell only for a font whose W ink is narrower
                // than its advance; DejaVu's covers 8 columns either way
                (advances.advance_width(g).round() as u32).max(ink)
            })
            .unwrap_or(h.div_ceil(2))
            .clamp(1, vtium_font::CELL_SIDE_MAX)
    }

    /// The width of an outline font's own cell `height` pixels high
    /// (ADR-1200 D17): the W rule's over each of `faces`, each a font's
    /// bytes and the family to take from it, the widest of them, as the C
    /// reference took the larger of its regular and bold faces' cells
    /// (`kmscon_font_freetype_init`). A face that cannot be read fails the
    /// whole answer, as the C reference failed its font when its bold face
    /// did not load.
    pub fn cell_width(faces: &[(&[u8], &str)], height: u32) -> Result<u32, String> {
        if !(1..=vtium_font::CELL_SIDE_MAX).contains(&height) {
            return Err(format!("a cell {height} pixels high, past CELL_SIDE_MAX"));
        }
        let mut widest = None;
        for &(data, family) in faces {
            let (font, _, _) = face(data, family)?;
            let ppem = ppem(&metrics(font)?, height);
            let mut drawer = Drawer {
                font,
                context: ScaleContext::new(),
            };
            let width = w_width(&mut drawer, ppem, height);
            widest = Some(widest.map_or(width, |w: u32| w.max(width)));
        }
        widest.ok_or_else(|| "no face to take the cell from".to_owned())
    }

    /// The baseline of a face in a cell `h` pixels high, drawn at `ppem`
    /// (ADR-1302 D3): its ascent below the cell's top, rounded up, raised by
    /// the whole pixels that the lowest ink of the face's printable ASCII,
    /// U+0021 to U+007E, hinted as it is drawn, passes the cell's bottom
    /// edge, so the underscore and the descenders end in the cell's bottom
    /// rows. Every glyph of the face is drawn on it, whatever a request
    /// holds. A face with no printable ASCII, or only color glyphs there,
    /// keeps its ascent.
    fn baseline_at(drawer: &mut Drawer<'_>, metrics: &swash::Metrics, ppem: f32, h: u32) -> u32 {
        let scale = ppem / f32::from(metrics.units_per_em);
        let base = ((metrics.ascent * scale).ceil() as u32).min(h);
        let charmap = drawer.font.charmap();
        let lowest = (0x21u32..=0x7e)
            .filter_map(char::from_u32)
            .map(|c| charmap.map(c))
            .filter(|&g| g != 0)
            .filter_map(|g| drawer.draw(g, ppem, (u32::MAX, u32::MAX)))
            .filter(|i| i.content != Content::Color && i.placement.height > 0)
            .map(|i| i64::from(base) - i64::from(i.placement.top) + i64::from(i.placement.height))
            .max();
        match lowest {
            Some(bottom) if bottom > i64::from(h) => {
                base.saturating_sub(u32::try_from(bottom - i64::from(h)).unwrap_or(u32::MAX))
            }
            _ => base,
        }
    }

    /// The baseline of the face `family` of `font` in a cell `height`
    /// pixels high, as [`rasterise`] draws it (ADR-1302 D3).
    pub fn baseline(font: &[u8], family: &str, height: u32) -> Result<u32, String> {
        if !(1..=vtium_font::CELL_SIDE_MAX).contains(&height) {
            return Err(format!("a cell {height} pixels high, past CELL_SIDE_MAX"));
        }
        let (font, _, _) = face(font, family)?;
        let metrics = metrics(font)?;
        let mut drawer = Drawer {
            font,
            context: ScaleContext::new(),
        };
        Ok(baseline_at(
            &mut drawer,
            &metrics,
            ppem(&metrics, height),
            height,
        ))
    }

    /// Where the underline and strike-through go in a cell `height` pixels
    /// high whose baseline is row `baseline`, from the face `family` of
    /// `font` (ADR-1302 D3, ADR-1200 D2), as the C reference took them: the
    /// underline's thickness and centre from the `post` table, below the
    /// baseline, its centre the table's position less half its thickness as
    /// FreeType's `underline_position` gives it, else a fourteenth of the cell at its bottom; the
    /// strike-through's top and thickness from the `OS/2` table's
    /// strikeout, above the baseline, when its size is above 0, else as
    /// thick as the underline and centred on half the x-height, `OS/2`'s
    /// `sxHeight` or else the `x` glyph's height, or half the baseline; each at least a row thick, at most a quarter of the cell,
    /// and inside it (`kmscon_font_freetype_get_deco`,
    /// `kmscon_font_get_deco`).
    pub fn decorations(
        font: &[u8],
        family: &str,
        height: u32,
        baseline: u32,
    ) -> Result<vtium_font::Decorations, String> {
        let (font, _, _) = face(font, family)?;
        let m = metrics(font)?;
        let h = f64::from(height.max(1));
        let scale = f64::from(ppem(&m, height)) / f64::from(m.units_per_em);
        let base = f64::from(baseline);
        let max_thick = (height / 4).max(1);
        let mut thick = (f64::from(m.stroke_size) * scale).round();
        if thick <= 0.0 {
            thick = (h / 14.0).floor().max(1.0);
        }
        // swash gives post's underlinePosition, the stroke's top; FreeType's
        // underline_position, which the C reference took as the stroke's
        // centre, is that less half the underlineThickness
        let centre = if m.underline_offset != 0.0 {
            base - f64::from(m.underline_offset - m.stroke_size / 2.0) * scale
        } else {
            h - thick / 2.0 - thick
        };
        let ul_thick = (thick as u32).clamp(1, max_thick);
        let ul_pos = ((centre - thick / 2.0).round().max(0.0) as u32).min(height - ul_thick);
        // OS/2: yStrikeoutSize at byte 26, yStrikeoutPosition at 28, and
        // from version 2 sxHeight at 86
        let os2 = font.table(swash::tag_from_bytes(b"OS/2"));
        let field = |at: usize| {
            os2.and_then(|t| t.get(at..at + 2))
                .and_then(|b| b.first_chunk::<2>())
                .map(|&b| f64::from(i16::from_be_bytes(b)))
        };
        let version = field(0).unwrap_or(0.0);
        let (strike_top, strike_thick) = match (field(26), field(28)) {
            (Some(size), Some(position)) if size > 0.0 => {
                let t = ((size * scale).round() as u32).clamp(1, max_thick);
                (base - position * scale, t)
            }
            _ => {
                let mut xh = field(86)
                    .filter(|&x| version >= 2.0 && x > 0.0)
                    .map(|x| x * scale);
                if xh.is_none() {
                    // the x glyph's top above the baseline, hinted
                    let mut drawer = Drawer {
                        font,
                        context: ScaleContext::new(),
                    };
                    let g = font.charmap().map('x');
                    xh = (g != 0)
                        .then(|| drawer.draw(g, ppem(&m, height), (u32::MAX, u32::MAX)))
                        .flatten()
                        .map(|i| f64::from(i.placement.top))
                        .filter(|&t| t > 0.0);
                }
                let xh = xh.unwrap_or(base / 2.0);
                (base - xh / 2.0 - f64::from(ul_thick) / 2.0, ul_thick)
            }
        };
        let strike_pos = (strike_top.round().max(0.0) as u32).min(height - strike_thick);
        Ok(vtium_font::Decorations {
            underline: ul_pos,
            underline_thickness: ul_thick,
            strike: strike_pos,
            strike_thickness: strike_thick,
        })
    }

    pub fn rasterise(request: &OutlineRequest) -> Result<Outline, String> {
        let (font, family, index) = face(&request.font, &request.family)?;
        let metrics = metrics(font)?;
        let charmap = font.charmap();
        let mut wanted: Vec<(u32, u16)> = Vec::new();
        charmap.enumerate(|c, g| {
            let control = c < 0x20 || (0x7f..0xa0).contains(&c);
            if g != 0 && !control && request.covers(c) && char::from_u32(c).is_some() {
                wanted.push((c, g));
            }
        });
        wanted.sort_unstable();
        wanted.dedup_by_key(|&mut (c, _)| c);
        let shaped =
            crate::shaping::shape::shape_all(&request.font, index, font, &request.clusters);
        let mut drawer = Drawer {
            font,
            context: ScaleContext::new(),
        };
        let mut strikes = Vec::new();
        for (i, &(want_w, h)) in request.cells.iter().enumerate() {
            let ppem = ppem(&metrics, h);
            let baseline = match request.baselines.get(i) {
                Some(&b) => b,
                None => baseline_at(&mut drawer, &metrics, ppem, h),
            };
            let advances = font.glyph_metrics(&[]).scale(ppem);
            let width = if want_w != 0 {
                want_w
            } else {
                w_width(&mut drawer, ppem, h)
            };
            let cell = Cell {
                width,
                height: h,
                baseline,
                ppem,
            };
            let mut glyphs: Vec<Placed> = Vec::new();
            let mut index: Vec<(u16, u32)> = Vec::new();
            let mut map = Vec::new();
            for &(c, g) in &wanted {
                let at = match index.iter().find(|(id, _)| *id == g) {
                    Some(&(_, i)) => i,
                    None => {
                        let advance = advances.advance_width(g);
                        // a glyph larger than a set's chunk cannot be stored:
                        // a color glyph two cells wide past about 64 pixels
                        let joins = crate::shaping::joins(c);
                        let Some(placed) = place(&mut drawer, g, advance, joins, &cell)
                            .filter(|p| p.pixels.len() <= SET_CHUNK_BYTES as usize)
                        else {
                            continue;
                        };
                        let i = u32::try_from(glyphs.len()).map_err(|_| "too many glyphs")?;
                        glyphs.push(placed);
                        index.push((g, i));
                        i
                    }
                };
                map.push((c, at));
            }
            let clusters =
                crate::shaping::shape::draw_all(&mut drawer, &shaped, &cell, &mut glyphs)?;
            strikes.push(Strike {
                width,
                height: h,
                baseline,
                glyphs,
                map,
                clusters,
            });
        }
        let color = strikes.iter().flat_map(|s| &s.glyphs).any(|g| g.color);
        let mut license = font
            .localized_strings()
            .find_by_id(StringId::License, Some("en"))
            .map(|s| {
                s.to_string()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        while license.len() > LICENSE_MAX {
            license.pop();
        }
        Ok(Outline {
            family,
            format: if color { "outline+color" } else { "outline" }.to_owned(),
            license,
            strikes,
        })
    }

    /// The cell glyphs are placed in, and the size they are drawn at.
    pub(crate) struct Cell {
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) baseline: u32,
        pub(crate) ppem: f32,
    }

    /// Glyph `g`, advancing `advance` pixels, placed in `cell`, one or two
    /// cells wide by its advance; `None` when the font draws nothing for
    /// it. A glyph of a character that joins (ADR-1302 D2, `joins`) is
    /// stretched sideways before it is rasterised, by its cells' width over
    /// its advance, unrounded, its pen on its cell's left edge, so its
    /// advance fills its cells and its joining strokes meet the next
    /// letter's.
    fn place(
        drawer: &mut Drawer<'_>,
        g: u16,
        advance: f32,
        joins: bool,
        cell: &Cell,
    ) -> Option<Placed> {
        let wide = advance > cell.width as f32 * 1.5;
        let span = cell.width as f32 * if wide { 2.0 } else { 1.0 };
        if joins && advance > 0.0 {
            let sx = span / advance;
            return place_drawn(
                &mut |ppem, fit| drawer.draw_stretched(g, ppem, fit, sx),
                wide,
                None,
                cell,
            );
        }
        place_drawn(
            &mut |ppem, fit| drawer.draw(g, ppem, fit),
            wide,
            Some(advance),
            cell,
        )
    }

    /// What `draw` draws at a size and within a box, placed in `cell`, two
    /// cells wide when `wide`. A mask is placed by the font (ADR-1302 D2):
    /// its pen starts at the cell's left edge, moved right by half of what
    /// its cells' width exceeds its advance rounded to the nearest pixel,
    /// rounded down, which is a move left when the advance is wider, and
    /// its ink lies at its bearing from the pen; with no advance, as for a
    /// ligature shaping places, the pen stays on the left edge. Nothing
    /// moves it to keep its ink inside its cells: the ink past them goes
    /// to its ink box, cut at its reach. A color image larger than its
    /// cells is drawn again at the size that fits them and centred across
    /// them.
    pub(crate) fn place_drawn(
        draw: &mut dyn FnMut(f32, (u32, u32)) -> Option<Image>,
        wide: bool,
        advance: Option<f32>,
        cell: &Cell,
    ) -> Option<Placed> {
        let h = cell.height;
        let bw = cell.width * if wide { 2 } else { 1 };
        let mut image = draw(cell.ppem, (bw, h))?;
        let color = image.content == Content::Color;
        let (iw, ih) = (image.placement.width, image.placement.height);
        if color && (iw > bw || ih > h) && iw > 0 && ih > 0 {
            // a COLR glyph, drawn again at the size that fits
            let fit = (bw as f32 / iw as f32).min(h as f32 / ih as f32);
            image = draw(cell.ppem * fit, (bw, h))?;
        }
        let (iw, ih) = (image.placement.width as i32, image.placement.height as i32);
        let (bw_i, h_i, base) = (bw as i32, h as i32, cell.baseline as i32);
        let (left, top) = if color {
            let top = (base - image.placement.top).clamp(0, (h_i - ih).max(0));
            ((bw_i - iw) / 2, top)
        } else {
            // the C reference's `centre_dx` rounded the advance, (adv + 32) >> 6
            let pen = advance.map_or(0, |a| (bw_i - a.round() as i32).div_euclid(2));
            (pen + image.placement.left, base - image.placement.top)
        };
        let ink = if color {
            None
        } else {
            ink_box(&image, cell.width, bw, h, left, top)
        };
        Some(Placed {
            wide,
            color,
            pixels: copy(&image, color, bw, h, left, top),
            ink,
        })
    }

    /// The ink box of a mask placed at `left`, `top` of a glyph `bw`
    /// pixels wide in cells `cell` pixels wide and `h` high (ADR-1302 D1,
    /// D6): its ink copied into its reach, one cell past its cells on each
    /// side, and cut to the columns that span its cells and its ink;
    /// `None` when its ink lies inside its cells, or when the box would
    /// pass a set's chunk, where the glyph is cut to its cells.
    fn ink_box(image: &Image, cell: u32, bw: u32, h: u32, left: i32, top: i32) -> Option<InkBox> {
        let reach = bw.checked_add(cell.checked_mul(2)?)?;
        let all = copy(
            image,
            false,
            reach,
            h,
            left.checked_add(i32::try_from(cell).ok()?)?,
            top,
        );
        let inked =
            |x: u32| (0..h).any(|y| all.get((y * reach + x) as usize).is_some_and(|&p| p != 0));
        let first = (0..reach).find(|&x| inked(x))?;
        let last = (0..reach).rev().find(|&x| inked(x))?;
        let from = first.min(cell);
        let to = (last + 1).max(cell + bw);
        if from == cell && to == cell + bw {
            return None;
        }
        let width = to - from;
        if u64::from(width) * u64::from(h) > u64::from(SET_CHUNK_BYTES) {
            return None;
        }
        let pixels = (0..h)
            .flat_map(|y| {
                let row = (y * reach) as usize;
                all.get(row + from as usize..row + to as usize)
                    .unwrap_or_default()
                    .iter()
                    .copied()
            })
            .collect();
        Some(InkBox {
            left: i32::try_from(from).ok()? - i32::try_from(cell).ok()?,
            width,
            pixels,
        })
    }
}
