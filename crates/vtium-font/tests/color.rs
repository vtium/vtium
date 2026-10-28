//! A set whose cell holds a strike in each pixel format (ADR-1200 D2,
//! D13): 1-bit glyphs from a bitmap source, R8 glyphs from an outline font
//! and premultiplied RGBA glyphs from a color font, as `vtium-vitrify`
//! builds a set from a code font, its fallbacks and color emoji. Each
//! glyph draws in its own format, a color glyph two cells wide among them.

mod common;

use std::sync::Arc;

use common::with_section;
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetError, SetFace, StrikeBuilder};
use vtium_font::{
    Cell, Decorations, Face, Font, Format, RangeCoverage, Style, Width, sprite::Range,
};

const CELL: Cell = Cell {
    width: 4,
    height: 6,
};

const BOLD: Style = Style {
    bold: true,
    italic: false,
};

/// A wide RGBA glyph, 8x6: each pixel's alpha its column's step, its
/// colors premultiplied by it.
fn color_glyph() -> Vec<u8> {
    let mut px = Vec::new();
    for y in 0..6u32 {
        for x in 0..8u32 {
            let a = (x * 32 + 31) as u8;
            let (r, g, b) = (a, (u32::from(a) * y / 6) as u8, a / 2);
            px.extend([r, g, b, a]);
        }
    }
    px
}

/// The three strikes at [`CELL`], in an order the writer sorts.
fn mixed_set() -> Vec<u8> {
    let decorations = Decorations {
        underline: 5,
        underline_thickness: 1,
        strike: 3,
        strike_thickness: 1,
    };
    let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default())
        .baseline(5)
        .decorations(decorations);
    let smile = color.glyph(Width::Wide, color_glyph());
    color.map('\u{1f600}', smile);
    let mut outline = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default())
        .baseline(5)
        .decorations(decorations);
    let a = outline.glyph(Width::Narrow, (0..24).map(|p| p * 10).collect());
    outline.map('A', a);
    let mut bitmap = StrikeBuilder::new(PixelFormat::Bits, CELL, Style::default())
        .baseline(5)
        .decorations(decorations);
    let line = bitmap.glyph(Width::Narrow, vec![0, 0, 0xf0, 0, 0, 0]);
    bitmap.map('\u{2500}', line);
    SetBuilder::new("mixed")
        .strike(color)
        .strike(outline)
        .strike(bitmap)
        .write()
        .expect("the set writes")
}

fn face() -> SetFace {
    let set = Arc::new(Set::from_bytes(mixed_set()).expect("the set reads"));
    SetFace::new(set, CELL.height).expect("a face")
}

// covers: text.emoji
#[test]
fn each_format_at_one_cell_draws_its_own_glyphs() {
    let set = Set::from_bytes(mixed_set()).expect("the set reads");
    let formats: Vec<_> = set.strikes().map(|s| (s.cell, s.format)).collect();
    assert_eq!(
        formats,
        [
            (CELL, PixelFormat::Bits),
            (CELL, PixelFormat::R8),
            (CELL, PixelFormat::Rgba)
        ]
    );
    assert_eq!(set.cells(), [CELL], "one cell, however many formats");

    let face = face();
    for c in ['A', '\u{2500}', '\u{1f600}'] {
        assert!(face.has_glyph(c), "{c:?}");
    }
    assert!(!face.has_glyph('B'));

    let a = face.draw(&['A'], Style::default()).expect("A");
    assert_eq!((a.format(), a.cells()), (Format::Mask, Width::Narrow));
    assert_eq!(a.pixels(), (0..24).map(|p| p * 10).collect::<Vec<u8>>());
    let line = face.draw(&['\u{2500}'], Style::default()).expect("a line");
    assert_eq!(line.format(), Format::Mask);
    assert_eq!(&line.pixels()[8..12], &[0xff; 4]);
    let smile = face
        .draw(&['\u{1f600}'], Style::default())
        .expect("a smile");
    assert_eq!((smile.format(), smile.cells()), (Format::Rgba, Width::Wide));
    assert_eq!((smile.width(), smile.height()), (8, 6));
    assert_eq!(smile.pixels(), color_glyph());
    // bold widens a mask; a color glyph is drawn as it is
    let bold = face.draw(&['\u{1f600}'], BOLD).expect("a bold smile");
    assert_eq!(bold.pixels(), color_glyph());
}

// covers: text.emoji
#[test]
fn a_color_glyph_reaches_the_font_two_cells_wide_and_scales() {
    let font = Font::new(face());
    let wide = font
        .glyph(&['\u{1f600}'], Width::Wide, Style::default())
        .expect("a wide glyph");
    assert_eq!((wide.format(), wide.cells()), (Format::Rgba, Width::Wide));
    assert_eq!(wide.pixels(), color_glyph());

    let set = Arc::new(Set::from_bytes(mixed_set()).expect("the set reads"));
    let big = SetFace::new(set, 2 * CELL.height).expect("a face");
    assert_eq!(big.scale(), 2);
    let smile = big.draw(&['\u{1f600}'], Style::default()).expect("a smile");
    assert_eq!((smile.width(), smile.height()), (16, 12));
    let src = color_glyph();
    for (y, x) in [(0, 0), (5, 3), (11, 15)] {
        let at = ((y / 2) * 8 + x / 2) * 4;
        assert_eq!(smile.pixel(x as u32, y as u32), Some(&src[at..at + 4]));
    }
}

#[test]
fn every_strike_at_the_cell_counts_toward_its_coverage() {
    let face = face();
    let boxes = face.coverage(Range::BoxDrawing);
    assert_eq!(boxes.covered, 1, "U+2500, from the 1-bit strike");
    assert_eq!(
        boxes.total,
        RangeCoverage::count(Range::BoxDrawing, |_| true).total
    );
    assert_eq!(
        face.decorations(),
        Decorations {
            underline: 5,
            underline_thickness: 1,
            strike: 3,
            strike_thickness: 1,
        }
    );
    // glyph 0 of the first strike that holds a glyph: the 1-bit one's
    let last = face.last_resort(Style::default()).expect("a last resort");
    assert_eq!(last.format(), Format::Mask);
}

#[test]
fn two_strikes_of_one_style_cell_and_format_are_refused() {
    let one = || {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let g = s.glyph(Width::Narrow, vec![0; 24]);
        s.map('A', g);
        s
    };
    assert!(
        SetBuilder::new("x")
            .strike(one())
            .strike(one())
            .write()
            .is_err()
    );
    // the R8 strike, second in STRK, made 1-bit like the first: refused
    let set = mixed_set();
    let repeated = with_section(&set, b"STRK", |s| s[8 + 24 + 1] = 0);
    assert!(matches!(
        Set::from_bytes(repeated),
        Err(SetError::Malformed("strikes out of order or repeated"))
    ));
}
