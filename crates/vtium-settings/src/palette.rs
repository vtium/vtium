//! Palette files: the color schemes a look can name beside the palettes
//! built into vtium-term.
//!
//! A palette sets the terminal's colors: the 16 ANSI colors, the default
//! foreground and background, and where the file gives them, the cursor's
//! color and the selection's colors. Programs name those colors by number,
//! and vtium resolves each cell through the palette when it draws. A
//! palette file is `NAME.palette` in one of two folders. The package
//! installs its color schemes in [`PACKAGE_PALETTES`], and an
//! administrator's palettes go in [`PALETTES`], whose file wins when both
//! folders hold the same name. On screen, vtium and vtium-config call a
//! palette a color scheme, the word people who have themed another
//! terminal already know.
//!
//! The file is Ghostty's theme format as it is, so a scheme written for
//! Ghostty imports unchanged: `key = value` lines, comments that start
//! with `#`, and blank lines. [`parse`] reads `palette` (`N=COLOR`, N from
//! 0 to 255), `foreground`, `background`, `cursor-color`,
//! `selection-background` and `selection-foreground`. A `COLOR` is
//! `#rrggbb`, `rrggbb` or an X11 color name, read as OSC 4 reads one. In
//! the last three keys, `cell-foreground` and `cell-background` leave the
//! color unset, so vtium draws the cursor or the selection by inverting
//! the cell. Each color the file leaves unset keeps `legacy`'s value. When
//! a key appears twice, the later line wins. Every other key is ignored
//! and listed once as a note, and vtium never follows a `config-file` or
//! `theme` line, so a palette file sets colors and nothing else.
//!
//! A line that is not `key = value`, a color vtium cannot read, an index
//! past 255 and a file over [`PALETTE_FILE_MAX`] bytes are errors. Any
//! error makes the whole file count as missing.
//!
//! References:
//! - ADR-1205 D10: what a palette is, its two folders, its name rule, its format, its errors and its bound.
//! - ADR-1205 D4: a palette file is a resource under the trust rule.
//! - ADR-1304 D4: the name rule a palette's name shares with a vtlook's.

use std::fmt;
use std::path::{Path, PathBuf};

use vtium_term::{PALETTE_NAMES, Palette, Rgb, Special};

use crate::bounds::{ERRORS_MAX, PALETTE_FILE_MAX};
use crate::error::{Problem, Quoted, Rejected, Untrusted};
use crate::file::{Trust, read_limited};
use crate::split::BOM;

/// The administrator's palette folder: set at build time with
/// `VTIUM_PALETTES`. Its file wins over the package's on the same name.
pub const PALETTES: &str = match option_env!("VTIUM_PALETTES") {
    Some(dir) => dir,
    None => "/etc/vtium/palettes",
};

/// The folder the package installs its color schemes in: set at build
/// time with `VTIUM_PACKAGE_PALETTES`.
pub const PACKAGE_PALETTES: &str = match option_env!("VTIUM_PACKAGE_PALETTES") {
    Some(dir) => dir,
    None => "/usr/share/vtium/palettes",
};

/// The folders vtium reads palette files from, in the order a name is
/// looked up: the administrator's first.
pub const FOLDERS: [&str; 2] = [PALETTES, PACKAGE_PALETTES];

/// The suffix of a palette file's name.
pub const SUFFIX: &str = ".palette";

/// The palette that takes its colors from vtium.conf's `palette-*` keys.
pub const CUSTOM: &str = "custom";

/// Whether `name` names a palette that needs no file: one of the palettes
/// vtium-term holds, or [`CUSTOM`]. No palette file may take such a name.
pub fn built_in(name: &str) -> bool {
    name == CUSTOM || PALETTE_NAMES.contains(&name)
}

/// Whether `name` can name a palette file: 1 to 64 of the characters
/// `A-Z a-z 0-9 . _ + -`, the rule for a vtlook's name, so it never holds
/// a path.
pub fn named(name: &str) -> bool {
    crate::parse::good_name(name)
}

/// A key the file sets that vtium does not read, at the first line that
/// sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ignored {
    pub key: Quoted,
    pub line: usize,
}

/// A palette file that reads cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub name: String,
    pub file: PathBuf,
    pub palette: Palette,
    /// Each key vtium ignores, once, in the file's order.
    pub ignored: Vec<Ignored>,
}

/// Why a palette file counts as missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A line that is not `key = value`.
    NotKeyValue,
    /// A color vtium cannot read, the value of `key`.
    BadColor { key: &'static str, value: Quoted },
    /// A `palette` value that is not `N=COLOR`.
    BadEntry(Quoted),
    /// A `palette` index past 255.
    PastLastColor(Quoted),
    /// Bytes that are not UTF-8.
    NotText,
    /// A file larger than [`PALETTE_FILE_MAX`].
    TooLarge,
    /// A file that takes the name of a palette that needs no file.
    Shadows(&'static str),
    /// A file the trust rule refuses.
    Untrusted(Untrusted),
    /// A path that is not a regular file.
    NotAFile,
    /// A file the system cannot read.
    Unreadable(std::io::ErrorKind),
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::NotKeyValue => f.write_str(
                "This line is not a setting. Each line of a color scheme reads key = value, or is a comment that starts with #.",
            ),
            Reason::BadColor { key, value } => write!(
                f,
                "The color {value} of {key} is not one vtium can read. A color is #rrggbb, rrggbb or an X11 color name such as DarkSlateGray."
            ),
            Reason::BadEntry(value) => write!(
                f,
                "The palette line {value} does not read N=COLOR. N is the number of a color from 0 to 255."
            ),
            Reason::PastLastColor(index) => write!(
                f,
                "The color number {index} is past 255, the last of the 256 colors."
            ),
            Reason::NotText => f.write_str("The file is not UTF-8 text."),
            Reason::TooLarge => write!(
                f,
                "The file is larger than {PALETTE_FILE_MAX} bytes, the most a color scheme may hold."
            ),
            Reason::Shadows(name) => write!(
                f,
                "The file takes the name of the built-in color scheme {name}, so vtium never reads it. Give the file another name."
            ),
            Reason::Untrusted(why) => {
                write!(f, "vtium does not read the file, because {}", why.reason())
            }
            Reason::NotAFile => f.write_str("The color scheme is not a regular file."),
            Reason::Unreadable(kind) => write!(f, "vtium cannot read the file: {kind}."),
        }
    }
}

/// One error of a palette file: its line, `None` for the whole file, and
/// why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub line: Option<usize>,
    pub reason: Reason,
}

/// A palette file that counts as missing: the first errors in the file's
/// order, at most [`ERRORS_MAX`], and the count of the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub name: String,
    pub file: PathBuf,
    pub faults: Vec<Fault>,
    pub more: usize,
}

impl Failure {
    fn one(name: &str, file: &Path, reason: Reason) -> Failure {
        Failure {
            name: name.to_owned(),
            file: file.to_path_buf(),
            faults: vec![Fault { line: None, reason }],
            more: 0,
        }
    }

    /// Where the first error is: the file, and its line when it has one.
    pub fn at(&self) -> String {
        match self.faults.first().and_then(|f| f.line) {
            Some(line) => format!("{}:{line}", self.file.display()),
            None => self.file.display().to_string(),
        }
    }

    /// The line vtium logs once for the failure: the scheme, where its
    /// first error is and why, the count of the other errors, and
    /// `instead`, which says what vtium draws in its place.
    pub fn log_line(&self, instead: &str) -> String {
        let why = self
            .faults
            .first()
            .map(|f| f.reason.to_string())
            .unwrap_or_default();
        let others = self
            .faults
            .len()
            .saturating_sub(1)
            .saturating_add(self.more);
        let more = match others {
            0 => String::new(),
            1 => " 1 more error.".to_owned(),
            n => format!(" {n} more errors."),
        };
        format!(
            "Color scheme {} ({}): {why}{more} {instead}",
            self.name,
            self.at()
        )
    }
}

/// A palette file's colors and the keys it ignores, or its errors.
pub type Parsed = Result<(Palette, Vec<Ignored>), (Vec<Fault>, usize)>;

/// Reads a palette file's bytes. The colors the file leaves unset keep
/// `legacy`'s values. `Err` holds the first [`ERRORS_MAX`] errors in the
/// file's order and the count of the rest.
pub fn parse(bytes: &[u8]) -> Parsed {
    if bytes.len() > PALETTE_FILE_MAX {
        let fault = Fault {
            line: None,
            reason: Reason::TooLarge,
        };
        return Err((vec![fault], 0));
    }
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            let before = bytes.get(..e.valid_up_to()).unwrap_or_default();
            let line = before
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                .saturating_add(1);
            let fault = Fault {
                line: Some(line),
                reason: Reason::NotText,
            };
            return Err((vec![fault], 0));
        }
    };
    let mut palette = Palette::default();
    let mut ignored: Vec<Ignored> = Vec::new();
    let mut faults = Vec::new();
    let mut more = 0_usize;
    for (at, line) in text.split('\n').enumerate() {
        let n = at.saturating_add(1);
        let line = line.strip_suffix('\r').unwrap_or(line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let result = match key_value(line) {
            Some((key, value)) => setting(&mut palette, key, value).map(|known| {
                if !known && ignored.iter().all(|i| i.key.raw() != key) {
                    ignored.push(Ignored {
                        key: Quoted::new(key),
                        line: n,
                    });
                }
            }),
            None => Err(Reason::NotKeyValue),
        };
        if let Err(reason) = result {
            if faults.len() < ERRORS_MAX {
                faults.push(Fault {
                    line: Some(n),
                    reason,
                });
            } else {
                more = more.saturating_add(1);
            }
        }
    }
    if faults.is_empty() {
        Ok((palette, ignored))
    } else {
        Err((faults, more))
    }
}

/// A line's key and value: the text before the first `=` and after it,
/// each trimmed, with one pair of double quotes around the value removed,
/// as Ghostty takes a quoted value. `None` when the line has no `=` or
/// its key is empty or holds a blank or a control character.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_graphic()) {
        return None;
    }
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    Some((key, value))
}

/// What a key vtium reads sets.
#[derive(Clone, Copy)]
enum Target {
    /// `palette`: one of the 256 colors.
    Entry,
    Fg,
    Bg,
    /// A color that `cell-foreground` and `cell-background` leave unset.
    Special(Special),
}

/// The keys vtium reads in a palette file.
const KEYS: [(&str, Target); 6] = [
    ("palette", Target::Entry),
    ("foreground", Target::Fg),
    ("background", Target::Bg),
    ("cursor-color", Target::Special(Special::Cursor)),
    (
        "selection-background",
        Target::Special(Special::SelectionBg),
    ),
    (
        "selection-foreground",
        Target::Special(Special::SelectionFg),
    ),
];

/// Sets what the line `key = value` sets in `palette`. `Ok(false)` for a
/// key vtium ignores.
fn setting(palette: &mut Palette, key: &str, value: &str) -> Result<bool, Reason> {
    let Some(&(key, target)) = KEYS.iter().find(|(k, _)| *k == key) else {
        return Ok(false);
    };
    match target {
        Target::Entry => {
            let bad = || Reason::BadEntry(Quoted::new(value));
            let (index, rest) = value.split_once('=').ok_or_else(bad)?;
            let index = index.trim();
            if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            let index = index
                .parse::<u8>()
                .map_err(|_| Reason::PastLastColor(Quoted::new(index)))?;
            palette.set_color(index, color(key, rest.trim())?);
        }
        Target::Fg => palette.set_fg(color(key, value)?),
        Target::Bg => palette.set_bg(color(key, value)?),
        Target::Special(which) => {
            let rgb = match value {
                "cell-foreground" | "cell-background" => None,
                _ => Some(color(key, value)?),
            };
            palette.set_special(which, rgb);
        }
    }
    Ok(true)
}

/// A color as a palette file writes it: `#rrggbb`, `rrggbb` or an X11
/// color name.
fn color(key: &'static str, value: &str) -> Result<Rgb, Reason> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    let rgb = if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        let channel = |at: usize| {
            hex.get(at..at.saturating_add(2))
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        };
        channel(0)
            .zip(channel(2))
            .zip(channel(4))
            .map(|((r, g), b)| Rgb::new(r, g, b))
    } else if value.starts_with('#') {
        None
    } else {
        Rgb::from_x11_name(value)
    };
    rgb.ok_or_else(|| Reason::BadColor {
        key,
        value: Quoted::new(value),
    })
}

/// Opens the palette file at `path` under `trust`: its bytes, at most
/// [`PALETTE_FILE_MAX`], or `None` when it does not exist.
///
/// This is the one place a palette file is opened, the seam where the
/// trust rule's full check and the batch, which says which resources are
/// active, take the place of the ownership walk vtium reads vtium.conf
/// with today.
///
/// References:
/// - ADR-1205 D4: the trust rule and the batch that a resource passes before it draws.
fn open(path: &Path, trust: &Trust) -> Result<Option<Vec<u8>>, Rejected> {
    read_limited(path, trust, PALETTE_FILE_MAX)
}

/// Reads the palette file `name` from the first of `folders` that holds
/// it, under `trust`. `Ok(None)` when no folder holds it, and for a name
/// no file can take. A file that takes a built-in palette's name is never
/// read, and is an error here. The first folder's file wins on a name
/// even when it has errors, so a broken file of the administrator's
/// counts as missing and never lets the package's file through.
pub fn read(name: &str, folders: &[&Path], trust: &Trust) -> Result<Option<Scheme>, Failure> {
    if !named(name) {
        return Ok(None);
    }
    for folder in folders {
        let file = folder.join(format!("{name}{SUFFIX}"));
        if let Some(shadowed) = shadowed(name) {
            if file.exists() {
                return Err(Failure::one(name, &file, Reason::Shadows(shadowed)));
            }
            continue;
        }
        let bytes = match open(&file, trust) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => continue,
            Err(rejected) => return Err(Failure::one(name, &file, refused(&rejected))),
        };
        return match parse(&bytes) {
            Ok((palette, ignored)) => Ok(Some(Scheme {
                name: name.to_owned(),
                file,
                palette,
                ignored,
            })),
            Err((faults, more)) => Err(Failure {
                name: name.to_owned(),
                file,
                faults,
                more,
            }),
        };
    }
    Ok(None)
}

/// The built-in palette `name` takes the name of, if any.
fn shadowed(name: &str) -> Option<&'static str> {
    PALETTE_NAMES
        .iter()
        .chain(std::iter::once(&CUSTOM))
        .find(|n| **n == name)
        .copied()
}

/// Every palette file in `folders` that takes the name of a palette that
/// needs no file, with the failure that says so. vtium never reads such a
/// file.
pub fn shadowing(folders: &[&Path]) -> Vec<Failure> {
    let mut found = Vec::new();
    for folder in folders {
        for name in PALETTE_NAMES.iter().chain(std::iter::once(&CUSTOM)) {
            let file = folder.join(format!("{name}{SUFFIX}"));
            if file.symlink_metadata().is_ok() {
                found.push(Failure::one(name, &file, Reason::Shadows(name)));
            }
        }
    }
    found
}

/// Why the walk refused a palette file.
fn refused(rejected: &Rejected) -> Reason {
    match rejected.errors.first().map(|e| &e.problem) {
        Some(Problem::Untrusted(why)) => Reason::Untrusted(why.clone()),
        Some(Problem::TooLarge) => Reason::TooLarge,
        Some(Problem::NotAFile) => Reason::NotAFile,
        Some(Problem::Unreadable(kind)) => Reason::Unreadable(*kind),
        _ => Reason::Unreadable(std::io::ErrorKind::Other),
    }
}

#[cfg(test)]
#[path = "palette/tests.rs"]
mod tests;
