//! Plays a tier 1 case or scenario in the terminal it runs in, one screen
//! at a time, the live path of `make tier1-shots`' pictures:
//!
//! ```text
//! tier1-play case NAME
//! tier1-play scenario NAME
//! ```
//!
//! A case is one of `tools/conformance/corpus/` or
//! `tools/conformance/vtium/`. The player waits for a key after each
//! screen, and q stops it. On a standard input that is not a terminal it
//! does not wait. It exits 0 when it played the case or the scenario, and
//! 2 when it refuses to run or cannot find the name.

use std::path::Path;
use std::process::ExitCode;

use vtium_conformance::runner::{Corpus, own};
use vtium_conformance::scenario::play::{self, Keys};
use vtium_conformance::scenario::run::Runner;
use vtium_conformance::spec::Spec;
use vtium_tier1_shots::select::{self, SCENARIOS, SETS};
use vtium_tier1_shots::{CONFORMANCE, play as player, root};

const USAGE: &str = "usage: tier1-play case NAME\n       tier1-play scenario NAME";
/// The interface's documents, each a later version of the one before, as
/// `conformance` reads them.
const INTERFACE: [&str; 2] = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"];

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn case(tier1: &Path, name: &str) -> Result<(), String> {
    for set in SETS {
        let corpus = Corpus::open(&tier1.join(set))?;
        if let Some(c) = corpus.cases.iter().find(|c| c.name == name) {
            let keys = Keys::new();
            let label = format!("{CONFORMANCE}/{set}");
            player::case(&corpus, c, &label, &mut std::io::stdout(), &mut || {
                keys.wait()
            })?;
            return Ok(());
        }
    }
    Err(format!(
        "{name} is no case of {CONFORMANCE}/corpus/cases.list or {CONFORMANCE}/vtium/cases.list"
    ))
}

fn scenario(tier1: &Path, name: &str) -> Result<(), String> {
    let dir = tier1.join(SCENARIOS);
    let s = select::scenarios(&dir)?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("{name} is no scenario in {CONFORMANCE}/{SCENARIOS}/"))?;
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
    let listed = own::provisional(&read(&dir.join("provisional.tsv"))?)?;
    let on_file = std::fs::read_to_string(dir.join(format!("{name}.transcript"))).ok();
    let keys = Keys::new();
    play::play(
        &runner,
        &s,
        on_file.as_deref(),
        listed.get(name).map(String::as_str),
        &mut std::io::stdout(),
        &mut || keys.wait(),
    )?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // the goldens hold what vtium-replay built with images and bidi prints
    if !vtium_conformance::command::IMAGES || !vtium_conformance::BIDI {
        eprintln!(
            "tier1-play plays vtium-replay built with images and bidi: build it with --features draw, as make tier1-play does"
        );
        return ExitCode::from(2);
    }
    let tier1 = root().join(CONFORMANCE);
    let played = match args.as_slice() {
        [kind, name] if kind == "case" => case(&tier1, name),
        [kind, name] if kind == "scenario" => scenario(&tier1, name),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match played {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("tier1-play: {why}");
            ExitCode::from(2)
        }
    }
}
