//! Any bytes as a font file through `describe::describe`, which the font
//! helper runs on each file vtium-config asks it to describe: no panic,
//! whatever the file, every count in a description it gives within its
//! bound, and each description read back from its frame as itself. The
//! same bytes as a frame through `Description::decode`, which vtium-config
//! runs on what the helper sends: no panic, and nothing in a name that a
//! terminal would take as a control character.
//!
//! References:
//! - ADR-1205 D3: the helper reads a font's family, coverage and sizes in
//!   its sandbox, and vtium-config shows what it answers.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font_helper::describe::{
    Description, FACES_MAX, RANGES_MAX, STRIKES_MAX, TEXT_BYTES_MAX, describe,
};

fuzz_target!(|bytes: &[u8]| {
    for name in ["font.ttf", "font.hex", "font.psf.gz"] {
        if let Ok(d) = describe(bytes, name) {
            assert!(d.faces.len() <= FACES_MAX);
            for face in &d.faces {
                assert!(face.family.len() <= TEXT_BYTES_MAX);
                assert!(face.strikes.len() <= STRIKES_MAX);
                assert!(face.coverage.len() <= RANGES_MAX);
            }
            assert_eq!(Description::decode(&d.encode()).as_ref(), Ok(&d));
        }
    }
    if let Ok(d) = Description::decode(bytes) {
        for face in &d.faces {
            for text in [&face.family, &face.style] {
                assert!(!text.chars().any(char::is_control), "{text:?}");
            }
        }
    }
});
