//! A `.hex` font's bytes through vtium-config's own `.hex` reader
//! (ADR-1204 D5): no panic, whatever the file, and in each font it accepts
//! every glyph is 16 rows of one byte or of two, as its width says, with
//! one glyph for each code point and every code point a character.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font_helper::hex::{HEX_CELL, font};
use vtium_font::Width;

fuzz_target!(|bytes: &[u8]| {
    let Ok(f) = font(bytes) else {
        return;
    };
    assert_eq!(f.cell, HEX_CELL);
    assert_eq!(f.glyphs.len(), f.map.len());
    for (width, bits) in &f.glyphs {
        let row = match width {
            Width::Narrow => 1,
            Width::Wide => 2,
        };
        assert_eq!(bits.len(), row * HEX_CELL.height as usize);
    }
    for (&c, &g) in &f.map {
        assert!(char::from_u32(c).is_some());
        assert!((g as usize) < f.glyphs.len());
    }
});
