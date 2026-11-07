//! The configuration: the top level of `vtium.conf`, in the key syntax of
//! `kmscon` configuration files, and the command line over it (ADR-1403
//! D2, D3).
//!
//! [`keys`] holds the one table of keys. [`load`] reads both sources and
//! gives the [`Config`] and the [`Report`]s of what it could not take: a
//! key it does not know, reported once however often it appears, a value
//! out of its bound, clamped, and a value it cannot read, ignored with the
//! key left as the sources below it set it. Nothing in the top level stops
//! the rest of it being read. vtium.conf's sections, the flow of ADR-1403,
//! are the head layer's to read: [`Loaded::file`] keeps the file as
//! [`load`] found it for that reading, and [`Config::for_head`] lays a
//! head's sections over the top level (D5).
//!
//! [`read_file`] reads vtium.conf only when the ownership check trusts it
//! (D3): a file root owns, or one the account vtium runs as owns, that
//! neither its group nor others can write, in directories that pass the
//! same check. vtium reads none of a file that fails and says why.
//!
//! A file is untrusted input and every size is bounded: [`FILE_MAX`] bytes
//! of a file are read, a line holds at most [`LINE_MAX`] bytes, a text
//! value [`TEXT_MAX`], and at most [`REPORTS_MAX`] reports are kept.

pub mod budget;
mod cmdline;
#[cfg(test)]
mod doc;
mod file;
pub mod keys;
mod palette;
mod value;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub use keys::{KEYS, Key, Kind, Scope, TEXT_MAX};

use vtium_render::DecoColor;

use crate::grab::Chord;

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
    /// A [`Kind::Text`], [`Kind::Path`], [`Kind::Choice`] or [`Kind::Char`]
    /// value.
    Text(String),
    /// A command's words, passed to it as given.
    Command(Vec<OsString>),
    Rgb([u8; 3]),
    Modes(Vec<ModeSpec>),
    /// A [`Kind::KeyBinding`] value.
    Chords(Vec<Chord>),
    /// A [`Kind::DecoColor`] value.
    DecoColor(DecoColor),
    /// A [`Kind::Size`] value: the width and the height.
    Size(u32, u32),
}

/// How the font helper hints outline glyphs, `font-hinting`'s value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hinting {
    /// `light`: each glyph fits the pixel grid up and down only.
    Light,
    /// `full`: each glyph fits the pixel grid both ways.
    Full,
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
    /// then one naming neither.
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
    /// The key is set in a section of vtium.conf only, never at its top
    /// level or on the command line, such as a look's `cell`.
    SectionOnly(&'static str),
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
    /// A color, a mode or a chord that does not read.
    BadValue {
        key: &'static str,
        value: String,
        why: &'static str,
    },
    /// A key binding binds a combination that switches the VT, which no key
    /// binds (ADR-1005 D10); the key keeps its other combinations.
    SwitchesVt { key: &'static str, chord: String },
    /// Two key bindings bind one combination: the one that comes first in the
    /// key order, `first`, takes it, and `then` keeps its others
    /// (ADR-1005 D10).
    BoundTwice {
        chord: String,
        first: &'static str,
        then: &'static str,
    },
    /// A key binding set by the name it had before the maintainer renamed
    /// it (ADR-1005 D10): vtium reads it as `new`. A source that sets both
    /// names takes the new one.
    Renamed {
        old: &'static str,
        new: &'static str,
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
    /// The path names something that is not a regular file, such as a
    /// FIFO, a device or a directory; it is not read.
    NotAFile,
    /// A command-line argument that is no option.
    StrayArgument(String),
    /// `--login` with no command after `--`.
    LoginWithoutCommand,
    /// The file fails the ownership check, so none of it is read
    /// (ADR-1403 D3).
    Untrusted(vtium_settings::Untrusted),
    /// This many reports more were not kept.
    More(usize),
}

impl Problem {
    /// The key the problem names, where it names one.
    pub fn key(&self) -> Option<&str> {
        match self {
            Problem::UnknownKey(k) => Some(k),
            Problem::CommandLineOnly(k)
            | Problem::SectionOnly(k)
            | Problem::NeedsValue(k)
            | Problem::TakesNoValue(k)
            | Problem::TooLong(k)
            | Problem::ControlCharacter(k)
            | Problem::NotANumber { key: k, .. }
            | Problem::Clamped { key: k, .. }
            | Problem::NotAChoice { key: k, .. }
            | Problem::BadCommand { key: k, .. }
            | Problem::BadValue { key: k, .. }
            | Problem::SwitchesVt { key: k, .. }
            | Problem::BoundTwice { then: k, .. }
            | Problem::Renamed { new: k, .. } => Some(k),
            Problem::NoKey
            | Problem::Nul
            | Problem::NotUtf8
            | Problem::LineTooLong
            | Problem::FileTooLong
            | Problem::Unreadable(_)
            | Problem::NotAFile
            | Problem::StrayArgument(_)
            | Problem::LoginWithoutCommand
            | Problem::Untrusted(_)
            | Problem::More(_) => None,
        }
    }
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
                write!(f, "{k} can be set only on the command line, ignored")
            }
            Problem::SectionOnly(k) => {
                write!(f, "{k} can be set only in a [look NAME] section, ignored")
            }
            Problem::NeedsValue(k) => write!(f, "{k} needs a value, ignored"),
            Problem::TakesNoValue(k) => write!(f, "{k} takes no value, ignored"),
            Problem::NotANumber { key, value } => {
                let or = match keys::find(key).map(|k| k.kind) {
                    Some(Kind::Number { word: Some(w), .. }) => format!(" or {w}"),
                    _ => String::new(),
                };
                write!(
                    f,
                    "{key}={} is not a whole number{or}, ignored",
                    quote(value)
                )
            }
            Problem::Clamped { key, value, used } => {
                write!(
                    f,
                    "{key}={} is out of range, so {used} is used",
                    quote(value)
                )
            }
            Problem::NotAChoice { key, value } => {
                let choices = match keys::find(key).map(|k| k.kind) {
                    Some(Kind::Choice(c)) => c.join(", "),
                    _ => String::new(),
                };
                write!(f, "{key}={} is not one of {choices}, ignored", quote(value))
            }
            Problem::TooLong(k) => write!(f, "{k} is longer than {TEXT_MAX} bytes, ignored"),
            Problem::ControlCharacter(k) => {
                write!(f, "{k} contains a control character, ignored")
            }
            Problem::BadCommand { key, why } => write!(f, "{key}: {why}, ignored"),
            Problem::BadValue { key, value, why } => {
                write!(f, "{key}={}: {why}, ignored", quote(value))
            }
            Problem::SwitchesVt { key, chord } => write!(
                f,
                "{key}: {chord} switches the VT, so vtium ignores this combination. No setting can bind Ctrl+Alt+F1 to F12 or the XF86Switch_VT keys"
            ),
            Problem::BoundTwice { chord, first, then } => write!(
                f,
                "{chord} is bound by both {first} and {then}. {first} keeps it, because vtium checks {first} first. To use it for {then}, remove it from {first}, or give {then} another combination"
            ),
            Problem::Renamed { old, new } => write!(
                f,
                "{old} is now called {new}. vtium reads {old} as {new} for now; rename it"
            ),
            Problem::NoKey => write!(f, "a line with no key, ignored"),
            Problem::Nul => write!(f, "a line with a NUL byte, ignored"),
            Problem::NotUtf8 => write!(f, "not UTF-8 text, ignored"),
            Problem::LineTooLong => write!(f, "a line longer than {LINE_MAX} bytes, ignored"),
            Problem::FileTooLong => {
                write!(f, "longer than {FILE_MAX} bytes, so the rest is ignored")
            }
            Problem::Unreadable(e) => write!(f, "cannot be read ({e}), ignored"),
            Problem::NotAFile => write!(f, "not a regular file, ignored"),
            Problem::StrayArgument(a) => write!(f, "unknown option {}, ignored", quote(a)),
            Problem::LoginWithoutCommand => {
                write!(f, "--login needs a command after --, ignored")
            }
            Problem::Untrusted(why) => write!(f, "{}", why.refusal()),
            Problem::More(n) => write!(f, "{n} more warnings"),
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
        if let Problem::UnknownKey(name) = &problem
            && !self.unknown.insert(name.clone())
        {
            return;
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
    /// A head's configuration ([`Config::for_head`]) is a clone that
    /// records the keys asked of it here too, so the record covers the
    /// keys every head reads.
    read: std::sync::Arc<std::sync::Mutex<BTreeSet<&'static str>>>,
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
            read: std::sync::Arc::default(),
        }
    }
}

impl Config {
    /// Sets a key. A mode line adds to the modes its own kind of source
    /// set: a file's to the files', the command line's to the command
    /// line's, which replace the files'.
    fn set(&mut self, a: Assignment) {
        if let Value::Modes(new) = &a.value
            && let Some((Some(Value::Modes(old)), origin)) = self.values.get_mut(a.key.name)
        {
            let cmdline = |o: &Origin| matches!(o, Origin::CommandLine { .. });
            if cmdline(origin) == cmdline(&a.origin) {
                old.extend(new.iter().cloned());
                return;
            }
        }
        self.values.insert(a.key.name, (Some(a.value), a.origin));
    }

    fn get(&self, key: &Key) -> Option<&Value> {
        if let Ok(mut read) = self.read.lock() {
            read.insert(key.name);
        }
        self.values.get(key.name).and_then(|(v, _)| v.as_ref())
    }

    /// Gives each key combination the key bindings bind to the key that comes
    /// first in the key order ([`crate::grab::ORDER`]), and drops each
    /// combination that switches the VT, with a report of each (ADR-1005
    /// D10). A report of a combination bound twice goes to the line of the
    /// key a source set, the later key's when both were set.
    fn resolve_bindings(&mut self, reports: &mut Reports) {
        let mut taken: Vec<(Chord, &'static str, Origin)> = Vec::new();
        for (key, _) in crate::grab::ORDER {
            let Some((Some(Value::Chords(chords)), origin)) = self.values.get_mut(key.name) else {
                continue;
            };
            let mut kept = Vec::new();
            for chord in std::mem::take(chords) {
                let written = crate::grab::written(&chord);
                if crate::grab::switches_vt(&chord) {
                    reports.push(
                        origin.clone(),
                        Problem::SwitchesVt {
                            key: key.name,
                            chord: written,
                        },
                    );
                } else if let Some((_, first, at)) = taken.iter().find(|(c, _, _)| *c == chord) {
                    if *first != key.name {
                        let at = if *origin == Origin::Default {
                            at.clone()
                        } else {
                            origin.clone()
                        };
                        reports.push(
                            at,
                            Problem::BoundTwice {
                                chord: written,
                                first,
                                then: key.name,
                            },
                        );
                    }
                } else {
                    taken.push((chord.clone(), key.name, origin.clone()));
                    kept.push(chord);
                }
            }
            *chords = kept;
        }
    }

    /// Sets the key binding `key` to `chords`, as a test's source would.
    #[cfg(test)]
    pub(crate) fn set_chords(&mut self, key: &'static Key, chords: Vec<Chord>) {
        self.values.insert(
            key.name,
            (Some(Value::Chords(chords)), Origin::CommandLine { arg: 1 }),
        );
    }

    /// Takes `keys` as `from` holds them, each value with where it came
    /// from; every other key keeps its own.
    pub fn take_keys(&mut self, from: &Config, keys: &[&Key]) {
        for key in keys {
            if let Some(v) = from.values.get(key.name) {
                self.values.insert(key.name, v.clone());
            }
        }
    }

    /// The names of the keys whose value was asked for so far.
    pub fn read(&self) -> BTreeSet<&'static str> {
        self.read.lock().map(|r| r.clone()).unwrap_or_default()
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

    /// A size key's width and height, if set.
    pub fn size(&self, key: &Key) -> Option<(u32, u32)> {
        match self.get(key) {
            Some(Value::Size(w, h)) => Some((*w, *h)),
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

    /// This configuration, the top level with the command line over it,
    /// with a head's section keys `settings` laid over it (ADR-1403 D5):
    /// the keys [`vtium_settings::Config::settings`] gives, each once at
    /// the most specific level that sets it, its head's section or its
    /// look's, since a section wins over the top level and the command
    /// line alike. Each value goes through the parser the top level's
    /// lines go through, with its line of `path`; the section reader has
    /// checked it against the same table, so a report here is one the
    /// section reader let pass, and the key keeps the top level's value.
    ///
    /// A look's `cell` reaches the head only where the same look sets no
    /// `font-size` other than 0: within one look, its `font-size` wins
    /// over its `cell` (ADR-1200 D16, ADR-1403 D5). Only a look sets
    /// `cell`, so a `cell` in the configuration this gives is the look's,
    /// and it wins over the top level's `font-size` ([`crate::head`]).
    pub fn for_head(
        &self,
        path: &Path,
        settings: &[vtium_settings::Setting],
    ) -> (Config, Vec<Report>) {
        let mut reports = Reports::default();
        let mut config = self.clone();
        let mut sized = false;
        for setting in settings {
            let origin = Origin::File {
                path: path.to_owned(),
                line: setting.line,
            };
            if let Some(value) = value::parse(setting.key, &setting.value, &origin, &mut reports) {
                if setting.key.name == keys::FONT_SIZE.name {
                    sized = value != Value::Number(0);
                }
                config.set(Assignment {
                    key: setting.key,
                    value,
                    origin,
                });
            }
        }
        if sized {
            config.values.remove(keys::CELL.name);
        }
        (config, reports.finish())
    }

    /// A color key's value, if set.
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

    /// The chords a key binding binds, none when unset.
    pub fn chords(&self, key: &Key) -> &[Chord] {
        match self.get(key) {
            Some(Value::Chords(c)) => c,
            _ => &[],
        }
    }

    /// A character key's value, if set.
    pub fn char(&self, key: &Key) -> Option<char> {
        self.text(key).and_then(|t| t.chars().next())
    }

    /// A decoration color key's value, the foreground when unset.
    pub fn deco_color(&self, key: &Key) -> DecoColor {
        match self.get(key) {
            Some(Value::DecoColor(c)) => *c,
            _ => DecoColor::Foreground,
        }
    }

    /// `display-buffers`: how many buffers each DRM display keeps, 1 to
    /// 3, or `None` when no source sets it and the build decides
    /// (ADR-1401 D2).
    pub fn display_buffers(&self) -> Option<u8> {
        self.text(&keys::DISPLAY_BUFFERS)
            .and_then(|t| t.parse().ok())
    }

    /// `font-hinting`: how the font helper hints outline glyphs, or
    /// `None` when no source sets it and the frit's `hinting=` decides
    /// (ADR-1200 D1).
    pub fn font_hinting(&self) -> Option<Hinting> {
        match self.text(&keys::FONT_HINTING) {
            Some("light") => Some(Hinting::Light),
            Some("full") => Some(Hinting::Full),
            _ => None,
        }
    }

    /// `font-line-spacing`: the height added to each row, a percentage
    /// of the text's height from 0 to 100, or `None` when no source sets
    /// it and the frit's `line-spacing=` decides (ADR-1302 D13).
    pub fn font_line_spacing(&self) -> Option<u8> {
        self.number(&keys::FONT_LINE_SPACING)
            .and_then(|n| u8::try_from(n).ok())
    }

    /// Where the key's value came from.
    pub fn origin(&self, key: &Key) -> Option<&Origin> {
        self.values.get(key.name).map(|(_, o)| o)
    }
}

/// vtium.conf as [`load`] found it, for the reading of its sections, the
/// flow of ADR-1403, which the head layer makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum File {
    /// There is no vtium.conf: vtium runs its compiled-in flow.
    Missing,
    /// vtium.conf fails the ownership check, and none of it is read
    /// (ADR-1403 D3).
    Untrusted(vtium_settings::Untrusted),
    /// vtium.conf cannot be read as a file.
    Inaccessible,
    /// vtium.conf's bytes, at most [`FILE_MAX`] and one, the top level's
    /// and the sections' alike.
    Read(Vec<u8>),
}

/// What [`load`] gives: the configuration, and what it could not take.
#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub reports: Vec<Report>,
    /// The command line names the subcommand `check-config`.
    pub check: bool,
    /// The vtium.conf the command line gives `check-config` to check in
    /// place of the installed one.
    pub candidate: Option<PathBuf>,
    /// `check-config --no-devices`: the rules are judged alone, without
    /// the devices present (ADR-1403 D3).
    pub no_devices: bool,
    /// The vtium.conf read: the candidate, else `vtium.conf` in the
    /// configuration directory.
    pub path: PathBuf,
    /// vtium.conf as it was found.
    pub file: File,
}

impl Loaded {
    /// The command line cannot start vtium: it holds a word that is no
    /// option, or `--login` with no command after `--`.
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

/// The configuration file in the configuration directory `dir`:
/// `vtium.conf`, the one file vtium reads (ADR-1403 D2). The configuration
/// directory is `/etc/vtium` unless `--configdir` names another, and vtium
/// never reads `/etc/kmscon`, the released `kmscon`'s (ADR-1500 D4).
pub fn file(dir: &Path) -> PathBuf {
    dir.join("vtium.conf")
}

/// The vtium.conf the console reads under `config`: [`file`] in the
/// configuration directory `config` names.
pub fn path(config: &Config) -> PathBuf {
    file(Path::new(config.text(&keys::CONFIGDIR).unwrap_or_default()))
}

/// Reads the configuration: `args` (the command line after the program's
/// name), then the top level of vtium.conf, the file [`file`] names in the
/// configuration directory `args` may set, or the candidate `check-config`
/// is given. `read` gives a file's bytes, or `None` when it does not
/// exist, and needs to give no more than [`FILE_MAX`] and one; an error
/// that wraps [`NotTrusted`] is a file the ownership check fails, which
/// [`read_file`] gives.
///
/// The command line wins over the file.
pub fn load(
    args: &[OsString],
    read: &mut dyn FnMut(&Path) -> io::Result<Option<Vec<u8>>>,
) -> Loaded {
    let mut reports = Reports::default();
    let cmdline::Parsed {
        assignments: given,
        check,
        candidate,
        no_devices,
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
    let path = candidate.clone().unwrap_or_else(|| file(&dir));
    let origin = Origin::File {
        path: path.clone(),
        line: 0,
    };
    let found = match read(&path) {
        Ok(Some(bytes)) => File::Read(bytes),
        Ok(None) => File::Missing,
        Err(e) => {
            let inner = e.get_ref();
            if let Some(NotTrusted(why)) = inner.and_then(|e| e.downcast_ref::<NotTrusted>()) {
                reports.push(origin.clone(), Problem::Untrusted(why.clone()));
                File::Untrusted(why.clone())
            } else {
                let problem = if inner.is_some_and(|e| e.is::<NotAFile>()) {
                    Problem::NotAFile
                } else {
                    Problem::Unreadable(e.kind())
                };
                reports.push(origin.clone(), problem);
                File::Inaccessible
            }
        }
    };
    if let File::Read(bytes) = &found {
        let top = match bytes.get(..FILE_MAX) {
            Some(within) if bytes.len() > FILE_MAX => {
                reports.push(origin, Problem::FileTooLong);
                within
            }
            _ => bytes.as_slice(),
        };
        for a in file::parse(top, &path, &mut reports) {
            config.set(a);
        }
    }
    for a in given {
        config.set(a);
    }
    config.resolve_bindings(&mut reports);
    Loaded {
        config,
        reports: reports.finish(),
        check,
        candidate,
        no_devices,
        path,
        file: found,
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

/// The error [`read_file`] gives for a file the ownership check fails,
/// with why it fails.
#[derive(Debug)]
pub struct NotTrusted(pub vtium_settings::Untrusted);

impl fmt::Display for NotTrusted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.refusal())
    }
}

impl std::error::Error for NotTrusted {}

/// Reads `path` for [`load`] as the console reads vtium.conf: `None` when
/// it does not exist, and at most [`FILE_MAX`] and one bytes once the
/// ownership check trusts it (ADR-1403 D3). The check trusts root's file
/// and, for a vtium that runs as another user, as the tests and a run by
/// hand do, that user's own, where neither the file's group nor others
/// can write it or a directory above it. Anything but a regular file is
/// refused before it is opened, and the file is opened as
/// [`read_bytes`] opens it.
pub fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let trust = vtium_settings::Trust::running_as(rustix::process::geteuid().as_raw());
    let inspected = match vtium_settings::inspect(path, &trust) {
        Ok(Some(inspected)) => inspected,
        Ok(None) => return Ok(None),
        Err(rejected) => return Err(rejection(&rejected)),
    };
    match inspected.trusted {
        Ok(()) => Ok(Some(inspected.bytes)),
        Err(why) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            NotTrusted(why),
        )),
    }
}

/// Reads `path` whoever owns it: `None` when it does not exist, and at
/// most [`FILE_MAX`] and one bytes. It is opened with `O_NOCTTY`, so that
/// a tty never becomes the controlling one, and without blocking, so that
/// a FIFO is refused at once with anything else that is not a regular
/// file. It reads a frit, and the vtium.conf `vtium check-config` checks
/// before it is installed, whose ownership it judges apart.
pub fn read_bytes(path: &Path) -> io::Result<Option<Vec<u8>>> {
    use rustix::fs::{Mode, OFlags};
    use std::io::Read;
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

/// The error a refusal to open or read the file gives [`load`].
fn rejection(rejected: &vtium_settings::Rejected) -> io::Error {
    match rejected.errors.first().map(|e| &e.problem) {
        Some(vtium_settings::Problem::NotAFile) => {
            io::Error::new(io::ErrorKind::InvalidInput, NotAFile)
        }
        Some(vtium_settings::Problem::Unreadable(kind)) => io::Error::from(*kind),
        _ => io::Error::other("cannot be read"),
    }
}
