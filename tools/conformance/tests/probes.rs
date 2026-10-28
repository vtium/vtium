//! The copies against `provenance.tsv`, and the probe runner's checks and
//! verdicts on probes written here, judged through the copied interface.

use std::path::Path;

use vtium_conformance::coverage::Capability;
use vtium_conformance::probes::{self, Outcome, Replay};
use vtium_conformance::runner::provenance;
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(TOOL).join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn spec() -> Spec {
    Spec::read(&[&read("corpus/INTERFACE.md"), &read("INTERFACE-vtium.md")])
        .unwrap_or_else(|e| panic!("{e}"))
}

fn cap(name: &str, status: &str, issue: &str) -> Capability {
    Capability {
        name: name.to_owned(),
        status: status.to_owned(),
        issue: issue.to_owned(),
        rows: Vec::new(),
        note: String::new(),
    }
}

/// The outcome of the one probe in `text`.
fn outcome(text: &str) -> Outcome {
    let spec = spec();
    let probes = probes::read("t.replay", text, &spec).unwrap_or_else(|e| panic!("{e}"));
    let replay =
        Replay::new(&spec, &vtium_conformance::describe()).unwrap_or_else(|e| panic!("{e}"));
    replay.run(&probes[0])
}

#[test]
fn the_copies_match_their_provenance() {
    let copies = provenance::read(&read("provenance.tsv")).unwrap_or_else(|e| panic!("{e}"));
    let root = Path::new(TOOL).join("../..");
    let faults = provenance::check(
        &root,
        &copies,
        &[
            "tools/conformance/corpus",
            "tools/conformance/probes",
            "tools/conformance/generator",
        ],
        &["tools/conformance/probes/vtium.replay"],
    );
    assert_eq!(faults, Vec::<String>::new());
    assert!(provenance::commit(&copies, "libtsm").is_ok());
    assert!(provenance::commit(&copies, "kmscon").is_ok());
}

#[test]
fn the_probe_files_read_and_their_expectations_check() {
    let spec = spec();
    let caps = vtium_conformance::coverage::capabilities(&read("capabilities.list"))
        .unwrap_or_else(|e| panic!("{e}"));
    probes::check_list(&caps).unwrap_or_else(|e| panic!("{e}"));
    let copied = probes::read("probes.replay", &read("probes/probes.replay"), &spec)
        .unwrap_or_else(|e| panic!("{e}"));
    let ours = probes::read("vtium.replay", &read("probes/vtium.replay"), &spec)
        .unwrap_or_else(|e| panic!("{e}"));
    let all = probes::merge(copied, ours).unwrap_or_else(|e| panic!("{e}"));
    for probe in &all {
        let missing = caps
            .iter()
            .any(|c| c.name == probe.name && c.status == "missing");
        probes::check_expectations(probe.file(), probe, &spec, missing)
            .unwrap_or_else(|e| panic!("{e}"));
    }
}

const BOLD: &str = "probe bold\nsize 4 1\nsend \"\\e[1mx\"\ncheck\nwant attr 0 0-0 bold\n";

#[test]
fn a_probe_passes_when_its_expectations_hold_and_fails_when_one_does_not() {
    assert_eq!(outcome(BOLD), Outcome::Pass);
    let Outcome::Fail(why) = outcome(&BOLD.replace("bold\n", "italic\n")) else {
        panic!("a want vtium does not print passed");
    };
    assert!(
        why.starts_with("line 5, block 1: no \"attr 0 0-0 italic\""),
        "{why}"
    );
    let reject = BOLD.replace("want attr", "reject-re attr 0 .*\nwant attr");
    assert!(matches!(outcome(&reject), Outcome::Fail(_)));
}

#[test]
fn replies_and_test_lines_are_judged_in_order() {
    let da1 = "probe da1\nsize 4 1\nsend \"\\e[c\"\ncheck\nreply-re \"\\e\\\\[\\\\?.*c\"\n";
    assert_eq!(outcome(da1), Outcome::Pass);
    let none = "probe none\nsize 4 1\nsend \"x\"\ncheck\nreply \"\\e[?1c\"\n";
    assert!(matches!(outcome(none), Outcome::Fail(why) if why.contains("replied \"\"")));
    let tests = "probe frames\nsize 4 1\ncmd frame-since;caller=0\ncmd frame-since;caller=0\ncheck\nwant test frame-since 0 changed N\nwant test frame-since 0 changed 0\n";
    assert_eq!(outcome(tests), Outcome::Pass);
    let reversed = tests.replace(
        "changed N\nwant test frame-since 0 changed 0",
        "changed 0\nwant test frame-since 0 changed N",
    );
    assert!(
        matches!(outcome(&reversed), Outcome::Fail(_)),
        "test lines out of order"
    );
}

#[test]
fn a_probe_is_absent_without_its_feature_and_a_placeholder_fails() {
    let sixel = "probe s\nsize 4 1\nneeds sixel\nsend \"x\"\ncheck\nwant row 0 |x|\n";
    assert!(matches!(outcome(sixel), Outcome::Fail(why) if why.starts_with("absent: ")));
    assert!(matches!(
        outcome("probe p\nplaceholder none yet\n"),
        Outcome::Fail(_)
    ));
}

#[test]
fn malformed_probes_are_refused_before_anything_runs() {
    let spec = spec();
    let refused = |text: &str| probes::read("t.replay", text, &spec).is_err();
    assert!(refused("size 4 1\n"), "a key before the first probe");
    assert!(
        refused("probe a\nsize 4 1\nsend \"x\"\nwant row 0 |x|\n"),
        "no check"
    );
    assert!(
        refused("probe a\nsize 4 1\nsend \"x\"\ncheck\n"),
        "nothing to fail it"
    );
    assert!(
        refused("probe a\nsend \"x\"\ncheck\nwant row 0 |x|\n"),
        "no size"
    );
    assert!(refused(
        "probe a\nsize 4 1\ncmd no-such\ncheck\nwant row 0 ||\n"
    ));
    assert!(refused(
        "probe a\nsize 4 1\nneeds no-such\nsend \"x\"\ncheck\nwant row 0 |x|\n"
    ));
    assert!(
        refused("probe a\nplaceholder p\nsend \"x\"\n"),
        "a placeholder that runs"
    );
    assert!(refused(&format!("{BOLD}{BOLD}")), "a probe twice");
    let probe = |text: &str| {
        probes::read("t.replay", text, &spec).unwrap_or_else(|e| panic!("{e}"))[0].clone()
    };
    let check = |text: &str, missing: bool| {
        probes::check_expectations("t.replay", &probe(text), &spec, missing).is_ok()
    };
    assert!(check(BOLD, false));
    assert!(
        !check(&BOLD.replace("bold\n", "bold overline\n"), false),
        "a reserved key"
    );
    assert!(check(&BOLD.replace("bold\n", "bold overline\n"), true));
    assert!(!check(
        &BOLD.replace("want attr 0 0-0 bold", "want attr 0 0-0 blod"),
        false
    ));
    let counted = "probe c\nsize 4 1\nview frame\nsend \"x\"\ncheck\nwant changed 4\n";
    assert!(!check(counted, false), "an unmasked count");
    assert!(check(&counted.replace("changed 4", "changed N"), false));
    let re = "probe r\nsize 4 1\nsend \"x\"\ncheck\nwant-re row 0 \\|y\\|\n";
    assert!(check(re, false));
    assert!(
        !check(re, true),
        "a missing probe's pattern needs an example"
    );
    assert!(
        !check(&format!("{re}example \"row 0 |x|\"\n"), true),
        "an example it misses"
    );
    assert!(check(&format!("{re}example \"row 0 |y|\"\n"), true));
}

#[test]
fn vtium_probes_take_the_place_of_placeholders_only() {
    let spec = spec();
    let read = |text: &str| probes::read("t.replay", text, &spec).unwrap_or_else(|e| panic!("{e}"));
    let copied = read("probe p\nplaceholder later\n");
    let merged = probes::merge(copied, read(&BOLD.replace("probe bold", "probe p")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(merged.len(), 1);
    assert!(!merged[0].is_placeholder());
    assert!(
        probes::merge(read(BOLD), read(BOLD)).is_err(),
        "a real probe replaced"
    );
}

#[test]
fn each_verdict_agrees_with_the_list_or_fails() {
    let build = [("images", false)];
    let agrees = |c: &Capability, o: Outcome| probes::verdict(c, &o, &build).1;
    let fail = || Outcome::Fail("no".to_owned());
    assert!(agrees(&cap("a", "works", "-"), Outcome::Pass));
    assert!(!agrees(&cap("a", "works", "-"), fail()));
    assert!(agrees(&cap("a", "missing", "#1"), fail()));
    assert!(
        !agrees(&cap("a", "missing", "#1"), Outcome::Pass),
        "a missing probe passes"
    );
    assert!(
        agrees(&cap("a", "works", "-Dimages"), fail()),
        "absent without images"
    );
    assert!(!agrees(&cap("a", "works", "-Dimages"), Outcome::Pass));
    assert!(
        !agrees(&cap("a", "works", "-Dvideo"), fail()),
        "a feature the build lacks"
    );
    assert!(!agrees(
        &cap("a", "works", "-"),
        Outcome::Unjudged("exit 3".to_owned())
    ));
    assert!(probes::check_list(&[cap("a", "missing", "-")]).is_err());
    assert!(probes::check_list(&[cap("a", "works", "#3")]).is_err());
}
