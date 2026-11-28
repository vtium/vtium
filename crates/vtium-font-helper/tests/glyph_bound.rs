//! Fonts that pass the font check but scale a glyph to tens of thousands
//! of pixels. vtium-config draws a glyph whose box is wider or taller than
//! twice the largest cell side, 4,096 pixels, as empty, so neither the
//! cell width nor the rasteriser asks zeno, swash's rasteriser, for a mask
//! of gigabytes or for coordinates past its fixed-point range.
//!
//! One font unit becomes (cell height − 0.5) / (ascent + descent) pixels,
//! so a font whose ascent and descent span one unit draws each unit at
//! almost the cell's height. Each test drives `outline::cell_width` and
//! `outline::rasterise` as the font helper calls them, and checks that
//! they answer without a panic and leave the oversized glyphs empty. A font
//! whose W is past the bound gives no cell width, so its source fails
//! rather than take a cell that its glyphs draw nothing in.

use vtium_font::CELL_SIDE_MAX;
use vtium_font_helper::frit::Hinting;
use vtium_font_helper::names::sound;
use vtium_font_helper::outline::{Outline, OutlineRequest, cell_width, rasterise};
use vtium_testfont::ttf::{self, Glyph};

const DEJAVU: &[u8] = include_bytes!("fixtures/DejaVuSansMono-ascii.ttf");

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([b[at], b[at + 1]]))
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// `font` with the bytes of table `tag` changed by `edit`, in place, so
/// the table keeps its length and the directory stays as it was. The
/// table's checksum goes stale, which no reader vtium-config uses checks.
fn edit_table(font: &[u8], tag: &[u8; 4], edit: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut out = font.to_vec();
    let record = (0..u16_at(font, 4))
        .map(|i| 12 + 16 * i)
        .find(|&r| &font[r..r + 4] == tag)
        .expect("the table is in the font");
    let (start, len) = (u32_at(font, record + 8), u32_at(font, record + 12));
    edit(&mut out[start..start + len]);
    out
}

/// DejaVu Sans Mono's ASCII fixture with its `hhea` ascender set to 1 and
/// its descender to 0, and the `OS/2` flag that says to use the typo
/// metrics cleared, so swash takes the `hhea` values: the ascent and
/// descent span one font unit.
fn dejavu_of_one_unit() -> Vec<u8> {
    let font = edit_table(DEJAVU, b"hhea", |hhea| {
        hhea[4..6].copy_from_slice(&1i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&0i16.to_be_bytes());
    });
    // fsSelection is the u16 at byte 62, and USE_TYPO_METRICS is its bit 7
    edit_table(&font, b"OS/2", |os2| os2[63] &= !0x80)
}

/// Test Mono's shape with an ascent of 1 and a descent of 0, and one glyph
/// for W: the rectangle from (0, 0) to (`x`, `y`) in font units.
fn rectangle_of_one_unit(x: i16, y: i16) -> Vec<u8> {
    let mut f = ttf::test_mono();
    f.ascent = 1;
    f.descent = 0;
    f.glyphs = vec![Glyph::empty(600), Glyph::rect(600, 0, 0, x, y)];
    f.cmap = vec![(0x57, 1)];
    f.colr = Vec::new();
    f.palette = Vec::new();
    f.build()
}

/// Test Mono's shape with an ascent of 1 and a descent of 0, and W a COLR
/// glyph whose one layer is the rectangle from (0, 0) to (`x`, `y`).
fn color_rectangle_of_one_unit(x: i16, y: i16) -> Vec<u8> {
    let mut f = ttf::test_mono();
    f.ascent = 1;
    f.descent = 0;
    f.glyphs = vec![
        Glyph::empty(600),
        Glyph::empty(600),
        Glyph::rect(600, 0, 0, x, y),
    ];
    f.cmap = vec![(0x57, 1)];
    f.colr = vec![(1, vec![(2, 0)])];
    f.palette = vec![[255, 0, 0, 255]];
    f.build()
}

/// Test Emoji's CBDT strike of 40 pixels to the em, with an ascent of 1
/// and a descent of 0, and its bitmap glyph mapped from A as well, so the
/// baseline measure draws it at the size the cell gives with no box to fit.
fn bitmap_of_one_unit() -> Vec<u8> {
    let mut f = ttf::test_emoji();
    f.ascent = 1;
    f.descent = 0;
    f.cmap.push((0x41, 1));
    f.build()
}

fn request(font: Vec<u8>, height: u32) -> OutlineRequest {
    request_in(font, 0, height)
}

/// A request for cells `width` pixels wide and `height` high, as a later
/// source of a frit is drawn in the first source's cell; a width of 0 asks
/// for the font's own.
fn request_in(font: Vec<u8>, width: u32, height: u32) -> OutlineRequest {
    OutlineRequest {
        font: font.into(),
        cells: vec![(width, height)],
        hinting: Hinting::Light,
        ..Default::default()
    }
}

/// Whether `answer` is the error of a font whose W is past the glyph
/// bound, which fails the source rather than give it a cell.
fn w_past_the_bound<T: std::fmt::Debug>(answer: Result<T, String>) -> bool {
    matches!(&answer, Err(e) if e.starts_with("its W draws wider or taller than 4,096 pixels"))
}

/// Whether the glyph mapped from `c` in strike 0 has no ink: every pixel
/// of its cells is 0 and it has no ink box.
fn empty(o: &Outline, c: char) -> bool {
    let s = &o.strikes[0];
    let (_, g) = s
        .map
        .iter()
        .find(|(cp, _)| *cp == u32::from(c))
        .expect("the character is mapped");
    let glyph = &s.glyphs[*g as usize];
    glyph.ink.is_none() && glyph.pixels.iter().all(|&p| p == 0)
}

// DejaVu's W spans about 1,200 font units, so at a cell 64 pixels high it
// would be about 78,000 × 95,000 pixels: a mask whose size overflows u32.
// The font gives no cell width, so the source fails as a whole. Drawn in
// another source's cell, every printable letter draws empty
#[test]
fn a_font_whose_span_is_one_unit_draws_its_letters_empty() {
    let font = dejavu_of_one_unit();
    assert_eq!(sound(&font), Ok(()));
    for height in [16, 64] {
        assert!(w_past_the_bound(cell_width(&[(&font, "")], height)));
        assert!(w_past_the_bound(rasterise(&request(font.clone(), height))));
        let o = rasterise(&request_in(font.clone(), 9, height)).expect("rasterised");
        for c in ['W', 'A', 'g', '_'] {
            assert!(empty(&o, c), "{c} at a cell {height} pixels high");
        }
    }
}

// Measured from its W's advance alone, this font's cell would be 2,048
// pixels wide at a cell 2,048 high, and its 95 printable glyphs, every one
// empty, would hold 398 MB of pixels. The font gives no cell width, so the
// rasteriser answers with the error before it draws a glyph
#[test]
fn a_cover_of_empty_glyphs_holds_no_cells_of_the_largest_size() {
    let font = dejavu_of_one_unit();
    let request = OutlineRequest {
        cells: vec![(0, 16), (0, 440), (0, CELL_SIDE_MAX)],
        cover: vec![(0x20, 0x7e)],
        ..request(font, 16)
    };
    assert!(w_past_the_bound(rasterise(&request)));
}

// A square of 32,767 font units is 247,000 pixels on a side at a cell 8
// pixels high and 507,000 at 16
#[test]
fn a_glyph_of_32767_units_draws_empty() {
    let font = rectangle_of_one_unit(32767, 32767);
    assert_eq!(sound(&font), Ok(()));
    for height in [8, 16] {
        assert!(w_past_the_bound(cell_width(&[(&font, "")], height)));
        let o = rasterise(&request_in(font.clone(), 9, height)).expect("rasterised");
        assert!(empty(&o, 'W'), "W at a cell {height} pixels high");
    }
}

// A rectangle 22,210 units wide and 1 high is 9,761,295 × 440 pixels at a
// cell 440 pixels high. The mask's size wraps in u32 to 2,504 bytes, and
// its x coordinates in 24.8 fixed point pass i32's range
#[test]
fn a_glyph_whose_mask_size_wraps_draws_empty() {
    let font = rectangle_of_one_unit(22210, 1);
    assert_eq!(sound(&font), Ok(()));
    assert!(w_past_the_bound(cell_width(&[(&font, "")], 440)));
    let o = rasterise(&request_in(font, 200, 440)).expect("rasterised");
    assert!(empty(&o, 'W'));
}

// The same rule holds for a glyph only 4,097 pixels wide: one column past
// the bound. At a cell 16 pixels high one font unit is 15.5 pixels, so
// 264 units are 4,092 pixels and 265 units are 4,107.5
#[test]
fn the_bound_is_twice_the_largest_cell_side() {
    let drawn = rectangle_of_one_unit(264, 1);
    assert_eq!(sound(&drawn), Ok(()));
    assert!(cell_width(&[(&drawn, "")], 16).is_ok());
    let o = rasterise(&request(drawn, 16)).expect("rasterised");
    assert!(!empty(&o, 'W'));
    let past = rectangle_of_one_unit(265, 1);
    assert!(w_past_the_bound(cell_width(&[(&past, "")], 16)));
    let o = rasterise(&request_in(past, 9, 16)).expect("rasterised");
    assert!(empty(&o, 'W'));
}

// vtium-config composes a COLR glyph's layers into one premultiplied
// image of the glyph's box, which for a 4,000-unit square at a cell 16
// pixels high is 62,000 pixels on a side
#[test]
fn a_color_glyph_past_the_bound_draws_empty() {
    let font = color_rectangle_of_one_unit(4000, 4000);
    assert_eq!(sound(&font), Ok(()));
    let o = rasterise(&request(font, 16)).expect("rasterised");
    assert!(empty(&o, 'W'));
}

// The baseline measure draws each printable ASCII glyph with no box to fit
// it in, so a color bitmap of 36 pixels at 40 to the em would be scaled to
// the size the cell gives: 2,047,500 pixels to the em at a cell 2,048
// pixels high, a bitmap of 1.8 million pixels on a side and 13 TB. The
// measure draws it as empty instead, and the glyph itself, which is fitted
// to its cells, still draws there
#[test]
fn a_color_bitmap_scaled_past_the_bound_is_not_drawn_to_measure_it() {
    let font = bitmap_of_one_unit();
    assert_eq!(sound(&font), Ok(()));
    let o = rasterise(&request(font, CELL_SIDE_MAX)).expect("rasterised");
    let s = &o.strikes[0];
    let (_, g) = s.map.iter().find(|(cp, _)| *cp == 0x41).expect("mapped");
    assert!(s.glyphs[*g as usize].color);
    assert!(!empty(&o, 'A'));
}
