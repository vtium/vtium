//! The console's built-in fonts in superscript and subscript cells, drawn
//! through the face that places them in a cell with line spacing
//! (ADR-1202 D1, D4, D7).

#![expect(clippy::unwrap_used, reason = "a test fails by panicking")]

use vtium::deferred::{DeferredFace, Shared};
use vtium_font::spacing::Band;
use vtium_font::{Cell, Font, Script, Style, Unifont, Variant, Width};

// In a cell with line spacing, the built-in font's superscript and
// subscript are its own small 1-bit glyphs, moved down with the text band,
// with bold and italic added to the small glyph
// covers: sgr.script
#[test]
fn a_built_in_script_glyph_sits_against_the_text_band() {
    let band = Band { above: 3, below: 2 };
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let spaced = Font::new(DeferredFace::spaced(
        Box::new(Unifont::new(16)),
        cell,
        band,
        16,
        Shared::default(),
    ));
    let own = Font::new(Unifont::new(16));
    for script in [Script::Super, Script::Sub] {
        for (bold, italic) in [(false, false), (true, false), (false, true)] {
            let variant = Variant {
                style: Style { bold, italic },
                script,
            };
            let got = spaced
                .drawn_variant(&['H'], Width::Narrow, variant)
                .unwrap()
                .glyph;
            let want = own
                .drawn_variant(&['H'], Width::Narrow, variant)
                .unwrap()
                .glyph;
            assert_eq!(got.height(), 21);
            for y in 0..21u32 {
                for x in 0..8 {
                    let w = y
                        .checked_sub(band.above)
                        .and_then(|y| want.pixel(x, y))
                        .map_or(0, |p| p[0]);
                    assert_eq!(got.pixel(x, y).unwrap()[0], w, "{variant:?} at {x},{y}");
                }
            }
        }
    }
}
