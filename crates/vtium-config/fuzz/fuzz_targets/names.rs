//! A font file's bytes through the name-table reader that `font:FAMILY`
//! uses to find an installed outline font by its family, and through the
//! reader of a face's version that `version=` matches against: no panic,
//! whatever the file, at most `FACES_MAX` faces, each read once at a rising
//! index under the cap, and every family a face gives is a name, not an
//! empty string.
//!
//! The same bytes then go through `names::swash_fonts`, which runs the font
//! check `names::sound` before it hands a font to swash. On each face of a
//! font the check passes, the target makes every call into swash that
//! vtium-config makes outside its tests: the face's strings, its metrics,
//! its character map, and the advance width of each glyph a character maps
//! to. On a face whose metrics give a size to draw at, which the rasteriser
//! requires before it draws, the target also measures glyph 0 and the
//! glyphs of the characters the rasteriser and the shaper look up, and
//! reads a few mapped glyphs and glyph 0 through a scaler, as color layers
//! and as outlines. A panic there is a structure the check misses.
//!
//! The target reads each outline but does not rasterise it, because the
//! rasteriser, zeno, overflows on a glyph scaled to tens of thousands of
//! pixels. That is a matter of the glyph's size, which no check of the
//! file's structure decides. The target also leaves out swash's reader of
//! color bitmap strikes, which the check does not cover, as the crate's
//! README says.
//!
//! References:
//! - ADR-1500 D8: `font:FAMILY` finds an installed font by its family.
//! - ADR-1204 D7: `version=` matches a face's version string.
#![no_main]

use libfuzzer_sys::fuzz_target;
use swash::scale::ScaleContext;
use swash::{FontRef, StringId};
use vtium_config::names::{FACES_MAX, faces, swash_fonts};
use vtium_config::serve::outline_version;

/// How many mapped glyphs the scaler reads on each face.
const SCALED: usize = 4;

fuzz_target!(|bytes: &[u8]| {
    names(bytes);
    let Ok(fonts) = swash_fonts(bytes) else {
        return;
    };
    let mut context = ScaleContext::new();
    for font in fonts.fonts().take(FACES_MAX as usize) {
        through_swash(font, &mut context);
    }
});

/// The name-table reader and the version reader on `bytes`.
fn names(bytes: &[u8]) {
    // the first face's version, and each family's below
    let _ = outline_version(bytes, "");
    let Ok(found) = faces(bytes) else {
        return;
    };
    for face in &found {
        for family in &face.families {
            let _ = outline_version(bytes, family);
        }
    }
    assert!(found.len() <= FACES_MAX as usize);
    for face in &found {
        assert!(face.index < FACES_MAX);
        assert!(face.families.iter().all(|f| !f.is_empty()));
    }
    // each face read once, in the collection's order
    assert!(found.windows(2).all(|w| w[0].index < w[1].index));
}

/// Each call vtium-config makes into swash, on one face that the check
/// passed.
fn through_swash(font: FontRef<'_>, context: &mut ScaleContext) {
    let strings = font.localized_strings();
    for id in [
        StringId::Family,
        StringId::SubFamily,
        StringId::TypographicFamily,
        StringId::TypographicSubFamily,
        StringId::Version,
        StringId::License,
    ] {
        for language in [None, Some("en")] {
            let _ = strings.find_by_id(id, language).map(|s| s.to_string());
        }
    }
    let metrics = font.metrics(&[]);
    let advances = font.glyph_metrics(&[]);
    // each distinct glyph a character maps to is measured once, so that a
    // group mapping a million characters to one glyph stays fast
    let mut seen = vec![false; usize::from(u16::MAX) + 1];
    let mut scaled = Vec::new();
    let charmap = font.charmap();
    charmap.enumerate(|_, g| {
        if !std::mem::replace(&mut seen[usize::from(g)], true) {
            let _ = advances.advance_width(g);
            if scaled.len() < SCALED {
                scaled.push(g);
            }
        }
    });
    // the rasteriser's own rule: it draws only a face with units per em
    // and a span from ascent to descent
    if metrics.units_per_em == 0 || metrics.ascent + metrics.descent.abs() <= 0.0 {
        return;
    }
    let _ = advances.scale(16.0).advance_width(0);
    for c in ['W', 'M', '0', 'x', '\u{640}'] {
        let _ = advances.advance_width(charmap.map(c));
    }
    let palette = font.color_palettes().next();
    scaled.push(0);
    for g in scaled {
        let mut scaler = context.builder(font).size(16.0).build();
        if scaler.has_color_outlines()
            && let Some(layers) = scaler.scale_color_outline(g)
        {
            for i in 0..layers.len() {
                let color = layers.get(i).and_then(|l| l.color_index());
                let _ = color.zip(palette).map(|(c, p)| p.get(c));
            }
        }
        let _ = scaler.scale_outline(g);
    }
}
