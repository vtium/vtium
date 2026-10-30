//! What vtium runs for each state its head file can be in (ADR-1401 D7).
//!
//! Failure is resolved at three levels, each asked only once the one before it has passed: whether the file can be trusted ([`Read`]), whether it states a disposition ([`Stated`]), and how much of the rest is good ([`Rest`]). A trusted file's disposition and content exist only inside [`Read::Trusted`], so nothing can consult an untrusted file's content. [`resolve`] maps every path to one [`Outcome`], by exhaustive matches with no wildcard arm, so a new variant fails to compile until it has an answer. The module reads no file and holds no state: the loader classifies what it found with [`scope`] and [`Trusted::of`] and asks [`resolve`].
//!
//! `outcomes.tsv`, beside the crate's manifest, lists every path with its outcome; `outcome_table_matches_resolve` fails when it differs from [`resolve`], and `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config outcome_table` rewrites it.
//!
//! Only a trusted file carries what it states:
//!
//! ```
//! use vtium_config::{Read, Rest, Stated, Trusted};
//! let read = Read::Trusted(Trusted { stated: Stated::Deny, rest: Rest::Valid });
//! assert!(matches!(read, Read::Trusted(_)));
//! ```
//!
//! and an untrusted one has nowhere to carry it:
//!
//! ```compile_fail
//! use vtium_config::{Read, Rest, Stated, Trusted};
//! let read = Read::Untrusted(Trusted { stated: Stated::Deny, rest: Rest::Valid });
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

/// The first level: whether the file can be trusted (ADR-1401 D10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Read {
    /// No file at the path: the shipped rules run, silently.
    Missing,
    /// The file fails the ownership check, not root's or writable by anyone else, or a directory above it does. It is never read.
    Untrusted,
    /// Something is at the path that vtium cannot open and read as a regular file: a directory, a FIFO, an I/O error. It is never read.
    Inaccessible,
    /// The file passes the ownership check and its text is read.
    Trusted(Trusted),
}

/// The second and third levels of a trusted file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Trusted {
    pub stated: Stated,
    pub rest: Rest,
}

closed! {
    /// The second level: the disposition the file states with its `default` line, anywhere among the top-level keys.
    pub enum Stated {
        /// No `default` line: open.
        Unset = "unset",
        Allow = "allow",
        Deny = "deny",
        /// A `default` line vtium cannot read: an unknown value, a line misplaced inside a section, or more than one `default` line. It counts as no disposition, and the file is at least partly broken.
        Unreadable = "unreadable",
    }
}

closed! {
    /// The third level: the file apart from its `default` lines.
    pub enum Rest {
        /// Valid, declaring at least one head.
        Valid = "valid",
        /// Valid, declaring rules or guards but no head.
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
    /// A `default` line as the parser reads it, wherever it sits.
    pub enum DefaultLine {
        Allow = "allow",
        Deny = "deny",
        /// A value but `allow` or `deny`.
        Unknown = "unknown",
        /// A `default` line inside a section.
        InSection = "in-section",
    }
}

closed! {
    /// What survives of a trusted file once its errors have dropped what they take (open cell ?4).
    pub enum Kept {
        Nothing = "nothing",
        /// Rules or guards, but no head.
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
        Guards = "guards",
    }
}

closed! {
    /// What one error takes down (open cell ?4).
    pub enum Scope {
        /// The file is never read: the first level's verdict.
        Trust = "trust",
        /// The text, or the format its version names, cannot be read: nothing applies, the disposition included.
        Text = "text",
        /// The file's structure cannot be trusted: nothing of it applies, but its disposition is read.
        File = "file",
        /// One head, with every rule naming it, each of which falls through to the next rule.
        Head = "head",
        /// One section that is unknown, repeated or headed by a line that does not parse, with its lines.
        Section = "section",
        /// One line: a rule, which falls through to the next one; a guard switch, which takes its unset value; a top-level key.
        Line = "line",
    }
}

closed! {
    /// Which rules run.
    pub enum Runs {
        /// The file's own rules, as written.
        File = "file",
        /// What survives of the file.
        Survivors = "survivors",
        /// The shipped rules, [`Config::builtin`](crate::Config::builtin).
        Builtin = "builtin",
        /// None: no input is delivered.
        Off = "off",
    }
}

closed! {
    /// The effective posture: what an input no rule places gets, and what an unset switch takes (ADR-1401 D3).
    pub enum Posture {
        /// An input no rule places follows the focus, and every unset switch is on.
        Allow = "allow",
        /// An input no rule places is ignored, and every unset switch but `never-dark` is off.
        Deny = "deny",
    }
}

closed! {
    /// Whether the rescue is honoured as the file sets it.
    pub enum Rescue {
        AsSet = "as-set",
        /// `rescue-key = off` and a head's `rescue = off` are ignored, as is everything else [`Outcome::switch`] forces on.
        Forced = "forced",
    }
}

closed! {
    /// Where the focus may rest.
    pub enum Landing {
        /// On the heads that take it, by their `focus` and the posture.
        AsSet = "as-set",
        /// Also on the first surviving head, so a rescued keyboard has somewhere to land (open cell ?3).
        FirstSurvivor = "first-survivor",
    }
}

closed! {
    /// The console line vtium shows on the lit display; the details go to the log.
    pub enum Line {
        Skipped = "skipped",
        Partly = "partly",
        DenyOff = "deny-off",
    }
}

closed! {
    /// Which displays may light.
    pub enum Lit {
        /// The displays the running rules place, and the boot display when the display guard lights it.
        Rules = "rules",
        /// The boot display alone, for the notice; no session is reached through it.
        Notice = "notice",
        /// None.
        Dark = "dark",
    }
}

closed! {
    /// A switch the file may write: a guard of `[guards]`, or a head's right (ADR-1401 D5, D9).
    pub enum SwitchKey {
        AntiLockout = "anti-lockout",
        RescueKey = "rescue-key",
        NeverDark = "never-dark",
        Focus = "focus",
        VtKeys = "vt-keys",
        Rescue = "rescue",
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
            Line::DenyOff => "config invalid, deny: input off",
        }
    }
}

impl Posture {
    /// The value a switch the file leaves unset takes: everything under allow, `never-dark` alone under deny.
    pub fn unset(self, key: SwitchKey) -> bool {
        match self {
            Posture::Allow => true,
            Posture::Deny => match key {
                SwitchKey::NeverDark => true,
                SwitchKey::AntiLockout
                | SwitchKey::RescueKey
                | SwitchKey::Focus
                | SwitchKey::VtKeys
                | SwitchKey::Rescue => false,
            },
        }
    }
}

/// What vtium does with its head file on one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Outcome {
    pub runs: Runs,
    pub posture: Posture,
    pub rescue: Rescue,
    pub landing: Landing,
    /// The console line, `None` for a silent start.
    pub line: Option<Line>,
    pub lit: Lit,
}

impl Outcome {
    /// How `key` takes its value in this run.
    pub fn switch(&self, key: SwitchKey) -> Switch {
        let written = Switch::Written {
            unset: self.posture.unset(key),
        };
        match self.runs {
            // the template writes every switch on
            Runs::Builtin => Switch::On,
            Runs::Off => match key {
                SwitchKey::NeverDark => Switch::On,
                SwitchKey::AntiLockout
                | SwitchKey::RescueKey
                | SwitchKey::Focus
                | SwitchKey::VtKeys
                | SwitchKey::Rescue => Switch::Off,
            },
            Runs::File | Runs::Survivors if self.forces(key) => Switch::On,
            Runs::File | Runs::Survivors => written,
        }
    }

    /// Whether this run ignores the file's `off` for `key`: in a partly broken run, nothing that would prevent the rescue is honoured.
    fn forces(&self, key: SwitchKey) -> bool {
        match self.rescue {
            Rescue::AsSet => false,
            Rescue::Forced => match key {
                SwitchKey::RescueKey | SwitchKey::Rescue => true,
                SwitchKey::NeverDark => DEGRADED_NEVER_DARK,
                SwitchKey::AntiLockout | SwitchKey::Focus | SwitchKey::VtKeys => false,
            },
        }
    }
}

/// What a valid open file that declares nothing does (open cell ?1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmptyOpen {
    /// It is valid and runs the shipped rules, silently.
    Builtin,
    /// It is valid and runs as written: no rules.
    AsWritten,
    /// It counts as broken.
    Invalid,
}

/// Which partly broken runs let the focus rest on the first surviving head (open cell ?3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lands {
    /// None: the focus rests where the file and the posture let it.
    AsSet,
    /// Those under deny.
    UnderDeny,
    /// Every one, so a file that writes `focus = off` on every head still leaves the rescue a landing.
    Always,
}

/// Open cell ?1, recommended: a valid open file that declares nothing behaves as the shipped rules.
pub(crate) const EMPTY_OPEN: EmptyOpen = EmptyOpen::Builtin;

/// Open cell ?2, recommended: a broken deny file lights the boot display for its notice alone.
pub(crate) const DENY_BROKEN: Lit = Lit::Notice;

/// Open cell ?3, recommended for deny and proposed for allow too: the focus may also rest on the first surviving head.
pub(crate) const RESCUE_LANDS: Lands = Lands::Always;

/// Proposed: a partly broken run ignores `never-dark = off` too, so its console line and a rescued keyboard have a lit display.
pub(crate) const DEGRADED_NEVER_DARK: bool = true;

/// The content level once the `default` line's own error is counted.
#[derive(Clone, Copy)]
enum Class {
    Valid,
    Empty,
    Partly,
    Broken,
}

fn builtin(line: Option<Line>) -> Outcome {
    Outcome {
        runs: Runs::Builtin,
        posture: Posture::Allow,
        rescue: Rescue::AsSet,
        landing: Landing::AsSet,
        line,
        lit: Lit::Rules,
    }
}

fn file(posture: Posture) -> Outcome {
    Outcome {
        runs: Runs::File,
        posture,
        ..builtin(None)
    }
}

/// A partly broken run: what survives, under `posture`, with the rescue forced on.
fn survivors(posture: Posture) -> Outcome {
    let first = match (RESCUE_LANDS, posture) {
        (Lands::Always, Posture::Allow | Posture::Deny) | (Lands::UnderDeny, Posture::Deny) => true,
        (Lands::AsSet, Posture::Allow | Posture::Deny) | (Lands::UnderDeny, Posture::Allow) => {
            false
        }
    };
    Outcome {
        runs: Runs::Survivors,
        posture,
        rescue: Rescue::Forced,
        landing: if first {
            Landing::FirstSurvivor
        } else {
            Landing::AsSet
        },
        line: Some(Line::Partly),
        lit: Lit::Rules,
    }
}

/// What vtium does with its head file on the path `read` (ADR-1401 D7).
pub fn resolve(read: &Read) -> Outcome {
    let trusted = match read {
        Read::Missing => return builtin(None),
        Read::Untrusted | Read::Inaccessible => return builtin(Some(Line::Skipped)),
        Read::Trusted(trusted) => trusted,
    };
    let (posture, readable) = match trusted.stated {
        Stated::Unset | Stated::Allow => (Posture::Allow, true),
        Stated::Unreadable => (Posture::Allow, false),
        Stated::Deny => (Posture::Deny, true),
    };
    // the `default` line vtium cannot read is an error of its own
    let class = match (trusted.rest, readable) {
        (Rest::Valid | Rest::Headless, true) => Class::Valid,
        (Rest::Empty, true) => Class::Empty,
        (Rest::Valid, false) | (Rest::Partly, true | false) => Class::Partly,
        (Rest::Headless | Rest::Empty, false) | (Rest::Broken, true | false) => Class::Broken,
    };
    match (class, posture) {
        (Class::Valid, Posture::Allow) => file(Posture::Allow),
        (Class::Valid | Class::Empty, Posture::Deny) => file(Posture::Deny),
        (Class::Empty, Posture::Allow) => match EMPTY_OPEN {
            EmptyOpen::Builtin => builtin(None),
            EmptyOpen::AsWritten => file(Posture::Allow),
            EmptyOpen::Invalid => builtin(Some(Line::Skipped)),
        },
        (Class::Partly, Posture::Allow) => survivors(Posture::Allow),
        (Class::Partly, Posture::Deny) => survivors(Posture::Deny),
        (Class::Broken, Posture::Allow) => builtin(Some(Line::Skipped)),
        (Class::Broken, Posture::Deny) => Outcome {
            runs: Runs::Off,
            posture: Posture::Deny,
            line: Some(Line::DenyOff),
            lit: DENY_BROKEN,
            ..builtin(None)
        },
    }
}

impl Trusted {
    /// A trusted file's levels from what its parse found: every `default` line, wherever it sits; the scope of every other error; and what survives the errors. Nothing here reads where a line sits, so no outcome depends on the order of the top-level keys.
    pub fn of(defaults: &[DefaultLine], scopes: &[Scope], kept: Kept) -> Trusted {
        let mut unread = false;
        let mut whole = false;
        for s in scopes {
            match s {
                Scope::Trust | Scope::Text => unread = true,
                Scope::File => whole = true,
                Scope::Head | Scope::Section | Scope::Line => {}
            }
        }
        let stated = if unread {
            Stated::Unreadable
        } else {
            match defaults {
                [] => Stated::Unset,
                [only] => match only {
                    DefaultLine::Allow => Stated::Allow,
                    DefaultLine::Deny => Stated::Deny,
                    DefaultLine::Unknown | DefaultLine::InSection => Stated::Unreadable,
                },
                [_, _, ..] => Stated::Unreadable,
            }
        };
        let kept = if unread || whole { Kept::Nothing } else { kept };
        let clean = scopes.is_empty();
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

/// What the error `problem`, on a line `within` the file, takes down (open cell ?4): an error inside a head's section drops that head, a bad rule drops that rule alone, and an error that leaves the file's structure untrustworthy drops the whole file.
pub fn scope(problem: &Problem, within: Within) -> Scope {
    let fixed = match problem {
        Problem::NotAFile | Problem::Unreadable(_) | Problem::Untrusted(_) => Some(Scope::Trust),
        Problem::TooLarge
        | Problem::NotUtf8
        | Problem::ControlCharacter(_)
        | Problem::MissingVersion
        | Problem::UnknownVersion(_) => Some(Scope::Text),
        // a line too long is unread, and among the top-level keys it may be the `default` line
        Problem::LineTooLong => Some(match within {
            Within::Top => Scope::Text,
            Within::Header | Within::Head | Within::Rules | Within::Guards => Scope::File,
        }),
        Problem::TooManyHeads | Problem::TooManyRules => Some(Scope::File),
        Problem::UnknownSection(_) | Problem::DuplicateSection(_) => Some(Scope::Section),
        Problem::BadName(_) | Problem::ReservedName(_) | Problem::DuplicateHead(_) => {
            Some(Scope::Head)
        }
        Problem::UnknownMatchKey(_)
        | Problem::BadToken(_)
        | Problem::UndefinedHead(_)
        | Problem::WrongKind { .. }
        | Problem::TooManyTokens => Some(Scope::Line),
        Problem::Syntax(_)
        | Problem::UnknownKey(_)
        | Problem::DuplicateKey(_)
        | Problem::BadValue { .. }
        | Problem::OutOfRange { .. } => None,
    };
    match fixed {
        Some(scope) => scope,
        None => match within {
            Within::Top | Within::Rules | Within::Guards => Scope::Line,
            Within::Header => Scope::Section,
            Within::Head => Scope::Head,
        },
    }
}

#[cfg(test)]
mod tests;
