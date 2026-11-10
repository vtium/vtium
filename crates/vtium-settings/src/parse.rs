//! The head file's bytes, and the sections of vtium.conf, to what survives
//! of them (ADR-1401 D3, D7; ADR-1403 D3, D4, D5).
//!
//! Above the first section come the top-level keys, `version = 1` and
//! `policy`, in either order. Then come sections: `[head NAME]` with a
//! head's keys, and `[rules]` with one rule a line; vtium.conf adds
//! `[look NAME]`, with a look's keys. A CRLF line ending is a newline. `#`
//! starts a comment at the start of a line or after a blank (a space or a
//! tab), a `#` inside a word is literal, and `\#` after a blank is a
//! literal `#`; blanks around `=` and between tokens are dropped.
//!
//! [`sections`] reads vtium.conf. Its top level, every line above the
//! first section heading as [`split`] finds it, holds vtium's own keys,
//! which the console reads the forgiving way, and the flow's `version` and
//! `policy`, which this reader takes; it skips every other top-level line,
//! so a top-level mistake never drops a section and an error in a section
//! never drops a top-level key. A head takes `look`, `consoles`,
//! `when-display-lost`, `isolated` and the key table's head keys, and a
//! look takes `vtlook` and the key table's look keys, each value checked
//! against the key's values and bounds (ADR-1403 D5).
//!
//! [`reading`] reads every line however many errors come before it, so a
//! check names every error the file holds, each with its line. Each error
//! drops what [`scope`] says it takes: a bad rule drops that rule, an
//! error inside a head's section drops the head with every rule naming
//! it, and an error that leaves the file's structure untrustworthy drops
//! everything but the `policy` lines. A policy line, a line above the
//! first section whose bytes begin with `policy` in any case, is judged
//! before its bytes are decoded: one vtium cannot read makes the file
//! closed and broken (ADR-1401 D7). What survives is a [`Config`], and
//! the `policy` lines, the scopes and what survives classify the file for
//! [`Trusted::of`]. [`parse`] is the strict reading: a [`Config`] only for
//! a file with no error.

use crate::bounds::{
    CONSOLES_MAX, ERRORS_MAX, FILE_MAX, HEADS_MAX, LINE_MAX, LOOKS_MAX, NAME_MAX, RULES_MAX,
    TOKENS_MAX, VT_MAX,
};
use crate::conf::comment;
use crate::config::{Config, Head, LABEL_MAX, Label, Look, Rule, Setting, Target, WhenDisplayLost};
use crate::error::{
    Drops, Error, Note, PolicyFault, Problem, Quoted, Rejected, Renamed, Syntax, Warning,
};
use crate::identity::Kind;
use crate::keys::{self, Key, Level, TEXT_MAX};
use crate::matching::{Match, Token, TokenError};
use crate::outcome::{Kept, Policy, PolicyLine, Scope, Trusted, Within, scope};
use crate::split::{
    BOM, POLICY_KEY, VERSION_KEY, begins_policy, heading, numbered, split, split_after,
};

/// A valid head file, or what survives of one, and what its check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub config: Config,
    /// Findings `vtium check-config` counts against the file.
    pub warnings: Vec<Warning>,
    /// Findings it reports without counting them.
    pub notes: Vec<Note>,
    /// Warnings past [`ERRORS_MAX`](crate::ERRORS_MAX), counted, not kept.
    pub more_warnings: usize,
    /// Notes past [`ERRORS_MAX`](crate::ERRORS_MAX), counted, not kept.
    pub more_notes: usize,
}

/// A head file read line by line: what survives its errors, the errors,
/// and what [`Trusted::of`] classifies it by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// What survives, with the warnings and notes of what survives.
    pub parsed: Parsed,
    /// Every error in the file's order, at most
    /// [`ERRORS_MAX`](crate::ERRORS_MAX) of them kept.
    pub errors: Rejected,
    /// Every `policy` line, wherever it sits, in the file's order.
    pub policies: Vec<PolicyLine>,
    /// What the errors take down, each scope once.
    pub scopes: Vec<Scope>,
}

impl Reading {
    /// What survives the errors.
    pub fn kept(&self) -> Kept {
        let config = &self.parsed.config;
        if !config.heads.is_empty() {
            Kept::Heads
        } else if !config.rules.is_empty() {
            Kept::Headless
        } else {
            Kept::Nothing
        }
    }

    /// The file's second and third levels (ADR-1401 D7).
    pub fn trusted(&self) -> Trusted {
        Trusted::of(&self.policies, &self.scopes, self.kept())
    }
}

/// Reads the sections of vtium.conf's bytes, and the `version` and
/// `policy` lines of its top level, with the whole file's line numbers
/// (ADR-1403 D3): at most [`FILE_MAX`] bytes, each line checked on its own
/// for UTF-8, control characters and its length, so a bad line drops what
/// its scope takes and no more. Every other top-level line is vtium's own
/// key, which this reader skips. Past [`FILE_MAX`] the sections are
/// dropped whole, and the `policy` lines are read from the lines within
/// the bound; a policy line the bound cuts cannot be read. A file with no
/// flow reads as one that declares nothing; [`sections`] tells it apart.
pub fn reading(bytes: &[u8]) -> Reading {
    read(bytes)
}

/// Reads the sections of vtium.conf's bytes, and the `version` and
/// `policy` lines of its top level, as [`reading`] reads a head file, with
/// the whole file's line numbers (ADR-1403 D3). Every other top-level line
/// is vtium's own key, which this reader skips. `None` for a file with no
/// flow, one with no section heading and no policy line, as a copied
/// kmscon.conf is: vtium runs its compiled-in flow for it, and the top
/// level still applies.
pub fn sections(bytes: &[u8]) -> Option<Reading> {
    split(bytes).has_flow().then(|| read(bytes))
}

/// Parses the sections of vtium.conf's text, strictly: `Ok(None)` for a
/// file with no flow, its [`Parsed`] when the sections and the policy hold
/// no error, else every error.
pub fn parse_sections(text: &str) -> Result<Option<Parsed>, Rejected> {
    match sections(text.as_bytes()) {
        None => Ok(None),
        Some(r) if r.errors.is_empty() => Ok(Some(r.parsed)),
        Some(r) => Err(r.errors),
    }
}

/// Reads each line of `bytes` into `p` with `line`, numbering them on from
/// `n`, a carriage return ending a line read as part of its newline;
/// returns the last number given, which the empty piece after a last
/// newline takes.
fn each_line(
    p: &mut Parser,
    bytes: &[u8],
    mut n: usize,
    line: fn(&mut Parser, usize, &[u8]),
) -> usize {
    let mut lines = bytes.split(|&b| b == b'\n').peekable();
    while let Some(raw) = lines.next() {
        n = n.saturating_add(1);
        let raw = match (lines.peek(), raw.split_last()) {
            (Some(_), Some((b'\r', rest))) => rest,
            _ => raw,
        };
        line(p, n, raw);
    }
    n
}

fn read(bytes: &[u8]) -> Reading {
    let mut p = Parser::default();
    let (within, cut) = if bytes.len() > FILE_MAX {
        p.error(None, Problem::TooLarge, Within::Top);
        let bounded = bytes.get(..FILE_MAX).unwrap_or_default();
        // the line the bound cuts is not read; the last line it keeps
        // keeps its newline, so a CRLF ending stays a newline
        let end = bounded
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |at| at.saturating_add(1));
        bounded.split_at_checked(end).unwrap_or((bounded, &[]))
    } else {
        (bytes, &[][..])
    };
    // a byte-order mark is no part of the first line, and counts toward
    // the bound
    let (within, cut) = match within.strip_prefix(BOM) {
        Some(rest) => (rest, cut),
        None if within.is_empty() => (within, cut.strip_prefix(BOM).unwrap_or(cut)),
        None => (within, cut),
    };
    let parts = split_after(within, false);
    p.headed = parts.first_section.is_some();
    let n = match parts.first_section {
        // the top level's lines, then the sections' from the heading
        Some(first) => {
            for (n, raw) in numbered(parts.top, 1) {
                p.top_line(n, raw.strip_suffix(b"\r").unwrap_or(raw));
            }
            each_line(
                &mut p,
                parts.sections,
                first.saturating_sub(1),
                Parser::raw_line,
            )
        }
        None => each_line(&mut p, parts.top, 0, Parser::top_line),
    };
    // the lines end in an empty one, numbered as the line the bound cuts
    if p.top() && begins_policy(cut) {
        p.unreadable_policy(n, PolicyFault::Cut);
    }
    p.finish()
}

/// Parses a head file's text, strictly: its [`Config`] when it holds no
/// error, else every error.
pub fn parse(text: &str) -> Result<Parsed, Rejected> {
    let r = reading(text.as_bytes());
    if r.errors.is_empty() {
        Ok(r.parsed)
    } else {
        Err(r.errors)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// Before any section, where the top-level keys sit.
    Top,
    /// The head at this index of `heads`.
    Head(usize),
    /// The look at this index of `looks`.
    Look(usize),
    Rules,
    /// `[labels]`, one label a line.
    Labels,
    /// A section an error dropped; its lines are skipped.
    Broken,
}

/// Where a rule sends what it matches, before every head is known.
enum Aim {
    Fixed(Target),
    /// A head's name.
    Name(String),
}

/// A rule as read, its target resolved once every head is known.
struct Read {
    line: usize,
    kind: Kind,
    matcher: Match,
    aim: Aim,
}

/// A head as read, with the keys it has set.
struct Declared {
    head: Head,
    keys: Vec<&'static str>,
    /// An error inside its section dropped it.
    dropped: bool,
}

/// A look as read, with the keys it has set.
struct DeclaredLook {
    look: Look,
    keys: Vec<&'static str>,
    /// An error inside its section dropped it.
    dropped: bool,
}

/// Where a key set once sits: among the top-level keys, or in the head or
/// the look at this index.
#[derive(Clone, Copy)]
enum Seen {
    Top,
    Head(usize),
    Look(usize),
}

#[derive(Default)]
struct Parser {
    errors: Rejected,
    /// Errors found once every line is read, merged in by line.
    late: Vec<Error>,
    /// Whether vtium.conf holds a section heading, so its flow needs
    /// `version = 1`; a policy line alone needs none (ADR-1403 D3).
    headed: bool,
    scopes: Vec<Scope>,
    policies: Vec<PolicyLine>,
    warnings: Vec<Warning>,
    section: Option<Section>,
    versioned: bool,
    /// The top-level keys set.
    top_keys: Vec<&'static str>,
    heads: Vec<Declared>,
    /// The names of heads whose heading an error dropped.
    dropped_names: Vec<String>,
    looks: Vec<DeclaredLook>,
    /// The names of looks an error dropped, whose heads take the default
    /// look with no warning of their own.
    dropped_looks: Vec<String>,
    looks_over: bool,
    rules: Vec<Read>,
    seen_rules: bool,
    labels: Vec<Label>,
    seen_labels: bool,
    heads_over: bool,
    rules_over: bool,
    /// An error dropped the whole file but its `policy` lines.
    whole: bool,
}

/// `s` without its outer blanks.
fn trim(s: &str) -> &str {
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
fn section_words(inner: &str) -> Vec<&str> {
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

fn blanks(s: &str) -> impl Iterator<Item = &str> {
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

/// Keeps the first [`ERRORS_MAX`] of findings sorted by line, and returns
/// how many it dropped.
fn keep_first<T>(findings: &mut Vec<T>) -> usize {
    let past = findings.len().saturating_sub(ERRORS_MAX);
    findings.truncate(ERRORS_MAX);
    past
}

/// The words `policy` takes.
pub(crate) const POLICY: [&str; 2] = ["open", "closed"];

/// The words `isolated` takes, yes then no.
pub(crate) const YES_NO: [&str; 2] = ["yes", "no"];

/// The forms `when-display-lost` takes (D8).
pub(crate) const WHEN_DISPLAY_LOST: [&str; 3] = ["keep", "hangup", "hangup DURATION"];

/// The keys a head takes in vtium.conf beside the key table's head keys
/// (ADR-1403 D5). Each is the head's own, even where the key table has a
/// top-level key of the same name: the top level's `vt` is the VT vtium
/// opens, and a head's `vt` the VTs the head runs on (ADR-1403 D4).
pub(crate) const CONF_HEAD_KEYS: [&str; 5] =
    ["look", "consoles", "when-display-lost", "isolated", "vt"];

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
        "isolated" => Takes::Words(&YES_NO),
        "when-display-lost" => Takes::Words(&WHEN_DISPLAY_LOST),
        "consoles" => Takes::Number { min: 1, max: None },
        "look" | VTLOOK => Takes::Name,
        // a key of the key table a section sets takes the table's values
        _ => match keys::find(key).map(|k| k.kind) {
            Some(keys::Kind::Choice(words)) => Takes::Words(words),
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
fn table_value(key: &'static Key, value: &str) -> Result<(), Problem> {
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
fn decimal(value: &str) -> Option<Option<u32>> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.parse().ok())
}

/// A head's `vt` (ADR-1403 D4): one VT number from 1 to [`VT_MAX`], or
/// several separated by blanks, each named once.
fn vts(value: &str) -> Result<Vec<u16>, Problem> {
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
fn when_display_lost(value: &str) -> Result<WhenDisplayLost, Problem> {
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
fn policy_line(line: &str) -> Result<PolicyLine, PolicyFault> {
    let [open, closed] = POLICY;
    match key_value(line) {
        None => Err(PolicyFault::NotKeyValue),
        Some((key, _)) if key != POLICY_KEY => Err(PolicyFault::Key(Quoted::new(key))),
        Some((_, value)) if value == open => Ok(PolicyLine::Open),
        Some((_, value)) if value == closed => Ok(PolicyLine::Closed),
        Some((_, value)) => Err(PolicyFault::Value(Quoted::new(value))),
    }
}

impl Parser {
    /// Records `problem` at `line`, `within` the part of the file the line
    /// sits in, and drops what its scope takes but a section, which the
    /// caller skips.
    fn error(&mut self, line: Option<usize>, problem: Problem, within: Within) {
        // the parser meets no problem of the trust level
        let scope = scope(&problem, within).unwrap_or(Scope::File);
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
        match (scope, self.section) {
            (Scope::File, _) => self.whole = true,
            (Scope::Head, Some(Section::Head(index))) if within == Within::Head => {
                if let Some(declared) = self.heads.get_mut(index) {
                    declared.dropped = true;
                }
            }
            // an error inside a look drops that look, and its heads take
            // the default look (ADR-1403 D5)
            (Scope::Section, Some(Section::Look(index))) if within == Within::Look => {
                if let Some(declared) = self.looks.get_mut(index) {
                    declared.dropped = true;
                }
            }
            _ => {}
        }
        self.errors
            .push_dropping(line, problem, Some(Drops::of(scope, within)));
    }

    /// Whether the line read next sits above the first section.
    fn top(&self) -> bool {
        matches!(self.section, None | Some(Section::Top))
    }

    /// Records the policy line `n` that vtium cannot read: the file is
    /// closed and broken, whatever its other lines hold (ADR-1401 D7). The
    /// error takes no scope of the rest, since the second level, the
    /// policy, settles the outcome.
    fn unreadable_policy(&mut self, n: usize, fault: PolicyFault) {
        self.policies.push(PolicyLine::Unreadable);
        self.errors.push_dropping(
            Some(n),
            Problem::UnreadablePolicy(fault),
            Some(Drops::Closed),
        );
    }

    /// Where a line sits, from the section it is read in. A line of
    /// `[labels]` counts as a top-level line, since an error there drops
    /// that line alone, as a top-level line's does.
    fn within(&self) -> Within {
        match self.section {
            None | Some(Section::Top | Section::Labels) => Within::Top,
            Some(Section::Head(_)) => Within::Head,
            Some(Section::Look(_)) => Within::Look,
            Some(Section::Rules | Section::Broken) => Within::Rules,
        }
    }

    /// One top-level line of vtium.conf: a policy line, read as the head
    /// file's is, or a `version` line; any other line is vtium's own key,
    /// which the console reads and this reader skips (ADR-1403 D3).
    fn top_line(&mut self, n: usize, raw: &[u8]) {
        let policy = begins_policy(raw);
        if raw.len() > LINE_MAX {
            if policy {
                self.unreadable_policy(n, PolicyFault::LineTooLong);
            }
            return;
        }
        let text = match std::str::from_utf8(raw) {
            Err(_) => Err(PolicyFault::NotUtf8),
            Ok(text) => match text.chars().find(|&c| c.is_control() && c != '\t') {
                Some(c) => Err(PolicyFault::ControlCharacter(c)),
                None => Ok(text),
            },
        };
        match text {
            Err(fault) if policy => self.unreadable_policy(n, fault),
            Err(_) => {}
            Ok(text) => {
                let line = content(text);
                let version = matches!(key_value(&line), Some((VERSION_KEY, _)));
                if policy || version {
                    self.section.get_or_insert(Section::Top);
                    if policy {
                        self.policy_key(n, &line);
                    } else {
                        self.top_key(n, &line);
                    }
                }
            }
        }
    }

    /// One line's bytes, its newline apart. A policy line is known by its
    /// first bytes, so one vtium cannot read for its length or its bytes
    /// is still a policy line.
    fn raw_line(&mut self, n: usize, raw: &[u8]) {
        let policy = self.top() && begins_policy(raw);
        // a heading vtium cannot read drops its section, and still ends the
        // lines above the first section
        let heading = heading(raw);
        let within = if heading {
            Within::Header
        } else {
            self.within()
        };
        if raw.len() > LINE_MAX {
            if policy {
                self.unreadable_policy(n, PolicyFault::LineTooLong);
            } else {
                if heading {
                    self.section = Some(Section::Broken);
                }
                self.error(Some(n), Problem::LineTooLong, within);
            }
            return;
        }
        let unreadable = match std::str::from_utf8(raw) {
            Err(_) => Err(Problem::NotUtf8),
            Ok(text) => match text.chars().find(|&c| c.is_control() && c != '\t') {
                Some(c) => Err(Problem::ControlCharacter(c)),
                None => Ok(text),
            },
        };
        match unreadable {
            Ok(text) => self.line(n, text, policy),
            Err(problem) if policy => {
                let fault = match problem {
                    Problem::ControlCharacter(c) => PolicyFault::ControlCharacter(c),
                    _ => PolicyFault::NotUtf8,
                };
                self.unreadable_policy(n, fault);
            }
            Err(problem) => {
                if heading {
                    self.section = Some(Section::Broken);
                }
                self.error(Some(n), problem, within);
            }
        }
    }

    fn line(&mut self, n: usize, raw: &str, policy: bool) {
        let line = content(raw);
        if line.is_empty() {
            return;
        }
        if let Some(inner) = line.strip_prefix('[') {
            self.section(n, inner);
            return;
        }
        let section = *self.section.get_or_insert(Section::Top);
        // a section vtium cannot read is ignored whole, the `policy` and
        // `version` lines in it with the rest
        let readable_section = !matches!(section, Section::Top | Section::Broken);
        if readable_section && matches!(key_value(&line), Some(("policy", _))) {
            self.policies.push(PolicyLine::InSection);
            self.error(Some(n), Problem::PolicyInSection, self.within());
            return;
        }
        // `version` is the top line misplaced, never the command line's
        // option of that name
        if readable_section && matches!(key_value(&line), Some((VERSION_KEY, _))) {
            self.error(Some(n), Problem::VersionInSection, self.within());
            return;
        }
        match section {
            Section::Top if policy => self.policy_key(n, &line),
            Section::Top => self.top_key(n, &line),
            Section::Head(index) => self.head_key(n, index, &line),
            Section::Look(index) => self.look_key(n, index, &line),
            Section::Rules => self.rule(n, &line),
            Section::Labels => self.label(n, &line),
            Section::Broken => {}
        }
    }

    /// Records `key` as set where `seen` says, naming the error when it
    /// was already.
    fn once(&mut self, n: usize, seen: Seen, key: &'static str) -> bool {
        let keys = match seen {
            Seen::Top => Some(&mut self.top_keys),
            Seen::Head(index) => self.heads.get_mut(index).map(|d| &mut d.keys),
            Seen::Look(index) => self.looks.get_mut(index).map(|d| &mut d.keys),
        };
        let Some(keys) = keys else {
            return false;
        };
        if keys.contains(&key) {
            self.error(Some(n), Problem::DuplicateKey(key), self.within());
            return false;
        }
        keys.push(key);
        true
    }

    /// A top-level line other than a policy line: a `version` line, else
    /// one of vtium's own keys, which the console reads and this reader
    /// skips (ADR-1403 D3).
    fn top_key(&mut self, n: usize, line: &str) {
        let Some((VERSION_KEY, value)) = key_value(line) else {
            return;
        };
        if self.once(n, Seen::Top, VERSION_KEY) {
            self.versioned = true;
            if value != "1" {
                self.error(
                    Some(n),
                    Problem::UnknownVersion(Quoted::new(value)),
                    Within::Top,
                );
            }
        }
    }

    /// A policy line vtium could decode: `policy = open` or
    /// `policy = closed`, a second of which is a key given twice, else a
    /// policy line vtium cannot read.
    fn policy_key(&mut self, n: usize, line: &str) {
        match policy_line(line) {
            Ok(read) => {
                self.policies.push(read);
                self.once(n, Seen::Top, POLICY_KEY);
            }
            Err(fault) => self.unreadable_policy(n, fault),
        }
    }

    fn section(&mut self, n: usize, inner: &str) {
        self.section = Some(Section::Broken);
        let Some(inner) = inner.strip_suffix(']') else {
            self.error(
                Some(n),
                Problem::Syntax(Syntax::UnclosedHeading),
                Within::Header,
            );
            return;
        };
        let words = section_words(inner);
        match words.as_slice() {
            ["rules"] => {
                if std::mem::replace(&mut self.seen_rules, true) {
                    self.error(Some(n), Problem::DuplicateSection("rules"), Within::Header);
                } else {
                    self.section = Some(Section::Rules);
                }
            }
            ["labels"] => {
                if std::mem::replace(&mut self.seen_labels, true) {
                    self.error(Some(n), Problem::DuplicateSection("labels"), Within::Header);
                } else {
                    self.section = Some(Section::Labels);
                }
            }
            ["guards"] => self.error(Some(n), Problem::Renamed(Renamed::Guards), Within::Header),
            ["head", name] => self.head(n, name),
            ["head", ..] => self.error(
                Some(n),
                Problem::Syntax(Syntax::HeadOneName),
                Within::Header,
            ),
            ["look", name] => self.look(n, name),
            ["look", ..] => self.error(
                Some(n),
                Problem::Syntax(Syntax::LookOneName),
                Within::Header,
            ),
            ["profile", ..] => self.error(
                Some(n),
                Problem::Renamed(Renamed::ProfileSection),
                Within::Header,
            ),
            _ => self.error(
                Some(n),
                Problem::NotASection(Quoted::new(inner)),
                Within::Header,
            ),
        }
    }

    fn head(&mut self, n: usize, name: &str) {
        let problem = if !good_name(name) {
            Problem::BadName(Quoted::new(name))
        } else if RESERVED.contains(&name) {
            Problem::ReservedName(Quoted::new(name))
        } else if name == "focus" {
            Problem::Renamed(Renamed::FocusHead)
        } else if name == "dark" {
            Problem::Renamed(Renamed::DarkHead)
        } else if self.heads.iter().any(|d| d.head.name == name) {
            Problem::DuplicateHead(Quoted::new(name))
        } else if self.heads.len() >= HEADS_MAX {
            if !std::mem::replace(&mut self.heads_over, true) {
                self.error(Some(n), Problem::TooManyHeads, Within::Header);
            }
            return;
        } else {
            self.section = Some(Section::Head(self.heads.len()));
            self.heads.push(Declared {
                head: Head::new(name, n),
                keys: Vec::new(),
                dropped: false,
            });
            return;
        };
        if !matches!(problem, Problem::DuplicateHead(_)) {
            // the rules naming it go with it
            self.dropped_names.push(name.to_owned());
        }
        self.error(Some(n), problem, Within::Header);
    }

    /// A look's heading in vtium.conf, `[look NAME]` (ADR-1403 D4).
    fn look(&mut self, n: usize, name: &str) {
        let problem = if !good_name(name) {
            Problem::BadLookName(Quoted::new(name))
        } else if self.looks.iter().any(|d| d.look.name == name) {
            Problem::DuplicateLook(Quoted::new(name))
        } else if self.looks.len() >= LOOKS_MAX {
            if !std::mem::replace(&mut self.looks_over, true) {
                self.error(Some(n), Problem::TooManyLooks, Within::Header);
            }
            return;
        } else {
            self.section = Some(Section::Look(self.looks.len()));
            self.looks.push(DeclaredLook {
                look: Look {
                    name: name.to_owned(),
                    line: n,
                    settings: Vec::new(),
                    vtlook: None,
                    vtlook_line: 0,
                },
                keys: Vec::new(),
                dropped: false,
            });
            return;
        };
        if !matches!(problem, Problem::DuplicateLook(_)) {
            // a head naming it takes the default look, and this error says why
            self.dropped_looks.push(name.to_owned());
        }
        self.error(Some(n), problem, Within::Header);
    }

    /// One line of a look's section: `vtlook`, or a key of the key table a
    /// look may set, its value checked against the key's values and bounds
    /// (ADR-1403 D5). An error drops the look.
    fn look_key(&mut self, n: usize, index: usize, line: &str) {
        let Some((key, value)) = key_value(line) else {
            self.error(Some(n), Problem::Syntax(Syntax::NotKeyValue), Within::Look);
            return;
        };
        if key == VTLOOK {
            if !self.once(n, Seen::Look(index), VTLOOK) {
                return;
            }
            if !good_name(value) {
                let problem = Problem::BadValue {
                    key: VTLOOK,
                    value: Quoted::new(value),
                };
                self.error(Some(n), problem, Within::Look);
            } else if let Some(d) = self.looks.get_mut(index) {
                d.look.vtlook = Some(value.to_owned());
                d.look.vtlook_line = n;
            }
            return;
        }
        let Some(table) = keys::find(key) else {
            self.error(Some(n), Problem::UnknownKey(Quoted::new(key)), Within::Look);
            return;
        };
        if !table.levels.contains(Level::Look) {
            let problem = Problem::NotHere {
                key: table.name,
                section: Level::Look,
                levels: table.levels,
            };
            self.error(Some(n), problem, Within::Look);
            return;
        }
        if !self.once(n, Seen::Look(index), table.name) {
            return;
        }
        match table_value(table, value) {
            Ok(()) => {
                if let Some(d) = self.looks.get_mut(index) {
                    d.look.settings.push(Setting {
                        key: table,
                        value: value.to_owned(),
                        line: n,
                    });
                }
            }
            Err(problem) => self.error(Some(n), problem, Within::Look),
        }
    }

    /// The key a head's line names; `Err` with the error for a key a head
    /// does not take, which names where the key may be set.
    fn head_key_name(key: &str) -> Result<&'static str, Problem> {
        let found = CONF_HEAD_KEYS
            .iter()
            .find(|k| **k == key)
            .copied()
            .or_else(|| {
                keys::find(key)
                    .filter(|k| k.levels.contains(Level::Head))
                    .map(|k| k.name)
            });
        found.ok_or_else(|| match key {
            "on-loss" => Problem::Renamed(Renamed::OnLoss),
            "rescue" => Problem::Renamed(Renamed::Rescue),
            "profile" => Problem::Renamed(Renamed::Profile),
            _ => match keys::find(key) {
                Some(table) => Problem::NotHere {
                    key: table.name,
                    section: Level::Head,
                    levels: table.levels,
                },
                None => Problem::UnknownKey(Quoted::new(key)),
            },
        })
    }

    fn head_key(&mut self, n: usize, index: usize, line: &str) {
        let Some((key, value)) = key_value(line) else {
            self.error(Some(n), Problem::Syntax(Syntax::NotKeyValue), Within::Head);
            return;
        };
        let key = match Self::head_key_name(key) {
            Ok(key) => key,
            Err(problem) => {
                self.error(Some(n), problem, Within::Head);
                return;
            }
        };
        if !self.once(n, Seen::Head(index), key) {
            return;
        }
        // a head's own key goes before the key table's key of its name: a
        // head's `vt` names the VTs it runs on, not the VT the top level's
        // `vt` opens (ADR-1403 D4)
        if let Some(table) = keys::find(key).filter(|_| !CONF_HEAD_KEYS.contains(&key)) {
            match table_value(table, value) {
                Ok(()) => {
                    if let Some(head) = self.heads.get_mut(index).map(|d| &mut d.head) {
                        if key == keys::LOGIN.name {
                            head.login = Some(value.to_owned());
                        }
                        head.settings.push(Setting {
                            key: table,
                            value: value.to_owned(),
                            line: n,
                        });
                    }
                }
                Err(problem) => self.error(Some(n), problem, Within::Head),
            }
            return;
        }
        let bad = || Problem::BadValue {
            key,
            value: Quoted::new(value),
        };
        let mut problem = None;
        let mut capped = false;
        if let Some(head) = self.heads.get_mut(index).map(|d| &mut d.head) {
            match key {
                "look" if good_name(value) => {
                    head.look = value.to_owned();
                    head.look_line = n;
                }
                "look" => problem = Some(bad()),
                "consoles" => match decimal(value) {
                    Some(Some(0)) => {
                        problem = Some(Problem::OutOfRange {
                            key,
                            value: Quoted::new(value),
                        });
                    }
                    Some(Some(count)) if count <= CONSOLES_MAX => {
                        head.consoles = count;
                        head.consoles_line = n;
                    }
                    Some(_) => {
                        head.consoles = CONSOLES_MAX;
                        head.consoles_line = n;
                        capped = true;
                    }
                    None => problem = Some(bad()),
                },
                "login" if !value.is_empty() => head.login = Some(value.to_owned()),
                "when-display-lost" => match when_display_lost(value) {
                    Ok(v) => head.when_display_lost = v,
                    Err(e) => problem = Some(e),
                },
                "vt" => match vts(value) {
                    Ok(v) => head.vts = v,
                    Err(e) => problem = Some(e),
                },
                "isolated" => {
                    let [yes, no] = YES_NO;
                    match value {
                        v if v == yes || v == no => head.isolated = Some(v == yes),
                        _ => problem = Some(bad()),
                    }
                }
                _ => problem = Some(bad()),
            }
        }
        if let Some(problem) = problem {
            self.error(Some(n), problem, Within::Head);
        }
        if capped {
            self.warnings.push(Warning::ConsolesCapped { line: n });
        }
    }

    fn rule(&mut self, n: usize, line: &str) {
        let words: Vec<&str> = blanks(line).collect();
        let kind = match words.first() {
            Some(&"display") => Kind::Display,
            Some(&"input") => Kind::Input,
            _ => {
                self.error(Some(n), Problem::Syntax(Syntax::RuleKind), Within::Rules);
                return;
            }
        };
        let Some(at) = words.iter().position(|w| *w == "->") else {
            self.error(Some(n), Problem::Syntax(Syntax::RuleArrow), Within::Rules);
            return;
        };
        let tokens = words.get(1..at).unwrap_or_default();
        let target = words.get(at.saturating_add(1)..).unwrap_or_default();
        let [target] = target else {
            self.error(
                Some(n),
                Problem::Syntax(Syntax::RuleOneTarget),
                Within::Rules,
            );
            return;
        };
        if self.rules.len() >= RULES_MAX {
            if !std::mem::replace(&mut self.rules_over, true) {
                self.error(Some(n), Problem::TooManyRules, Within::Rules);
            }
            return;
        }
        let Some(matcher) = self.matcher(n, tokens) else {
            return;
        };
        let aim = match (*target, kind) {
            (word @ ("roaming" | "ignore"), Kind::Display) | (word @ "off", Kind::Input) => {
                let target = match word {
                    "roaming" => "roaming",
                    "ignore" => "ignore",
                    _ => "off",
                };
                self.error(Some(n), Problem::WrongKind { target, kind }, Within::Rules);
                return;
            }
            ("focus", _) => {
                self.error(Some(n), Problem::Renamed(Renamed::Focus), Within::Rules);
                return;
            }
            ("dark", _) => {
                self.error(Some(n), Problem::Renamed(Renamed::Dark), Within::Rules);
                return;
            }
            ("roaming", _) => Aim::Fixed(Target::Roaming),
            ("ignore", _) => Aim::Fixed(Target::Ignore),
            ("off", _) => Aim::Fixed(Target::Off),
            (name, _) => Aim::Name(name.to_owned()),
        };
        self.rules.push(Read {
            line: n,
            kind,
            matcher,
            aim,
        });
    }

    /// One line of `[labels]`: `display MATCH = LABEL` or
    /// `input MATCH = LABEL`. The match is checked as a rule's match is,
    /// and the label runs from after the `=` that stands alone to the
    /// line's end. An error drops this line alone.
    fn label(&mut self, n: usize, line: &str) {
        let within = Within::Top;
        let mut words = blanks(line);
        let kind = match words.next() {
            Some("display") => Kind::Display,
            Some("input") => Kind::Input,
            _ => {
                self.error(Some(n), Problem::Syntax(Syntax::LabelKind), within);
                return;
            }
        };
        let tokens: Vec<&str> = words.take_while(|w| *w != "=").collect();
        // the byte after the first `=` that stands alone as a word
        let blank = |c: Option<char>| c.is_none_or(|c| c == ' ' || c == '\t');
        let equals = line
            .match_indices('=')
            .map(|(i, _)| i)
            .find(|&i| {
                let before = line.get(..i).and_then(|b| b.chars().next_back());
                let after = line
                    .get(i.saturating_add(1)..)
                    .and_then(|a| a.chars().next());
                blank(before) && blank(after)
            })
            .map(|i| i.saturating_add(1));
        let Some(after) = equals else {
            self.error(Some(n), Problem::Syntax(Syntax::LabelEquals), within);
            return;
        };
        let text = trim(line.get(after..).unwrap_or_default());
        let problem = if text.is_empty() {
            Some(Syntax::LabelEmpty)
        } else if text.chars().count() > LABEL_MAX {
            Some(Syntax::LabelLong)
        } else {
            None
        };
        // a match a rule cannot hold is named before the label's own fault
        let Some(matcher) = self.label_matcher(n, &tokens) else {
            return;
        };
        if let Some(problem) = problem {
            self.error(Some(n), Problem::Syntax(problem), within);
            return;
        }
        self.labels.push(Label {
            line: n,
            kind,
            matcher,
            selector: tokens.join(" "),
            text: text.to_owned(),
        });
    }

    /// A label's match, checked as [`Parser::matcher`] checks a rule's,
    /// with each error dropping the label's line alone.
    fn label_matcher(&mut self, n: usize, tokens: &[&str]) -> Option<Match> {
        let mut quiet = Parser {
            section: Some(Section::Rules),
            ..Parser::default()
        };
        let matcher = quiet.matcher(n, tokens);
        for e in quiet.errors.errors {
            self.error(e.line, e.problem, Within::Top);
        }
        matcher
    }

    fn matcher(&mut self, n: usize, tokens: &[&str]) -> Option<Match> {
        let problem = match tokens {
            [] => Problem::Syntax(Syntax::RuleMatch),
            ["*"] => return Some(Match::Any),
            _ if tokens.contains(&"*") => Problem::Syntax(Syntax::StarAlone),
            _ if tokens.len() > TOKENS_MAX => Problem::TooManyTokens,
            _ => {
                let mut parsed = Vec::with_capacity(tokens.len());
                let mut good = true;
                for token in tokens {
                    match Token::parse(token) {
                        Ok(t) => parsed.push(t),
                        Err(e) => {
                            let problem = match e {
                                TokenError::UnknownKey | TokenError::NoKey => {
                                    Problem::UnknownMatchKey(Quoted::new(token))
                                }
                                TokenError::BadValue => Problem::BadToken(Quoted::new(token)),
                            };
                            self.error(Some(n), problem, Within::Rules);
                            good = false;
                        }
                    }
                }
                return good.then_some(Match::All(parsed));
            }
        };
        self.error(Some(n), problem, Within::Rules);
        None
    }

    /// A late error, found once every line is read.
    fn late(&mut self, line: Option<usize>, problem: Problem, within: Within) {
        let scope = scope(&problem, within).unwrap_or(Scope::File);
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
        if scope == Scope::File {
            self.whole = true;
        }
        self.late.push(Error {
            line,
            problem,
            drops: Some(Drops::of(scope, within)),
        });
    }

    fn finish(mut self) -> Reading {
        // a file with no section, as a copied kmscon.conf is, needs no
        // version even when it states a policy (ADR-1403 D3)
        let needs_version = self.headed;
        if !self.versioned && needs_version {
            self.late(None, Problem::MissingVersion, Within::Top);
        }
        let mut heads = Vec::new();
        for declared in std::mem::take(&mut self.heads) {
            if declared.dropped {
                self.dropped_names.push(declared.head.name);
            } else {
                heads.push(declared.head);
            }
        }
        let mut looks = Vec::new();
        for declared in std::mem::take(&mut self.looks) {
            if declared.dropped {
                self.dropped_looks.push(declared.look.name);
            } else {
                looks.push(declared.look);
            }
        }
        // a head naming a look with no section draws with the default look
        {
            for head in &heads {
                let named = head.look != "default"
                    && !looks.iter().any(|l| l.name == head.look)
                    && !self.dropped_looks.contains(&head.look);
                if named {
                    self.warnings.push(Warning::NoLook {
                        line: head.look_line,
                        head: Quoted::new(&head.name),
                        look: Quoted::new(&head.look),
                    });
                }
            }
        }
        let mut rules = Vec::new();
        for read in std::mem::take(&mut self.rules) {
            let target = match read.aim {
                Aim::Fixed(target) => target,
                Aim::Name(name) => {
                    let declared = heads.iter().position(|h| h.name == name);
                    match declared {
                        Some(i) => Target::Head(i),
                        // a rule naming a dropped head goes with it
                        None if self.dropped_names.contains(&name) => continue,
                        None => {
                            let problem = Problem::UndefinedHead(Quoted::new(&name));
                            self.late(Some(read.line), problem, Within::Rules);
                            continue;
                        }
                    }
                }
            };
            rules.push(Rule {
                line: read.line,
                kind: read.kind,
                matcher: read.matcher,
                target,
            });
        }
        let late = std::mem::take(&mut self.late);
        self.errors.merge(late);
        let closes = |p: &PolicyLine| match p {
            PolicyLine::Closed | PolicyLine::Unreadable => true,
            PolicyLine::Open | PolicyLine::InSection => false,
        };
        let policy = if self.policies.iter().any(closes) {
            Policy::Closed
        } else {
            Policy::Open
        };
        let parsed = if self.whole {
            Parsed {
                config: Config {
                    policy,
                    survivors: true,
                    ..Config::default()
                },
                ..Parsed::default()
            }
        } else {
            let config = Config {
                policy,
                looks,
                heads,
                rules,
                survivors: !self.errors.is_empty(),
                tail: None,
                labels: std::mem::take(&mut self.labels),
            };
            findings(config, self.warnings)
        };
        Reading {
            parsed,
            errors: self.errors,
            policies: self.policies,
            scopes: self.scopes,
        }
    }
}

/// What survives, with the warnings its reading found and the findings
/// over its rules.
fn findings(config: Config, mut warnings: Vec<Warning>) -> Parsed {
    let mut notes = Vec::new();
    for (i, rule) in config.rules.iter().enumerate() {
        let earlier = config.rules.get(..i).unwrap_or_default();
        if let Some(by) = earlier
            .iter()
            .find(|e| e.kind == rule.kind && rule.matcher.within(&e.matcher))
        {
            warnings.push(Warning::Shadowed {
                line: rule.line,
                by: by.line,
            });
        }
        let weak = matches!(rule.target, Target::Head(_))
            && rule.kind == Kind::Input
            && !rule.matcher.tokens().is_empty()
            && rule
                .matcher
                .tokens()
                .iter()
                .all(|t| matches!(t, Token::Id(..) | Token::Name(_)));
        if weak {
            notes.push(Note::WeakMatch { line: rule.line });
        }
    }
    warnings.sort_by_key(Warning::line);
    notes.sort_by_key(Note::line);
    let more_warnings = keep_first(&mut warnings);
    let more_notes = keep_first(&mut notes);
    Parsed {
        config,
        warnings,
        notes,
        more_warnings,
        more_notes,
    }
}
