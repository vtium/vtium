//! A font file's bytes through the name-table reader that `font:FAMILY`
//! uses to find an installed outline font by its family (ADR-1500 D8): no
//! panic, whatever the file, at most `FACES_MAX` faces, and every family a
//! face gives is a name, not an empty string.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config::names::{FACES_MAX, faces};

fuzz_target!(|bytes: &[u8]| {
    let Ok(found) = faces(bytes) else {
        return;
    };
    assert!(found.len() <= FACES_MAX as usize);
    for face in &found {
        assert!(face.families.iter().all(|f| !f.is_empty()));
    }
});
