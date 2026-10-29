//! What a check of the head file finds: errors, which reject the file
//! whole, and warnings and notes, which let it apply (ADR-1401 D7).

use std::fmt::{self, Write as _};

use crate::bounds::{ERRORS_MAX, QUOTE_MAX};
use crate::identity::Kind;

/// Text quoted from a file: at most [`QUOTE_MAX`] characters, escaped
/// when shown, so a control character or an escape sequence in the file
/// never reaches a screen or a log as itself (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quoted {
    text: String,
    cut: bool,
}

impl Quoted {
    pub(crate) fn new(s: &str) -> Quoted {
        let mut chars = s.chars();
        let text = chars.by_ref().take(QUOTE_MAX).collect();
        Quoted {
            text,
            cut: chars.next().is_some(),
        }
    }

    /// A path, quoted whole: a path is bounded by the system, and a log
    /// line naming a file needs all of it.
    pub(crate) fn path(path: &std::path::Path) -> Quoted {
        Quoted {
            text: path.to_string_lossy().into_owned(),
            cut: false,
        }
    }

    /// The quoted text as the file holds it, unescaped.
    pub fn raw(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Quoted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        for c in self.text.chars() {
            for e in c.escape_debug() {
                f.write_char(e)?;
            }
        }
        if self.cut {
            f.write_str("...")?;
        }
        f.write_char('"')
    }
}

/// Why a file or a directory above it is not trusted (D10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Untrusted {
    /// Owned by another user than the one trusted.
    Owner { path: Quoted, uid: u32 },
    /// Writable by its group or by others.
    Writable { path: Quoted, mode: u32 },
}

impl fmt::Display for Untrusted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Untrusted::Owner { path, uid } => write!(f, "{path} is owned by uid {uid}, not root"),
            Untrusted::Writable { path, mode } => {
                write!(
                    f,
                    "{path} is writable by its group or others (mode {mode:o})"
                )
            }
        }
    }
}

/// One thing that makes the file invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The path is not a regular file once symlinks are followed.
    NotAFile,
    /// The file could not be read.
    Unreadable(std::io::ErrorKind),
    /// The file or a directory above it fails the ownership check.
    Untrusted(Untrusted),
    /// More than [`FILE_MAX`](crate::FILE_MAX) bytes.
    TooLarge,
    /// Not UTF-8 from this line.
    NotUtf8,
    /// A line longer than [`LINE_MAX`](crate::LINE_MAX) bytes.
    LineTooLong,
    /// A control character other than tab and newline.
    ControlCharacter(char),
    /// No `version = 1` as the first line that is not a comment.
    MissingVersion,
    /// A version vtium does not know.
    UnknownVersion(Quoted),
    /// A line that is not a section, a `key = value` or a rule.
    Syntax(&'static str),
    UnknownSection(Quoted),
    /// A second `[rules]` or `[guards]`.
    DuplicateSection(&'static str),
    /// A key a section does not take.
    UnknownKey(Quoted),
    /// A key a section already set.
    DuplicateKey(&'static str),
    /// A head's or a profile's name outside the name rule.
    BadName(Quoted),
    /// `focus`, `ignore` or `dark` as a head's name.
    ReservedName(Quoted),
    /// Two heads of one name.
    DuplicateHead(Quoted),
    /// A value its key cannot take.
    BadValue {
        key: &'static str,
        value: Quoted,
    },
    /// A number outside its key's range.
    OutOfRange {
        key: &'static str,
        value: Quoted,
    },
    /// A rule's match key D4 does not name.
    UnknownMatchKey(Quoted),
    /// A match token whose value its key cannot take.
    BadToken(Quoted),
    /// A rule naming a head no section declares.
    UndefinedHead(Quoted),
    /// A target the rule's kind cannot take: `focus` or `ignore` for a
    /// display, `dark` for an input.
    WrongKind {
        target: &'static str,
        kind: Kind,
    },
    /// More than [`HEADS_MAX`](crate::HEADS_MAX) heads.
    TooManyHeads,
    /// More than [`RULES_MAX`](crate::RULES_MAX) rules.
    TooManyRules,
    /// More than [`TOKENS_MAX`](crate::TOKENS_MAX) tokens in one match.
    TooManyTokens,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::NotAFile => f.write_str("not a regular file"),
            Problem::Unreadable(kind) => write!(f, "cannot be read: {kind}"),
            Problem::Untrusted(why) => write!(f, "not trusted: {why}"),
            Problem::TooLarge => write!(f, "longer than {} bytes", crate::FILE_MAX),
            Problem::NotUtf8 => f.write_str("not UTF-8"),
            Problem::LineTooLong => write!(f, "line longer than {} bytes", crate::LINE_MAX),
            Problem::ControlCharacter(c) => {
                write!(f, "control character {} in the line", c.escape_unicode())
            }
            Problem::MissingVersion => f.write_str("the first line is not `version = 1`"),
            Problem::UnknownVersion(v) => write!(f, "unknown version {v}"),
            Problem::Syntax(what) => f.write_str(what),
            Problem::UnknownSection(s) => write!(f, "unknown section {s}"),
            Problem::DuplicateSection(s) => write!(f, "a second [{s}] section"),
            Problem::UnknownKey(k) => write!(f, "unknown key {k}"),
            Problem::DuplicateKey(k) => write!(f, "`{k}` set twice"),
            Problem::BadName(n) => write!(
                f,
                "name {n} is not 1 to {} of A-Z a-z 0-9 . _ + -",
                crate::NAME_MAX
            ),
            Problem::ReservedName(n) => write!(f, "head name {n} is reserved"),
            Problem::DuplicateHead(n) => write!(f, "a second head named {n}"),
            Problem::BadValue { key, value } => write!(f, "`{key}` cannot take {value}"),
            Problem::OutOfRange { key, value } => write!(f, "`{key}` {value} is out of range"),
            Problem::UnknownMatchKey(t) => write!(f, "unknown match key in {t}"),
            Problem::BadToken(t) => write!(f, "match token {t} has a value its key cannot take"),
            Problem::UndefinedHead(n) => write!(f, "rule names undefined head {n}"),
            Problem::WrongKind { target, kind } => {
                write!(
                    f,
                    "`{target}` is no target for {} rule",
                    match kind {
                        Kind::Display => "a display",
                        Kind::Input => "an input",
                    }
                )
            }
            Problem::TooManyHeads => write!(f, "more than {} heads", crate::HEADS_MAX),
            Problem::TooManyRules => write!(f, "more than {} rules", crate::RULES_MAX),
            Problem::TooManyTokens => {
                write!(f, "more than {} tokens in one match", crate::TOKENS_MAX)
            }
        }
    }
}

/// One error, with its line, or `None` for the file as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: Option<usize>,
    pub problem: Problem,
}

impl fmt::Display for Error {
    /// `line 6: rule names undefined head "desc"`; the caller puts the
    /// file's name before it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.problem),
            None => write!(f, "{}", self.problem),
        }
    }
}

/// A head file rejected whole: its errors in the file's order, at most
/// [`ERRORS_MAX`], and how many more there were.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rejected {
    pub errors: Vec<Error>,
    pub more: usize,
}

impl Rejected {
    pub(crate) fn push(&mut self, line: Option<usize>, problem: Problem) {
        if self.errors.len() < ERRORS_MAX {
            self.errors.push(Error { line, problem });
        } else {
            self.more = self.more.saturating_add(1);
        }
    }

    pub(crate) fn one(line: Option<usize>, problem: Problem) -> Rejected {
        let mut r = Rejected::default();
        r.push(line, problem);
        r
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

/// A finding about a valid file that `vtium check-config` counts against
/// it (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// A head asked for more than [`CONSOLES_MAX`](crate::CONSOLES_MAX)
    /// consoles and gets the cap.
    ConsolesCapped { line: usize },
    /// No device can reach the rule: the earlier rule at `by` takes every
    /// device it matches.
    Shadowed { line: usize, by: usize },
}

/// A finding `vtium check-config` reports without counting it against the
/// file (D4, D9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A guard of `[guards]` is off.
    GuardOff { line: usize, guard: &'static str },
    /// A head's `rescue = off`.
    RescueOff { line: usize },
    /// A rule sends input to a head by `id` or `name` alone (probe 4).
    WeakMatch { line: usize },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::ConsolesCapped { line } => write!(
                f,
                "line {line}: more consoles than the cap; the head gets {}",
                crate::CONSOLES_MAX
            ),
            Warning::Shadowed { line, by } => {
                write!(
                    f,
                    "line {line}: the rule on line {by} takes every device this one matches"
                )
            }
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::GuardOff { line, guard } => write!(f, "line {line}: the {guard} guard is off"),
            Note::RescueOff { line } => {
                write!(
                    f,
                    "line {line}: the rescue key leaves this head's keyboards"
                )
            }
            Note::WeakMatch { line } => write!(
                f,
                "line {line}: input reaches a head by `id` or `name` alone, which any device can claim"
            ),
        }
    }
}
