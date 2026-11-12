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

// ADR-1202 D3: the built-in font drawn at twice its size in a deferred
// face, as it is in a look's cell and while a frit's glyph is on its way,
// takes its superscripts and subscripts from itself at its own size, as
// the built-in font's own face does
// covers: sgr.script
#[test]
fn a_built_in_font_at_twice_its_size_takes_its_scripts_at_its_own_size() {
    let cell = Cell {
        width: 16,
        height: 32,
    };
    let deferred = Font::new(DeferredFace::new(
        Box::new(Unifont::new(32)),
        cell,
        16,
        Shared::default(),
    ));
    let own = Font::new(Unifont::new(32));
    let one = Unifont::new(16);
    for script in [Script::Super, Script::Sub] {
        for (bold, italic) in [(false, false), (true, false), (false, true)] {
            let variant = Variant {
                style: Style { bold, italic },
                script,
            };
            let got = deferred
                .drawn_variant(&['g'], Width::Narrow, variant)
                .unwrap()
                .glyph;
            let want = own
                .drawn_variant(&['g'], Width::Narrow, variant)
                .unwrap()
                .glyph;
            assert_eq!(got.pixels(), want.pixels(), "{variant:?}");
        }
        let variant = Variant {
            style: Style::default(),
            script,
        };
        let got = deferred
            .drawn_variant(&['g'], Width::Narrow, variant)
            .unwrap()
            .glyph;
        let small = one.render('g', Style::default()).unwrap();
        let top = if script == Script::Super { 0 } else { 16 };
        for y in 0..32u32 {
            for x in 0..16u32 {
                let inside = (4..12).contains(&x) && (top..top + 16).contains(&y);
                let want = inside && small.pixel(x - 4, y - top).unwrap()[0] == 255;
                assert_eq!(
                    got.pixel(x, y).unwrap()[0] == 255,
                    want,
                    "{script:?} at {x},{y}"
                );
            }
        }
    }
}
