//! What a check of the head file finds: errors, each of which drops what
//! [`scope`](crate::scope) says it takes, and warnings and notes, which
//! drop nothing (ADR-1401 D7).

use std::fmt::{self, Write as _};

use crate::bounds::{ERRORS_MAX, QUOTE_MAX};
use crate::identity::Kind;
use crate::keys::{Level, Levels};
use crate::outcome::{Read, Scope, Trusted, Within};

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
    pub fn path(path: &std::path::Path) -> Quoted {
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

impl Untrusted {
    /// The command that mends the path that fails (ADR-1403 D3):
    /// `chown root:root PATH` for one another user owns, and
    /// `chmod go-w PATH` for one its group or others can write.
    pub fn mend(&self) -> Mend<'_> {
        Mend(self)
    }

    /// Why vtium does not read vtium.conf, with the command that mends it,
    /// as the start's log and `vtium check-config` say it (ADR-1403 D3).
    pub fn refusal(&self) -> Refusal<'_> {
        Refusal(self)
    }
}

/// The command that mends an [`Untrusted`] path, which
/// [`Untrusted::mend`] gives.
#[derive(Clone, Copy, Debug)]
pub struct Mend<'a>(pub(crate) &'a Untrusted);

/// Why vtium does not read vtium.conf, and the command that mends it,
/// which [`Untrusted::refusal`] gives.
#[derive(Clone, Copy, Debug)]
pub struct Refusal<'a>(pub(crate) &'a Untrusted);

/// A line whose shape is not one the file's syntax takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    /// A section heading that does not end in `]`.
    UnclosedHeading,
    /// `[head]` with no name or with more than one.
    HeadOneName,
    /// `[look]` with no name or with more than one.
    LookOneName,
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

/// A name of the head file's first cut, which a later word replaced
/// (ADR-1401 D3): each is an error whose message names the new word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renamed {
    /// The target `focus`, now `roaming`.
    Focus,
    /// The target `dark`, now `off`.
    Dark,
    /// `focus` as a head's name, reserved as the first cut's target.
    FocusHead,
    /// `dark` as a head's name, reserved as the first cut's target.
    DarkHead,
    /// The head key `on-loss`, now `when-display-lost`.
    OnLoss,
    /// `when-display-lost`'s value `nohup`, now `keep`.
    Nohup,
    /// The head key `rescue`, which `isolated` replaced.
    Rescue,
    /// The `[guards]` section, which the policy replaced.
    Guards,
    /// A head's `profile` in vtium.conf, which `look` replaced (ADR-1403
    /// D1).
    Profile,
    /// A `[profile NAME]` section in vtium.conf, which `[look NAME]`
    /// replaced (ADR-1403 D1).
    ProfileSection,
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
    /// A line that is not UTF-8.
    NotUtf8,
    /// A line longer than [`LINE_MAX`](crate::LINE_MAX) bytes.
    LineTooLong,
    /// A control character other than tab, and other than a carriage
    /// return ending a line.
    ControlCharacter(char),
    /// No `version` line above the first section.
    MissingVersion,
    /// A version vtium does not know.
    UnknownVersion(Quoted),
    /// A line that is not a section, a `key = value` or a rule.
    Syntax(Syntax),
    /// A second `[rules]`.
    DuplicateSection(&'static str),
    /// A key a section does not take.
    UnknownKey(Quoted),
    /// A key a section already set.
    DuplicateKey(&'static str),
    /// A head's or a profile's name outside the name rule.
    BadName(Quoted),
    /// `roaming`, `ignore` or `off` as a head's name.
    ReservedName(Quoted),
    /// Two heads of one name.
    DuplicateHead(Quoted),
    /// A value its key cannot take.
    BadValue { key: &'static str, value: Quoted },
    /// A number outside its key's range.
    OutOfRange { key: &'static str, value: Quoted },
    /// A rule's match key D4 does not name.
    UnknownMatchKey(Quoted),
    /// A match token whose value its key cannot take.
    BadToken(Quoted),
    /// A rule naming a head no section declares.
    UndefinedHead(Quoted),
    /// A target the rule's kind cannot take: `roaming` or `ignore` for a
    /// display, `off` for an input.
    WrongKind { target: &'static str, kind: Kind },
    /// More than [`HEADS_MAX`](crate::HEADS_MAX) heads.
    TooManyHeads,
    /// More than [`RULES_MAX`](crate::RULES_MAX) rules.
    TooManyRules,
    /// More than [`TOKENS_MAX`](crate::TOKENS_MAX) tokens in one match.
    TooManyTokens,
    /// A name of the first cut.
    Renamed(Renamed),
    /// A `policy` line inside a section, which counts as no policy
    /// (D3).
    PolicyInSection,
    /// A line above the first section that begins with `policy` and that
    /// vtium cannot read, which makes the file closed and broken (D7).
    UnreadablePolicy(PolicyFault),
    /// A `version` line inside a section, where it is the top line
    /// misplaced.
    VersionInSection,
    /// A section of vtium.conf that is none of `[look NAME]`, `[head NAME]`
    /// and `[rules]` (ADR-1403 D4).
    NotASection(Quoted),
    /// A look's name outside the name rule.
    BadLookName(Quoted),
    /// A second look of one name; the first stays.
    DuplicateLook(Quoted),
    /// More than [`LOOKS_MAX`](crate::LOOKS_MAX) looks.
    TooManyLooks,
    /// A key of the key table that a section of this kind cannot set, which
    /// the key table lets `levels` set (ADR-1403 D5).
    NotHere {
        key: &'static str,
        /// The kind of section the line sits in.
        section: Level,
        /// Where the key may be set; empty for a key the command line
        /// alone sets.
        levels: Levels,
    },
    /// A value longer than [`TEXT_MAX`](crate::keys::TEXT_MAX) bytes.
    TooLong(&'static str),
}

/// Why vtium cannot read a policy line, a line above the first section
/// that begins with `policy` in any case (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyFault {
    /// A byte that is not UTF-8.
    NotUtf8,
    /// A control character other than tab, and other than a carriage
    /// return ending the line.
    ControlCharacter(char),
    /// More than [`LINE_MAX`](crate::LINE_MAX) bytes.
    LineTooLong,
    /// The line runs past [`FILE_MAX`](crate::FILE_MAX), where vtium stops
    /// reading the file.
    Cut,
    /// No `=` between a key and a value, as in `policy` alone.
    NotKeyValue,
    /// A key other than `policy` exactly, as in `Policy` or `policies`.
    Key(Quoted),
    /// A value other than `open` or `closed`.
    Value(Quoted),
}

/// What one error of a read file drops, as `vtium check-config` reports
/// it (D7, "The rest"): its [`Scope`], with a line told apart as a rule
/// or another line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drops {
    /// Everything but the file's `policy` lines.
    File,
    /// One head, with every rule naming it.
    Head,
    /// One section, with its lines.
    Section,
    /// One rule; a device walks on to the next rule.
    Rule,
    /// One line that is not a rule, such as a top-level key.
    Line,
    /// The whole file, `policy` lines included, under the closed policy:
    /// a policy line vtium cannot read makes the file closed and broken.
    Closed,
}

impl Drops {
    /// Every scope an error can drop, so a reader of `vtium check-config`'s
    /// report can tell the line that says what an error drops from the
    /// error itself.
    pub const ALL: [Drops; 6] = [
        Drops::File,
        Drops::Head,
        Drops::Section,
        Drops::Rule,
        Drops::Line,
        Drops::Closed,
    ];

    /// What an error of `scope` drops on a line `within` the file.
    pub fn of(scope: Scope, within: Within) -> Drops {
        match (scope, within) {
            (Scope::File, _) => Drops::File,
            (Scope::Head, _) => Drops::Head,
            (Scope::Section, _) => Drops::Section,
            (Scope::Line, Within::Rules) => Drops::Rule,
            (Scope::Line, Within::Top | Within::Header | Within::Head | Within::Look) => {
                Drops::Line
            }
        }
    }
}

/// One error, with its line, or `None` for the file as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: Option<usize>,
    pub problem: Problem,
    /// What the error drops of a file that is read; `None` for one that
    /// leaves the file unread, such as the ownership check's.
    pub drops: Option<Drops>,
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

/// Whether `error` is a policy line vtium cannot read, the error that
/// makes the file closed and broken (D7), which a list of errors always
/// keeps so that a check and the panic screen name that line.
fn closes(error: &Error) -> bool {
    matches!(error.problem, Problem::UnreadablePolicy(_))
}

/// A head file's errors in the file's order, at most [`ERRORS_MAX`], and
/// how many more there were.
/// The first policy line vtium cannot read is always among them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rejected {
    pub errors: Vec<Error>,
    pub more: usize,
}

impl Rejected {
    pub(crate) fn push(&mut self, line: Option<usize>, problem: Problem) {
        self.push_dropping(line, problem, None);
    }

    /// Pushes an error of a read file, which drops `drops`.
    pub(crate) fn push_dropping(
        &mut self,
        line: Option<usize>,
        problem: Problem,
        drops: Option<Drops>,
    ) {
        let error = Error {
            line,
            problem,
            drops,
        };
        if self.errors.len() < ERRORS_MAX {
            self.errors.push(error);
        } else {
            // the first policy line vtium cannot read takes the last place
            if closes(&error) && !self.errors.iter().any(closes) {
                self.errors.pop();
                self.errors.push(error);
            }
            self.more = self.more.saturating_add(1);
        }
    }

    /// Adds errors found after the file was read, such as a rule's
    /// undefined head, and keeps the first [`ERRORS_MAX`] by line, the
    /// first policy line vtium cannot read among them. The errors held
    /// were pushed in the file's order, so any dropped before lies after
    /// every one kept and stays dropped.
    pub(crate) fn merge(&mut self, late: Vec<Error>) {
        self.errors.extend(late);
        self.errors.sort_by_key(|e| e.line);
        if self.errors.len() > ERRORS_MAX {
            let shown = self.errors.iter().take(ERRORS_MAX).any(closes);
            let held = match self.errors.iter().position(closes) {
                Some(at) if !shown => Some(self.errors.remove(at)),
                Some(_) | None => None,
            };
            let keep = ERRORS_MAX.saturating_sub(usize::from(held.is_some()));
            let past = self.errors.len().saturating_sub(keep);
            self.more = self.more.saturating_add(past);
            self.errors.truncate(keep);
            self.errors.extend(held);
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
    /// The head whose `consoles` is on `line` asks for more consoles than
    /// `cap`, vtium.conf's `head-consoles-max`, and gets the cap (D10).
    PastHeadCap { line: usize, head: Quoted, cap: u32 },
    /// The heads together ask for more consoles than `total`, vtium.conf's
    /// `consoles-max`, so the head whose `consoles` is on `line` gets
    /// `gets` (D10).
    PastTotal {
        line: usize,
        head: Quoted,
        gets: u32,
        total: u32,
    },
    /// The look's `font-name`, on `line`, names a frit that is not in
    /// `dir` (D7; ADR-1403 D5).
    MissingFrit {
        line: usize,
        look: Quoted,
        frit: Quoted,
        dir: Quoted,
    },
    /// The head's `look`, on `line`, names a look vtium.conf has no
    /// `[look NAME]` section for, so the head draws with the default look
    /// (ADR-1403 D5).
    NoLook {
        line: usize,
        head: Quoted,
        look: Quoted,
    },
}

/// A finding `vtium check-config` reports without counting it against the
/// file (D4, D9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A rule sends input to a head by `id` or `name` alone (probe 4).
    WeakMatch { line: usize },
}

impl Warning {
    /// The line the warning names.
    pub fn line(&self) -> usize {
        match self {
            Warning::ConsolesCapped { line }
            | Warning::Shadowed { line, .. }
            | Warning::PastHeadCap { line, .. }
            | Warning::PastTotal { line, .. }
            | Warning::MissingFrit { line, .. }
            | Warning::NoLook { line, .. } => *line,
        }
    }
}

/// What a device present that no rule places gets (D9, "What fell to the
/// default").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// A display: it stays dark.
    Dark,
    /// An input under open: it roams into the active head.
    Roams,
    /// An input under open that roams while no head can be the active
    /// head: it reaches none.
    RoamsNowhere,
    /// An input under closed: it is ignored.
    Ignored,
    /// An input of a partly broken open file that walks the shipped rules'
    /// tail and roams into the active head.
    TailRoams,
    /// An input of a partly broken open file that walks the shipped rules'
    /// tail and roams while no head can be the active head: it reaches
    /// none.
    TailRoamsNowhere,
    /// An input of a partly broken open file that walks the shipped rules'
    /// tail and is ignored.
    TailIgnored,
}

/// A finding `vtium check-config` notes about what runs, outside the exit
/// status (D3, D5, D9). [`findings::remarks`](crate::remarks) gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remark {
    /// No rule names the head declared on the line it names.
    Unnamed { head: Quoted },
    /// No rule names the first head of an open file, which the display
    /// guard gives the boot display while no head has a display.
    UnnamedLitByGuard { head: Quoted },
    /// Every head is isolated, so no keyboard can switch the VT and no head
    /// can be the active head.
    EveryHeadIsolated,
    /// No head that can be the active head has a display present.
    NoActiveHead,
    /// Under closed, no keyboard present reaches any head: a lockout.
    ClosedLockout,
    /// A device present no rule places, as it reports itself.
    Fell { device: Quoted, fate: Fate },
}

impl Note {
    /// The line the note names.
    pub fn line(&self) -> usize {
        match self {
            Note::WeakMatch { line } => *line,
        }
    }
}

/// What a start does with a head file it found as `read`, the outcome
/// [`resolve`](crate::resolve) gives, as `vtium check-config` reports it
/// after the file's errors (D7's outcomes table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtStart(pub Read);

/// The policy a trusted file states and what it sets, as `vtium
/// check-config` reports it after [`AtStart`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stating(pub Trusted);

/// A path as a message names it, with every control and format character
/// escaped, so a newline in it never splits the line, and a backslash
/// escaped too, so an escape never reads as a character the path holds.
#[derive(Clone, Copy, Debug)]
pub struct Shown<'a>(pub &'a std::path::Path);

impl fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.to_string_lossy().chars() {
            if matches!(c, '"' | '\'') {
                f.write_char(c)?;
            } else {
                for e in c.escape_debug() {
                    f.write_char(e)?;
                }
            }
        }
        Ok(())
    }
}

/// A finding located in a file: `PATH:LINE: WHAT`, or `PATH: WHAT` for the
/// file as a whole, as vtium.conf's reports read and as `vtium
/// check-config`, the start's log, the notice and vtium-config show the head
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
        write!(f, "{}", Shown(self.path))?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.what)
    }
}
