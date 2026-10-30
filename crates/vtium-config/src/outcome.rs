//! What vtium runs for each state its head file can be in (ADR-1401 D7).
//!
//! Failure is resolved at three levels, each asked only once the one before it has passed: whether the file can be trusted ([`Read`]), whether it states a policy ([`Stated`]), and how much of the rest is good ([`Rest`]). A trusted file's policy and content exist only inside [`Read::Trusted`], so nothing can consult an untrusted file's content. [`resolve`] maps every path to one [`Outcome`], by exhaustive matches with no wildcard arm, so a new variant fails to compile until it has an answer. The module reads no file and holds no state: the loader classifies what it found with [`scope`] and [`Trusted::of`] and asks [`resolve`], and a reload asks [`Outcome::reload`].
//!
//! A readable closed policy is sticky. Once a trusted file states a readable `policy = closed`, nothing opens anything up: no other line of the file, no broken line in it, and no event while vtium runs. A reload under it applies only a valid closed file, so going from closed to open takes a vtium restart.
//!
//! `outcomes.tsv`, beside the crate's manifest, lists every path with its outcome; `outcome_table_matches_resolve` fails when it differs from [`resolve`], and `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config outcome_table` rewrites it.
//!
//! Only a trusted file carries what it states:
//!
//! ```
//! use vtium_config::{Read, Rest, Stated, Trusted};
//! let read = Read::Trusted(Trusted { stated: Stated::Closed, rest: Rest::Valid });
//! assert!(matches!(read, Read::Trusted(_)));
//! ```
//!
//! and an untrusted one has nowhere to carry it:
//!
//! ```compile_fail
//! use vtium_config::{Read, Rest, Stated, Trusted};
//! let read = Read::Untrusted(Trusted { stated: Stated::Closed, rest: Rest::Valid });
//! ```
//!
//! A problem that leaves the file unread is the first level's verdict, never a scope of a trusted file, so an untrusted file cannot reach [`Trusted::of`]:
//!
//! ```compile_fail,E0599
//! use vtium_config::{Kept, Scope, Trusted};
//! let trusted = Trusted::of(&[], &[Scope::Trust], Kept::Heads);
//! ```

use crate::error::Problem;

/// A fieldless enum with every value in [`ALL`](Self::ALL) and its name in the outcome table, so the table's enumeration cannot miss a value.
macro_rules! closed {
    ($(#[$meta:meta])* pub enum $name:ident { $($(#[$vmeta:meta])* $value:ident = $text:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name { $($(#[$vmeta])* $value,)+ }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$value,)+];

            /// The value's name in the outcome table.
            pub fn name(self) -> &'static str {
                match self { $($name::$value => $text,)+ }
            }
        }
    };
}

/// [`Read`] with every verdict that reads nothing in [`UNREAD`](Read::UNREAD), so the paths' enumeration cannot miss one; a verdict carrying anything but [`Read::Trusted`]'s levels does not match the macro.
macro_rules! read {
    ($(#[$meta:meta])* pub enum Read { $(#[$tmeta:meta])* Trusted(Trusted), $($(#[$vmeta:meta])* $unread:ident,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Read { $(#[$tmeta])* Trusted(Trusted), $($(#[$vmeta])* $unread,)+ }

        impl Read {
            /// Every verdict that reads nothing, in declaration order.
            pub const UNREAD: &'static [Read] = &[$(Read::$unread,)+];
        }
    };
}

read! {
    /// The first level: whether the file can be trusted (ADR-1401 D10).
    pub enum Read {
        /// The file passes the ownership check and its text is read.
        Trusted(Trusted),
        /// No file at the path: the shipped rules run, silently.
        Missing,
        /// The file fails the ownership check, not root's or writable by anyone else, or a directory above it does. It is never read.
        Untrusted,
        /// Something is at the path that vtium cannot open and read as a regular file: a directory, a FIFO, an I/O error. It is never read.
        Inaccessible,
    }
}

/// The second and third levels of a trusted file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Trusted {
    pub stated: Stated,
    pub rest: Rest,
}

closed! {
    /// The second level: the policy the file states with its `policy` line, anywhere among the top-level keys. A readable `policy = closed` wins over every other `policy` line.
    pub enum Stated {
        /// No `policy` line: open.
        Unset = "unset",
        Open = "open",
        Closed = "closed",
        /// No readable `policy = closed`, and a `policy` line vtium cannot read: an unknown value, a line misplaced inside a section, or a second `policy` line. It counts as no policy, and the file is at least partly broken.
        Unreadable = "unreadable",
    }
}

closed! {
    /// The third level: the file apart from its `policy` lines.
    pub enum Rest {
        /// Valid, declaring at least one head.
        Valid = "valid",
        /// Valid, declaring rules but no head.
        Headless = "headless",
        /// Valid, declaring nothing.
        Empty = "empty",
        /// Errors that leave at least one usable head.
        Partly = "partly",
        /// Errors that leave no usable head.
        Broken = "broken",
    }
}

closed! {
    /// A `policy` line as the parser reads it, wherever it sits.
    pub enum PolicyLine {
        Open = "open",
        Closed = "closed",
        /// A value but `open` or `closed`.
        Unknown = "unknown",
        /// A `policy` line inside a section.
        InSection = "in-section",
    }
}

closed! {
    /// What survives of a trusted file once its errors have dropped what they take (open cell ?4).
    pub enum Kept {
        Nothing = "nothing",
        /// Rules, but no head.
        Headless = "headless",
        Heads = "heads",
    }
}

closed! {
    /// Where in the file an error's line sits.
    pub enum Within {
        /// Among the top-level keys, before the first section.
        Top = "top",
        /// A section's header.
        Header = "header",
        /// Inside a `[head NAME]` section.
        Head = "head",
        Rules = "rules",
    }
}

closed! {
    /// What one error takes down (open cell ?4).
    pub enum Scope {
        /// The file's structure or format cannot be trusted: nothing of it applies, but a readable `policy` line still states its policy, since a trusted file always fails to its policy.
        File = "file",
        /// One head, with every rule naming it, each of which falls through to the next rule.
        Head = "head",
        /// One section that is unknown, repeated or headed by a line that does not parse, with its lines.
        Section = "section",
        /// One line: a rule, which falls through to the next one, or a top-level key.
        Line = "line",
    }
}

closed! {
    /// Which rules run.
    pub enum Runs {
        /// The file's own rules, as written, declaring at least one head.
        File = "file",
        /// The file, which declares no head, so no head of its own runs: under closed every display stays dark, loaded but off (ADR-1401 D7), and under open the display guard lights the default head.
        NoHead = "no-head",
        /// What survives of the file.
        Survivors = "survivors",
        /// The shipped rules, [`Config::builtin`](crate::Config::builtin).
        Builtin = "builtin",
        /// None: no input is delivered.
        Off = "off",
    }
}

closed! {
    /// The effective policy (ADR-1401 D3): what an input no rule places gets, whether a head the file leaves unset is isolated, and whether the anti-lockout guard and the rescue key act.
    pub enum Policy {
        /// An input no rule places roams, no unset head is isolated, and the anti-lockout guard and the rescue key are on.
        Open = "open",
        /// An input no rule places is ignored, every unset head is isolated, and the anti-lockout guard and the rescue key are off.
        Closed = "closed",
    }
}

closed! {
    /// How a run holds `isolated`, and with it the rescue.
    pub enum Rescue {
        /// As the file writes it, else as the policy leaves it.
        AsSet = "as-set",
        /// A partly broken open run: every surviving head acts as not isolated, so the rescue key reaches its keyboards, its VT keys switch the VT, and it may be the active head, as the shipped rules isolate no head.
        Forced = "forced",
        /// A partly broken closed run: isolation as written and the rescue key off, as the policy says, since a readable closed policy is sticky, so a keyboard may land nowhere.
        Sticky = "sticky",
    }
}

closed! {
    /// The console line vtium shows on the lit display; the details go to the log.
    pub enum Line {
        Skipped = "skipped",
        Partly = "partly",
        ClosedOff = "closed-off",
        /// A run that declares a head has no display for any of them, under a policy whose display guard lights no head.
        NoDisplay = "no-display",
    }
}

closed! {
    /// What an input no running rule places gets.
    pub enum Unplaced {
        /// It roams, as an open file's policy gives it.
        Roaming = "roaming",
        /// It walks the shipped rules' tail: a keyboard, pointer or key device roams, and every other input is ignored, so a partly broken open run is never more open than the shipped rules.
        Shipped = "shipped",
        Ignore = "ignore",
    }
}

closed! {
    /// Which displays may light.
    pub enum Lit {
        /// The displays the running rules place, and the boot display when the display guard lights it.
        Rules = "rules",
        /// The boot display alone, for the notice; no session is reached through it.
        Notice = "notice",
    }
}

closed! {
    /// What the display guard, always on, lights when the running rules light no display (ADR-1401 D9).
    pub enum Fallback {
        /// The boot display, for the first head the running text declares, else for the default head, the compiled-in configuration's `[head console]`.
        FirstHead = "first-head",
        /// Never a head on a display its rules do not claim: the boot display for the notice alone, [`Outcome::dark_notice`], when the run declares a head or has failed, and nothing for a valid closed file with no head, which is dark and silent, loaded but off.
        NoticeOnly = "notice-only",
    }
}

closed! {
    /// What vtium does when no display appears at start (ADR-1401 D8).
    pub enum DisplayWait {
        /// `display-wait` as kmscon.conf sets it: with no display by its end, vtium exits 1 and the unit's `OnFailure=` starts a getty on the VT.
        AsSet = "as-set",
        /// vtium holds the VT and waits for a display however long it takes, as `display-wait = forever` does, so no getty starts on the VT: a closed file never gives up its VT for lack of a display.
        Forever = "forever",
    }
}

closed! {
    /// A switch of a run. The policy alone sets the anti-lockout guard and the rescue key (ADR-1401 D9); a head's `isolated` is the one switch the file writes (D5).
    pub enum SwitchKey {
        AntiLockout = "anti-lockout",
        RescueKey = "rescue-key",
        /// An isolated head sends its keyboards' VT keys to its session, keeps them from the rescue key, and is never the active head.
        Isolated = "isolated",
    }
}

/// How a switch takes its value in a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Switch {
    /// On, whatever the file writes.
    On,
    /// Off, whatever the file writes.
    Off,
    /// As the running text writes it, else `unset`.
    Written { unset: bool },
}

impl Switch {
    /// The switch's value where the running text writes `written`, `None` for unset.
    pub fn of(self, written: Option<bool>) -> bool {
        match self {
            Switch::On => true,
            Switch::Off => false,
            Switch::Written { unset } => written.unwrap_or(unset),
        }
    }

    /// The switch's name in the outcome table.
    pub fn name(self) -> &'static str {
        match self {
            Switch::On => "on",
            Switch::Off => "off",
            Switch::Written { unset: true } => "set|on",
            Switch::Written { unset: false } => "set|off",
        }
    }
}

impl Line {
    /// The line as the console shows it.
    pub fn text(self) -> &'static str {
        match self {
            Line::Skipped => "invalid config skipped, using builtin",
            Line::Partly => "config partly invalid, running what's valid",
            Line::ClosedOff => "vtium: config invalid, policy closed: input off",
            // nothing about input: each head's own keyboards still deliver to it while it is unlit
            Line::NoDisplay => "vtium: no display for any head",
        }
    }
}

impl Policy {
    /// The value a switch takes under this policy where the file writes none: the guards on and no head isolated under open, the reverse under closed.
    pub fn unset(self, key: SwitchKey) -> bool {
        match (self, key) {
            (Policy::Open, SwitchKey::AntiLockout | SwitchKey::RescueKey)
            | (Policy::Closed, SwitchKey::Isolated) => true,
            (Policy::Open, SwitchKey::Isolated)
            | (Policy::Closed, SwitchKey::AntiLockout | SwitchKey::RescueKey) => false,
        }
    }

    /// What vtium does when no display appears at start.
    pub fn display_wait(self) -> DisplayWait {
        match self {
            Policy::Open => DisplayWait::AsSet,
            Policy::Closed => DisplayWait::Forever,
        }
    }

    /// What the display guard may light when the running rules light no display.
    pub fn fallback(self) -> Fallback {
        match self {
            Policy::Open => Fallback::FirstHead,
            Policy::Closed => Fallback::NoticeOnly,
        }
    }
}

/// What vtium does with its head file on one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Outcome {
    pub runs: Runs,
    pub policy: Policy,
    pub rescue: Rescue,
    /// The console line, `None` for a silent start.
    pub line: Option<Line>,
    pub lit: Lit,
    pub fallback: Fallback,
    pub unplaced: Unplaced,
}

impl Outcome {
    /// How `key` takes its value in this run.
    pub fn switch(&self, key: SwitchKey) -> Switch {
        let fixed = |on: bool| if on { Switch::On } else { Switch::Off };
        match (self.runs, key) {
            // the shipped head is not isolated, and the guards are on
            (
                Runs::Builtin,
                SwitchKey::AntiLockout | SwitchKey::RescueKey | SwitchKey::Isolated,
            ) => fixed(Policy::Open.unset(key)),
            // nothing is delivered
            (Runs::Off, SwitchKey::AntiLockout | SwitchKey::RescueKey | SwitchKey::Isolated) => {
                fixed(Policy::Closed.unset(key))
            }
            (
                Runs::File | Runs::NoHead | Runs::Survivors,
                SwitchKey::AntiLockout | SwitchKey::RescueKey,
            ) => fixed(self.policy.unset(key)),
            (Runs::File | Runs::NoHead | Runs::Survivors, SwitchKey::Isolated) => match self.rescue
            {
                Rescue::Forced => Switch::Off,
                Rescue::AsSet | Rescue::Sticky => Switch::Written {
                    unset: self.policy.unset(key),
                },
            },
        }
    }

    /// What vtium does when no display appears at start: the policy decides, the shipped rules being open.
    pub fn display_wait(&self) -> DisplayWait {
        self.policy.display_wait()
    }

    /// Whether the running text declares a head: the shipped rules declare the default head, and a file that declares none or a run of nothing declares none.
    pub fn declares_head(&self) -> bool {
        match self.runs {
            Runs::File | Runs::Survivors | Runs::Builtin => true,
            Runs::NoHead | Runs::Off => false,
        }
    }

    /// The notice the display guard shows on the boot display when no head the running text declares has a display: the run's own line, else `vtium: no display for any head`, the one notice a valid file shows, and none for a valid closed file that declares no head, which is dark and silent. Under open the guard lights a head instead.
    pub fn dark_notice(&self) -> Option<Line> {
        match (self.fallback, self.line) {
            (Fallback::FirstHead, _) => None,
            (Fallback::NoticeOnly, Some(line)) => Some(line),
            (Fallback::NoticeOnly, None) => self.declares_head().then_some(Line::NoDisplay),
        }
    }

    /// What runs once a reload reads `read` while this outcome runs (vtium/vtium#86): a valid trusted file the running policy accepts runs as it would at start, the owner's deliberate change, and anything else leaves whatever runs as it is. A running open policy accepts any valid file; a running closed one accepts only a valid closed file, so going from closed to open takes a vtium restart, which reads the file afresh.
    pub fn reload(&self, read: &Read) -> Outcome {
        let valid = match read {
            Read::Missing | Read::Untrusted | Read::Inaccessible => None,
            Read::Trusted(t) => match (t.stated, t.rest) {
                (Stated::Unset | Stated::Open, Rest::Valid | Rest::Headless | Rest::Empty) => {
                    Some(Policy::Open)
                }
                (Stated::Closed, Rest::Valid | Rest::Headless | Rest::Empty) => {
                    Some(Policy::Closed)
                }
                (Stated::Unreadable, Rest::Valid | Rest::Headless | Rest::Empty)
                | (
                    Stated::Unset | Stated::Open | Stated::Closed | Stated::Unreadable,
                    Rest::Partly | Rest::Broken,
                ) => None,
            },
        };
        let accepted = match (self.policy, valid) {
            (Policy::Open, Some(Policy::Open | Policy::Closed))
            | (Policy::Closed, Some(Policy::Closed)) => true,
            (Policy::Closed, Some(Policy::Open)) | (Policy::Open | Policy::Closed, None) => false,
        };
        if accepted { resolve(read) } else { *self }
    }
}

/// The content level once the `policy` line's own error is counted.
#[derive(Clone, Copy)]
enum Class {
    Valid,
    Headless,
    Empty,
    Partly,
    Broken,
}

fn builtin(line: Option<Line>) -> Outcome {
    Outcome {
        runs: Runs::Builtin,
        policy: Policy::Open,
        rescue: Rescue::AsSet,
        line,
        lit: Lit::Rules,
        fallback: Fallback::FirstHead,
        unplaced: Unplaced::Shipped,
    }
}

fn file(policy: Policy) -> Outcome {
    Outcome {
        runs: Runs::File,
        policy,
        fallback: policy.fallback(),
        unplaced: match policy {
            Policy::Open => Unplaced::Roaming,
            Policy::Closed => Unplaced::Ignore,
        },
        ..builtin(None)
    }
}

/// A partly broken run: what survives, under `policy`. Under open nothing that would prevent the rescue is honoured, so no head is isolated, and the shipped rules' tail places what no surviving rule does; under closed nothing opens, since a readable closed policy is sticky.
fn survivors(policy: Policy) -> Outcome {
    let (rescue, unplaced) = match policy {
        Policy::Open => (Rescue::Forced, Unplaced::Shipped),
        Policy::Closed => (Rescue::Sticky, Unplaced::Ignore),
    };
    Outcome {
        runs: Runs::Survivors,
        policy,
        rescue,
        line: Some(Line::Partly),
        lit: Lit::Rules,
        fallback: policy.fallback(),
        unplaced,
    }
}

/// What vtium does with its head file on the path `read` (ADR-1401 D7).
pub fn resolve(read: &Read) -> Outcome {
    let trusted = match read {
        Read::Missing => return builtin(None),
        Read::Untrusted | Read::Inaccessible => return builtin(Some(Line::Skipped)),
        Read::Trusted(trusted) => trusted,
    };
    let (policy, readable) = match trusted.stated {
        Stated::Unset | Stated::Open => (Policy::Open, true),
        Stated::Unreadable => (Policy::Open, false),
        Stated::Closed => (Policy::Closed, true),
    };
    // the `policy` line vtium cannot read is an error of its own
    let class = match (trusted.rest, readable) {
        (Rest::Valid, true) => Class::Valid,
        (Rest::Headless, true) => Class::Headless,
        (Rest::Empty, true) => Class::Empty,
        (Rest::Valid, false) | (Rest::Partly, true | false) => Class::Partly,
        (Rest::Headless | Rest::Empty, false) | (Rest::Broken, true | false) => Class::Broken,
    };
    match (class, policy) {
        (Class::Valid, Policy::Open) => file(Policy::Open),
        (Class::Valid, Policy::Closed) => file(Policy::Closed),
        (Class::Headless, Policy::Open) => Outcome {
            runs: Runs::NoHead,
            ..file(Policy::Open)
        },
        // dark and silent, loaded but off
        (Class::Headless | Class::Empty, Policy::Closed) => Outcome {
            runs: Runs::NoHead,
            ..file(Policy::Closed)
        },
        // a valid open file that declares nothing behaves as the shipped rules
        (Class::Empty, Policy::Open) => builtin(None),
        (Class::Partly, Policy::Open) => survivors(Policy::Open),
        (Class::Partly, Policy::Closed) => survivors(Policy::Closed),
        (Class::Broken, Policy::Open) => builtin(Some(Line::Skipped)),
        // the boot display lights for the notice alone, and nothing is delivered
        (Class::Broken, Policy::Closed) => Outcome {
            runs: Runs::Off,
            policy: Policy::Closed,
            line: Some(Line::ClosedOff),
            lit: Lit::Notice,
            fallback: Fallback::NoticeOnly,
            unplaced: Unplaced::Ignore,
            ..builtin(None)
        },
    }
}

impl Trusted {
    /// A trusted file's levels from what its parse found: every `policy` line, wherever it sits; the scope of every other error; and what survives the errors. Nothing here reads where a line sits, so no outcome depends on the order of the top-level keys, and a readable closed policy wins over every other line.
    pub fn of(policies: &[PolicyLine], scopes: &[Scope], kept: Kept) -> Trusted {
        let whole = scopes.iter().any(|s| match s {
            Scope::File => true,
            Scope::Head | Scope::Section | Scope::Line => false,
        });
        let closed = policies.iter().any(|d| match d {
            PolicyLine::Closed => true,
            PolicyLine::Open | PolicyLine::Unknown | PolicyLine::InSection => false,
        });
        let stated = if closed {
            Stated::Closed
        } else {
            match policies {
                [] => Stated::Unset,
                [only] => match only {
                    PolicyLine::Open => Stated::Open,
                    PolicyLine::Closed | PolicyLine::Unknown | PolicyLine::InSection => {
                        Stated::Unreadable
                    }
                },
                [_, _, ..] => Stated::Unreadable,
            }
        };
        let kept = if whole { Kept::Nothing } else { kept };
        // a second or unreadable `policy` line beside a readable closed one is an error of the rest; an unreadable policy is one of its own, which `resolve` counts
        let extra = match policies {
            [] | [PolicyLine::Open | PolicyLine::Closed] => false,
            [PolicyLine::Unknown | PolicyLine::InSection] | [_, _, ..] => true,
        };
        let clean = scopes.is_empty() && !(extra && stated == Stated::Closed);
        let rest = match (clean, kept) {
            (true, Kept::Heads) => Rest::Valid,
            (true, Kept::Headless) => Rest::Headless,
            (true, Kept::Nothing) => Rest::Empty,
            (false, Kept::Heads) => Rest::Partly,
            (false, Kept::Headless | Kept::Nothing) => Rest::Broken,
        };
        Trusted { stated, rest }
    }
}

/// What the error `problem`, on a line `within` the file, takes down (open cell ?4): an error inside a head's section drops that head, a bad rule drops that rule alone, and an error that leaves the file's structure or format untrustworthy drops the whole file. No error leaves a readable `policy` line unread: a trusted file always fails to its policy, and only one that fails the trust check or cannot be opened fails open. A problem that leaves the file unread is no scope: `scope` gives the first level's verdict as the error, [`Read::Untrusted`] or [`Read::Inaccessible`], so it never reaches [`Trusted::of`].
pub fn scope(problem: &Problem, within: Within) -> Result<Scope, Read> {
    let fixed = match problem {
        Problem::NotAFile | Problem::Unreadable(_) => return Err(Read::Inaccessible),
        Problem::Untrusted(_) => return Err(Read::Untrusted),
        Problem::TooLarge
        | Problem::MissingVersion
        | Problem::UnknownVersion(_)
        | Problem::LineTooLong
        | Problem::TooManyHeads
        | Problem::TooManyRules => Some(Scope::File),
        Problem::UnknownSection(_) | Problem::DuplicateSection(_) => Some(Scope::Section),
        Problem::BadName(_) | Problem::ReservedName(_) | Problem::DuplicateHead(_) => {
            Some(Scope::Head)
        }
        Problem::UnknownMatchKey(_)
        | Problem::BadToken(_)
        | Problem::UndefinedHead(_)
        | Problem::WrongKind { .. }
        | Problem::TooManyTokens => Some(Scope::Line),
        // a line that is not UTF-8 or holds a control character but a CRLF's carriage return is a bad line of its section
        Problem::NotUtf8
        | Problem::ControlCharacter(_)
        | Problem::Syntax(_)
        | Problem::UnknownKey(_)
        | Problem::DuplicateKey(_)
        | Problem::BadValue { .. }
        | Problem::OutOfRange { .. } => None,
    };
    Ok(match fixed {
        Some(scope) => scope,
        None => match within {
            Within::Top | Within::Rules => Scope::Line,
            Within::Header => Scope::Section,
            Within::Head => Scope::Head,
        },
    })
}

#[cfg(test)]
mod tests;
