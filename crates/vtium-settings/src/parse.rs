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
//! `when-display-lost`, `isolated`, `switch-on-enter` and the key table's head keys, and a
//! look takes `vtlook` and the key table's look keys, each value checked
//! against the key's values and bounds (ADR-1403 D5).
//!
//! [`reading`] reads every line however many errors come before it, so a
//! check names every error the file holds, each with its line. Each error
//! drops what [`scope`] says it takes. A bad rule drops that rule. A key
//! that vtium does not know, or that its section does not take, drops only
//! its line. Any other error inside a head's section drops the head with
//! every rule naming it, and an error that leaves the file's structure
//! untrustworthy drops everything but the `policy` lines. A policy line, a line above the
//! first section whose bytes begin with `policy` in any case, is judged
//! before its bytes are decoded: one vtium cannot read makes the file
//! closed and broken (ADR-1401 D7). What survives is a [`Config`], and
//! the `policy` lines, the scopes and what survives classify the file for
//! [`Trusted::of`]. [`parse`] is the strict reading: a [`Config`] only for
//! a file with no error.

use crate::bounds::{ERRORS_MAX, FILE_MAX};
use crate::config::{Config, Head, Label, Look, Target};
use crate::error::{Error, Note, PolicyFault, Problem, Rejected, Warning};
use crate::identity::Kind;
use crate::matching::{Match, Token};
use crate::outcome::{Kept, Policy, PolicyLine, Scope, Trusted, Within};
use crate::split::{BOM, begins_policy, numbered, split, split_after};

#[cfg(test)]
use crate::keys::Level;
#[cfg(doc)]
use crate::outcome::scope;

mod lines;
mod parser;

pub use lines::escape_label;
pub(crate) use lines::{
    CONF_HEAD_KEYS, POLICY, RESERVED, Takes, VTLOOK, content, content_range, good_name, key_value,
    range, rule_line, rules_heading, takes,
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

/// The policy of vtium.conf's bytes, read from the lines above the first
/// section alone, by the rules [`reading`] reads it by (ADR-1401 D7, D9):
/// closed when a readable policy line says `closed` or vtium cannot read
/// a policy line, a line the file bound cuts among them, and open when
/// neither holds. vtium reads the policy alone this way when the kernel
/// command line turns it off for the boot, so it reads no section and no
/// other key of a file it then does not run.
pub fn policy(bytes: &[u8]) -> Policy {
    let mut p = Parser::default();
    feed(&mut p, bytes, false);
    of(&p.policies)
}

/// The policy that a file's `policy` lines give: closed when one of them
/// says `closed` or vtium cannot read it, else open.
fn of(policies: &[PolicyLine]) -> Policy {
    let closes = |p: &PolicyLine| match p {
        PolicyLine::Closed | PolicyLine::Unreadable => true,
        PolicyLine::Open | PolicyLine::InSection => false,
    };
    if policies.iter().any(closes) {
        Policy::Closed
    } else {
        Policy::Open
    }
}

fn read(bytes: &[u8]) -> Reading {
    let mut p = Parser::default();
    feed(&mut p, bytes, true);
    p.finish()
}

/// Reads `bytes` into `p`: the lines above the first section, then, when
/// `sections` is set, the sections from their heading on.
fn feed(p: &mut Parser, bytes: &[u8], sections: bool) {
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
            if !sections {
                // the bound cuts a line below the first section, which is
                // no policy line
                return;
            }
            each_line(p, parts.sections, first.saturating_sub(1), Parser::raw_line)
        }
        None => each_line(p, parts.top, 0, Parser::top_line),
    };
    // the lines end in an empty one, numbered as the line the bound cuts
    if p.top() && begins_policy(cut) {
        p.unreadable_policy(n, PolicyFault::Cut);
    }
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

/// Keeps the first [`ERRORS_MAX`] of findings sorted by line, and returns
/// how many it dropped.
fn keep_first<T>(findings: &mut Vec<T>) -> usize {
    let past = findings.len().saturating_sub(ERRORS_MAX);
    findings.truncate(ERRORS_MAX);
    past
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

#[cfg(test)]
mod tests;
