//! Draws every tier 1 golden as a PNG through vtium's own renderer, for
//! the maintainer's review (ADR-1505, V3):
//!
//! ```text
//! tier1-shots OUT
//! ```
//!
//! Each case of `tools/conformance/corpus/` and `tools/conformance/vtium/`
//! is replayed through vtium-replay's code, with its test commands and the
//! fixture host, and checked against its goldens. The terminal it leaves
//! is drawn into `OUT/corpus/NAME.png` or `OUT/vtium/NAME.png`. Each
//! screen a behaviour scenario of `tools/conformance/scenarios/` dumps is
//! drawn into `OUT/scenarios/NAME-NN.png`, and the scenario's transcript is
//! checked. `OUT/manifest.json` lists every picture (tools/conformance's
//! README, "Tier 1's pictures"). A picture whose output differs from its
//! golden is drawn, and the tool prints the lines that differ. Each
//! picture is checked against its live path, `make tier1-play`, from the
//! same replay: when the live path's bytes, read back through a terminal,
//! draw another picture, the tool prints its name and writes that drawing
//! beside it as `NAME.live.png`.
//!
//! OUT must be new, empty, or the output of an earlier run, which holds a
//! `manifest.json`. The tool then removes that run's pictures and manifest
//! first. It exits 0 when it drew every picture, 1 when a replay or a
//! drawing failed, and 2 when it refuses to run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_conformance::runner::{Corpus, own};
use vtium_conformance::scenario::Sort;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::spec::Spec;
use vtium_tier1_shots::judge::Side;
use vtium_tier1_shots::manifest::{self, Group, Shot};
use vtium_tier1_shots::select::{self, SCENARIOS, SETS};
use vtium_tier1_shots::shoot::{self, Drawn};
use vtium_tier1_shots::{CONFORMANCE, root};

/// The interface's documents, each a later version of the one before, as
/// `conformance` reads them.
const INTERFACE: [&str; 2] = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"];
/// The lists of provisional goldens: the cases', which covers both sets,
/// and the scenarios'.
const CASES_PROVISIONAL: &str = "vtium/provisional.tsv";
const SCENARIOS_PROVISIONAL: &str = "scenarios/provisional.tsv";
/// What a run writes in OUT, and so what the next run removes.
const MANIFEST: &str = "manifest.json";

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Readies OUT: new or empty, or an earlier run's output, whose pictures
/// and manifest go.
fn ready(out: &Path) -> Result<(), String> {
    let shown = out.display();
    if out.join(MANIFEST).is_file() {
        for dir in SETS.iter().chain(&[SCENARIOS]) {
            let dir = out.join(dir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
        }
        std::fs::remove_file(out.join(MANIFEST)).map_err(|e| format!("{shown}: {e}"))?;
    } else if out.exists() {
        let mut entries = std::fs::read_dir(out).map_err(|e| format!("{shown}: {e}"))?;
        if entries.next().is_some() {
            return Err(format!(
                "{shown} holds files and no {MANIFEST}, so it is not an earlier run's output. Name a new or empty directory."
            ));
        }
    }
    for dir in SETS.iter().chain(&[SCENARIOS]) {
        let dir = out.join(dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Every picture: each set's cases, then each behaviour scenario's dumps;
/// and the behaviour scenarios that dump no screen, which have none.
fn shots(tier1: &Path) -> Result<(Vec<Drawn>, Vec<String>), String> {
    let cases = own::provisional(&read(&tier1.join(CASES_PROVISIONAL))?)?;
    let mut out = Vec::new();
    for (set, group) in SETS.iter().zip([Group::Corpus, Group::Vtium]) {
        let corpus = Corpus::open(&tier1.join(set))?;
        for case in &corpus.cases {
            out.push(shoot::case(&corpus, group, case, cases.get(&case.name)));
        }
    }
    let texts = INTERFACE
        .iter()
        .map(|f| read(&tier1.join(f)))
        .collect::<Result<Vec<_>, _>>()?;
    let spec = Spec::read(&texts.iter().map(String::as_str).collect::<Vec<_>>())?;
    let runner = Runner::new(
        &spec,
        &vtium_conformance::describe(),
        tier1.join("fixtures"),
    )?;
    let listed: BTreeMap<String, String> =
        own::provisional(&read(&tier1.join(SCENARIOS_PROVISIONAL))?)?;
    let dir = tier1.join(SCENARIOS);
    let mut undumped = Vec::new();
    for s in select::scenarios(&dir)? {
        if s.sort == Sort::Behaviour && select::dumps(&s).is_empty() {
            undumped.push(s.name.clone());
            continue;
        }
        let transcript = dir.join(format!("{}.transcript", s.name));
        out.extend(shoot::scenario(
            &runner,
            &s,
            &transcript,
            listed.get(&s.name),
        ));
    }
    Ok((out, undumped))
}

/// Prints a shot whose output differs from a golden, or that failed, with
/// the lines that differ.
fn report(shot: &Shot) {
    let state = match &shot.provisional {
        Some(_) => "provisional",
        None => "reviewed",
    };
    let what = match &shot.dump {
        Some(d) => format!(
            "{} (dump {} of {}, {})",
            shot.file(),
            d.number,
            d.of,
            d.command
        ),
        None => shot.file(),
    };
    if let Some(why) = &shot.error {
        println!("FAIL {what}: {why}");
    }
    for g in shot.goldens.iter().filter(|g| !g.differs.is_empty()) {
        println!(
            "differs {what}: the {} output differs from {}, a {state} golden",
            g.view, g.path
        );
        for l in &g.differs {
            let sign = match l.side {
                Side::Golden => '-',
                Side::Output => '+',
            };
            println!("    {sign} {:>5} {}", l.number, l.text);
        }
    }
}

fn run(out: &Path) -> Result<bool, String> {
    let tier1: PathBuf = root().join(CONFORMANCE);
    ready(out)?;
    let (shots, undumped) = shots(&tier1)?;
    let mut ok = true;
    for Drawn {
        shot,
        picture,
        live,
    } in &shots
    {
        for (img, file) in [(picture, shot.file()), (live, shot.live_file())] {
            if let Some(img) = img {
                let path = out.join(file);
                std::fs::write(&path, shoot::png(img))
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
        if !shot.matches() {
            report(shot);
        }
        if picture.is_some() && !shot.live.matches {
            println!(
                "live differs {}: the bytes `{}` writes for this screen draw {}",
                shot.file(),
                shot.live.command,
                shot.live_file()
            );
        }
        ok &= shot.error.is_none() && picture.is_some();
    }
    let list: Vec<Shot> = shots.into_iter().map(|d| d.shot).collect();
    let path = out.join(MANIFEST);
    std::fs::write(&path, manifest::json(&list)).map_err(|e| format!("{}: {e}", path.display()))?;
    let count = |g: Group| list.iter().filter(|s| s.group == g).count();
    let differ = list.iter().filter(|s| !s.matches()).count();
    let live = list.iter().filter(|s| !s.live.matches).count();
    println!(
        "tier1-shots: {} pictures in {}: corpus {}, vtium {}, scenarios {}; {differ} differ from their goldens or failed; the live play draws {live} differently",
        list.len(),
        out.display(),
        count(Group::Corpus),
        count(Group::Vtium),
        count(Group::Scenarios),
    );
    println!(
        "tier1-shots: {} behaviour scenarios dump no screen, so they have no picture: {}",
        undumped.len(),
        undumped.join(" ")
    );
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out] = args.as_slice() else {
        eprintln!("usage: tier1-shots OUT");
        return ExitCode::from(2);
    };
    // the goldens hold what vtium-replay built with images and bidi prints
    if !vtium_conformance::command::IMAGES || !vtium_conformance::BIDI {
        eprintln!(
            "tier1-shots draws vtium-replay built with images and bidi: build it with --features draw, as make tier1-shots does"
        );
        return ExitCode::from(2);
    }
    match run(Path::new(out)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(why) => {
            eprintln!("tier1-shots: {why}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tier1-shots-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // a directory that is not an earlier run's output keeps its files
    #[test]
    fn a_directory_with_files_and_no_manifest_is_refused() {
        let dir = scratch("refused");
        std::fs::create_dir_all(dir.join("corpus")).unwrap();
        std::fs::write(dir.join("corpus/keep.txt"), "x").unwrap();
        let why = ready(&dir).expect_err("refused");
        assert!(why.contains(MANIFEST), "{why}");
        assert!(dir.join("corpus/keep.txt").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // an earlier run's pictures and manifest go, and only those
    #[test]
    fn an_earlier_run_is_cleared_and_its_other_files_stay() {
        let dir = scratch("earlier");
        std::fs::create_dir_all(dir.join("scenarios")).unwrap();
        std::fs::write(dir.join("scenarios/old-01.png"), "x").unwrap();
        std::fs::write(dir.join(MANIFEST), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        ready(&dir).unwrap();
        assert!(!dir.join("scenarios/old-01.png").exists());
        assert!(!dir.join(MANIFEST).exists());
        assert!(dir.join("notes.txt").is_file());
        assert!(dir.join("corpus").is_dir() && dir.join("vtium").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
