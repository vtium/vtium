//! The parse oracle: vtium-parse against the C reference's parser.
//!
//! `build.rs` extracts the C reference's parser from the libtsm commit
//! `pin.tsv` names and compiles it with `harness/shim.c`. Each stream is
//! played through it (`c_side`) and through vtium-parse (`rust_side`), and
//! the two traces are compared line by line. The C side applies each
//! departure `departures.tsv` lists, so any other difference fails the run.
//!
//! ```text
//! parse-oracle run       the differential over the corpus and the generator
//! parse-oracle selftest  shows the run turns red for every plant
//! parse-oracle replay F  the differential over a saved script
//! ```

mod c_side;
mod departures;
mod generate;
#[allow(
    dead_code,
    reason = "finding the commit and the corpus is build.rs's part"
)]
mod pin;
mod plants;
mod rust_side;
mod script;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use c_side::CSide;
use departures::{Departs, Departure};
use rust_side::RustSide;
use script::Stream;

const USAGE: &str = "\
usage: parse-oracle run [--seed N] [--streams N] [--without DEPARTURE]... [--out DIR]
       parse-oracle selftest [--seed N] [--streams N] [--out DIR]
       parse-oracle replay SCRIPT [--without DEPARTURE]...

  --seed N             the generator's seed (default 1)
  --streams N          generated streams after the corpus (default 3000)
  --without DEPARTURE  take a departure out of the expectations
  --out DIR            where differences and plant builds go (default: out/ here)";

struct Options {
    command: String,
    script: Option<PathBuf>,
    seed: u64,
    streams: usize,
    without: Vec<Departure>,
    out: PathBuf,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Options {
        command: args.next().ok_or(USAGE)?,
        script: None,
        seed: 1,
        streams: 3000,
        without: Vec::new(),
        out: Path::new(env!("CARGO_MANIFEST_DIR")).join("out"),
    };
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--seed" => o.seed = value()?.parse().map_err(|_| "--seed takes a number")?,
            "--streams" => o.streams = value()?.parse().map_err(|_| "--streams takes a number")?,
            "--without" => o.without.push(Departure::from_id(&value()?)?),
            "--out" => o.out = PathBuf::from(value()?),
            _ if o.command == "replay" && o.script.is_none() => o.script = Some(arg.into()),
            _ => return Err(format!("unknown argument {arg}\n{USAGE}")),
        }
    }
    Ok(o)
}

/// The first difference between the traces of a stream.
struct Difference {
    label: String,
    line: usize,
    c: Vec<String>,
    rust: Vec<String>,
}

/// What a run compared.
struct Outcome {
    streams: usize,
    lines: usize,
    difference: Option<Difference>,
}

/// Plays each stream through both sides until the first difference, whose
/// stream is saved to `save`.
fn compare(streams: impl Iterator<Item = Stream>, departs: Departs, save: &Path) -> Outcome {
    let mut outcome = Outcome {
        streams: 0,
        lines: 0,
        difference: None,
    };
    for (i, mut stream) in streams.enumerate() {
        stream.number = i as u64;
        let c = script::play(&mut CSide::new(stream.mode, departs), &stream);
        let rust = script::play(&mut RustSide::new(stream.mode, stream.number), &stream);
        if c != rust {
            let (c, rust): (Vec<&str>, Vec<&str>) = (c.lines().collect(), rust.lines().collect());
            let line = c.iter().zip(&rust).take_while(|(a, b)| a == b).count();
            let around = |lines: &[&str]| {
                lines[line.saturating_sub(3)..(line + 3).min(lines.len())]
                    .iter()
                    .map(|l| l.to_string())
                    .collect()
            };
            if let Some(dir) = save.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(save, script::encode(&[&stream]));
            outcome.difference = Some(Difference {
                label: stream.label,
                line,
                c: around(&c),
                rust: around(&rust),
            });
            return outcome;
        }
        outcome.streams += 1;
        outcome.lines += c.lines().count();
    }
    outcome
}

/// Every stream a run compares: the corpus, the cases, then the generator.
fn streams(seed: u64, n: usize) -> impl Iterator<Item = Stream> {
    generate::corpus()
        .chain(generate::cases())
        .chain(generate::generated(seed, n))
}

fn departs(expected: &[Departure], without: &[Departure]) -> Departs {
    expected
        .iter()
        .filter(|d| !without.contains(d))
        .fold(Departs::default(), |all, &d| all.with(d))
}

fn report_difference(d: &Difference, save: &Path) {
    println!("DIFFERENCE in {}, at trace line {}", d.label, d.line + 1);
    println!("  C reference:");
    d.c.iter().for_each(|l| println!("    {l}"));
    println!("  vtium-parse:");
    d.rust.iter().for_each(|l| println!("    {l}"));
    println!(
        "  the stream's script: {} (parse-oracle replay replays it)",
        save.display()
    );
}

fn run(o: &Options) -> Result<u8, String> {
    let departs = departs(&departures::expected()?, &o.without);
    println!(
        "C reference: libtsm {} ({} conformance inputs)",
        &env!("ORACLE_COMMIT")[..12],
        generate::corpus_len()
    );
    println!("departures expected: {}", departs.ids().join(", "));
    #[expect(
        clippy::disallowed_methods,
        reason = "the run's duration is reported, never compared"
    )]
    let started = Instant::now();
    let save = o.out.join("difference.script");
    let outcome = compare(streams(o.seed, o.streams), departs, &save);
    if let Some(d) = &outcome.difference {
        report_difference(d, &save);
        return Ok(1);
    }
    println!(
        "agree: {} streams, {} trace lines, no unlisted difference ({:.1}s)",
        outcome.streams,
        outcome.lines,
        started.elapsed().as_secs_f64()
    );
    Ok(0)
}

fn replay(o: &Options) -> Result<u8, String> {
    let path = o.script.as_ref().ok_or(USAGE)?;
    let script = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let streams = script::decode(&script, &path.display().to_string())?;
    let departs = departs(&departures::expected()?, &o.without);
    let save = o.out.join("replayed.script");
    let outcome = compare(streams.into_iter(), departs, &save);
    match &outcome.difference {
        Some(d) => {
            report_difference(d, &save);
            Ok(1)
        }
        None => {
            println!(
                "agree: {} streams, {} trace lines",
                outcome.streams, outcome.lines
            );
            Ok(0)
        }
    }
}

fn selftest(o: &Options) -> Result<u8, String> {
    let expected = departures::expected()?;
    let all = departs(&expected, &[]);
    let save = o.out.join("difference.script");
    let mut missed = Vec::new();
    let mut expect = |what: &str, red: bool, detail: String| {
        println!(
            "  {:5}  {what}: {detail}",
            if red { "red" } else { "GREEN" }
        );
        if !red {
            missed.push(what.to_string());
        }
    };
    let found = |outcome: &Outcome| match &outcome.difference {
        Some(d) => format!("differs at stream {}, {}", outcome.streams + 1, d.label),
        None => "no difference".into(),
    };

    println!("baseline, every departure expected:");
    let baseline = compare(streams(o.seed, o.streams), all, &save);
    if let Some(d) = &baseline.difference {
        report_difference(d, &save);
        println!("selftest: the baseline differs, so nothing else means anything");
        return Ok(1);
    }
    println!(
        "  agree  {} streams, {} trace lines",
        baseline.streams, baseline.lines
    );

    println!("each departure taken out of the expectations:");
    for &d in &expected {
        let outcome = compare(streams(o.seed, o.streams), departs(&expected, &[d]), &save);
        expect(d.id(), outcome.difference.is_some(), found(&outcome));
    }

    println!("each defect planted in vtium-parse:");
    for plant in plants::PLANTS {
        let binary = plants::build(plant, &o.out)?;
        let out = Command::new(&binary)
            .args([
                "run",
                "--seed",
                &o.seed.to_string(),
                "--streams",
                &o.streams.to_string(),
            ])
            .arg("--out")
            .arg(o.out.join("plants").join(plant.name).join("out"))
            .output()
            .map_err(|e| format!("{}: {e}", binary.display()))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = stdout
            .lines()
            .find(|l| l.starts_with("DIFFERENCE"))
            .unwrap_or("no difference")
            .to_string();
        expect(plant.name, out.status.code() == Some(1), detail);
    }

    println!("each range moved a line up and a line down:");
    let pin = pin::read_pin(include_str!("../pin.tsv"))?;
    let ranges = pin::read_ranges(include_str!("../extract.tsv"))?;
    let gitdir = Path::new(env!("ORACLE_LIBTSM_GIT"));
    for (i, r) in ranges.iter().enumerate() {
        let extracted: Vec<&str> = [(-1, "-1"), (1, "+1")]
            .into_iter()
            .filter(|&(shift, _)| {
                let mut moved = ranges.clone();
                moved[i].first = r.first.saturating_add_signed(shift);
                moved[i].last = r.last.saturating_add_signed(shift);
                pin::extract(gitdir, &pin.commit, &moved, &o.out.join("moved")).is_ok()
            })
            .map(|(_, name)| name)
            .collect();
        let detail = if extracted.is_empty() {
            "refused both".to_string()
        } else {
            format!("extracted at {}", extracted.join(", "))
        };
        expect(&r.name, extracted.is_empty(), detail);
    }

    if !missed.is_empty() {
        println!(
            "selftest: {} not caught: {}",
            missed.len(),
            missed.join(", ")
        );
        return Ok(1);
    }
    println!("selftest: every departure, plant and moved range caught");
    Ok(0)
}

fn main() -> ExitCode {
    let result = options().and_then(|o| {
        c_side::check()?;
        match o.command.as_str() {
            "run" => run(&o),
            "selftest" => selftest(&o),
            "replay" => replay(&o),
            _ => Err(USAGE.into()),
        }
    });
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("parse-oracle: refused: {e}");
            ExitCode::from(2)
        }
    }
}
