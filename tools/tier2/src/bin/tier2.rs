//! tier2: draw the screens and judge them, draw them only, or compare two
//! images.
//!
//! ```text
//! tier2 run [--out DIR]
//! tier2 render [--out DIR]
//! tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]
//! tier2 fetch-fonts
//! ```
//!
//! `fetch-fonts` fetches the outline fonts the outline screens draw with
//! into their cache and checks each against its SHA-256 (`fonts`); it exits
//! 0 when every font is in place and 1 when one is not.
//!
//! `run` exits 0 when every case passes, 1 when one does not and 2 when it
//! refuses to run; `compare` exits 0 on pass or near, 1 on fail and 2 on
//! an input it refuses.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_tier2::compare;
use vtium_tier2::draw;
use vtium_tier2::ppm;
#[cfg(feature = "gpu")]
use vtium_tier2::report::CaseReport;
use vtium_tier2::report::Status;
use vtium_tier2::runner::{self, Run};
use vtium_tier2::screens::{CASES, WAITING};
use vtium_tier2::tol::Tolerance;

const HERE: &str = env!("CARGO_MANIFEST_DIR");

fn usage() -> ExitCode {
    eprintln!(
        "usage: tier2 run [--out DIR]\n       tier2 render [--out DIR]\n       \
         tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]\n       \
         tier2 fetch-fonts"
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
    if report.exists()
        && let Err(e) = std::fs::remove_file(&report)
    {
        return refuse(format!("{}: {e}", report.display()));
    }
    let lists = match runner::load(&root) {
        Ok(x) => x,
        Err(e) => return refuse(e),
    };
    let goldens = here.join("goldens");
    let r = Run {
        cases: CASES,
        goldens: &goldens,
        pending: &lists.pending,
        provisional: &lists.provisional,
        out: &out,
        root: &out,
    };
    if let Err(e) = r.check_lists() {
        return refuse(e);
    }
    #[cfg_attr(
        not(feature = "gpu"),
        expect(unused_mut, reason = "the GPU cases join them")
    )]
    let mut cases = match r.cases() {
        Ok(c) => c,
        Err(e) => return refuse(e),
    };
    #[cfg(feature = "gpu")]
    cases.extend(gpu_cases(here, &out));
    #[cfg(not(feature = "gpu"))]
    println!("GPU path not built: no @gpu cases (make tier2 builds tier 2 with --features gpu)");
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
    // vtium's commit, as `make tier2` passes it; a run started by hand
    // records none
    let tools: Vec<(&str, String)> = std::env::var("T2_VTIUM_REV")
        .ok()
        .filter(|rev| !rev.is_empty())
        .map(|rev| ("vtium", rev))
        .into_iter()
        .collect();
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

/// Each screen's case on the GPU path, drawn on Mesa's llvmpipe; one error
/// case when llvmpipe cannot be set up, which fails the run.
#[cfg(feature = "gpu")]
fn gpu_cases(here: &Path, out: &Path) -> Vec<CaseReport> {
    let screens: Vec<_> = CASES.iter().map(|c| c.screen).collect();
    match vtium_tier2::gpu::llvmpipe() {
        Ok(mut gl) => vtium_tier2::gpu::reports(
            &screens,
            here,
            &mut gl,
            (&out.join("actual"), &out.join("diff")),
        ),
        Err(e) => {
            let mut r = CaseReport::new(&vtium_tier2::gpu::case_name("*"), Status::Error);
            r.note = Some(e);
            vec![r]
        }
    }
}

fn render(opts: &[(String, String)]) -> ExitCode {
    let out = opt(opts, "--out").unwrap_or_else(|| Path::new(HERE).join("out/render"));
    if let Err(e) = std::fs::create_dir_all(&out) {
        return refuse(format!("{}: {e}", out.display()));
    }
    let mut failed = false;
    // the screens that wait on a defect's fix draw too, so it can be seen
    for case in CASES.iter().chain(WAITING) {
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
        if let Some(d) = opt(opts, "--diff")
            && c.pixels > 0
            && c.size_mismatch.is_none()
        {
            std::fs::write(&d, compare::diff_image(&a, &tol, &c.points).to_ppm())
                .map_err(|e| format!("{}: {e}", d.display()))?;
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

fn fetch_fonts() -> ExitCode {
    let dir = vtium_tier2::fonts::cache_dir();
    match vtium_tier2::fonts::fetch(&dir, &vtium_tier2::fonts::FONTS) {
        Ok(()) => {
            println!("tier2: the outline fonts are in {}", dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tier2: {e}");
            ExitCode::FAILURE
        }
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
        "fetch-fonts" => &[],
        _ => return usage(),
    };
    let Some((opts, files)) = split(rest, flags) else {
        return usage();
    };
    match cmd.as_str() {
        "run" if files.is_empty() => run(&opts),
        "render" if files.is_empty() => render(&opts),
        "compare" => compare_cmd(&opts, &files),
        "fetch-fonts" if files.is_empty() => fetch_fonts(),
        _ => usage(),
    }
}
