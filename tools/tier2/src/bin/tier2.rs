//! tier2: draw the screens and judge them, draw them only, or compare two
//! images.
//!
//! ```text
//! tier2 run [--out DIR]
//! tier2 render [--out DIR]
//! tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]
//! tier2 provenance [--kmscon DIR]
//! ```
//!
//! `provenance` checks each copy of the C reference's files against its
//! source in a kmscon checkout, read with `git show` and `git ls-tree` and
//! nothing else, for a review of a change to the copies; no gate runs it.
//!
//! `run` exits 0 when every case passes, 1 when one does not and 2 when it
//! refuses to run; `compare` exits 0 on pass or near, 1 on fail and 2 on
//! an input it refuses.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use vtium_conformance::runner::provenance;
use vtium_tier2::compare;
use vtium_tier2::draw;
use vtium_tier2::ppm;
use vtium_tier2::report::Status;
use vtium_tier2::runner::{self, Run};
use vtium_tier2::screens::CASES;
use vtium_tier2::tol::Tolerance;

const HERE: &str = env!("CARGO_MANIFEST_DIR");

fn usage() -> ExitCode {
    eprintln!(
        "usage: tier2 run [--out DIR]\n       tier2 render [--out DIR]\n       \
         tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]\n       \
         tier2 provenance [--kmscon DIR]"
    );
    ExitCode::from(2)
}

/// Each `--flag` given with its value.
type Opts = Vec<(String, String)>;

/// The value of each `--flag VALUE` in `args`, and the other arguments.
fn split(args: &[String], flags: &[&str]) -> Option<(Opts, Vec<String>)> {
    let (mut opts, mut rest) = (Vec::new(), Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if flags.contains(&a.as_str()) {
            opts.push((a.clone(), it.next()?.clone()));
        } else if a.starts_with("--") {
            return None;
        } else {
            rest.push(a.clone());
        }
    }
    Some((opts, rest))
}

fn opt(opts: &[(String, String)], flag: &str) -> Option<PathBuf> {
    opts.iter()
        .rev()
        .find(|(f, _)| f == flag)
        .map(|(_, v)| PathBuf::from(v))
}

fn refuse(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("tier2: {e}");
    ExitCode::from(2)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(opts: &[(String, String)]) -> ExitCode {
    let here = Path::new(HERE);
    let root = here.join("../..");
    let out = opt(opts, "--out").unwrap_or_else(|| here.join("out"));
    let report = out.join("report.json");
    // a stale report never stands in for this run's
    if report.exists() {
        if let Err(e) = std::fs::remove_file(&report) {
            return refuse(format!("{}: {e}", report.display()));
        }
    }
    let lists = match runner::load(&root, CASES) {
        Ok(x) => x,
        Err(e) => return refuse(e),
    };
    let goldens = here.join("reference/goldens");
    let vtium_goldens = here.join("goldens");
    let r = Run {
        cases: CASES,
        kmscon_goldens: &goldens,
        vtium_goldens: &vtium_goldens,
        departures: &lists.departures,
        pending: &lists.pending,
        provisional: &lists.provisional,
        c_goldens: &lists.c_goldens,
        out: &out,
        root: &out,
    };
    if let Err(e) = r.check_lists(&lists.titles) {
        return refuse(e);
    }
    let cases = match r.cases() {
        Ok(c) => c,
        Err(e) => return refuse(e),
    };
    for c in &cases {
        let mut line = format!("{:<11} {}", c.status.name(), c.name);
        if c.pixels > 0 {
            line.push_str(&format!(
                ": {} pixel(s) differ, max channel delta {}",
                c.pixels, c.max_delta
            ));
        }
        if let Some(n) = &c.note {
            line.push_str(&format!(" ({n})"));
        }
        println!("{line}");
    }
    let tools = [("kmscon", lists.commit.clone())];
    if let Err(e) = runner::write_report(&report, &tools, &cases) {
        return refuse(e);
    }
    let bad = cases.iter().filter(|c| !c.status.passes()).count();
    let count = |st: Status| cases.iter().filter(|c| c.status == st).count();
    let (pending, provisional) = (count(Status::Pending), count(Status::Provisional));
    println!(
        "tier2: {} case(s) judged, {provisional} provisional, {pending} pending, {bad} not passing; report {}",
        cases.len() - pending - provisional,
        report.display()
    );
    if bad > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn render(opts: &[(String, String)]) -> ExitCode {
    let out = opt(opts, "--out").unwrap_or_else(|| Path::new(HERE).join("out/render"));
    if let Err(e) = std::fs::create_dir_all(&out) {
        return refuse(format!("{}: {e}", out.display()));
    }
    let mut failed = false;
    for case in CASES {
        let s = &case.screen;
        match draw::draw(s) {
            Ok(img) => {
                let path = out.join(format!("{}.ppm", s.name));
                if let Err(e) = std::fs::write(&path, img.to_ppm()) {
                    return refuse(format!("{}: {e}", path.display()));
                }
                println!("{}", path.display());
            }
            Err(e) => {
                eprintln!("tier2: {}: {e}", s.name);
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn compare_cmd(opts: &[(String, String)], files: &[String]) -> ExitCode {
    let [golden, actual] = files else {
        return usage();
    };
    let load = |p: &str| -> Result<ppm::Image, String> {
        let bytes = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
        ppm::parse(&bytes).map_err(|e| format!("{p}: {e}"))
    };
    let result = || -> Result<_, String> {
        let tol = match opt(opts, "--tol") {
            Some(p) => Tolerance::parse(&read(&p)?, &p.display().to_string())?,
            None => Tolerance::default(),
        };
        let (g, a) = (load(golden)?, load(actual)?);
        let c = compare::compare(&g, &a, &tol)?;
        if let Some(d) = opt(opts, "--diff") {
            if c.pixels > 0 && c.size_mismatch.is_none() {
                std::fs::write(&d, compare::diff_image(&a, &tol, &c.points).to_ppm())
                    .map_err(|e| format!("{}: {e}", d.display()))?;
            }
        }
        Ok(c)
    };
    let c = match result() {
        Ok(c) => c,
        Err(e) => return refuse(e),
    };
    println!(
        "{}: {} pixel(s) differ, max channel delta {}",
        c.verdict.name(),
        c.pixels,
        c.max_delta
    );
    if let Some(((gw, gh), (aw, ah))) = c.size_mismatch {
        println!("  size differs: golden {gw}x{gh}, actual {aw}x{ah}");
    }
    for b in &c.regions {
        println!("  region x={}..{} y={}..{}", b.x0, b.x1, b.y0, b.y1);
    }
    if c.verdict == compare::Verdict::Fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `git -C DIR ARGS`'s output, or why it failed.
fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "git -C {} {}: {}",
            dir.display(),
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Each copy against its row of `provenance.tsv` and each row against its
/// source, and the copied goldens with `pending.tsv` against the C
/// reference's tier 2 goldens at the commit, the sources read from the
/// checkout at `--kmscon`.
fn provenance_cmd(opts: &[(String, String)]) -> ExitCode {
    let here = Path::new(HERE);
    let root = here.join("../..");
    let kmscon = opt(opts, "--kmscon").unwrap_or_else(|| root.join("../kmscon"));
    let result = || -> Result<(usize, Vec<String>), String> {
        let (copies, commit) = runner::provenance_rows(&read(&here.join("provenance.tsv"))?)?;
        let listed: BTreeSet<String> = runner::c_goldens(&read(&here.join("c-goldens.list"))?)?
            .into_iter()
            .collect();
        let mut faults = provenance::check(&root, &copies, &[runner::COPIED], &[]);
        faults.extend(provenance::verify(&copies, |_, commit, path| {
            git(&kmscon, &["show", &format!("{commit}:{path}")])
        }));
        let listing = git(
            &kmscon,
            &[
                "ls-tree",
                "--name-only",
                &commit,
                &format!("{}/", runner::GOLDENS),
            ],
        )?;
        let at_pin: BTreeSet<String> = String::from_utf8_lossy(&listing)
            .lines()
            .filter_map(|l| l.rsplit('/').next().map(str::to_owned))
            .collect();
        faults.extend(runner::check_pin(&at_pin, &listed));
        Ok((copies.len(), faults))
    };
    let (rows, faults) = match result() {
        Ok(x) => x,
        Err(e) => return refuse(e),
    };
    for fault in &faults {
        println!("FAIL {fault}");
    }
    println!("tier2 provenance: {rows} rows, {} fault(s)", faults.len());
    if faults.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        return usage();
    };
    let flags: &[&str] = match cmd.as_str() {
        "run" => &["--out"],
        "render" => &["--out"],
        "compare" => &["--tol", "--diff"],
        "provenance" => &["--kmscon"],
        _ => return usage(),
    };
    let Some((opts, files)) = split(rest, flags) else {
        return usage();
    };
    match cmd.as_str() {
        "run" if files.is_empty() => run(&opts),
        "render" if files.is_empty() => render(&opts),
        "compare" => compare_cmd(&opts, &files),
        "provenance" if files.is_empty() => provenance_cmd(&opts),
        _ => usage(),
    }
}
