//! The lines of a superscript or subscript cell (ADR-1202 D8): one set for
//! each script, from the face's own source, placed where the script
//! glyph's own box lies.

#![cfg(feature = "unifont")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]

use vtium_font::script::{Rect, lines};
use vtium_font::spacing::Band;
use vtium_font::{
    Cell, Decorations, Face, Font, Glyph, Kind, Origin, Script, Style, Unifont, Width,
};

/// The rows D8's table gives a cell: the underline's top row, the strike's
/// top row, the overline's row, and the thickness of each.
fn rows(d: Decorations) -> (u32, u32, u32, u32, u32) {
    (
        d.underline,
        d.strike,
        d.overline,
        d.underline_thickness,
        d.strike_thickness,
    )
}

// D8's table: unifont's normal, superscript and subscript lines at 1x
// (script glyphs by D4), 2x (by D3 at 1x) and 3x (by D3 at 2x)
// covers: sgr.script
#[test]
fn unifonts_script_lines_are_the_rows_of_d8s_table() {
    for (height, normal, sup, sub) in [
        (16, (15, 8, 0, 1, 1), (10, 5, 0, 1, 1), (15, 10, 5, 1, 1)),
        (32, (30, 16, 0, 2, 2), (15, 8, 0, 1, 1), (31, 24, 16, 1, 1)),
        (48, (45, 24, 0, 3, 3), (30, 16, 0, 2, 2), (46, 32, 16, 2, 2)),
    ] {
        let font = Font::new(Unifont::new(height));
        assert_eq!(font.cell().height, height);
        assert_eq!(
            rows(font.script_decorations(Script::Baseline)),
            normal,
            "{height}"
        );
        assert_eq!(rows(font.decorations()), normal, "{height}");
        assert_eq!(
            rows(font.script_decorations(Script::Super)),
            sup,
            "super at {height}"
        );
        assert_eq!(
            rows(font.script_decorations(Script::Sub)),
            sub,
            "sub at {height}"
        );
    }
}

/// A 1-bit face 8x16 drawn at its own size whose lines are its own, not
/// the plain ones, with `band` added by line spacing.
struct Stated {
    lines: Decorations,
    band: Band,
}

impl Face for Stated {
    fn cell(&self) -> Cell {
        Cell {
            width: 8,
            height: 16 + self.band.added(),
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, _c: char) -> bool {
        true
    }
    fn draw(&self, _cluster: &[char], _style: Style) -> Option<Glyph> {
        Glyph::mask(8, self.cell().height)
    }
    fn decorations(&self) -> Decorations {
        Decorations {
            underline: self.lines.underline + self.band.above,
            strike: self.lines.strike + self.band.above,
            ..self.lines
        }
    }
    fn band(&self) -> Band {
        self.band
    }
    fn origin(&self, _cluster: &[char], _style: Style) -> Origin {
        Origin {
            kind: Kind::Bitmap,
            own: Some(Rect {
                left: 0,
                top: self.band.above,
                width: 8,
                height: 16,
            }),
            ..Origin::default()
        }
    }
}

const STATED: Decorations = Decorations {
    underline: 12,
    underline_thickness: 3,
    strike: 6,
    strike_thickness: 2,
    overline: 0,
};

/// The rows from `top` that a glyph whose rows `from..from + rows` are ink
/// across the whole cell has ink in once D4 makes it small in `script`.
fn d4_rows(face: &Stated, script: Script, from: u32, rows: u32) -> (u32, u32) {
    let h = face.cell().height;
    let mut px = vec![0u8; (8 * h) as usize];
    for y in from..from + rows {
        for x in 0..8 {
            px[(y * 8 + x) as usize] = 255;
        }
    }
    let g = Glyph::from_pixels(8, h, vtium_font::Format::Mask, px).unwrap();
    let small = g
        .scripted(script, &face.origin(&[], Style::default()), face.band)
        .unwrap();
    let lit: Vec<u32> = (0..h)
        .filter(|&y| small.pixel(3, y).unwrap()[0] != 0)
        .collect();
    (lit[0], lit.len() as u32)
}

// D8 step 3: D4's map takes a font's stated lines to the rows where D4
// takes the glyph's pixels from the same rows, with and without line
// spacing
// covers: sgr.script
#[test]
fn d4s_map_takes_each_line_where_it_takes_the_glyphs_rows() {
    for band in [Band::NONE, Band { above: 3, below: 2 }] {
        let face = Stated {
            lines: STATED,
            band,
        };
        let font = Font::new(Stated {
            lines: STATED,
            band,
        });
        for script in [Script::Super, Script::Sub] {
            let d = font.script_decorations(script);
            let ul = d4_rows(&face, script, STATED.underline + band.above, 3);
            let st = d4_rows(&face, script, STATED.strike + band.above, 2);
            assert_eq!(
                (d.underline, d.underline_thickness),
                ul,
                "{script:?} {band:?}"
            );
            assert_eq!((d.strike, d.strike_thickness), st, "{script:?} {band:?}");
        }
    }
}

// D8 step 4 and D1: line spacing moves a script set with the script box:
// a superscript's overline on the text band's top row, a subscript's on
// the top row of the small box against the band's bottom
// covers: sgr.script
#[test]
fn line_spacing_moves_a_script_set_with_the_script_box() {
    let lines = |band: Band| {
        let font = Font::new(Stated {
            lines: Decorations::for_height(16),
            band,
        });
        (
            font.script_decorations(Script::Super),
            font.script_decorations(Script::Sub),
        )
    };
    let (sup, sub) = lines(Band::NONE);
    let (spaced_sup, spaced_sub) = lines(Band { above: 3, below: 2 });
    assert_eq!((sup.overline, sub.overline), (0, 5));
    assert_eq!((spaced_sup.overline, spaced_sub.overline), (3, 8));
    for (a, b) in [(sup, spaced_sup), (sub, spaced_sub)] {
        assert_eq!(b.underline, a.underline + 3);
        assert_eq!(b.strike, a.strike + 3);
        assert_eq!(b.underline_thickness, a.underline_thickness);
    }
}

// D8 step 7: every line of every script set lies inside the cell, at
// every height and line spacing, by D3 and by D4
// covers: sgr.script
#[test]
fn every_line_of_every_script_set_lies_inside_the_cell() {
    let inside = |d: Decorations, h: u32, what: &str| {
        assert!(
            d.underline_thickness >= 1 && d.strike_thickness >= 1,
            "{what}"
        );
        assert!(d.underline + d.underline_thickness <= h, "{what}: {d:?}");
        assert!(d.strike + d.strike_thickness <= h, "{what}: {d:?}");
        assert!(d.overline + d.underline_thickness <= h, "{what}: {d:?}");
    };
    for height in 16..=16 * 8 {
        let font = Font::new(Unifont::new(height));
        for script in [Script::Super, Script::Sub] {
            let h = font.cell().height;
            inside(
                font.script_decorations(script),
                h,
                &format!("unifont {height}"),
            );
        }
    }
    for above in 0..6 {
        for below in 0..6 {
            let band = Band { above, below };
            let font = Font::new(Stated {
                lines: Decorations {
                    underline: 15,
                    underline_thickness: 1,
                    ..STATED
                },
                band,
            });
            for script in [Script::Super, Script::Sub] {
                inside(
                    font.script_decorations(script),
                    16 + band.added(),
                    &format!("{band:?}"),
                );
            }
        }
    }
    for height in 1..=64 {
        let normal = Decorations::for_height(height);
        for script in [Script::Super, Script::Sub] {
            inside(
                lines::mapped(normal, script, (0, height), height, Band::NONE),
                height,
                &format!("mapped {height}"),
            );
        }
    }
}

// D8 step 6: a superscript cell and a normal cell draw their lines on
// their own rows, so an underline across the two steps
// covers: sgr.script
#[test]
fn a_superscripts_underline_lies_under_the_raised_glyph() {
    let font = Font::new(Unifont::new(16));
    let sup = font.script_decorations(Script::Super);
    let g = font
        .drawn_variant(
            &['x'],
            Width::Narrow,
            vtium_font::Variant {
                style: Style::default(),
                script: Script::Super,
            },
        )
        .unwrap()
        .glyph;
    let ink_bottom = (0..16)
        .rev()
        .find(|&y| (0..8).any(|x| g.pixel(x, y).unwrap()[0] != 0))
        .unwrap();
    assert!(
        sup.underline > ink_bottom,
        "under the glyph's ink, row {ink_bottom}"
    );
    assert_ne!(sup.underline, font.decorations().underline);
}
