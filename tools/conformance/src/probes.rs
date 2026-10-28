//! The capability probes: each stanza of the probe files replayed through
//! vtium-replay and judged against `capabilities.list`.
//!
//! A probe listed `works` must pass and one listed `missing` must fail, so
//! the change that adds a feature also sets its line to `works`. A probe
//! listed with a build feature, `-Dimages`, must pass in a build with it
//! and fail in one without. A probe the list does not name, or a name in
//! the list no probe defines, fails the run. The README's "The probes"
//! gives the stanza format and the checks made before anything runs.

use std::collections::{BTreeMap, BTreeSet};

mod judge;

use regex::Regex;
use regex::bytes::Regex as BytesRegex;

use crate::spec::{Spec, whole};

pub use judge::{Outcome, Replay, build_feature, check_list, verdict};

/// The keys that build a probe's input; every other key judges the block
/// of the last `check`.
const INPUT_KEYS: [&str; 3] = ["send", "cmd", "check"];

/// Each view and the feature it needs.
const VIEW_FEATURES: [(&str, Option<&str>); 3] = [
    ("screen", None),
    ("frame", Some("view-frame")),
    ("draw2", Some("view-draw2")),
];

/// One step of a probe, with the line it was read from.
#[derive(Clone, Debug)]
enum Step {
    Send(Vec<u8>),
    Cmd(String),
    Check,
    Want(String),
    WantRe(Regex),
    Reject(String),
    RejectRe(Regex),
    Reply(Vec<u8>),
    /// The pattern, and its source as the probe file wrote it.
    ReplyRe(BytesRegex, String),
}

impl Step {
    fn key(&self) -> &'static str {
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

/// A stanza of a probe file.
#[derive(Clone, Debug)]
pub struct Probe {
    pub name: String,
    /// The file and line the stanza opens on.
    pub at: String,
    size: Option<(usize, usize)>,
    view: String,
    placeholder: Option<String>,
    needs: BTreeSet<String>,
    pub fixtures: Vec<String>,
    steps: Vec<(Step, usize)>,
    /// The example after a `want-re` or `reply-re`, by the step's index.
    examples: BTreeMap<usize, (Vec<u8>, usize)>,
}

impl Probe {
    pub fn is_placeholder(&self) -> bool {
        self.placeholder.is_some()
    }
}

/// The bytes of a `"..."` string written with `\e`, `\r`, `\n`, `\t`,
/// `\\`, `\"` and `\xHH`, or None.
pub fn unquote(text: &str) -> Option<Vec<u8>> {
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return None,
            '\\' => {
                let (_, e) = chars.next()?;
                let byte = match e {
                    'e' => 0x1b,
                    'r' => b'\r',
                    'n' => b'\n',
                    't' => b'\t',
                    '\\' => b'\\',
                    '"' => b'"',
                    'x' => {
                        let hex = body.get(i.saturating_add(2)..i.saturating_add(4))?;
                        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                            return None;
                        }
                        chars.next();
                        chars.next();
                        u8::from_str_radix(hex, 16).ok()?
                    }
                    _ => return None,
                };
                out.push(byte);
            }
            c => {
                let mut buf = [0; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    Some(out)
}

/// `data` printable: ESC as `\e` and the other controls as `\xNN`.
fn shown(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .chars()
        .map(|c| match c {
            '\x1b' => "\\e".to_owned(),
            c if (c as u32) < 0x20 || c == '\x7f' => format!("\\x{:02x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// A byte pattern, as a probe file writes it after unquoting, matched
/// whole against the replies: any byte for `.`, a newline included, and
/// each byte that is not printable ASCII, or that a backslash escapes,
/// written as `\xHH`.
fn bytes_regex(pattern: &[u8]) -> Result<BytesRegex, String> {
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

/// Reads the probes of `text`, the file `file`, each checked for form.
pub fn read(file: &str, text: &str, spec: &Spec) -> Result<Vec<Probe>, String> {
    let mut probes: Vec<Probe> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i.saturating_add(1);
        let trimmed = line.trim();
        if trimmed.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let (key, arg) = line.split_once(' ').unwrap_or((line, ""));
        let at = format!("{file}:{n}");
        if key == "probe" {
            if let Some(p) = probes.iter().find(|p| p.name == arg) {
                return Err(format!("{at}: {arg} is already a probe, at {}", p.at));
            }
            probes.push(Probe {
                name: arg.to_owned(),
                at: at.clone(),
                size: None,
                view: "screen".to_owned(),
                placeholder: None,
                needs: BTreeSet::new(),
                fixtures: Vec::new(),
                steps: Vec::new(),
                examples: BTreeMap::new(),
            });
            continue;
        }
        let Some(probe) = probes.last_mut() else {
            return Err(format!("{at}: {key} before the first probe line"));
        };
        let quoted = || {
            unquote(arg).ok_or_else(|| {
                format!(r#"{at}: want {key} "..." with \e, \r, \n, \t, \\, \" or \xHH escapes"#)
            })
        };
        let step = match key {
            "size" => {
                let size = arg.split_once(' ').and_then(|(c, r)| {
                    let n = |s: &str| {
                        (s.starts_with(|c: char| ('1'..='9').contains(&c))
                            && s.bytes().all(|b| b.is_ascii_digit()))
                        .then(|| s.parse().ok())
                        .flatten()
                    };
                    Some((n(c)?, n(r)?))
                });
                probe.size = Some(size.ok_or_else(|| format!("{at}: want size COLS ROWS"))?);
                None
            }
            "view" => {
                if !VIEW_FEATURES.iter().any(|(v, _)| *v == arg) {
                    return Err(format!(
                        "{at}: view {arg:?} is not one of screen, frame, draw2"
                    ));
                }
                probe.view = arg.to_owned();
                None
            }
            "needs" => {
                for feature in arg.split_whitespace() {
                    if !spec.features.contains(feature) {
                        return Err(format!("{at}: {feature} is not a feature of the spec"));
                    }
                    probe.needs.insert(feature.to_owned());
                }
                None
            }
            "fixture" => {
                probe.fixtures.push(arg.to_owned());
                None
            }
            "placeholder" => {
                probe.placeholder = Some(arg.to_owned());
                None
            }
            "cmd" => {
                let name = arg.split(';').next().unwrap_or_default();
                if !spec.commands.contains_key(name) {
                    return Err(format!("{at}: {name} is not a command of the spec"));
                }
                Some(Step::Cmd(arg.to_owned()))
            }
            "send" => Some(Step::Send(quoted()?)),
            "reply" => Some(Step::Reply(quoted()?)),
            "reply-re" => {
                let source = quoted()?;
                let re = bytes_regex(&source).map_err(|e| format!("{at}: {e}"))?;
                Some(Step::ReplyRe(re, shown(&source)))
            }
            "want-re" => Some(Step::WantRe(whole(arg).map_err(|e| format!("{at}: {e}"))?)),
            "reject-re" => Some(Step::RejectRe(
                whole(arg).map_err(|e| format!("{at}: {e}"))?,
            )),
            "check" => Some(Step::Check),
            "want" => Some(Step::Want(arg.to_owned())),
            "reject" => Some(Step::Reject(arg.to_owned())),
            "example" => {
                let last = probe.steps.len().checked_sub(1);
                let after = last.and_then(|k| probe.steps.get(k)).map(|(s, _)| s.key());
                match (unquote(arg), last) {
                    (Some(data), Some(k))
                        if matches!(after, Some("want-re" | "reply-re"))
                            && !probe.examples.contains_key(&k) =>
                    {
                        probe.examples.insert(k, (data, n));
                    }
                    _ => {
                        return Err(format!(
                            r#"{at}: want example "..." once, after a want-re or reply-re"#
                        ));
                    }
                }
                None
            }
            _ => return Err(format!("{at}: {key:?} is not a key of a probe file")),
        };
        if let Some(step) = step {
            probe.steps.push((step, n));
        }
    }
    for probe in &probes {
        let at = format!("{}: {}", probe.at, probe.name);
        if probe.placeholder.is_some() {
            if !probe.steps.is_empty() {
                return Err(format!("{at}: a placeholder has nothing to run"));
            }
            continue;
        }
        if probe.size.is_none() {
            return Err(format!("{at}: no size"));
        }
        if !probe.steps.iter().any(|(s, _)| {
            matches!(
                s,
                Step::Want(_) | Step::WantRe(_) | Step::Reply(_) | Step::ReplyRe(..)
            )
        }) {
            return Err(format!(
                "{at}: no want or reply, so nothing it prints could fail it"
            ));
        }
        let mut checked = false;
        for (step, n) in &probe.steps {
            if INPUT_KEYS.contains(&step.key()) {
                checked = matches!(step, Step::Check);
            } else if !checked {
                return Err(format!(
                    "{file}:{n}: {} after input with no check between",
                    step.key()
                ));
            }
        }
    }
    Ok(probes)
}

/// The probes of the copied file, with vtium's own: a probe of vtium's
/// takes the place of a placeholder of the same name, and of nothing else.
pub fn merge(copied: Vec<Probe>, ours: Vec<Probe>) -> Result<Vec<Probe>, String> {
    let mut out = copied;
    for probe in ours {
        match out.iter_mut().find(|p| p.name == probe.name) {
            Some(p) if p.is_placeholder() => *p = probe,
            Some(p) => {
                return Err(format!(
                    "{}: {} is already a probe at {}, which is not a placeholder",
                    probe.at, probe.name, p.at
                ));
            }
            None => out.push(probe),
        }
    }
    Ok(out)
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
                    "a probe that needs damage-exact compares exact counts"
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
        // line included, which the C reference's runner could not take
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
                    " (reserved keys are for missing probes)"
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
                            "{file}:{n}: a missing probe's {} needs an example line after it, a reply or line the feature would print",
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

    #[test]
    fn strings_unquote_with_their_escapes_and_refuse_the_rest() {
        assert_eq!(
            unquote(r#""a\e[\x41\\\"é""#),
            Some(b"a\x1b[A\\\"\xc3\xa9".to_vec())
        );
        assert_eq!(unquote(r#""a"b""#), None, "a bare quote");
        assert_eq!(unquote(r#""\q""#), None, "an unknown escape");
        assert_eq!(unquote(r#""\x4""#), None, "one hex digit");
        assert_eq!(unquote(r#""\""#), None, "a trailing backslash");
        assert_eq!(unquote("abc"), None, "no quotes");
    }

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
