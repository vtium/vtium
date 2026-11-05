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
//! job on every pull request and push to main, or lets it pass without
//! judging, so the name this test gives is always a job that judges.

use std::path::Path;

use vtium_specimens::draw::{Absent, Sources};

/// The id, in `.github/workflows/ci.yml`, of the job that draws and judges
/// the profiles this test cannot draw. A macro, so that `JUDGE` is built
/// from it and renaming the job on either side fails the second test.
macro_rules! job {
    () => {
        "specimens"
    };
}

/// The job that draws and judges the profiles this test cannot draw.
const JUDGE: &str = concat!("`make specimens-check` in CI's ", job!(), " job");

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

/// The lines of the block that `header` opens in `lines`: the header and
/// each line after it up to the next line, not blank and not a comment,
/// indented no deeper than the header, without the blank lines and
/// comments at its end. `None` when no line is `header`.
fn block<'a>(lines: &[&'a str], header: &str) -> Option<Vec<&'a str>> {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let at = lines.iter().position(|l| *l == header)?;
    let depth = indent(header);
    let body = lines[at + 1..].iter().take_while(|l| {
        let t = l.trim();
        t.is_empty() || t.starts_with('#') || indent(l) > depth
    });
    let mut block: Vec<&str> = std::iter::once(lines[at]).chain(body.copied()).collect();
    while block.len() > 1
        && block
            .last()
            .is_some_and(|l| l.trim().is_empty() || l.trim().starts_with('#'))
    {
        block.pop();
    }
    Some(block)
}

/// The keys a line of a block sets, a list item's included.
fn key(line: &str) -> &str {
    let t = line.trim_start();
    t.strip_prefix("- ").unwrap_or(t)
}

// The job the first test names runs in CI on every pull request and on each
// push to main, as the other jobs do: in its own steps it fetches the pinned
// fonts and then runs the check, and nothing in it skips the job or a step
// or lets either fail without failing the workflow.
#[test]
fn ci_runs_the_job_that_judges_what_cargo_test_skips() {
    let ci = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    let ci = std::fs::read_to_string(&ci).unwrap();
    let lines: Vec<&str> = ci.lines().collect();
    let on = block(&lines, "on:").expect("ci.yml has no `on:` block");
    assert!(
        block(&on, "  pull_request:").is_some_and(|b| b.len() == 1),
        "ci.yml must run on every pull request, with no filter: {on:?}"
    );
    assert!(
        block(&on, "  push:").is_some_and(|b| b[1..] == ["    branches: [main]"]),
        "ci.yml must run on each push to main: {on:?}"
    );
    let jobs = block(&lines, "jobs:").expect("ci.yml has no `jobs:` block");
    let header = concat!("  ", job!(), ":");
    let job = block(&jobs, header).unwrap_or_else(|| panic!("ci.yml has no job {header}"));
    for line in &job {
        let k = key(line);
        assert!(
            !k.starts_with("if:") && !k.starts_with("continue-on-error:"),
            "the job {} must not skip or excuse itself or a step: {line}",
            job!()
        );
    }
    let runs: Vec<&str> = job
        .iter()
        .filter_map(|l| key(l).strip_prefix("run: "))
        .collect();
    let at = |cmd: &str| runs.iter().position(|r| *r == cmd);
    let fonts = at("make specimens-fonts");
    let check = at("make specimens-check");
    assert!(
        fonts.is_some() && check.is_some() && fonts < check,
        "the job {} must run `make specimens-fonts` and then `make specimens-check`: {runs:?}",
        job!()
    );
}
