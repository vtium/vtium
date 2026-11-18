//! Any input as a vtlook, read against vtium's own filters' declarations.
//!
//! A vtlook is a file root installs and any user can hand to `vtium
//! check-config`, and the console reads it at start, so its reader must
//! never panic. The target also checks that the reader's answer is
//! consistent: a vtlook that reads cleanly comes with no error, and one
//! that fails names at least one error, each about the file it was given,
//! on a line the file has.
//!
//! References:
//! - ADR-1304 D4: the vtlook's syntax and bounds.
//! - ADR-1304 D6: any error fails the whole vtlook, and each report names its file and line.
#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use vtium_settings::vtlook::read_text;

fuzz_target!(|data: &[u8]| {
    let file = Path::new("/etc/vtium/looks/fuzz.vtlook");
    let lines = data.split(|&b| b == b'\n').count();
    match read_text("fuzz", file, data) {
        Ok(look) => {
            assert_eq!(look.name, "fuzz");
            // vtium declares no filter yet, so nothing reads cleanly with one
            assert!(!look.holds_filters());
        }
        Err(reports) => {
            assert!(!reports.is_empty());
            for r in &reports {
                assert_eq!(r.file, file);
                assert_eq!(r.vtlook, "fuzz");
                if let Some(n) = r.line {
                    assert!((1..=lines).contains(&n), "line {n} of {lines}");
                }
                let _shown = format!("{}: {r}", r.at());
            }
            let _logged = vtium_settings::vtlook::log_line(&reports, "console");
        }
    }
});
