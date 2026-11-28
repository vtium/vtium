//! The parser: text to a [`Macro`], one line at a time, stopping at the
//! first line that does not parse.
//!
//! Each line is read on its own. A line may have spaces and tabs around
//! its words, and may end with a carriage return before its newline. The
//! first line may start with a byte-order mark. Step names, key names and
//! modifiers are spelled with a capital letter, as VHS spells them, and a
//! word in another case is refused with the right spelling.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Read};

use crate::bounds::{
    COUNT_MAX, FILE_MAX, LABEL_MAX, LINE_MAX, NAME_MAX, SLEEP_MAX, STEPS_MAX, TYPE_MAX,
};
use crate::{
    Character, Comment, Count, Expectation, Key, KeyPress, KeyStep, Label, Macro, Mods, Name, Node,
    Pause, QUOTES, Step, Text,
};

/// The first error in a macro: the number of its line, counted from 1,
/// and one sentence or two saying what the line should hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    line: usize,
    message: String,
}

impl Error {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Error {
        Error {
            line,
            message: message.into(),
        }
    }

    /// The number of the line, counted from 1.
    pub fn line(&self) -> usize {
        self.line
    }

    /// What is wrong with the line and what it should hold, without the
    /// line's number.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The error with the macro's file name, as
    /// `cullet-11.vtmacro:7: Clik is not a step. ...`.
    pub fn in_file<'a>(&'a self, file: &'a str) -> impl fmt::Display + 'a {
        InFile { file, error: self }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

struct InFile<'a> {
    file: &'a str,
    error: &'a Error,
}

impl fmt::Display for InFile<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.file, self.error.line, self.error.message
        )
    }
}

/// The most bytes [`Parser::read`] takes for one line: the line, a
/// byte-order mark, a carriage return and the newline. A line that reaches
/// it without a newline is too long, and the bytes after it stay unread.
const READ_MAX: usize = LINE_MAX.saturating_add("\u{feff}\r\n".len());

/// Reads a macro one line at a time, as `--macro -` reads lines from
/// standard input as they arrive. It counts the lines, so each line is
/// checked against the step count, and it checks that `Set Live` comes
/// before any step that acts and that no two screenshots share a name.
///
/// A stream is bounded by [`STEPS_MAX`] lines of [`LINE_MAX`] bytes,
/// and not by a file's [`FILE_MAX`], because the parser keeps only one
/// line of it at a time.
///
/// References:
/// - ADR-1208 D1: the step count and the line length bound `--macro -`.
/// - ADR-1208 D2: Set Live first, and each screenshot's name used once.
#[derive(Clone, Debug, Default)]
pub struct Parser {
    lines: usize,
    /// The bytes read so far, counted only for a file.
    bytes: Option<usize>,
    acted: bool,
    /// Each screenshot's name, with the line that used it.
    names: BTreeMap<String, usize>,
    /// Set by [`Parser::read`] at its first error, after which the stream
    /// ends.
    stopped: bool,
}

impl Parser {
    /// A parser at the start of a stream.
    pub fn new() -> Parser {
        Parser::default()
    }

    /// A parser at the start of a file, which also holds the macro to
    /// [`FILE_MAX`] bytes.
    pub(crate) fn file() -> Parser {
        Parser {
            bytes: Some(0),
            ..Parser::default()
        }
    }

    /// Reads the next line from `r` into its step, or `None` once the
    /// stream has ended. It reads at most one line, and no more than a line
    /// can hold, so a line too long fails without the rest of it being
    /// read. After an error the stream ends, and the next call gives
    /// `None`.
    pub fn read<R: BufRead>(&mut self, r: &mut R) -> Option<Result<Step, Error>> {
        if self.stopped {
            return None;
        }
        let mut buf = Vec::new();
        let read = r
            .by_ref()
            .take(u64::try_from(READ_MAX).unwrap_or(u64::MAX))
            .read_until(b'\n', &mut buf);
        let step = match read {
            Ok(0) => return None,
            Ok(_) => {
                let raw = buf.strip_suffix(b"\n").unwrap_or(&buf);
                self.line(raw)
            }
            Err(e) => {
                let n = self.lines.saturating_add(1);
                Err(Error::new(n, format!("This line cannot be read: {e}.")))
            }
        };
        self.stopped = step.is_err();
        Some(step)
    }

    /// Reads the next line, given without its newline, into its step.
    pub fn line(&mut self, raw: &[u8]) -> Result<Step, Error> {
        let n = self.count(raw.len())?;
        let raw = if n == 1 {
            raw.strip_prefix("\u{feff}".as_bytes()).unwrap_or(raw)
        } else {
            raw
        };
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.len() > LINE_MAX {
            return Err(Error::new(
                n,
                format!("This line is longer than {LINE_MAX} bytes, the most a line holds."),
            ));
        }
        let text =
            std::str::from_utf8(raw).map_err(|_| Error::new(n, "This line is not UTF-8 text."))?;
        if text.chars().any(|c| c.is_control() && c != '\t') {
            return Err(Error::new(
                n,
                "This line holds a control character other than a tab.",
            ));
        }
        let step = step(text).map_err(|m| Error::new(n, m))?;
        self.check(n, &step)?;
        Ok(step)
    }

    /// Takes a step that is already built, as [`Macro::new`] does, and
    /// checks it as if its canonical line had been read.
    pub(crate) fn accept(&mut self, step: &Step) -> Result<(), Error> {
        let len = step.to_string().len();
        let n = self.count(len)?;
        if len > LINE_MAX {
            return Err(Error::new(
                n,
                format!("This line is longer than {LINE_MAX} bytes, the most a line holds."),
            ));
        }
        self.check(n, step)
    }

    /// Counts one more line of `len` bytes and its newline, and returns
    /// its number.
    fn count(&mut self, len: usize) -> Result<usize, Error> {
        let n = self.lines.saturating_add(1);
        self.lines = n;
        self.bytes = self.bytes.map(|b| b.saturating_add(len).saturating_add(1));
        if self.bytes.is_some_and(|b| b > FILE_MAX) {
            let mib = FILE_MAX / (1024 * 1024);
            return Err(Error::new(
                n,
                format!("The macro is larger than {mib} MiB, the most a macro holds."),
            ));
        }
        if n > STEPS_MAX {
            return Err(Error::new(
                n,
                format!("The macro has more than {STEPS_MAX} lines, the most a macro holds."),
            ));
        }
        Ok(n)
    }

    /// Checks what a line cannot know alone: that `Set Live` comes before
    /// any step that acts, and that a screenshot's name is new.
    fn check(&mut self, n: usize, step: &Step) -> Result<(), Error> {
        if *step == Step::SetLive && self.acted {
            return Err(Error::new(
                n,
                "Set Live must be the first step. Only comments and blank lines may come before it.",
            ));
        }
        if let Step::Screenshot(name) = step {
            if let Some(first) = self.names.get(name.as_str()) {
                return Err(Error::new(
                    n,
                    format!(
                        "Screenshot {} uses the name of the Screenshot on line {first}, so its file would overwrite that one. Choose another name.",
                        name.as_str()
                    ),
                ));
            }
            self.names.insert(name.as_str().to_owned(), n);
        }
        self.acted |= step.acts();
        Ok(())
    }
}

/// Reads a macro's text, stopping at the first line that does not parse.
pub fn parse(text: &str) -> Result<Macro, Error> {
    parse_bytes(text.as_bytes())
}

/// Reads a macro from bytes, which need not be UTF-8: a line that is not
/// UTF-8 is an error on that line. The bound on the macro's size counts a
/// newline after each line, the last one included.
pub fn parse_bytes(data: &[u8]) -> Result<Macro, Error> {
    // Bytes past the bound are never read: the line that reaches past it
    // fails on the size alone.
    let data = data.get(..FILE_MAX.saturating_add(1)).unwrap_or(data);
    if data.is_empty() {
        return Ok(Macro::default());
    }
    let body = data.strip_suffix(b"\n").unwrap_or(data);
    let mut parser = Parser::file();
    let mut steps = Vec::new();
    for line in body.split(|&b| b == b'\n') {
        steps.push(parser.line(line)?);
    }
    Ok(Macro { steps })
}

/// The spaces and tabs that separate the words of a line.
const SPACE: [char; 2] = [' ', '\t'];

/// What a step can be, for the message about a word that is not one.
const STEPS: &str = "A step is a key, Type, Click, Expect, Sleep, Screenshot or Set Live.";

/// What a key can be, for the message about a word that is not one.
const KEYS: &str = "A key is Enter, Escape, Tab, Space, Backspace, an arrow, PageUp, PageDown, Home, End, F1 to F12, or one character.";

/// The steps of VHS's tapes that macros do not have.
const VHS_STEPS: [&str; 9] = [
    "Output", "Require", "Hide", "Show", "Wait", "Source", "Env", "Copy", "Paste",
];

/// The keys of VHS's tapes that macros do not have.
const VHS_KEYS: [&str; 2] = ["Delete", "Insert"];

/// The modifiers, in the order the canonical text writes them.
const MODS: [&str; 3] = ["Ctrl", "Alt", "Shift"];

/// The first word of a line and the words after it.
fn split_word(s: &str) -> (&str, &str) {
    match s.split_once(SPACE) {
        Some((word, rest)) => (word, rest.trim_start_matches(SPACE)),
        None => (s, ""),
    }
}

/// A word as a message quotes it, cut to 32 characters so the message
/// stays one short line.
fn shown(word: &str) -> String {
    match word.char_indices().nth(32) {
        Some((i, _)) => format!("{}…", word.get(..i).unwrap_or(word)),
        None => word.to_owned(),
    }
}

/// A number written in decimal digits alone, with no sign, or `None`.
/// Digits past what a `u64` holds read as `u64::MAX`.
fn digits(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().unwrap_or(u64::MAX))
}

/// One line, with its control characters already refused, as a step.
fn step(line: &str) -> Result<Step, String> {
    let text = line.trim_start_matches(SPACE);
    if let Some(comment) = text.strip_prefix('#') {
        return Comment::new(comment)
            .map(Step::Comment)
            .ok_or_else(|| "This comment cannot be read.".to_owned());
    }
    let text = text.trim_end_matches(SPACE);
    if text.is_empty() {
        return Ok(Step::Blank);
    }
    let (word, rest) = split_word(text);
    match word {
        "Type" => {
            let text = quoted(
                rest,
                "Type",
                "Type needs its text in quotes, as in Type \"cullet\".",
                "text",
            )?;
            let len = text.chars().count();
            if text.contains('\t') {
                Err("The text after Type holds a tab. Press Tab as a key step instead.".to_owned())
            } else if len > TYPE_MAX {
                Err(format!(
                    "The text after Type is longer than {TYPE_MAX} characters, the most one step types."
                ))
            } else {
                Text::new(text)
                    .map(Step::Type)
                    .ok_or_else(|| "The text after Type cannot be typed.".to_owned())
            }
        }
        "Click" => match split_word(rest) {
            ("Mouse", rest) => label(
                rest,
                "Click Mouse",
                "Click Mouse needs a label in quotes, as in Click Mouse \"Save\".",
                "label",
            )
            .map(Step::ClickMouse),
            _ => label(
                rest,
                "Click",
                "Click needs a label in quotes, as in Click \"Save\".",
                "label",
            )
            .map(Step::Click),
        },
        "Expect" => expect(rest).map(Step::Expect),
        "Sleep" => sleep(rest).map(Step::Sleep),
        "Screenshot" => {
            if !Name::plain(rest) {
                return Err(format!(
                    "Screenshot needs a name for its file, as in Screenshot after-save. A name is up to {NAME_MAX} letters, digits, -, _ and ., and does not start with a dot."
                ));
            }
            Name::new(rest).map(Step::Screenshot).ok_or_else(|| {
                format!("Screenshot {rest} would overwrite a file that debug mode writes itself. Choose another name.")
            })
        }
        "Set" if rest == "Live" => Ok(Step::SetLive),
        "Set" => Err("Set takes only Live in a macro, as in Set Live.".to_owned()),
        _ => {
            let press =
                parse_key(word).map_err(|e| pace(word).unwrap_or_else(|| e.message(word)))?;
            let count = match rest {
                "" => Some(Count::ONE),
                _ => digits(rest)
                    .and_then(|n| u8::try_from(n).ok())
                    .and_then(Count::new),
            };
            let count = count.ok_or_else(|| {
                format!("A key takes only a count from 1 to {COUNT_MAX} after it, as in Down 3.")
            })?;
            KeyStep::new(press, count)
                .map(Step::Key)
                .map_err(|u| u.message(&shown(word)))
        }
    }
}

/// The message for a step's name with VHS's speed after it, as in
/// `Type@500ms`, or `None` for a word that is not one.
fn pace(word: &str) -> Option<String> {
    let (name, speed) = word.split_once('@')?;
    let step = ["Type", "Click", "Expect", "Sleep", "Screenshot", "Set"].contains(&name);
    let named = !name.is_empty() && !speed.is_empty() && (step || parse_key(name).is_ok());
    named.then(|| {
        format!(
            "{} gives the step a speed. A macro sets no speed for a step, because the player sets its own pace. Write {} without @{}.",
            shown(word),
            shown(name),
            shown(speed)
        )
    })
}

/// The text between the quotes after a step's name. `missing` is the
/// message for words with no quote, and `what` names the text in the
/// other messages.
fn quoted<'a>(rest: &'a str, step: &str, missing: &str, what: &str) -> Result<&'a str, String> {
    let mut chars = rest.chars();
    let q = match chars.next() {
        Some(q) if QUOTES.contains(&q) => q,
        _ => return Err(missing.to_owned()),
    };
    let (text, after) = chars
        .as_str()
        .split_once(q)
        .ok_or_else(|| format!("The {what} after {step} has no closing quote."))?;
    if !after.trim_start_matches(SPACE).is_empty() {
        return Err(format!("Only the text in quotes may follow {step}."));
    }
    if text.is_empty() {
        return Err(format!("The {what} after {step} is empty."));
    }
    Ok(text)
}

/// A label or a text that the player looks for on the screen.
fn label(rest: &str, step: &str, missing: &str, what: &str) -> Result<Label, String> {
    let text = quoted(rest, step, missing, what)?;
    if text.contains('\t') {
        return Err(format!(
            "The {what} after {step} holds a tab. Write the words with one space between each."
        ));
    }
    if text.chars().count() > LABEL_MAX {
        return Err(format!(
            "The {what} after {step} is longer than {LABEL_MAX} characters, the most a label holds."
        ));
    }
    Label::new(text).ok_or_else(|| {
        format!("The {what} after {step} has a space at its start or end, or two spaces in a row. The screen is read with one space between words, so it would never match.")
    })
}

/// The words after `Expect`.
fn expect(rest: &str) -> Result<Expectation, String> {
    if rest.starts_with(QUOTES) {
        return label(rest, "Expect", "", "text").map(Expectation::Shows);
    }
    match split_word(rest) {
        ("Not", rest) => label(rest, "Expect Not", "Expect Not needs a text in quotes, as in Expect Not \"problems\".", "text").map(Expectation::ShowsNot),
        ("At", rest) => Node::new(rest)
            .map(Expectation::At)
            .ok_or_else(|| format!("Expect At needs a place in the TUI, as in Expect At looks.look. A place is lowercase words joined by dots, at most {NAME_MAX} bytes.")),
        (word @ ("Saved" | "Unsaved"), rest) if !rest.is_empty() => Err(format!("Expect {word} takes nothing after it.")),
        ("Saved", _) => Ok(Expectation::Saved),
        ("Unsaved", _) => Ok(Expectation::Unsaved),
        ("Exited", rest) => digits(rest)
            .and_then(|n| u8::try_from(n).ok())
            .map(Expectation::Exited)
            .ok_or_else(|| "Expect Exited needs a status from 0 to 255, as in Expect Exited 0.".to_owned()),
        _ => Err("Expect takes a text in quotes, Not and a text, At and a place, Saved, Unsaved, or Exited and a status, as in Expect \"11 pt\".".to_owned()),
    }
}

/// The words after `Sleep`: a whole number of milliseconds, as in
/// `500ms`, or a number of seconds, as in `2s`, `0.5s` or a bare `2` or
/// `0.5` as VHS reads it. A number of seconds has at most three digits
/// after its point, because a pause counts whole milliseconds.
fn sleep(rest: &str) -> Result<Pause, String> {
    let form = || {
        "Sleep takes a number of seconds or a whole number of milliseconds, as in Sleep 2, Sleep 0.5s or Sleep 500ms."
            .to_owned()
    };
    let ms = match rest.strip_suffix("ms") {
        Some(n) => digits(n).ok_or_else(form)?,
        None => {
            let seconds = rest.strip_suffix('s').unwrap_or(rest);
            let (whole, frac) = seconds.split_once('.').unwrap_or((seconds, "000"));
            let whole = digits(whole).ok_or_else(form)?;
            let frac_ms = digits(frac).ok_or_else(form)?;
            if frac.len() > 3 {
                return Err("This Sleep is finer than a millisecond. Write at most three digits after the point, as in Sleep 0.25.".to_owned());
            }
            // "5" after the point is 500 ms, "25" is 250 ms.
            let scale = match frac.len() {
                1 => 100,
                2 => 10,
                _ => 1,
            };
            whole
                .saturating_mul(1000)
                .saturating_add(frac_ms.saturating_mul(scale))
        }
    };
    if ms == 0 {
        return Err("This Sleep lasts no time. A Sleep lasts at least 1 ms.".to_owned());
    }
    u32::try_from(ms)
        .ok()
        .and_then(Pause::from_millis)
        .ok_or_else(|| {
            format!(
                "This Sleep lasts longer than {} seconds, the longest a macro waits.",
                SLEEP_MAX.as_secs()
            )
        })
}

/// Why a word is not a key.
pub(crate) enum KeyError {
    /// The same modifier twice.
    Twice(&'static str),
    /// Modifiers and no key after them.
    NoKey,
    /// A word before a `+` that is not a modifier.
    NotModifier(String),
    /// A modifier on its own.
    Modifier,
    /// A word after the modifiers that is not a key.
    NotKey(String),
    /// A word with no modifier that is neither a key nor a step.
    NotStep,
}

impl KeyError {
    /// The message for `word`, the whole word on the line.
    pub(crate) fn message(&self, word: &str) -> String {
        let w = shown(word);
        match self {
            KeyError::Twice(m) => format!("{w} names {m} more than once."),
            KeyError::NoKey => {
                format!("{w} has no key after its last +. Write a key after it, as in Ctrl+S.")
            }
            KeyError::NotModifier(m) => format!(
                "{} is not a modifier. A key takes Ctrl, Alt and Shift, as in Ctrl+S.",
                shown(m)
            ),
            KeyError::Modifier => {
                format!("{w} is a modifier and needs a key after it, as in Ctrl+S.")
            }
            KeyError::NotKey(k) if VHS_KEYS.contains(&k.as_str()) => {
                format!("{k} is not a key that macros can press. {KEYS}")
            }
            KeyError::NotKey(k) => format!("{} is not a key. {KEYS}", shown(k)),
            KeyError::NotStep => not_step(word),
        }
    }
}

/// The message for a word with no modifier that is neither a key nor a
/// step.
fn not_step(word: &str) -> String {
    let w = shown(word);
    if VHS_STEPS.contains(&word) {
        return format!("{w} is a step of VHS's tapes that macros do not have. {STEPS}");
    }
    if VHS_KEYS.contains(&word) {
        return format!("{w} is not a key that macros can press. {KEYS}");
    }
    let names = ["Type", "Click", "Expect", "Sleep", "Screenshot", "Set"]
        .into_iter()
        .chain(Key::NAMED.iter().map(|(name, _)| *name))
        .chain(["Esc"])
        .chain(MODS);
    match names
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(word))
    {
        Some(right) => format!(
            "{w} is not a step. Steps and key names start with a capital letter, as in {right}."
        ),
        None => format!("{w} is not a step. {STEPS}"),
    }
}

/// A key with its modifiers, as in `Alt+Shift+L`, `Enter` or `q`. The key
/// after the last modifier is a key name or one character, so `Ctrl++` is
/// Ctrl and `+`.
pub(crate) fn parse_key(word: &str) -> Result<KeyPress, KeyError> {
    let mut mods = Mods::NONE;
    let mut rest = word;
    while let Some((name, after)) = MODS.iter().find_map(|m| {
        rest.strip_prefix(m)
            .and_then(|r| r.strip_prefix('+'))
            .map(|r| (*m, r))
    }) {
        let held = match name {
            "Ctrl" => &mut mods.ctrl,
            "Alt" => &mut mods.alt,
            _ => &mut mods.shift,
        };
        if *held {
            return Err(KeyError::Twice(name));
        }
        *held = true;
        rest = after;
    }
    if rest.is_empty() {
        return Err(KeyError::NoKey);
    }
    if let Some(key) = named(rest) {
        return Ok(KeyPress { mods, key });
    }
    let mut chars = rest.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Character::new(c)
            .map(|c| KeyPress {
                mods,
                key: Key::Char(c),
            })
            .ok_or_else(|| KeyError::NotKey(rest.to_owned()));
    }
    if let Some((before, _)) = rest
        .split_once('+')
        .filter(|(before, _)| !before.is_empty())
    {
        return Err(KeyError::NotModifier(before.to_owned()));
    }
    if mods.is_empty() {
        return Err(if MODS.contains(&rest) {
            KeyError::Modifier
        } else {
            KeyError::NotStep
        });
    }
    Err(KeyError::NotKey(rest.to_owned()))
}

/// The key a name names, `Esc` included.
fn named(name: &str) -> Option<Key> {
    match name {
        "Esc" => Some(Key::Escape),
        _ => Key::NAMED.iter().find(|(n, _)| *n == name).map(|(_, k)| *k),
    }
}
