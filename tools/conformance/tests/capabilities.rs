//! The capability judge on capability scenarios written here: each
//! scenario played through vtium-replay's code and judged by its
//! assertions, the checks made before anything runs, and each verdict
//! against the list.

use std::path::{Path, PathBuf};

use vtium_conformance::coverage::Capability;
use vtium_conformance::probes::{self, Outcome};
use vtium_conformance::scenario::{self, Scenario, capability, run::Runner};
use vtium_conformance::spec::Spec;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(TOOL).join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn spec() -> Spec {
    Spec::read(&[&read("corpus/INTERFACE.md"), &read("INTERFACE-vtium.md")])
        .unwrap_or_else(|e| panic!("{e}"))
}

fn runner_in(spec: &Spec, fixtures: PathBuf) -> Runner<'_> {
    Runner::new(spec, &vtium_conformance::describe(), fixtures).unwrap_or_else(|e| panic!("{e}"))
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

/// The capability scenario `t` with this body after its header.
fn scenario(body: &str) -> Scenario {
    scenario::parse(
        "t",
        "t.scenario",
        &format!("# spec: x\n# capability: t\n{body}"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The outcome of the capability scenario with this body.
fn outcome(body: &str) -> Outcome {
    let spec = spec();
    runner_in(&spec, Path::new(TOOL).join("fixtures")).capability(&scenario(body))
}

/// Whether the checks made before anything runs pass the scenario with
/// this body, in `spec`, for a capability listed `missing` or `works`.
fn checks(spec: &Spec, body: &str, missing: bool) -> bool {
    let (status, issue) = if missing {
        ("missing", "#1")
    } else {
        ("works", "-")
    };
    let runner = runner_in(spec, Path::new(TOOL).join("fixtures"));
    capability::check(&[cap("t", status, issue)], &[scenario(body)], &runner).is_ok()
}

const BOLD: &str = "size 4 1\nsend \"\\e[1mx\"\ncheck\nwant attr 0 0-0 bold\n";

#[test]
fn a_scenario_passes_when_its_assertions_hold_and_fails_when_one_does_not() {
    assert_eq!(outcome(BOLD), Outcome::Pass);
    let Outcome::Fail(why) = outcome(&BOLD.replace("bold\n", "italic\n")) else {
        panic!("a want vtium does not print passed");
    };
    assert!(
        why.starts_with("line 6, block 1: no \"attr 0 0-0 italic\""),
        "{why}"
    );
    let reject = BOLD.replace("want attr", "reject-re attr 0 .*\nwant attr");
    assert!(matches!(outcome(&reject), Outcome::Fail(_)));
}

#[test]
fn replies_and_test_lines_are_judged_in_order() {
    let da1 = "size 4 1\nsend \"\\e[c\"\ncheck\nreply-re \"\\e\\\\[\\\\?.*c\"\n";
    assert_eq!(outcome(da1), Outcome::Pass);
    let none = "size 4 1\nsend \"x\"\ncheck\nreply \"\\e[?1c\"\n";
    assert!(matches!(outcome(none), Outcome::Fail(why) if why.contains("replied \"\"")));
    let tests = "size 4 1\ncmd frame-since;caller=0\ncmd frame-since;caller=0\ncheck\nwant test frame-since 0 changed N\nwant test frame-since 0 changed 0\n";
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
fn a_scenario_is_absent_without_its_feature_and_a_placeholder_fails() {
    // damage-exact, which vtium-replay declines in every build
    let exact = "size 4 1\nneeds damage-exact\nsend \"x\"\ncheck\nwant row 0 |x|\n";
    assert!(matches!(outcome(exact), Outcome::Fail(why) if why.starts_with("absent: ")));
    assert!(matches!(
        outcome("placeholder none yet\n"),
        Outcome::Fail(_)
    ));
}

#[test]
fn the_checks_before_anything_runs_hold_assertions_to_the_grammar() {
    let spec = spec();
    assert!(checks(&spec, BOLD, false));
    // 1.0 reserves overline, so a working capability under 1.0 alone may
    // not want it; vtium's attrs-renditions prints it, so under 1.4 one may
    let spec_10 = Spec::read(&[&read("corpus/INTERFACE.md")]).unwrap_or_else(|e| panic!("{e}"));
    let overlined = BOLD.replace("bold\n", "bold overline\n");
    assert!(!checks(&spec_10, &overlined, false), "a reserved key");
    assert!(checks(&spec_10, &overlined, true));
    assert!(
        checks(&spec, &overlined, false),
        "a key attrs-renditions prints"
    );
    assert!(
        !checks(&spec, &BOLD.replace("bold\n", "overline bold\n"), false),
        "a key out of its place"
    );
    assert!(!checks(
        &spec,
        &BOLD.replace("want attr 0 0-0 bold", "want attr 0 0-0 blod"),
        false
    ));
    let counted = "size 4 1\nview frame\nsend \"x\"\ncheck\nwant changed 4\n";
    assert!(!checks(&spec, counted, false), "an unmasked count");
    assert!(checks(
        &spec,
        &counted.replace("changed 4", "changed N"),
        false
    ));
    let re = "size 4 1\nsend \"x\"\ncheck\nwant-re row 0 \\|y\\|\n";
    assert!(checks(&spec, re, false));
    assert!(
        !checks(&spec, re, true),
        "a missing capability's pattern needs an example"
    );
    assert!(
        !checks(&spec, &format!("{re}example \"row 0 |x|\"\n"), true),
        "an example it misses"
    );
    assert!(checks(&spec, &format!("{re}example \"row 0 |y|\"\n"), true));
    let unknown = "size 4 1\ncmd no-such\ncheck\nwant row 0 ||\n";
    assert!(
        !checks(&spec, unknown, true),
        "a command the interface lacks"
    );
    let needs = "size 4 1\nneeds no-such\nsend \"x\"\ncheck\nwant row 0 |x|\n";
    assert!(!checks(&spec, needs, true), "a feature the interface lacks");
}

#[test]
fn a_scenario_reads_its_fixture_from_the_fixture_directory() {
    let spec = spec();
    let body = "size 8 1\nfixture kmscon.termcap\ncmd termcap;file=kmscon.termcap\nsend \"\\eP+q544e\\e\\\\\"\ncheck\nreply \"\\eP1+r544E=6B6D73636F6E\\e\\\\\"\n";
    let served = runner_in(&spec, Path::new(TOOL).join("fixtures"));
    assert_eq!(served.capability(&scenario(body)), Outcome::Pass);
    let none = runner_in(&spec, Path::new(TOOL).join("no-such-dir"));
    assert!(
        matches!(none.capability(&scenario(body)), Outcome::Unjudged(why) if why.contains("kmscon.termcap")),
        "a fixture the directory lacks"
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
        "a missing capability passes"
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
