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
       conformance expect VIEW CASE
       conformance probes
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
        seed: 1,
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
    let mut chunked = 0;
    for case in &corpus.cases {
        let input = corpus.input(case, vtium_conformance::view::View::Screen)?;
        match chunking::check(&input, o.seed, o.rounds) {
            Ok(()) => chunked += 1,
            // a case the replay refuses has failed above already
            Err(why) if why.starts_with("exits ") => {}
            Err(why) => {
                ok = false;
                println!("FAIL chunking {}: {why}", case.name);
            }
        }
    }
    println!(
        "chunking: {chunked} cases unchanged by {} rounds of extra cuts from seed {}",
        o.rounds, o.seed
    );
    Ok(ok)
}

/// Writes vtium's output for a listed departure to `expected/`, to be read
/// against the golden before it is committed.
fn expect(o: &Options) -> Result<bool, String> {
    let [view, case] = &o.rest[..] else {
        return Err(USAGE.to_owned());
    };
    let corpus = corpus(o, &pins()?)?;
    let rows = lists::expected_rows(&read(&Path::new(TOOL).join("expected.tsv"))?)?;
    let departure = rows
        .iter()
        .any(|r| &r.case == case && !r.pending && (r.view == "*" || &r.view == view));
    let pending = rows
        .iter()
        .any(|r| &r.case == case && r.pending && (r.view == "*" || &r.view == view));
    if !departure || pending {
        return Err(format!(
            "{case} has no departure row for the {view} view in expected.tsv"
        ));
    }
    let (v, _, _) = VIEWS
        .iter()
        .find(|(_, n, _)| n == view)
        .ok_or_else(|| format!("no view {view}"))?;
    let c = corpus
        .cases
        .iter()
        .find(|c| &c.name == case)
        .ok_or_else(|| format!("no case {case}"))?;
    let out = vtium_conformance::replay::run(&corpus.input(c, *v)?).map_err(|f| f.message)?;
    let dir = Path::new(TOOL).join("expected").join(view);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{case}.txt"));
    std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "wrote {}; read it against {}/golden*/{case}.txt",
        path.display(),
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
