//! vtium.conf's top level read and edited line by line (ADR-1201 D5,
//! ADR-1403 D8), so that a file a person wrote keeps its comments, blank
//! lines, order and line endings after a tool has changed it.
//!
//! A top-level line is one key: `key=value`, or a switch's `key` to turn
//! it on and `no-key` to turn it off. One comment rule serves the whole of
//! vtium.conf (ADR-1403 D4): a `#` starts a comment at the start of a line
//! or after a blank, a `#` inside a word is part of the word, and `\#`
//! after a blank is a literal `#`. A `#` that opens a `#rrggbb` color value
//! is part of the value even after a blank. Spaces, tabs and a carriage
//! return around the key and the value are dropped. When a key is set on
//! more than one line, vtium takes the last. [`key_line`] reads one line by
//! that rule.
//!
//! [`setting`], [`switch`] and [`edit_conf`] read and change only the top
//! level, the lines above the first section heading, as
//! [`split`](crate::split) finds it, so a look's `font-size = 14` is never
//! taken for the top level's `font-size`. [`edit_conf`] makes a list of
//! [`Edit`]s:
//!
//! - A key that is set has its value replaced on the top-level line vtium
//!   takes it from, with the blanks and any comment after it kept.
//! - A key that is not set but has a commented top-level line, as
//!   `#font-size=0` in the shipped example, gets its new line directly
//!   below that comment.
//! - Any other new key goes at the end of the top level: at the end of a
//!   file with no section, and in a file with sections directly after the
//!   last top-level line that holds a key, or before the first heading when
//!   none does, never inside a section.
//! - [`Edit::Unset`] comments out every top-level line that sets the key,
//!   so the key falls back to its default and each line is there to set
//!   again.
//! - A switch is written `key` or `no-key`.
//!
//! The editor knows the syntax and not the keys: which keys exist and what
//! values they take is the console's key table, and `vtium check-config`
//! judges an edited file before it is installed.
//!
//! [`split_line`] is the line reading the console used before vtium.conf
//! held sections, which ends a value at any `#` but one that opens a
//! `#rrggbb` value, as the C reference did (`src/misc/conf.c`,
//! `parse_line`). The console reads by it until it moves to [`key_line`].

use std::borrow::Cow;
use std::ops::Range;

use crate::split::split;

/// One change to vtium.conf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit<'a> {
    /// Sets a key that takes a value: `key=value`.
    Set(&'a str, &'a str),
    /// Turns a switch on, `key`, or off, `no-key`.
    Switch(&'a str, bool),
    /// Leaves a key at its default.
    Unset(&'a str),
}

/// Why [`edit_conf`] changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfEdit {
    /// A key's name is not lowercase letters, digits and `-`.
    BadKey(String),
    /// vtium would read this value back as something else, or not at all.
    BadValue { key: String, value: String },
}

/// What one line holds, by byte ranges of the line.
struct Scan {
    key: Range<usize>,
    value: Option<Range<usize>>,
    /// Where each `\` of a `\#` inside the value sits, which reading the
    /// value drops.
    escapes: Vec<usize>,
}

fn blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r')
}

/// `range` of `line` with blanks dropped from both ends.
fn trimmed(line: &[u8], range: Range<usize>) -> Range<usize> {
    let part = line.get(range.clone()).unwrap_or_default();
    let lead = part.iter().position(|&b| !blank(b)).unwrap_or(part.len());
    let end = part
        .iter()
        .rposition(|&b| !blank(b))
        .map_or(lead, |e| e.saturating_add(1));
    range.start.saturating_add(lead)..range.start.saturating_add(end)
}

/// Whether `s` starts with `#rrggbb` ended by the line, a blank or a
/// comment.
fn begins_rgb(s: &[u8]) -> bool {
    match s {
        [b'#', rgb @ ..] if rgb.len() >= 6 => {
            let (digits, rest) = rgb.split_at(6);
            digits.iter().all(u8::is_ascii_hexdigit)
                && matches!(rest.first(), None | Some(b' ' | b'\t' | b'\r' | b'#'))
        }
        _ => false,
    }
}

/// Whether the `\` at `at` of `line` makes the `#` after it a literal one.
fn escaped_hash(line: &[u8], at: usize) -> bool {
    line.get(at.saturating_add(1)) == Some(&b'#')
}

/// Where a line's comment starts, by vtium.conf's one comment rule
/// (ADR-1403 D4), and where each `\` of a `\#` before it sits: a `#` at
/// the start of the line or after a blank starts the comment, a `#` inside
/// a word is part of the word, `\#` after a blank is a literal `#`, and a
/// `#` that opens a `#rrggbb` value after the line's first `=` is part of
/// the value. Without a comment it starts at the line's end. The section
/// reader and the top level's readers both find comments by this.
pub(crate) fn comment(line: &[u8]) -> (usize, Vec<usize>) {
    let mut at = 0_usize;
    let mut after_blank = true;
    let mut seen_equals = false;
    let mut escapes = Vec::new();
    while let Some(&b) = line.get(at) {
        match b {
            b'#' if after_blank => return (at, escapes),
            b'\\' if after_blank && escaped_hash(line, at) => {
                escapes.push(at);
                at = at.saturating_add(2);
                after_blank = false;
                continue;
            }
            b'=' if !seen_equals => {
                seen_equals = true;
                let rest = line.get(at.saturating_add(1)..).unwrap_or_default();
                let lead = rest
                    .iter()
                    .position(|b| !matches!(b, b' ' | b'\t'))
                    .unwrap_or(rest.len());
                at = at.saturating_add(1).saturating_add(lead);
                after_blank = lead > 0;
                if begins_rgb(line.get(at..).unwrap_or_default()) {
                    at = at.saturating_add(7);
                    after_blank = false;
                }
                continue;
            }
            _ => {}
        }
        after_blank = blank(b);
        at = at.saturating_add(1);
    }
    (line.len(), escapes)
}

/// One line's key and value by the one comment rule; `None` for a blank or
/// comment line. `line` holds no newline.
fn scan(line: &[u8]) -> Option<Scan> {
    let (end, escapes) = comment(line);
    let body = line.get(..end).unwrap_or_default();
    let equals = body.iter().position(|&b| b == b'=');
    let value = equals.map(|equals| {
        let after = equals.saturating_add(1);
        let rest = body.get(after..).unwrap_or_default();
        let lead = rest
            .iter()
            .position(|b| !matches!(b, b' ' | b'\t'))
            .unwrap_or(rest.len());
        trimmed(line, after.saturating_add(lead)..end)
    });
    let key = trimmed(line, 0..equals.unwrap_or(end));
    let escapes = match &value {
        Some(v) => escapes.into_iter().filter(|e| v.contains(e)).collect(),
        None => Vec::new(),
    };
    (!key.is_empty() || value.is_some()).then_some(Scan {
        key,
        value,
        escapes,
    })
}

/// One top-level line of vtium.conf as vtium reads it: its key, and its
/// value with every `\#` read as `#`, or `None` for a switch's line, which
/// has no `=`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyLine<'a> {
    pub key: &'a [u8],
    pub value: Option<Cow<'a, [u8]>>,
}

/// One top-level line's key and value by vtium.conf's one comment rule
/// (ADR-1403 D4): the key with the blanks around it dropped, and the value
/// after `=` with its blanks and any comment after it dropped and each
/// `\#` read as `#`; `None` for a blank or comment line. `line` holds no
/// newline. A `#` glued to a value, as in `font-size=12#big`, stays in the
/// value, which the C reference read as `12` (`src/misc/conf.c`,
/// `parse_line`).
pub fn key_line(line: &[u8]) -> Option<KeyLine<'_>> {
    let s = scan(line)?;
    let key = line.get(s.key).unwrap_or_default();
    let value = s.value.map(|v| {
        let raw = line.get(v.clone()).unwrap_or_default();
        if s.escapes.is_empty() {
            Cow::Borrowed(raw)
        } else {
            Cow::Owned(
                raw.iter()
                    .enumerate()
                    .filter(|(i, _)| !s.escapes.contains(&v.start.saturating_add(*i)))
                    .map(|(_, &b)| b)
                    .collect(),
            )
        }
    });
    Some(KeyLine { key, value })
}

/// One line's key and value by the C reference's comment rule, which ends
/// a key or a value at any `#` but one that opens a `#rrggbb` value.
fn scan_at_any_hash(line: &[u8]) -> Option<(Range<usize>, Option<Range<usize>>)> {
    let end = line
        .iter()
        .position(|&b| b == b'#' || b == b'=')
        .unwrap_or(line.len());
    let value = match line.get(end) {
        Some(b'=') => {
            let after = end.saturating_add(1);
            let rest = line.get(after..).unwrap_or_default();
            let lead = rest
                .iter()
                .position(|b| !matches!(b, b' ' | b'\t'))
                .unwrap_or(rest.len());
            let start = after.saturating_add(lead);
            let body = line.get(start..).unwrap_or_default();
            let skip = usize::from(begins_rgb(body));
            let stop = body
                .iter()
                .skip(skip)
                .position(|&b| b == b'#')
                .map_or(body.len(), |p| p.saturating_add(skip));
            Some(trimmed(line, start..start.saturating_add(stop)))
        }
        _ => None,
    };
    let key = trimmed(line, 0..end);
    (!key.is_empty() || value.is_some()).then_some((key, value))
}

/// One vtium.conf line's key and value as the console reads it until it
/// moves to [`key_line`]: the key with the blanks around it dropped, and
/// the value after `=` with its blanks and any comment after it dropped,
/// or `None` for a line with no `=`, a switch's; `None` for a blank or
/// comment line. `line` holds no newline. A `#` ends the key or the value
/// wherever it sits, but one that opens a `#rrggbb` value.
pub fn split_line(line: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
    let (key, value) = scan_at_any_hash(line)?;
    let key = line.get(key).unwrap_or_default();
    let value = value.map(|v| line.get(v).unwrap_or_default());
    Some((key, value))
}

/// The text of a line's key, or empty.
fn key_of<'a>(line: &'a str, s: &Scan) -> &'a str {
    line.get(s.key.clone()).unwrap_or_default()
}

/// A line of the file, its ending apart.
struct Line {
    text: String,
    ending: &'static str,
}

fn lines(text: &str) -> Vec<Line> {
    text.split_inclusive('\n')
        .map(|l| {
            let (body, ending) = match l.strip_suffix("\r\n") {
                Some(b) => (b, "\r\n"),
                None => match l.strip_suffix('\n') {
                    Some(b) => (b, "\n"),
                    None => (l, ""),
                },
            };
            Line {
                text: body.to_owned(),
                ending,
            }
        })
        .collect()
}

/// Whether `line` sets the key `key`: a value key on a line with a value,
/// a switch on a line without one, under its name or `no-` and its name.
fn sets(line: &str, key: &str, switch: bool) -> bool {
    scan(line.as_bytes()).is_some_and(|s| {
        let k = key_of(line, &s);
        if switch {
            s.value.is_none() && (k == key || k.strip_prefix("no-") == Some(key))
        } else {
            s.value.is_some() && k == key
        }
    })
}

/// Whether `line` is a comment whose text, without its `#`, sets `key`.
fn commented(line: &str, key: &str, switch: bool) -> bool {
    let rest = line.trim_start_matches([' ', '\t']);
    rest.strip_prefix('#')
        .is_some_and(|inner| !inner.starts_with('#') && sets(inner, key, switch))
}

fn good_key(key: &str) -> bool {
    !key.is_empty()
        && key.strip_prefix("no-") != Some("")
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether vtium reads `key=value` back as that key and that value.
fn round_trips(key: &str, value: &str) -> bool {
    let line = format!("{key}={value}");
    !value.contains(['\n', '\0'])
        && key_line(line.as_bytes()).is_some_and(|l| {
            l.key == key.as_bytes()
                && l.value
                    .is_some_and(|v| *v == *value.as_bytes() && !v.is_empty())
        })
}

/// The value vtium takes for `key` from the top level of the file `text`,
/// the value on the last top-level line that sets it; `None` when no
/// top-level line sets it. A section's line of the same name is not the
/// top level's.
pub fn setting(text: &str, key: &str) -> Option<String> {
    let top: Vec<(usize, &[u8])> = split(text.as_bytes()).top_lines().collect();
    top.into_iter().rev().find_map(|(_, line)| {
        let read = key_line(line)?;
        (read.key == key.as_bytes())
            .then(|| read.value.map(|v| String::from_utf8_lossy(&v).into_owned()))
            .flatten()
    })
}

/// Whether the switch `key` is on at the top level of the file `text`, by
/// the last top-level line that sets it, `key` or `no-key`; `None` when no
/// top-level line sets it.
pub fn switch(text: &str, key: &str) -> Option<bool> {
    let top: Vec<(usize, &[u8])> = split(text.as_bytes()).top_lines().collect();
    top.into_iter().rev().find_map(|(_, line)| {
        let read = key_line(line)?;
        if read.value.is_some() {
            return None;
        }
        match read.key {
            k if k == key.as_bytes() => Some(true),
            k if k.strip_prefix(b"no-") == Some(key.as_bytes()) => Some(false),
            _ => None,
        }
    })
}

/// Makes `edits` to the top level of the vtium.conf `text`, in order, and
/// returns the whole file with only the lines they need changed.
pub fn edit_conf(text: &str, edits: &[Edit<'_>]) -> Result<String, ConfEdit> {
    // a byte-order mark is no part of the first line, as the split reads
    // it, and stays the file's first bytes whatever line goes first
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let mut file = lines(body);
    // the lines above the first heading; an added line stays above it
    let parts = split(text.as_bytes());
    let mut top_count = parts.top_lines().count();
    let sections = parts.first_section.is_some();
    // a new line ends as the file's first line does
    let newline = match file.first().map(|l| l.ending) {
        Some("\r\n") => "\r\n",
        _ => "\n",
    };
    for edit in edits {
        let (key, switch) = match *edit {
            Edit::Set(key, _) => (key, false),
            Edit::Switch(key, _) => (key, true),
            Edit::Unset(key) => (key, false),
        };
        if !good_key(key) {
            return Err(ConfEdit::BadKey(key.to_owned()));
        }
        let new = match *edit {
            Edit::Set(_, value) if round_trips(key, value) => Some(value.to_owned()),
            Edit::Set(_, value) => {
                return Err(ConfEdit::BadValue {
                    key: key.to_owned(),
                    value: value.to_owned(),
                });
            }
            Edit::Switch(_, on) => Some(if on {
                key.to_owned()
            } else {
                format!("no-{key}")
            }),
            Edit::Unset(_) => None,
        };
        let top_lines = file.get_mut(..top_count).unwrap_or_default();
        let Some(new) = new else {
            for line in top_lines.iter_mut() {
                if sets(&line.text, key, false) || sets(&line.text, key, true) {
                    line.text.insert(0, '#');
                }
            }
            continue;
        };
        if let Some(line) = top_lines
            .iter_mut()
            .rev()
            .find(|l| sets(&l.text, key, switch))
        {
            let s = scan(line.text.as_bytes());
            let range = match (s, switch) {
                (Some(s), false) => s.value,
                (Some(s), true) => Some(s.key),
                (None, _) => None,
            };
            if let Some(range) = range {
                line.text.replace_range(range, &new);
            }
            continue;
        }
        let whole = if switch { new } else { format!("{key}={new}") };
        let below = top_lines
            .iter()
            .position(|l| commented(&l.text, key, switch));
        // the line a new key goes after: its commented line, else the end
        // of a file with no section, else the last top-level line holding
        // a key, else the last top-level line, so it stays above the first
        // heading; with none, the key opens the file
        let after = match below {
            Some(at) => Some(at),
            None if !sections => top_count.checked_sub(1),
            None => top_lines
                .iter()
                .rposition(|l| scan(l.text.as_bytes()).is_some())
                .or_else(|| top_count.checked_sub(1)),
        };
        let ending = after
            .and_then(|at| file.get(at))
            .map_or(newline, |l| match l.ending {
                "" => newline,
                end => end,
            });
        if let Some(l) = after.and_then(|at| file.get_mut(at))
            && l.ending.is_empty()
        {
            l.ending = newline;
        }
        let at = after.map_or(0, |at| at.saturating_add(1));
        file.insert(
            at.min(file.len()),
            Line {
                text: whole,
                ending,
            },
        );
        top_count = top_count.saturating_add(1);
    }
    let edited: String = file
        .iter()
        .map(|l| format!("{}{}", l.text, l.ending))
        .collect();
    Ok(format!("{bom}{edited}"))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn read(line: &str) -> Option<(String, Option<String>)> {
        key_line(line.as_bytes()).map(|l| {
            (
                String::from_utf8(l.key.to_vec()).unwrap(),
                l.value.map(|v| String::from_utf8(v.into_owned()).unwrap()),
            )
        })
    }

    fn pair(k: &str, v: Option<&str>) -> Option<(String, Option<String>)> {
        Some((k.to_owned(), v.map(str::to_owned)))
    }

    // ADR-1403 D4: one comment rule for the whole of vtium.conf.
    #[test]
    fn a_hash_starts_a_comment_only_at_the_start_or_after_a_blank() {
        assert_eq!(read("font-size=12 # big"), pair("font-size", Some("12")));
        assert_eq!(read("font-size=12#big"), pair("font-size", Some("12#big")));
        assert_eq!(read("login = cmd --x#1"), pair("login", Some("cmd --x#1")));
        assert_eq!(read("login = a \\#b # c"), pair("login", Some("a #b")));
        assert_eq!(read("login = a\\#b"), pair("login", Some("a\\#b")));
        assert_eq!(read("term = #a0b1c2 # c"), pair("term", Some("#a0b1c2")));
        assert_eq!(read("term=#a0b1c2"), pair("term", Some("#a0b1c2")));
        assert_eq!(read("term = #a0b1c"), pair("term", Some("")));
        assert_eq!(read("term=#a0b1c"), pair("term", Some("#a0b1c")));
        assert_eq!(read("bell # on"), pair("bell", None));
        assert_eq!(read("# font-size=1"), None);
        assert_eq!(read("   "), None);
        assert_eq!(read("=x"), pair("", Some("x")));
    }

    #[test]
    fn the_c_reference_s_reading_ends_a_value_at_any_hash() {
        let split =
            |l: &str| split_line(l.as_bytes()).map(|(k, v)| (k.to_vec(), v.map(<[u8]>::to_vec)));
        assert_eq!(
            split("sb-size=10#comment"),
            Some((b"sb-size".to_vec(), Some(b"10".to_vec())))
        );
    }
}
