//! Any bytes as one cursor theme file through the Xcursor reader the font
//! helper runs for the console's pointer (ADR-1309 D6). The first byte
//! picks the `pointer-size` asked for, none or 8 to 64. No panic, and each
//! picture the reader keeps is at most 64 pixels a side, holds exactly its
//! pixels, has its hotspot inside it, and has no color above its alpha.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config::xcursor::read;

fuzz_target!(|input: &[u8]| {
    let Some((&first, bytes)) = input.split_first() else {
        return;
    };
    let size = (first >= 8).then(|| u32::from(first % 57) + 8);
    let Ok(pictures) = read(bytes, size) else {
        return;
    };
    assert!(!pictures.is_empty());
    assert!(size.is_none() || pictures.len() == 1);
    for p in &pictures {
        assert!((1..=64).contains(&p.width) && (1..=64).contains(&p.height));
        assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
        assert!(p.hot.0 < p.width && p.hot.1 < p.height);
        for px in p.rgba.chunks(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }
});
