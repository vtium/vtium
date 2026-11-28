//! A BDF font's bytes through vtium-config's own BDF reader (ADR-1200
//! D14) and its reader of the font's version (ADR-1204 D7): no panic,
//! whatever the file, each glyph the reader keeps is exactly its cells'
//! rows, and the font and version read in one pass are those read apart.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font_helper::bdf::{font, read, version};

fuzz_target!(|bytes: &[u8]| {
    let stated = version(bytes);
    let Ok(f) = font(bytes) else {
        assert!(read(bytes, true).is_err());
        return;
    };
    let (once, once_version) = read(bytes, true).expect("a font that reads reads in one pass");
    assert_eq!(once.map, f.map);
    assert_eq!(once.glyphs, f.glyphs);
    assert_eq!(once_version, stated);
    assert_eq!(f.glyphs.len(), f.map.len());
    for (width, bits) in &f.glyphs {
        let across = f.cell.width * width.cells();
        assert_eq!(bits.len(), across.div_ceil(8) as usize * f.cell.height as usize);
    }
    for &g in f.map.values() {
        assert!((g as usize) < f.glyphs.len());
    }
});
