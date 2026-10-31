//! The catalog of the head file's messages (ADR-1401 D7): every error,
//! warning and note a check of the file shows a person, in one place, so
//! they can be reviewed and reworded together. vtium shows an error on the
//! screen in the notice of a file it ignores, and `vtium check-config`
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

use std::fmt;

use crate::error::{Note, Problem, Syntax, Untrusted, Warning};
use crate::identity::{Class, Kind};
use crate::matching::Token;
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
    pub const VERSION_LINE: &str = "version = 1";
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
    pub const IGNORE: &str = "ignore";
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
                "This line is not under a section heading such as `{head}`, `{rules}` or `{guards}`. Move it under the heading it belongs to.",
                head = HEAD_HEADING,
                rules = RULES_HEADING,
                guards = GUARDS_HEADING,
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
                "The file must start with `{version}`, before anything but comments. Add that line at the top.",
                version = VERSION_LINE,
            ),
            Problem::UnknownVersion(v) => write!(
                f,
                "vtium knows only `{version}`, not version {v}. Change the first line to `{version}`.",
                version = VERSION_LINE,
            ),
            Problem::Syntax(syntax) => write!(f, "{syntax}"),
            Problem::UnknownSection(s) => write!(
                f,
                "vtium knows no section {s}. Rename it to `{head}`, `{rules}` or `{guards}`, the sections vtium knows.",
                head = HEAD_HEADING,
                rules = RULES_HEADING,
                guards = GUARDS_HEADING,
            ),
            Problem::DuplicateSection(section) => write!(
                f,
                "The file already has a `{heading}` section, and may have only one. Move this section's lines into the first one.",
                heading = match *section {
                    "rules" => RULES_HEADING,
                    _ => GUARDS_HEADING,
                },
            ),
            Problem::UnknownKey(k) => write!(
                f,
                "This section takes no key named {k}. Check the key's spelling, or remove the line."
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
                "A display rule cannot have `{target}` as its target. Send it to a head or to `{dark}`.",
                dark = DARK,
            ),
            Problem::WrongKind {
                target,
                kind: Kind::Input,
            } => write!(
                f,
                "An input rule cannot have `{target}` as its target. Send it to a head, to `{focus}` or to `{ignore}`.",
                focus = FOCUS,
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
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::ConsolesCapped { .. } => write!(
                f,
                "more consoles than the cap; the head gets {max}",
                max = crate::CONSOLES_MAX,
            ),
            Warning::Shadowed { by, .. } => write!(
                f,
                "the rule on line {by} takes every device this one matches"
            ),
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::GuardOff { guard, .. } => write!(f, "the {guard} guard is off"),
            Note::RescueOff { .. } => f.write_str("the rescue key leaves this head's keyboards"),
            Note::WeakMatch { .. } => write!(
                f,
                "input reaches a head by `{id}` or `{name}` alone, which any device can claim",
                id = ID,
                name = NAME,
            ),
        }
    }
}
