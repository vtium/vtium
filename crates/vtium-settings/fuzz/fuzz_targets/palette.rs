//! Any input as a palette file, a color scheme in Ghostty's theme format.
//!
//! A palette file is a file root installs and any user can hand to `vtium
//! check-config`, and the console reads it when it starts, so its reader
//! must never panic. The target also checks that the reader's answer is
//! consistent. A file that reads cleanly lists each ignored key once, on
//! a line the file has, in the file's order, and never lists a key vtium
//! reads. A file that fails names at least one error and at most
//! `ERRORS_MAX`, each on a line the file has, and a file over the bound
//! fails as too large whatever it holds. The log line and every reason
//! print without a control character, so nothing the file holds reaches a
//! console or a log as itself.
//!
//! References:
//! - ADR-1205 D10: the format, the keys vtium reads, the errors and the bound.
#![no_main]

use std::path::PathBuf;

use libfuzzer_sys::fuzz_target;
use vtium_settings::palette::{Failure, Reason, parse};
use vtium_settings::{ERRORS_MAX, PALETTE_FILE_MAX};

/// The keys the reader takes; any other key is ignored.
const READ: [&str; 6] = [
    "palette",
    "foreground",
    "background",
    "cursor-color",
    "selection-background",
    "selection-foreground",
];

fuzz_target!(|data: &[u8]| {
    // the last line counts though it has no newline, and a file over the
    // bound is never read line by line
    let lines = data.split(|&b| b == b'\n').count();
    match parse(data) {
        Ok((_, ignored)) => {
            assert!(data.len() <= PALETTE_FILE_MAX);
            let mut last = 0;
            for (i, key) in ignored.iter().enumerate() {
                assert!(!READ.contains(&key.key.raw()), "{:?}", key.key);
                assert!((1..=lines).contains(&key.line));
                assert!(key.line > last, "the ignored keys are in the file's order");
                last = key.line;
                assert!(
                    ignored.iter().skip(i + 1).all(|k| k.key != key.key),
                    "{:?} listed twice",
                    key.key
                );
                assert!(!key.key.to_string().chars().any(char::is_control));
            }
        }
        Err((faults, more)) => {
            assert!(!faults.is_empty());
            assert!(faults.len() <= ERRORS_MAX);
            assert!(more == 0 || faults.len() == ERRORS_MAX);
            if data.len() > PALETTE_FILE_MAX {
                assert_eq!(faults.len(), 1);
                assert!(matches!(faults[0].reason, Reason::TooLarge));
            }
            for f in &faults {
                if let Some(n) = f.line {
                    assert!((1..=lines).contains(&n), "line {n} of {lines}");
                }
                let shown = f.reason.to_string();
                assert!(!shown.chars().any(char::is_control), "{shown:?}");
            }
            let failure = Failure {
                name: "fuzz".to_owned(),
                file: PathBuf::from("/etc/vtium/palettes/fuzz.palette"),
                faults,
                more,
            };
            let logged = failure.log_line("vtium draws the color scheme legacy instead.");
            assert!(!logged.chars().any(char::is_control), "{logged:?}");
        }
    }
});
