//! The readers of one line or one value: a line's content without its
//! comment, a key and its value, a name, a rule, a policy line, and the
//! value each key of the key table takes.

use crate::bounds::{NAME_MAX, VT_MAX};
use crate::conf::comment;
use crate::config::WhenDisplayLost;
use crate::error::{PolicyFault, Problem, Quoted, Rejected, Renamed, Syntax};
use crate::identity::Kind;
use crate::keys::{self, Key, TEXT_MAX};
use crate::matching::Match;
use crate::outcome::PolicyLine;
use crate::split::POLICY_KEY;

use super::{Parser, Section};

/// `s` without its outer blanks.
pub(super) fn trim(s: &str) -> &str {
    s.trim_matches(|c| c == ' ' || c == '\t')
}

/// The byte range of a line's content, as [`content`] reads it: from its
/// first byte that is not a blank to the end of its last before its
/// comment, with each `\#` still written as the file writes it.
pub(crate) fn content_range(line: &str) -> std::ops::Range<usize> {
    let (end, _) = comment(line.as_bytes());
    let body = line.get(..end).unwrap_or_default();
    let start = body
        .len()
        .saturating_sub(body.trim_start_matches([' ', '\t']).len());
    let stop = body.trim_end_matches([' ', '\t']).len();
    start..stop.max(start)
}

/// A line with its comment and its outer blanks dropped, by vtium.conf's
/// one comment rule, [`comment`]: each `\#` that rule reads as a literal
/// `#` is one.
pub(crate) fn content(line: &str) -> String {
    let (end, escapes) = comment(line.as_bytes());
    let body = line.get(..end).unwrap_or_default();
    let mut out = String::with_capacity(body.len());
    for (at, c) in body.char_indices() {
        if !escapes.contains(&at) {
            out.push(c);
        }
    }
    trim(&out).to_owned()
}

/// The words of a section heading's text between its brackets, as the
/// reader takes them: `[ rules ]` is `["rules"]`.
pub(super) fn section_words(inner: &str) -> Vec<&str> {
    blanks(inner).collect()
}

/// Whether a line's content is the heading of the `[rules]` section, read
/// as the reader reads a heading, blanks inside the brackets and all.
pub(crate) fn rules_heading(content: &str) -> bool {
    content
        .strip_prefix('[')
        .and_then(|c| c.strip_suffix(']'))
        .is_some_and(|inner| section_words(inner) == ["rules"])
}

pub(super) fn blanks(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t']).filter(|t| !t.is_empty())
}

/// `key = value`, each trimmed, where the line has an `=`.
pub(crate) fn key_value(line: &str) -> Option<(&str, &str)> {
    line.split_once('=').map(|(k, v)| (trim(k), trim(v)))
}

/// A label as a `[labels]` line writes it: each `#` that follows a blank,
/// or that starts the label, written `\#`, so the comment rule reads it as
/// the label's own `#`.
pub fn escape_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut after_blank = true;
    for c in label.chars() {
        if c == '#' && after_blank {
            out.push('\\');
        }
        out.push(c);
        after_blank = c == ' ' || c == '\t';
    }
    out
}

/// Whether `name` keeps the name rule: 1 to [`NAME_MAX`] of
/// `A-Z a-z 0-9 . _ + -`.
pub(crate) fn good_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= NAME_MAX
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// The words `policy` takes.
pub(crate) const POLICY: [&str; 2] = ["open", "closed"];

/// The words `isolated` and `switch-on-enter` take, yes then no.
pub(crate) const YES_NO: [&str; 2] = ["yes", "no"];

/// The forms `when-display-lost` takes (D8).
pub(crate) const WHEN_DISPLAY_LOST: [&str; 3] = ["keep", "hangup", "hangup DURATION"];

/// The keys a head takes in vtium.conf beside the key table's head keys
/// (ADR-1403 D5). Each is the head's own, even where the key table has a
/// top-level key of the same name: the top level's `vt` is the VT vtium
/// opens, and a head's `vt` the VTs the head runs on (ADR-1403 D4).
pub(crate) const CONF_HEAD_KEYS: [&str; 6] = [
    "look",
    "consoles",
    "when-display-lost",
    "isolated",
    "switch-on-enter",
    "vt",
];

/// The key a look names its vtlook with (ADR-1403 D1, ADR-1304 D4).
pub(crate) const VTLOOK: &str = "vtlook";

/// The reserved names, a head's name no head may take: the targets.
/// The first cut's targets, `focus` and `dark`, stay reserved too.
pub(crate) const RESERVED: [&str; 3] = ["roaming", "ignore", "off"];

/// One rule's line read alone, as `[rules]` reads it: its kind and match,
/// or the errors it holds. Its target is checked once a whole file is
/// read, since a rule may name a head declared after it.
pub(crate) fn rule_line(line: &str) -> Result<(Kind, Match), Rejected> {
    let mut p = Parser {
        section: Some(Section::Rules),
        ..Parser::default()
    };
    p.rule(1, &content(line));
    if !p.errors.is_empty() {
        return Err(p.errors);
    }
    match p.rules.pop() {
        Some(read) => Ok((read.kind, read.matcher)),
        None => Err(Rejected::one(Some(1), Problem::Syntax(Syntax::RuleKind))),
    }
}

/// What a key of the head file takes, as its parser reads it, which an
/// error naming a value the key cannot take lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Takes {
    /// One of these words or forms.
    Words(&'static [&'static str]),
    /// A whole number from `min`, at most `max` where there is one.
    Number { min: u32, max: Option<u32> },
    /// A duration of at most `max` seconds.
    Seconds { max: u32 },
    /// Any text but none.
    Text,
    /// A name, by the name rule.
    Name,
}

/// What `key` takes, by the parser's tables: the values a bad value is
/// measured against.
pub(crate) fn takes(key: &str) -> Takes {
    match key {
        "policy" => Takes::Words(&POLICY),
        "isolated" | "switch-on-enter" => Takes::Words(&YES_NO),
        "when-display-lost" => Takes::Words(&WHEN_DISPLAY_LOST),
        "consoles" => Takes::Number { min: 1, max: None },
        "look" | VTLOOK => Takes::Name,
        // a key of the key table a section sets takes the table's values
        _ => match keys::find(key).map(|k| k.kind) {
            Some(keys::Kind::Choice(words)) => Takes::Words(words),
            // every word a name key lists is itself a name by the rule, so
            // the rule alone says what the key takes
            Some(keys::Kind::Name(_)) => Takes::Name,
            Some(keys::Kind::Number { min, max, .. }) => Takes::Number {
                min: u32::try_from(min).unwrap_or(u32::MAX),
                max: Some(u32::try_from(max).unwrap_or(u32::MAX)),
            },
            _ => Takes::Text,
        },
    }
}

/// Checks the value a section gives a key of the key table against the
/// key's values and bounds, as the top level reads the key, but strictly:
/// a value out of bounds is an error in the section, never clamped
/// (ADR-1403 D5). Each section key is of a kind this reads, which
/// `every_section_key_is_of_a_kind_the_section_reader_checks` holds.
pub(super) fn table_value(key: &'static Key, value: &str) -> Result<(), Problem> {
    let bad = || Problem::BadValue {
        key: key.name,
        value: Quoted::new(value),
    };
    if value.is_empty() {
        return Err(bad());
    }
    match key.kind {
        keys::Kind::Text | keys::Kind::Command | keys::Kind::Path | keys::Kind::Paths
            if value.len() > TEXT_MAX =>
        {
            Err(Problem::TooLong(key.name))
        }
        keys::Kind::Path if !keys::absolute(value) => Err(bad()),
        keys::Kind::Paths if !value.split(':').all(keys::absolute) => Err(bad()),
        keys::Kind::Choice(words) if !words.contains(&value) => Err(bad()),
        keys::Kind::Name(words) if !keys::name(words, value) => Err(bad()),
        keys::Kind::Cell if keys::parse_cell(value).is_none() => Err(bad()),
        keys::Kind::Number { word, .. } if word == Some(value) => Ok(()),
        keys::Kind::Number { min, max, unit, .. } => {
            // a byte count may end in K, M or G, powers of 1024
            let suffixed = |s: [char; 2], scale: u64| {
                value
                    .strip_suffix(s)
                    .filter(|_| unit == "bytes")
                    .map(|d| (d, scale))
            };
            let (digits, scale) = suffixed(['K', 'k'], 1 << 10)
                .or_else(|| suffixed(['M', 'm'], 1 << 20))
                .or_else(|| suffixed(['G', 'g'], 1 << 30))
                .unwrap_or((value, 1));
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            let n = digits
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(scale));
            match n {
                Some(n) if (min..=max).contains(&n) => Ok(()),
                _ => Err(Problem::OutOfRange {
                    key: key.name,
                    value: Quoted::new(value),
                }),
            }
        }
        _ => Ok(()),
    }
}

/// The range `key` takes, where a value of the right shape lies outside
/// it.
pub(crate) fn range(key: &str) -> Takes {
    match key {
        "when-display-lost" => Takes::Seconds { max: u32::MAX },
        _ => takes(key),
    }
}

/// A decimal number of digits alone; `None` for anything else, and
/// `Some(None)` for digits past `u32`.
pub(super) fn decimal(value: &str) -> Option<Option<u32>> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.parse().ok())
}

/// A head's `vt` (ADR-1403 D4): one VT number from 1 to [`VT_MAX`], or
/// several separated by blanks, each named once.
pub(super) fn vts(value: &str) -> Result<Vec<u16>, Problem> {
    let mut out: Vec<u16> = Vec::new();
    for word in blanks(value) {
        let vt = decimal(word)
            .flatten()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (1..=VT_MAX).contains(n))
            .ok_or_else(|| Problem::BadVt(Quoted::new(word)))?;
        if out.contains(&vt) {
            return Err(Problem::RepeatedVt(vt));
        }
        out.push(vt);
    }
    if out.is_empty() {
        return Err(Problem::BadVt(Quoted::new("")));
    }
    Ok(out)
}

/// `when-display-lost`'s value (D8): `keep`, `hangup`, or
/// `hangup DURATION`, the duration in seconds or with an `s`, `m` or `h`
/// suffix.
pub(super) fn when_display_lost(value: &str) -> Result<WhenDisplayLost, Problem> {
    const KEY: &str = "when-display-lost";
    let bad = || Problem::BadValue {
        key: KEY,
        value: Quoted::new(value),
    };
    let [keep, hangup, _] = WHEN_DISPLAY_LOST;
    let mut words = blanks(value);
    let (first, second, rest) = (words.next(), words.next(), words.next());
    match (first, second, rest) {
        (Some(w), None, _) if w == keep => Ok(WhenDisplayLost::Keep),
        (Some(w), None, _) if w == hangup => Ok(WhenDisplayLost::Hangup),
        (Some("nohup"), None, _) => Err(Problem::Renamed(Renamed::Nohup)),
        (Some(w), Some(duration), None) if w == hangup => {
            let (digits, unit) = match duration.strip_suffix(['s', 'm', 'h']) {
                Some(digits) => (digits, duration.get(digits.len()..).unwrap_or_default()),
                None => (duration, "s"),
            };
            let scale = match unit {
                "m" => 60,
                "h" => 3600,
                _ => 1,
            };
            let number = decimal(digits).ok_or_else(bad)?;
            number
                .and_then(|n| n.checked_mul(scale))
                .map(WhenDisplayLost::HangupAfter)
                .ok_or(Problem::OutOfRange {
                    key: KEY,
                    value: Quoted::new(value),
                })
        }
        _ => Err(bad()),
    }
}

/// A policy line's text as the model reads it: `policy = open` or
/// `policy = closed`, else why vtium cannot read it.
pub(super) fn policy_line(line: &str) -> Result<PolicyLine, PolicyFault> {
    let [open, closed] = POLICY;
    match key_value(line) {
        None => Err(PolicyFault::NotKeyValue),
        Some((key, _)) if key != POLICY_KEY => Err(PolicyFault::Key(Quoted::new(key))),
        Some((_, value)) if value == open => Ok(PolicyLine::Open),
        Some((_, value)) if value == closed => Ok(PolicyLine::Closed),
        Some((_, value)) => Err(PolicyFault::Value(Quoted::new(value))),
    }
}
