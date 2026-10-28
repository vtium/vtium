//! The interface check: the copied interface, goldens and examples hold
//! for vtium-replay, and each kind of fault is caught on inputs written
//! here.

use std::collections::BTreeSet;
use std::path::Path;

use vtium_conformance::interface::{self, Tally};
use vtium_conformance::runner::Corpus;
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

fn read(file: &str) -> String {
    std::fs::read_to_string(Path::new(TOOL).join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// The interface's documents: 1.0 as copied, and vtium's additions.
fn texts() -> Vec<String> {
    vec![read("corpus/INTERFACE.md"), read("INTERFACE-vtium.md")]
}

fn spec(texts: &[String]) -> Spec {
    Spec::read(&texts.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_else(|e| panic!("{e}"))
}

fn strs(texts: &[String]) -> Vec<&str> {
    texts.iter().map(String::as_str).collect()
}

/// The features vtium-replay declares.
fn declared() -> BTreeSet<String> {
    vtium_conformance::describe()
        .lines()
        .filter_map(|l| l.strip_prefix("feature "))
        .map(str::to_owned)
        .collect()
}

/// A good screen view of a 4 by 2 screen.
const GOOD: &str = "size 4x2\ncursor 0,0\nflags none\nrow 0 |ab|\nattr 0 0-1 bold\nrow 1 ||\n";

fn faults(tally: &Tally) -> String {
    tally.faults.join("\n")
}

#[test]
fn the_interface_holds_for_vtium_replay_on_the_copied_corpus() {
    let texts = texts();
    let spec = spec(&texts);
    let corpus = Corpus::open(&Path::new(TOOL).join("corpus")).unwrap_or_else(|e| panic!("{e}"));
    let check = interface::check(
        &spec,
        &strs(&texts),
        &corpus,
        &vtium_conformance::describe(),
    );
    assert!(check.faults().is_empty(), "{}", check.faults().join("\n"));
    assert!(check.goldens > 0, "no golden was read");
    assert!(check.examples.count > 0, "no example ran");
    assert_eq!(check.palettes.count, 9);
    assert!(check.refusals.count > 0 && check.controls.count > 0);
}

#[test]
fn a_good_output_parses_and_each_broken_one_is_caught() {
    let spec = spec(&texts());
    assert_eq!(
        interface::check_output(&spec, "screen", GOOD, 4, 2, false),
        None
    );
    for (what, text) in [
        ("a skipped row", GOOD.replace("row 1 ||\n", "")),
        ("a row number out of order", GOOD.replace("row 1", "row 2")),
        (
            "a run past the columns",
            GOOD.replace("0-1 bold", "3-4 bold"),
        ),
        (
            "a run continuing the one before",
            GOOD.replace("attr 0 0-1 bold", "attr 0 0-0 bold\nattr 0 1-1 bold"),
        ),
        (
            "lines out of order",
            GOOD.replace("cursor 0,0\nflags none", "flags none\ncursor 0,0"),
        ),
        (
            "a line of no kind",
            GOOD.replace("flags none", "flags nothing"),
        ),
        ("no final newline", GOOD.trim_end().to_owned()),
        (
            "a checkpoint going back",
            format!("checkpoint 5\n{GOOD}checkpoint 4\n{GOOD}"),
        ),
    ] {
        assert!(
            interface::check_output(&spec, "screen", &text, 4, 2, false).is_some(),
            "{what} passes"
        );
    }
}

#[test]
fn a_frame_view_takes_its_size_from_its_size_line() {
    let spec = spec(&texts());
    let frame = "size 2x1\ncursor 0,0\nflags none\npalette legacy fg=#e5e5e5 bg=#000000\nchanged 2\n\
                 frame-cursor 0,0 default hidden\nrow 0 ||\n";
    assert_eq!(
        interface::check_output(&spec, "frame", frame, 9, 9, false),
        None
    );
    let wide = frame.replace(
        "row 0 ||",
        "row 0 |ab|\ncells 0 0-2 w=1 fg=#000000 bg=#000000 ul=#000000",
    );
    assert!(interface::check_output(&spec, "frame", &wide, 9, 9, false).is_some());
}

#[test]
fn an_example_whose_output_differs_is_caught_and_one_needing_more_is_left_out() {
    let texts = texts();
    let spec = spec(&texts);
    let declared = declared();
    let good = interface::examples(&strs(&texts), &spec, &declared);
    assert!(good.faults.is_empty(), "{}", faults(&good));
    let bad = texts[0].replace("row 0 |red wavy|", "row 0 |red wavx|");
    let tally = interface::examples(&[bad.as_str()], &spec, &declared);
    assert!(
        tally.faults.iter().any(|f| f.starts_with("example screen")),
        "{}",
        faults(&tally)
    );
    let none = Default::default();
    let tally = interface::examples(&[texts[0].as_str()], &spec, &none);
    assert_eq!(tally.count, 0);
    assert!(tally.skipped > 0);
    assert!(tally.faults.iter().any(|f| f == "no worked example ran"));
}

#[test]
fn an_output_block_without_its_example_is_caught() {
    let texts = texts();
    let spec = spec(&texts);
    let lone = format!("{}\n```output lone\nsize 1x1\n```\n", texts[0]);
    let tally = interface::examples(&[lone.as_str()], &spec, &declared());
    assert!(
        tally.faults.iter().any(|f| f.starts_with("output lone")),
        "{}",
        faults(&tally)
    );
}

#[test]
fn a_palette_row_vtium_resolves_otherwise_is_caught() {
    let text = texts().remove(0);
    let good = interface::palettes(&[text.as_str()]);
    assert!(good.faults.is_empty(), "{}", faults(&good));
    let bad = text.replacen("vga 000000 aa0000", "vga 000000 ab0000", 1);
    let tally = interface::palettes(&[bad.as_str()]);
    assert_eq!(tally.faults.len(), 1, "{}", faults(&tally));
    let short = text.replacen("vga 000000 aa0000 ", "vga ", 1);
    assert_eq!(interface::palettes(&[short.as_str()]).faults.len(), 1);
}

#[test]
fn a_refusal_exiting_otherwise_is_caught() {
    let good = interface::refusals(interface::exit_code);
    assert!(good.faults.is_empty(), "{}", faults(&good));
    let lax = interface::refusals(|_| 0);
    assert!(
        lax.faults
            .iter()
            .any(|f| f == "refusal: COLS with a leading zero exits 0, not 2"),
        "{}",
        faults(&lax)
    );
}

#[test]
fn the_grammar_controls_hold_and_a_loose_grammar_fails_them() {
    let texts = texts();
    let good = interface::controls(&spec(&texts));
    assert!(good.faults.is_empty(), "{}", faults(&good));
    let loose = texts[0].replace(
        "line attr screen attr {N} {N}-{N}{ATTR}",
        "line attr screen attr {N} {N}-{N}.*",
    );
    let tally = interface::controls(&spec(&[loose, texts[1].clone()]));
    assert!(
        tally.faults.iter().any(|f| f.contains("keys out of order")),
        "{}",
        faults(&tally)
    );
}
