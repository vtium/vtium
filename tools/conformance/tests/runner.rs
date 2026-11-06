//! The runner's verdicts and the replay's command line, on a corpus
//! written here, so they run without the checked-in corpus.

use std::path::{Path, PathBuf};
use std::process::Command;

use vtium_conformance::runner::{Corpus, Verdict, judge, mask};
use vtium_conformance::view::View;

/// vtium's screen view of `ab` on a 4 by 1 screen, written out by hand.
const AB: &str = "size 4x1\ncursor 2,0\nflags AUTO_WRAP\nrow 0 |ab|\n";

/// A test's own directory, removed when the test ends, whether it passes
/// or fails.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A corpus of one case, `ab`, in a directory of its own, which goes when
/// the [`Scratch`] returned with it is dropped.
fn corpus(name: &str) -> (Scratch, Corpus) {
    let root =
        std::env::temp_dir().join(format!("vtium-conformance-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let scratch = Scratch(root.clone());
    let write = |path: PathBuf, text: &[u8]| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    };
    write(root.join("cases.list"), b"# name cols rows\nab 4 1\n");
    write(root.join("cases/ab.bin"), b"ab");
    write(root.join("golden/ab.txt"), AB.as_bytes());
    (
        scratch,
        Corpus::open(&root).unwrap_or_else(|e| panic!("{e}")),
    )
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
        out.starts_with("interface 1.2\nview screen\nview frame\n"),
        "{out}"
    );
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
