//! The active resources beside a draft, and the commands `activate`,
//! `deactivate` and `active`.
//!
//! vtium uses only the active resources: the fonts, color schemes,
//! color maps, shaders and pointer themes that the batch file,
//! `/etc/vtium/batch.conf`, lists one path on each line, and those that
//! the lists packages and root keep in the `batch.d` folders name, less
//! what a `-` line of batch.conf turns off ([`View`]). The built-in
//! font is always active and never appears there. A change to the batch
//! waits beside the draft, in `drafts/NAME.batch`, until the draft is
//! installed, so it installs with the draft and goes when the draft is
//! reverted or removed.
//!
//! `NAME.batch` keeps changes, one `activate PATH` or `deactivate PATH`
//! on each line, and not a copy of the batch file. The install applies
//! them to the batch file as it is then, so a line root wrote by hand
//! stays. When no batch file is installed, or vtium refuses the one that
//! is, the install starts from the list of a fresh install, as vtium does,
//! so the first change never drops the default fonts. vtium refuses a
//! batch file that fails the owner and mode check, is too large, or cannot
//! be read, and vtium-config reads the file the same way
//! ([`Installed::read`]), so it never builds on lines vtium ignores.
//!
//! References:
//! - ADR-1205 D4: the batch, its file, the trust rule each activated file passes, the list of a fresh install, and a draft keeping its changes to the batch beside it.
//! - ADR-1205 D4, as amended on 2026-11-27: the packages' lists, root's lists and the `-` lines of batch.conf.
//! - ADR-1205 D8: deactivating a resource warns and names the looks that use it.
//! - ADR-1403 D8: a draft keeps its font set changes beside it in the same way.

mod command;
mod installed;
mod lists;
mod resource;
mod uses;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtium_settings::batch::{Batch, Entry};

use crate::outcome::Fail;

pub use command::{Done, Resources, State, command};
pub use installed::{HEADER, Installed, applied, base, batch_file, fresh};
pub use lists::{
    Folders, NOT_INSTALLED, Off, Start, View, activated_by, list_lines, not_installed,
    origin_words, start,
};
pub use resource::{kind, rule_user, trusted};
pub use uses::{
    Places, Through, Use, drawing_palette, drawing_vtlook, named_files, users, uses, vtlook_files,
};

/// One change a draft makes to the active resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Activate,
    Deactivate,
}

impl Change {
    /// The word for the change, as `NAME.batch` and the commands write it.
    pub const fn word(self) -> &'static str {
        match self {
            Change::Activate => "activate",
            Change::Deactivate => "deactivate",
        }
    }
}

/// A draft's changes to the active resources, in the order they were made,
/// at most one for each path. An index by path finds a path's change
/// without a scan of the list, so reading a large `NAME.batch` does not
/// take time in the square of its length.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    list: Vec<(PathBuf, Change)>,
    /// Each path's place in `list`.
    index: BTreeMap<PathBuf, usize>,
}

impl Changes {
    /// Reads the text of a `NAME.batch` file at `shown`. A line vtium-config
    /// cannot read refuses the whole file, naming the line.
    pub fn parse(text: &str, shown: &Path) -> Result<Changes, Fail> {
        let mut changes = Changes::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parsed = line.split_once(' ').and_then(|(word, rest)| {
                let change = match word {
                    "activate" => Change::Activate,
                    "deactivate" => Change::Deactivate,
                    _ => return None,
                };
                Some((plain(rest.trim())?, change))
            });
            let Some((path, change)) = parsed else {
                return Err(Fail::File(format!(
                    "{} line {}: vtium-config cannot read this line. Each line reads activate PATH or deactivate PATH, with an absolute path.",
                    shown.display(),
                    n.saturating_add(1)
                )));
            };
            changes.set(path, Some(change));
        }
        Ok(changes)
    }

    /// The text of the `NAME.batch` file of the draft `draft`.
    pub fn text(&self, draft: &str) -> String {
        let mut out = format!(
            "# The changes that draft {draft} makes to the resources vtium may use.\n# vtium-config applies them when it installs the draft.\n"
        );
        for (path, change) in &self.list {
            out.push_str(&format!("{} {}\n", change.word(), path.display()));
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// The change for `path`, if the draft makes one.
    pub fn get(&self, path: &Path) -> Option<Change> {
        let at = *self.index.get(path)?;
        self.list.get(at).map(|(_, c)| *c)
    }

    /// Makes `change` the draft's change for `path`, or with `None` takes
    /// the draft's change for it back.
    pub fn set(&mut self, path: PathBuf, change: Option<Change>) {
        let at = self.index.get(&path).copied();
        match (at.and_then(|at| self.list.get_mut(at)), change) {
            (Some(slot), Some(change)) => slot.1 = change,
            (None, Some(change)) => {
                self.index.insert(path.clone(), self.list.len());
                self.list.push((path, change));
            }
            (Some(_), None) => {
                self.list.retain(|(p, _)| *p != path);
                self.reindex();
            }
            (None, None) => {}
        }
    }

    /// Builds the index again after changes left `list`.
    fn reindex(&mut self) {
        #[cfg(test)]
        tests::LIST_PASSES.with(|n| n.set(n.get().saturating_add(1)));
        self.index = self
            .list
            .iter()
            .enumerate()
            .map(|(at, (p, _))| (p.clone(), at))
            .collect();
    }

    /// Each change, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, Change)> {
        self.list.iter().map(|(p, c)| (p.as_path(), *c))
    }

    /// Keeps only the changes that `base`, the batch the system holds,
    /// does not already match: an activation of a path it lists, and a
    /// deactivation of a path it lacks, are no change.
    pub fn settle(&mut self, base: &Batch) {
        self.list.retain(|(path, change)| match change {
            Change::Activate => !base.holds(path),
            Change::Deactivate => base.holds(path),
        });
        self.reindex();
    }
}

/// `1 change to the active resources`, or `N changes ...`, as the state
/// line and `revert --dry-run` count a draft's changes.
pub fn resource_changes(n: usize) -> String {
    match n {
        1 => "1 change to the active resources".to_owned(),
        n => format!("{n} changes to the active resources"),
    }
}

/// `word` as a path the batch file can list: an absolute path without
/// `..`, with `.` and doubled slashes dropped, read as vtium reads a line
/// of the batch file. A path that holds a control character, such as a tab
/// or an escape, is refused: `active` separates its fields with tabs, and
/// an escape would reach the terminal raw when a command prints the path.
pub fn plain(word: &str) -> Option<PathBuf> {
    if word.chars().any(char::is_control) {
        return None;
    }
    let read = Batch::parse(word.as_bytes(), Path::new(""));
    match (read.findings.as_slice(), read.batch.entries()) {
        ([], [Entry::Path(path)]) => Some(path.clone()),
        _ => None,
    }
}

/// Whether `word` names the built-in font, which is always active: a
/// built-in font's name, `builtin`, or a frit's `builtin:NAME` source.
pub fn builtin(word: &str) -> bool {
    crate::frit::RESERVED.contains(&word) || word == "builtin" || word.starts_with("builtin:")
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests;
