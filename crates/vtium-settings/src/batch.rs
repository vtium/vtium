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
use crate::bounds::ERRORS_MAX;
use crate::error::{Problem, Untrusted};

pub use crate::bounds::BATCH_MAX;

mod vconsole;
pub use vconsole::{Vconsole, vconsole};

/// The batch file, set at build time with `VTIUM_BATCH`.
pub const BATCH_FILE: &str = match option_env!("VTIUM_BATCH") {
    Some(path) => path,
    None => "/etc/vtium/batch.conf",
};

/// The folder where the package installs its font files: set at build
/// time with `VTIUM_PACKAGE_FONTS`, so a console installed under another
/// prefix, such as `/usr/local`, activates the fonts where they really are.
pub const PACKAGE_FONTS: &str = match option_env!("VTIUM_PACKAGE_FONTS") {
    Some(dir) => dir,
    None => "/usr/share/vtium/fonts",
};

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

/// The suffixes kbd's `setfont` tries after a console font's name, in
/// order (`src/libkfont/context.c`). The font helper searches with the same
/// list, so the fresh-install list holds every file the helper can find.
pub const FONT_SUFFIXES: [&str; 5] = ["", ".psfu", ".psf", ".cp", ".fnt"];

/// The compressions `setfont` tries after each suffix, in order
/// (`kbdfile.c`).
pub const COMPRESSED: [&str; 5] = ["", ".gz", ".bz2", ".xz", ".zst"];

/// The pointer file the package installs: set at build time with
/// `VTIUM_PACKAGE_POINTERS`, as the font folder is with
/// `VTIUM_PACKAGE_FONTS`.
pub const PACKAGE_POINTERS: &str = match option_env!("VTIUM_PACKAGE_POINTERS") {
    Some(path) => path,
    None => "/usr/share/vtium/pointers.svg",
};

/// Why the font helper refuses a font file the batch does not list, as
/// what follows "because" in its reason.
pub const NOT_ACTIVE: &str = "it is not among the active fonts.";

/// What follows a refused font file's path in the font helper's reason,
/// before why vtium refuses the file. `vtium check-config` reads the path
/// and the why back from the helper's answer to a sources query.
pub const MISSING_BECAUSE: &str = ": vtium treats this file as missing, because ";

/// The most bytes vtium reads of vconsole.conf or the kernel command line.
const SMALL_FILE_MAX: u64 = 64 * 1024;

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
                "{}: The list of active fonts, color schemes, effects and pointer themes does not exist. vtium uses the list of a fresh install instead: the built-in font, Cullet Mono, Terminus, Unifont, the console font that vconsole.conf names, the color schemes vtium ships, and vtium's pointer file. To activate a resource, press Space on it in vtium-config's Fonts, Effects or Looks tab, or run vtium-config activate PATH, then install the change. vtium-config then writes this file with the fresh-install list and your change. If you write the file by hand instead, it replaces the fresh-install list, so list every path that stays active, vtium's own fonts, color schemes and pointer file included.",
                path.display()
            ),
            Finding::Refused { path, why } => write!(
                f,
                "{}: vtium ignores this list of active fonts, color schemes, effects and pointer themes and uses the list of a fresh install, because {why}",
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
    /// The count of the lines vtium ignores past the first [`ERRORS_MAX`],
    /// which `findings` does not list.
    pub more: usize,
}

impl Read {
    /// The line `vtium check-config` shows after the findings when vtium
    /// ignores more lines than it lists, or `None` when it lists them all.
    pub fn more_line(&self) -> Option<String> {
        if self.more == 0 {
            return None;
        }
        let Some(Finding::Line { path, .. }) = self.findings.last() else {
            return None;
        };
        Some(format!(
            "{}: vtium also ignores {} more lines of this file. Only the first {ERRORS_MAX} are listed.",
            path.display(),
            self.more
        ))
    }
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
    /// A byte-order mark at the start is dropped, as every other file
    /// vtium reads drops one. vtium keeps the first [`ERRORS_MAX`] lines it
    /// ignores as findings and counts the rest in [`Read::more`], so a
    /// file of nothing but bad lines costs a bounded list.
    pub fn parse(bytes: &[u8], path: &Path) -> Read {
        let bytes = bytes.strip_prefix(crate::split::BOM).unwrap_or(bytes);
        let mut entries = Vec::new();
        let mut findings = Vec::new();
        let mut more = 0_usize;
        for (n, raw) in bytes.split(|&b| b == b'\n').enumerate() {
            let line = n.saturating_add(1);
            let mut fault = |fault| {
                if findings.len() < ERRORS_MAX {
                    findings.push(Finding::Line {
                        path: path.to_path_buf(),
                        line,
                        fault,
                    });
                } else {
                    more = more.saturating_add(1);
                }
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
            batch: Batch::new(entries),
            findings,
            more,
        }
    }

    /// The list of a fresh install: the font files the package ships in
    /// `fonts` ([`FRESH_FONTS`]), the console font `console_font` that
    /// vconsole.conf's `FONT=` names, every file in `palettes`, the folder
    /// of the palettes the package ships, and the package's pointer file
    /// `pointers`, normally [`PACKAGE_POINTERS`]. The built-in font is
    /// always active besides.
    ///
    /// vconsole.conf names the console font by an absolute path or by
    /// kbd's name, such as `ter-v16n`. vtium looks for a font by kbd's
    /// name in `console_font_folders`, normally [`CONSOLE_FONT_FOLDERS`],
    /// and lists each regular file there whose name is the font's name
    /// with one of kbd's suffixes and compressions, or none. A file of that
    /// name in any other folder is not active. An absolute path is listed
    /// as it is, and so is each file of that path with a suffix and a
    /// compression, since the font helper tries them as `setfont` does.
    pub fn fresh_install(
        fonts: &Path,
        palettes: &Path,
        console_font: Option<&str>,
        console_font_folders: &[&Path],
        pointers: &Path,
    ) -> Batch {
        let mut entries: Vec<Entry> = FRESH_FONTS
            .iter()
            .map(|f| Entry::Path(fonts.join(f)))
            .collect();
        if let Some(font) = console_font.filter(|f| !f.is_empty()) {
            console_fonts(font, console_font_folders, &mut entries);
        }
        entries.push(Entry::Path(pointers.to_path_buf()));
        // a plain loop, where an iterator chain over read_dir costs the
        // console a kilobyte of Flatten code
        let mut shipped: Vec<PathBuf> = Vec::new();
        if let Ok(list) = std::fs::read_dir(palettes) {
            for entry in list {
                let Ok(entry) = entry else { continue };
                if entry.file_type().is_ok_and(|t| t.is_file()) {
                    shipped.push(entry.path());
                }
            }
        }
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
                more: 0,
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
                more: 0,
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

/// Adds to `out` the files that are the console font `name`. An absolute
/// `name` is that path, whether or not a file is there, and each existing
/// file of that path with one of kbd's suffixes and compressions after it. Any other `name` is
/// kbd's name for a font in `folders`: each existing file named `name` with
/// a suffix and a compression. A name that is not one plain file name, such
/// as one holding `/` or `..`, names no file. Plain loops keep this small
/// in the console, where an iterator chain compiles to several times the
/// code.
fn console_fonts(name: &str, folders: &[&Path], out: &mut Vec<Entry>) {
    if let Ok(path) = plain(name) {
        out.push(Entry::Path(path.clone()));
        suffixed(&path, true, out);
        return;
    }
    if name.contains('/') || name == "." || name == ".." {
        return;
    }
    for folder in folders {
        suffixed(&folder.join(name), false, out);
    }
}

/// Adds to `out` each regular file named `stem` with one of kbd's suffixes
/// and compressions after it, leaving out `stem` itself when `listed`.
fn suffixed(stem: &Path, listed: bool, out: &mut Vec<Entry>) {
    for suffix in FONT_SUFFIXES {
        for packed in COMPRESSED {
            if listed && suffix.is_empty() && packed.is_empty() {
                continue;
            }
            let mut path = stem.as_os_str().to_owned();
            path.push(suffix);
            path.push(packed);
            let path = PathBuf::from(path);
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
                out.push(Entry::Path(path));
            }
        }
    }
}

/// Where vtium reads what builds the batch: the batch file, vconsole.conf
/// and the kernel command line that name the console font, the folders
/// kbd keeps console fonts in, the folder of the package's fonts and
/// palettes, and the package's pointer file. [`Places::system`] gives this
/// machine's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    pub file: PathBuf,
    pub vconsole_conf: PathBuf,
    pub cmdline: PathBuf,
    pub fonts: PathBuf,
    pub palettes: PathBuf,
    pub pointers: PathBuf,
    pub console_font_folders: Vec<PathBuf>,
}

impl Places {
    /// This machine's: [`BATCH_FILE`], `/etc/vconsole.conf`,
    /// `/proc/cmdline`, [`PACKAGE_FONTS`], the package's palettes,
    /// [`PACKAGE_POINTERS`] and [`CONSOLE_FONT_FOLDERS`].
    pub fn system() -> Places {
        Places {
            file: PathBuf::from(BATCH_FILE),
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            cmdline: PathBuf::from("/proc/cmdline"),
            fonts: PathBuf::from(PACKAGE_FONTS),
            palettes: PathBuf::from(crate::palette::PACKAGE_PALETTES),
            pointers: PathBuf::from(PACKAGE_POINTERS),
            console_font_folders: CONSOLE_FONT_FOLDERS.iter().map(PathBuf::from).collect(),
        }
    }
}

/// `path`'s text, at most [`SMALL_FILE_MAX`] bytes; empty when vtium cannot
/// read it.
fn small_text(path: &Path) -> String {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(SMALL_FILE_MAX).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Batch {
    /// The list of a fresh install at `places`: [`Batch::fresh_install`],
    /// with the console font that vconsole.conf names, and the kernel
    /// command line over it.
    pub fn fresh(places: &Places) -> Batch {
        let named = vconsole(
            &small_text(&places.vconsole_conf),
            &small_text(&places.cmdline),
        );
        let mut folders: Vec<&Path> = Vec::new();
        for folder in &places.console_font_folders {
            folders.push(folder);
        }
        Batch::fresh_install(
            &places.fonts,
            &places.palettes,
            named.font.as_deref(),
            &folders,
            &places.pointers,
        )
    }

    /// The batch at `places`, read under `trust`, the check vtium.conf
    /// passes, with the list of a fresh install in its place when the file
    /// is missing or vtium cannot use it. This is the one place the console
    /// and `vtium check-config` build the batch: the console reads it when
    /// it starts and again at each new login, and hands the same batch to
    /// its font helpers, its color schemes, its color maps and its pointer
    /// files.
    ///
    /// References:
    /// - ADR-1205 D4: one batch, read again at each new login, and the list of a fresh install in its place.
    pub fn system(places: &Places, trust: &Trust) -> Read {
        Batch::read(&places.file, trust, Batch::fresh(places))
    }
}

#[cfg(test)]
mod tests {
    use super::PACKAGE_FONTS;

    /// The package's font folder is `/usr/share/vtium/fonts` unless the
    /// build names another with `VTIUM_PACKAGE_FONTS`, as `make install`
    /// does for its prefix. Run once without the setting and once with it,
    /// such as `VTIUM_PACKAGE_FONTS=/usr/local/share/vtium/fonts`, the test
    /// checks both the default and that the build reads the setting.
    #[test]
    fn the_package_font_folder_is_a_build_setting() {
        match option_env!("VTIUM_PACKAGE_FONTS") {
            None => assert_eq!(PACKAGE_FONTS, "/usr/share/vtium/fonts"),
            Some(dir) => {
                assert_eq!(PACKAGE_FONTS, dir);
                assert!(
                    dir.starts_with('/'),
                    "VTIUM_PACKAGE_FONTS is {dir}, not absolute"
                );
            }
        }
    }
}
