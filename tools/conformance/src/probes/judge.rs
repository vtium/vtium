//! Running a probe through vtium-replay, judging what it prints, and
//! each capability's verdict against the list.

use std::collections::BTreeSet;
use std::path::PathBuf;

use regex::Regex;

use super::{INPUT_KEYS, Probe, Step, VIEW_FEATURES, shown};
use crate::coverage::Capability;
use crate::replay::Input;
use crate::runner::mask_line;
use crate::spec::Spec;
use crate::view::View;

/// The implementation the probes run through: vtium-replay, judged by
/// the spec, its `--describe` read for what it offers.
pub struct Replay<'a> {
    spec: &'a Spec,
    views: BTreeSet<String>,
    features: BTreeSet<String>,
    commands: BTreeSet<String>,
    /// Where a probe's fixtures are, when the runner serves them.
    fixtures: Option<PathBuf>,
}

/// A probe's result: passed, failed with why, or not judged, with why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    Unjudged(String),
}

impl<'a> Replay<'a> {
    /// Reads `describe`, refusing an implementation of another major
    /// version.
    pub fn new(spec: &'a Spec, describe: &str) -> Result<Self, String> {
        let version = describe
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("interface "))
            .and_then(|v| v.split_once('.'))
            .map(|(major, _)| major);
        if version != Some(spec.major.as_str()) {
            return Err(format!(
                "vtium-replay --describe starts {:?}; want interface {}.N",
                describe.lines().next().unwrap_or_default(),
                spec.major
            ));
        }
        let words = |prefix: &str| {
            describe
                .lines()
                .filter_map(|l| l.strip_prefix(prefix))
                .filter_map(|r| r.split_whitespace().next())
                .map(str::to_owned)
                .collect()
        };
        Ok(Self {
            spec,
            views: words("view "),
            features: words("feature "),
            commands: words("command "),
            fixtures: None,
        })
    }

    /// Serves each probe the fixtures it names from `dir`.
    #[must_use]
    pub fn with_fixtures(mut self, dir: PathBuf) -> Self {
        self.fixtures = Some(dir);
        self
    }

    /// What vtium-replay lacks for `probe`, or None.
    fn absent(&self, probe: &Probe) -> Option<String> {
        if !self.views.contains(&probe.view) {
            return Some(format!("the implementation has no {} view", probe.view));
        }
        let mut needs = probe.needs.clone();
        if let Some((_, Some(f))) = VIEW_FEATURES.iter().find(|(v, _)| *v == probe.view) {
            needs.insert((*f).to_owned());
        }
        for (step, _) in &probe.steps {
            if let Step::Cmd(text) = step {
                let name = text.split(';').next().unwrap_or_default();
                if !self.commands.contains(name) {
                    return Some(format!("the implementation has no {name} command"));
                }
                if let Some(Some(f)) = self.spec.commands.get(name) {
                    needs.insert(f.clone());
                }
            }
        }
        let lacking: Vec<&str> = needs
            .iter()
            .filter(|f| !self.features.contains(*f))
            .map(String::as_str)
            .collect();
        (!lacking.is_empty())
            .then(|| format!("the implementation does not declare {}", lacking.join(", ")))
    }

    /// Replays `probe` and judges what it prints.
    pub fn run(&self, probe: &Probe) -> Outcome {
        if let Some(why) = &probe.placeholder {
            return Outcome::Fail(format!("placeholder: {why}"));
        }
        if let Some(why) = self.absent(probe) {
            return Outcome::Fail(format!("absent: {why}"));
        }
        let fixtures = self.fixtures.clone().unwrap_or_default();
        if let Some(name) = probe.fixtures.iter().find(|f| !fixtures.join(f).is_file()) {
            return Outcome::Unjudged(format!("no fixture {name} in {}", fixtures.display()));
        }
        let mut data = Vec::new();
        let mut cuts = Vec::new();
        for (step, _) in &probe.steps {
            match step {
                Step::Send(bytes) => data.extend_from_slice(bytes),
                Step::Cmd(text) => {
                    data.extend_from_slice(b"\x1b_tsm-test;");
                    data.extend_from_slice(text.as_bytes());
                    data.extend_from_slice(b"\x1b\\");
                }
                Step::Check => cuts.push(data.len()),
                _ => {}
            }
        }
        let (cols, rows) = probe.size.unwrap_or((1, 1));
        let view = if probe.view == "frame" {
            View::Frame
        } else {
            View::Screen
        };
        let input = Input {
            view,
            cols,
            rows,
            data,
            cuts: cuts.clone(),
            fixtures,
        };
        let out = match crate::replay::run(&input) {
            Ok(out) => out,
            Err(f) => {
                return Outcome::Unjudged(format!("the replay exited {}: {}", f.code, f.message));
            }
        };
        let exact = probe.needs.contains("damage-exact") && self.features.contains("damage-exact");
        let mut lines = Vec::new();
        for line in out.split_terminator('\n') {
            if !self.spec.parses(line, &probe.view, false) {
                return Outcome::Unjudged(format!(
                    "{line:?} is not a {} line of the grammar",
                    probe.view
                ));
            }
            lines.push(if exact {
                line.to_owned()
            } else {
                mask_line(line)
            });
        }
        judge(probe, &lines, &cuts, exact)
    }
}

/// Judges the lines a probe's replay printed.
fn judge(probe: &Probe, lines: &[String], cuts: &[usize], exact: bool) -> Outcome {
    // a test line belongs to the block whose checkpoint follows it
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    for line in lines {
        if line.starts_with("test ") {
            pending.push(line);
        } else if line.starts_with("checkpoint ") {
            let mut block = vec![line.as_str()];
            block.append(&mut pending);
            blocks.push(block);
        } else if let Some(block) = blocks.last_mut() {
            block.push(line);
        }
    }
    if blocks.len() != cuts.len().saturating_add(1) {
        return Outcome::Fail(format!(
            "the replay printed {} blocks, want {}",
            blocks.len(),
            cuts.len().saturating_add(1)
        ));
    }
    for (k, cut) in cuts.iter().enumerate() {
        let want = format!("checkpoint {cut}");
        if blocks[k][0] != want {
            return Outcome::Fail(format!(
                "block {} is {:?}, want {want:?}",
                k.saturating_add(1),
                blocks[k][0]
            ));
        }
    }
    // assertions on test lines take them in order: the next one each
    // matches is after the last one matched in its block
    let mut k: Option<usize> = None;
    let mut tests: Vec<&str> = Vec::new();
    for (step, n) in &probe.steps {
        if matches!(step, Step::Check) {
            let next = k.map_or(0, |k| k.saturating_add(1));
            k = Some(next);
            tests = blocks[next]
                .iter()
                .copied()
                .filter(|l| l.starts_with("test "))
                .collect();
            continue;
        }
        let Some(k) = k else { continue };
        if INPUT_KEYS.contains(&step.key()) {
            continue;
        }
        let block = &blocks[k];
        let what = format!("line {n}, block {}", k.saturating_add(1));
        let masked = |line: &str| {
            if exact {
                line.to_owned()
            } else {
                mask_line(line)
            }
        };
        let in_tests = match step {
            Step::Want(arg) if arg.starts_with("test ") => {
                let arg = masked(arg);
                Some((tests.iter().position(|l| *l == arg), arg.clone(), ""))
            }
            Step::WantRe(re) if re.as_str().starts_with(r"\A(?:test ") => Some((
                tests.iter().position(|l| re.is_match(l)),
                pattern(re).to_owned(),
                "matching ",
            )),
            _ => None,
        };
        if let Some((hit, pattern, how)) = in_tests {
            match hit {
                Some(hit) => tests.drain(..=hit),
                None => {
                    let all = block.iter().filter(|l| l.starts_with("test ")).count();
                    let taken = all.saturating_sub(tests.len());
                    let after = if taken > 0 {
                        format!(" after test line {taken}")
                    } else {
                        String::new()
                    };
                    return Outcome::Fail(format!("{what}: no test line {how}{pattern:?}{after}"));
                }
            };
            continue;
        }
        let reply: Vec<u8> = block
            .iter()
            .filter_map(|l| l.strip_prefix("reply "))
            .flat_map(|hex| {
                hex.as_bytes()
                    .chunks(2)
                    .filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                    .collect::<Vec<_>>()
            })
            .collect();
        let fail = match step {
            Step::Want(arg) => {
                let arg = masked(arg);
                (!block.contains(&arg.as_str())).then(|| {
                    let kind: Vec<&str> = arg.split(' ').take(2).collect();
                    let kind = kind.join(" ");
                    let near: Vec<String> = block
                        .iter()
                        .filter(|l| l.starts_with(&format!("{kind} ")))
                        .map(|l| format!("{l:?}"))
                        .collect();
                    if near.is_empty() {
                        format!("{what}: no {arg:?}; no {kind:?} line")
                    } else {
                        format!("{what}: no {arg:?}; it has {}", near.join(", "))
                    }
                })
            }
            Step::WantRe(re) => (!block.iter().any(|l| re.is_match(l)))
                .then(|| format!("{what}: no line matches {:?}", pattern(re))),
            Step::Reject(arg) => {
                let arg = masked(arg);
                block
                    .contains(&arg.as_str())
                    .then(|| format!("{what}: {arg:?}"))
            }
            Step::RejectRe(re) => block
                .iter()
                .find(|l| re.is_match(l))
                .map(|l| format!("{what}: {l:?} matches {:?}", pattern(re))),
            Step::Reply(want) => (reply != *want).then(|| {
                format!(
                    "{what}: replied \"{}\", want \"{}\"",
                    shown(&reply),
                    shown(want)
                )
            }),
            Step::ReplyRe(re, source) => (!re.is_match(&reply)).then(|| {
                format!(
                    "{what}: replied \"{}\", want a match of \"{source}\"",
                    shown(&reply)
                )
            }),
            _ => None,
        };
        if let Some(why) = fail {
            return Outcome::Fail(why);
        }
    }
    Outcome::Pass
}

/// A pattern as its probe file wrote it, without the anchors `whole` adds.
fn pattern(re: &Regex) -> &str {
    re.as_str()
        .strip_prefix(r"\A(?:")
        .and_then(|p| p.strip_suffix(r")\z"))
        .unwrap_or(re.as_str())
}

/// The list's checks of its own: a missing probe names the vtium issue
/// that owes it, and a working one names none, or the build feature it
/// needs.
pub fn check_list(caps: &[Capability]) -> Result<(), String> {
    let issue = |s: &str| {
        s.strip_prefix('#')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    for c in caps {
        match c.status.as_str() {
            "missing" if !issue(&c.issue) => {
                return Err(format!(
                    "capabilities.list: {}: a missing probe names its issue (#N)",
                    c.name
                ));
            }
            "works" if c.issue != "-" && build_feature(&c.issue).is_none() => {
                return Err(format!(
                    "capabilities.list: {}: a working probe has no issue (-), or names the build feature it needs (-Dname)",
                    c.name
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The build feature `-Dname` names.
pub fn build_feature(issue: &str) -> Option<&str> {
    issue
        .strip_prefix("-D")
        .filter(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
}

/// One capability's verdict line, and whether it agrees with the list.
pub fn verdict(cap: &Capability, outcome: &Outcome, build: &[(&str, bool)]) -> (String, bool) {
    let name = &cap.name;
    let feature = build_feature(&cap.issue);
    let built = feature.map(|f| build.iter().find(|(b, _)| *b == f).map(|(_, on)| *on));
    match (outcome, feature, built) {
        (_, Some(_), Some(None)) => (
            format!(
                "FAIL {name}: needs {}, which the build does not name",
                cap.issue
            ),
            false,
        ),
        (Outcome::Unjudged(why), _, _) => (format!("FAIL {name}: {why}"), false),
        (Outcome::Pass, Some(_), Some(Some(false))) => (
            format!("FAIL {name}: passes in a build without {}", cap.issue),
            false,
        ),
        (Outcome::Fail(why), Some(_), Some(Some(false))) => (
            format!("ok   {name}: absent, a build without {}: {why}", cap.issue),
            true,
        ),
        (Outcome::Fail(why), _, _) if cap.status == "works" => {
            (format!("FAIL {name}: listed as working, but {why}"), false)
        }
        (Outcome::Pass, _, _) if cap.status == "missing" => (
            format!(
                "FAIL {name}: listed as missing ({}) but passes; set it to works in capabilities.list and close {}",
                cap.issue, cap.issue
            ),
            false,
        ),
        (Outcome::Pass, _, _) => (format!("ok   {name}: works"), true),
        (Outcome::Fail(why), _, _) => (format!("ok   {name}: missing, {}: {why}", cap.issue), true),
    }
}
