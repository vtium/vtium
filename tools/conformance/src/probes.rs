//! The probe: a scenario's input and assertions as the judge reads them,
//! played through vtium-replay's code and judged block by block, and each
//! capability's verdict against `capabilities.list` (ADR-1504 D4).
//!
//! A capability listed `works` must pass and one listed `missing` must
//! fail, so the change that adds a feature also sets its line to `works`.
//! A capability listed with a build feature, `-Dimages`, must pass in a
//! build with it and fail in one without. [`crate::scenario::capability`]
//! applies these rules to the capability scenarios, and the README's "The
//! capability scenarios" gives the checks made before anything runs.

use std::collections::{BTreeMap, BTreeSet};

mod judge;

use regex::Regex;
use regex::bytes::Regex as BytesRegex;

use crate::spec::Spec;

pub(crate) use judge::judge_blocks;
pub use judge::{Outcome, Replay, build_feature, check_list, verdict};

/// The steps that build a probe's input; every other step judges the
/// block of the last `check`.
const INPUT_KEYS: [&str; 3] = ["send", "cmd", "check"];

/// Each view and the feature it needs.
const VIEW_FEATURES: [(&str, Option<&str>); 4] = [
    ("screen", None),
    ("frame", Some("view-frame")),
    ("draw2", Some("view-draw2")),
    ("display", Some("bidi")),
];

/// One step of a probe, with the line it was read from.
#[derive(Clone, Debug)]
pub(crate) enum Step {
    Send(Vec<u8>),
    Cmd(String),
    Check,
    Want(String),
    WantRe(Regex),
    Reject(String),
    RejectRe(Regex),
    Reply(Vec<u8>),
    /// The pattern, and its source as the scenario wrote it.
    ReplyRe(BytesRegex, String),
}

impl Step {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::Send(_) => "send",
            Self::Cmd(_) => "cmd",
            Self::Check => "check",
            Self::Want(_) => "want",
            Self::WantRe(_) => "want-re",
            Self::Reject(_) => "reject",
            Self::RejectRe(_) => "reject-re",
            Self::Reply(_) => "reply",
            Self::ReplyRe(..) => "reply-re",
        }
    }
}

/// A scenario's input steps, checks and assertions, as the judge reads
/// them ([`crate::scenario::Scenario::probe`]).
#[derive(Clone, Debug)]
pub struct Probe {
    pub name: String,
    pub(crate) size: Option<(usize, usize)>,
    pub(crate) view: String,
    pub(crate) placeholder: Option<String>,
    pub(crate) needs: BTreeSet<String>,
    pub fixtures: Vec<String>,
    pub(crate) steps: Vec<(Step, usize)>,
    /// The example after a `want-re` or `reply-re`, by the step's index.
    pub(crate) examples: BTreeMap<usize, (Vec<u8>, usize)>,
}

/// `data` printable: ESC as `\e` and the other controls as `\xNN`.
pub(crate) fn shown(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .chars()
        .map(|c| match c {
            '\x1b' => "\\e".to_owned(),
            c if (c as u32) < 0x20 || c == '\x7f' => format!("\\x{:02x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// A byte pattern, as a scenario writes it after unquoting, matched
/// whole against the replies: any byte for `.`, a newline included, and
/// each byte that is not printable ASCII, or that a backslash escapes,
/// written as `\xHH`.
pub(crate) fn bytes_regex(pattern: &[u8]) -> Result<BytesRegex, String> {
    let mut text = String::new();
    let mut i = 0;
    while let Some(&b) = pattern.get(i) {
        let next = pattern.get(i.saturating_add(1)).copied();
        let printable = |b: u8| (0x20..0x7f).contains(&b);
        match (b, next) {
            (b'\\', Some(n)) if !printable(n) => {
                text.push_str(&format!("\\x{n:02x}"));
                i = i.saturating_add(2);
            }
            (b'\\', Some(n)) => {
                text.push('\\');
                text.push(char::from(n));
                i = i.saturating_add(2);
            }
            (b, _) if printable(b) => {
                text.push(char::from(b));
                i = i.saturating_add(1);
            }
            (b, _) => {
                text.push_str(&format!("\\x{b:02x}"));
                i = i.saturating_add(1);
            }
        }
    }
    BytesRegex::new(&format!(r"(?s-u)\A(?:{text})\z")).map_err(|e| e.to_string())
}

/// A nonzero count, which is masked unless the probe and the
/// implementation are both damage-exact, and the placement's changed flag.
fn damage_count(line: &str) -> Option<&str> {
    let (head, n) = line.rsplit_once(' ')?;
    let is_head = head == "changed"
        || head
            .strip_prefix("test frame-since ")
            .and_then(|r| r.strip_suffix(" changed"))
            .is_some_and(|k| matches!(k.as_bytes(), [b'0'..=b'7']));
    let is_count = n == "N"
        || (n.starts_with(|c: char| ('1'..='9').contains(&c))
            && n.bytes().all(|b| b.is_ascii_digit()));
    (is_head && is_count).then_some(n)
}

fn placement_changed(line: &str) -> bool {
    line.starts_with("placement ") && line.ends_with(" changed")
}

/// Checks that each test command a capability scenario sends is a command
/// of the replay interface, and each feature its `needs` line names a
/// feature of it, so a misspelt name fails the run before anything runs
/// instead of reading as a capability vtium lacks.
pub fn check_names(file: &str, probe: &Probe, spec: &Spec) -> Result<(), String> {
    for (step, n) in &probe.steps {
        if let Step::Cmd(text) = step {
            let name = text.split(';').next().unwrap_or_default();
            if !spec.commands.contains_key(name) {
                return Err(format!(
                    "{file}:{n}: {name} is not a command of the replay interface"
                ));
            }
        }
    }
    match probe.needs.iter().find(|f| !spec.features.contains(*f)) {
        Some(f) => Err(format!(
            "{file}: {f} is not a feature of the replay interface"
        )),
        None => Ok(()),
    }
}

/// Checks a probe's expectations against the spec before anything runs:
/// each `want` and `reject` line, and each `want-re` example, is a line of
/// the probe's view (with the reserved keys allowed in a missing probe
/// only), each `want-re` and `reply-re` matches its example, and a missing
/// probe gives each one an example. A masked count, `N`, stands for a
/// nonzero digit.
pub fn check_expectations(
    file: &str,
    probe: &Probe,
    spec: &Spec,
    missing: bool,
) -> Result<(), String> {
    let exact = probe.needs.contains("damage-exact");
    let parses = |line: &str, n: usize| -> Result<(), String> {
        if let Some(count) = damage_count(line)
            && (count == "N") == exact
        {
            return Err(format!(
                "{file}:{n}: {line:?}: {}",
                if exact {
                    "an expectation that needs damage-exact compares exact counts"
                } else {
                    "a nonzero count is informative unless damage-exact; write N, or add needs damage-exact"
                }
            ));
        }
        if placement_changed(line) && !exact {
            return Err(format!(
                "{file}:{n}: {line:?}: a placement's changed flag is informative unless damage-exact; leave it out, or add needs damage-exact"
            ));
        }
        // a masked count stands for a nonzero one, a frame's own `changed`
        // line included
        let unmasked = match damage_count(line) {
            Some("N") => format!("{} 1", &line[..line.len() - 2]),
            _ => line.to_owned(),
        };
        if !spec.parses(&unmasked, &probe.view, missing) {
            return Err(format!(
                "{file}:{n}: {line:?} is not a {} view line of the spec's grammar{}",
                probe.view,
                if missing {
                    ""
                } else {
                    " (the reserved keys are only for a capability listed missing)"
                }
            ));
        }
        Ok(())
    };
    for (i, (step, n)) in probe.steps.iter().enumerate() {
        match step {
            Step::Want(line) | Step::Reject(line) => parses(line, *n)?,
            Step::WantRe(_) | Step::ReplyRe(..) => {
                let Some((example, m)) = probe.examples.get(&i) else {
                    if missing {
                        return Err(format!(
                            "{file}:{n}: This {} needs an example line after it, because its capability is listed missing: a reply or a line the feature would print.",
                            step.key()
                        ));
                    }
                    continue;
                };
                let matched = match step {
                    Step::WantRe(re) => {
                        let line = String::from_utf8_lossy(example);
                        parses(&line, *m)?;
                        re.is_match(&line)
                    }
                    Step::ReplyRe(re, _) => re.is_match(example),
                    _ => true,
                };
                if !matched {
                    return Err(format!(
                        "{file}:{m}: the example does not match the {} on line {n}",
                        step.key()
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::unquote;

    #[test]
    fn byte_patterns_match_whole_with_escaped_bytes_and_dot_matching_any_byte() {
        let re = |p: &str| bytes_regex(&unquote(p).unwrap_or_default());
        let dcs = re(r#""\eP!\\|.*\e\\\\""#).unwrap_or_else(|e| panic!("{e}"));
        assert!(dcs.is_match(b"\x1bP!|00\xff\n\x1b\\"));
        assert!(!dcs.is_match(b"\x1bP!|00\x1b\\x"), "matched whole");
        let csi = re(r#""\e\\[\\?6[1-5](;.*)?c""#).unwrap_or_else(|e| panic!("{e}"));
        assert!(csi.is_match(b"\x1b[?62;22c"));
        assert!(!csi.is_match(b"\x1b[?60c"));
    }

    #[test]
    fn counts_are_masked_unless_exact() {
        assert_eq!(damage_count("changed 12"), Some("12"));
        assert_eq!(damage_count("test frame-since 3 changed N"), Some("N"));
        assert_eq!(damage_count("changed 0"), None);
        assert_eq!(damage_count("test frame-since 8 changed 1"), None);
        assert!(placement_changed("placement 0 row 0 changed"));
    }
}
