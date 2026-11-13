//! `conformance scenarios`, and the scenarios' part of `conformance run`
//! and `conformance interface` (ADR-1504): the behaviour scenarios played
//! and judged against their transcripts, `--write` for a new transcript,
//! `--play` for a reviewer, and `--bin` for another replay program.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use vtium_conformance::probes::Outcome;
use vtium_conformance::runner::own;
use vtium_conformance::scenario::play::{self, Keys};
use vtium_conformance::scenario::run::{Runner, Verdict, verdict, write_transcript};
use vtium_conformance::scenario::{self, Scenario, Sort, transcript};
use vtium_conformance::spec::Spec;

use super::{TOOL, read};

/// The scenarios, each `NAME.scenario` with a behaviour scenario's
/// transcript, `NAME.transcript`, beside it.
pub const DIR: &str = "scenarios";
/// The behaviour scenarios whose transcripts wait for the maintainer's
/// review, `NAME REASON` a line.
pub const PROVISIONAL: &str = "scenarios/provisional.tsv";

pub const USAGE: &str = "       conformance scenarios [NAME...]
       conformance scenarios --write [--replace] NAME...
       conformance scenarios --play NAME...
       conformance scenarios --bin NAME DIR";

fn dir() -> PathBuf {
    Path::new(TOOL).join(DIR)
}

fn transcript_path(name: &str) -> PathBuf {
    dir().join(format!("{name}.transcript"))
}

/// Every scenario under `scenarios/`, in the order of their names. A
/// transcript with no scenario beside it stops the run.
pub fn all() -> Result<Vec<Scenario>, String> {
    let entries = std::fs::read_dir(dir()).map_err(|e| format!("{DIR}/: {e}"))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    let mut out = Vec::new();
    for path in &paths {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match path.extension().and_then(|x| x.to_str()) {
            Some("scenario") => {
                let file = format!("{DIR}/{name}.scenario");
                out.push(scenario::parse(&name, &file, &read(path)?)?);
            }
            Some("transcript") if !path.with_extension("scenario").is_file() => {
                return Err(format!(
                    "{DIR}/{name}.transcript has no scenario beside it. A transcript is the golden of {DIR}/{name}.scenario."
                ));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The provisional list, each name checked against the behaviour
/// scenarios.
pub fn provisional(scenarios: &[Scenario]) -> Result<BTreeMap<String, String>, String> {
    let list = own::provisional(&read(&Path::new(TOOL).join(PROVISIONAL))?)
        .map_err(|e| format!("{DIR}/{e}"))?;
    for name in list.keys() {
        if !scenarios
            .iter()
            .any(|s| &s.name == name && s.sort == Sort::Behaviour)
        {
            return Err(format!(
                "{PROVISIONAL} names {name}, which is no behaviour scenario in {DIR}/."
            ));
        }
    }
    Ok(list)
}

/// The interface check over the scenarios: each behaviour scenario's
/// assertions against the grammar of its view, and each transcript's dumps
/// against the grammar of the views they print.
pub fn faults(spec: &Spec, scenarios: &[Scenario]) -> Vec<String> {
    let mut faults = Vec::new();
    for s in scenarios.iter().filter(|s| s.sort == Sort::Behaviour) {
        if let Err(e) = scenario::run::check_assertions(s, spec) {
            faults.push(e);
        }
        if let Ok(text) = std::fs::read_to_string(transcript_path(&s.name))
            && let Some(fault) = transcript::check(spec, s, &text)
        {
            faults.push(format!("{DIR}/{}.transcript: {fault}", s.name));
        }
    }
    faults
}

/// Judges `scenarios`' behaviour scenarios against their transcripts and
/// puts each through the chunking check; prints each failure, each
/// provisional transcript that differs and a summary. Returns whether
/// every verdict holds.
pub fn judge(
    runner: &Runner,
    scenarios: &[Scenario],
    provisional: &BTreeMap<String, String>,
    seed: u64,
    rounds: usize,
) -> bool {
    let (mut pass, mut fail, mut drift, mut chunked) = (0, 0, 0, 0);
    let behaviour: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| s.sort == Sort::Behaviour)
        .collect();
    for s in &behaviour {
        let reason = provisional.get(&s.name);
        let on_file = std::fs::read_to_string(transcript_path(&s.name)).ok();
        let played = runner.play(s, &[]);
        let playable = played.is_ok();
        let mut ok = true;
        match verdict(&played, on_file.as_deref(), reason.is_some()) {
            Verdict::Pass => {}
            Verdict::Provisional(why) => {
                drift += 1;
                println!(
                    "provisional scenario {}: Its transcript judges nothing until the maintainer reviews it ({}). {why}",
                    s.name,
                    reason.map_or("", String::as_str)
                );
            }
            Verdict::Fail(why) => {
                ok = false;
                println!("FAIL scenario {}: {}", s.name, why.replace("NAME", &s.name));
            }
        }
        // a scenario that cannot be played has one failure, printed above
        if !playable {
            fail += 1;
            continue;
        }
        match runner.chunking(s, seed, rounds) {
            Ok(()) => chunked += 1,
            Err(why) => {
                ok = false;
                println!("FAIL chunking scenario {}: {why}", s.name);
            }
        }
        if ok {
            pass += 1;
        } else {
            fail += 1;
        }
    }
    let listed = behaviour
        .iter()
        .filter(|s| provisional.contains_key(&s.name))
        .count();
    let capabilities = scenarios.len().saturating_sub(behaviour.len());
    println!(
        "scenarios: {} behaviour scenarios, {pass} pass, {fail} FAIL, {listed} provisional, {drift} differ from a provisional transcript; {chunked} unchanged by extra cuts; {capabilities} capability scenarios, which conformance probes judges",
        behaviour.len()
    );
    fail == 0
}

/// The scenarios `names` names, or every one when `names` is empty.
fn chosen<'a>(scenarios: &'a [Scenario], names: &[String]) -> Result<Vec<&'a Scenario>, String> {
    if names.is_empty() {
        return Ok(scenarios.iter().collect());
    }
    names
        .iter()
        .map(|n| {
            let n = n.strip_suffix(".scenario").unwrap_or(n);
            let n = n.rsplit('/').next().unwrap_or(n);
            scenarios
                .iter()
                .find(|s| s.name == n)
                .ok_or_else(|| format!("{DIR}/ holds no scenario named {n}."))
        })
        .collect()
}

/// `--write [--replace] NAME...`: plays each behaviour scenario and writes
/// its transcript. A new transcript goes on the provisional list; an
/// existing one is refused unless `--replace` says to rewrite it.
fn write(
    runner: &Runner,
    scenarios: &[Scenario],
    names: &[String],
    replace: bool,
) -> Result<bool, String> {
    if names.is_empty() {
        return Err("`--write` names the scenarios whose transcripts it writes.".to_owned());
    }
    let list_path = Path::new(TOOL).join(PROVISIONAL);
    for s in chosen(scenarios, names)? {
        if s.sort != Sort::Behaviour {
            return Err(format!(
                "{} is a capability scenario. Its assertions judge it, and it has no transcript.",
                s.name
            ));
        }
        if transcript_path(&s.name).exists() && !replace {
            return Err(format!(
                "{DIR}/{}.transcript exists. A transcript changes only in a golden commit, written with `--write --replace {}`.",
                s.name, s.name
            ));
        }
        let played = runner
            .play(s, &[])
            .map_err(|why| format!("{}: {why}", s.name))?;
        if let Outcome::Fail(why) | Outcome::Unjudged(why) = &played.assertions {
            return Err(format!(
                "{}: an assertion fails, so its transcript is not written: {why}",
                s.name
            ));
        }
        let listed = write_transcript(&dir(), &list_path, &s.name, &played.transcript, replace)?;
        println!("wrote {DIR}/{}.transcript", s.name);
        if listed {
            println!("listed {} in {PROVISIONAL}", s.name);
        }
    }
    Ok(true)
}

/// `--bin NAME DIR`: writes the behaviour scenario's replay input as
/// `NAME.bin` and `NAME.cuts`, its fixtures in `NAME.files/`, and prints
/// the command line that plays it.
fn bin(runner: &Runner, scenarios: &[Scenario], args: &[String]) -> Result<bool, String> {
    let [name, out] = args else {
        return Err(format!(
            "`--bin` takes a scenario's name and a directory.\n{USAGE}"
        ));
    };
    let s = chosen(scenarios, std::slice::from_ref(name))?[0];
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (input, _) = runner.input(s, vtium_conformance::view::View::Screen, &[]);
    let cuts: String = input.cuts.iter().map(|c| format!("{c}\n")).collect();
    let bin = out.join(format!("{}.bin", s.name));
    let cuts_path = out.join(format!("{}.cuts", s.name));
    std::fs::write(&bin, &input.data).map_err(|e| format!("{}: {e}", bin.display()))?;
    std::fs::write(&cuts_path, cuts).map_err(|e| format!("{}: {e}", cuts_path.display()))?;
    if !s.fixtures.is_empty() {
        let files = out.join(format!("{}.files", s.name));
        std::fs::create_dir_all(&files).map_err(|e| format!("{}: {e}", files.display()))?;
        for f in &s.fixtures {
            std::fs::copy(Path::new(TOOL).join("fixtures").join(f), files.join(f))
                .map_err(|e| format!("fixtures/{f}: {e}"))?;
        }
    }
    let frame = if s.views().contains(&scenario::Dump::Frame) {
        format!(
            "\nvtium-replay --frame {} {} {} {}",
            s.cols,
            s.rows,
            bin.display(),
            cuts_path.display()
        )
    } else {
        String::new()
    };
    println!(
        "wrote {} and {}; play them with\nvtium-replay {} {} {} {}{frame}",
        bin.display(),
        cuts_path.display(),
        s.cols,
        s.rows,
        bin.display(),
        cuts_path.display()
    );
    Ok(true)
}

/// `--play NAME...`: plays each behaviour scenario for a reviewer.
fn play_all(runner: &Runner, scenarios: &[Scenario], names: &[String]) -> Result<bool, String> {
    if names.is_empty() {
        return Err("`--play` names the scenarios it plays.".to_owned());
    }
    let list = provisional(scenarios)?;
    let keys = Keys::new();
    let mut stdout = std::io::stdout();
    for s in chosen(scenarios, names)? {
        let on_file = std::fs::read_to_string(transcript_path(&s.name)).ok();
        let more = play::play(
            runner,
            s,
            on_file.as_deref(),
            list.get(&s.name).map(String::as_str),
            &mut stdout,
            &mut || keys.wait(),
        )?;
        if !more {
            break;
        }
    }
    let _ = stdout.flush();
    Ok(true)
}

/// `conformance scenarios ...`.
pub fn command(runner: &Runner, args: &[String], seed: u64, rounds: usize) -> Result<bool, String> {
    let scenarios = all()?;
    let (flags, names): (Vec<&String>, Vec<&String>) =
        args.iter().partition(|a| a.starts_with("--"));
    let names: Vec<String> = names.into_iter().cloned().collect();
    let flags: Vec<&str> = flags.iter().map(|f| f.as_str()).collect();
    match flags[..] {
        [] => {
            let list = provisional(&scenarios)?;
            let chosen: Vec<Scenario> = chosen(&scenarios, &names)?.into_iter().cloned().collect();
            let mut ok = true;
            for fault in faults(runner.spec(), &chosen) {
                ok = false;
                println!("FAIL replay-interface: {fault}");
            }
            Ok(judge(runner, &chosen, &list, seed, rounds) && ok)
        }
        ["--write"] => write(runner, &scenarios, &names, false),
        ["--write", "--replace"] | ["--replace", "--write"] => {
            write(runner, &scenarios, &names, true)
        }
        ["--play"] => play_all(runner, &scenarios, &names),
        ["--bin"] => bin(runner, &scenarios, &names),
        _ => Err(format!("unknown options {}\n{USAGE}", flags.join(" "))),
    }
}
