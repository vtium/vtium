//! A BDF font's bytes through vtium-vitrify's own BDF reader (ADR-1200
//! D14): no panic, whatever the file, and each glyph the reader keeps is
//! exactly its cells' rows.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_vitrify::bdf::font;

fuzz_target!(|bytes: &[u8]| {
    let Ok(f) = font(bytes) else {
        return;
    };
    assert_eq!(f.glyphs.len(), f.map.len());
    for (width, bits) in &f.glyphs {
        let across = f.cell.width * width.cells();
        assert_eq!(bits.len(), across.div_ceil(8) as usize * f.cell.height as usize);
    }
    for &g in f.map.values() {
        assert!((g as usize) < f.glyphs.len());
    }
});
