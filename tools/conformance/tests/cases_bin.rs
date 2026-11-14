//! `conformance cases --bin`: the files it writes for vtium-replay are the
//! input the runner replays for that case, so a case run by hand and the
//! case the runner judges cannot drift apart.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use vtium_conformance::replay;
use vtium_conformance::runner::{self, Corpus};
use vtium_conformance::view::View;

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

/// The corpus and vtium's own cases, as the runner opens them.
fn sets() -> [Corpus; 2] {
    ["corpus", "vtium"].map(|dir| {
        Corpus::open(&Path::new(TOOL).join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}"))
    })
}

/// Each file of a fixture directory, by name, with its bytes.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .map(|e| {
            let path = e
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
                .path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (name, bytes)
        })
        .collect()
}

/// The views the runner judges a case in: the display view too in a build
/// with bidi, when the case has a golden of it.
fn views(set: &Corpus, case: &runner::Case) -> Vec<(View, &'static str)> {
    let both = [(View::Screen, ""), (View::Frame, "--frame")];
    #[cfg(feature = "bidi")]
    if set
        .root
        .join("golden-display")
        .join(format!("{}.txt", case.name))
        .is_file()
    {
        return both
            .into_iter()
            .chain([(View::Display, "--display")])
            .collect();
    }
    let _ = (set, case);
    both.to_vec()
}

// Every case of the corpus and of vtium's own cases, readable or a
// stream, is written as the stream, cuts and fixtures the runner feeds the
// replay. vtium-replay, given the written files on its command line,
// prints in each view what the runner's replay of that case prints.
#[test]
fn each_case_written_for_vtium_replay_is_the_input_the_runner_replays() {
    let out = Scratch::new("cases-bin-each");
    let mut readable = 0usize;
    let mut fixtures = 0usize;
    for set in &sets() {
        for case in &set.cases {
            let want = set
                .input(case, View::Screen)
                .unwrap_or_else(|e| panic!("{e}"));
            let written = set
                .write_input(case, &out)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert_eq!(written.bin, out.join(format!("{}.bin", case.name)));
            assert_eq!(written.cuts, out.join(format!("{}.cuts", case.name)));
            let data = std::fs::read(&written.bin).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                data == want.data,
                "{}: the written stream differs from the stream the runner replays",
                case.name
            );
            let cuts = std::fs::read(&written.cuts).unwrap_or_else(|e| panic!("{e}"));
            let cuts = replay::cuts(&cuts, data.len())
                .unwrap_or_else(|f| panic!("{}: {}", case.name, f.message));
            assert_eq!(cuts, want.cuts, "{}: the written cuts", case.name);
            let theirs = files(&want.fixtures);
            assert_eq!(
                files(&replay::Input::fixtures_for(&written.bin)),
                theirs,
                "{}: the written fixtures",
                case.name
            );
            if !theirs.is_empty() {
                fixtures += 1;
            }
            if set
                .root
                .join("cases")
                .join(format!("{}.case", case.name))
                .is_file()
            {
                readable += 1;
            }
            for (view, flag) in views(set, case) {
                let mut args: Vec<String> = flag
                    .split_whitespace()
                    .map(str::to_owned)
                    .chain([case.cols.to_string(), case.rows.to_string()])
                    .collect();
                args.push(written.bin.display().to_string());
                args.push(written.cuts.display().to_string());
                let by_hand = vtium_conformance::main(&args);
                let runner = replay::run(&set.input(case, view).unwrap_or_else(|e| panic!("{e}")));
                assert!(
                    by_hand == runner,
                    "{} in the view {view:?}: vtium-replay {} prints what the runner's replay does not",
                    case.name,
                    args.join(" ")
                );
            }
        }
    }
    // the corpus's two readable cases, vtium's own, and the cases with
    // fixtures all went through the check above
    assert!(readable > 100, "{readable} readable cases");
    assert!(fixtures > 10, "{fixtures} cases with fixtures");
}

// A name that no cases.list holds is refused with where the names are, and
// with the command for a behaviour scenario, whose name is often meant.
#[test]
fn an_unknown_case_name_says_where_the_case_names_are() {
    let [corpus, own] = sets();
    let Err(why) = runner::find(&[&corpus, &own], "key-no-such-case") else {
        panic!("key-no-such-case is found");
    };
    assert!(
        why.starts_with("No case is named key-no-such-case."),
        "{why}"
    );
    for want in [
        "corpus/cases.list",
        "vtium/cases.list",
        "conformance scenarios --bin",
    ] {
        assert!(why.contains(want), "{want:?} is not in {why:?}");
    }
    let (set, case) = runner::find(&[&corpus, &own], "alt-bce").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (set.root.as_path(), case.name.as_str()),
        (own.root.as_path(), "alt-bce")
    );
}

// The command line: `conformance cases --bin NAME DIR` writes the files
// and prints the vtium-replay line of each view the runner judges; an
// unknown name exits 2 with the reason.
#[test]
fn conformance_cases_bin_writes_the_files_and_prints_how_to_play_them() {
    let out = Scratch::new("cases-bin-command");
    let run = |name: &str| {
        Command::new(env!("CARGO_BIN_EXE_conformance"))
            .args(["cases", "--bin", name])
            .arg(&*out)
            .output()
            .unwrap_or_else(|e| panic!("conformance: {e}"))
    };
    let done = run("kitty-upload-file");
    let stdout = String::from_utf8_lossy(&done.stdout);
    assert!(
        done.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let bin = out.join("kitty-upload-file.bin");
    let cuts = out.join("kitty-upload-file.cuts");
    assert!(bin.is_file() && cuts.is_file(), "{stdout}");
    assert!(
        out.join("kitty-upload-file.files").is_dir(),
        "the fixtures are not written:\n{stdout}"
    );
    let [_, own] = sets();
    let case = own
        .cases
        .iter()
        .find(|c| c.name == "kitty-upload-file")
        .unwrap_or_else(|| panic!("kitty-upload-file is not in vtium/cases.list"));
    for flag in ["", "--frame "] {
        let line = format!(
            "vtium-replay {flag}{} {} {} {}",
            case.cols,
            case.rows,
            bin.display(),
            cuts.display()
        );
        assert!(
            stdout.lines().any(|l| l == line),
            "no line {line:?} in:\n{stdout}"
        );
    }
    assert!(
        stdout.contains("vtium/golden/kitty-upload-file.txt"),
        "{stdout}"
    );

    let refused = run("key-no-such-case");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.starts_with("conformance: No case is named key-no-such-case."),
        "{stderr}"
    );
}
