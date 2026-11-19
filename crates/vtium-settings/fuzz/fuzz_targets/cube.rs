//! Any input as a `.cube` file, read by the `.cube` reader.
//!
//! A `.cube` file is a file root installs, and `vtium check-config` reads
//! one for any user, so its reader must never panic. A file that reads
//! cleanly is a table of a size vtium reads, and it builds its color
//! table, which must succeed. A file that fails names at least one fault
//! and at most `ERRORS_MAX`, each on a line the file has.
//!
//! References:
//! - ADR-1304 D9: the sizes vtium reads and the clamp to 0 to 1.
//! - ADR-1304 D14: the grammar and the report of each fault.
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_render::filter::{Map, SIDES_3D, SIZES_1D, Shape, Table};
use vtium_settings::ERRORS_MAX;
use vtium_settings::vtlook::read_cube;

fuzz_target!(|data: &[u8]| {
    let pieces = data.split(|&b| b == b'\n').count();
    match read_cube(data) {
        Ok(cube) => {
            match cube.shape() {
                Shape::Three(n) => assert!(SIDES_3D.contains(&n)),
                Shape::One(n) => assert!(SIZES_1D.contains(&n)),
            }
            let built = Table::build(&[Map::cube(Arc::new(cube))]);
            assert!(built.is_ok());
        }
        Err(faults) => {
            assert!(!faults.faults.is_empty());
            assert!(faults.faults.len() <= ERRORS_MAX);
            for f in &faults.faults {
                if let Some(n) = f.line {
                    assert!((1..=pieces).contains(&n), "line {n} of {pieces}");
                }
                assert!(!f.message.chars().any(|c| c.is_control()), "{:?}", f.message);
            }
        }
    }
});
