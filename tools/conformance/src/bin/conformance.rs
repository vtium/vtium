//! Tier 1 for vtium: the C reference's conformance corpus, at the libtsm
//! commit `pin.tsv` names, replayed through vtium and judged against its
//! goldens; and the C reference's capability probes run through
//! vtium-replay. The crate's README gives the commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_conformance::runner::lists::{self, Expected, Pin};
use vtium_conformance::runner::{self, Corpus, VIEWS, Verdict, chunking, source};

const TOOL: &str = env!("CARGO_MANIFEST_DIR");
const CONFORMANCE: &str = "test/conformance";
const CAPABILITIES: &str = "tests/capabilities";
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
       conformance probes
       conformance issues
each takes [--libtsm DIR] [--kmscon DIR] [--out DIR]";

struct Options {
    libtsm: PathBuf,
    kmscon: PathBuf,
    out: PathBuf,
    without: Vec<String>,
    seed: u64,
    rounds: usize,
    rest: Vec<String>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let root = repo_root();
    let mut o = Options {
        libtsm: root.join("../libtsm"),
        kmscon: root.join("../kmscon"),
        out: Path::new(TOOL).join("out"),
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
            "--out" => o.out = PathBuf::from(value()?),
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

fn pins() -> Result<Vec<Pin>, String> {
    lists::pins(&read(&Path::new(TOOL).join("pin.tsv"))?)
}

/// The corpus at the pinned libtsm commit.
fn corpus(o: &Options, pins: &[Pin]) -> Result<Corpus, String> {
    let pin = lists::pin(pins, "libtsm")?;
    let dir = source::extract(&o.libtsm, pin, &[CONFORMANCE], &o.out)?;
    Corpus::open(&dir.join(CONFORMANCE))
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

/// vtium-replay's `--describe` against the interface's `describe` block:
/// the same major version, its lines in the block's order, and every
/// feature interface 1 makes mandatory.
fn check_describe(interface: &str) -> Result<(), String> {
    let block = interface
        .split("```describe\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .ok_or("INTERFACE.md has no describe block")?;
    let ours = vtium_conformance::describe();
    let major = |text: &str| {
        text.lines()
            .next()
            .and_then(|l| l.strip_prefix("interface "))
            .and_then(|v| v.split('.').next().map(str::to_owned))
    };
    if major(block) != major(&ours) {
        return Err(format!(
            "the interface is {:?}, vtium-replay {:?}",
            major(block),
            major(&ours)
        ));
    }
    let mut theirs = block.lines().skip(1);
    for line in ours.lines().skip(1) {
        if !theirs.any(|l| l == line) {
            return Err(format!(
                "--describe line {line:?} is not in the describe block, or out of its order"
            ));
        }
    }
    for line in block.lines().filter_map(|l| l.strip_prefix("feature ")) {
        if !OPTIONAL.contains(&line) && !ours.lines().any(|l| l == format!("feature {line}")) {
            return Err(format!("--describe lacks the mandatory feature {line}"));
        }
    }
    Ok(())
}

fn run(o: &Options) -> Result<bool, String> {
    let pins = pins()?;
    let corpus = corpus(o, &pins)?;
    let expected = expectations(&corpus, &o.without)?;
    check_describe(&read(&corpus.root.join("INTERFACE.md"))?)?;
    let pin = lists::pin(&pins, "libtsm")?;
    println!(
        "conformance: {} cases at libtsm {}",
        corpus.cases.len(),
        pin.commit
    );
    let mut ok = true;
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
    let corpus = corpus(o, &pins()?)?;
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

/// The capability probes: the C reference's `run.py` at the pinned kmscon
/// commit, judging vtium's capability list through vtium-replay.
fn probes(o: &Options) -> Result<bool, String> {
    let pins = pins()?;
    let corpus = corpus(o, &pins)?;
    let kmscon = source::extract(
        &o.kmscon,
        lists::pin(&pins, "kmscon")?,
        &[CAPABILITIES],
        &o.out,
    )?;
    let caps = kmscon.join(CAPABILITIES);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let replay = exe.with_file_name("vtium-replay");
    if !replay.exists() {
        return Err(format!(
            "{} is not built: cargo build -p vtium-conformance --bins",
            replay.display()
        ));
    }
    let status = std::process::Command::new("python3")
        .arg("-I")
        .arg(caps.join("run.py"))
        .arg("--list")
        .arg(Path::new(TOOL).join("capabilities.list"))
        // vtium is built without images until vtium-image lands (#48, #49)
        .arg("--build")
        .arg("images=false")
        .arg("--replay")
        .arg(&replay)
        .arg("--probes")
        .arg(caps.join("probes.replay"))
        .arg("--spec")
        .arg(corpus.root.join("INTERFACE.md"))
        .status()
        .map_err(|e| format!("python3: {e}"))?;
    Ok(status.success())
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
            "probes" if o.rest.is_empty() => probes(&o),
            "issues" if o.rest.is_empty() => issues(),
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
