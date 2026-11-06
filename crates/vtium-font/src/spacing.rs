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

/// `glyph`, drawn at the text band's height, placed in a cell taller by
/// `band` as `fit` says (ADR-1302 D13): `fill` stretches a `bitmap` glyph
/// by repeating its top and bottom rows and scales any other mask
/// vertically, and `natural` and `room` pad it with blank rows. A color
/// glyph is always padded, never stretched. Its ink box, where it has one,
/// is placed the same way. `None` past the glyph's bounds.
pub fn place(glyph: &Glyph, band: Band, fit: Fit, bitmap: bool) -> Option<Glyph> {
    if band.is_none() {
        return Some(glyph.clone());
    }
    let bpp = usize::try_from(glyph.format().bytes_per_pixel()).ok()?;
    let fill = fit == Fit::Fill && glyph.format() == Format::Mask;
    let stretch = |rows: &[u8], width: u32| -> Option<Vec<u8>> {
        let stride = usize::try_from(width).ok()?.checked_mul(bpp)?;
        match (fill, bitmap) {
            (true, true) => repeat_rows(rows, stride, band),
            (true, false) => scale_rows(rows, stride, band),
            (false, _) => pad_rows(rows, stride, band),
        }
    };
    let height = glyph.height().checked_add(band.added())?;
    let pixels = stretch(glyph.pixels(), glyph.width())?;
    let out = Glyph::from_pixels(glyph.width(), height, glyph.format(), pixels)?
        .with_cells(glyph.cells());
    match glyph.ink() {
        None => Some(out),
        Some(ink) => out.with_ink(ink.left(), ink.width(), stretch(ink.pixels(), ink.width())?),
    }
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
            .with_ink(-1, 5, vec![255; 10])
            .unwrap();
        let p = place(&g, Band { above: 1, below: 0 }, Fit::Natural, true).unwrap();
        assert_eq!((p.width(), p.height(), p.cells()), (4, 3, Width::Wide));
        let ink = p.ink().unwrap();
        assert_eq!((ink.left(), ink.width()), (-1, 5));
        assert_eq!(ink.pixels()[..5], [0; 5]);
        assert_eq!(ink.pixels()[5..], [255; 10]);
    }
}
