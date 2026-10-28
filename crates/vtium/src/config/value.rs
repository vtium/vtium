//! A key's value from its text, checked against the key's bound.

use std::ffi::OsString;

use super::keys::{self, Key, Kind, TEXT_MAX};
use super::{ModeSpec, Origin, Problem, Reports, Value};

/// The key's default as a value; `None` when it has none.
pub(super) fn default(key: &Key) -> Option<Value> {
    match key.default {
        keys::Default::Switch(b) => Some(Value::Switch(b)),
        keys::Default::Number(n) => Some(Value::Number(n)),
        keys::Default::Text(t) => Some(Value::Text(t.to_owned())),
        keys::Default::Command(c) => Some(Value::Command(c.iter().map(OsString::from).collect())),
        keys::Default::Rgb(c) => Some(Value::Rgb(c)),
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
        Kind::Number { word, .. } if word == Some(text) => Some(Value::Text(text.to_owned())),
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
                Ok(words) => Some(Value::Command(
                    words.into_iter().map(OsString::from).collect(),
                )),
                Err(why) => {
                    reports.push(origin.clone(), Problem::BadCommand { key: key.name, why });
                    None
                }
            }
        }
        Kind::Rgb => match parse_rgb(text) {
            Ok(c) => Some(Value::Rgb(c)),
            Err(why) => {
                let value = text.to_owned();
                reports.push(
                    origin.clone(),
                    Problem::BadValue {
                        key: key.name,
                        value,
                        why,
                    },
                );
                None
            }
        },
        Kind::Mode => match parse_mode(text) {
            Ok(m) => Some(Value::Modes(vec![m])),
            Err(why) => {
                let value = text.to_owned();
                reports.push(
                    origin.clone(),
                    Problem::BadValue {
                        key: key.name,
                        value,
                        why,
                    },
                );
                None
            }
        },
        Kind::Grab => {
            if !text_ok(key, text, origin, reports) {
                return None;
            }
            match crate::grab::parse(text) {
                Ok(chords) => Some(Value::Chords(chords)),
                Err(why) => {
                    let value = text.to_owned();
                    reports.push(
                        origin.clone(),
                        Problem::BadValue {
                            key: key.name,
                            value,
                            why,
                        },
                    );
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

/// A color, `r,g,b`, each a whole number from 0 to 255
/// (`src/config.c:806-855`).
pub(super) fn parse_rgb(text: &str) -> Result<[u8; 3], &'static str> {
    let mut out = [0_u8; 3];
    let mut parts = text.split(',');
    for slot in &mut out {
        let part = parts.next().ok_or("three values are needed, r,g,b")?.trim();
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err("each value is a whole number");
        }
        *slot = part.parse().map_err(|_| "each value is at most 255")?;
    }
    if parts.next().is_some() {
        return Err("three values are needed, r,g,b");
    }
    Ok(out)
}

/// The most bytes of a card's or a connector's name in a mode line, the
/// C reference's buffers less their NUL.
const MODE_NAME_MAX: usize = 63;

/// A mode line, `[GPU][CONNECTOR]WxH[@RATE]` (`src/config.c:564-617`).
pub(super) fn parse_mode(text: &str) -> Result<ModeSpec, &'static str> {
    let name = |s: &str| -> Result<Option<String>, &'static str> {
        if s.is_empty() {
            Err("an empty name in brackets")
        } else if s.len() > MODE_NAME_MAX {
            Err("a name longer than 63 bytes")
        } else {
            Ok(Some(s.to_owned()))
        }
    };
    let (gpu, connector, dims) = match text.split_once('[') {
        Some((gpu, rest)) => {
            let (connector, dims) = rest.split_once(']').ok_or("a '[' with no ']'")?;
            let gpu = if gpu.is_empty() { None } else { name(gpu)? };
            (gpu, name(connector)?, dims)
        }
        None => (None, None, text),
    };
    let side = |s: &str| -> Result<u32, &'static str> {
        match s.parse::<u32>() {
            Ok(n) if (1..=65_535).contains(&n) && s.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
            _ => Err("a size is WxH, each from 1 to 65535"),
        }
    };
    let (size, rate) = match dims.split_once('@') {
        Some((size, rate)) => {
            let rate = rate
                .parse::<u32>()
                .ok()
                .filter(|_| rate.bytes().all(|b| b.is_ascii_digit()))
                .ok_or("a rate is a whole number of Hz")?;
            (size, (rate > 0).then_some(rate))
        }
        None => (dims, None),
    };
    let (w, h) = size
        .split_once('x')
        .ok_or("a size is WxH, each from 1 to 65535")?;
    Ok(ModeSpec {
        gpu,
        connector,
        width: side(w)?,
        height: side(h)?,
        rate,
    })
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
