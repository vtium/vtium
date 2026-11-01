//! The head file's bytes to what survives of them (ADR-1401 D3, D7).
//!
//! Above the first section come the top-level keys, `version = 1` and
//! `policy`, in either order. Then come sections: `[head NAME]` with a
//! head's keys, and `[rules]` with one rule a line. A CRLF line ending is a
//! newline. `#` starts a comment at the start of a line or after a blank
//! (a space or a tab), a `#` inside a word is literal, and `\#` after a
//! blank is a literal `#`; blanks around `=` and between tokens are
//! dropped.
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
    CONSOLES_MAX, ERRORS_MAX, FILE_MAX, HEADS_MAX, LINE_MAX, NAME_MAX, RULES_MAX, TOKENS_MAX,
};
use crate::config::{Config, Head, Rule, Target, WhenDisplayLost};
use crate::error::{
    Drops, Error, Note, PolicyFault, Problem, Quoted, Rejected, Renamed, Syntax, Warning,
};
use crate::identity::Kind;
use crate::matching::{Match, Token, TokenError};
use crate::outcome::{Kept, Policy, PolicyLine, Scope, Trusted, Within, scope};

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

/// Reads a head file's bytes: at most [`FILE_MAX`] of them, each line
/// checked on its own for UTF-8, control characters and its length, so a
/// bad line drops what its scope takes and no more. Past [`FILE_MAX`] the
/// file is dropped whole, and its `policy` lines are read from the lines
/// within the bound; a policy line the bound cuts cannot be read.
pub fn reading(bytes: &[u8]) -> Reading {
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
    let mut lines = within.split(|&b| b == b'\n').peekable();
    let mut n = 0_usize;
    while let Some(raw) = lines.next() {
        n = n.saturating_add(1);
        // a carriage return ending a line is part of its newline
        let raw = match (lines.peek(), raw.split_last()) {
            (Some(_), Some((b'\r', rest))) => rest,
            _ => raw,
        };
        p.raw_line(n, raw);
    }
    // the lines end in an empty one, numbered as the line the bound cuts
    if p.top() && begins_policy(cut) {
        p.unreadable_policy(n, PolicyFault::Cut);
    }
    p.finish()
}

/// Whether `c` does not show on screen as itself: a space or a format
/// character other than a blank or a tab, such as a no-break space, a
/// zero-width space or a byte-order mark, which a quote escapes.
fn hidden(c: char) -> bool {
    !matches!(c, ' ' | '\t' | '"' | '\'' | '\\') && c.escape_debug().count() > 1
}

/// The UTF-8 byte-order mark.
const BOM: &[u8] = b"\xef\xbb\xbf";

/// Whether a line's bytes, after their leading blanks, begin with `policy`
/// in any case: a policy line, wherever above the first section it sits,
/// whatever bytes follow (ADR-1401 D7).
fn begins_policy(raw: &[u8]) -> bool {
    let start = raw
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(raw.len());
    raw.get(start..)
        .and_then(|rest| rest.get(..POLICY_KEY.len()))
        .is_some_and(|word| word.eq_ignore_ascii_case(POLICY_KEY.as_bytes()))
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
    Rules,
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

#[derive(Default)]
struct Parser {
    errors: Rejected,
    /// Errors found once every line is read, merged in by line.
    late: Vec<Error>,
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
    rules: Vec<Read>,
    seen_rules: bool,
    heads_over: bool,
    rules_over: bool,
    /// An error dropped the whole file but its `policy` lines.
    whole: bool,
}

/// `s` without its outer blanks.
fn trim(s: &str) -> &str {
    s.trim_matches(|c| c == ' ' || c == '\t')
}

/// A line with its comment and its outer blanks dropped: `#` at the start
/// of the line or after a blank starts the comment, and `\#` there is a
/// literal `#`.
fn content(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut after_blank = true;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '#' if after_blank => break,
            '\\' if after_blank && chars.peek() == Some(&'#') => {
                chars.next();
                out.push('#');
                after_blank = false;
                continue;
            }
            _ => out.push(c),
        }
        after_blank = matches!(c, ' ' | '\t');
    }
    trim(&out).to_owned()
}

fn blanks(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t']).filter(|t| !t.is_empty())
}

/// `key = value`, each trimmed, where the line has an `=`.
fn key_value(line: &str) -> Option<(&str, &str)> {
    line.split_once('=').map(|(k, v)| (trim(k), trim(v)))
}

/// Whether `name` keeps the name rule: 1 to [`NAME_MAX`] of
/// `A-Z a-z 0-9 . _ + -`.
fn good_name(name: &str) -> bool {
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

/// The top-level key that states the file's policy.
const POLICY_KEY: &str = "policy";

/// The words `policy` takes.
pub(crate) const POLICY: [&str; 2] = ["open", "closed"];

/// The words `isolated` takes, yes then no.
pub(crate) const YES_NO: [&str; 2] = ["yes", "no"];

/// The forms `when-display-lost` takes (D8).
pub(crate) const WHEN_DISPLAY_LOST: [&str; 3] = ["keep", "hangup", "hangup DURATION"];

/// The reserved names, a head's name no head may take: the targets.
/// The first cut's targets, `focus` and `dark`, stay reserved too.
const RESERVED: [&str; 3] = ["roaming", "ignore", "off"];

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
}

/// What `key` takes, by the parser's tables: the values a bad value is
/// measured against.
pub(crate) fn takes(key: &str) -> Takes {
    match key {
        "policy" => Takes::Words(&POLICY),
        "isolated" => Takes::Words(&YES_NO),
        "when-display-lost" => Takes::Words(&WHEN_DISPLAY_LOST),
        "consoles" => Takes::Number { min: 1, max: None },
        _ => Takes::Text,
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

    /// Where a line sits, from the section it is read in.
    fn within(&self) -> Within {
        match self.section {
            None | Some(Section::Top) => Within::Top,
            Some(Section::Head(_)) => Within::Head,
            Some(Section::Rules | Section::Broken) => Within::Rules,
        }
    }

    /// One line's bytes, its newline apart. A policy line is known by its
    /// first bytes, so one vtium cannot read for its length or its bytes
    /// is still a policy line.
    fn raw_line(&mut self, n: usize, raw: &[u8]) {
        let policy = self.top() && begins_policy(raw);
        // a heading vtium cannot read drops its section, and still ends the
        // lines above the first section
        let heading = raw
            .iter()
            .find(|b| !matches!(b, b' ' | b'\t'))
            .is_some_and(|&b| b == b'[');
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
        if section != Section::Top && matches!(key_value(&line), Some(("policy", _))) {
            self.policies.push(PolicyLine::InSection);
            self.error(Some(n), Problem::PolicyInSection, self.within());
            return;
        }
        match section {
            Section::Top if policy => self.policy_key(n, &line),
            Section::Top => self.top_key(n, &line),
            Section::Head(index) => self.head_key(n, index, &line),
            Section::Rules => self.rule(n, &line),
            Section::Broken => {}
        }
    }

    /// Records `key` as set in `seen`, naming the error when it was.
    fn once(&mut self, n: usize, seen_in: Option<usize>, key: &'static str) -> bool {
        let seen = match seen_in {
            Some(index) => self.heads.get(index).map(|d| &d.keys),
            None => Some(&self.top_keys),
        };
        if seen.is_some_and(|keys| keys.contains(&key)) {
            self.error(Some(n), Problem::DuplicateKey(key), self.within());
            return false;
        }
        match seen_in {
            Some(index) => {
                if let Some(declared) = self.heads.get_mut(index) {
                    declared.keys.push(key);
                }
            }
            None => self.top_keys.push(key),
        }
        true
    }

    fn top_key(&mut self, n: usize, line: &str) {
        match key_value(line) {
            Some(("version", value)) => {
                if self.once(n, None, "version") {
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
            // a key that holds a character that does not show is quoted, so
            // its message shows the character the owner cannot see
            _ if line
                .split('=')
                .next()
                .is_some_and(|key| key.chars().any(hidden)) =>
            {
                self.error(
                    Some(n),
                    Problem::HiddenOutsideSection(Quoted::new(line)),
                    Within::Top,
                )
            }
            _ => self.error(
                Some(n),
                Problem::Syntax(Syntax::OutsideSection),
                Within::Top,
            ),
        }
    }

    /// A policy line vtium could decode: `policy = open` or
    /// `policy = closed`, a second of which is a key given twice, else a
    /// policy line vtium cannot read.
    fn policy_key(&mut self, n: usize, line: &str) {
        match policy_line(line) {
            Ok(read) => {
                self.policies.push(read);
                self.once(n, None, POLICY_KEY);
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
        let words: Vec<&str> = blanks(inner).collect();
        match words.as_slice() {
            ["rules"] => {
                if std::mem::replace(&mut self.seen_rules, true) {
                    self.error(Some(n), Problem::DuplicateSection("rules"), Within::Header);
                } else {
                    self.section = Some(Section::Rules);
                }
            }
            ["guards"] => self.error(Some(n), Problem::Renamed(Renamed::Guards), Within::Header),
            ["head", name] => self.head(n, name),
            ["head", ..] => self.error(
                Some(n),
                Problem::Syntax(Syntax::HeadOneName),
                Within::Header,
            ),
            _ => self.error(
                Some(n),
                Problem::UnknownSection(Quoted::new(inner)),
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

    fn head_key(&mut self, n: usize, index: usize, line: &str) {
        const KEYS: [&str; 5] = [
            "profile",
            "consoles",
            "login",
            "when-display-lost",
            "isolated",
        ];
        let Some((key, value)) = key_value(line) else {
            self.error(Some(n), Problem::Syntax(Syntax::NotKeyValue), Within::Head);
            return;
        };
        let Some(&key) = KEYS.iter().find(|k| **k == key) else {
            let problem = match key {
                "on-loss" => Problem::Renamed(Renamed::OnLoss),
                "rescue" => Problem::Renamed(Renamed::Rescue),
                "font-name" => Problem::Renamed(Renamed::HeadFont("font-name")),
                "font-size" => Problem::Renamed(Renamed::HeadFont("font-size")),
                _ => Problem::UnknownKey(Quoted::new(key)),
            };
            self.error(Some(n), problem, Within::Head);
            return;
        };
        if !self.once(n, Some(index), key) {
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
                "profile" if good_name(value) => {
                    head.profile = value.to_owned();
                    head.profile_line = n;
                }
                "profile" => problem = Some(Problem::BadName(Quoted::new(value))),
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
        if !self.versioned {
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
                heads,
                rules,
                survivors: !self.errors.is_empty(),
                tail: None,
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
