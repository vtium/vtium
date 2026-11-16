//! Any bytes as a frit's text, read by the frit reader as the glyph helper
//! reads a frit with its layers in place (ADR-1200 D1, ADR-1204 D5): no
//! panic, whatever the bytes. In a frit the reader accepts, what each
//! source needs holds its form (ADR-1204 D7): its
//! `check=` ranges sorted, merged and in order, its `sha256=` 64 lowercase
//! hexadecimal digits, its `url=` a web address, and its `version=` a
//! version that reads back from its own spelling. Any bytes as a font's
//! version string give no version, or one that reads back the same.
#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use vtium_config::frit::{Frit, Version};

fuzz_target!(|bytes: &[u8]| {
    let file = Path::new("t.vtfrit");
    if let Ok(text) = std::str::from_utf8(bytes)
        && let Some(found) = Version::in_string(text)
    {
        assert_eq!(Version::parse(&found.spelling), Some(found.clone()));
    }
    for parsed in [
        Frit::parse(bytes, file, "t"),
        Frit::parse_spliced(bytes, file, "t", Path::new("layers")),
    ] {
        let Ok(frit) = parsed else {
            continue;
        };
        for source in &frit.sources {
            let needs = &source.needs;
            assert!(
                needs
                    .check
                    .windows(2)
                    .all(|w| w[0].1.saturating_add(1) < w[1].0),
                "check= ranges not merged"
            );
            assert!(needs.check.iter().all(|&(a, b)| a <= b && b <= 0x10_ffff));
            if let Some(hash) = &needs.sha256 {
                assert!(
                    hash.len() == 64
                        && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                );
            }
            if let Some(url) = &needs.url {
                assert!(url.starts_with("https://") || url.starts_with("http://"));
            }
            if let Some(version) = &needs.version {
                assert_eq!(Version::parse(&version.spelling).as_ref(), Some(version));
            }
        }
    }
});
