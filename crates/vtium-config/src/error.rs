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

/// A line whose shape is not one the file's syntax takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    /// A line outside every section, such as a key above the first
    /// heading.
    OutsideSection,
    /// A section heading that does not end in `]`.
    UnclosedHeading,
    /// `[head]` with no name or with more than one.
    HeadOneName,
    /// A line of a section that takes keys with no `=` in it.
    NotKeyValue,
    /// A rule that starts with neither kind.
    RuleKind,
    /// A rule with no `->`.
    RuleArrow,
    /// A rule with no target, or more than one, after `->`.
    RuleOneTarget,
    /// A rule with nothing between its kind and `->`.
    RuleMatch,
    /// `*` beside other tokens.
    StarAlone,
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
    Syntax(Syntax),
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

/// One error, with its line, or `None` for the file as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: Option<usize>,
    pub problem: Problem,
}

impl Error {
    /// The line the error names, `None` for the file as a whole.
    pub fn line(&self) -> Option<usize> {
        self.line
    }
}

impl fmt::Display for Error {
    /// `rule names undefined head "desc"`; [`at`] puts the file and the
    /// line before it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.problem)
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

    /// Adds errors found after the file was read, such as a rule's
    /// undefined head, and keeps the first [`ERRORS_MAX`] by line. The
    /// errors held were pushed in the file's order, so any dropped before
    /// lies after every one kept and stays dropped.
    pub(crate) fn merge(&mut self, late: Vec<Error>) {
        self.errors.extend(late);
        self.errors.sort_by_key(|e| e.line);
        if self.errors.len() > ERRORS_MAX {
            let past = self.errors.len().saturating_sub(ERRORS_MAX);
            self.more = self.more.saturating_add(past);
            self.errors.truncate(ERRORS_MAX);
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

impl Warning {
    /// The line the warning names.
    pub fn line(&self) -> usize {
        match self {
            Warning::ConsolesCapped { line } | Warning::Shadowed { line, .. } => *line,
        }
    }
}

impl Note {
    /// The line the note names.
    pub fn line(&self) -> usize {
        match self {
            Note::GuardOff { line, .. } | Note::RescueOff { line } | Note::WeakMatch { line } => {
                *line
            }
        }
    }
}

/// A finding located in a file: `PATH:LINE: WHAT`, or `PATH: WHAT` for the
/// file as a whole, as kmscon.conf's reports read and as `vtium
/// check-config`, the start's log, the notice and `vtiumctl` show the head
/// file's. The path is shown with every control and format character
/// escaped, so a newline in it never splits the line.
#[derive(Clone, Copy, Debug)]
pub struct At<'a, T> {
    path: &'a std::path::Path,
    line: Option<usize>,
    what: T,
}

/// `what`, located at `line` of the file at `path`.
pub fn at<T: fmt::Display>(path: &std::path::Path, line: Option<usize>, what: T) -> At<'_, T> {
    At { path, line, what }
}

impl<T: fmt::Display> fmt::Display for At<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.path.to_string_lossy().chars() {
            if matches!(c, '\\' | '"' | '\'') {
                f.write_char(c)?;
            } else {
                for e in c.escape_debug() {
                    f.write_char(e)?;
                }
            }
        }
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.what)
    }
}
