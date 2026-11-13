//! The generator against the copied corpus: every synthetic case it writes is the copy byte for byte, its stream, its cuts and its fixtures. Every case of `cases.list` that the generator does not write is a recording or a readable case, `corpus/cases/NAME.case`. A readable case is the only source of its case.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtium_conformance::generate::{self, Inputs};

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

fn cases_dir() -> PathBuf {
    Path::new(TOOL).join("corpus/cases")
}

/// The files of a case's fixture directory, by name, or none.
fn fixtures(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .map(|e| {
            let path = e.unwrap_or_else(|e| panic!("{e}")).path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (name, data)
        })
        .collect()
}

#[test]
fn the_generator_reproduces_every_synthetic_case() {
    let inputs = Inputs::read(&Path::new(TOOL).join("generator")).unwrap_or_else(|e| panic!("{e}"));
    let made = generate::cases(&inputs);
    let dir = cases_dir();
    let mut faults = Vec::new();
    for case in &made {
        let readable = dir.join(format!("{}.case", case.name));
        if readable.exists() {
            faults.push(format!(
                "{}: The corpus holds this case as a readable case, {}, and the generator still writes it. Remove the case's recipe from src/generate/.",
                case.name,
                readable.display()
            ));
            continue;
        }
        let bin = dir.join(format!("{}.bin", case.name));
        match std::fs::read(&bin) {
            Ok(copy) if copy == case.stream => {}
            Ok(copy) => {
                let at = copy
                    .iter()
                    .zip(&case.stream)
                    .position(|(a, b)| a != b)
                    .unwrap_or(copy.len().min(case.stream.len()));
                faults.push(format!(
                    "{}: the stream differs from the copy at byte {at} ({} bytes, the copy {})",
                    case.name,
                    case.stream.len(),
                    copy.len()
                ));
            }
            Err(e) => faults.push(format!("{}: {e}", bin.display())),
        }
        let cuts = std::fs::read_to_string(dir.join(format!("{}.cuts", case.name))).ok();
        if cuts.as_deref() != case.cuts_file().as_deref() {
            faults.push(format!(
                "{}: cuts {:?}, the copy {cuts:?}",
                case.name,
                case.cuts_file()
            ));
        }
        let copied = fixtures(&dir.join(format!("{}.files", case.name)));
        if copied != case.files {
            faults.push(format!(
                "{}: fixtures {:?}, the copy {:?}",
                case.name,
                case.files.keys().collect::<Vec<_>>(),
                copied.keys().collect::<Vec<_>>()
            ));
        }
    }
    assert_eq!(faults, Vec::<String>::new());

    let list = std::fs::read_to_string(Path::new(TOOL).join("corpus/cases.list"))
        .unwrap_or_else(|e| panic!("{e}"));
    let unmade: Vec<&str> = list
        .lines()
        .filter_map(|l| l.split('#').next()?.split_whitespace().next())
        .filter(|name| !made.iter().any(|c| c.name == *name))
        .filter(|name| !dir.join(format!("{name}.case")).exists())
        .collect();
    assert_eq!(unmade, ["prog-shell", "prog-less", "prog-vim", "prog-tmux"]);
    for case in &made {
        let line = format!("{} {} {}", case.name, case.cols, case.rows);
        assert!(
            list.lines().any(|l| l == line),
            "cases.list has no line {line:?}"
        );
    }
}

#[test]
fn the_generator_refuses_inputs_it_lacks() {
    let empty = Scratch::new("generate");
    let got = Inputs::read(&empty);
    let why = got.err().unwrap_or_default();
    assert!(why.contains("recorded/icat-detect.bin"), "{why}");
}

#[test]
fn the_generator_writes_into_an_empty_directory_only() {
    let dir = Scratch::new("generate-write");
    let inputs = Inputs::read(&Path::new(TOOL).join("generator")).unwrap_or_else(|e| panic!("{e}"));
    let cases = generate::cases(&inputs);
    let first = generate::write(&dir, &cases);
    let again = generate::write(&dir, &cases);
    assert_eq!(first.map(|list| list.lines().count()), Ok(158));
    let why = again.err().unwrap_or_default();
    assert!(why.contains("is not empty"), "{why}");
}
