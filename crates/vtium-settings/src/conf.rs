//! vtium.conf edited line by line (ADR-1201 D5), so that a file a person
//! wrote keeps its comments, blank lines, order and line endings after a
//! tool has changed it.
//!
//! The file's syntax is the console's: one key a line, `key=value`, or a
//! switch's `key` to turn it on and `no-key` to turn it off; `#` starts a
//! comment anywhere but where it opens a `#rrggbb` color value; spaces,
//! tabs and a carriage return around the key and the value are dropped.
//! When a key is set on more than one line, vtium takes the last.
//!
//! [`edit_conf`] makes a list of [`Edit`]s:
//!
//! - A key that is set has its value replaced on the line vtium takes it
//!   from, with the blanks and any comment after it kept.
//! - A key that is not set but has a commented line, as `#font-size=0` in
//!   the shipped example, gets its new line directly below that comment.
//! - Any other new key is added at the end of the file.
//! - [`Edit::Unset`] comments out every line that sets the key, so the key
//!   falls back to its default and each line is there to set again.
//! - A switch is written `key` or `no-key`.
//!
//! The editor knows the syntax and not the keys: which keys exist and what
//! values they take is the console's key table, and `vtium check-config`
//! judges an edited file before it is installed.

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
struct Split {
    key: std::ops::Range<usize>,
    value: Option<std::ops::Range<usize>>,
}

fn blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r')
}

/// `range` of `line` with blanks dropped from both ends.
fn trimmed(line: &[u8], range: std::ops::Range<usize>) -> std::ops::Range<usize> {
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

/// One line's key and value, as the console reads them; `None` for a
/// blank or comment line. `line` holds no newline.
fn split(line: &[u8]) -> Option<Split> {
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
    (!key.is_empty() || value.is_some()).then_some(Split { key, value })
}

/// The text of a line's key, or empty.
fn key_of<'a>(line: &'a str, s: &Split) -> &'a str {
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
    split(line.as_bytes()).is_some_and(|s| {
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

/// Whether the console reads `key=value` back as that key and that value.
fn round_trips(key: &str, value: &str) -> bool {
    let line = format!("{key}={value}");
    !value.contains(['\n', '\0'])
        && split(line.as_bytes()).is_some_and(|s| {
            key_of(&line, &s) == key
                && s.value
                    .and_then(|v| line.get(v))
                    .is_some_and(|v| v == value && !v.is_empty())
        })
}

/// The value vtium takes for `key` from the file `text`, the value on the
/// last line that sets it; `None` when no line sets it.
pub fn setting(text: &str, key: &str) -> Option<String> {
    lines(text).iter().rev().find_map(|l| {
        let s = split(l.text.as_bytes())?;
        (key_of(&l.text, &s) == key)
            .then(|| s.value.and_then(|v| l.text.get(v)).map(str::to_owned))
            .flatten()
    })
}

/// Makes `edits` to the vtium.conf `text`, in order, and returns the
/// whole file with only the lines they need changed.
pub fn edit_conf(text: &str, edits: &[Edit<'_>]) -> Result<String, ConfEdit> {
    let mut file = lines(text);
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
        let Some(new) = new else {
            for line in file.iter_mut() {
                if sets(&line.text, key, false) || sets(&line.text, key, true) {
                    line.text.insert(0, '#');
                }
            }
            continue;
        };
        if let Some(line) = file.iter_mut().rev().find(|l| sets(&l.text, key, switch)) {
            let s = split(line.text.as_bytes());
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
        let below = file.iter().position(|l| commented(&l.text, key, switch));
        match below {
            Some(at) => {
                let ending = file.get(at).map_or(newline, |l| match l.ending {
                    "" => newline,
                    end => end,
                });
                if let Some(l) = file.get_mut(at)
                    && l.ending.is_empty()
                {
                    l.ending = newline;
                }
                file.insert(
                    at.saturating_add(1),
                    Line {
                        text: whole,
                        ending,
                    },
                );
            }
            None => {
                if let Some(last) = file.last_mut()
                    && last.ending.is_empty()
                {
                    last.ending = newline;
                }
                file.push(Line {
                    text: whole,
                    ending: newline,
                });
            }
        }
    }
    Ok(file
        .iter()
        .map(|l| format!("{}{}", l.text, l.ending))
        .collect())
}
