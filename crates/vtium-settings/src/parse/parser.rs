//! The parser's handling of each line: the top-level keys, the section
//! headings, a head's and a look's keys, the rules and the labels, and
//! the finish that resolves each rule's head and gathers what survives.

use super::lines::{
    CONF_HEAD_KEYS, RESERVED, VTLOOK, YES_NO, blanks, content, decimal, good_name, key_value,
    policy_line, section_words, table_value, trim, vts, when_display_lost,
};
use super::{
    Aim, Declared, DeclaredLook, Parsed, Parser, Read, Reading, Section, Seen, findings, of,
};
use crate::bounds::{CONSOLES_MAX, HEADS_MAX, LINE_MAX, LOOKS_MAX, RULES_MAX, TOKENS_MAX};
use crate::config::{Config, Head, LABEL_MAX, Label, Look, Rule, Setting, Target};
use crate::error::{Drops, Error, PolicyFault, Problem, Quoted, Renamed, Syntax, Warning};
use crate::identity::Kind;
use crate::keys::{self, Level};
use crate::matching::{Match, Token, TokenError};
use crate::outcome::{PolicyLine, Scope, Within, scope};
use crate::split::{POLICY_KEY, VERSION_KEY, begins_policy, heading};

impl Parser {
    /// Records `problem` at `line`, `within` the part of the file the line
    /// sits in, and drops what its scope takes but a section, which the
    /// caller skips.
    pub(super) fn error(&mut self, line: Option<usize>, problem: Problem, within: Within) {
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
    pub(super) fn top(&self) -> bool {
        matches!(self.section, None | Some(Section::Top))
    }

    /// Records the policy line `n` that vtium cannot read: the file is
    /// closed and broken, whatever its other lines hold (ADR-1401 D7). The
    /// error takes no scope of the rest, since the second level, the
    /// policy, settles the outcome.
    pub(super) fn unreadable_policy(&mut self, n: usize, fault: PolicyFault) {
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
    pub(super) fn top_line(&mut self, n: usize, raw: &[u8]) {
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
    pub(super) fn raw_line(&mut self, n: usize, raw: &[u8]) {
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
    /// look may set, its value checked against the key's values and bounds.
    /// A key vtium does not know, or one a look does not take, drops only
    /// its line, and any other error drops the look.
    ///
    /// References:
    /// - ADR-1403 D5: the keys a look takes.
    /// - ADR-1510 D2: an unknown or misplaced key drops only its line.
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
                "isolated" | "switch-on-enter" => {
                    let [yes, no] = YES_NO;
                    match value {
                        v if v == yes || v == no => {
                            if key == "isolated" {
                                head.isolated = Some(v == yes);
                            } else {
                                head.switch_on_enter = v == yes;
                            }
                        }
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

    pub(super) fn rule(&mut self, n: usize, line: &str) {
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

    pub(super) fn finish(mut self) -> Reading {
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
        let policy = of(&self.policies);
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
                // a key vtium ignores drops nothing, so the file runs as
                // written beside it, as it does with no error at all
                survivors: self.scopes.iter().any(|s| s.drops())
                    || self.policies.contains(&PolicyLine::Unreadable),
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
