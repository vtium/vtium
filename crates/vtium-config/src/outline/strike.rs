//! Color bitmap strikes: a font's glyphs stored as PNG images at one or
//! more sizes, in an `sbix` table or in a `CBLC` table that locates each
//! image in a `CBDT` table. Noto Color Emoji stores its emoji this way.
//!
//! vtium-config reads the tables itself with read-fonts and decodes each
//! image with the png crate. Every count and offset a font gives is checked
//! against the table that holds it before it is used, so a damaged table
//! draws nothing instead of looping, wrapping or reading another glyph's
//! image. vtium-config reads at most [`STRIKES_MAX`] strikes and
//! [`RECORDS_MAX`] index records of each, once per face, so the work of a
//! lookup does not grow with the font's size. An image is refused before
//! it is decoded when its PNG header gives a size other than the size its
//! glyph metrics give, or for sbix a size past twice the strike's own, and
//! the png crate inflates only the bytes the image's rows need. The images
//! keep the placement that swash, the rasteriser vtium-config draws
//! outlines with, gives them, except an sbix origin, which vtium-config
//! reads as the 16 bits the table gives.
//!
//! References:
//! - ADR-1200, the note of 2026-11-21: read-fonts and png read color
//!   bitmap strikes in place of swash's strike reader.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io::Cursor;

use skrifa::raw::tables::bitmap::{
    BitmapContent, BitmapDataFormat, BitmapLocation, BitmapMetrics, BitmapSize, IndexSubtable,
    IndexSubtableList, IndexSubtableRecord,
};
use skrifa::raw::tables::cbdt::Cbdt;
use skrifa::raw::tables::sbix::{GlyphData, Strike};
use skrifa::raw::types::FixedSize as _;
use skrifa::raw::types::{GlyphId16, Tag};
use skrifa::raw::{FontData, FontRef, TableProvider};

use super::raster::GLYPH_SIDE_MAX;

/// The most bytes one decoded image may take: 4,096 × 4,096 pixels of four
/// bytes, 64 MiB. It bounds what the png crate allocates as it decodes too.
const DECODED_BYTES_MAX: usize = (GLYPH_SIDE_MAX as usize) * (GLYPH_SIDE_MAX as usize) * 4;

/// The most strikes vtium-config reads from a CBLC or sbix table, the
/// first ones in the table. A font ships one strike for each size it
/// draws: Noto Color Emoji has one, and none of the 1,108 faces with bitmap
/// strikes measured when this bound was chosen had more than two. A
/// damaged table may name millions, all pointing at the same data.
const STRIKES_MAX: usize = 64;

/// The most index records vtium-config reads for one CBLC strike, the
/// first ones in its list. A font needs one record for each run of glyphs
/// stored alike: Noto Color Emoji has three, and none of those 1,108 faces
/// had more than eleven.
const RECORDS_MAX: usize = 4_096;

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
    /// An `sbix` table's strikes, in the table's order. swash prefers sbix
    /// to CBLC and CBDT when a font has both, and so does vtium-config.
    Sbix {
        strikes: Vec<Strike<'a>>,
        /// Whether the face is Apple Color Emoji, whose images stand on an
        /// origin 100 font units below the baseline when they give none.
        apple: bool,
        upem: u16,
    },
    Cbdt {
        cbdt: Cbdt<'a>,
        /// The CBLC strikes, in the table's order.
        sizes: Vec<Size<'a>>,
    },
}

/// One CBLC strike, with its glyph ranges sorted for a binary search.
pub(crate) struct Size<'a> {
    size: &'a BitmapSize,
    list: IndexSubtableList<'a>,
    ranges: Vec<Range>,
}

/// Glyphs `first` to `last`, whose image the index record `record` of a
/// strike's list locates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    first: u16,
    last: u16,
    record: usize,
}

/// Counts the index records and strikes a lookup reads, for the tests that
/// check that work is bounded. It does nothing outside the tests.
fn checked(_n: usize) {
    #[cfg(test)]
    CHECKED.with(|c| c.set(c.get() + _n));
}

#[cfg(test)]
thread_local! {
    static CHECKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    ///
    /// It reads the first [`STRIKES_MAX`] strikes, and for each CBLC
    /// strike the first [`RECORDS_MAX`] index records, sorted once into
    /// glyph ranges. A strike or record past those draws nothing.
    pub(crate) fn new(font: &FontRef<'a>, apple: impl FnOnce() -> bool) -> Self {
        if let Some(data) = font.table_data(Tag::new(b"sbix")) {
            let glyphs = font.maxp().map_or(0, |m| m.num_glyphs());
            let count = data.read_at::<u32>(4).unwrap_or(0);
            let count = held(data, 8, count, 4).min(STRIKES_MAX);
            checked(count);
            let strikes = (0..count)
                .filter_map(|i| {
                    let offset = usize::try_from(data.read_at::<u32>(8 + i * 4).ok()?).ok()?;
                    Strike::read(data.slice(offset..)?, glyphs).ok()
                })
                .collect();
            return Strikes::Sbix {
                strikes,
                apple: apple(),
                upem: font.head().map_or(1, |h| h.units_per_em()),
            };
        }
        let (Ok(cblc), Ok(cbdt)) = (font.cblc(), font.cbdt()) else {
            return Strikes::None;
        };
        let data = cblc.offset_data();
        let count = held(data, 8, cblc.num_sizes(), BitmapSize::RAW_BYTE_LEN).min(STRIKES_MAX);
        let sizes: &'a [BitmapSize] = data
            .read_array(8..8 + count * BitmapSize::RAW_BYTE_LEN)
            .unwrap_or_default();
        let sizes = sizes.iter().filter_map(|s| Size::new(s, data)).collect();
        Strikes::Cbdt { cbdt, sizes }
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
                strikes,
                apple,
                upem,
            } => {
                checked(strikes.len());
                for strike in strikes {
                    take(sbix_png(strike, g, *apple, *upem));
                }
            }
            Strikes::Cbdt { cbdt, sizes } => {
                for size in sizes {
                    take(size.png(cbdt, g));
                }
            }
        }
        best
    }
}

impl<'a> Size<'a> {
    /// The strike `size` of the CBLC table `cblc`, its index records read
    /// and sorted into ranges; `None` when its list lies outside the table.
    fn new(size: &'a BitmapSize, cblc: FontData<'a>) -> Option<Self> {
        let start = usize::try_from(size.index_subtable_list_offset()).ok()?;
        let end = start.checked_add(usize::try_from(size.index_subtable_list_size()).ok()?)?;
        let data = cblc.slice(start..end)?;
        let count = held(data, 0, size.number_of_index_subtables(), 8).min(RECORDS_MAX);
        let list = IndexSubtableList::read(data, u32::try_from(count).ok()?).ok()?;
        checked(count);
        let records = list
            .index_subtable_records()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                (
                    r.first_glyph_index().to_u16(),
                    r.last_glyph_index().to_u16(),
                    i,
                )
            });
        let ranges = ranges(records);
        Some(Size { size, list, ranges })
    }

    /// The index of the first record whose range holds `g`, found by a
    /// binary search over the strike's ranges.
    fn record(&self, g: u16) -> Option<usize> {
        let mut probes = 0;
        let i = self.ranges.partition_point(|r| {
            probes += 1;
            r.last < g
        });
        checked(probes);
        let range = self.ranges.get(i)?;
        (range.first <= g).then_some(range.record)
    }

    /// Glyph `g`'s PNG in this strike, from `cbdt`.
    fn png(&self, cbdt: &Cbdt<'a>, g: u16) -> Option<Found<'a>> {
        let gid = GlyphId16::new(g);
        if !(self.size.start_glyph_index()..=self.size.end_glyph_index()).contains(&gid) {
            return None;
        }
        let record = self.list.index_subtable_records().get(self.record(g)?)?;
        cbdt_png(self.size, &self.list, record, cbdt, g)
    }
}

/// Index records `(first, last, index)`, in their list's order, as ranges
/// sorted by glyph that do not overlap. Each glyph keeps the first record
/// in the list whose range holds it, as a walk through the list would
/// find, and a record whose last glyph comes before its first holds none.
///
/// The records are sorted by their first glyph and swept from the lowest
/// glyph up. The records whose ranges hold the glyph the sweep stands on
/// wait in a heap ordered by their index in the list, so the heap's top
/// is the one that glyph keeps. That glyph's range runs until the top
/// record ends or the next record starts, whichever comes first.
fn ranges(records: impl Iterator<Item = (u16, u16, usize)>) -> Vec<Range> {
    let mut records: Vec<(u16, u16, usize)> = records.filter(|&(f, l, _)| f <= l).collect();
    records.sort_unstable_by_key(|&(first, _, i)| (first, i));
    let mut out: Vec<Range> = Vec::new();
    let mut waiting: BinaryHeap<Reverse<(usize, u16)>> = BinaryHeap::new();
    let mut next = records.iter().peekable();
    // the glyph the sweep stands on, which passes 65,535 once at the end
    let mut at = 0u32;
    loop {
        if waiting.is_empty() {
            let Some(&&(first, _, _)) = next.peek() else {
                break;
            };
            at = at.max(u32::from(first));
        }
        while let Some(&(_, last, i)) = next.next_if(|r| u32::from(r.0) <= at) {
            waiting.push(Reverse((i, last)));
        }
        while waiting.peek().is_some_and(|w| u32::from(w.0.1) < at) {
            waiting.pop();
        }
        let Some(&Reverse((record, last))) = waiting.peek() else {
            continue;
        };
        // the next record starts after `at`, so its first glyph is above 0
        let end = next
            .peek()
            .map_or(u32::from(last), |r| u32::from(last).min(u32::from(r.0) - 1));
        // both lie within the top record's range, so they fit in 16 bits
        out.push(Range {
            first: at as u16,
            last: end as u16,
            record,
        });
        at = end + 1;
    }
    out
}

/// Glyph `g`'s PNG in an sbix strike. A `dupe` record names another glyph
/// whose image it shares, followed once, as swash follows it; any other
/// kind of image, such as JPEG or TIFF, draws nothing. sbix gives no glyph
/// metrics, so the strike's size is the only bound on an image's size: an
/// image wider or taller than twice the strike's pixels to the em, or than
/// [`GLYPH_SIDE_MAX`], is refused before it is decoded.
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
    let side = (2 * u32::from(ppem)).min(GLYPH_SIDE_MAX);
    let (width, height) = png_size(data.data())?;
    if width > side || height > side {
        return None;
    }
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

/// Glyph `g`'s PNG in the CBLC strike `size`, from CBDT, located by the
/// index record `record` of the strike's `list`, whose range holds `g`.
/// Every offset is summed with a check, so an offset past CBDT's end draws
/// nothing. An image whose PNG header gives a size other than its glyph
/// metrics give is refused: the metrics hold each side in one byte, so no
/// image is wider or taller than 255 pixels.
fn cbdt_png<'a>(
    size: &BitmapSize,
    list: &IndexSubtableList<'a>,
    record: &IndexSubtableRecord,
    cbdt: &Cbdt<'a>,
    g: u16,
) -> Option<Found<'a>> {
    let subtable = record.index_subtable(list.offset_data()).ok()?;
    // the record's range holds `g`, so this cannot underflow
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
    if !inside(
        location.data_offset,
        location.data_size,
        cbdt.offset_data().len(),
    ) {
        return None;
    }
    let image = cbdt.data(&location).ok()?;
    let BitmapContent::Data(BitmapDataFormat::Png, png) = image.content else {
        return None;
    };
    let metrics_size = match &image.metrics {
        BitmapMetrics::Small(m) => (m.width(), m.height()),
        BitmapMetrics::Big(m) => (m.width(), m.height()),
    };
    if png_size(png)? != (u32::from(metrics_size.0), u32::from(metrics_size.1)) {
        return None;
    }
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
    let (width, height) = png_size(png)?;
    if !(1..=GLYPH_SIDE_MAX).contains(&width) || !(1..=GLYPH_SIDE_MAX).contains(&height) {
        return None;
    }
    let limits = png::Limits {
        bytes: DECODED_BYTES_MAX,
    };
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(png), limits);
    // vtium-config reads neither text nor color profiles, so the decoder
    // passes over them, instead of keeping text under its limit and
    // inflating a profile only to drop it
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
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

/// Whether a glyph's image, `size` bytes from `offset`, lies inside the
/// CBDT table's data of `len` bytes. read-fonts adds the offset and the
/// size without a check before it slices the table. With a 32-bit `usize`,
/// as on armv7, the offsets of a damaged font can make that sum overflow,
/// which panics in a debug build. So the reader hands read-fonts only an
/// image that this function accepts, whose end it computed with a checked
/// add.
fn inside(offset: usize, size: usize, len: usize) -> bool {
    offset.checked_add(size).is_some_and(|end| end <= len)
}

/// The width and height `png`'s header gives, read from the bytes without
/// the png crate.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
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
    Some((
        u32::from_be_bytes([w0, w1, w2, w3]),
        u32::from_be_bytes([h0, h1, h2, h3]),
    ))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    //! The work a lookup does, counted in index records and strikes read
    //! rather than timed, and the bounds on the strikes and records
    //! vtium-config reads. Each font holds only the tables a lookup reads.

    use super::*;

    /// The end check before read-fonts' unchecked add, with the values
    /// near `usize::MAX` that only a 32-bit `usize` reaches from a font's
    /// 32-bit offsets: a sum that overflows is refused, as is an end past
    /// the table, and an image that ends at the table's end is read.
    #[test]
    fn an_image_whose_end_overflows_or_passes_the_table_is_refused() {
        let max = usize::MAX;
        assert!(!inside(max - 1, 2, max), "the sum overflows");
        assert!(!inside(max, 1, max), "the sum overflows");
        assert!(inside(max - 2, 2, max), "the end is the table's end");
        assert!(!inside(max - 2, 2, max - 1), "the end passes the table");
        assert!(inside(0, 0, 0));
        assert!(inside(2, 2, 4));
        assert!(!inside(3, 2, 4));
    }

    fn p16(v: &mut Vec<u8>, x: u16) {
        v.extend(x.to_be_bytes());
    }

    fn p32(v: &mut Vec<u8>, x: u32) {
        v.extend(x.to_be_bytes());
    }

    /// A font of one face holding `t`, whose tags are sorted.
    fn sfnt(t: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0, 1, 0, 0];
        p16(&mut out, t.len() as u16);
        out.extend([0; 6]);
        let mut at = 12 + 16 * t.len();
        for (tag, data) in t {
            out.extend(*tag);
            out.extend([0; 4]);
            p32(&mut out, at as u32);
            p32(&mut out, data.len() as u32);
            at += data.len().next_multiple_of(4);
        }
        for (_, data) in t {
            out.extend(data);
            out.resize(out.len().next_multiple_of(4), 0);
        }
        out
    }

    /// A 4 × 4 PNG of opaque blue.
    fn blue() -> Vec<u8> {
        vtium_testfont::ttf::png(4, 4, &[0, 0, 255, 255].repeat(16))
    }

    /// A CBDT holding [`blue`] in image format 17 at offset 4, with small
    /// metrics of 4 × 4.
    fn cbdt() -> Vec<u8> {
        let png = blue();
        let mut d = vec![0, 3, 0, 0, 4, 4, 0, 4, 4];
        p32(&mut d, png.len() as u32);
        d.extend(png);
        d
    }

    /// A CBLC of `sizes` strikes of 40 pixels to the em, all sharing one
    /// list of `records` index records. Record `target` covers glyph 1
    /// through a format 1 subtable that locates [`cbdt`]'s image; each
    /// other record covers glyph 0 alone. Each strike covers glyphs 1 to 3
    /// when `strike` is `None`; otherwise only strike `strike` does, and
    /// the others cover glyphs 2 and 3.
    fn cblc(sizes: u32, strike: Option<u32>, records: u32, target: u32) -> Vec<u8> {
        let list = 8 + 48 * sizes;
        let sub = 8 * records;
        let image = 9 + blue().len() as u32;
        let mut v = Vec::new();
        p16(&mut v, 3);
        p16(&mut v, 0);
        p32(&mut v, sizes);
        for i in 0..sizes {
            p32(&mut v, list);
            p32(&mut v, sub + 16);
            p32(&mut v, records);
            p32(&mut v, 0);
            for _ in 0..2 {
                v.extend([40, 0, 40, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
            }
            let first = if strike.is_none_or(|s| s == i) { 1 } else { 2 };
            p16(&mut v, first);
            p16(&mut v, 3);
            v.extend([40, 40, 32, 1]);
        }
        for i in 0..records {
            let g = u16::from(i == target);
            p16(&mut v, g);
            p16(&mut v, g);
            p32(&mut v, sub);
        }
        p16(&mut v, 1);
        p16(&mut v, 17);
        p32(&mut v, 4);
        p32(&mut v, 0);
        p32(&mut v, image);
        v
    }

    fn cblc_font(sizes: u32, strike: Option<u32>, records: u32, target: u32) -> Vec<u8> {
        sfnt(&[
            (b"CBDT", cbdt()),
            (b"CBLC", cblc(sizes, strike, records, target)),
        ])
    }

    /// An sbix of `strikes` strike offsets, of which only strike `target`
    /// holds glyph 1's PNG, in a font of 4 glyphs; the rest name one
    /// strike that holds no glyph.
    fn sbix_font(strikes: u32, target: u32) -> Vec<u8> {
        let png = blue();
        let mut s = Vec::new();
        p16(&mut s, 1);
        p16(&mut s, 1);
        p32(&mut s, strikes);
        let empty = 8 + 4 * strikes;
        let full = empty + 4 + 4 * 5;
        for i in 0..strikes {
            p32(&mut s, if i == target { full } else { empty });
        }
        // the strike that holds nothing: every glyph's data is empty
        p16(&mut s, 40);
        p16(&mut s, 72);
        for _ in 0..5 {
            p32(&mut s, 24);
        }
        // the strike that holds glyph 1
        p16(&mut s, 40);
        p16(&mut s, 72);
        let end = 24 + 8 + png.len() as u32;
        for o in [24, 24, end, end, end] {
            p32(&mut s, o);
        }
        s.extend([0, 0, 0, 0]);
        s.extend(b"png ");
        s.extend(&png);
        let mut maxp = vec![0, 0, 0x50, 0];
        p16(&mut maxp, 4);
        sfnt(&[(b"maxp", maxp), (b"sbix", s)])
    }

    /// Whether glyph 1 is held and draws in `font`, and the index records
    /// and strikes vtium-config read for it, building the strikes
    /// included.
    fn glyph_1(font: &[u8]) -> (bool, usize) {
        CHECKED.with(|c| c.set(0));
        let font = FontRef::new(font).expect("a font");
        let strikes = Strikes::new(&font, || false);
        let held = strikes.holds(1);
        let drawn = strikes.bitmap(1).is_some();
        assert_eq!(held, drawn, "the color test and the drawing agree");
        for g in [2, 3] {
            assert!(!strikes.holds(g));
        }
        (held, CHECKED.with(std::cell::Cell::get))
    }

    // N strikes can all name one list of M index records. A lookup that
    // walks every record of every strike checks N × M records for each
    // glyph, here 9.6 billion. vtium-config reads at most STRIKES_MAX
    // strikes and RECORDS_MAX records of each, once, and then finds a
    // glyph's record by a binary search
    #[test]
    fn a_lookup_reads_a_bounded_number_of_index_records() {
        let (held, checked) = glyph_1(&cblc_font(40_000, None, 240_000, 239_999));
        let bound = STRIKES_MAX * RECORDS_MAX + 5 * STRIKES_MAX * 16;
        assert!(checked <= bound, "{checked} records checked, past {bound}");
        assert!(!held, "glyph 1's record is past the records read");
    }

    #[test]
    fn a_lookup_in_sbix_reads_a_bounded_number_of_strikes() {
        let (held, checked) = glyph_1(&sbix_font(1_000_000, 999_999));
        let bound = 6 * STRIKES_MAX;
        assert!(checked <= bound, "{checked} strikes checked, past {bound}");
        assert!(!held, "glyph 1's strike is past the strikes read");
    }

    #[test]
    fn the_last_strike_read_draws_and_the_next_does_not() {
        let last = STRIKES_MAX as u32 - 1;
        let n = last + 2;
        assert!(glyph_1(&cblc_font(n, Some(last), 1, 0)).0);
        assert!(!glyph_1(&cblc_font(n, Some(last + 1), 1, 0)).0);
        assert!(glyph_1(&sbix_font(n, last)).0);
        assert!(!glyph_1(&sbix_font(n, last + 1)).0);
    }

    // Records may overlap, start past their end, or come in any order; each
    // glyph keeps the first record in the list whose range holds it, as a
    // walk through the list finds it. Compared with that walk over random
    // lists, near glyph 0 and near glyph 65,535
    #[test]
    fn each_glyph_keeps_the_first_record_that_holds_it() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % u64::from(n)) as u16
        };
        for round in 0..2_000 {
            let base = if round % 2 == 0 { 0 } else { u16::MAX - 40 };
            let count = usize::from(next(12));
            let records: Vec<(u16, u16, usize)> = (0..count)
                .map(|i| (base + next(41), base + next(41), i))
                .collect();
            let found = ranges(records.iter().copied());
            for pair in found.windows(2) {
                assert!(pair[0].last < pair[1].first, "{found:?}");
            }
            for g in base..=base + 40 {
                let walk = records.iter().find(|r| r.0 <= g && g <= r.1).map(|r| r.2);
                let range = found.iter().find(|r| r.first <= g && g <= r.last);
                assert_eq!(range.map(|r| r.record), walk, "glyph {g} of {records:?}");
            }
        }
    }

    #[test]
    fn the_last_record_read_draws_and_the_next_does_not() {
        let last = RECORDS_MAX as u32 - 1;
        assert!(glyph_1(&cblc_font(1, None, last + 2, last)).0);
        assert!(!glyph_1(&cblc_font(1, None, last + 2, last + 1)).0);
    }
}
