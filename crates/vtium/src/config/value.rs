//! A key's value from its text, checked against the key's bound.

use super::keys::{self, Key, Kind, TEXT_MAX};
use super::{Origin, Problem, Reports, Value};

/// The key's default as a value; `None` when it has none.
pub(super) fn default(key: &Key) -> Option<Value> {
    match key.default {
        keys::Default::Switch(b) => Some(Value::Switch(b)),
        keys::Default::Number(n) => Some(Value::Number(n)),
        keys::Default::Text(t) => Some(Value::Text(t.to_owned())),
        keys::Default::Command(c) => {
            Some(Value::Command(c.iter().map(|w| (*w).to_owned()).collect()))
        }
        keys::Default::Unset => None,
    }
}

/// The key a name sets, and whether it is a switch's `no-` form.
pub(super) fn lookup(name: &str) -> Option<(&'static Key, bool)> {
    if let Some(key) = keys::find(name) {
        return Some((key, false));
    }
    let key = keys::find(name.strip_prefix("no-")?)?;
    (key.kind == Kind::Switch).then_some((key, true))
}

/// Text holds at most [`TEXT_MAX`] bytes and no control character.
fn text_ok(key: &'static Key, text: &str, origin: &Origin, reports: &mut Reports) -> bool {
    if text.len() > TEXT_MAX {
        reports.push(origin.clone(), Problem::TooLong(key.name));
        false
    } else if text.chars().any(char::is_control) {
        reports.push(origin.clone(), Problem::ControlCharacter(key.name));
        false
    } else {
        true
    }
}

/// The value `text` gives a number, text or choice key; `None`, reported,
/// when it gives none.
pub(super) fn parse(
    key: &'static Key,
    text: &str,
    origin: &Origin,
    reports: &mut Reports,
) -> Option<Value> {
    match key.kind {
        Kind::Number { min, max, .. } => {
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                let value = text.to_owned();
                reports.push(
                    origin.clone(),
                    Problem::NotANumber {
                        key: key.name,
                        value,
                    },
                );
                return None;
            }
            // All digits: too large to parse is past any bound.
            let n = text.parse::<u64>().unwrap_or(u64::MAX);
            let used = n.clamp(min, max);
            if used != n {
                let value = text.to_owned();
                reports.push(
                    origin.clone(),
                    Problem::Clamped {
                        key: key.name,
                        value,
                        used,
                    },
                );
            }
            Some(Value::Number(used))
        }
        Kind::Choice(choices) => {
            if choices.contains(&text) {
                Some(Value::Text(text.to_owned()))
            } else {
                let value = text.to_owned();
                reports.push(
                    origin.clone(),
                    Problem::NotAChoice {
                        key: key.name,
                        value,
                    },
                );
                None
            }
        }
        Kind::Text => text_ok(key, text, origin, reports).then(|| Value::Text(text.to_owned())),
        Kind::Command => {
            if !text_ok(key, text, origin, reports) {
                return None;
            }
            match split_command(text) {
                Ok(words) => Some(Value::Command(words)),
                Err(why) => {
                    reports.push(origin.clone(), Problem::BadCommand { key: key.name, why });
                    None
                }
            }
        }
        Kind::Switch => {
            reports.push(origin.clone(), Problem::TakesNoValue(key.name));
            None
        }
    }
}

/// A command line split into words as the C reference's `login` key was:
/// spaces and tabs separate words; single quotes keep everything up to the
/// next one; double quotes keep everything up to the next one but a
/// backslash before `"` or `\`; outside quotes a backslash keeps the next
/// character. Nothing is expanded.
pub(super) fn split_command(text: &str) -> Result<Vec<String>, &'static str> {
    #[derive(PartialEq)]
    enum Quote {
        None,
        Single,
        Double,
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = Quote::None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (&quote, c) {
            (Quote::None, ' ' | '\t') => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (Quote::None, '\'') => quote = Quote::Single,
            (Quote::None, '"') => quote = Quote::Double,
            (Quote::None, '\\') => {
                word.push(chars.next().ok_or("a backslash ends the command")?);
                in_word = true;
            }
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            (Quote::Double, '\\') => match chars.peek() {
                Some(&next @ ('"' | '\\')) => {
                    word.push(next);
                    in_word = true;
                    chars.next();
                }
                _ => {
                    word.push('\\');
                    in_word = true;
                }
            },
            (_, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote != Quote::None {
        return Err("a quote is not closed");
    }
    if in_word {
        words.push(word);
    }
    if words.is_empty() {
        return Err("no command");
    }
    Ok(words)
}
