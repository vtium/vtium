//! The configuration: `kmscon.conf` in the C reference's key syntax,
//! `<seat>.seat.conf` over it, and the command line over both.
//!
//! [`keys`] holds the one table of keys. [`load`] reads every source and
//! gives the [`Config`] and the [`Report`]s of what it could not take: a
//! key it does not know, reported once however often it appears, a value
//! out of its bound, clamped, and a value it cannot read, ignored with the
//! key left as the sources below it set it. Nothing in a file stops the
//! rest of the file being read.
//!
//! A file is untrusted input and every size is bounded: [`FILE_MAX`] bytes
//! of a file are read, a line holds at most [`LINE_MAX`] bytes, a text
//! value [`TEXT_MAX`], and at most [`REPORTS_MAX`] reports are kept.

mod cmdline;
#[cfg(test)]
mod doc;
mod file;
pub mod keys;
mod value;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub use keys::{KEYS, Key, Kind, Scope, TEXT_MAX};

/// The most bytes of a configuration file read; the rest is ignored and
/// reported.
pub const FILE_MAX: usize = 64 * 1024;

/// The most bytes of one line of a configuration file; a longer line is
/// ignored and reported.
pub const LINE_MAX: usize = 8 * 1024;

/// The most reports kept; past it they are counted.
pub const REPORTS_MAX: usize = 64;

/// A key's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Switch(bool),
    Number(u64),
    /// A [`Kind::Text`] or [`Kind::Choice`] value.
    Text(String),
    /// A command's words, passed to it as given.
    Command(Vec<OsString>),
    Rgb([u8; 3]),
    Modes(Vec<ModeSpec>),
}

/// One `mode` line: the mode, and the card and connector it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeSpec {
    /// The card's udev `ID_PATH`, or any card.
    pub gpu: Option<String>,
    /// The connector's name (`HDMI-A-1`), or any connector.
    pub connector: Option<String>,
    pub width: u32,
    pub height: u32,
    /// The refresh rate in Hz, or any.
    pub rate: Option<u32>,
}

impl ModeSpec {
    /// The mode configured for `connector` on the card at `gpu` among
    /// `modes`: one naming both first, then one naming the connector,
    /// then one naming neither (`src/config.c:663-698`).
    pub fn find<'a>(
        modes: &'a [ModeSpec],
        gpu: Option<&str>,
        connector: &str,
    ) -> Option<&'a ModeSpec> {
        let both = |m: &&ModeSpec| {
            m.gpu.is_some() && m.gpu.as_deref() == gpu && m.connector.as_deref() == Some(connector)
        };
        let connector_only =
            |m: &&ModeSpec| m.gpu.is_none() && m.connector.as_deref() == Some(connector);
        let any = |m: &&ModeSpec| m.gpu.is_none() && m.connector.is_none();
        modes
            .iter()
            .find(both)
            .or_else(|| modes.iter().find(connector_only))
            .or_else(|| modes.iter().find(any))
    }
}

/// Where a value or a report came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The key table's default.
    Default,
    /// A line of a file, counted from 1.
    File { path: PathBuf, line: usize },
    /// An argument on the command line, counted from 1 after the program's
    /// name.
    CommandLine { arg: usize },
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Default => write!(f, "default"),
            Origin::File { path, line: 0 } => write!(f, "{}", path.display()),
            Origin::File { path, line } => write!(f, "{}:{line}", path.display()),
            Origin::CommandLine { arg } => write!(f, "argument {arg}"),
        }
    }
}

/// What a report says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No key has this name.
    UnknownKey(String),
    /// The key is set on the command line only.
    CommandLineOnly(&'static str),
    /// The key needs a value and was given none.
    NeedsValue(&'static str),
    /// A switch was given a value.
    TakesNoValue(&'static str),
    /// A number key's value is not a whole number.
    NotANumber { key: &'static str, value: String },
    /// A number outside the key's bound, clamped to it.
    Clamped {
        key: &'static str,
        value: String,
        used: u64,
    },
    /// A value not among the key's choices.
    NotAChoice { key: &'static str, value: String },
    /// A text value longer than [`TEXT_MAX`] bytes.
    TooLong(&'static str),
    /// A text value holding a control character.
    ControlCharacter(&'static str),
    /// A `login` command with an open quote, a trailing backslash, or no
    /// word.
    BadCommand {
        key: &'static str,
        why: &'static str,
    },
    /// A colour or a mode that does not read.
    BadValue {
        key: &'static str,
        value: String,
        why: &'static str,
    },
    /// A line with no key before its `=`.
    NoKey,
    /// A line holding a NUL byte.
    Nul,
    /// A line or an argument that is not UTF-8.
    NotUtf8,
    /// A line longer than [`LINE_MAX`] bytes.
    LineTooLong,
    /// The file is longer than [`FILE_MAX`] bytes; the rest was not read.
    FileTooLong,
    /// The file exists and could not be read.
    Unreadable(io::ErrorKind),
    /// The path names something other than a regular file, a FIFO or a
    /// device, which is not read.
    NotAFile,
    /// A command-line argument that is no option.
    StrayArgument(String),
    /// `--login` with no command after `--`.
    LoginWithoutCommand,
    /// The seat's name cannot name a file, so its file is not read.
    BadSeatName(String),
    /// This many reports more were not kept.
    More(usize),
}

/// The longest stretch of a value a report quotes.
const QUOTE_MAX: usize = 64;

/// `text` as a report quotes it: escaped, at most [`QUOTE_MAX`]
/// characters.
fn quote(text: &str) -> String {
    let mut out: String = text.chars().take(QUOTE_MAX).collect();
    if text.chars().nth(QUOTE_MAX).is_some() {
        out.push('…');
    }
    format!("\"{}\"", out.escape_debug())
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::UnknownKey(k) => write!(f, "unknown key {}, ignored", quote(k)),
            Problem::CommandLineOnly(k) => {
                write!(f, "{k} is set on the command line only, ignored")
            }
            Problem::NeedsValue(k) => write!(f, "{k} needs a value, ignored"),
            Problem::TakesNoValue(k) => write!(f, "{k} takes no value, ignored"),
            Problem::NotANumber { key, value } => {
                write!(f, "{key}={} is not a whole number, ignored", quote(value))
            }
            Problem::Clamped { key, value, used } => {
                write!(f, "{key}={} is out of bounds, {used} used", quote(value))
            }
            Problem::NotAChoice { key, value } => {
                let choices = match keys::find(key).map(|k| k.kind) {
                    Some(Kind::Choice(c)) => c.join(", "),
                    _ => String::new(),
                };
                write!(f, "{key}={} is none of {choices}, ignored", quote(value))
            }
            Problem::TooLong(k) => write!(f, "{k} is longer than {TEXT_MAX} bytes, ignored"),
            Problem::ControlCharacter(k) => {
                write!(f, "{k} holds a control character, ignored")
            }
            Problem::BadCommand { key, why } => write!(f, "{key}: {why}, ignored"),
            Problem::BadValue { key, value, why } => {
                write!(f, "{key}={}: {why}, ignored", quote(value))
            }
            Problem::NoKey => write!(f, "a line with no key, ignored"),
            Problem::Nul => write!(f, "a line holding a NUL byte, ignored"),
            Problem::NotUtf8 => write!(f, "not UTF-8, ignored"),
            Problem::LineTooLong => write!(f, "a line longer than {LINE_MAX} bytes, ignored"),
            Problem::FileTooLong => {
                write!(f, "longer than {FILE_MAX} bytes; the rest is ignored")
            }
            Problem::Unreadable(e) => write!(f, "cannot be read ({e}), ignored"),
            Problem::NotAFile => write!(f, "is not a regular file, ignored"),
            Problem::StrayArgument(a) => write!(f, "{} is no option, ignored", quote(a)),
            Problem::LoginWithoutCommand => {
                write!(f, "--login needs a command after --, ignored")
            }
            Problem::BadSeatName(s) => {
                write!(f, "the seat {} names no file, its file not read", quote(s))
            }
            Problem::More(n) => write!(f, "{n} more reports"),
        }
    }
}

/// Something the configuration could not take as given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub origin: Origin,
    pub problem: Problem,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.problem)
    }
}

/// The reports so far, each unknown key once, at most [`REPORTS_MAX`].
#[derive(Debug, Default)]
pub(crate) struct Reports {
    list: Vec<Report>,
    unknown: BTreeSet<String>,
    dropped: usize,
}

impl Reports {
    pub(crate) fn push(&mut self, origin: Origin, problem: Problem) {
        if let Problem::UnknownKey(name) = &problem {
            if !self.unknown.insert(name.clone()) {
                return;
            }
        }
        if self.list.len() < REPORTS_MAX {
            self.list.push(Report { origin, problem });
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    fn finish(mut self) -> Vec<Report> {
        if self.dropped > 0 {
            self.list.push(Report {
                origin: Origin::Default,
                problem: Problem::More(self.dropped),
            });
        }
        self.list
    }
}

/// A value one source gives a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub(crate) key: &'static Key,
    pub(crate) value: Value,
    pub(crate) origin: Origin,
}

/// Every key's value and where it came from.
#[derive(Clone, Debug)]
pub struct Config {
    values: BTreeMap<&'static str, (Option<Value>, Origin)>,
    /// The keys whose value was asked for, which a test holds against
    /// the table: each key a file sets is read or listed as not built.
    read: RefCell<BTreeSet<&'static str>>,
}

impl PartialEq for Config {
    fn eq(&self, other: &Config) -> bool {
        self.values == other.values
    }
}

impl Eq for Config {}

impl std::default::Default for Config {
    /// Every key at its default.
    fn default() -> Config {
        let values = KEYS
            .iter()
            .map(|k| (k.name, (value::default(k), Origin::Default)))
            .collect();
        Config {
            values,
            read: RefCell::default(),
        }
    }
}

impl Config {
    /// Sets a key. A mode line adds to the modes its own kind of source
    /// set: a file's to the files', the command line's to the command
    /// line's, which replace the files'.
    fn set(&mut self, a: Assignment) {
        if let Value::Modes(new) = &a.value {
            if let Some((Some(Value::Modes(old)), origin)) = self.values.get_mut(a.key.name) {
                let cmdline = |o: &Origin| matches!(o, Origin::CommandLine { .. });
                if cmdline(origin) == cmdline(&a.origin) {
                    old.extend(new.iter().cloned());
                    return;
                }
            }
        }
        self.values.insert(a.key.name, (Some(a.value), a.origin));
    }

    fn get(&self, key: &Key) -> Option<&Value> {
        self.read.borrow_mut().insert(key.name);
        self.values.get(key.name).and_then(|(v, _)| v.as_ref())
    }

    /// The names of the keys whose value was asked for so far.
    pub fn read(&self) -> BTreeSet<&'static str> {
        self.read.borrow().clone()
    }

    /// A switch's value; false for a key that is no switch.
    pub fn switch(&self, key: &Key) -> bool {
        matches!(self.get(key), Some(Value::Switch(true)))
    }

    /// A number key's value, if set.
    pub fn number(&self, key: &Key) -> Option<u64> {
        match self.get(key) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// A text or choice key's value, if set.
    pub fn text(&self, key: &Key) -> Option<&str> {
        match self.get(key) {
            Some(Value::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// A command key's words, if set.
    pub fn command(&self, key: &Key) -> Option<&[OsString]> {
        match self.get(key) {
            Some(Value::Command(c)) => Some(c),
            _ => None,
        }
    }

    /// A colour key's value, if set.
    pub fn rgb(&self, key: &Key) -> Option<[u8; 3]> {
        match self.get(key) {
            Some(Value::Rgb(c)) => Some(*c),
            _ => None,
        }
    }

    /// The `mode` lines, none when unset.
    pub fn modes(&self, key: &Key) -> &[ModeSpec] {
        match self.get(key) {
            Some(Value::Modes(m)) => m,
            _ => &[],
        }
    }

    /// Where the key's value came from.
    pub fn origin(&self, key: &Key) -> Option<&Origin> {
        self.values.get(key.name).map(|(_, o)| o)
    }
}

/// What [`load`] gives: the configuration, and what it could not take.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub reports: Vec<Report>,
    /// The command line names the subcommand `check-config`.
    pub check: bool,
}

impl Loaded {
    /// The command line cannot start vtium: it holds a word that is no
    /// option, or `--login` with no command after `--`. The C reference
    /// refused both (`src/misc/conf.c:310-313`, `src/config.c:439-450`).
    pub fn refuses_start(&self) -> bool {
        self.reports.iter().any(|r| {
            matches!(r.origin, Origin::CommandLine { .. })
                && matches!(
                    r.problem,
                    Problem::StrayArgument(_) | Problem::LoginWithoutCommand
                )
        })
    }
}

/// The configuration files under `dir`, the lowest precedence first: the
/// main file, then the seat's. The decision on vtium's own file
/// (vtium/vtium#21) changes this function and nothing else.
pub fn files(dir: &Path, seat: Option<&str>) -> Vec<PathBuf> {
    let mut out = vec![dir.join("kmscon.conf")];
    if let Some(seat) = seat {
        out.push(dir.join(format!("{seat}.seat.conf")));
    }
    out
}

/// A seat's name as logind allows it, which names a file under the
/// configuration directory and nothing outside it.
fn seat_name_ok(seat: &str) -> bool {
    const SUFFIX: usize = ".seat.conf".len();
    !seat.is_empty()
        && seat.len().saturating_add(SUFFIX) <= 255
        && seat
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Reads the configuration: `args` (the command line after the program's
/// name), then the files [`files`] names under `configdir`, which `args`
/// may set, for `seat`. `read` gives a file's bytes, or `None` when it does
/// not exist, and needs to give no more than [`FILE_MAX`] and one.
///
/// The command line wins over the seat's file, and the seat's file over the
/// main one.
pub fn load(
    args: &[OsString],
    seat: Option<&str>,
    read: &mut dyn FnMut(&Path) -> io::Result<Option<Vec<u8>>>,
) -> Loaded {
    let mut reports = Reports::default();
    let cmdline::Parsed {
        assignments: given,
        check,
    } = cmdline::parse(args, &mut reports);

    let mut config = Config::default();
    if let Some(dir) = given
        .iter()
        .rev()
        .find(|a| a.key.name == keys::CONFIGDIR.name)
    {
        config.set(dir.clone());
    }
    let dir = PathBuf::from(config.text(&keys::CONFIGDIR).unwrap_or_default());
    let seat = seat.filter(|s| {
        let ok = seat_name_ok(s);
        if !ok {
            reports.push(Origin::Default, Problem::BadSeatName((*s).to_owned()));
        }
        ok
    });

    for path in files(&dir, seat) {
        let origin = Origin::File {
            path: path.clone(),
            line: 0,
        };
        let mut bytes = match read(&path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => continue,
            Err(e) => {
                let problem = if e.get_ref().is_some_and(|e| e.is::<NotAFile>()) {
                    Problem::NotAFile
                } else {
                    Problem::Unreadable(e.kind())
                };
                reports.push(origin, problem);
                continue;
            }
        };
        if bytes.len() > FILE_MAX {
            bytes.truncate(FILE_MAX);
            reports.push(origin, Problem::FileTooLong);
        }
        for a in file::parse(&bytes, &path, &mut reports) {
            config.set(a);
        }
    }
    for a in given {
        config.set(a);
    }
    Loaded {
        config,
        reports: reports.finish(),
        check,
    }
}

/// The error [`read_file`] gives for a path that is no regular file.
#[derive(Debug)]
struct NotAFile;

impl fmt::Display for NotAFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a regular file")
    }
}

impl std::error::Error for NotAFile {}

/// Reads `path` for [`load`]: `None` when it does not exist, and at most
/// [`FILE_MAX`] and one bytes. It is opened as the C reference opened it,
/// so that a tty never becomes the controlling one (`O_NOCTTY`,
/// `src/misc/conf.c:533-590`), and without blocking, so that a FIFO is
/// refused at once with anything else that is not a regular file.
pub fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::open(path, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(e) if e == rustix::io::Errno::NOENT => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let file = std::fs::File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, NotAFile));
    }
    let mut bytes = Vec::new();
    let limit = u64::try_from(FILE_MAX)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}
