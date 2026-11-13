//! Tier 1 for vtium: the corpus and the behaviour scenarios replayed
//! through vtium and judged against their goldens; and the capability
//! list judged through the capability scenarios. The crate's README gives
//! the commands.

#[path = "conformance/scenarios.rs"]
mod scenarios;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use vtium_conformance::generate::{self, Inputs};
use vtium_conformance::interface;
use vtium_conformance::issues;
use vtium_conformance::runner::{self, Corpus, VIEWS, Verdict, chunking, own};
use vtium_conformance::scenario::capability;
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::spec::Spec;
use vtium_conformance::view::View;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");
/// The corpus.
const CORPUS: &str = "corpus";
/// vtium's own cases, laid out as the corpus is, with the list of those
/// whose goldens wait for the maintainer's review.
const OWN_CASES: &str = "vtium";
const PROVISIONAL: &str = "vtium/provisional.tsv";
/// The interface's documents, each a later version of the one before.
const INTERFACE: [&str; 2] = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"];
/// The build features the capability list names, each with whether this
/// build of vtium-replay has it. `-Dimages` names the image pipeline with
/// the sixel and kitty handlers (#48, #49), and `-Dbidi` right-to-left text
/// (ADR-1004); make conformance-probes builds vtium-replay with both, so
/// the capabilities that need them are judged as working there and as
/// absent in a build without them.
const BUILD: [(&str, bool); 2] = [
    ("images", cfg!(feature = "images")),
    ("bidi", cfg!(feature = "bidi")),
];
/// Features interface 1 lets an implementation without images, or
/// without the draw2 view, leave out.
const OPTIONAL: [&str; 8] = [
    "view-draw2",
    "damage-exact",
    "images",
    "sixel",
    "kitty-graphics",
    "image-host",
    "kitty-animation",
    "bidi",
];
/// Features vtium's additions describe and vtium-replay does not offer
/// yet: the issue INTERFACE-vtium.md names adds each one. `--describe` need
/// not declare them, and a scenario that needs one fails as absent. A
/// `--describe` that declares one is refused, so the change that adds the
/// feature also takes it off this list.
const OWED: [&str; 0] = [];

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
/// the same major version, its lines in the blocks' order, every feature
/// the interface makes mandatory, and none of the owed features.
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
    for feature in OWED {
        if ours.lines().any(|l| l == format!("feature {feature}")) {
            return Err(format!(
                "--describe declares the feature {feature}, which the runner lists as owed. Take it off OWED in src/bin/conformance.rs, and raise the interface's version."
            ));
        }
    }
    for feature in &spec.features {
        if !OPTIONAL.contains(&feature.as_str())
            && !OWED.contains(&feature.as_str())
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
    for (view, name, golden_dir) in views(set) {
        let (mut pass, mut drift, mut fail) = (0, 0, 0);
        let cases: Vec<&runner::Case> = set
            .cases
            .iter()
            .filter(|c| view_has_golden(set, c, golden_dir))
            .collect();
        for case in cases.iter().copied() {
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
        let listed = cases
            .iter()
            .filter(|c| provisional.contains_key(&c.name))
            .count();
        println!(
            "{label}{name}: {} cases, {pass} match their goldens, {fail} FAIL, {listed} provisional, {drift} differ from a provisional golden",
            cases.len()
        );
    }
    Ok(ok)
}

/// The views `set`'s cases are judged in: the screen and frame views, and
/// in a build with the bidi feature the display view, for the cases that
/// have a golden of it.
fn views(set: &Corpus) -> Vec<(View, &'static str, &'static str)> {
    let _ = set;
    #[cfg(feature = "bidi")]
    return VIEWS.into_iter().chain([runner::DISPLAY]).collect();
    #[cfg(not(feature = "bidi"))]
    VIEWS.to_vec()
}

/// Whether `case` is judged in the view whose goldens are in `golden_dir`:
/// every case is in the screen and frame views, and a case is in the
/// display view when `golden-display/` holds its golden.
fn view_has_golden(set: &Corpus, case: &runner::Case, golden_dir: &str) -> bool {
    VIEWS.iter().any(|(_, _, d)| *d == golden_dir)
        || set
            .root
            .join(golden_dir)
            .join(format!("{}.txt", case.name))
            .is_file()
}

fn run(o: &Options) -> Result<bool, String> {
    // the goldens hold what vtium-replay built with images and bidi prints
    if !vtium_conformance::command::IMAGES || !vtium_conformance::BIDI {
        return Err(
            "conformance run judges vtium-replay built with images and bidi: build with --features images,bidi, as make conformance does"
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
    for (view, name, golden_dir) in views(&own_cases) {
        let mut chunked = 0;
        let cases = corpus.cases.iter().map(|c| (&corpus, c));
        for (set, case) in cases.chain(own_cases.cases.iter().map(|c| (&own_cases, c))) {
            if !view_has_golden(set, case, golden_dir) {
                continue;
            }
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

/// The capability list judged through the capability scenarios under
/// `scenarios/` (ADR-1504 D4): a scenario whose capability the list does
/// not name, a name no scenario declares, and a verdict that disagrees
/// with the list each fail the run.
fn probes() -> Result<bool, String> {
    let spec = spec()?;
    let caps = vtium_conformance::coverage::capabilities(&read(
        &Path::new(TOOL).join("capabilities.list"),
    )?)?;
    let report = capability::judge(&caps, &scenarios::all()?, &runner(&spec)?, &BUILD)?;
    for line in &report.lines {
        println!("{line}");
    }
    let count = |s: &str| caps.iter().filter(|c| c.status == s).count();
    println!(
        "{} capabilities, each judged by its scenario under {}/: {} work, {} missing; {} disagree with the list",
        caps.len(),
        scenarios::DIR,
        count("works"),
        count("missing"),
        report.failures
    );
    Ok(report.failures == 0)
}

/// The repository whose issues the capability list and ADR-1000 name.
const REPO: &str = "vtium/vtium";

/// Runs `gh` with these arguments and returns each line it prints.
fn gh(args: &[&str]) -> Result<Vec<String>, String> {
    let out = std::process::Command::new("gh")
        .args(args)
        .args(["--repo", REPO])
        .output()
        .map_err(|e| format!("cannot start gh: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gh {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

/// The state of every issue on vtium/vtium, in one `gh` call, and the
/// numbers of its pull requests, in a second one made only when a number
/// is not an issue.
fn issue_states(
    named: impl IntoIterator<Item = u32>,
) -> Result<impl Fn(u32) -> issues::State, String> {
    use issues::State;
    let mut states: BTreeMap<u32, State> = BTreeMap::new();
    let lines = gh(&[
        "issue",
        "list",
        "--state",
        "all",
        "--limit",
        "100000",
        "--json",
        "number,state",
        "--jq",
        r#".[] | "\(.number) \(.state)""#,
    ])?;
    for line in &lines {
        let (number, state) = line
            .split_once(' ')
            .and_then(|(n, s)| Some((n.parse::<u32>().ok()?, s)))
            .ok_or_else(|| format!("gh issue list printed {line:?}"))?;
        let state = if state == "OPEN" {
            State::Open
        } else {
            State::Closed
        };
        states.insert(number, state);
    }
    if states.is_empty() {
        return Err(format!("gh issue list found no issue on {REPO}"));
    }
    if named.into_iter().any(|n| !states.contains_key(&n)) {
        let lines = gh(&[
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "100000",
            "--json",
            "number",
            "--jq",
            ".[].number",
        ])?;
        for line in &lines {
            let n = line
                .parse::<u32>()
                .map_err(|_| format!("gh pr list printed {line:?}"))?;
            states.insert(n, State::PullRequest);
        }
    }
    Ok(move |n| states.get(&n).copied().unwrap_or(State::Missing))
}

/// Holds the capability list and ADR-1000's issue column to GitHub. Every
/// issue that a missing capability names must be open, since the issue's
/// work makes the capability work. ADR-1000's rows must agree with the
/// issues they name, by the rules in `issues`.
fn issues() -> Result<bool, String> {
    let caps = vtium_conformance::coverage::capabilities(&read(
        &Path::new(TOOL).join("capabilities.list"),
    )?)?;
    let mut owed: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for cap in caps.iter().filter(|c| c.status == "missing") {
        let n = cap
            .issue
            .strip_prefix('#')
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(|| {
                format!(
                    "capabilities.list: the missing capability {} names {:?}, which is not an issue written as #N",
                    cap.name, cap.issue
                )
            })?;
        owed.entry(n).or_default().push(cap.name.clone());
    }
    let adr = Path::new(TOOL).join("../../docs/architecture/term");
    let mut rows = Vec::new();
    for name in ["ADR-1000-features.tsv", "ADR-1000-modes.tsv"] {
        rows.extend(issues::rows(name, &read(&adr.join(name))?)?);
    }
    let named: Vec<u32> = owed
        .keys()
        .copied()
        .chain(rows.iter().flat_map(|r| r.issues.iter().copied()))
        .collect();
    let state = issue_states(named)?;
    let mut failures = 0usize;
    for (n, names) in &owed {
        if state(*n) != issues::State::Open {
            failures = failures.saturating_add(1);
            println!(
                "FAIL Issue #{n} is not open, and capabilities.list still says it owes {}. Set each capability that works to works, or name the open issue that owes it.",
                names.join(", ")
            );
        }
    }
    for message in issues::check(&rows, &state) {
        failures = failures.saturating_add(1);
        println!("FAIL {message}");
    }
    println!(
        "issues: {} named by capabilities.list, {} ADR-1000 rows read, {failures} failed",
        owed.len(),
        rows.len()
    );
    Ok(failures == 0)
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
