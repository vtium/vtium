//! Added line spacing (ADR-1302 D13): the user may make each row taller
//! by a whole percentage of the text's height, and each kind of glyph uses
//! the added height in its own way.
//!
//! The added height is split into two bands, one above the text and one
//! below it, with an odd pixel going above ([`Band`]). The middle of the
//! row, the text's own height, is the text band. A glyph is drawn at the
//! text band's height and then placed in the taller cell by its [`Fit`]:
//! a `natural` glyph keeps its size and gets blank rows above and below it,
//! and a `fill` glyph stretches to the whole cell, a bitmap glyph by
//! repeating its top and bottom pixel rows outward and an outline glyph by
//! scaling vertically. A color glyph always keeps its size.
//!
//! The console and the glyph helper place glyphs with the same functions,
//! so a glyph the helper draws and one the console draws itself land on
//! the same rows.

use crate::bounds::CELL_SIDE_MAX;
use crate::glyph::{Format, Glyph};

/// The most line spacing a frit or vtium.conf may ask for: 100 percent of
/// the text's height (ADR-1302 D13).
pub const LINE_SPACING_MAX: u32 = 100;

/// The rows added above and below the text band of each row.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Band {
    pub above: u32,
    pub below: u32,
}

impl Band {
    /// No rows added: the cell is the text's own height.
    pub const NONE: Band = Band { above: 0, below: 0 };

    /// The band that `percent` of line spacing adds to text `text` pixels
    /// high: `percent` of `text`, at most [`LINE_SPACING_MAX`], rounded to
    /// the nearest whole pixel with a half rounding up, and split evenly
    /// with the odd pixel above. The cell never grows past
    /// [`CELL_SIDE_MAX`]: the added height is cut to what is left under it.
    pub fn of(text: u32, percent: u32) -> Band {
        let percent = u64::from(percent.min(LINE_SPACING_MAX));
        let added = u64::from(text)
            .saturating_mul(percent)
            .saturating_add(50)
            .checked_div(100)
            .unwrap_or(0);
        let room = CELL_SIDE_MAX.saturating_sub(text);
        let added = u32::try_from(added).unwrap_or(u32::MAX).min(room);
        let below = added.checked_div(2).unwrap_or(0);
        Band {
            above: added.saturating_sub(below),
            below,
        }
    }

    /// The rows added in all.
    pub fn added(self) -> u32 {
        self.above.saturating_add(self.below)
    }

    /// Whether no row is added.
    pub fn is_none(self) -> bool {
        self.added() == 0
    }
}

/// A cell and the rows line spacing adds to it: the key a frit's glyphs
/// are asked for, generated and held by. One cell size can carry two bands
/// on two heads, as when one head's `font-size` and line spacing reach the
/// size another head's frit states with no spacing, and the two draw
/// different glyphs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpacedCell {
    pub cell: crate::face::Cell,
    pub band: Band,
}

impl From<crate::face::Cell> for SpacedCell {
    /// `cell` with no rows added, as [`SpacedCell::plain`] gives it.
    fn from(cell: crate::face::Cell) -> SpacedCell {
        SpacedCell::plain(cell)
    }
}

impl SpacedCell {
    /// `cell` with no rows added.
    pub const fn plain(cell: crate::face::Cell) -> SpacedCell {
        SpacedCell {
            cell,
            band: Band::NONE,
        }
    }
}

/// How a set of characters uses the taller row (ADR-1302 D13, a frit's
/// `fit=`, ADR-1200 D1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Fit {
    /// Stretched to the whole cell, so that line and block characters
    /// still join from row to row.
    Fill,
    /// The font's size, on its baseline in the text band, with the added
    /// height blank above and below it.
    Natural,
    /// The letter as under `Natural`, and its marks may also use the
    /// added height above and below the text band.
    Room,
}

/// The characters that are `fill` when a frit gives their set no `fit=`:
/// box drawing, block elements and shades, Braille, Powerline, and the
/// two Legacy Computing blocks, which hold the sextants and the octants.
const FILL_DEFAULT: &[(u32, u32)] = &[
    (0x2500, 0x259f),
    (0x2800, 0x28ff),
    (0xe0a0, 0xe0d7),
    (0x1cc00, 0x1cebf),
    (0x1fb00, 0x1fbff),
];

/// The scripts whose characters are `room` when a frit gives their set no
/// `fit=`: Hebrew, Arabic, Syriac, Thaana and N'Ko, with their
/// presentation forms and supplements.
const ROOM_DEFAULT: &[(u32, u32)] = &[
    (0x0590, 0x07ff),
    (0x0860, 0x08ff),
    (0xfb1d, 0xfdff),
    (0xfe70, 0xfeff),
];

fn within(spans: &[(u32, u32)], c: u32) -> bool {
    spans
        .iter()
        .any(|&(first, last)| (first..=last).contains(&c))
}

impl Fit {
    /// Each value's name as a frit spells it.
    pub const NAMES: [(&'static str, Fit); 3] = [
        ("fill", Fit::Fill),
        ("natural", Fit::Natural),
        ("room", Fit::Room),
    ];

    /// The value `name` spells, or `None`.
    pub fn parse(name: &str) -> Option<Fit> {
        Fit::NAMES
            .iter()
            .find(|&&(n, _)| n == name)
            .map(|&(_, fit)| fit)
    }

    /// The value's name as a frit spells it.
    pub fn name(self) -> &'static str {
        match self {
            Fit::Fill => "fill",
            Fit::Natural => "natural",
            Fit::Room => "room",
        }
    }

    /// How `c` uses the taller row when its set has no `fit=`: line and
    /// block characters are `fill`; Arabic, Hebrew, Syriac, Thaana, N'Ko
    /// and a combining mark of any script, which `mark` says `c` is, are
    /// `room`; everything else is `natural`.
    pub fn default_for(c: u32, mark: bool) -> Fit {
        if within(FILL_DEFAULT, c) {
            Fit::Fill
        } else if mark || within(ROOM_DEFAULT, c) {
            Fit::Room
        } else {
            Fit::Natural
        }
    }
}

/// `rows`, whole rows of `stride` bytes each, with `band.above` blank rows
/// over them and `band.below` blank rows under them. `None` for a stride
/// of 0 or rows that are not whole.
pub fn pad_rows(rows: &[u8], stride: usize, band: Band) -> Option<Vec<u8>> {
    if stride == 0 || !rows.len().is_multiple_of(stride) {
        return None;
    }
    let blank = |n: u32| -> Option<Vec<u8>> {
        Some(vec![0u8; usize::try_from(n).ok()?.checked_mul(stride)?])
    };
    let mut out = blank(band.above)?;
    out.extend_from_slice(rows);
    out.extend(blank(band.below)?);
    Some(out)
}

/// `rows`, whole rows of `stride` bytes each, with their first row
/// repeated `band.above` times over them and their last row repeated
/// `band.below` times under them: a bitmap glyph stretched to fill the
/// taller cell. `None` for a stride of 0, no rows, or rows that are not
/// whole.
pub fn repeat_rows(rows: &[u8], stride: usize, band: Band) -> Option<Vec<u8>> {
    if stride == 0 || !rows.len().is_multiple_of(stride) {
        return None;
    }
    let first = rows.get(..stride)?;
    let last = rows.get(rows.len().checked_sub(stride)?..)?;
    let mut out = Vec::with_capacity(
        rows.len()
            .checked_add(usize::try_from(band.added()).ok()?.checked_mul(stride)?)?,
    );
    for _ in 0..band.above {
        out.extend_from_slice(first);
    }
    out.extend_from_slice(rows);
    for _ in 0..band.below {
        out.extend_from_slice(last);
    }
    Some(out)
}

/// `rows` of coverage, one byte a pixel and `width` pixels across, scaled
/// vertically to `band.added()` more rows: an outline glyph stretched to
/// fill the taller cell. Each new row is the average of the old rows it
/// covers, each weighted by how much of it the new row covers. `None` for
/// a width of 0, no rows, or rows that are not whole.
pub fn scale_rows(rows: &[u8], width: usize, band: Band) -> Option<Vec<u8>> {
    if width == 0 || rows.is_empty() || !rows.len().is_multiple_of(width) {
        return None;
    }
    let from = u64::try_from(rows.len().checked_div(width)?).ok()?;
    let to = from.checked_add(u64::from(band.added()))?;
    let mut out = Vec::with_capacity(usize::try_from(to).ok()?.checked_mul(width)?);
    let old: Vec<&[u8]> = rows.chunks_exact(width).collect();
    for j in 0..to {
        // the new row covers old rows [j * from, (j + 1) * from) in units
        // of 1/to of an old row
        let start = j.checked_mul(from)?;
        let end = start.checked_add(from)?;
        let first = start.checked_div(to)?;
        let last = end.checked_sub(1)?.checked_div(to)?;
        for x in 0..width {
            let mut sum = 0u64;
            for i in first..=last {
                let top = i.checked_mul(to)?.max(start);
                let bottom = i.checked_add(1)?.checked_mul(to)?.min(end);
                let weight = bottom.saturating_sub(top);
                let px = old.get(usize::try_from(i).ok()?)?.get(x)?;
                sum = sum.checked_add(weight.checked_mul(u64::from(*px))?)?;
            }
            // the weights add up to `from`; round to the nearest
            let value = sum.checked_add(from.checked_div(2)?)?.checked_div(from)?;
            out.push(u8::try_from(value.min(255)).ok()?);
        }
    }
    Some(out)
}

/// An ink box's rows: `width` columns from `left` and `height` rows from
/// `top`, each offset from the glyph's first cell's top left corner and 0
/// or negative, one byte of coverage a pixel, row by row ([`crate::Ink`]).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct InkRows {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// `glyph`, drawn at the text band's height, placed in a cell taller by
/// `band` as `fit` says ([`place_rows`]). `None` past the glyph's bounds.
pub fn place(glyph: &Glyph, band: Band, fit: Fit, bitmap: bool) -> Option<Glyph> {
    if band.is_none() {
        return Some(glyph.clone());
    }
    let ink = glyph.ink().map(|ink| InkRows {
        left: ink.left(),
        top: ink.top(),
        width: ink.width(),
        height: ink.height(),
        pixels: ink.pixels().to_vec(),
    });
    let size = (glyph.width(), glyph.height());
    let (pixels, ink) = place_rows(
        glyph.pixels(),
        size,
        glyph.format(),
        ink.as_ref(),
        (band, fit, bitmap),
    )?;
    let height = glyph.height().checked_add(band.added())?;
    let out = Glyph::from_pixels(glyph.width(), height, glyph.format(), pixels)?
        .with_cells(glyph.cells());
    Some(match ink {
        Some(ink) => out
            .clone()
            .with_ink((ink.left, ink.top), (ink.width, ink.height), ink.pixels)
            .unwrap_or(out),
        None => out,
    })
}

/// A glyph's `rows`, `size.0` pixels across and `size.1` down in
/// `format`, drawn at the text band's height, and its ink box `ink`,
/// placed in a cell taller by `band` as `fit` says (ADR-1302 D13): `fill`
/// stretches a `bitmap` glyph by repeating its top and bottom rows and
/// scales any other mask vertically, and `natural` and `room` pad it with
/// blank rows. A color glyph is always padded, never stretched. The
/// console and the glyph helper both place glyphs with this function.
/// Gives the placed rows and the placed box, `None` for the box when the
/// placed ink stays inside the taller cell; `None` past the bounds.
///
/// A stretched glyph's box is first cut to the text band's rows and then
/// stretched as the glyph is. A padded glyph keeps its ink where it was
/// drawn (ADR-1306 D8): the ink its box holds above or below the text band
/// now lies in the rows the band adds, which are part of its cell, so it
/// joins the glyph's own pixels, and the box keeps only what still passes
/// the taller cell.
pub fn place_rows(
    rows: &[u8],
    (width, height): (u32, u32),
    format: Format,
    ink: Option<&InkRows>,
    (band, fit, bitmap): (Band, Fit, bool),
) -> Option<(Vec<u8>, Option<InkRows>)> {
    let bpp = usize::try_from(format.bytes_per_pixel()).ok()?;
    let fill = fit == Fit::Fill && format == Format::Mask;
    let stretch = |rows: &[u8], width: u32, bpp: usize| -> Option<Vec<u8>> {
        let stride = usize::try_from(width).ok()?.checked_mul(bpp)?;
        match (fill, bitmap) {
            (true, true) => repeat_rows(rows, stride, band),
            (true, false) => scale_rows(rows, stride, band),
            (false, _) => pad_rows(rows, stride, band),
        }
    };
    let pixels = stretch(rows, width, bpp)?;
    let Some(ink) = ink else {
        return Some((pixels, None));
    };
    let whole = height.checked_add(band.added())?;
    if fill {
        // the text band's rows of the box, stretched with the glyph
        let stride = usize::try_from(ink.width).ok()?;
        let first = usize::try_from(ink.top.checked_neg()?).ok()?;
        let rows = usize::try_from(height).ok()?;
        let band_rows = ink
            .pixels
            .get(first.checked_mul(stride)?..first.checked_add(rows)?.checked_mul(stride)?)?;
        let boxed = InkRows {
            left: ink.left,
            top: 0,
            width: ink.width,
            height: whole,
            pixels: stretch(band_rows, ink.width, 1)?,
        };
        let past = (boxed.left, boxed.width) != (0, width);
        return Some((pixels, past.then_some(boxed)));
    }
    padded_ink(width, whole, ink, band)
}

/// A glyph `width` pixels across, padded into a cell `whole` pixels high,
/// given the ink of its box `ink` moved down by the rows `band` adds
/// above: its own pixels are that ink cut to its cells, and its box is the
/// ink and its cells together, kept only where the ink passes them.
fn padded_ink(
    width: u32,
    whole: u32,
    ink: &InkRows,
    band: Band,
) -> Option<(Vec<u8>, Option<InkRows>)> {
    let cell_h = i64::from(whole);
    let moved = i64::from(ink.top).checked_add(i64::from(band.above))?;
    let top = moved.min(0);
    let bottom = moved.checked_add(i64::from(ink.height))?.max(cell_h);
    let stride = usize::try_from(ink.width).ok()?;
    let rows = usize::try_from(bottom.checked_sub(top)?).ok()?;
    let mut all = vec![0u8; rows.checked_mul(stride)?];
    let at = usize::try_from(moved.checked_sub(top)?)
        .ok()?
        .checked_mul(stride)?;
    all.get_mut(at..at.checked_add(ink.pixels.len())?)?
        .copy_from_slice(&ink.pixels);
    // the cells' own pixels: the box's rows of the cell and its columns of
    // the glyph's cells
    let x0 = usize::try_from(ink.left.checked_neg()?).ok()?;
    let w = usize::try_from(width).ok()?;
    let y0 = usize::try_from(top.checked_neg()?).ok()?;
    let mut own = Vec::with_capacity(w.checked_mul(usize::try_from(cell_h).ok()?)?);
    for y in 0..usize::try_from(cell_h).ok()? {
        let row = y0.checked_add(y)?.checked_mul(stride)?.checked_add(x0)?;
        own.extend_from_slice(all.get(row..row.checked_add(w)?)?);
    }
    let boxed = InkRows {
        left: ink.left,
        top: i32::try_from(top).ok()?,
        width: ink.width,
        height: u32::try_from(rows).ok()?,
        pixels: all,
    };
    let past = (boxed.left, boxed.top, boxed.width, boxed.height) != (0, 0, width, whole);
    Some((own, past.then_some(boxed)))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::face::Width;

    #[test]
    fn the_band_is_a_rounded_percentage_split_evenly_with_the_odd_pixel_above() {
        assert_eq!(Band::of(16, 0), Band::NONE);
        // 25% of 16 is 4: two above, two below
        assert_eq!(Band::of(16, 25), Band { above: 2, below: 2 });
        // 20% of 16 is 3.2, so 3: the odd pixel above
        assert_eq!(Band::of(16, 20), Band { above: 2, below: 1 });
        // 10% of 15 is 1.5, which rounds up to 2
        assert_eq!(Band::of(15, 10), Band { above: 1, below: 1 });
        // 10% of 14 is 1.4, so 1, above
        assert_eq!(Band::of(14, 10), Band { above: 1, below: 0 });
        assert_eq!(
            Band::of(20, 100),
            Band {
                above: 10,
                below: 10
            }
        );
        // past 100 is held to 100
        assert_eq!(Band::of(20, 250), Band::of(20, 100));
        // the cell never passes its bound
        let b = Band::of(CELL_SIDE_MAX - 3, 100);
        assert_eq!(b.added(), 3);
        assert_eq!(Band::of(CELL_SIDE_MAX, 50), Band::NONE);
        for h in 1..200 {
            for p in 0..=100 {
                let b = Band::of(h, p);
                assert!(b.above == b.below || b.above == b.below + 1, "{h} {p}");
            }
        }
    }

    #[test]
    fn line_and_block_characters_fill_scripts_with_marks_have_room_the_rest_is_natural() {
        for c in [
            0x2500, 0x257f, 0x2588, 0x2591, 0x2800, 0x28ff, 0xe0b0, 0x1fb00, 0x1fb3c, 0x1cd00,
        ] {
            assert_eq!(Fit::default_for(c, false), Fit::Fill, "{c:x}");
        }
        for c in [0x05d0, 0x0628, 0x0710, 0x0780, 0x07ca, 0xfe8f, 0xfb2a] {
            assert_eq!(Fit::default_for(c, false), Fit::Room, "{c:x}");
        }
        assert_eq!(Fit::default_for(0x0301, true), Fit::Room);
        for c in [u32::from('A'), u32::from('0'), 0x1f600, 0x4e00, 0x25a1] {
            assert_eq!(Fit::default_for(c, false), Fit::Natural, "{c:x}");
        }
        for (name, fit) in Fit::NAMES {
            assert_eq!(Fit::parse(name), Some(fit));
            assert_eq!(fit.name(), name);
        }
        assert_eq!(Fit::parse("Fill"), None);
    }

    fn mask(width: u32, rows: &[&[u8]]) -> Glyph {
        Glyph::from_pixels(
            width,
            u32::try_from(rows.len()).unwrap(),
            Format::Mask,
            rows.concat(),
        )
        .unwrap()
    }

    fn rows(g: &Glyph) -> Vec<Vec<u8>> {
        g.rows().map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn a_natural_glyph_keeps_its_rows_with_blank_rows_above_and_below() {
        let g = mask(2, &[&[255, 0], &[0, 255], &[9, 9]]);
        let band = Band { above: 2, below: 1 };
        for fit in [Fit::Natural, Fit::Room] {
            let p = place(&g, band, fit, true).unwrap();
            assert_eq!(
                rows(&p),
                [
                    vec![0, 0],
                    vec![0, 0],
                    vec![255, 0],
                    vec![0, 255],
                    vec![9, 9],
                    vec![0, 0]
                ]
            );
        }
        assert_eq!(place(&g, Band::NONE, Fit::Fill, true).unwrap(), g);
    }

    #[test]
    fn a_bitmap_fill_glyph_repeats_its_top_and_bottom_rows() {
        let g = mask(2, &[&[0, 255], &[255, 255], &[255, 0]]);
        let p = place(&g, Band { above: 2, below: 1 }, Fit::Fill, true).unwrap();
        assert_eq!(
            rows(&p),
            [
                vec![0, 255],
                vec![0, 255],
                vec![0, 255],
                vec![255, 255],
                vec![255, 0],
                vec![255, 0]
            ]
        );
    }

    #[test]
    fn an_outline_fill_glyph_scales_vertically() {
        // a solid column scales to a solid column, a line through the
        // middle of a 4-row glyph to a 6-row glyph keeps its share of ink
        let g = mask(1, &[&[255], &[255], &[255], &[255]]);
        let p = place(&g, Band { above: 1, below: 1 }, Fit::Fill, false).unwrap();
        assert_eq!(rows(&p), vec![vec![255]; 6]);
        let g = mask(1, &[&[0], &[255], &[255], &[0]]);
        let p = place(&g, Band { above: 1, below: 1 }, Fit::Fill, false).unwrap();
        let col: Vec<u8> = rows(&p).concat();
        assert_eq!(col.len(), 6);
        // the ink is in the middle: the first and last rows are clear
        assert_eq!((col[0], col[5]), (0, 0));
        let ink: u32 = col.iter().map(|&v| u32::from(v)).sum();
        assert!((ink as i64 - 3 * 255).abs() <= 3, "{col:?}");
        // the top row of a 2-row glyph reaches 3 rows of 4
        let g = mask(1, &[&[255], &[0]]);
        let p = scale_rows(g.pixels(), 1, Band { above: 1, below: 1 }).unwrap();
        assert_eq!(p, [255, 255, 0, 0]);
    }

    #[test]
    fn a_color_glyph_is_never_stretched() {
        let g = Glyph::from_pixels(1, 2, Format::Rgba, vec![9, 9, 9, 9, 1, 2, 3, 4]).unwrap();
        let p = place(&g, Band { above: 1, below: 1 }, Fit::Fill, false).unwrap();
        assert_eq!(
            p.pixels(),
            [0, 0, 0, 0, 9, 9, 9, 9, 1, 2, 3, 4, 0, 0, 0, 0].as_slice()
        );
        assert_eq!(p.format(), Format::Rgba);
    }

    #[test]
    fn a_wide_glyphs_cells_and_ink_box_are_kept() {
        let g = Glyph::mask(4, 2)
            .unwrap()
            .with_cells(Width::Wide)
            .with_ink((-1, 0), (5, 2), vec![255; 10])
            .unwrap();
        let p = place(&g, Band { above: 1, below: 0 }, Fit::Natural, true).unwrap();
        assert_eq!((p.width(), p.height(), p.cells()), (4, 3, Width::Wide));
        let ink = p.ink().unwrap();
        assert_eq!((ink.left(), ink.width()), (-1, 5));
        assert_eq!(ink.pixels()[..5], [0; 5]);
        assert_eq!(ink.pixels()[5..], [255; 10]);
    }

    // ADR-1306 D8: a natural glyph's ink above its text band lies in the
    // rows the band adds, which are its cell's, so it joins the glyph's own
    // pixels; what passes the taller cell stays in the box. A 2x2 glyph
    // whose box holds a row above the band in the second column and two
    // rows above it in the first, placed with one row added above, owns
    // the near row and keeps the far one in a box a row above the cell
    #[test]
    fn a_natural_glyphs_ink_above_its_band_moves_into_the_added_rows() {
        let g = Glyph::from_pixels(2, 2, Format::Mask, vec![1, 2, 3, 4])
            .unwrap()
            .with_ink((0, -2), (2, 4), vec![9, 0, 8, 8, 1, 2, 3, 4])
            .unwrap();
        for fit in [Fit::Natural, Fit::Room] {
            let p = place(&g, Band { above: 1, below: 1 }, fit, false).unwrap();
            assert_eq!(p.height(), 4);
            assert_eq!(p.pixels(), [8, 8, 1, 2, 3, 4, 0, 0].as_slice(), "{fit:?}");
            let ink = p.ink().unwrap();
            assert_eq!(
                (ink.left(), ink.top(), ink.width(), ink.height()),
                (0, -1, 2, 5)
            );
            assert_eq!(ink.pixels(), [9, 0, 8, 8, 1, 2, 3, 4, 0, 0].as_slice());
        }
        // all of it inside the taller cell: no box is left
        let near = Glyph::from_pixels(2, 2, Format::Mask, vec![1, 2, 3, 4])
            .unwrap()
            .with_ink((0, -1), (2, 3), vec![8, 8, 1, 2, 3, 4])
            .unwrap();
        let p = place(&near, Band { above: 1, below: 0 }, Fit::Natural, false).unwrap();
        assert_eq!(p.pixels(), [8, 8, 1, 2, 3, 4].as_slice());
        assert!(p.ink().is_none());
    }

    // a stretched glyph's box keeps only the text band's rows, stretched
    // with it
    #[test]
    fn a_filled_glyphs_box_is_cut_to_its_band_and_stretched() {
        let g = Glyph::from_pixels(1, 2, Format::Mask, vec![1, 2])
            .unwrap()
            .with_ink((-1, -1), (2, 3), vec![5, 5, 0, 1, 0, 2])
            .unwrap();
        let p = place(&g, Band { above: 1, below: 1 }, Fit::Fill, true).unwrap();
        assert_eq!(p.pixels(), [1, 1, 2, 2].as_slice());
        let ink = p.ink().unwrap();
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (-1, 0, 2, 4)
        );
        assert_eq!(ink.pixels(), [0, 1, 0, 1, 0, 2, 0, 2].as_slice());
    }
}
