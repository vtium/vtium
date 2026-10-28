//! Tier 1 for vtium: the corpus, copied from the C reference, replayed
//! through vtium and judged against its goldens; and the capability
//! probes run through vtium-replay. The crate's README gives the commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_conformance::generate::{self, Inputs};
use vtium_conformance::interface;
use vtium_conformance::probes::{self, Outcome, Replay};
use vtium_conformance::runner::lists::{self, Expected};
use vtium_conformance::runner::{self, Corpus, VIEWS, Verdict, chunking, provenance};
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");
/// The corpus and the probes, each file copied with a row in
/// `provenance.tsv`; vtium's own probes sit beside the copied file.
const CORPUS: &str = "corpus";
/// The interface's documents, each a later version of the one before.
const INTERFACE: [&str; 2] = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"];
const PROBES: [&str; 2] = ["probes/probes.replay", "probes/vtium.replay"];
/// The directories every file of which is a copy, but for vtium's own.
const COPIED: [&str; 3] = [
    "tools/conformance/corpus",
    "tools/conformance/probes",
    "tools/conformance/generator",
];
const OWN: [&str; 1] = ["tools/conformance/probes/vtium.replay"];
/// The build features the capability list names: vtium is built without
/// images until vtium-image lands (#48, #49).
const BUILD: [(&str, bool); 1] = [("images", false)];
/// Features interface 1 lets an implementation without images, or
/// without the draw2 view, leave out.
const OPTIONAL: [&str; 6] = [
    "view-draw2",
    "damage-exact",
    "images",
    "sixel",
    "kitty-graphics",
    "image-host",
];

const USAGE: &str = "usage: conformance [run] [--without CASE]... [--seed N] [--rounds N]
       conformance expect VIEW CASE...
       conformance generate DIR
       conformance interface
       conformance probes
       conformance issues
       conformance provenance [--libtsm DIR] [--kmscon DIR]";

struct Options {
    libtsm: PathBuf,
    kmscon: PathBuf,
    without: Vec<String>,
    seed: u64,
    rounds: usize,
    rest: Vec<String>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        libtsm: repo_root().join("../libtsm"),
        kmscon: repo_root().join("../kmscon"),
        without: Vec::new(),
        // a CI run takes its own seed, so each run tries other cuts
        seed: std::env::var("GITHUB_RUN_ID")
            .ok()
            .and_then(|id| id.parse().ok())
            .unwrap_or(1),
        rounds: 2,
        rest: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--libtsm" => o.libtsm = PathBuf::from(value()?),
            "--kmscon" => o.kmscon = PathBuf::from(value()?),
            "--without" => o.without.push(value()?),
            "--seed" => o.seed = value()?.parse().map_err(|_| "--seed takes a number")?,
            "--rounds" => o.rounds = value()?.parse().map_err(|_| "--rounds takes a number")?,
            a if a.starts_with("--") => return Err(format!("unknown option {a}\n{USAGE}")),
            _ => o.rest.push(arg.clone()),
        }
    }
    Ok(o)
}

fn repo_root() -> PathBuf {
    Path::new(TOOL).join("../..")
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The copies, each checked against its row of `provenance.tsv`.
fn copies() -> Result<Vec<provenance::Copy>, String> {
    let copies = provenance::read(&read(&Path::new(TOOL).join("provenance.tsv"))?)?;
    let faults = provenance::check(&repo_root(), &copies, &COPIED, &OWN);
    if faults.is_empty() {
        Ok(copies)
    } else {
        Err(faults.join("\n"))
    }
}

fn corpus() -> Result<Corpus, String> {
    Corpus::open(&Path::new(TOOL).join(CORPUS))
}

/// The interface's documents: 1.0 as the corpus holds it, and vtium's
/// additions.
fn texts() -> Result<Vec<String>, String> {
    INTERFACE
        .iter()
        .map(|file| read(&Path::new(TOOL).join(file)))
        .collect()
}

/// The interface, read from its documents.
fn spec() -> Result<Spec, String> {
    Spec::read(&texts()?.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The listed expectations, each departure's entry checked against the
/// file that lists it.
fn expectations(
    corpus: &Corpus,
    without: &[String],
) -> Result<BTreeMap<(String, String), Expected>, String> {
    let rows = lists::expected_rows(&read(&Path::new(TOOL).join("expected.tsv"))?)?;
    for row in &rows {
        if !corpus.cases.iter().any(|c| c.name == row.case) {
            return Err(format!(
                "expected.tsv:{}: {} is not in cases.list",
                row.line, row.case
            ));
        }
    }
    lists::check_sources(&rows, |src| match src {
        "INTERFACE.md" => read(&corpus.root.join("INTERFACE.md")),
        path => read(&repo_root().join(path)),
    })?;
    let views: Vec<&str> = VIEWS.iter().map(|(_, name, _)| *name).collect();
    let mut all = lists::expectations(&rows, &views, &Path::new(TOOL).join("expected"))?;
    // --without takes a case's rows out of the expectations, as a plant
    all.retain(|(_, case), _| !without.contains(case));
    Ok(all)
}

/// vtium-replay's `--describe` against the interface's `describe` blocks:
/// the same major version, its lines in the blocks' order, and every
/// feature the interface makes mandatory.
fn check_describe(spec: &Spec) -> Result<(), String> {
    let ours = vtium_conformance::describe();
    let major = ours
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("interface "))
        .and_then(|v| v.split('.').next());
    if major != Some(spec.major.as_str()) {
        return Err(format!(
            "the interface is {:?}, vtium-replay {major:?}",
            spec.major
        ));
    }
    let mut theirs = spec.describe.iter().skip(1);
    for line in ours.lines().skip(1) {
        if !theirs.any(|l| l == line) {
            return Err(format!(
                "--describe line {line:?} is not in the describe block, or out of its order"
            ));
        }
    }
    for feature in &spec.features {
        if !OPTIONAL.contains(&feature.as_str())
            && !ours.lines().any(|l| l == format!("feature {feature}"))
        {
            return Err(format!("--describe lacks the mandatory feature {feature}"));
        }
    }
    Ok(())
}

/// The interface check (`vtium_conformance::interface`): the goldens
/// under the grammar, the worked examples, the palettes, the refusals and
/// the grammar's controls. Prints each fault, or one line when all hold.
fn check_interface(spec: &Spec, corpus: &Corpus) -> Result<bool, String> {
    let texts = texts()?;
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let check = interface::check(spec, &texts, corpus, &vtium_conformance::describe());
    let faults = check.faults();
    for fault in &faults {
        println!("FAIL replay-interface: {fault}");
    }
    if faults.is_empty() {
        println!("{}", check.summary());
    }
    Ok(faults.is_empty())
}

/// `--describe` and the interface check alone.
fn interface() -> Result<bool, String> {
    copies()?;
    let spec = spec()?;
    check_describe(&spec)?;
    check_interface(&spec, &corpus()?)
}

fn run(o: &Options) -> Result<bool, String> {
    let copies = copies()?;
    let corpus = corpus()?;
    let expected = expectations(&corpus, &o.without)?;
    let spec = spec()?;
    check_describe(&spec)?;
    println!(
        "conformance: {} cases, copied from libtsm {}; {} copies match provenance.tsv",
        corpus.cases.len(),
        provenance::commit(&copies, "libtsm")?,
        copies.len()
    );
    let mut ok = check_interface(&spec, &corpus)?;
    for (view, name, golden_dir) in VIEWS {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for case in &corpus.cases {
            let input = corpus.input(case, view)?;
            let got = vtium_conformance::replay::run(&input);
            let golden = corpus.golden(case, golden_dir)?;
            let listed = expected.get(&(name.to_owned(), case.name.clone()));
            let verdict = runner::judge(&got, &golden, listed);
            let label = match &verdict {
                Verdict::Pass => "pass",
                Verdict::Departure => "departure",
                Verdict::Pending => "pending",
                Verdict::Fail(why) => {
                    ok = false;
                    println!("FAIL {name} {}: {why}", case.name);
                    "FAIL"
                }
            };
            if let (Verdict::Departure | Verdict::Pending, Some(e)) = (&verdict, listed) {
                for row in &e.rows {
                    println!("{label} {name} {}: {} {}", case.name, row.source, row.entry);
                }
            }
            *counts.entry(label).or_default() += 1;
        }
        let count = |k: &str| counts.get(k).copied().unwrap_or(0);
        println!(
            "{name}: {} pass, {} departure, {} pending, {} FAIL",
            count("pass"),
            count("departure"),
            count("pending"),
            count("FAIL")
        );
    }
    println!(
        "chunking: seed {}, {} rounds of extra cuts",
        o.seed, o.rounds
    );
    for (view, name, _) in VIEWS {
        let mut chunked = 0;
        for case in &corpus.cases {
            let input = corpus.input(case, view)?;
            match chunking::check(&input, o.seed, o.rounds) {
                Ok(()) => chunked += 1,
                // the replay refuses the case: the verdicts above judged
                // the refusal, as a failure or against a pending snapshot
                Err(why) if why.starts_with("exits ") => {}
                Err(why) => {
                    ok = false;
                    println!("FAIL chunking {name} {}: {why}", case.name);
                }
            }
        }
        println!("chunking {name}: {chunked} cases unchanged");
    }
    Ok(ok)
}

/// Writes vtium's output for listed cases: a departure's expected output
/// under `expected/<view>/`, a pending case's snapshot, its output or its
/// exit, under `expected/pending/<view>/`. Each is read against the golden
/// before it is committed.
fn expect(o: &Options) -> Result<bool, String> {
    let [view, cases @ ..] = &o.rest[..] else {
        return Err(USAGE.to_owned());
    };
    if cases.is_empty() {
        return Err(USAGE.to_owned());
    }
    let (v, _, _) = VIEWS
        .iter()
        .find(|(_, n, _)| n == view)
        .ok_or_else(|| format!("no view {view}"))?;
    let corpus = corpus()?;
    let rows = lists::expected_rows(&read(&Path::new(TOOL).join("expected.tsv"))?)?;
    for case in cases {
        let listed: Vec<_> = rows
            .iter()
            .filter(|r| &r.case == case && (r.view == "*" || &r.view == view))
            .collect();
        if listed.is_empty() {
            return Err(format!(
                "{case} has no row for the {view} view in expected.tsv"
            ));
        }
        let pending = listed.iter().any(|r| r.pending);
        let c = corpus
            .cases
            .iter()
            .find(|c| &c.name == case)
            .ok_or_else(|| format!("no case {case}"))?;
        let got = vtium_conformance::replay::run(&corpus.input(c, *v)?);
        let text = match (&got, pending) {
            (_, true) => runner::snapshot(&got),
            (Ok(out), false) => out.clone(),
            (Err(f), false) => return Err(format!("{case} exits {}: {}", f.code, f.message)),
        };
        let path = lists::expected_path(&Path::new(TOOL).join("expected"), view, case, pending);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        // what moved, for the commit that lands the rewrite to say
        match std::fs::read_to_string(&path) {
            Ok(old) if old == text => {
                println!("unchanged {}", path.display());
                continue;
            }
            Ok(old) => println!("wrote {}\n{}", path.display(), runner::diff(&old, &text)),
            Err(_) => println!("wrote {}, which is new", path.display()),
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!(
        "read each departure against {}/golden*/, and each snapshot by git diff",
        corpus.root.display()
    );
    Ok(true)
}

/// Writes every synthetic case under the directory named, from the
/// generator's recipes and its inputs under `generator/`, and prints the
/// lines `cases.list` takes.
fn write_cases(o: &Options) -> Result<bool, String> {
    let [dir] = &o.rest[..] else {
        return Err(USAGE.to_owned());
    };
    let inputs = Inputs::read(&Path::new(TOOL).join("generator"))?;
    print!(
        "{}",
        generate::write(Path::new(dir), &generate::cases(&inputs))?
    );
    Ok(true)
}

/// The capability probes, each judged against `capabilities.list`: a
/// probe the list does not name, a name no probe defines, and a verdict
/// that disagrees with the list each fail the run.
fn probes() -> Result<bool, String> {
    copies()?;
    let spec = spec()?;
    let caps = vtium_conformance::coverage::capabilities(&read(
        &Path::new(TOOL).join("capabilities.list"),
    )?)?;
    probes::check_list(&caps)?;
    let mut all = Vec::new();
    for file in PROBES {
        let text = match std::fs::read_to_string(Path::new(TOOL).join(file)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !all.is_empty() => continue,
            Err(e) => return Err(format!("{file}: {e}")),
        };
        all = probes::merge(all, probes::read(file, &text, &spec)?)?;
    }
    probes::check_departures(&all, |path| read(&repo_root().join(path)))?;
    for probe in &all {
        let missing = caps
            .iter()
            .any(|c| c.name == probe.name && c.status == "missing");
        probes::check_expectations(probe.file(), probe, &spec, missing)?;
    }
    let replay = Replay::new(&spec, &vtium_conformance::describe())?
        .with_fixtures(Path::new(TOOL).join("fixtures"));
    let mut failures = 0usize;
    for probe in all
        .iter()
        .filter(|p| !caps.iter().any(|c| c.name == p.name))
    {
        println!(
            "FAIL {}: a probe the list does not name; add it to capabilities.list",
            probe.name
        );
        failures += 1;
    }
    for cap in &caps {
        let Some(probe) = all.iter().find(|p| p.name == cap.name) else {
            println!(
                "FAIL {}: capabilities.list names a probe nothing defines",
                cap.name
            );
            failures += 1;
            continue;
        };
        let outcome: Outcome = replay.run(probe);
        let (line, agrees) = probes::verdict(cap, &outcome, &BUILD);
        println!("{line}");
        if !agrees {
            failures += 1;
        }
    }
    let count = |s: &str| caps.iter().filter(|c| c.status == s).count();
    println!(
        "{} probes: {} work, {} missing; {failures} disagree with the list",
        caps.len(),
        count("works"),
        count("missing")
    );
    Ok(failures == 0)
}

/// Each row of `provenance.tsv` against its source, read from the
/// checkouts with `git show COMMIT:PATH` and nothing else, for a review of
/// a change to the copies. The gates never run it.
fn verify_provenance(o: &Options) -> Result<bool, String> {
    let copies = provenance::read(&read(&Path::new(TOOL).join("provenance.tsv"))?)?;
    let faults = provenance::verify(&copies, |repo, commit, path| {
        let dir = match repo {
            "libtsm" => &o.libtsm,
            "kmscon" => &o.kmscon,
            _ => return Err(format!("no checkout for {repo}")),
        };
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["show", &format!("{commit}:{path}")])
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(format!(
                "git -C {} show: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    });
    for fault in &faults {
        println!("FAIL {fault}");
    }
    println!(
        "provenance: {} rows, {} match their sources",
        copies.len(),
        copies.len().saturating_sub(faults.len())
    );
    Ok(faults.is_empty())
}

/// Every issue the lists name as owing a case or a capability, checked
/// open through `gh`: a pending row or a missing capability whose issue
/// closed is stale, since the issue's work should have taken the case up.
fn issues() -> Result<bool, String> {
    let rows = lists::expected_rows(&read(&Path::new(TOOL).join("expected.tsv"))?)?;
    let caps = vtium_conformance::coverage::capabilities(&read(
        &Path::new(TOOL).join("capabilities.list"),
    )?)?;
    let mut owed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.pending) {
        owed.entry(row.source.clone())
            .or_default()
            .push(row.case.clone());
    }
    for cap in caps.iter().filter(|c| c.status == "missing") {
        owed.entry(cap.issue.clone())
            .or_default()
            .push(cap.name.clone());
    }
    let mut ok = true;
    for (issue, names) in &owed {
        let n = issue.trim_start_matches('#');
        let out = std::process::Command::new("gh")
            .args([
                "issue",
                "view",
                n,
                "--repo",
                "vtium/vtium",
                "--json",
                "state",
                "-q",
                ".state",
            ])
            .output()
            .map_err(|e| format!("gh: {e}"))?;
        let state = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !out.status.success() {
            return Err(format!(
                "gh issue view {n}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if state != "OPEN" {
            ok = false;
            println!(
                "FAIL {issue} is {state}, and still owes {}",
                names.join(", ")
            );
        }
    }
    println!(
        "issues: {} named, {}",
        owed.len(),
        if ok { "all open" } else { "some closed" }
    );
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = options(&args).and_then(|mut o| {
        let command = if o.rest.is_empty() {
            "run".to_owned()
        } else {
            o.rest.remove(0)
        };
        match command.as_str() {
            "run" if o.rest.is_empty() => run(&o),
            "expect" => expect(&o),
            "generate" => write_cases(&o),
            "interface" if o.rest.is_empty() => interface(),
            "probes" if o.rest.is_empty() => probes(),
            "issues" if o.rest.is_empty() => issues(),
            "provenance" if o.rest.is_empty() => verify_provenance(&o),
            _ => Err(USAGE.to_owned()),
        }
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(why) => {
            eprintln!("conformance: {why}");
            ExitCode::from(2)
        }
    }
}
