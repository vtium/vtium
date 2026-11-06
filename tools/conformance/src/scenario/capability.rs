//! The capability list judged through capability scenarios (ADR-1504
//! D4), with the rules the probe runner applied to the probes.
//!
//! A capability listed `works` fails the run when an assertion of its
//! scenario fails, and one listed `missing` fails it when every assertion
//! passes. A capability listed with a build feature passes in a build with
//! it and fails, as absent, in one without. A scenario whose capability
//! the list does not name, a name in the list that no scenario declares,
//! two scenarios that declare one capability, and a scenario whose fixture
//! is missing each fail the run. Each scenario's assertions are checked
//! against the grammar before anything runs.

use super::run::Runner;
use super::{Scenario, Sort};
use crate::coverage::Capability;
use crate::probes::{check_expectations, check_list, verdict};

/// What judging the list printed, a line for each capability and each
/// fault, and how many disagree with the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub failures: usize,
}

/// The capability a scenario declares, or None for a behaviour scenario.
fn declared(s: &Scenario) -> Option<&str> {
    match &s.sort {
        Sort::Capability(name) => Some(name),
        Sort::Behaviour => None,
    }
}

/// Checks each capability scenario before anything runs: the list's own
/// rules, and each assertion against the grammar for the scenario's view,
/// with the reserved keys allowed only for a `missing` capability.
pub fn check(caps: &[Capability], scenarios: &[Scenario], runner: &Runner) -> Result<(), String> {
    check_list(caps)?;
    for s in scenarios {
        let Some(name) = declared(s) else { continue };
        let missing = caps.iter().any(|c| c.name == name && c.status == "missing");
        check_expectations(&s.file, &s.probe(), runner.spec(), missing)?;
    }
    Ok(())
}

/// Judges `caps` through the capability scenarios among `scenarios`, in a
/// build whose features `build` names.
pub fn judge(
    caps: &[Capability],
    scenarios: &[Scenario],
    runner: &Runner,
    build: &[(&str, bool)],
) -> Result<Report, String> {
    check(caps, scenarios, runner)?;
    let mut report = Report::default();
    let fail = |report: &mut Report, line: String| {
        report.lines.push(line);
        report.failures = report.failures.saturating_add(1);
    };
    let declaring: Vec<(&str, &Scenario)> = scenarios
        .iter()
        .filter_map(|s| declared(s).map(|n| (n, s)))
        .collect();
    for (k, (name, s)) in declaring.iter().enumerate() {
        if !caps.iter().any(|c| c.name == *name) {
            fail(
                &mut report,
                format!(
                    "FAIL {name}: The scenario {} declares a capability that capabilities.list does not name. Add a line for it to the list.",
                    s.file
                ),
            );
        }
        if let Some((_, other)) = declaring.iter().take(k).find(|(n, _)| n == name) {
            fail(
                &mut report,
                format!(
                    "FAIL {name}: Two scenarios declare this capability, {} and {}. Each capability has one scenario.",
                    other.file, s.file
                ),
            );
        }
    }
    for cap in caps {
        let Some((_, s)) = declaring.iter().find(|(n, _)| *n == cap.name) else {
            fail(
                &mut report,
                format!(
                    "FAIL {}: capabilities.list names a capability that no scenario declares. Add a scenario with `# capability: {}` to its header.",
                    cap.name, cap.name
                ),
            );
            continue;
        };
        let (line, agrees) = verdict(cap, &runner.capability(s), build);
        if agrees {
            report.lines.push(line);
        } else {
            fail(&mut report, line);
        }
    }
    Ok(report)
}
