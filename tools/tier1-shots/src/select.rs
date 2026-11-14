//! What the tool draws: every case of the corpus and of vtium's own cases,
//! with the goldens that judge each, and every screen a behaviour
//! scenario dumps.

use std::path::Path;

use vtium_conformance::runner::{Case, DISPLAY, VIEWS};
use vtium_conformance::scenario::{self, Scenario};
use vtium_conformance::view::View;

/// The sets of replay cases, each a directory of tier 1 laid out as the
/// corpus is: the copied corpus, then vtium's own cases.
pub const SETS: [&str; 2] = ["corpus", "vtium"];

/// The scenarios' directory in tier 1's.
pub const SCENARIOS: &str = "scenarios";

/// The views `case` of the set at `set` is judged in, each with its golden
/// directory: the screen and frame views for every case, and the display
/// view when `golden-display/` holds the case's golden, as `conformance
/// run` judges it.
pub fn views(set: &Path, case: &Case) -> Vec<(View, &'static str, &'static str)> {
    let mut out = VIEWS.to_vec();
    let (_, _, dir) = DISPLAY;
    if set.join(dir).join(format!("{}.txt", case.name)).is_file() {
        out.push(DISPLAY);
    }
    out
}

/// Every scenario under `dir`, read from its `NAME.scenario` file, in the
/// order of their names. `file` names each as `scenarios/NAME.scenario`.
pub fn scenarios(dir: &Path) -> Result<Vec<Scenario>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".scenario").map(str::to_owned))
        .collect();
    names.sort();
    names
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.scenario"));
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            scenario::parse(name, &format!("{SCENARIOS}/{name}.scenario"), &text)
        })
        .collect()
}

/// One screen a scenario dumps: the block of the replay that holds it, the
/// command that asks for it as the transcript echoes it, and that
/// command's line in the scenario's file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dumped {
    pub block: usize,
    pub echo: String,
    pub line: usize,
}

/// The screens `s` dumps, in order. A behaviour scenario has a cut after
/// each command, so the block at a `dump`'s cut is the screen that `dump`
/// prints. A capability scenario has no transcript, and the reader
/// refuses a `dump` in one.
pub fn dumps(s: &Scenario) -> Vec<Dumped> {
    let compiled = s.compile();
    s.commands()
        .zip(&compiled.command_cuts)
        .filter(|(c, _)| !c.dump.is_empty())
        .map(|(c, &block)| Dumped {
            block,
            echo: c.echo.clone(),
            line: c.line,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEHAVIOUR: &str = "# spec: a test of the selection
size 10 2
send \"ab\"
dump screen
key Up
send \"c\"
dump frame
";

    #[test]
    fn each_dump_of_a_behaviour_scenario_is_a_screen_in_order() {
        let s = scenario::parse("key-test", "scenarios/key-test.scenario", BEHAVIOUR).unwrap();
        let dumped = dumps(&s);
        let got: Vec<(usize, &str)> = dumped.iter().map(|d| (d.block, d.echo.as_str())).collect();
        // size, send, dump, key, send, dump: the blocks after each command
        assert_eq!(got, [(2, "dump screen"), (5, "dump frame")]);
        assert_eq!(dumped[0].line, 4);
    }

    #[test]
    fn a_scenario_without_a_dump_draws_nothing() {
        let text = "# spec: no dump\nsize 10 2\nsend \"ab\"\n";
        let s = scenario::parse("key-none", "scenarios/key-none.scenario", text).unwrap();
        assert_eq!(dumps(&s), []);
    }

    #[test]
    fn a_capability_scenario_draws_nothing() {
        let text = r#"# spec: ADR-1000 query.da1
# capability: da1
size 10 2
send "\e[c"
check
reply-re "\e\\[\\?.*c"
"#;
        let s = scenario::parse("da1", "scenarios/da1.scenario", text).unwrap();
        assert_eq!(s.sort, scenario::Sort::Capability("da1".to_owned()));
        assert_eq!(dumps(&s), []);
    }

    // the display view joins a case's views only when its golden exists
    #[test]
    fn a_case_is_judged_in_the_display_view_only_with_a_golden_there() {
        let dir = std::env::temp_dir().join(format!("tier1-shots-views-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("golden-display")).unwrap();
        std::fs::write(dir.join("golden-display/rtl.txt"), "").unwrap();
        let case = |name: &str| Case {
            name: name.to_owned(),
            cols: 4,
            rows: 2,
        };
        let names = |c: &Case| -> Vec<&str> { views(&dir, c).iter().map(|(_, n, _)| *n).collect() };
        assert_eq!(names(&case("rtl")), ["screen", "frame", "display"]);
        assert_eq!(names(&case("ltr")), ["screen", "frame"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
