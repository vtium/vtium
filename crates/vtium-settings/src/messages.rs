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
    AtStart, Drops, Fate, Mend, Note, PolicyFault, Problem, Reason, Refusal, Remark, Renamed,
    Shown, Stating, Syntax, Untrusted, Warning,
};
use crate::identity::{Class, Kind};
use crate::keys::{Level, Levels};
use crate::matching::Token;
use crate::outcome::{Line, Policy, Read, Rescue, Runs, Stated, Trusted, resolve};
use crate::parse::{Takes, range, takes};
use crate::route::{Judged, Missing};

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
    pub const LOOK_HEADING: &str = "[look NAME]";
    /// A head's key that names its look.
    pub const LOOK: &str = "look";
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
    pub const DISPLAY_LABEL: &str = "display MATCH = LABEL";
    pub const INPUT_LABEL: &str = "input MATCH = LABEL";
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
            Syntax::UnclosedHeading => write!(
                f,
                "This section heading has no closing `{closing}`. Add one at the end of the line.",
                closing = CLOSING,
            ),
            Syntax::HeadOneName => write!(
                f,
                "A head's heading takes one name with no spaces, such as `{head}`. Give it one name.",
                head = HEAD_HEADING,
            ),
            Syntax::LookOneName => write!(
                f,
                "A look's heading takes one name with no spaces, such as `{look}`. Give it one name.",
                look = LOOK_HEADING,
            ),
            Syntax::NotKeyValue => write!(
                f,
                "This line should read `{key_value}`. Put an `{equals}` between the key and its value.",
                key_value = KEY_VALUE,
                equals = EQUALS,
            ),
            Syntax::RuleKind => write!(
                f,
                "A rule starts with `{display}` or `{input}`. Start this rule with the kind of device it places.",
                display = DISPLAY,
                input = INPUT,
            ),
            Syntax::RuleArrow => write!(
                f,
                "This rule has no `{arrow}`. A rule reads `{display_rule}` or `{input_rule}`. Add `{arrow}` before the target.",
                display_rule = DISPLAY_RULE,
                input_rule = INPUT_RULE,
                arrow = ARROW,
            ),
            Syntax::RuleOneTarget => write!(
                f,
                "This rule has more than one target after `{arrow}`. Leave one target there.",
                arrow = ARROW,
            ),
            Syntax::RuleMatch => write!(
                f,
                "This rule matches nothing. Before `{arrow}`, write `{star}` or one or more `{token}` matches. Add what this rule should match.",
                star = STAR,
                token = TOKEN,
                arrow = ARROW,
            ),
            Syntax::StarAlone => write!(
                f,
                "`{star}` matches every device, so it cannot be combined with other matches. Remove the other matches, or remove the `{star}`.",
                star = STAR,
            ),
            Syntax::LabelKind => write!(
                f,
                "A label line starts with `{display}` or `{input}`. Start this line with the kind of device it labels.",
                display = DISPLAY,
                input = INPUT,
            ),
            Syntax::LabelEquals => write!(
                f,
                "This label line has no `{equals}`. A label line reads `{display_label}` or `{input_label}`. Add `{equals}` between the match and the label.",
                display_label = DISPLAY_LABEL,
                input_label = INPUT_LABEL,
                equals = EQUALS,
            ),
            Syntax::LabelEmpty => write!(
                f,
                "This label line has no label after `{equals}`. Write the label after `{equals}`, or remove the line.",
                equals = EQUALS,
            ),
            Syntax::LabelLong => write!(
                f,
                "This label is longer than {max} characters. Shorten it to {max} characters or fewer.",
                max = crate::LABEL_MAX,
            ),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::NotAFile => f.write_str(
                "This is not a regular file. Put vtium.conf's text in a regular file at this path.",
            ),
            Problem::Unreadable(system_error) => write!(
                f,
                "vtium cannot read this file: {system_error}. Check that root can read it.",
            ),
            Problem::Untrusted(Untrusted::Owner { path, uid }) => write!(
                f,
                "vtium does not trust this file: {path} belongs to user {uid}, not to root. Make root the owner of that path, and let only root write to it.",
            ),
            Problem::Untrusted(Untrusted::Writable { path, mode }) => write!(
                f,
                "vtium does not trust this file: other users can change {path} (mode {mode:o}). Make root the owner of that path, and let only root write to it.",
            ),
            Problem::TooLarge => write!(
                f,
                "This file is too long: vtium reads at most {max} bytes. Remove what it does not need.",
                max = crate::FILE_MAX,
            ),
            Problem::NotUtf8 => f.write_str("This line is not UTF-8 text. Save the file as UTF-8."),
            Problem::LineTooLong => write!(
                f,
                "This line is too long: a line holds at most {max} bytes. Shorten it.",
                max = crate::LINE_MAX,
            ),
            Problem::ControlCharacter(c) => write!(
                f,
                "This line contains the control character {character}, which vtium.conf cannot contain. Delete it.",
                character = c.escape_unicode(),
            ),
            Problem::MissingVersion => write!(
                f,
                "`{version}` must be at the top of vtium.conf, above the first section. Add that line at the top.",
                version = VERSION_LINE,
            ),
            Problem::UnknownVersion(v) => write!(
                f,
                "vtium does not know version {v}; it reads only `{version}`. Change the version line to `{version}`.",
                version = VERSION_LINE,
            ),
            Problem::Syntax(syntax) => write!(f, "{syntax}"),
            Problem::DuplicateSection(section) => write!(
                f,
                "The file has a second `[{section}]` section, and only one is allowed. Move this section's lines into the first one."
            ),
            Problem::UnknownKey(k) => write!(
                f,
                "This section has no key named {k}. Check the spelling, or remove the line."
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
                "The name {n} cannot be used: a name has 1 to {max} characters, each a letter, a digit or one of `{punctuation}`. Rename it.",
                max = crate::NAME_MAX,
                punctuation = NAME_PUNCTUATION,
            ),
            Problem::ReservedName(n) => write!(
                f,
                "A head cannot be named {n}: rules use {n} as a special target. Choose another name."
            ),
            Problem::DuplicateHead(n) => write!(
                f,
                "The file already has a head named {n}. Rename one of the two."
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
                Takes::Name => write!(
                    f,
                    "`{key}` cannot be {value}: a name has 1 to {max} characters, each a letter, a digit or one of `{punctuation}`. Change the name.",
                    max = crate::NAME_MAX,
                    punctuation = NAME_PUNCTUATION,
                ),
            },
            Problem::OutOfRange { key, value } => match range(key) {
                Takes::Number { min, max: None } => {
                    write!(f, "`{key}` cannot be {value}: it takes {min} or more.")
                }
                Takes::Number {
                    min,
                    max: Some(max),
                } => write!(f, "`{key}` cannot be {value}: it takes {min} to {max}."),
                Takes::Seconds { max } => write!(
                    f,
                    "`{key}` cannot be {value}: it takes up to {max} seconds."
                ),
                Takes::Words(_) | Takes::Text | Takes::Name => {
                    write!(f, "`{key}` cannot be {value}: it is out of range.")
                }
            },
            Problem::BadVt(v) if v.raw().is_empty() => write!(
                f,
                "This head's `vt` names no VT. Give it a VT number from 1 to {max}, or remove the line so that the head runs on every VT.",
                max = crate::VT_MAX,
            ),
            Problem::BadVt(v) => write!(
                f,
                "This head's `vt` names {v}, which is not a VT. A VT is a number from 1 to {max}, and `vt = 3 4` names two of them.",
                max = crate::VT_MAX,
            ),
            Problem::RepeatedVt(vt) => write!(
                f,
                "This head's `vt` names VT {vt} twice. Remove one of the two."
            ),
            Problem::UnknownMatchKey(t) => write!(
                f,
                "A rule cannot match on {t}. Use one of {keys}.",
                keys = Listed(&Token::KEYS),
            ),
            Problem::BadToken(t) if t.raw().starts_with("class:") => write!(
                f,
                "vtium does not know the device class in {t}. Use one of {classes}.",
                classes = Listed(&Class::ALL.map(Class::name)),
            ),
            Problem::BadToken(t) => {
                let key = t.raw().split(':').next().unwrap_or_default();
                match Token::KEYS.iter().position(|k| *k == key) {
                    Some(i) => write!(
                        f,
                        "The match {t} has a value its key cannot take. Write it as `{form}`.",
                        form = Token::FORMS.get(i).copied().unwrap_or_default(),
                    ),
                    None => write!(
                        f,
                        "The match {t} has a value its key cannot take. Check the value."
                    ),
                }
            }
            Problem::UndefinedHead(n) => write!(
                f,
                "This rule sends devices to head {n}, but the file has no head of that name. Correct the name, or add a `{head}` section for it.",
                head = HEAD_HEADING,
            ),
            Problem::WrongKind {
                target,
                kind: Kind::Display,
            } => write!(
                f,
                "A display rule cannot send to `{target}`. Send the display to a head or to `{off}`.",
                off = OFF,
            ),
            Problem::WrongKind {
                target,
                kind: Kind::Input,
            } => write!(
                f,
                "An input rule cannot send to `{target}`. Send the device to a head, to `{roaming}` or to `{ignore}`.",
                roaming = ROAMING,
                ignore = IGNORE,
            ),
            Problem::TooManyHeads => write!(
                f,
                "The file has too many heads: vtium runs at most {max}. Remove the heads you do not need.",
                max = crate::HEADS_MAX,
            ),
            Problem::TooManyRules => write!(
                f,
                "The file has too many rules: vtium reads at most {max}. Remove the rules you do not need.",
                max = crate::RULES_MAX,
            ),
            Problem::TooManyTokens => write!(
                f,
                "This rule has too many matches: a rule takes at most {max}. Use fewer.",
                max = crate::TOKENS_MAX,
            ),
            Problem::Renamed(renamed) => write!(f, "{renamed}"),
            Problem::PolicyInSection => write!(
                f,
                "`{policy}` must be at the top of vtium.conf, above the first section, so vtium ignores this line. Move it there, beside `{version}`.",
                policy = POLICY,
                version = VERSION,
            ),
            Problem::UnreadablePolicy(fault) => write!(f, "{fault}"),
            Problem::NotASection(s) => write!(
                f,
                "vtium.conf has no section {s}. The sections are `{look}`, `{head}` and `{rules}`; rename this one.",
                look = LOOK_HEADING,
                head = HEAD_HEADING,
                rules = RULES_HEADING,
            ),
            Problem::BadLookName(n) => write!(
                f,
                "The look name {n} cannot be used: a name has 1 to {max} characters, each a letter, a digit or one of `{punctuation}`. Rename the look, and change every head's `{look}` line that names it.",
                max = crate::NAME_MAX,
                punctuation = NAME_PUNCTUATION,
                look = LOOK,
            ),
            Problem::DuplicateLook(n) => write!(
                f,
                "vtium.conf already has a look named {n}, so vtium skips this second one. Rename one of the two, or move this section's lines into the first."
            ),
            Problem::TooManyLooks => write!(
                f,
                "vtium.conf has too many looks: vtium reads at most {max}. Remove the looks you do not need.",
                max = crate::LOOKS_MAX,
            ),
            Problem::NotHere {
                key,
                section,
                levels,
            } => write!(
                f,
                "{}",
                NotHere {
                    key,
                    section: *section,
                    levels: *levels,
                }
            ),
            Problem::TooLong(key) => write!(
                f,
                "The value of `{key}` is too long: a value holds at most {max} bytes. Shorten it.",
                max = crate::keys::TEXT_MAX,
            ),
            Problem::VersionInSection => write!(
                f,
                "`{version}` must be at the top of vtium.conf, above the first section. Move this line there.",
                version = VERSION,
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
                "vtium cannot read this `{policy}` line because it is not UTF-8 text, so it treats the file as broken with the closed policy. Save the file as UTF-8.",
                policy = POLICY,
            ),
            PolicyFault::ControlCharacter(c) => write!(
                f,
                "vtium cannot read this `{policy}` line because it contains the control character {character}, so it treats the file as broken with the closed policy. Delete the character.",
                policy = POLICY,
                character = c.escape_unicode(),
            ),
            PolicyFault::LineTooLong => write!(
                f,
                "vtium cannot read this `{policy}` line because it is longer than {max} bytes, so it treats the file as broken with the closed policy. Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                max = crate::LINE_MAX,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Cut => write!(
                f,
                "vtium cannot read this `{policy}` line because it runs past the first {max} bytes, which is all vtium reads of a file, so it treats the file as broken with the closed policy. Remove what the file does not need.",
                policy = POLICY,
                max = crate::FILE_MAX,
            ),
            PolicyFault::NotKeyValue => write!(
                f,
                "vtium cannot read this `{policy}` line because it has no `{equals}`, so it treats the file as broken with the closed policy. (Above the first section, any line that begins with `{policy}`, in any letter case, is the policy line.) Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                equals = EQUALS,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Key(key) => write!(
                f,
                "vtium cannot read this `{policy}` line because its key is {key}, so it treats the file as broken with the closed policy. (Above the first section, any line that begins with `{policy}`, in any letter case, is the policy line.) Write it as `{open}` or `{closed}`.",
                policy = POLICY,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
            PolicyFault::Value(value) => write!(
                f,
                "`{policy}` cannot be {value}, so vtium treats the file as broken with the closed policy. Change it to one of {choices}.",
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
                "`{consoles}` is too high: a head can have at most {max} consoles, so this head gets {max}. Lower it to {max} or fewer.",
                consoles = CONSOLES,
                max = crate::CONSOLES_MAX,
            ),
            Warning::PastHeadCap { head, cap, .. } => write!(
                f,
                "Head {head} asks for more consoles than `{key}` allows, so it gets {cap}. Lower its `{consoles}` to {cap} or fewer, or raise `{key}` in vtium.conf.",
                key = HEAD_CONSOLES_MAX,
                consoles = CONSOLES,
            ),
            Warning::PastTotal {
                head, gets, total, ..
            } => write!(
                f,
                "The heads together ask for more consoles than `{key}` ({total}) allows, so head {head} gets {gets}. Lower the heads' `{consoles}`, or raise `{key}` in vtium.conf.",
                key = CONSOLES_TOTAL_MAX,
                consoles = CONSOLES,
            ),
            Warning::MissingFrit {
                look, frit, dir, ..
            } => write!(
                f,
                "Look {look} uses the font set {frit}, but {dir} has no {frit}.vtfrit. vtium then looks for a kbd font named {frit}, and uses its built-in font if there is none. Put the font set in {dir}, or change the look's `{key}`.",
                key = crate::keys::FONT_NAME.name,
            ),
            Warning::NoLook { head, look, .. } => write!(
                f,
                "Head {head} uses the default look, because vtium.conf has no look named {look}. Add a `{heading}` section with that name, or change the head's `{key}`.",
                heading = LOOK_HEADING,
                key = LOOK,
            ),
            Warning::Shadowed { by, .. } => write!(
                f,
                "this rule never matches: the rule on line {by} matches every device it would"
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
                "A head cannot be named `{focus}`: it is the old name of the target `{roaming}`. Choose another name.",
                focus = FOCUS,
                roaming = ROAMING,
            ),
            Renamed::DarkHead => write!(
                f,
                "A head cannot be named `{dark}`: it is the old name of the target `{off}`. Choose another name.",
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
                "The key `{rescue}` no longer exists. Use `{isolated}` instead: it stops the rescue key from moving this head's keyboards, and stops them from switching the VT. Or remove the line.",
                rescue = RESCUE,
                isolated = ISOLATED_YES,
            ),
            Renamed::Profile => write!(
                f,
                "The key `{profile}` is now called `{look}`. Rename the key to `{look}`, and give the look a `{look_heading}` section.",
                look = LOOK,
                profile = PROFILE,
                look_heading = LOOK_HEADING,
            ),
            Renamed::ProfileSection => write!(
                f,
                "A profile is now called a look. Rename the heading to `{look}`.",
                look = LOOK_HEADING,
            ),
            Renamed::Guards => write!(
                f,
                "The `{guards}` section no longer exists: `{policy}` now sets the guards, on with `{open}` and off with `{closed}`. Remove the section and its lines.",
                guards = GUARDS_HEADING,
                policy = POLICY,
                open = POLICY_OPEN,
                closed = POLICY_CLOSED,
            ),
        }
    }
}

/// The words that begin the message of a display that reaches no head.
const NO_DISPLAY_REACHES: &str = "No connected display reaches a head";
/// The words that begin the message of a head that no keyboard reaches,
/// before the head's name.
const NO_KEYBOARD_REACHES: &str = "No keyboard reaches head";
/// The words after the name of an isolated head that no keyboard reaches.
const ISOLATED_FROM_ROAMING: &str = "is isolated, so roaming keyboards do not reach it";

impl Missing {
    /// Words that only the messages of the gaps a connected device leaves
    /// contain: a display that reaches no head, a head no keyboard reaches,
    /// and an isolated head no keyboard reaches. A reader of `vtium
    /// check-config`'s report finds those messages by them.
    pub const DEVICE_GAPS: [&'static str; 3] = [
        NO_DISPLAY_REACHES,
        NO_KEYBOARD_REACHES,
        ISOLATED_FROM_ROAMING,
    ];
}

impl fmt::Display for Missing {
    /// The first thing a complete route lacks, and what to do about it
    /// (ADR-1403 D3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Missing::NoDisplay {
                display,
                rule: Some(rule),
            } => write!(
                f,
                "{lead}: rule {rule} sends display {display} to `{off}`. Add a rule above it that sends the display to a head.",
                lead = NO_DISPLAY_REACHES,
                off = OFF,
            ),
            Missing::NoDisplay {
                display,
                rule: None,
            } => write!(
                f,
                "{lead}: no rule matches display {display}, so it stays off. Add a display rule that sends it to a head.",
                lead = NO_DISPLAY_REACHES,
            ),
            Missing::NoKeyboard { head, display } => write!(
                f,
                "{lead} {head}, which shows on {display}. Add an input rule that sends a keyboard to {head} or to `{roaming}`.",
                lead = NO_KEYBOARD_REACHES,
                roaming = ROAMING,
            ),
            Missing::Isolated { head } => write!(
                f,
                "Head {head} {isolated}. Add an input rule that sends a keyboard to {head}, or write `{not}` in its section.",
                isolated = ISOLATED_FROM_ROAMING,
                not = ISOLATED_NO,
            ),
            Missing::NoDisplayRule => write!(
                f,
                "No display rule sends a display to a head. Add a rule `{rule}` whose target is a head.",
                rule = DISPLAY_RULE,
            ),
            Missing::NoKeyboardRule { head, rule } => write!(
                f,
                "No input rule sends a keyboard to head {head}, which gets its displays from rule {rule}. Add an input rule that sends a keyboard to {head} or to `{roaming}`.",
                roaming = ROAMING,
            ),
        }
    }
}

impl fmt::Display for Judged {
    /// How the route was judged, which `vtium check-config` notes when it
    /// judged by the rules alone (ADR-1403 D3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Judged::Devices => "vtium checked the routes against the connected devices.",
            Judged::NoDisplay => {
                "No display is connected, so vtium checked the routes by the rules alone."
            }
            Judged::NoKeyboard => {
                "No keyboard is connected, so vtium checked the routes by the rules alone."
            }
            Judged::RulesAlone => {
                "vtium checked the routes by the rules alone, without the connected devices."
            }
        })
    }
}

/// A key a section cannot set, and where the key table lets it be set:
/// each pairing of the section and the levels is a whole message.
struct NotHere<'a> {
    key: &'a str,
    section: Level,
    levels: Levels,
}

impl fmt::Display for NotHere<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key;
        match (self.section, self.levels) {
            (_, Levels::NONE) => write!(
                f,
                "`{key}` can be set only on vtium's command line, not in vtium.conf. Remove the line."
            ),
            (Level::Head, Levels::TOP_LOOK) => write!(
                f,
                "A head's section cannot set `{key}`: a head gets it from its look. Set it in the head's `{look}` section, or at the top of vtium.conf for every head.",
                look = LOOK_HEADING,
            ),
            (Level::Head, Levels::LOOK) => write!(
                f,
                "A head's section cannot set `{key}`: only a look can set it. Set it in the head's `{look}` section.",
                look = LOOK_HEADING,
            ),
            (Level::Look, Levels::TOP_HEAD) => write!(
                f,
                "A look cannot set `{key}`. Set it in a `{head}` section for one head, or at the top of vtium.conf for every head.",
                head = HEAD_HEADING,
            ),
            (Level::Head, _) => write!(
                f,
                "A head's section cannot set `{key}`: it applies to every head alike. Set it at the top of vtium.conf, above the first section."
            ),
            (Level::Look | Level::Top, _) => write!(
                f,
                "A look cannot set `{key}`: it applies to every head alike. Set it at the top of vtium.conf, above the first section."
            ),
        }
    }
}

/// A path as a shell command takes it: bare when every character is one a
/// shell reads as itself, else in single quotes, with every control and
/// format character escaped so the message stays on one line.
struct ShellPath<'a>(&'a str);

impl fmt::Display for ShellPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = |c: char| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ':' | ',')
        };
        if !self.0.is_empty() && self.0.chars().all(plain) {
            return f.write_str(self.0);
        }
        f.write_str("'")?;
        for c in self.0.chars() {
            match c {
                '\'' => f.write_str("'\\''")?,
                c if c.escape_debug().count() > 1 && c != '\\' && c != '"' => {
                    for e in c.escape_debug() {
                        fmt::Write::write_char(f, e)?;
                    }
                }
                c => fmt::Write::write_char(f, c)?,
            }
        }
        f.write_str("'")
    }
}

impl fmt::Display for Mend<'_> {
    /// `chown root:root PATH` or `chmod go-w PATH`, the path as a shell
    /// takes it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Untrusted::Owner { path, .. } => write!(f, "chown root:root {}", ShellPath(path.raw())),
            Untrusted::Writable { path, .. } => write!(f, "chmod go-w {}", ShellPath(path.raw())),
        }
    }
}

impl fmt::Display for Reason<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mend = self.0.mend();
        match self.0 {
            Untrusted::Owner { path, uid } => write!(
                f,
                "{path} belongs to user {uid}, not to root. Run `{mend}` to make root the owner.",
            ),
            Untrusted::Writable { path, mode } => write!(
                f,
                "other users can change {path} (mode {mode:o}). Run `{mend}` so that only its owner can change it.",
            ),
        }
    }
}

impl fmt::Display for Refusal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "vtium ignores this file and uses its built-in configuration, because {}",
            self.0.reason()
        )
    }
}

impl Line {
    /// The line as the console shows it, worded exactly as ADR-1401's
    /// outcomes table words it, since the observables and `outcomes.tsv`
    /// quote it.
    pub fn text(self) -> &'static str {
        match self {
            Line::Skipped => "vtium.conf not used: running the built-in configuration.",
            Line::Partly => "vtium.conf has errors: running the parts without errors.",
            Line::ClosedOff => "vtium.conf has errors and counts as closed: all input is off.",
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
                write!(f, "To see why, run {check} {path}.", check = CHECK_CONFIG)
            }
            Line::ClosedOff => write!(
                f,
                "This console takes no input. For the full report, run {check} {path} over ssh or from other boot media.",
                check = CHECK_CONFIG,
            ),
        }
    }
}

impl fmt::Display for Drops {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Drops::File => write!(
                f,
                "Because of this error, vtium ignores the whole file except its `{policy}` line.",
                policy = POLICY,
            ),
            Drops::Head => f.write_str(
                "Because of this error, vtium ignores this head and every rule that names it.",
            ),
            Drops::Section => {
                f.write_str("Because of this error, vtium ignores this whole section.")
            }
            Drops::Rule => f.write_str(
                "Because of this error, vtium ignores this rule, and each device goes on to the next rule.",
            ),
            Drops::Line => f.write_str("Because of this error, vtium ignores this line."),
            Drops::Closed => f.write_str(
                "Because of this error, vtium treats the whole file as broken with the closed policy: it runs nothing from the file, takes no input, and shows an error screen on the boot display.",
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
                "There is no vtium.conf here, so vtium starts with its built-in configuration and shows no notice.",
            ),
            Read::Untrusted => write!(
                f,
                "vtium does not read a file it does not trust, so it starts with its built-in configuration and shows `{line}`."
            ),
            Read::Inaccessible => write!(
                f,
                "vtium cannot read this file, so it starts with its built-in configuration and shows `{line}`."
            ),
            Read::Trusted(Trusted {
                stated: Stated::Unreadable,
                ..
            }) => write!(
                f,
                "vtium cannot read a `{policy}` line of this file, so at start it treats the whole file as broken with the closed policy: it takes no input and shows an error screen on the boot display, starting `{line}`.",
                policy = POLICY,
            ),
            Read::Trusted(_) => match (outcome.runs, outcome.policy, outcome.line) {
                (Runs::File, Policy::Open | Policy::Closed, _) => {
                    f.write_str("vtium starts with this file as written and shows no notice.")
                }
                (Runs::NoHead, Policy::Open, _) => f.write_str(
                    "This file has rules but no head, so vtium starts with its rules. Each display that no rule turns off shows a notice that it is not configured for any terminal.",
                ),
                (Runs::NoHead, Policy::Closed, _) => f.write_str(
                    "This file has no head and its policy is closed, so vtium starts with every display off, takes no input and shows nothing.",
                ),
                (Runs::Survivors, Policy::Open, _) => write!(
                    f,
                    "vtium starts with the heads and rules that have no errors, then the built-in rules. Every head acts as not isolated, whatever its `{isolated}` says. It shows `{line}`.",
                    isolated = ISOLATED,
                ),
                (Runs::Survivors, Policy::Closed, _) => write!(
                    f,
                    "vtium starts with the heads and rules that have no errors, and the rescue key off. Each head keeps its `{isolated}` setting, and a head without one is isolated. It shows `{line}`.",
                    isolated = ISOLATED,
                ),
                (Runs::Builtin, Policy::Open | Policy::Closed, None) => f.write_str(
                    "This file sets up no head, look or rule, so vtium starts with its built-in configuration and shows no notice.",
                ),
                (Runs::Builtin, Policy::Open | Policy::Closed, Some(_)) => write!(
                    f,
                    "Every head in this file has errors, so vtium starts with its built-in configuration and shows `{line}`."
                ),
                (Runs::Off, Policy::Open | Policy::Closed, _) => write!(
                    f,
                    "Every head in this file has errors and the policy is closed, so vtium takes no input and shows an error screen on the boot display, starting `{line}`."
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
                "The file sets no `{policy}`, so its policy is open: an input device no rule places roams, a head is isolated only where it says `{isolated}`, and the keyboard safeguards (anti-lockout and the rescue key) are on.",
                policy = POLICY,
                isolated = ISOLATED_YES,
            ),
            (Stated::Unset, Rescue::Forced) => write!(
                f,
                "The file sets no `{policy}`, so its policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `{isolated}` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.",
                policy = POLICY,
                isolated = ISOLATED,
            ),
            (Stated::Open, Rescue::AsSet | Rescue::Sticky) => write!(
                f,
                "The policy is open: an input device no rule places roams, a head is isolated only where it says `{isolated}`, and the keyboard safeguards (anti-lockout and the rescue key) are on.",
                isolated = ISOLATED_YES,
            ),
            (Stated::Open, Rescue::Forced) => write!(
                f,
                "The policy is open. Because parts of the file have errors, every head acts as not isolated, whatever its `{isolated}` says; an input device that no working rule places goes on to the built-in rules; and the keyboard safeguards (anti-lockout and the rescue key) are on.",
                isolated = ISOLATED,
            ),
            (Stated::Closed, Rescue::AsSet | Rescue::Sticky | Rescue::Forced) => write!(
                f,
                "The policy is closed: an input device no rule places is ignored, a head without an `{isolated}` line is isolated, and the keyboard safeguards (anti-lockout and the rescue key) are off.",
                isolated = ISOLATED,
            ),
            (Stated::Unreadable, Rescue::AsSet | Rescue::Sticky | Rescue::Forced) => write!(
                f,
                "vtium cannot read the `{policy}` line above the first section, so the file's policy is closed. Write that line as `{open}` or `{closed}`.",
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
                "weak match: this rule matches by `{id}` or `{name}` alone, which any device can claim",
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
                "Head {head} is not used: no rule sends a display or input device to it. Add a rule that sends a display to it, or remove the head."
            ),
            Remark::UnnamedLitByGuard { head } => write!(
                f,
                "No rule names head {head}. Because the file has errors, vtium gives it the boot display under the open policy when no other head has a display. Add a rule that sends a display to it, or remove the head."
            ),
            Remark::EveryHeadIsolated => write!(
                f,
                "Every head is isolated, so no keyboard can switch the VT and no head can be the active head. A head gets only the keyboards its own rules send to it. Write `{isolated}` in a head's section if a keyboard should roam to it.",
                isolated = ISOLATED_NO,
            ),
            Remark::NoActiveHead => f.write_str(
                "No head that can be active has a connected display, so a roaming keyboard reaches no head until one turns on. Connect a display for a head that is not isolated.",
            ),
            Remark::ClosedLockout => write!(
                f,
                "No keyboard reaches any head, so with the closed policy nobody can type at this machine. Before you install the file, send a keyboard to a head, or to `{roaming}` with a head that is not isolated.",
                roaming = ROAMING,
            ),
            Remark::Fell { device, fate } => match fate {
                Fate::Dark => write!(
                    f,
                    "No rule matches display {device}, so it stays off. Add a `{display}` rule for it if a head should use it.",
                    display = DISPLAY,
                ),
                Fate::Roams => write!(
                    f,
                    "No rule matches input device {device}, so with the open policy it types into the active head. Add an `{input}` rule to send it somewhere else.",
                    input = INPUT,
                ),
                Fate::RoamsNowhere => write!(
                    f,
                    "Input device {device} reaches no head: no rule matches it, so with the open policy it roams, but no head that can be active has a connected display. Add an `{input}` rule that sends it to a head.",
                    input = INPUT,
                ),
                Fate::Ignored => write!(
                    f,
                    "No rule matches input device {device}, so with the closed policy vtium ignores it. Add an `{input}` rule for it if it should reach a head.",
                    input = INPUT,
                ),
                Fate::TailRoams => write!(
                    f,
                    "No working rule matches input device {device}, so it goes on to the built-in rules and types into the active head. Fix the file's errors, or add an `{input}` rule to send it somewhere else.",
                    input = INPUT,
                ),
                Fate::TailRoamsNowhere => write!(
                    f,
                    "Input device {device} reaches no head: no working rule matches it, so it goes on to the built-in rules and roams, but no head that can be active has a connected display. Fix the file's errors, or add an `{input}` rule that sends it to a head.",
                    input = INPUT,
                ),
                Fate::TailIgnored => write!(
                    f,
                    "No working rule matches input device {device}, so it goes on to the built-in rules, which ignore it. Fix the file's errors, or add an `{input}` rule for it if it should reach a head.",
                    input = INPUT,
                ),
            },
        }
    }
}

impl fmt::Display for crate::write::SectionEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::error::Quoted;
        use crate::write::SectionEdit;
        use syntax::NAME_PUNCTUATION;
        match self {
            SectionEdit::NoSection(name) => write!(
                f,
                "vtium.conf has no section named {}. Pick one of its sections.",
                Quoted::new(name)
            ),
            SectionEdit::Exists(name) => write!(
                f,
                "vtium.conf already has a section named {}. Choose another name.",
                Quoted::new(name)
            ),
            SectionEdit::BadName(name) => write!(
                f,
                "{} cannot be used as a name: a name has 1 to {max} letters, digits or {NAME_PUNCTUATION}. Choose another name.",
                Quoted::new(name),
                max = crate::NAME_MAX,
            ),
            // the section reader's message for a key in the wrong section,
            // which says where the key is set
            SectionEdit::BadKey { key, .. } if key == VERSION => write!(
                f,
                "`{version}` must be at the top of vtium.conf, above the first section.",
                version = VERSION,
            ),
            SectionEdit::BadKey { key, section } => match crate::keys::find(key) {
                Some(table) => write!(
                    f,
                    "{}",
                    NotHere {
                        key: table.name,
                        section: *section,
                        levels: table.levels,
                    }
                ),
                None => write!(
                    f,
                    "This section has no key named {}. Check the spelling.",
                    Quoted::new(key)
                ),
            },
            SectionEdit::BadValue { key, value } => write!(
                f,
                "`{key}` cannot be set to {}: vtium would read it back as a different value. Write the value on one line, with no comment in it.",
                Quoted::new(value)
            ),
            SectionEdit::NoRule(n) => write!(
                f,
                "vtium.conf has no rule {n}. Rules are numbered from 1, in the order `[rules]` lists them."
            ),
            SectionEdit::BadRule(_) => f.write_str(
                "This is not a valid rule. Write it as `display MATCH -> TARGET` or `input MATCH -> TARGET`.",
            ),
            SectionEdit::Ambiguous(name) => write!(
                f,
                "vtium.conf has two sections named {}, so vtium-config cannot tell which one to change. Rename or remove one of them with vtium config edit.",
                Quoted::new(name)
            ),
            SectionEdit::LineTooLong { line, bytes } => write!(
                f,
                "This change would make line {line} of vtium.conf {bytes} bytes long. vtium does not read a line longer than {max} bytes, so vtium-config leaves the file as it is. Use a shorter value, rule, or name, or shorten the comment on that line with vtium config edit.",
                max = crate::LINE_MAX,
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
                    "vtium.conf not used: running the built-in configuration.",
                    "To see why, run vtium check-config /etc/vtium/heads.conf.".to_owned()
                ),
                (
                    "vtium.conf has errors: running the parts without errors.",
                    "To see why, run vtium check-config /etc/vtium/heads.conf.".to_owned()
                ),
                (
                    "vtium.conf has errors and counts as closed: all input is off.",
                    "This console takes no input. For the full report, run vtium check-config /etc/vtium/heads.conf over ssh or from other boot media.".to_owned()
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
            "To see why, run vtium check-config /tmp/a\\nb\\u{1b}[2J.conf."
        );
    }
}
