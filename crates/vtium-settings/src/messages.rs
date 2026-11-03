//! The catalog of the head file's messages (ADR-1401 D7): every error,
//! warning and note a check of the file shows a person, and the console
//! line of each outcome, in one place, so they can be reviewed and
//! reworded together. vtium shows an error on the screen in the notice of
//! a file it ignores, and `vtium check-config`
//! prints every one, each after the file and the line it names, as
//! `/etc/vtium/heads.conf:4: ...`. Text quoted from the file goes through
//! [`Quoted`], which escapes it.
//!
//! Each error says what is wrong, then what the person reading it can do
//! about it, in plain words. Each is a whole message: a choice between
//! wordings is two messages, never one sentence built from pieces. The
//! file's own syntax, such as `[head NAME]` or `version = 1`, enters a
//! message as a placeholder's value from [`syntax`], so a translation of
//! the words can never change it; so does the system's own error text,
//! named `system_error` where a message carries it.
//!
//! The four console lines of D7's outcomes keep ADR-1401's wording, and
//! each gains a sentence that says what to do next ([`Line::next`]).

use std::fmt;

use crate::error::{
    AtStart, Drops, Fate, Note, PolicyFault, Problem, Remark, Renamed, Shown, Stating, Syntax,
    Untrusted, Warning,
};
use crate::identity::{Class, Kind};
use crate::matching::Token;
use crate::outcome::{Line, Policy, Read, Rescue, Runs, Stated, Trusted, resolve};
use crate::parse::{Takes, range, takes};

/// The file's own words `items`, each in backticks, separated by commas:
/// a list for a message to name, written as it is shown.
struct Listed<'a>(&'a [&'static str]);

impl fmt::Display for Listed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, item) in self.0.iter().enumerate() {
            if n > 0 {
                f.write_str(", ")?;
            }
            write!(f, "`{item}`")?;
        }
        Ok(())
    }
}

/// The head file's own syntax as messages name it: placeholders' values,
/// never words to translate.
pub mod syntax {
    pub const HEAD_HEADING: &str = "[head NAME]";
    pub const RULES_HEADING: &str = "[rules]";
    pub const GUARDS_HEADING: &str = "[guards]";
    pub const VERSION: &str = "version";
    pub const VERSION_LINE: &str = "version = 1";
    pub const POLICY: &str = "policy";
    pub const POLICY_OPEN: &str = "policy = open";
    pub const POLICY_CLOSED: &str = "policy = closed";
    pub const KEY_VALUE: &str = "key = value";
    pub const EQUALS: &str = "=";
    pub const CLOSING: &str = "]";
    pub const DISPLAY: &str = "display";
    pub const INPUT: &str = "input";
    pub const DISPLAY_RULE: &str = "display MATCH -> TARGET";
    pub const INPUT_RULE: &str = "input MATCH -> TARGET";
    pub const ARROW: &str = "->";
    pub const STAR: &str = "*";
    pub const TOKEN: &str = "key:value";
    pub const DARK: &str = "dark";
    pub const FOCUS: &str = "focus";
    pub const OFF: &str = "off";
    pub const ROAMING: &str = "roaming";
    pub const IGNORE: &str = "ignore";
    pub const ON_LOSS: &str = "on-loss";
    pub const WHEN_DISPLAY_LOST: &str = "when-display-lost";
    pub const NOHUP: &str = "nohup";
    pub const KEEP: &str = "keep";
    pub const RESCUE: &str = "rescue";
    pub const ISOLATED_YES: &str = "isolated = yes";
    pub const ISOLATED: &str = "isolated";
    pub const ISOLATED_NO: &str = "isolated = no";
    pub const CONSOLES: &str = "consoles";
    /// vtium.conf's key for a head's console cap.
    pub const HEAD_CONSOLES_MAX: &str = "head-consoles-max";
    /// vtium.conf's key for the consoles every head holds together.
    pub const CONSOLES_TOTAL_MAX: &str = "consoles-max";
    /// The command that checks the head file, as a next step names it.
    pub const CHECK_CONFIG: &str = "vtium check-config";
    pub const PROFILE: &str = "profile";
    pub const ID: &str = "id";
    pub const NAME: &str = "name";
    /// The characters a name takes beyond letters and digits.
    pub const NAME_PUNCTUATION: &str = ". _ + -";
}

use syntax::*;

impl fmt::Display for Syntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Syntax::OutsideSection => write!(
                f,
                "Above the first section the file takes only `{version}` and `{policy}`, and this line is neither. Move it under the heading it belongs to, such as `{head}` or `{rules}`.",
                version = VERSION,
                policy = POLICY,
                head = HEAD_HEADING,
                rules = RULES_HEADING,
            ),
            Syntax::UnclosedHeading => write!(
                f,
                "This section heading has no closing `{closing}`. Add one at the end of the line.",
                closing = CLOSING,
            ),
            Syntax::HeadOneName => write!(
                f,
                "A head's heading names exactly one head, as `{head}`. Give it one name, with no spaces in it.",
                head = HEAD_HEADING,
            ),
            Syntax::NotKeyValue => write!(
                f,
                "This line should read `{key_value}`. Put an `{equals}` between the key and its value.",
                key_value = KEY_VALUE,
                equals = EQUALS,
            ),
            Syntax::RuleKind => write!(
                f,
                "A rule starts with `{display}` or `{input}`. Start this one with the kind of device it places.",
                display = DISPLAY,
                input = INPUT,
            ),
            Syntax::RuleArrow => write!(
                f,
                "A rule reads `{display_rule}` or `{input_rule}`, and this one has no `{arrow}`. Add `{arrow}` before its target.",
                display_rule = DISPLAY_RULE,
                input_rule = INPUT_RULE,
                arrow = ARROW,
            ),
            Syntax::RuleOneTarget => write!(
                f,
                "A rule names exactly one target after `{arrow}`. Leave one target there.",
                arrow = ARROW,
            ),
            Syntax::RuleMatch => write!(
                f,
                "A rule matches `{star}` or one or more `{token}` tokens before its `{arrow}`. Add what this rule should match.",
                star = STAR,
                token = TOKEN,
                arrow = ARROW,
            ),
            Syntax::StarAlone => write!(
                f,
                "`{star}` matches every device, so it stands alone, with no other token beside it. Remove the other tokens, or remove the `{star}`.",
                star = STAR,
            ),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::NotAFile => f.write_str(
                "This is not a regular file. Put the head file's text in a regular file at this path.",
            ),
            Problem::Unreadable(system_error) => write!(
                f,
                "vtium cannot read this file. The system says: {system_error}. Check that root can read it.",
            ),
            Problem::Untrusted(Untrusted::Owner { path, uid }) => write!(
                f,
                "vtium does not trust this file, because {path} belongs to user {uid} and not to root. Make root the owner of that path, and let only root write to it.",
            ),
            Problem::Untrusted(Untrusted::Writable { path, mode }) => write!(
                f,
                "vtium does not trust this file, because {path} can be changed by its group or by other users (mode {mode:o}). Make root the owner of that path, and let only root write to it.",
            ),
            Problem::TooLarge => write!(
                f,
                "This file is longer than {max} bytes, the most vtium reads. Remove what it does not need.",
                max = crate::FILE_MAX,
            ),
            Problem::NotUtf8 => f.write_str("This line is not UTF-8 text. Save the file as UTF-8."),
            Problem::LineTooLong => write!(
                f,
                "This line is longer than {max} bytes, the most a line may hold. Shorten it.",
                max = crate::LINE_MAX,
            ),
            Problem::ControlCharacter(c) => write!(
                f,
                "This line holds the control character {character}, which a head file may not hold. Delete it.",
                character = c.escape_unicode(),
            ),
            Problem::MissingVersion => write!(
                f,
                "The file needs `{version}` above its first section. Add that line at the top.",
                version = VERSION_LINE,
            ),
            Problem::UnknownVersion(v) => write!(
                f,
                "vtium knows only `{version}`, not version {v}. Change the version line to `{version}`.",
                version = VERSION_LINE,
            ),
            Problem::Syntax(syntax) => write!(f, "{syntax}"),
            Problem::UnknownSection(s) => write!(
                f,
                "vtium knows no section {s}. Rename it to `{head}` or `{rules}`, the sections vtium knows.",
                head = HEAD_HEADING,
                rules = RULES_HEADING,
            ),
            Problem::DuplicateSection(_) => write!(
                f,
                "The file already has a `{rules}` section, and may have only one. Move this section's lines into the first one.",
                rules = RULES_HEADING,
            ),
            Problem::UnknownKey(k) => write!(
                f,
                "This section takes no key named {k}. Check the key's spelling, or remove the line."
            ),
            // the top-level keys sit above every section
            Problem::DuplicateKey(k) if *k == VERSION || *k == POLICY => write!(
                f,
                "`{k}` is already set above the first section. Keep one of the two lines."
            ),
            Problem::DuplicateKey(k) => write!(
                f,
                "`{k}` is already set in this section. Keep one of the two lines."
            ),
            Problem::BadName(n) => write!(
                f,
                "The name {n} must be 1 to {max} characters, each a letter, a digit or one of `{punctuation}`. Rename it.",
                max = crate::NAME_MAX,
                punctuation = NAME_PUNCTUATION,
            ),
            Problem::ReservedName(n) => write!(
                f,
                "A head cannot be named {n}, which rules use as a target of their own. Choose another name."
            ),
            Problem::DuplicateHead(n) => write!(
                f,
                "The file already declares a head named {n}. Give one of the two another name."
            ),
            Problem::BadValue { key, value } => match takes(key) {
                Takes::Words(words) => write!(
                    f,
                    "`{key}` cannot be {value}. Change it to one of {choices}.",
                    choices = Listed(words),
                ),
                Takes::Number { min, max: None } => write!(
                    f,
                    "`{key}` cannot be {value}. Change it to a whole number of {min} or more."
                ),
                Takes::Number {
                    min,
                    max: Some(max),
                } => write!(
                    f,
                    "`{key}` cannot be {value}. Change it to a whole number from {min} to {max}."
                ),
                Takes::Seconds { max } => write!(
                    f,
                    "`{key}` cannot be {value}. Change it to a number of seconds up to {max}."
                ),
                Takes::Text => write!(f, "`{key}` cannot be empty. Give it a value."),
            },
            Problem::OutOfRange { key, value } => match range(key) {
                Takes::Number { min, max: None } => write!(
                    f,
                    "{value} is outside the range `{key}` takes, {min} or more. Choose a value inside it."
                ),
                Takes::Number {
                    min,
                    max: Some(max),
                } => write!(
                    f,
                    "{value} is outside the range `{key}` takes, {min} to {max}. Choose a value inside it."
                ),
                Takes::Seconds { max } => write!(
                    f,
                    "{value} is outside the range `{key}` takes, up to {max} seconds. Choose a shorter duration."
                ),
                Takes::Words(_) | Takes::Text => write!(
                    f,
                    "{value} is outside the range `{key}` takes. Choose a value inside it."
                ),
            },
            Problem::UnknownMatchKey(t) => write!(
                f,
                "The token {t} names a key a rule cannot match on. Use one of {keys}.",
                keys = Listed(&Token::KEYS),
            ),
            Problem::BadToken(t) if t.raw().starts_with("class:") => write!(
                f,
                "The token {t} names a class vtium does not know. Use one of {classes}.",
                classes = Listed(&Class::ALL.map(Class::name)),
            ),
            Problem::BadToken(t) => {
                let key = t.raw().split(':').next().unwrap_or_default();
                match Token::KEYS.iter().position(|k| *k == key) {
                    Some(i) => write!(
                        f,
                        "The token {t} gives its key a value it cannot take. Write it as `{form}`.",
                        form = Token::FORMS.get(i).copied().unwrap_or_default(),
                    ),
                    None => write!(
                        f,
                        "The token {t} gives its key a value it cannot take. Check the value."
                    ),
                }
            }
            Problem::UndefinedHead(n) => write!(
                f,
                "This rule sends devices to the head {n}, which the file does not declare. Correct the name, or declare the head with its own `{head}` section.",
                head = HEAD_HEADING,
            ),
            Problem::WrongKind {
                target,
                kind: Kind::Display,
            } => write!(
                f,
                "A display rule cannot have `{target}` as its target. Send it to a head or to `{off}`.",
                off = OFF,
            ),
            Problem::WrongKind {
                target,
                kind: Kind::Input,
            } => write!(
                f,
                "An input rule cannot have `{target}` as its target. Send it to a head, to `{roaming}` or to `{ignore}`.",
                roaming = ROAMING,
                ignore = IGNORE,
            ),
            Problem::TooManyHeads => write!(
                f,
                "The file declares more than {max} heads, the most vtium runs. Remove the heads it does not need.",
                max = crate::HEADS_MAX,
            ),
            Problem::TooManyRules => write!(
                f,
                "The file has more than {max} rules, the most vtium reads. Remove the rules it does not need.",
                max = crate::RULES_MAX,
            ),
            Problem::TooManyTokens => write!(
                f,
                "This rule matches on more than {max} tokens, the most one rule may use. Match on fewer.",
                max = crate::TOKENS_MAX,
            ),
            Problem::Renamed(renamed) => write!(f, "{renamed}"),
            Problem::PolicyInSection => write!(
                f,
                "vtium reads `{policy}` only above the first section, so it ignores this line, and the file's policy comes from the lines above the first section. Move it there, beside `{version}`.",
                policy = POLICY,
                version = VERSION,
            ),
            Problem::UnreadablePolicy(fault) => write!(f, "{fault}"),
            Problem::HiddenOutsideSection(start) => write!(
                f,
                "Above the first section the file takes only `{version}` and `{policy}`, and this line is neither. From just before a character that does not show on screen, it reads {start}, with that character escaped. Delete that character, or move the line under the heading it belongs to, such as `{head}` or `{rules}`.",
                version = VERSION,
                policy = POLICY,
                head = HEAD_HEADING,
                rules = RULES_HEADING,
            ),
        }
    }
}

impl fmt::Display for PolicyFault {
    /// Why vtium cannot read a `policy` line, which makes the file closed
    /// and broken (D7), and what to do about it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyFault::NotUtf8 => write!(
                f,
                "This `{policy}` line is not UTF-8 text, so vtium cannot read it and counts the file as closed and broken. Save the file as UTF-8.",
                policy = POLICY,
            ),
            PolicyFault::ControlCharacter(c) => write!(
                f,
                "This `{policy}` line holds the control character {character}, so vtium cannot read it and counts the file as closed and broken. Delete the character.",
                policy = POLICY,
                character = c.escape_unicode(),
            ),
            PolicyFault::LineTooLong => write!(
                f,
                "This `{policy}` line is longer than {max} bytes, the most a line may hold, so vtium cannot read it and counts the file as closed and broken. Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                max = crate::LINE_MAX,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Cut => write!(
                f,
                "This `{policy}` line runs past the first {max} bytes, the most vtium reads of a file, so vtium cannot read it and counts the file as closed and broken. Remove what the file does not need.",
                policy = POLICY,
                max = crate::FILE_MAX,
            ),
            PolicyFault::NotKeyValue => write!(
                f,
                "vtium reads a line above the first section that begins with `{policy}`, in any case, as the file's `{policy}` line. This one has no `{equals}`, so vtium cannot read it and counts the file as closed and broken. Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                equals = EQUALS,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Key(key) => write!(
                f,
                "vtium reads a line above the first section that begins with `{policy}`, in any case, as the file's `{policy}` line. This one's key is {key}, so vtium cannot read it and counts the file as closed and broken. Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Value(value) => write!(
                f,
                "`{policy}` cannot be {value}, so vtium cannot read this line and counts the file as closed and broken. Change it to one of {choices}.",
                policy = POLICY,
                choices = Listed(&crate::parse::POLICY),
            ),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::ConsolesCapped { .. } => write!(
                f,
                "`{consoles}` is past {max}, the most consoles a head may have, so the head gets {max} at most. Lower it to {max} or fewer.",
                consoles = CONSOLES,
                max = crate::CONSOLES_MAX,
            ),
            Warning::PastHeadCap { head, cap, .. } => write!(
                f,
                "The head {head} asks for more consoles than `{key}` allows, {cap}, so it gets {cap}. Lower its `{consoles}` to {cap} or fewer, or raise `{key}` in vtium.conf.",
                key = HEAD_CONSOLES_MAX,
                consoles = CONSOLES,
            ),
            Warning::PastTotal {
                head, gets, total, ..
            } => write!(
                f,
                "The heads together ask for more consoles than `{key}` allows, {total}, so the head {head} gets {gets}. Lower the heads' `{consoles}`, or raise `{key}` in vtium.conf.",
                key = CONSOLES_TOTAL_MAX,
                consoles = CONSOLES,
            ),
            Warning::MissingFrit {
                head, profile, dir, ..
            } => write!(
                f,
                "The head {head}'s profile names the frit {profile}, which is not in {dir}, so the head draws the default frit. Put that frit in {dir}, or change the head's `{key}`.",
                key = PROFILE,
            ),
            Warning::Shadowed { by, .. } => write!(
                f,
                "the rule on line {by} takes every device this one matches"
            ),
        }
    }
}

impl fmt::Display for Renamed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Renamed::Focus => write!(
                f,
                "The target `{focus}` is now called `{roaming}`. Change this rule's target to `{roaming}`.",
                focus = FOCUS,
                roaming = ROAMING,
            ),
            Renamed::Dark => write!(
                f,
                "The target `{dark}` is now called `{off}`. Change this rule's target to `{off}`.",
                dark = DARK,
                off = OFF,
            ),
            Renamed::FocusHead => write!(
                f,
                "A head cannot be named `{focus}`, the old name of the target `{roaming}`. Choose another name.",
                focus = FOCUS,
                roaming = ROAMING,
            ),
            Renamed::DarkHead => write!(
                f,
                "A head cannot be named `{dark}`, the old name of the target `{off}`. Choose another name.",
                dark = DARK,
                off = OFF,
            ),
            Renamed::OnLoss => write!(
                f,
                "The key `{on_loss}` is now called `{when_display_lost}`. Rename the key.",
                on_loss = ON_LOSS,
                when_display_lost = WHEN_DISPLAY_LOST,
            ),
            Renamed::Nohup => write!(
                f,
                "The value `{nohup}` is now called `{keep}`. Change it to `{keep}`.",
                nohup = NOHUP,
                keep = KEEP,
            ),
            Renamed::Rescue => write!(
                f,
                "The key `{rescue}` is gone: `{isolated}` now keeps the rescue key away from this head's keyboards, and also keeps them from switching the VT. Write that instead, or remove the line.",
                rescue = RESCUE,
                isolated = ISOLATED_YES,
            ),
            Renamed::HeadFont(key) => write!(
                f,
                "A head has no `{key}` of its own: its `{profile}` sets its font. Set the font in the head's profile, or remove the line.",
                profile = PROFILE,
            ),
            Renamed::Guards => write!(
                f,
                "The `{guards}` section is gone: the file's `{policy}` now sets the guards, on under `{open}` and off under `{closed}`. Remove the section and its lines.",
                guards = GUARDS_HEADING,
                policy = POLICY,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
        }
    }
}

impl Line {
    /// The line as the console shows it, worded exactly as ADR-1401's
    /// outcomes table words it, since the observables and `outcomes.tsv`
    /// quote it.
    pub fn text(self) -> &'static str {
        match self {
            Line::Skipped => "invalid config skipped, using builtin",
            Line::Partly => "config partly invalid, running what's valid",
            Line::ClosedOff => "vtium: config invalid, policy closed: input off",
            // nothing about input: each head's own keyboards still deliver to it while it is unlit
            Line::NoDisplay => "vtium: no display for any head",
        }
    }

    /// The sentence the notice shows after [`text`](Self::text): what to
    /// do next about the head file at `path` (D7, "What to do next").
    pub fn next(self, path: &std::path::Path) -> Next<'_> {
        Next { line: self, path }
    }
}

/// What to do next after a console line, about the head file at `path`.
#[derive(Clone, Copy, Debug)]
pub struct Next<'a> {
    line: Line,
    path: &'a std::path::Path,
}

impl fmt::Display for Next<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = Shown(self.path);
        match self.line {
            Line::Skipped | Line::Partly => {
                write!(f, "Run {check} {path} to see why.", check = CHECK_CONFIG,)
            }
            Line::ClosedOff => write!(
                f,
                "Nothing on this console takes input. Run {check} {path} over ssh, or from other boot media, for the full report.",
                check = CHECK_CONFIG,
            ),
            Line::NoDisplay => write!(
                f,
                "Connect a display that a rule in {path} sends to a head."
            ),
        }
    }
}

impl fmt::Display for Drops {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Drops::File => write!(
                f,
                "For this error vtium drops the whole file but its `{policy}`.",
                policy = POLICY,
            ),
            Drops::Head => f.write_str(
                "For this error vtium drops this head, with every rule that names it.",
            ),
            Drops::Section => {
                f.write_str("For this error vtium drops this section, with every line in it.")
            }
            Drops::Rule => f.write_str(
                "For this error vtium drops this rule alone, and a device walks on to the next rule.",
            ),
            Drops::Line => f.write_str("For this error vtium drops this line alone."),
            Drops::Closed => f.write_str(
                "For this error vtium counts the whole file as closed and broken: it runs nothing of the file, delivers no input, and shows the panic screen on the boot display.",
            ),
        }
    }
}

impl fmt::Display for AtStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let outcome = resolve(&self.0);
        let line = outcome.line.map_or("", Line::text);
        match self.0 {
            Read::Missing => f.write_str(
                "There is no head file here, so a start runs the compiled-in configuration and shows nothing.",
            ),
            Read::Untrusted => write!(
                f,
                "A start never reads a file it does not trust, so it runs the shipped default and shows `{line}`."
            ),
            Read::Inaccessible => write!(
                f,
                "A start cannot read this file, so it runs the shipped default and shows `{line}`."
            ),
            Read::Trusted(Trusted {
                stated: Stated::Unreadable,
                ..
            }) => write!(
                f,
                "vtium cannot read a `{policy}` line of this file, so a start counts the whole file as closed and broken: it delivers no input and shows the panic screen on the boot display, starting `{line}`.",
                policy = POLICY,
            ),
            Read::Trusted(_) => match (outcome.runs, outcome.policy, outcome.line) {
                (Runs::File, Policy::Open | Policy::Closed, _) => {
                    f.write_str("A start runs this file as written and shows nothing.")
                }
                (Runs::NoHead, Policy::Open, _) => f.write_str(
                    "This file has rules and declares no head, so a start runs its rules, and the display guard lights the boot display for the default head.",
                ),
                (Runs::NoHead, Policy::Closed, _) => f.write_str(
                    "This file declares no head under the closed policy, so a start lights no display, delivers no input and shows nothing: loaded but off.",
                ),
                (Runs::Survivors, Policy::Open, _) => write!(
                    f,
                    "A start runs the heads and rules that survive, then the shipped rules' tail, with every head acting as not isolated whatever its `{isolated}` says, and shows `{line}`.",
                    isolated = ISOLATED,
                ),
                (Runs::Survivors, Policy::Closed, _) => write!(
                    f,
                    "A start runs the heads and rules that survive, with the rescue key off; each head keeps the isolation it states, and a head that states none is isolated. It shows `{line}`."
                ),
                (Runs::Builtin, Policy::Open | Policy::Closed, None) => f.write_str(
                    "This file declares nothing, so a start runs the shipped default and shows nothing.",
                ),
                (Runs::Builtin, Policy::Open | Policy::Closed, Some(_)) => write!(
                    f,
                    "No head survives this file's errors, so a start runs the shipped default and shows `{line}`."
                ),
                (Runs::Off, Policy::Open | Policy::Closed, _) => write!(
                    f,
                    "No head survives this file's errors under the closed policy, so a start delivers no input and shows the panic screen on the boot display, starting `{line}`."
                ),
            },
        }
    }
}

impl fmt::Display for Stating {
    /// The policy a trusted file states, and what it sets: under a partly
    /// broken open file every head acts as not isolated (D7).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rescue = resolve(&Read::Trusted(self.0)).rescue;
        match (self.0.stated, rescue) {
            (Stated::Unset, Rescue::AsSet | Rescue::Sticky) => write!(
                f,
                "The file sets no `{policy}`, so its policy is open: an input no rule places roams, a head is isolated only where it says `{isolated}`, and the anti-lockout guard and the rescue key are on.",
                policy = POLICY,
                isolated = ISOLATED_YES,
            ),
            (Stated::Unset, Rescue::Forced) => write!(
                f,
                "The file sets no `{policy}`, so its policy is open, and since the file is partly broken every head acts as not isolated whatever its `{isolated}` says: an input no surviving rule places walks the shipped rules' tail, and the anti-lockout guard and the rescue key are on.",
                policy = POLICY,
                isolated = ISOLATED,
            ),
            (Stated::Open, Rescue::AsSet | Rescue::Sticky) => write!(
                f,
                "The policy is open: an input no rule places roams, a head is isolated only where it says `{isolated}`, and the anti-lockout guard and the rescue key are on.",
                isolated = ISOLATED_YES,
            ),
            (Stated::Open, Rescue::Forced) => write!(
                f,
                "The policy is open, and since the file is partly broken every head acts as not isolated whatever its `{isolated}` says: an input no surviving rule places walks the shipped rules' tail, and the anti-lockout guard and the rescue key are on.",
                isolated = ISOLATED,
            ),
            (Stated::Closed, Rescue::AsSet | Rescue::Sticky | Rescue::Forced) => write!(
                f,
                "The policy is closed: an input no rule places is ignored, a head that leaves `{isolated}` unset is isolated, and the anti-lockout guard and the rescue key are off.",
                isolated = ISOLATED,
            ),
            (Stated::Unreadable, Rescue::AsSet | Rescue::Sticky | Rescue::Forced) => write!(
                f,
                "vtium cannot read a `{policy}` line above the first section, so the file's policy is closed. Write that line as `{open}` or `{closed}`.",
                policy = POLICY,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::WeakMatch { .. } => write!(
                f,
                "input reaches a head by `{id}` or `{name}` alone, which any device can claim",
                id = ID,
                name = NAME,
            ),
        }
    }
}

impl fmt::Display for Remark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Remark::Unnamed { head } => write!(
                f,
                "No rule names the head {head}, so no display or input reaches it. Add a rule that sends a display to it, or remove the head."
            ),
            Remark::UnnamedLitByGuard { head } => write!(
                f,
                "No rule names the head {head}, so it has a display only when the display guard gives it the boot display, which the guard does under the open policy while no other head has a display. Add a rule that sends a display to it, or remove the head."
            ),
            Remark::EveryHeadIsolated => write!(
                f,
                "Every head is isolated, so no keyboard can switch the VT and no head can be the active head: only a keyboard a head's own rule claims reaches it. Write `{isolated}` on a head if a keyboard should roam to it.",
                isolated = ISOLATED_NO,
            ),
            Remark::NoActiveHead => f.write_str(
                "No head that can be the active head has a display present, so a keyboard that roams reaches no head until one lights. Connect a display for a head that is not isolated.",
            ),
            Remark::ClosedLockout => write!(
                f,
                "No keyboard present reaches any head, so under the closed policy nobody can type at this machine. Send a keyboard to a head, or to `{roaming}` with a head that is not isolated, before you install the file.",
                roaming = ROAMING,
            ),
            Remark::Fell { device, fate } => match fate {
                Fate::Dark => write!(
                    f,
                    "No rule matches the display {device}, so it stays dark. Add a `{display}` rule for it if a head should use it.",
                    display = DISPLAY,
                ),
                Fate::Roams => write!(
                    f,
                    "No rule matches the input {device}, so the open policy lets it roam into the active head. Add an `{input}` rule for it to place it anywhere else.",
                    input = INPUT,
                ),
                Fate::RoamsNowhere => write!(
                    f,
                    "No rule matches the input {device}, so the open policy lets it roam, but no head that can be the active head has a display present, so it reaches no head. Add an `{input}` rule that sends it to a head.",
                    input = INPUT,
                ),
                Fate::Ignored => write!(
                    f,
                    "No rule matches the input {device}, so the closed policy ignores it. Add an `{input}` rule for it if it should reach a head.",
                    input = INPUT,
                ),
                Fate::TailRoams => write!(
                    f,
                    "No surviving rule matches the input {device}, so it walks the shipped rules' tail and roams into the active head. Fix the file's errors, or add an `{input}` rule for it to place it anywhere else.",
                    input = INPUT,
                ),
                Fate::TailRoamsNowhere => write!(
                    f,
                    "No surviving rule matches the input {device}, so it walks the shipped rules' tail and roams, but no head that can be the active head has a display present, so it reaches no head. Fix the file's errors, or add an `{input}` rule that sends it to a head.",
                    input = INPUT,
                ),
                Fate::TailIgnored => write!(
                    f,
                    "No surviving rule matches the input {device}, so it walks the shipped rules' tail and is ignored. Fix the file's errors, or add an `{input}` rule for it if it should reach a head.",
                    input = INPUT,
                ),
            },
        }
    }
}

impl fmt::Display for crate::write::ProfileEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::error::Quoted;
        use crate::write::ProfileEdit;
        use syntax::{CHECK_CONFIG, HEAD_HEADING, NAME_PUNCTUATION, PROFILE};
        match self {
            ProfileEdit::Unreadable(_) => write!(
                f,
                "The head file holds errors, so vtium-config leaves it as it is. Run {CHECK_CONFIG} on the file to see them, and fix them first."
            ),
            ProfileEdit::NoHead(head) => write!(
                f,
                "The head file has no head named {head}. Each head is a {HEAD_HEADING} section; pick one of the heads it names.",
                head = Quoted::new(head),
            ),
            ProfileEdit::BadName(name) => write!(
                f,
                "A {PROFILE}'s name is 1 to {max} letters, digits or {NAME_PUNCTUATION}, so vtium would refuse {name}. Choose a name of those characters.",
                max = crate::bounds::NAME_MAX,
                name = Quoted::new(name),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::outcome::Line;

    // ADR-1401 D7, "What to do next": each console line keeps the record's
    // wording, which the observables and `outcomes.tsv` quote, and gains a
    // next step naming the file from its path
    // covers: platform.heads.rules
    #[test]
    fn each_console_line_keeps_its_wording_and_gains_a_next_step() {
        let path = Path::new("/etc/vtium/heads.conf");
        let shown: Vec<(&str, String)> = Line::ALL
            .iter()
            .map(|l| (l.text(), l.next(path).to_string()))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "invalid config skipped, using builtin",
                    "Run vtium check-config /etc/vtium/heads.conf to see why.".to_owned()
                ),
                (
                    "config partly invalid, running what's valid",
                    "Run vtium check-config /etc/vtium/heads.conf to see why.".to_owned()
                ),
                (
                    "vtium: config invalid, policy closed: input off",
                    "Nothing on this console takes input. Run vtium check-config /etc/vtium/heads.conf over ssh, or from other boot media, for the full report.".to_owned()
                ),
                (
                    "vtium: no display for any head",
                    "Connect a display that a rule in /etc/vtium/heads.conf sends to a head."
                        .to_owned()
                ),
            ]
        );
    }

    // a backslash in a path is escaped too, so a path holding a backslash
    // and an `n` never reads as one holding a newline
    #[test]
    fn a_backslash_in_a_path_is_escaped() {
        let backslash = crate::error::Shown(Path::new("/tmp/a\\nb.conf")).to_string();
        let newline = crate::error::Shown(Path::new("/tmp/a\nb.conf")).to_string();
        assert_eq!(backslash, "/tmp/a\\\\nb.conf");
        assert_eq!(newline, "/tmp/a\\nb.conf");
        assert_ne!(backslash, newline);
    }

    // the path is the file's, escaped, so a control character in it never
    // reaches the screen as itself
    #[test]
    fn the_next_step_shows_its_path_escaped() {
        let path = Path::new("/tmp/a\nb\u{1b}[2J.conf");
        assert_eq!(
            Line::Partly.next(path).to_string(),
            "Run vtium check-config /tmp/a\\nb\\u{1b}[2J.conf to see why."
        );
    }
}
