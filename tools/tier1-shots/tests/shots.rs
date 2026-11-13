//! The tool's replays and drawings on a case and a scenario written here:
//! each picture's size, its match check against goldens written here, and
//! the same bytes from two drawings. The pictures' pixels are the
//! renderer's, which tier 2 judges; these tests judge what the tool adds.
#![cfg(feature = "draw")]

use std::path::{Path, PathBuf};

use vtium_conformance::runner::Corpus;
use vtium_conformance::scenario::{self, run::Runner};
use vtium_conformance::spec::Spec;
use vtium_tier1_shots::judge::{Line, Side};
use vtium_tier1_shots::manifest::Group;
use vtium_tier1_shots::shoot;
use vtium_tier1_shots::{CONFORMANCE, root};

/// `ab` on the first row and a red `c` on the second, at 4 by 2.
const STREAM: &[u8] = b"ab\x1b[2;1H\x1b[31mc";

const SCREEN: &str = "size 4x2
cursor 1,1
flags AUTO_WRAP
row 0 |ab|
row 1 |c|
attr 1 0-0 fg=1
";

const FRAME: &str = "size 4x2
cursor 1,1
flags AUTO_WRAP
palette legacy fg=#e5e5e5 bg=#000000
changed 8
frame-cursor 1,1 default visible w=1 fg=#000000 bg=#e5e5e5 ul=#000000
row 0 |ab|
row 1 |c|
cells 1 0-0 w=1 fg=#cd0000 bg=#000000 ul=#cd0000
";

/// A set of one case, `t`, with `frame` as its frame golden, in a
/// directory of its own under the temporary directory.
fn set(tag: &str, frame: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tier1-shots-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["cases", "golden", "golden-frame"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::write(dir.join("cases.list"), "t 4 2\n").unwrap();
    std::fs::write(dir.join("cases/t.bin"), STREAM).unwrap();
    std::fs::write(dir.join("golden/t.txt"), SCREEN).unwrap();
    std::fs::write(dir.join("golden-frame/t.txt"), frame).unwrap();
    dir
}

#[test]
fn a_case_equal_to_its_goldens_matches_and_draws_at_8x16_cells() {
    let dir = set("match", FRAME);
    let corpus = Corpus::open(&dir).unwrap();
    let (shot, picture) = shoot::case(&corpus, Group::Vtium, &corpus.cases[0], None);
    assert_eq!(shot.error, None);
    let views: Vec<&str> = shot.goldens.iter().map(|g| g.view.as_str()).collect();
    assert_eq!(views, ["screen", "frame"]);
    assert!(shot.matches(), "{:?}", shot.goldens);
    assert_eq!(
        shot.goldens[1].path,
        format!("{CONFORMANCE}/vtium/golden-frame/t.txt")
    );
    let img = picture.expect("a picture");
    assert_eq!((img.width(), img.height()), (32, 32));
    assert_eq!((shot.width, shot.height), (32, 32));
    std::fs::remove_dir_all(&dir).unwrap();
}

// a golden that says `changed 0` where the output says `changed 8` is a
// difference the masking keeps, and the case is still drawn
#[test]
fn a_case_that_differs_from_a_golden_is_drawn_and_names_the_lines() {
    let dir = set("differs", &FRAME.replace("changed 8", "changed 0"));
    let corpus = Corpus::open(&dir).unwrap();
    let (shot, picture) = shoot::case(&corpus, Group::Vtium, &corpus.cases[0], None);
    assert!(picture.is_some());
    assert!(!shot.matches());
    assert_eq!(shot.goldens[0].differs, []);
    assert_eq!(
        shot.goldens[1].differs,
        [
            Line {
                side: Side::Golden,
                number: 5,
                text: "changed 0".to_owned(),
            },
            Line {
                side: Side::Output,
                number: 5,
                text: "changed 8".to_owned(),
            },
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn two_drawings_of_a_case_write_the_same_bytes() {
    let dir = set("twice", FRAME);
    let corpus = Corpus::open(&dir).unwrap();
    let png = || {
        let (_, picture) = shoot::case(&corpus, Group::Vtium, &corpus.cases[0], None);
        shoot::png(&picture.unwrap())
    };
    assert_eq!(png(), png());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// vtium-replay's code as the scenarios meet it, over tier 1's interface
/// documents and fixtures.
fn spec() -> Spec {
    let tier1 = root().join(CONFORMANCE);
    let texts: Vec<String> = ["corpus/INTERFACE.md", "INTERFACE-vtium.md"]
        .iter()
        .map(|f| std::fs::read_to_string(tier1.join(f)).unwrap())
        .collect();
    Spec::read(&texts.iter().map(String::as_str).collect::<Vec<_>>()).unwrap()
}

// each dump is drawn as the screen stands at its command: the first after
// `ab`, the second after the screen grew to 5 by 3 and `c` was written
#[test]
fn each_dump_of_a_scenario_draws_the_screen_at_its_command() {
    let text = "# spec: a test of the dumps' pictures
size 4 2
send \"ab\"
dump screen
resize 5 3
send \"c\"
dump screen
";
    let s = scenario::parse("view-test", "scenarios/view-test.scenario", text).unwrap();
    let spec = spec();
    let runner = Runner::new(
        &spec,
        &vtium_conformance::describe(),
        root().join(CONFORMANCE).join("fixtures"),
    )
    .unwrap();
    let missing = Path::new("/nonexistent/view-test.transcript");
    let shots = shoot::scenario(&runner, &s, missing, None);
    let got: Vec<(String, usize, u32, u32)> = shots
        .iter()
        .map(|(shot, _)| {
            let d = shot.dump.as_ref().unwrap();
            (shot.file(), d.of, shot.width, shot.height)
        })
        .collect();
    assert_eq!(
        got,
        [
            ("scenarios/view-test-01.png".to_owned(), 2, 32, 32),
            ("scenarios/view-test-02.png".to_owned(), 2, 40, 48),
        ]
    );
    // a scenario with no transcript on file is drawn and reported
    let error = shots[0].0.error.as_deref().unwrap_or_default();
    assert!(error.contains("view-test.transcript"), "{error}");
}
