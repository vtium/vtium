//! Color bitmap strikes: a font's glyphs stored as PNG images at one or
//! more sizes, in an `sbix` table or in a `CBLC` table that locates each
//! image in a `CBDT` table. Noto Color Emoji stores its emoji this way.
//!
//! vtium-config reads the tables itself with read-fonts and decodes each
//! image with the png crate. Every count and offset a font gives is checked
//! against the table that holds it before it is used, so a damaged table
//! draws nothing instead of looping, wrapping or reading another glyph's
//! image. An image is refused before anything is allocated for it when its
//! PNG header asks for more than [`super::raster::GLYPH_SIDE_MAX`] pixels
//! on a side, and the png crate inflates only the bytes the image's rows
//! need. The images keep the placement that swash, the rasteriser
//! vtium-config draws outlines with, gives them, so a font draws as it did
//! when swash read its strikes.
//!
//! References:
//! - ADR-1200, the note of 2026-11-21: read-fonts and png read color
//!   bitmap strikes in place of swash's strike reader.

use std::io::Cursor;

use skrifa::raw::tables::bitmap::{
    BitmapContent, BitmapDataFormat, BitmapLocation, BitmapMetrics, BitmapSize, IndexSubtable,
    IndexSubtableList,
};
use skrifa::raw::tables::cbdt::Cbdt;
use skrifa::raw::tables::cblc::Cblc;
use skrifa::raw::tables::sbix::{GlyphData, Strike};
use skrifa::raw::types::FixedSize as _;
use skrifa::raw::types::{GlyphId16, Tag};
use skrifa::raw::{FontData, FontRef, TableProvider};

use super::raster::GLYPH_SIDE_MAX;

/// The most bytes one decoded image may take: 4,096 × 4,096 pixels of four
/// bytes, 64 MiB. It bounds what the png crate allocates as it decodes too.
const DECODED_BYTES_MAX: usize = (GLYPH_SIDE_MAX as usize) * (GLYPH_SIDE_MAX as usize) * 4;

/// A glyph's image from a color bitmap strike, as the strike stores it:
/// RGBA of 8 bits a sample, not premultiplied, row by row.
pub(crate) struct ColorBitmap {
    /// The strike's size, in pixels to the em.
    pub(crate) ppem: u16,
    /// The image's left edge right of the glyph's origin, in pixels.
    pub(crate) left: i32,
    /// The image's top edge above the baseline, in pixels.
    pub(crate) top: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// Where a glyph's PNG image lies in a strike, and how it is placed, found
/// without decoding it.
struct Found<'a> {
    ppem: u16,
    png: &'a [u8],
    left: i32,
    /// The top edge above the baseline, or for sbix the bottom edge's
    /// offset, which the image's height raises to its top.
    top: i32,
    /// sbix places the image's bottom edge; CBDT its top edge.
    from_bottom: bool,
}

/// The color bitmap strikes of one face.
pub(crate) enum Strikes<'a> {
    None,
    /// An `sbix` table. swash prefers it to CBLC and CBDT when a font has
    /// both, and so does vtium-config.
    Sbix {
        data: FontData<'a>,
        glyphs: u16,
        /// Whether the face is Apple Color Emoji, whose images stand on an
        /// origin 100 font units below the baseline when they give none.
        apple: bool,
        upem: u16,
    },
    Cbdt {
        cblc: Cblc<'a>,
        cbdt: Cbdt<'a>,
    },
}

/// `count` records of `size` bytes from `start` in `data`, cut to the
/// records the data holds, so a count of four billion in a table of a few
/// bytes names only the records that are there.
fn held(data: FontData<'_>, start: usize, count: u32, size: usize) -> usize {
    let room = data.len().saturating_sub(start) / size;
    usize::try_from(count).map_or(room, |n| n.min(room))
}

impl<'a> Strikes<'a> {
    /// The strikes of `font`. `apple` says whether the face is Apple Color
    /// Emoji, asked only of a face with an sbix table.
    pub(crate) fn new(font: &FontRef<'a>, apple: impl FnOnce() -> bool) -> Self {
        if let Some(data) = font.table_data(Tag::new(b"sbix")) {
            return Strikes::Sbix {
                data,
                glyphs: font.maxp().map_or(0, |m| m.num_glyphs()),
                apple: apple(),
                upem: font.head().map_or(1, |h| h.units_per_em()),
            };
        }
        match (font.cblc(), font.cbdt()) {
            (Ok(cblc), Ok(cbdt)) => Strikes::Cbdt { cblc, cbdt },
            _ => Strikes::None,
        }
    }

    /// Whether glyph `g` has an image in a strike of a size above 0.
    pub(crate) fn holds(&self, g: u16) -> bool {
        self.find(g).is_some()
    }

    /// Glyph `g`'s image from the largest strike that holds it, decoded.
    pub(crate) fn bitmap(&self, g: u16) -> Option<ColorBitmap> {
        let found = self.find(g)?;
        let (width, height, rgba) = decode(found.png)?;
        let mut top = found.top;
        if found.from_bottom {
            top = top.checked_add(i32::try_from(height).ok()?)?;
        }
        Some(ColorBitmap {
            ppem: found.ppem,
            left: found.left,
            top,
            width,
            height,
            rgba,
        })
    }

    /// The largest strike that holds `g`, the first of those of one size,
    /// as swash picks it; a strike of size 0 draws nothing.
    fn find(&self, g: u16) -> Option<Found<'a>> {
        let mut best: Option<Found<'a>> = None;
        let mut take = |found: Option<Found<'a>>| {
            if let Some(f) = found
                && f.ppem != 0
                && best.as_ref().is_none_or(|b| f.ppem > b.ppem)
            {
                best = Some(f);
            }
        };
        match self {
            Strikes::None => {}
            Strikes::Sbix {
                data,
                glyphs,
                apple,
                upem,
            } => {
                let count = data.read_at::<u32>(4).unwrap_or(0);
                for i in 0..held(*data, 8, count, 4) {
                    let Ok(offset) = data.read_at::<u32>(8 + i * 4) else {
                        break;
                    };
                    let strike = usize::try_from(offset)
                        .ok()
                        .and_then(|o| data.slice(o..))
                        .and_then(|d| Strike::read(d, *glyphs).ok());
                    take(strike.and_then(|s| sbix_png(&s, g, *apple, *upem)));
                }
            }
            Strikes::Cbdt { cblc, cbdt } => {
                let data = cblc.offset_data();
                let count = held(data, 8, cblc.num_sizes(), BitmapSize::RAW_BYTE_LEN);
                let sizes: &[BitmapSize] = data
                    .read_array(8..8 + count * BitmapSize::RAW_BYTE_LEN)
                    .unwrap_or_default();
                for size in sizes {
                    take(cbdt_png(size, data, cbdt, g));
                }
            }
        }
        best
    }
}

/// Glyph `g`'s PNG in an sbix strike. A `dupe` record names another glyph
/// whose image it shares, followed once, as swash follows it; any other
/// kind of image, such as JPEG or TIFF, draws nothing.
fn sbix_png<'a>(strike: &Strike<'a>, g: u16, apple: bool, upem: u16) -> Option<Found<'a>> {
    let mut data: GlyphData<'a> = strike.glyph_data(g.into()).ok()??;
    if data.graphic_type() == Tag::new(b"dupe") {
        let &[hi, lo, ..] = data.data() else {
            return None;
        };
        let to = u16::from_be_bytes([hi, lo]);
        data = strike.glyph_data(to.into()).ok()??;
    }
    if data.graphic_type() != Tag::new(b"png ") {
        return None;
    }
    let ppem = strike.ppem();
    let mut top = i32::from(data.origin_offset_y());
    // swash's rule for Apple Color Emoji, whose images give no origin:
    // they stand 100 font units below the baseline
    if top == 0 && apple && upem != 0 {
        top = (-100.0 * f32::from(ppem) / f32::from(upem)).round() as i32;
    }
    Some(Found {
        ppem,
        png: data.data(),
        left: i32::from(data.origin_offset_x()),
        top,
        from_bottom: true,
    })
}

/// Glyph `g`'s PNG in the CBLC strike `size`, from CBDT. The index
/// subtable whose range holds `g` locates its image, and every offset is
/// summed with a check, so an offset past CBDT's end draws nothing.
fn cbdt_png<'a>(
    size: &BitmapSize,
    cblc: FontData<'a>,
    cbdt: &Cbdt<'a>,
    g: u16,
) -> Option<Found<'a>> {
    let gid = GlyphId16::new(g);
    if !(size.start_glyph_index()..=size.end_glyph_index()).contains(&gid) {
        return None;
    }
    let start = usize::try_from(size.index_subtable_list_offset()).ok()?;
    let end = start.checked_add(usize::try_from(size.index_subtable_list_size()).ok()?)?;
    let list = cblc.slice(start..end)?;
    let count = held(list, 0, size.number_of_index_subtables(), 8);
    let list = IndexSubtableList::read(list, u32::try_from(count).ok()?).ok()?;
    let record = list
        .index_subtable_records()
        .iter()
        .find(|r| (r.first_glyph_index()..=r.last_glyph_index()).contains(&gid))?;
    let subtable = record.index_subtable(list.offset_data()).ok()?;
    let at = usize::from(g - record.first_glyph_index().to_u16());
    let base = usize::try_from(subtable.image_data_offset()).ok()?;
    let mut location = BitmapLocation {
        format: subtable.image_format(),
        bit_depth: size.bit_depth(),
        ..BitmapLocation::default()
    };
    // each format gives the image's start and end from imageDataOffset
    let (from, to) = match &subtable {
        IndexSubtable::Format1(s) => {
            let o = s.sbit_offsets();
            (
                o.get(at)?.get() as usize,
                o.get(at.checked_add(1)?)?.get() as usize,
            )
        }
        IndexSubtable::Format3(s) => {
            let o = s.sbit_offsets();
            (
                usize::from(o.get(at)?.get()),
                usize::from(o.get(at.checked_add(1)?)?.get()),
            )
        }
        IndexSubtable::Format2(s) => {
            location.metrics = s.big_metrics().first().copied();
            let image = usize::try_from(s.image_size()).ok()?;
            let from = at.checked_mul(image)?;
            (from, from.checked_add(image)?)
        }
        IndexSubtable::Format4(s) => {
            // numGlyphs records, then one that ends the last glyph's data
            let pairs = s.glyph_array();
            let n = usize::try_from(s.num_glyphs())
                .ok()?
                .min(pairs.len().saturating_sub(1));
            let i = pairs
                .get(..n)?
                .binary_search_by(|p| p.glyph_id().to_u16().cmp(&g))
                .ok()?;
            (
                usize::from(pairs.get(i)?.sbit_offset()),
                usize::from(pairs.get(i.checked_add(1)?)?.sbit_offset()),
            )
        }
        IndexSubtable::Format5(s) => {
            location.metrics = s.big_metrics().first().copied();
            let ids = s.glyph_array();
            let n = usize::try_from(s.num_glyphs()).ok()?.min(ids.len());
            let i = ids
                .get(..n)?
                .binary_search_by(|id| id.get().to_u16().cmp(&g))
                .ok()?;
            let image = usize::try_from(s.image_size()).ok()?;
            let from = i.checked_mul(image)?;
            (from, from.checked_add(image)?)
        }
    };
    if to < from {
        return None;
    }
    location.data_offset = base.checked_add(from)?;
    location.data_size = to - from;
    // read-fonts adds the two unchecked, so the end is checked here
    let end = location.data_offset.checked_add(location.data_size)?;
    if end > cbdt.offset_data().len() {
        return None;
    }
    let image = cbdt.data(&location).ok()?;
    let BitmapContent::Data(BitmapDataFormat::Png, png) = image.content else {
        return None;
    };
    // swash's placement: a small metrics record holds the horizontal
    // bearings unless the strike's flags say its metrics are vertical
    let (left, top) = match image.metrics {
        BitmapMetrics::Small(m) if size.flags().bits() & 2 == 0 => (m.bearing_x(), m.bearing_y()),
        BitmapMetrics::Small(_) => (0, 0),
        BitmapMetrics::Big(m) => (m.hori_bearing_x(), m.hori_bearing_y()),
    };
    Some(Found {
        ppem: u16::from(size.ppem_y()),
        png,
        left: i32::from(left),
        top: i32::from(top),
        from_bottom: false,
    })
}

/// `png` decoded to 8-bit RGBA, with its width and height. A header whose
/// width or height is 0 or past [`GLYPH_SIDE_MAX`] is refused before
/// anything is allocated. 16-bit samples keep their high byte, palettes and
/// grey are expanded to RGBA, and a `tRNS` chunk gives the alpha. The png
/// crate inflates only the bytes the image's rows need and passes over the
/// rest of the data, so a stream that inflates to gigabytes costs no more
/// than its image.
fn decode(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    // the signature, then IHDR, which PNG requires to come first: its
    // length, its name, then the width and height
    let Some(
        &[
            0x89,
            b'P',
            b'N',
            b'G',
            b'\r',
            b'\n',
            0x1a,
            b'\n',
            _,
            _,
            _,
            _,
            b'I',
            b'H',
            b'D',
            b'R',
            w0,
            w1,
            w2,
            w3,
            h0,
            h1,
            h2,
            h3,
        ],
    ) = png.get(..24)
    else {
        return None;
    };
    let (width, height) = (
        u32::from_be_bytes([w0, w1, w2, w3]),
        u32::from_be_bytes([h0, h1, h2, h3]),
    );
    if !(1..=GLYPH_SIDE_MAX).contains(&width) || !(1..=GLYPH_SIDE_MAX).contains(&height) {
        return None;
    }
    let limits = png::Limits {
        bytes: DECODED_BYTES_MAX,
    };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(png), limits);
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    if reader.info().size() != (width, height) {
        return None;
    }
    let size = reader.output_buffer_size()?;
    if size > DECODED_BYTES_MAX {
        return None;
    }
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf).ok()?;
    buf.truncate(frame.buffer_size());
    let rgba = match (frame.color_type, frame.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buf,
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[v, a]| [v, v, v, a])
            .collect(),
        _ => return None,
    };
    let pixels = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    (rgba.len() == pixels.checked_mul(4)?).then_some((width, height, rgba))
}
