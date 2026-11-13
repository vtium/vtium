//! Any bytes as vtium.conf, through the section reader, read whole with
//! `reading` and as a flow with `sections` (ADR-1401 D7, ADR-1403 D3): no
//! panic,
//! every rule that survives sends its devices to a head that survives,
//! the section reader names no error on a line that holds one of
//! vtium's own top-level keys, which the console reads, so a top-level
//! mistake never drops a section, and the policy read alone, as
//! `vtium.enable=0` reads it, is the policy of the whole reading
//! (ADR-1401 D9).
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_settings::{Reading, Target, policy, reading, sections, split};

/// Every rule of what survives aims at a head that survives.
fn targets_hold(r: &Reading) {
    let config = &r.parsed.config;
    for rule in &config.rules {
        if let Target::Head(i) = rule.target {
            assert!(i < config.heads.len(), "a rule names head {i} of {}", config.heads.len());
        }
    }
}

fuzz_target!(|bytes: &[u8]| {
    let whole = reading(bytes);
    targets_hold(&whole);
    assert_eq!(policy(bytes), whole.parsed.config.policy, "the policy read alone");
    let Some(r) = sections(bytes) else {
        assert!(!split(bytes).has_flow());
        return;
    };
    targets_hold(&r);
    let keys: Vec<usize> = split(bytes).key_lines().map(|(n, _)| n).collect();
    for e in &r.errors.errors {
        if let Some(line) = e.line {
            assert!(!keys.contains(&line), "an error on top-level key line {line}: {e:?}");
        }
    }
});
