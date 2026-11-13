//! The runner's verdicts and the replay's command line, on a corpus
//! written here, so they run without the checked-in corpus.

use std::path::{Path, PathBuf};
use std::process::Command;

use vtium_conformance::runner::{Corpus, Verdict, judge, mask};
use vtium_conformance::view::View;

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

/// vtium's screen view of `ab` on a 4 by 1 screen, written out by hand.
const AB: &str = "size 4x1\ncursor 2,0\nflags AUTO_WRAP\nrow 0 |ab|\n";

/// A corpus of one case, `ab`, in a directory of its own, which goes when
/// the [`Scratch`] returned with it is dropped.
fn corpus(name: &str) -> (Scratch, Corpus) {
    let root = Scratch::new(&format!("conformance-{name}"));
    let write = |path: PathBuf, text: &[u8]| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    };
    write(root.join("cases.list"), b"# name cols rows\nab 4 1\n");
    write(root.join("cases/ab.bin"), b"ab");
    write(root.join("golden/ab.txt"), AB.as_bytes());
    let corpus = Corpus::open(&root).unwrap_or_else(|e| panic!("{e}"));
    (root, corpus)
}

// A test that panics after it made its corpus still leaves no directory
// behind: the guard's drop runs as the panic unwinds.
#[test]
fn a_corpus_directory_is_removed_when_the_test_panics() {
    let (tx, rx) = std::sync::mpsc::channel();
    let ended = std::thread::spawn(move || {
        let (root, _) = corpus("panics");
        let _ = tx.send(root.to_path_buf());
        panic!("the test fails after it made its corpus");
    })
    .join();
    assert!(ended.is_err(), "the test panicked");
    let root = rx.recv().unwrap_or_else(|e| panic!("{e}"));
    assert!(!root.exists(), "{} is left behind", root.display());
}

fn replay(corpus: &Corpus) -> Result<String, vtium_conformance::Failure> {
    let input = corpus
        .input(&corpus.cases[0], View::Screen)
        .unwrap_or_else(|e| panic!("{e}"));
    vtium_conformance::replay::run(&input)
}

#[test]
fn a_case_that_matches_its_golden_passes() {
    let (_dir, c) = corpus("match");
    assert_eq!(judge(&replay(&c), AB), Verdict::Pass);
}

// A planted difference from a golden fails, and only a masked count may
// differ.
#[test]
fn a_difference_from_the_golden_fails() {
    let (_dir, c) = corpus("plant");
    let got = replay(&c);
    assert!(matches!(
        judge(&got, &AB.replace("|ab|", "|ac|")),
        Verdict::Fail(_)
    ));
    assert!(matches!(
        judge(&got, &AB.replace("cursor 2,0", "cursor 1,0")),
        Verdict::Fail(_)
    ));
    let counted = Ok(format!("{AB}changed 3\n"));
    assert_eq!(judge(&counted, &format!("{AB}changed 4\n")), Verdict::Pass);
    assert!(matches!(
        judge(&counted, &format!("{AB}changed 0\n")),
        Verdict::Fail(_)
    ));
}

#[test]
fn a_replay_that_exits_fails() {
    let failure = Err(vtium_conformance::Failure {
        code: 2,
        message: "test command image needs feature images".to_owned(),
        output: "checkpoint 1\n".to_owned(),
    });
    assert!(matches!(judge(&failure, AB), Verdict::Fail(_)));
}

#[test]
fn the_counts_an_implementation_may_draw_differently_are_masked() {
    let text = "changed 12\nchanged 0\ntest frame-since 3 changed 7\ntest frame-since 3 changed 0\nplacement 0 row 0 col 1 image 0 opaque changed\nrow 0 |changed 5|\n";
    assert_eq!(
        mask(text),
        "changed N\nchanged 0\ntest frame-since 3 changed N\ntest frame-since 3 changed 0\nplacement 0 row 0 col 1 image 0 opaque\nrow 0 |changed 5|\n"
    );
}

fn vtium_replay(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_vtium-replay"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("vtium-replay: {e}"));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn the_command_line_exits_as_the_interface_says() {
    let (_dir, c) = corpus("cli");
    let bin = c.root.join("cases/ab.bin");
    let bin = bin.to_str().unwrap_or_default();
    assert_eq!(vtium_replay(&["4", "1", bin]), (Some(0), AB.to_owned()));
    let (code, out) = vtium_replay(&["--describe"]);
    assert_eq!(code, Some(0));
    assert!(
        out.starts_with("interface 1.10\nview screen\nview frame\n"),
        "{out}"
    );
    // interface 1.6's host commands and the bell line, each with its
    // feature
    for line in [
        "feature selection\nfeature marks\nfeature links\nfeature backspace\nfeature state\nfeature bell\n",
        "command select selection\ncommand copy selection\ncommand prompt marks\ncommand link links\ncommand hover links\ncommand backspace backspace\ncommand state state\n",
    ] {
        assert!(out.contains(line), "{out}");
    }
    for args in [
        &["0", "1", bin][..],
        &["2049", "1", bin],
        &["4", "01", bin],
        &["4", "1", "/nonexistent.bin"],
        &["--draw2", "4", "1", bin],
        &["4", "1"],
        &["4", "1", bin, "/nonexistent.cuts"],
    ] {
        assert_eq!(vtium_replay(args).0, Some(2), "{args:?}");
    }
}

#[test]
fn the_fixture_directory_sits_beside_the_stream() {
    assert_eq!(
        vtium_conformance::replay::Input::fixtures_for(Path::new("cases/host-identify.bin")),
        PathBuf::from("cases/host-identify.files")
    );
}

/// The corpus of [`corpus`] with `ab` written as a readable case, the
/// scenario language's `size`, `send` and `dump` lines under a header, in
/// place of its stream.
fn readable(name: &str, source: &str) -> (Scratch, Result<Corpus, String>) {
    let (root, _) = corpus(name);
    let _ = std::fs::remove_file(root.join("cases/ab.bin"));
    let _ = std::fs::write(root.join("cases/ab.case"), source);
    let opened = Corpus::open(&root);
    (root, opened)
}

fn readable_input(corpus: &Corpus) -> Result<vtium_conformance::replay::Input, String> {
    corpus.input(&corpus.cases[0], View::Screen)
}

#[test]
fn a_readable_case_is_judged_against_its_golden_as_a_stream_case_is() {
    let (_dir, c) = readable("readable", "# spec: ECMA-48\nsize 4 1\nsend \"ab\"\n");
    let c = c.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(judge(&replay(&c), AB), Verdict::Pass);
}

#[test]
fn a_readable_case_whose_bytes_differ_from_its_golden_fails() {
    let (_dir, c) = readable("readable-plant", "# spec: ECMA-48\nsize 4 1\nsend \"ac\"\n");
    let c = c.unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(judge(&replay(&c), AB), Verdict::Fail(_)));
}

// A `dump state` adds the state command and a cut after it; a `send` adds
// no cut, and a dump at the stream's end adds none, since the replay
// prints a block there anyway.
#[test]
fn a_readable_case_cuts_after_each_dump_and_nowhere_else() {
    let source =
        "# spec: ECMA-48\nsize 4 1\nsend \"a\"\ndump state\nsend \"b\"\nsend \"\\r\"\ndump state\n";
    let (_dir, c) = readable("readable-cuts", source);
    let c = c.unwrap_or_else(|e| panic!("{e}"));
    let input = readable_input(&c).unwrap_or_else(|e| panic!("{e}"));
    let state = b"\x1b_tsm-test;state\x1b\\";
    let mut want = b"a".to_vec();
    want.extend_from_slice(state);
    let cut = want.len();
    want.extend_from_slice(b"b\r");
    want.extend_from_slice(state);
    assert_eq!(input.data, want);
    assert_eq!(input.cuts, vec![cut]);
    assert_eq!((input.cols, input.rows), (4, 1));
}

#[test]
fn a_readable_case_is_refused_when_its_form_is_wrong() {
    for (name, source, why) in [
        (
            "readable-size",
            "# spec: x\nsize 5 1\nsend \"ab\"\n",
            "5 by 1",
        ),
        (
            "readable-spec",
            "size 4 1\nsend \"ab\"\n",
            "no `# spec:` line",
        ),
        (
            "readable-check",
            "# spec: x\nsize 4 1\nsend \"ab\"\ncheck\nwant row 0 |ab|\n",
            "golden",
        ),
        (
            "readable-capability",
            "# spec: x\n# capability: ab\nsize 4 1\n",
            "capability",
        ),
    ] {
        let (_dir, c) = readable(name, source);
        let c = c.unwrap_or_else(|e| panic!("{e}"));
        let err = readable_input(&c).map(|_| ()).unwrap_err();
        assert!(err.contains(why), "{name}: {err}");
    }
    let (dir, c) = readable("readable-both", "# spec: x\nsize 4 1\nsend \"ab\"\n");
    let _ = std::fs::write(dir.join("cases/ab.bin"), b"ab");
    let c = c.unwrap_or_else(|e| panic!("{e}"));
    let err = readable_input(&c).map(|_| ()).unwrap_err();
    assert!(err.contains("both"), "{err}");
}
