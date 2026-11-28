//! The installed batch file as vtium reads it, and the batch file an
//! install writes from a draft's changes.

use std::path::{Path, PathBuf};

use vtium_settings::batch::{self as active, Batch, Entry};

use crate::apply::Targets;

use super::{Change, Changes};

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

/// The list of a fresh install on this machine: the fonts, color schemes
/// and pointer file the package ships, and the console font that
/// vconsole.conf's `FONT=` names, with the kernel command line over it.
/// vtium uses it while no batch file is installed, and builds it the same
/// way ([`Batch::fresh`]).
pub fn fresh() -> Batch {
    Batch::fresh(&vtium_settings::batch::Places::system())
}

/// The batch the system holds: the one the installed batch file's text
/// `installed` lists, or `fresh` when vtium reads no batch file.
pub fn base(installed: Option<&str>, fresh: &Batch) -> Batch {
    match installed {
        Some(text) => Batch::parse(text.as_bytes(), Path::new("")).batch,
        None => fresh.clone(),
    }
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

    /// The batch vtium uses: the one the file lists, or `fresh`, the list
    /// of a fresh install, when vtium reads no file. A line that vtium
    /// ignores, such as one that is not UTF-8, lists nothing.
    pub fn base(&self, fresh: &Batch) -> Batch {
        match self {
            Installed::Read(bytes) => Batch::parse(bytes, Path::new("")).batch,
            Installed::Missing | Installed::Refused(_) => fresh.clone(),
        }
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

/// The batch file's text after `changes`, starting from `installed`, the
/// installed file's text, or from [`HEADER`] and `fresh` when none is
/// installed: a deactivation takes out each line that names its path, and
/// an activation of a path no line names adds a line at the end. Every
/// other line, comments and blank lines included, stays as it was. `None`
/// when the result lists the same resources as the system holds now, so
/// the install leaves the batch file as it is.
pub fn applied(installed: Option<&str>, fresh: &Batch, changes: &Changes) -> Option<String> {
    let text = edited(installed, fresh, changes);
    let after = Batch::parse(text.as_bytes(), Path::new("")).batch;
    (after != base(installed, fresh)).then_some(text)
}

/// The text [`applied`] makes, whether or not it changes anything. Each
/// line is read once, and the paths it names are looked up in sets, so a
/// long list of changes takes time in proportion to its length.
fn edited(installed: Option<&str>, fresh: &Batch, changes: &Changes) -> String {
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
    let gone: BTreeSet<&Path> = changes
        .iter()
        .filter(|(_, c)| *c == Change::Deactivate)
        .map(|(p, _)| p)
        .collect();
    let mut named = BTreeSet::new();
    let mut text = String::new();
    for line in &lines {
        let path = line_path(line);
        if path.as_deref().is_some_and(|p| gone.contains(p)) {
            continue;
        }
        named.extend(path);
        text.push_str(line);
        text.push('\n');
    }
    // `changes` holds each path once, so no activation adds a line that
    // another activation added
    for (path, change) in changes.iter() {
        if change == Change::Activate && !named.contains(path) {
            text.push_str(&path.display().to_string());
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

/// Why the batch file that `changes` make, starting from `installed` and
/// `fresh` as [`applied`] does, is refused: it is longer than vtium reads,
/// and longer than the file `before` makes. `None` when the file fits, or
/// when it shrinks, so that a draft that holds too many activations can
/// always be brought back within the bound by deactivating resources.
pub(super) fn too_long(
    installed: Option<&str>,
    fresh: &Batch,
    before: &Changes,
    changes: &Changes,
) -> Option<usize> {
    let after = edited(installed, fresh, changes).len();
    (after > active::BATCH_MAX && after > edited(installed, fresh, before).len()).then_some(after)
}
