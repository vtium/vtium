//! The committed specimens against a fresh drawing of them, so `cargo test`
//! fails on a change that moves an accepted profile until the moved files
//! are committed. A provisional profile's drift is printed and fails
//! nothing.
//!
//! `cargo test` fetches no fonts, so it generates no glyphs for the outline
//! profiles, 1920x1080 and 2560x1440, or for the profile 320x240-tiny5.
//! It names CI's specimens job, which runs `make specimens-fonts` and then
//! `make specimens-check`, as the job that draws and judges those three,
//! and `check` skips them here for that reason. It still judges their
//! text files. The second test fails if CI's workflow stops running that
//! job, so the name this test gives is never a job that does not run.

use std::path::Path;

use vtium_specimens::draw::{Absent, Sources};

/// The job that draws and judges the profiles this test cannot draw.
const JUDGE: &str = "`make specimens-check` in CI's specimens job";

#[test]
fn every_accepted_specimen_matches_its_drawing() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specimens");
    let sources = Sources {
        desktop: Err(Absent::JudgedBy(JUDGE)),
        tiny5: Err(Absent::JudgedBy(JUDGE)),
    };
    let faults = vtium_specimens::check::check(&dir, &sources).unwrap();
    for s in &faults.skipped {
        println!("skipped {s}");
    }
    for d in &faults.drift {
        println!("drift in a provisional profile: {d}");
    }
    assert!(
        !faults.failed(),
        "specimens: {:?}\nsimulations: {:?}\n`make specimens` redraws them",
        faults.goldens,
        faults.simulations
    );
}

// The job the first test names runs in CI: a step that fetches the pinned
// fonts, then a step that runs the check.
#[test]
fn ci_runs_the_job_that_judges_what_cargo_test_skips() {
    let ci = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    let ci = std::fs::read_to_string(&ci).unwrap();
    let runs: Vec<&str> = ci
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- run: "))
        .collect();
    let at = |cmd: &str| runs.iter().position(|r| *r == cmd);
    let fonts = at("make specimens-fonts");
    let check = at("make specimens-check");
    assert!(
        fonts.is_some() && check.is_some() && fonts < check,
        "ci.yml must run `make specimens-fonts` and then `make specimens-check`: {runs:?}"
    );
}
