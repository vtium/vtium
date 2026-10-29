//! The head file's text to a [`Config`], or rejected whole (ADR-1401 D3,
//! D7).
//!
//! The first line that is not blank or a comment is `version = 1`. Then
//! come sections: `[head NAME]` with a head's keys, `[rules]` with one
//! rule a line, and `[guards]` with the guards' switches. `#` starts a
//! comment anywhere on a line; blanks (spaces and tabs) around `=` and
//! between tokens are dropped. Every line is read however many errors come
//! before it, so a check names every error the file holds, each with its
//! line, and no part of an invalid file applies.

use crate::bounds::{
    CONSOLES_MAX, FONT_SIZE_MAX, HEADS_MAX, LINE_MAX, NAME_MAX, RULES_MAX, TOKENS_MAX,
};
use crate::config::{Config, Guards, Head, OnLoss, Rule, Target};
use crate::error::{Error, Note, Problem, Quoted, Rejected, Warning};
use crate::identity::Kind;
use crate::matching::{Match, Token, TokenError};

/// A valid head file and what its check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub config: Config,
    /// Findings `vtium check-config` counts against the file.
    pub warnings: Vec<Warning>,
    /// Findings it reports without counting them.
    pub notes: Vec<Note>,
}

/// Parses a head file's text; [`text`](crate::text) checks its bytes
/// first.
pub fn parse(text: &str) -> Result<Parsed, Rejected> {
    let mut p = Parser::default();
    for (n, line) in text.split('\n').enumerate() {
        p.line(n.saturating_add(1), line);
    }
    p.finish()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// After the version, before any section.
    Top,
    /// The head at this index of `heads`.
    Head(usize),
    Rules,
    Guards,
    /// A section that did not parse; its lines are skipped.
    Broken,
}

/// A rule whose target names a head, resolved once every head is known.
struct Pending {
    rule: usize,
    name: String,
}

#[derive(Default)]
struct Parser {
    errors: Rejected,
    warnings: Vec<Warning>,
    notes: Vec<Note>,
    versioned: bool,
    section: Option<Section>,
    config: Config,
    pending: Vec<Pending>,
    /// The keys each head has set, by its index.
    head_keys: Vec<Vec<&'static str>>,
    guard_keys: Vec<&'static str>,
    seen_rules: bool,
    seen_guards: bool,
    heads_over: bool,
    rules_over: bool,
}

/// A line with its comment and its outer blanks dropped.
fn content(line: &str) -> &str {
    let code = line.split_once('#').map_or(line, |(code, _)| code);
    code.trim_matches(|c| c == ' ' || c == '\t')
}

fn blanks(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t']).filter(|t| !t.is_empty())
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

/// `on` or `off`.
fn switch(value: &str) -> Option<bool> {
    match value {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// A decimal number of digits alone; `None` for anything else, and
/// `Some(None)` for digits past `u32`.
fn decimal(value: &str) -> Option<Option<u32>> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.parse().ok())
}

/// `on-loss`'s value (D8): `nohup`, `hangup`, or `hangup DURATION`, the
/// duration in seconds or with an `s`, `m` or `h` suffix.
fn on_loss(value: &str) -> Result<OnLoss, Problem> {
    let bad = || Problem::BadValue {
        key: "on-loss",
        value: Quoted::new(value),
    };
    let mut words = blanks(value);
    let (first, second, rest) = (words.next(), words.next(), words.next());
    match (first, second, rest) {
        (Some("nohup"), None, _) => Ok(OnLoss::Nohup),
        (Some("hangup"), None, _) => Ok(OnLoss::Hangup),
        (Some("hangup"), Some(duration), None) => {
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
                .map(OnLoss::HangupAfter)
                .ok_or(Problem::OutOfRange {
                    key: "on-loss",
                    value: Quoted::new(value),
                })
        }
        _ => Err(bad()),
    }
}

impl Parser {
    fn error(&mut self, line: usize, problem: Problem) {
        self.errors.push(Some(line), problem);
    }

    fn line(&mut self, n: usize, raw: &str) {
        if raw.len() > LINE_MAX {
            self.error(n, Problem::LineTooLong);
            return;
        }
        let line = content(raw);
        if line.is_empty() {
            return;
        }
        if !self.versioned {
            self.versioned = true;
            self.section = Some(Section::Top);
            match line.split_once('=') {
                Some((key, value)) if content(key) == "version" => {
                    let value = content(value);
                    if value != "1" {
                        self.error(n, Problem::UnknownVersion(Quoted::new(value)));
                    }
                    return;
                }
                // the line is still read for what it is, so its own
                // errors are named too
                _ => self.error(n, Problem::MissingVersion),
            }
        }
        if let Some(inner) = line.strip_prefix('[') {
            self.section(n, inner);
            return;
        }
        match self.section {
            Some(Section::Head(index)) => self.head_key(n, index, line),
            Some(Section::Rules) => self.rule(n, line),
            Some(Section::Guards) => self.guard_key(n, line),
            Some(Section::Broken) => {}
            Some(Section::Top) | None => self.error(n, Problem::Syntax("a line outside a section")),
        }
    }

    fn section(&mut self, n: usize, inner: &str) {
        self.section = Some(Section::Broken);
        let Some(inner) = inner.strip_suffix(']') else {
            self.error(n, Problem::Syntax("a section header with no closing `]`"));
            return;
        };
        let words: Vec<&str> = blanks(inner).collect();
        match words.as_slice() {
            ["rules"] => {
                if std::mem::replace(&mut self.seen_rules, true) {
                    self.error(n, Problem::DuplicateSection("rules"));
                } else {
                    self.section = Some(Section::Rules);
                }
            }
            ["guards"] => {
                if std::mem::replace(&mut self.seen_guards, true) {
                    self.error(n, Problem::DuplicateSection("guards"));
                } else {
                    self.section = Some(Section::Guards);
                }
            }
            ["head", name] => self.head(n, name),
            ["head", ..] => self.error(n, Problem::Syntax("`[head NAME]` takes one name")),
            _ => self.error(n, Problem::UnknownSection(Quoted::new(inner))),
        }
    }

    fn head(&mut self, n: usize, name: &str) {
        if !good_name(name) {
            self.error(n, Problem::BadName(Quoted::new(name)));
        } else if matches!(name, "focus" | "ignore" | "dark") {
            self.error(n, Problem::ReservedName(Quoted::new(name)));
        } else if self.config.head(name).is_some() {
            self.error(n, Problem::DuplicateHead(Quoted::new(name)));
        } else if self.config.heads.len() >= HEADS_MAX {
            if !std::mem::replace(&mut self.heads_over, true) {
                self.error(n, Problem::TooManyHeads);
            }
        } else {
            self.section = Some(Section::Head(self.config.heads.len()));
            self.config.heads.push(Head::new(name, n));
            self.head_keys.push(Vec::new());
        }
    }

    /// Splits `key = value`, naming the error when the line is not one.
    fn key_value<'a>(&mut self, n: usize, line: &'a str) -> Option<(&'a str, &'a str)> {
        let Some((key, value)) = line.split_once('=') else {
            self.error(n, Problem::Syntax("expected `key = value`"));
            return None;
        };
        Some((content(key), content(value)))
    }

    /// Records `key` as set in `seen`, naming the error when it was.
    fn once(&mut self, n: usize, seen_in: Option<usize>, key: &'static str) -> bool {
        let seen = match seen_in {
            Some(index) => self.head_keys.get_mut(index),
            None => Some(&mut self.guard_keys),
        };
        let Some(seen) = seen else { return false };
        if seen.contains(&key) {
            self.error(n, Problem::DuplicateKey(key));
            return false;
        }
        seen.push(key);
        true
    }

    fn head_key(&mut self, n: usize, index: usize, line: &str) {
        const KEYS: [&str; 7] = [
            "profile",
            "consoles",
            "login",
            "on-loss",
            "rescue",
            "font-name",
            "font-size",
        ];
        let Some((key, value)) = self.key_value(n, line) else {
            return;
        };
        let Some(&key) = KEYS.iter().find(|k| **k == key) else {
            self.error(n, Problem::UnknownKey(Quoted::new(key)));
            return;
        };
        if !self.once(n, Some(index), key) {
            return;
        }
        let bad = || Problem::BadValue {
            key,
            value: Quoted::new(value),
        };
        let out_of_range = || Problem::OutOfRange {
            key,
            value: Quoted::new(value),
        };
        let mut problem = None;
        let mut capped = false;
        let mut rescue_off = false;
        if let Some(head) = self.config.heads.get_mut(index) {
            match key {
                "profile" if good_name(value) => head.profile = value.to_owned(),
                "profile" => problem = Some(Problem::BadName(Quoted::new(value))),
                "consoles" => match decimal(value) {
                    Some(Some(0)) => problem = Some(out_of_range()),
                    Some(Some(count)) if count <= CONSOLES_MAX => head.consoles = count,
                    Some(_) => {
                        head.consoles = CONSOLES_MAX;
                        capped = true;
                    }
                    None => problem = Some(bad()),
                },
                "login" if !value.is_empty() => head.login = Some(value.to_owned()),
                "on-loss" => match on_loss(value) {
                    Ok(v) => head.on_loss = v,
                    Err(e) => problem = Some(e),
                },
                "rescue" => match switch(value) {
                    Some(on) => {
                        head.rescue = on;
                        rescue_off = !on;
                    }
                    None => problem = Some(bad()),
                },
                "font-name" if !value.is_empty() => head.font.name = Some(value.to_owned()),
                "font-size" => match decimal(value) {
                    Some(Some(size)) if size <= FONT_SIZE_MAX => head.font.size = Some(size),
                    Some(_) => problem = Some(out_of_range()),
                    None => problem = Some(bad()),
                },
                _ => problem = Some(bad()),
            }
        }
        if let Some(problem) = problem {
            self.error(n, problem);
        }
        if capped {
            self.warnings.push(Warning::ConsolesCapped { line: n });
        }
        if rescue_off {
            self.notes.push(Note::RescueOff { line: n });
        }
    }

    fn guard_key(&mut self, n: usize, line: &str) {
        let Some((key, value)) = self.key_value(n, line) else {
            return;
        };
        let guard: &'static str = match key {
            "anti-lockout" => "anti-lockout",
            "rescue-key" => "rescue-key",
            "never-dark" => "never-dark",
            _ => {
                self.error(n, Problem::UnknownKey(Quoted::new(key)));
                return;
            }
        };
        if !self.once(n, None, guard) {
            return;
        }
        let Some(on) = switch(value) else {
            self.error(
                n,
                Problem::BadValue {
                    key: guard,
                    value: Quoted::new(value),
                },
            );
            return;
        };
        let g: &mut Guards = &mut self.config.guards;
        match guard {
            "anti-lockout" => g.anti_lockout = on,
            "rescue-key" => g.rescue_key = on,
            _ => g.never_dark = on,
        }
        if !on {
            self.notes.push(Note::GuardOff { line: n, guard });
        }
    }

    fn rule(&mut self, n: usize, line: &str) {
        let words: Vec<&str> = blanks(line).collect();
        let kind = match words.first() {
            Some(&"display") => Kind::Display,
            Some(&"input") => Kind::Input,
            _ => {
                self.error(
                    n,
                    Problem::Syntax("a rule starts with `display` or `input`"),
                );
                return;
            }
        };
        let arrow = words.iter().position(|w| *w == "->");
        let (tokens, target) = match arrow {
            Some(at) => (
                words.get(1..at).unwrap_or_default(),
                words.get(at.saturating_add(1)..).unwrap_or_default(),
            ),
            None => {
                self.error(n, Problem::Syntax("a rule is `KIND MATCH -> TARGET`"));
                return;
            }
        };
        let [target] = target else {
            self.error(n, Problem::Syntax("a rule names one target after `->`"));
            return;
        };
        if self.config.rules.len() >= RULES_MAX {
            if !std::mem::replace(&mut self.rules_over, true) {
                self.error(n, Problem::TooManyRules);
            }
            return;
        }
        let Some(matcher) = self.matcher(n, tokens) else {
            return;
        };
        let target = match *target {
            "focus" | "ignore" if kind == Kind::Display => {
                let target = if *target == "focus" {
                    "focus"
                } else {
                    "ignore"
                };
                self.error(n, Problem::WrongKind { target, kind });
                return;
            }
            "dark" if kind == Kind::Input => {
                self.error(
                    n,
                    Problem::WrongKind {
                        target: "dark",
                        kind,
                    },
                );
                return;
            }
            "focus" => Target::Focus,
            "ignore" => Target::Ignore,
            "dark" => Target::Dark,
            name => {
                self.pending.push(Pending {
                    rule: self.config.rules.len(),
                    name: name.to_owned(),
                });
                // resolved in `finish`, once every head is declared
                Target::Head(usize::MAX)
            }
        };
        self.config.rules.push(Rule {
            line: n,
            kind,
            matcher,
            target,
        });
    }

    fn matcher(&mut self, n: usize, tokens: &[&str]) -> Option<Match> {
        match tokens {
            [] => {
                self.error(
                    n,
                    Problem::Syntax("a rule matches `*` or `key:value` tokens"),
                );
                None
            }
            ["*"] => Some(Match::Any),
            _ if tokens.contains(&"*") => {
                self.error(n, Problem::Syntax("`*` stands alone in a match"));
                None
            }
            _ if tokens.len() > TOKENS_MAX => {
                self.error(n, Problem::TooManyTokens);
                None
            }
            _ => {
                let mut parsed = Vec::with_capacity(tokens.len());
                let mut good = true;
                for token in tokens {
                    match Token::parse(token) {
                        Ok(t) => parsed.push(t),
                        Err(TokenError::UnknownKey | TokenError::NoKey) => {
                            self.error(n, Problem::UnknownMatchKey(Quoted::new(token)));
                            good = false;
                        }
                        Err(TokenError::BadValue) => {
                            self.error(n, Problem::BadToken(Quoted::new(token)));
                            good = false;
                        }
                    }
                }
                good.then_some(Match::All(parsed))
            }
        }
    }

    fn finish(mut self) -> Result<Parsed, Rejected> {
        // errors found once every line is read, merged in by line
        let mut late = Vec::new();
        if !self.versioned {
            late.push(Error {
                line: None,
                problem: Problem::MissingVersion,
            });
        }
        for pending in std::mem::take(&mut self.pending) {
            let index = self
                .config
                .heads
                .iter()
                .position(|h| h.name == pending.name);
            let line = self.config.rules.get(pending.rule).map_or(0, |r| r.line);
            match (index, self.config.rules.get_mut(pending.rule)) {
                (Some(index), Some(rule)) => rule.target = Target::Head(index),
                _ => late.push(Error {
                    line: Some(line),
                    problem: Problem::UndefinedHead(Quoted::new(&pending.name)),
                }),
            }
        }
        self.errors.merge(late);
        if !self.errors.is_empty() {
            return Err(self.errors);
        }
        let config = self.config;
        let mut warnings = self.warnings;
        let mut notes = self.notes;
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
        warnings.sort_by_key(|w| match w {
            Warning::ConsolesCapped { line } | Warning::Shadowed { line, .. } => *line,
        });
        notes.sort_by_key(|n| match n {
            Note::GuardOff { line, .. } | Note::RescueOff { line } | Note::WeakMatch { line } => {
                *line
            }
        });
        Ok(Parsed {
            config,
            warnings,
            notes,
        })
    }
}
