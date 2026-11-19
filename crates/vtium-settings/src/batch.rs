//! The batch: the file that lists the resources vtium may draw from, one
//! absolute path on each line.
//!
//! vtium draws only from the active resources, the ones the batch lists.
//! A font, a color map, a shader, a palette or a pointer theme that the
//! batch does not hold counts as missing. The batch file is
//! `/etc/vtium/batch.conf`, set at build time with `VTIUM_BATCH`. vtium
//! reads it under the same owner and mode check as vtium.conf, so only
//! root can change what is active.
//!
//! A line holds one absolute path. A line that starts with `#` is a
//! comment, blank lines are allowed, and spaces around a path are
//! dropped. vtium ignores a line that does not hold an absolute path
//! without `..`, and reports it.
//!
//! When the batch file is missing, as on a machine upgraded from an
//! earlier vtium, the batch is the list of a fresh install:
//! [`Batch::fresh_install`]. vtium also uses that list when the batch
//! file fails the owner and mode check or is too large, because a batch
//! that anyone else could have written says nothing about what root
//! chose. The built-in font is always active and never appears in the
//! file. [`Batch::holds`] answers whether a path is in the batch.
//!
//! References:
//! - ADR-1205 D4: the batch, its file, the fresh-install list, the note of 2026-11-18 on a missing file, and the note of 2026-11-19 on where the console font is found and on a batch file vtium cannot use.
//! - ADR-1205 D10: the palettes the package ships are active from the first start.
//! - ADR-1401 D10: the owner and mode check the batch file passes, as vtium.conf does.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::Trust;
use crate::error::{Problem, Untrusted};

pub use crate::bounds::BATCH_MAX;

mod vconsole;
pub use vconsole::{Vconsole, vconsole};

/// The batch file, set at build time with `VTIUM_BATCH`.
pub const BATCH_FILE: &str = match option_env!("VTIUM_BATCH") {
    Some(path) => path,
    None => "/etc/vtium/batch.conf",
};

/// The folder where the package installs its font files, whatever the
/// install prefix is.
pub const PACKAGE_FONTS: &str = "/usr/share/vtium/fonts";

/// The font files the package installs in [`PACKAGE_FONTS`] and a fresh
/// install activates: Terminus in each size and weight vtium ships, GNU
/// Unifont, and Cullet Mono's two faces. The Makefile's `FONT_FILE_NAMES`
/// and `CULLET_MONO_FACES` install the same names, which a test checks.
pub const FRESH_FONTS: [&str; 21] = [
    "ter-u12n.bdf.gz",
    "ter-u12b.bdf.gz",
    "ter-u14n.bdf.gz",
    "ter-u14b.bdf.gz",
    "ter-u16n.bdf.gz",
    "ter-u16b.bdf.gz",
    "ter-u18n.bdf.gz",
    "ter-u18b.bdf.gz",
    "ter-u20n.bdf.gz",
    "ter-u20b.bdf.gz",
    "ter-u22n.bdf.gz",
    "ter-u22b.bdf.gz",
    "ter-u24n.bdf.gz",
    "ter-u24b.bdf.gz",
    "ter-u28n.bdf.gz",
    "ter-u28b.bdf.gz",
    "ter-u32n.bdf.gz",
    "ter-u32b.bdf.gz",
    "unifont_all-17.0.03.hex",
    "CulletMono-Regular.ttf",
    "CulletMono-Bold.ttf",
];

/// The longest line vtium reads as a path: `PATH_MAX`, the longest path
/// the kernel takes, with its terminating zero.
const PATH_MAX: usize = 4096;

/// The folders where kbd keeps its console fonts on the distributions vtium
/// knows: Arch's and Fedora's, the older `/lib` one, and Debian's. A console
/// font that vconsole.conf names by kbd's name is active only in these.
pub const CONSOLE_FONT_FOLDERS: [&str; 4] = [
    "/usr/share/kbd/consolefonts",
    "/usr/lib/kbd/consolefonts",
    "/lib/kbd/consolefonts",
    "/usr/share/consolefonts",
];

/// The endings vtium tries after a console font's name: none, and then
/// the suffixes kbd's console fonts carry, longest first, which a file
/// name loses to give the name vconsole.conf's `FONT=` uses.
const CONSOLE_FONT_SUFFIXES: [&str; 11] = [
    "", ".psfu.gz", ".psf.gz", ".psfu", ".psf", ".cp.gz", ".fnt.gz", ".cp", ".fnt", ".gz", ".bz2",
];

/// One problem with a line of the batch file. vtium ignores the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineFault {
    /// The line does not hold an absolute path.
    NotAbsolute,
    /// The path holds `..`.
    ParentStep,
    /// The line is longer than the longest path the kernel takes.
    TooLong,
    /// The line is not UTF-8.
    NotUtf8,
}

impl fmt::Display for LineFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LineFault::NotAbsolute => "this line does not hold an absolute path, one that starts with /",
            LineFault::ParentStep => "this path holds `..`, so vtium cannot tell which folder it names without following it",
            LineFault::TooLong => "this line is longer than the longest path Linux takes",
            LineFault::NotUtf8 => "this line is not UTF-8 text",
        })
    }
}

/// What vtium found while reading the batch, which `vtium check-config`
/// shows as a note or an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// The batch file does not exist, so the batch is the fresh-install
    /// list.
    Missing { path: PathBuf },
    /// The batch file could not be read, failed the owner and mode
    /// check, or is larger than [`BATCH_MAX`]: the batch is the
    /// fresh-install list.
    Refused { path: PathBuf, why: Unread },
    /// A line vtium ignores, numbered from 1.
    Line {
        path: PathBuf,
        line: usize,
        fault: LineFault,
    },
}

impl fmt::Display for Finding {
    /// The finding as `vtium check-config` shows it: the file first, then
    /// what vtium does and what to do.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::Missing { path } => write!(
                f,
                "{}: the list of active fonts and effects does not exist, so vtium uses the list of a fresh install: the built-in font, Cullet Mono, Terminus, Unifont, the console font that vconsole.conf names, and the color schemes vtium ships. To keep that list, write those files' paths into {}, one on each line, as root.",
                path.display(),
                path.display()
            ),
            Finding::Refused { path, why } => write!(
                f,
                "{}: vtium ignores this list of active fonts and effects and uses the list of a fresh install, because {why}",
                path.display()
            ),
            Finding::Line { path, line, fault } => write!(
                f,
                "{}:{line}: {fault}. Because of this, vtium ignores the line.",
                path.display()
            ),
        }
    }
}

/// Why vtium does not use the batch file it found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unread {
    /// The file or a folder above it fails the owner and mode check.
    Untrusted(Untrusted),
    /// The file is larger than [`BATCH_MAX`] bytes.
    TooLarge,
    /// The path names something other than a regular file.
    NotAFile,
    /// vtium could not read the file.
    Unreadable(std::io::ErrorKind),
}

impl From<Problem> for Unread {
    fn from(problem: Problem) -> Unread {
        match problem {
            Problem::Untrusted(why) => Unread::Untrusted(why),
            Problem::TooLarge => Unread::TooLarge,
            Problem::Unreadable(kind) => Unread::Unreadable(kind),
            _ => Unread::NotAFile,
        }
    }
}

impl fmt::Display for Unread {
    /// What follows "because" in [`Finding::Refused`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unread::Untrusted(why) => write!(f, "{}", why.reason()),
            Unread::TooLarge => write!(
                f,
                "the file is longer than {BATCH_MAX} bytes, the most vtium reads. Remove the lines it does not need."
            ),
            Unread::NotAFile => f.write_str("the path does not name a regular file."),
            Unread::Unreadable(kind) => {
                write!(
                    f,
                    "vtium cannot read the file: {kind}. Check that root can read it."
                )
            }
        }
    }
}

/// One entry of the batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A resource by its absolute path.
    Path(PathBuf),
}

/// The batch: the resources vtium may draw from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    entries: Vec<Entry>,
}

/// The batch file's verdict: the batch, and what vtium found reading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// The batch vtium uses.
    pub batch: Batch,
    /// The findings, in the file's order.
    pub findings: Vec<Finding>,
}

/// A path as a batch line gives it: absolute, with no `..`, and with `.`
/// and doubled slashes dropped.
fn plain(text: &str) -> Result<PathBuf, LineFault> {
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(LineFault::NotAbsolute);
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(LineFault::ParentStep);
    }
    Ok(path.components().collect())
}

impl Batch {
    /// The batch listing `entries`.
    pub fn new(entries: Vec<Entry>) -> Batch {
        Batch { entries }
    }

    /// The entries, in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Reads a batch file's bytes. `path` names the file in the findings.
    pub fn parse(bytes: &[u8], path: &Path) -> Read {
        let mut entries = Vec::new();
        let mut findings = Vec::new();
        for (n, raw) in bytes.split(|&b| b == b'\n').enumerate() {
            let line = n.saturating_add(1);
            let mut fault = |fault| {
                findings.push(Finding::Line {
                    path: path.to_path_buf(),
                    line,
                    fault,
                });
            };
            if raw.len() > PATH_MAX {
                fault(LineFault::TooLong);
                continue;
            }
            let Ok(text) = std::str::from_utf8(raw) else {
                fault(LineFault::NotUtf8);
                continue;
            };
            let text = text.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            match plain(text) {
                Ok(p) => entries.push(Entry::Path(p)),
                Err(f) => fault(f),
            }
        }
        Read {
            batch: Batch { entries },
            findings,
        }
    }

    /// The list of a fresh install: the font files the package ships in
    /// `fonts` ([`FRESH_FONTS`]), the console font `console_font` that
    /// vconsole.conf's `FONT=` names, and every file in `palettes`, the
    /// folder of the palettes the package ships. The built-in font is
    /// always active besides.
    ///
    /// vconsole.conf names the console font by an absolute path or by
    /// kbd's name, such as `ter-v16n`. vtium looks for a font by kbd's
    /// name in `console_font_folders`, normally [`CONSOLE_FONT_FOLDERS`],
    /// and lists each regular file there whose name is the font's name
    /// with one of kbd's suffixes or none. A file of that name in any
    /// other folder is not active.
    pub fn fresh_install(
        fonts: &Path,
        palettes: &Path,
        console_font: Option<&str>,
        console_font_folders: &[&Path],
    ) -> Batch {
        let mut entries: Vec<Entry> = FRESH_FONTS
            .iter()
            .map(|f| Entry::Path(fonts.join(f)))
            .collect();
        if let Some(font) = console_font.filter(|f| !f.is_empty()) {
            if let Ok(path) = plain(font) {
                entries.push(Entry::Path(path));
            } else {
                entries.extend(console_fonts(font, console_font_folders).map(Entry::Path));
            }
        }
        let mut shipped: Vec<PathBuf> = std::fs::read_dir(palettes)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .collect();
        shipped.sort();
        entries.extend(shipped.into_iter().map(Entry::Path));
        Batch { entries }
    }

    /// Reads the batch file at `path` under `trust`, the check vtium.conf
    /// passes: the file, read to at most [`BATCH_MAX`] bytes, or the
    /// fresh-install list `fresh` when the file is missing, fails the
    /// check, is too large or cannot be read. Each case but a clean read
    /// adds a finding.
    pub fn read(path: &Path, trust: &Trust, fresh: Batch) -> Read {
        match crate::file::read_limited(path, trust, BATCH_MAX) {
            Ok(Some(bytes)) => Batch::parse(&bytes, path),
            Ok(None) => Read {
                batch: fresh,
                findings: vec![Finding::Missing {
                    path: path.to_path_buf(),
                }],
            },
            Err(rejected) => Read {
                batch: fresh,
                findings: vec![Finding::Refused {
                    path: path.to_path_buf(),
                    why: rejected
                        .errors
                        .into_iter()
                        .next()
                        .map_or(Unread::NotAFile, |e| e.problem.into()),
                }],
            },
        }
    }

    /// Whether the batch holds the resource at `path`: an entry of the
    /// same path once `.` and doubled slashes are dropped. A relative
    /// path, one holding `..`, or one that is not UTF-8, which no line of
    /// the batch file can name, is never in the batch.
    pub fn holds(&self, path: &Path) -> bool {
        let Some(Ok(path)) = path.to_str().map(plain) else {
            return false;
        };
        self.entries.iter().any(|Entry::Path(p)| *p == path)
    }
}

/// The regular files in `folders` that are kbd's console font `name`: the
/// file named `name`, and each file named `name` with one of kbd's
/// suffixes. A name that is not one plain file name, such as one holding
/// `/` or `..`, names no file.
fn console_fonts<'a>(name: &'a str, folders: &'a [&Path]) -> impl Iterator<Item = PathBuf> + 'a {
    let plain_name = !name.contains('/') && name != "." && name != "..";
    folders
        .iter()
        .filter(move |_| plain_name)
        .flat_map(move |folder| {
            CONSOLE_FONT_SUFFIXES
                .iter()
                .map(move |suffix| folder.join(format!("{name}{suffix}")))
        })
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
}
