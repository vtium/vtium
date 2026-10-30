//! What vtium runs for each state its head file can be in (ADR-1401 D7).
//!
//! Failure is resolved at three levels, each asked only once the one before it has passed: whether the file can be trusted ([`Read`]), whether it states a disposition ([`Stated`]), and how much of the rest is good ([`Rest`]). A trusted file's disposition and content exist only inside [`Read::Trusted`], so nothing can consult an untrusted file's content. [`resolve`] maps every path to one [`Outcome`], by exhaustive matches with no wildcard arm, so a new variant fails to compile until it has an answer. The module reads no file and holds no state: the loader classifies what it found with [`scope`] and [`Trusted::of`] and asks [`resolve`], and a reload asks [`Outcome::reload`].
//!
//! A readable deny is sticky. Once a trusted file states a readable `default = deny`, nothing opens anything up: no other line of the file, no broken line in it, and no change of state while vtium runs, a reload included.
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
    /// The second level: the disposition the file states with its `default` line, anywhere among the top-level keys. A readable `default = deny` wins over every other `default` line.
    pub enum Stated {
        /// No `default` line: open.
        Unset = "unset",
        Allow = "allow",
        Deny = "deny",
        /// No readable `default = deny`, and a `default` line vtium cannot read: an unknown value, a line misplaced inside a section, or a second `default` line. It counts as no disposition, and the file is at least partly broken.
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
        /// The file is never read: the first level's verdict, which a parse never reports.
        Trust = "trust",
        /// The file's structure or format cannot be trusted: nothing of it applies, but a readable `default` line still states its disposition, since a trusted file always fails to its disposition.
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
        /// A partly broken allow run: a head's `rescue = off` and `never-dark = off` are ignored.
        Forced = "forced",
        /// A partly broken deny run: the rescue stays as the file sets it, since a readable deny is sticky, so a keyboard the file does not grant the rescue may land nowhere; `never-dark = off` alone is ignored, as in every partly broken run.
        Sticky = "sticky",
    }
}

closed! {
    /// Where the focus may rest.
    pub enum Landing {
        /// On the heads that take it, by their `focus` and the posture.
        AsSet = "as-set",
        /// Also on the first surviving head that is lit, so a rescued keyboard has somewhere to land.
        FirstLitSurvivor = "first-lit-survivor",
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
    /// What an input no running rule places gets.
    pub enum Unplaced {
        /// It follows the focus, as an allow file's posture gives it.
        Focus = "focus",
        /// It walks the shipped rules' tail: a keyboard, pointer or key device follows the focus, and every other input is ignored, so a partly broken allow run is never more open than the shipped rules.
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
    /// What the display guard lights when the running rules light no display (ADR-1401 D9).
    pub enum Fallback {
        /// The boot display, for the first head the running text declares, else for the default head, the compiled-in configuration's `[head console]`.
        FirstHead = "first-head",
        /// Never a head on a display its rules do not claim: the boot display for the notice alone, and nothing for a run that declares no head and shows no line, so a valid deny file with no heads is dark and silent, loaded but off.
        NoticeOnly = "notice-only",
    }
}

closed! {
    /// A switch of a run: a guard (ADR-1401 D9), of which the file writes `never-dark` alone, or a head's right (D5).
    pub enum SwitchKey {
        AntiLockout = "anti-lockout",
        /// Set by the posture alone, as `anti-lockout` is: on under allow, off under deny.
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

    /// What the display guard may light when the running rules light no display.
    pub fn fallback(self) -> Fallback {
        match self {
            Posture::Allow => Fallback::FirstHead,
            Posture::Deny => Fallback::NoticeOnly,
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
    pub fallback: Fallback,
    pub unplaced: Unplaced,
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
            Runs::File | Runs::Survivors => match key {
                // the posture decides them; the head file has no such key
                SwitchKey::AntiLockout | SwitchKey::RescueKey => match self.posture {
                    Posture::Allow => Switch::On,
                    Posture::Deny => Switch::Off,
                },
                SwitchKey::NeverDark | SwitchKey::Focus | SwitchKey::VtKeys | SwitchKey::Rescue
                    if self.forces(key) =>
                {
                    Switch::On
                }
                SwitchKey::NeverDark | SwitchKey::Focus | SwitchKey::VtKeys | SwitchKey::Rescue => {
                    written
                }
            },
        }
    }

    /// Whether this run ignores the file's `off` for `key`: in a partly broken run, nothing that would prevent the rescue is honoured.
    fn forces(&self, key: SwitchKey) -> bool {
        // a sticky deny forces nothing that reaches an input
        let open = match self.rescue {
            Rescue::AsSet => return false,
            Rescue::Forced => true,
            Rescue::Sticky => false,
        };
        match key {
            SwitchKey::Rescue => open,
            // its console line and a rescued keyboard have a lit display
            SwitchKey::NeverDark => true,
            SwitchKey::AntiLockout
            | SwitchKey::RescueKey
            | SwitchKey::Focus
            | SwitchKey::VtKeys => false,
        }
    }

    /// What runs once a reload reads `read` while this outcome runs (vtium/vtium#86): a valid trusted file runs as it would at start, and a missing, untrusted, inaccessible, partly broken or broken file leaves whatever runs as it is, under either posture. A running deny is therefore sticky for the boot; a deny file deleted or made untrusted before a reboot starts the shipped rules with the notice.
    pub fn reload(&self, read: &Read) -> Outcome {
        let valid = match read {
            Read::Missing | Read::Untrusted | Read::Inaccessible => false,
            Read::Trusted(t) => matches!(
                (t.stated, t.rest),
                (
                    Stated::Unset | Stated::Allow | Stated::Deny,
                    Rest::Valid | Rest::Headless | Rest::Empty
                )
            ),
        };
        if valid { resolve(read) } else { *self }
    }
}

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
        fallback: Fallback::FirstHead,
        unplaced: Unplaced::Shipped,
    }
}

fn file(posture: Posture) -> Outcome {
    Outcome {
        runs: Runs::File,
        posture,
        fallback: posture.fallback(),
        unplaced: match posture {
            Posture::Allow => Unplaced::Focus,
            Posture::Deny => Unplaced::Ignore,
        },
        ..builtin(None)
    }
}

/// A partly broken run: what survives, under `posture`. Under allow nothing that would prevent the rescue is honoured, and the shipped rules' tail places what no surviving rule does; under deny the rescue stays as the file sets it, since a readable deny is sticky.
fn survivors(posture: Posture) -> Outcome {
    let (rescue, landing, unplaced) = match posture {
        Posture::Allow => (Rescue::Forced, Landing::FirstLitSurvivor, Unplaced::Shipped),
        Posture::Deny => (Rescue::Sticky, Landing::AsSet, Unplaced::Ignore),
    };
    Outcome {
        runs: Runs::Survivors,
        posture,
        rescue,
        landing,
        line: Some(Line::Partly),
        lit: Lit::Rules,
        fallback: posture.fallback(),
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
        // a valid open file that declares nothing behaves as the shipped rules
        (Class::Empty, Posture::Allow) => builtin(None),
        (Class::Partly, Posture::Allow) => survivors(Posture::Allow),
        (Class::Partly, Posture::Deny) => survivors(Posture::Deny),
        (Class::Broken, Posture::Allow) => builtin(Some(Line::Skipped)),
        // the boot display lights for the notice alone, and nothing is delivered
        (Class::Broken, Posture::Deny) => Outcome {
            runs: Runs::Off,
            posture: Posture::Deny,
            line: Some(Line::DenyOff),
            lit: Lit::Notice,
            fallback: Fallback::NoticeOnly,
            unplaced: Unplaced::Ignore,
            ..builtin(None)
        },
    }
}

impl Trusted {
    /// A trusted file's levels from what its parse found: every `default` line, wherever it sits; the scope of every other error; and what survives the errors. Nothing here reads where a line sits, so no outcome depends on the order of the top-level keys, and a readable deny wins over every other line.
    pub fn of(defaults: &[DefaultLine], scopes: &[Scope], kept: Kept) -> Trusted {
        let whole = scopes.iter().any(|s| match s {
            Scope::Trust | Scope::File => true,
            Scope::Head | Scope::Section | Scope::Line => false,
        });
        let deny = defaults.iter().any(|d| match d {
            DefaultLine::Deny => true,
            DefaultLine::Allow | DefaultLine::Unknown | DefaultLine::InSection => false,
        });
        let stated = if deny {
            Stated::Deny
        } else {
            match defaults {
                [] => Stated::Unset,
                [only] => match only {
                    DefaultLine::Allow => Stated::Allow,
                    DefaultLine::Deny | DefaultLine::Unknown | DefaultLine::InSection => {
                        Stated::Unreadable
                    }
                },
                [_, _, ..] => Stated::Unreadable,
            }
        };
        let kept = if whole { Kept::Nothing } else { kept };
        // a second or unreadable `default` line beside a readable deny is an error of the rest; an unreadable disposition is one of its own, which `resolve` counts
        let extra = match defaults {
            [] | [DefaultLine::Allow | DefaultLine::Deny] => false,
            [DefaultLine::Unknown | DefaultLine::InSection] | [_, _, ..] => true,
        };
        let clean = scopes.is_empty() && !(extra && stated == Stated::Deny);
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

/// What the error `problem`, on a line `within` the file, takes down (open cell ?4): an error inside a head's section drops that head, a bad rule drops that rule alone, and an error that leaves the file's structure or format untrustworthy drops the whole file. No error leaves a readable `default` line unread: a trusted file always fails to its disposition, and only one that fails the trust check or cannot be opened fails open.
pub fn scope(problem: &Problem, within: Within) -> Scope {
    let fixed = match problem {
        Problem::NotAFile | Problem::Unreadable(_) | Problem::Untrusted(_) => Some(Scope::Trust),
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
        // a line that is not UTF-8 or holds a control character is a bad line of its section
        Problem::NotUtf8
        | Problem::ControlCharacter(_)
        | Problem::Syntax(_)
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
