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
//! request  magic "VTVOUT1\0", font length u32, the font, family length u32,
//!          the family, cell count u32, each cell's width u32 (0: the font
//!          sets it) and height u32, range count u32, each range's first
//!          and last codepoint u32
//! answer   magic "VTVOUTL1", family length u32, the family, format length
//!          u32, the format, licence length u32, the licence, strike count u32, then each strike: cell width
//!          u32, cell height u32, baseline u32, glyph count u32, map length
//!          u32, each glyph's flags u8 (1 two cells wide, 2 RGBA) and its
//!          rows, then the map: each (codepoint u32, glyph u32), sorted
//! ```
//!
//! Little-endian throughout.
//!
//! Each cell's font size follows the C reference's flow (`font_freetype.c`,
//! a cell request for half a pixel under the height): the font's ascent
//! and descent span the cell's height less half a pixel, the baseline sits
//! the ascent below the top, and the font that sets the cell sets its width
//! from `W`'s advance or ink, whichever is wider. A glyph is placed by its
//! own bearing on the baseline, moved left by half of what passes the
//! cell's right edge and cut at the cell's edges. A glyph advancing more
//! than one and a half cells is two cells wide. A color glyph larger than
//! its cells is drawn again at the size that fits them, centred across
//! them and kept inside the cell's height. Outlines are hinted; color
//! glyphs are not, and a COLR glyph's layers are laid over each other here,
//! premultiplied with the renderer's rounding.

use crate::frit::Cover;
use vtium_font::{CELL_SIDE_MAX, SET_CHUNK_BYTES};

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

pub(crate) const REQUEST_MAGIC: &[u8; 8] = b"VTVOUT1\0";
const ANSWER_MAGIC: &[u8; 8] = b"VTVOUTL1";

const GLYPH_WIDE: u8 = 1;
const GLYPH_COLOR: u8 = 2;

/// What the parser is given to rasterise an outline font.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutlineRequest {
    pub font: Vec<u8>,
    /// The family to take: in a collection, its first face of that family;
    /// a single font must be of it. Empty takes the first face.
    pub family: String,
    /// Each cell to draw at, width and height; a width of 0 lets the font
    /// set the cell's width, as the set's first source does.
    pub cells: Vec<(u32, u32)>,
    /// The codepoints to draw, as ranges; empty for every one the font
    /// maps.
    pub cover: Vec<Cover>,
}

/// One glyph placed in its cells: R8 coverage or premultiplied RGBA, row
/// by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub wide: bool,
    pub color: bool,
    pub pixels: Vec<u8>,
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
        let mut out = REQUEST_MAGIC.to_vec();
        put32(&mut out, len32(self.font.len()));
        out.extend_from_slice(&self.font);
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
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<OutlineRequest, &'static str> {
        let mut r = Reader(bytes);
        if r.take(8)? != REQUEST_MAGIC {
            return Err("not an outline request");
        }
        let n = r.count(usize::try_from(OUTLINE_BYTES_MAX).unwrap_or(usize::MAX))?;
        let font = r.take(n)?.to_vec();
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
        if !r.0.is_empty() {
            return Err("bytes after the request");
        }
        Ok(OutlineRequest {
            font,
            family,
            cells,
            cover,
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
            ] {
                put32(&mut out, v);
            }
            for g in &s.glyphs {
                let flags =
                    if g.wide { GLYPH_WIDE } else { 0 } | if g.color { GLYPH_COLOR } else { 0 };
                out.push(flags);
                out.extend_from_slice(&g.pixels);
            }
            for &(c, g) in &s.map {
                put32(&mut out, c);
                put32(&mut out, g);
            }
        }
        out
    }

    /// Reads and checks an answer to `request`: a strike for each cell
    /// asked for, of its height and, where asked, its width, each inside
    /// [`CELL_SIDE_MAX`] with its baseline inside it; every glyph exactly
    /// as long as its flags say and no larger than a set's chunk, RGBA
    /// premultiplied; every codepoint a scalar value in order, one the
    /// request covers, and every glyph one the strike holds; nothing after
    /// the last strike.
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
            let map_len = r.count(0x11_0000)?;
            let mut glyphs = Vec::new();
            for _ in 0..glyph_count {
                let flags = r.u8()?;
                if flags & !(GLYPH_WIDE | GLYPH_COLOR) != 0 {
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
                glyphs.push(Placed {
                    wide,
                    color,
                    pixels,
                });
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
            strikes.push(Strike {
                width,
                height,
                baseline,
                glyphs,
                map,
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

pub use raster::rasterise;

/// The child's side: swash, on bytes no one has checked. A panic here ends
/// the child, which fails the source and nothing else (ADR-1200 D14).
mod raster {
    use super::{
        LICENSE_MAX, Outline, OutlineRequest, Placed, SET_CHUNK_BYTES, Strike, family_key,
    };
    use swash::scale::image::{Content, Image};
    use swash::scale::{Render, ScaleContext, Source, StrikeWith};
    use swash::zeno::{Mask, Origin, Placement};
    use swash::{FontDataRef, FontRef, StringId};

    /// The face `family` names in `data`, or its first face for an empty
    /// family; with the family its name table gives.
    fn face<'a>(data: &'a [u8], family: &str) -> Result<(FontRef<'a>, String), String> {
        let fonts = FontDataRef::new(data).ok_or("not a TrueType or OpenType font")?;
        let want = family_key(family);
        let mut first = None;
        for font in fonts.fonts() {
            let strings = font.localized_strings();
            let names: Vec<String> = [StringId::TypographicFamily, StringId::Family]
                .into_iter()
                .filter_map(|id| strings.find_by_id(id, Some("en")))
                .map(|s| s.to_string())
                .collect();
            let name = names.first().cloned().unwrap_or_default();
            if want.is_empty() {
                return Ok((font, name));
            }
            if names.iter().any(|n| family_key(n) == want) {
                return Ok((font, name));
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
    fn pixel(image: &Image, x: u32, y: u32, color: bool) -> [u8; 4] {
        let w = image.placement.width as usize;
        let (x, y) = (x as usize, y as usize);
        if color {
            let at = (y * w + x) * 4;
            match image.data.get(at..at + 4) {
                Some(&[r, g, b, a]) => {
                    if matches!(image.source, Source::ColorBitmap(_)) {
                        // a PNG's colors are not premultiplied
                        let m = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
                        [m(r), m(g), m(b), a]
                    } else {
                        [r.min(a), g.min(a), b.min(a), a]
                    }
                }
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
    const FOREGROUND: [u8; 4] = [128, 128, 128, 255];

    /// `t / 255` rounded, for `t` up to 255 * 255.
    fn div255(t: u32) -> u32 {
        (t + 127) / 255
    }

    /// Draws one face's glyphs: color glyphs unhinted, as COLR layers or
    /// bitmaps, and outlines hinted.
    struct Drawer<'a> {
        font: FontRef<'a>,
        context: ScaleContext,
    }

    impl Drawer<'_> {
        /// Glyph `g` at `ppem`: its color form if it has one, else its
        /// outline as coverage.
        fn draw(&mut self, g: u16, ppem: f32) -> Option<Image> {
            let mut scaler = self.context.builder(self.font).size(ppem).build();
            if scaler.has_color_outlines() {
                if let Some(layers) = scaler.scale_color_outline(g) {
                    return Some(self.compose(&layers));
                }
            }
            let bitmap = [Source::ColorBitmap(StrikeWith::BestFit)];
            if let Some(image) = Render::new(&bitmap).render(&mut scaler, g) {
                return Some(image);
            }
            let mut scaler = self
                .context
                .builder(self.font)
                .size(ppem)
                .hint(true)
                .build();
            Render::new(&[Source::Outline]).render(&mut scaler, g)
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

    pub fn rasterise(request: &OutlineRequest) -> Result<Outline, String> {
        let (font, family) = face(&request.font, &request.family)?;
        let metrics = font.metrics(&[]);
        let span = metrics.ascent + metrics.descent.abs();
        if metrics.units_per_em == 0 || span <= 0.0 {
            return Err("the font gives no ascent, descent or units per em".to_owned());
        }
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
        let mut drawer = Drawer {
            font,
            context: ScaleContext::new(),
        };
        let mut strikes = Vec::new();
        for &(want_w, h) in &request.cells {
            // the C reference asked for a cell half a pixel under the height
            let ppem = (h as f32 - 0.5) * f32::from(metrics.units_per_em) / span;
            let scale = ppem / f32::from(metrics.units_per_em);
            let baseline = ((metrics.ascent * scale).ceil() as u32).min(h);
            let advances = font.glyph_metrics(&[]).scale(ppem);
            let width = if want_w != 0 {
                want_w
            } else {
                ['W', 'M', '0']
                    .into_iter()
                    .map(|c| charmap.map(c))
                    .find(|&g| g != 0)
                    .map(|g| {
                        let ink = drawer.draw(g, ppem).map_or(0, |i| i.placement.width);
                        (advances.advance_width(g).round() as u32).max(ink)
                    })
                    .unwrap_or(h.div_ceil(2))
                    .clamp(1, vtium_font::CELL_SIDE_MAX)
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
                        let Some(placed) = place(&mut drawer, g, advance, &cell)
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
            strikes.push(Strike {
                width,
                height: h,
                baseline,
                glyphs,
                map,
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
    struct Cell {
        width: u32,
        height: u32,
        baseline: u32,
        ppem: f32,
    }

    /// Glyph `g`, advancing `advance` pixels, placed in `cell`, one or two
    /// cells wide by its advance; `None` when the font draws nothing for
    /// it.
    fn place(drawer: &mut Drawer<'_>, g: u16, advance: f32, cell: &Cell) -> Option<Placed> {
        let h = cell.height;
        let mut image = drawer.draw(g, cell.ppem)?;
        let color = image.content == Content::Color;
        let wide = advance > cell.width as f32 * 1.5;
        let bw = cell.width * if wide { 2 } else { 1 };
        let (iw, ih) = (image.placement.width, image.placement.height);
        if color && (iw > bw || ih > h) && iw > 0 && ih > 0 {
            let fit = (bw as f32 / iw as f32).min(h as f32 / ih as f32);
            image = drawer.draw(g, cell.ppem * fit)?;
        }
        let (iw, ih) = (image.placement.width as i32, image.placement.height as i32);
        let (bw_i, h_i, base) = (bw as i32, h as i32, cell.baseline as i32);
        let (left, top) = if color {
            let top = (base - image.placement.top).clamp(0, (h_i - ih).max(0));
            ((bw_i - iw) / 2, top)
        } else {
            let mut left = image.placement.left;
            if left + iw > bw_i {
                left -= (left + iw - bw_i) / 2;
            }
            (left, base - image.placement.top)
        };
        Some(Placed {
            wide,
            color,
            pixels: copy(&image, color, bw, h, left, top),
        })
    }
}
