//! The installed batch file as vtium reads it, and the batch file an
//! install writes from a draft's changes.

use std::path::{Path, PathBuf};

use vtium_settings::batch::{self as active, Batch, Entry, off_line};

use crate::apply::Targets;

use super::{Change, Changes, Start, View};

/// The batch file that an install to `targets` writes: the one vtium
/// reads, [`active::BATCH_FILE`], for the system's directories, and
/// `batch.conf` in the configuration directory for any other, such as a
/// test's.
pub fn batch_file(targets: &Targets) -> PathBuf {
    if targets.is_system() {
        PathBuf::from(active::BATCH_FILE)
    } else {
        targets.configdir.join("batch.conf")
    }
}

/// What the batch starts from on this machine besides the batch file:
/// the list of a fresh install, which the package's fonts, color schemes
/// and pointer file and vconsole.conf's console font make, and the lists
/// of the two `batch.d` folders, read under root's trust as the console
/// reads them ([`Start::system`]). An install to other directories, such
/// as a test's, starts from [`Start::of`] instead.
pub fn fresh() -> Start {
    Start::system()
}

/// The batch the system holds: the one the installed batch file's text
/// `installed` lists, or `start`'s list of a fresh install when vtium
/// reads no batch file, with the lists of `start`'s folders.
pub fn base(installed: Option<&str>, start: &Start) -> Batch {
    View::new(installed.map(str::as_bytes), start)
        .batch()
        .clone()
}

/// The installed batch file, as vtium reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// No batch file is installed, so vtium uses the list of a fresh
    /// install.
    Missing,
    /// vtium ignores the batch file and uses the list of a fresh install.
    /// The text says why, file first, in the words of `vtium check-config`.
    Refused(String),
    /// vtium reads the batch file: its bytes.
    Read(Vec<u8>),
}

impl Installed {
    /// Reads the batch file at `path` as vtium does when it runs as the
    /// user `euid`. The file and each folder above it must belong to root
    /// or to `euid`, and nobody else may write them. vtium reads at most
    /// [`active::BATCH_MAX`] bytes. A file that fails the check, is larger,
    /// is not a regular file or cannot be read counts as missing, because
    /// someone other than root could have written it.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: vtium uses the list of a fresh install in place of a batch file it cannot use.
    pub fn read(path: &Path, euid: u32) -> Installed {
        use vtium_settings::batch::{BATCH_MAX, Finding, Unread};
        let trust = vtium_settings::Trust::running_as(euid);
        match vtium_settings::read_limited(path, &trust, BATCH_MAX) {
            Ok(Some(bytes)) => Installed::Read(bytes),
            Ok(None) => Installed::Missing,
            Err(rejected) => {
                let why = rejected
                    .errors
                    .into_iter()
                    .next()
                    .map_or(Unread::NotAFile, |e| e.problem.into());
                let finding = Finding::Refused {
                    path: path.to_path_buf(),
                    why,
                };
                Installed::Refused(finding.to_string())
            }
        }
    }

    /// The batch vtium uses, as [`View::new`] reads it: the one the file
    /// lists, or `start`'s list of a fresh install when vtium reads no
    /// file, with the lists of `start`'s folders. A line that vtium
    /// ignores, such as one that is not UTF-8, lists nothing.
    pub fn view(&self, start: &Start) -> View {
        match self {
            Installed::Read(bytes) => View::new(Some(bytes), start),
            Installed::Missing | Installed::Refused(_) => View::new(None, start),
        }
    }

    /// The batch of [`Installed::view`].
    pub fn base(&self, start: &Start) -> Batch {
        self.view(start).batch().clone()
    }

    /// The text of the file vtium reads, the one that changes start from,
    /// or `None` when vtium reads no file and changes start from the list
    /// of a fresh install. `Err` names the first line of the file at
    /// `path` that is not UTF-8, which vtium-config cannot keep in a file
    /// it writes.
    pub fn text(&self, path: &Path) -> Result<Option<&str>, String> {
        let Installed::Read(bytes) = self else {
            return Ok(None);
        };
        std::str::from_utf8(bytes).map(Some).map_err(|e| {
            let before = bytes.get(..e.valid_up_to()).unwrap_or_default();
            let line = before
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                .saturating_add(1);
            format!(
                "{}:{line}: this line is not UTF-8 text. vtium ignores the line, and vtium-config cannot keep it in the file it writes. Fix the line or remove it as root, then install again.",
                path.display()
            )
        })
    }
}

/// The comment that opens a batch file vtium-config writes when vtium
/// reads none, because none is installed or vtium refuses the installed
/// one. It is wrapped at 78 columns, as vtium.conf's comments are.
pub const HEADER: &str = "\
# vtium uses only the resources this file lists: fonts, color schemes,
# color maps, shaders and pointer themes, one absolute path on each line.
# The built-in font is always active and is not listed here.
#
# vtium-config wrote this file from the list of a fresh install, when a
# draft changed the active resources while vtium used that list.
# vtium-config activate and vtium-config deactivate change it through a
# draft, and root can edit it.

";

/// The path a line of a batch file names, if it names one.
fn line_path(line: &str) -> Option<PathBuf> {
    let read = Batch::parse(line.as_bytes(), Path::new(""));
    match read.batch.entries() {
        [Entry::Path(path)] => Some(path.clone()),
        _ => None,
    }
}

/// The path a `-` line of a batch file turns off, if it is one.
fn line_off(line: &str) -> Option<PathBuf> {
    let read = Batch::parse(line.as_bytes(), Path::new(""));
    let mut off = read.batch.turned_off();
    off.next()
        .map(Path::to_path_buf)
        .filter(|_| off.next().is_none())
}

/// The batch file's text after `changes`, starting from `installed`, the
/// installed file's text, or from [`HEADER`] and `start`'s list of a fresh
/// install when none is installed. Every line that `changes` do not name
/// stays as it was, comments and blank lines included. `None` when the
/// result leaves the same resources active and turned off as the system
/// holds now, so the install leaves the batch file as it is.
///
/// A deactivation takes out each line that names its path, and adds a
/// `-PATH` line where a list names the path, so the path stays off while
/// the list names it. An activation takes out each `-` line that names
/// its path, and adds a line of its own only where no list would then
/// activate the path. So no list's paths are ever copied into the file,
/// also when it is first written from the list of a fresh install, and a
/// package's fonts go with the package.
///
/// References:
/// - ADR-1205 D4, as amended on 2026-11-27: vtium-config writes `-PATH` for a deactivated path a list names, takes the `-` line out to activate it, adds a line only where no list activates the path, and never copies a list into batch.conf.
pub fn applied(installed: Option<&str>, start: &Start, changes: &Changes) -> Option<String> {
    let before = base(installed, start);
    let text = edited(installed, &start.fresh, &before, changes);
    let after = base(Some(&text), start);
    let same = |a: &Batch, b: &Batch| {
        a.entries().len() == b.entries().len()
            && a.entries().iter().all(|Entry::Path(p)| b.holds(p))
            && a.turned_off().eq(b.turned_off())
    };
    (!same(&after, &before)).then_some(text)
}

/// The text [`applied`] makes, whether or not it changes anything, where
/// `base` is the batch the system holds, whose lists say which paths a
/// list names. Each line is read once, and the paths it names are looked
/// up in sets, so a long list of changes takes time in proportion to its
/// length.
fn edited(installed: Option<&str>, fresh: &Batch, base: &Batch, changes: &Changes) -> String {
    use std::collections::BTreeSet;
    let lines: Vec<String> = match installed {
        Some(text) => text.lines().map(str::to_owned).collect(),
        None => HEADER
            .lines()
            .map(str::to_owned)
            .chain(
                fresh
                    .entries()
                    .iter()
                    .map(|Entry::Path(p)| p.display().to_string()),
            )
            .collect(),
    };
    let wanted = |want: Change| -> BTreeSet<&Path> {
        changes
            .iter()
            .filter(|(_, c)| *c == want)
            .map(|(p, _)| p)
            .collect()
    };
    let (gone, back) = (wanted(Change::Deactivate), wanted(Change::Activate));
    let mut named = BTreeSet::new();
    let mut off = BTreeSet::new();
    let mut text = String::new();
    for line in &lines {
        let path = line_path(line);
        if path.as_deref().is_some_and(|p| gone.contains(p)) {
            continue;
        }
        let turned = line_off(line);
        if turned.as_deref().is_some_and(|p| back.contains(p)) {
            continue;
        }
        named.extend(path);
        off.extend(turned);
        text.push_str(line);
        text.push('\n');
    }
    // `changes` holds each path once, so no change adds a line that
    // another change added
    for (path, change) in changes.iter() {
        let listed = base.listed_by(path).is_some();
        let line = match change {
            Change::Activate if !listed && !named.contains(path) => {
                Some(path.display().to_string())
            }
            Change::Deactivate if listed && !off.contains(path) => off_line(path),
            _ => None,
        };
        if let Some(line) = line {
            text.push_str(&line);
            text.push('\n');
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    text
}

/// How many bytes vtium reads of the batch file at most, in KiB, as the
/// messages about the bound say it.
pub(super) const BATCH_KIB: usize = active::BATCH_MAX / 1024;

/// Why the batch file that `changes` make, starting from `installed`,
/// `fresh` and `base` as [`applied`] does, is refused: it is longer than
/// vtium reads, and longer than the file `before` makes. `None` when the
/// file fits, or when it shrinks, so that a draft that holds too many
/// activations can always be brought back within the bound by
/// deactivating resources.
pub(super) fn too_long(
    installed: Option<&str>,
    (fresh, base): (&Batch, &Batch),
    before: &Changes,
    changes: &Changes,
) -> Option<usize> {
    let after = edited(installed, fresh, base, changes).len();
    let was = edited(installed, fresh, base, before).len();
    (after > active::BATCH_MAX && after > was).then_some(after)
}
