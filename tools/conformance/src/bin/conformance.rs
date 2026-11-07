//! Tier 1 for vtium: the corpus and the behaviour scenarios replayed
//! through vtium and judged against their goldens; and the capability
//! probes run through vtium-replay. The crate's README gives the commands.

#[path = "conformance/scenarios.rs"]
mod scenarios;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use vtium_conformance::generate::{self, Inputs};
use vtium_conformance::interface;
use vtium_conformance::probes::{self, Outcome, Replay};
use vtium_conformance::runner::{self, Corpus, VIEWS, Verdict, chunking, own};
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");
/// The corpus; the probes sit in `probes/`,
/// vtium's own beside the copied file.
const CORPUS: &str = "corpus";
/// vtium's own cases, laid out as the corpus is, with the list of those
/// whose goldens wait for the maintainer's review.
const OWN_CASES: &str = "vtium";
const PROVISIONAL: &str = "vtium/provisional.tsv";
/// The interface's documents, each a later version of the one before.
const INTERFACE: [&str; 2] = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"];
const PROBES: [&str; 2] = ["probes/probes.replay", "probes/vtium.replay"];
/// The build features the capability list names. Its `-Dimages` names the
/// image pipeline with the sixel and kitty handlers. vtium-replay is built with images, which holds
/// the pipeline and both handlers (#48, #49), so the probes that need them
/// are judged as working.
const BUILD: [(&str, bool); 1] = [("images", true)];
/// Features interface 1 lets an implementation without images, or
/// without the draw2 view, leave out.
const OPTIONAL: [&str; 7] = [
    "view-draw2",
    "damage-exact",
    "images",
    "sixel",
    "kitty-graphics",
    "image-host",
    "kitty-animation",
];

const USAGE: &str = "usage: conformance [run] [--seed N] [--rounds N]
       conformance generate DIR
       conformance interface
       conformance probes
       conformance issues
       conformance scenarios [NAME...]
       conformance scenarios --write [--replace] NAME...
       conformance scenarios --play NAME...
       conformance scenarios --bin NAME DIR";

struct Options {
    seed: u64,
    rounds: usize,
    rest: Vec<String>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
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
            "--seed" => o.seed = value()?.parse().map_err(|_| "--seed takes a number")?,
            "--rounds" => o.rounds = value()?.parse().map_err(|_| "--rounds takes a number")?,
            a if a.starts_with("--") => return Err(format!("unknown option {a}\n{USAGE}")),
            _ => o.rest.push(arg.clone()),
        }
    }
    Ok(o)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
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
    let own = Corpus::open(&Path::new(TOOL).join(OWN_CASES))?;
    let check = interface::check(
        spec,
        &texts,
        &[corpus, &own],
        &vtium_conformance::describe(),
    );
    let mut faults = check.faults();
    let all = scenarios::all()?;
    faults.extend(scenarios::faults(spec, &all));
    for fault in &faults {
        println!("FAIL replay-interface: {fault}");
    }
    if faults.is_empty() {
        println!(
            "{}; {} scenarios' assertions and transcripts parse",
            check.summary(),
            all.len()
        );
    }
    Ok(faults.is_empty())
}

/// `--describe` and the interface check alone.
fn interface() -> Result<bool, String> {
    let spec = spec()?;
    check_describe(&spec)?;
    check_interface(&spec, &corpus()?)
}

/// The provisional list: the cases, of the corpus or of vtium's own,
/// whose goldens wait for the maintainer's review.
fn provisional(corpus: &Corpus, own_cases: &Corpus) -> Result<BTreeMap<String, String>, String> {
    let provisional = own::provisional(&read(&Path::new(TOOL).join(PROVISIONAL))?)?;
    for listed in provisional.keys() {
        if ![corpus, own_cases]
            .iter()
            .any(|set| set.cases.iter().any(|c| &c.name == listed))
        {
            return Err(format!(
                "{PROVISIONAL}: {listed} is no case of {CORPUS}/cases.list or {OWN_CASES}/cases.list"
            ));
        }
    }
    Ok(provisional)
}

/// Judges every case of `set` in each view against its golden, and prints
/// each failure and each provisional golden that differs; `label` heads
/// the lines. Returns whether every verdict holds.
fn judge_set(
    label: &str,
    set: &Corpus,
    provisional: &BTreeMap<String, String>,
) -> Result<bool, String> {
    let mut ok = true;
    for (view, name, golden_dir) in VIEWS {
        let (mut pass, mut drift, mut fail) = (0, 0, 0);
        for case in &set.cases {
            let got = vtium_conformance::replay::run(&set.input(case, view)?);
            let golden = set.golden(case, golden_dir)?;
            match (runner::judge(&got, &golden), provisional.get(&case.name)) {
                (Verdict::Pass, _) => pass += 1,
                (Verdict::Fail(_), Some(reason)) => {
                    drift += 1;
                    println!(
                        "provisional {label}{name} {}: differs from its golden, which judges nothing until it is reviewed ({reason})",
                        case.name
                    );
                }
                (Verdict::Fail(why), None) => {
                    fail += 1;
                    ok = false;
                    println!("FAIL {label}{name} {}: {why}", case.name);
                }
            }
        }
        let listed = set
            .cases
            .iter()
            .filter(|c| provisional.contains_key(&c.name))
            .count();
        println!(
            "{label}{name}: {} cases, {pass} match their goldens, {fail} FAIL, {listed} provisional, {drift} differ from a provisional golden",
            set.cases.len()
        );
    }
    Ok(ok)
}

fn run(o: &Options) -> Result<bool, String> {
    // the goldens hold what vtium-replay built with images prints
    if !vtium_conformance::command::IMAGES {
        return Err(
            "conformance run judges vtium-replay built with images: build with --features images, as make conformance does"
                .to_owned(),
        );
    }
    let corpus = corpus()?;
    let own_cases = Corpus::open(&Path::new(TOOL).join(OWN_CASES))?;
    let provisional = provisional(&corpus, &own_cases)?;
    let spec = spec()?;
    check_describe(&spec)?;
    println!("conformance: {} cases", corpus.cases.len());
    let mut ok = check_interface(&spec, &corpus)?;
    ok &= judge_set("", &corpus, &provisional)?;
    ok &= judge_set("vtium ", &own_cases, &provisional)?;
    println!(
        "chunking: seed {}, {} rounds of extra cuts",
        o.seed, o.rounds
    );
    for (view, name, _) in VIEWS {
        let mut chunked = 0;
        let cases = corpus.cases.iter().map(|c| (&corpus, c));
        for (set, case) in cases.chain(own_cases.cases.iter().map(|c| (&own_cases, c))) {
            let input = set.input(case, view)?;
            match chunking::check(&input, o.seed, o.rounds) {
                Ok(()) => chunked += 1,
                // the replay refuses the case: the verdicts above judged
                // the refusal
                Err(why) if why.starts_with("exits ") => {}
                Err(why) => {
                    ok = false;
                    println!("FAIL chunking {name} {}: {why}", case.name);
                }
            }
        }
        println!("chunking {name}: {chunked} cases unchanged");
    }
    let all = scenarios::all()?;
    let listed = scenarios::provisional(&all)?;
    let runner = runner(&spec)?;
    ok &= scenarios::judge(&runner, &all, &listed, o.seed, o.rounds);
    Ok(ok)
}

/// vtium-replay's code as the scenarios meet it, with the fixtures in
/// `fixtures/`.
fn runner(spec: &Spec) -> Result<Runner<'_>, String> {
    Runner::new(
        spec,
        &vtium_conformance::describe(),
        Path::new(TOOL).join("fixtures"),
    )
}

/// `conformance scenarios ...`, which plays the scenarios through
/// vtium-replay built with images, the build their transcripts hold.
fn scenario_command(args: &[String]) -> Result<bool, String> {
    if !vtium_conformance::command::IMAGES {
        return Err(
            "conformance scenarios plays vtium-replay built with images: build with --features images, as make conformance does"
                .to_owned(),
        );
    }
    let spec = spec()?;
    let o = options(&[])?;
    scenarios::command(&runner(&spec)?, args, o.seed, o.rounds)
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

/// Every issue the capability list names as owing a missing capability,
/// checked open through `gh`: a missing capability whose issue closed is
/// stale, since the issue's work should have made the capability work.
fn issues() -> Result<bool, String> {
    let caps = vtium_conformance::coverage::capabilities(&read(
        &Path::new(TOOL).join("capabilities.list"),
    )?)?;
    let mut owed: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
    let result = if args.first().is_some_and(|a| a == "scenarios") {
        scenario_command(args.get(1..).unwrap_or_default())
    } else {
        options(&args).and_then(|mut o| {
            let command = if o.rest.is_empty() {
                "run".to_owned()
            } else {
                o.rest.remove(0)
            };
            match command.as_str() {
                "run" if o.rest.is_empty() => run(&o),
                "generate" => write_cases(&o),
                "interface" if o.rest.is_empty() => interface(),
                "probes" if o.rest.is_empty() => probes(),
                "issues" if o.rest.is_empty() => issues(),
                _ => Err(USAGE.to_owned()),
            }
        })
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(why) => {
            eprintln!("conformance: {why}");
            ExitCode::from(2)
        }
    }
}
