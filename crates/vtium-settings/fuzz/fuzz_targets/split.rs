//! Any bytes as vtium.conf through the split at its first section heading
//! and the top level's line editor (ADR-1403 D3, D8): no panic, the two
//! parts are the whole file in order, the top level holds no heading, and
//! setting a key changes the top level alone, leaving the sections byte
//! for byte, with vtium taking the new value.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_settings::{Edit, edit_conf, key_line, setting, split};

fuzz_target!(|bytes: &[u8]| {
    let parts = split(bytes);
    assert_eq!([parts.top, parts.sections].concat(), bytes);
    assert_eq!(parts.first_section.is_some(), !parts.sections.is_empty());
    let mut last = 0;
    for (n, line) in parts.top_lines() {
        assert!(line.iter().find(|b| !matches!(b, b' ' | b'\t')) != Some(&b'['));
        let _ = key_line(line);
        last = n;
    }
    if let Some(first) = parts.first_section {
        assert_eq!(first, last + 1);
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let Ok(out) = edit_conf(text, &[Edit::Set("font-size", "12")]) else {
        panic!("a good key and value are refused");
    };
    assert_eq!(split(out.as_bytes()).sections, parts.sections);
    assert_eq!(setting(&out, "font-size").as_deref(), Some("12"));
});
