//! The lists of active resources that packages and root keep beside the
//! batch file, as vtium-config reads them, and the `-` lines of the batch
//! file that turn a listed path off.
//!
//! A package can install a list of the resources it activates, such as
//! `/usr/share/vtium/batch.d/vtium-fonts.conf`. Root can keep lists of its
//! own in `/etc/vtium/batch.d/`, where a file with the name of a package's
//! list turns that list off or replaces it. vtium reads the lists wherever
//! it reads the batch file, and vtium-config reads them with the same
//! reader, vtium-settings's [`Read::add_lists`], so what vtium-config
//! shows as active is what vtium draws from ([`View`]).
//!
//! vtium-config writes only batch.conf, and never copies a list's paths
//! into it, so removing a package still deactivates its fonts. To
//! deactivate a path that a list names, an install writes a `-PATH` line.
//! To activate a path that batch.conf turns off, it takes the `-` line
//! out, and it adds a line of its own only where no list would activate
//! the path ([`super::applied`]).
//!
//! References:
//! - ADR-1205 D4, as amended on 2026-11-27: where the lists live, how vtium reads them, turning a list off from /etc, turning one path off with a `-` line, how vtium-config shows and deactivates a package's font, and what `vtium config active` prints.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtium_settings::Trust;
use vtium_settings::batch::{
    BATCH_MAX, Batch, Entry, Folder, LISTS_FOLDER, ListFile, ListFinding, ListState, Origin,
    Places, Read,
};

use crate::apply::Targets;

/// The two folders of lists: the packages' folder, whose files the
/// package manager owns, and root's, beside the batch file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    pub package: PathBuf,
    pub admin: PathBuf,
}

impl Folders {
    /// This machine's: `/usr/share/vtium/batch.d` and
    /// `/etc/vtium/batch.d`, or the folders the build names in their
    /// place, as vtium reads them.
    pub fn system() -> Folders {
        let places = Places::system();
        Folders {
            package: places.package_lists(),
            admin: places.admin_lists(),
        }
    }

    /// The folders for an install to `targets`: this machine's for the
    /// system's directories. For any other, such as a test's, they are
    /// `package-lists` and `batch.d` in the configuration directory, the
    /// second beside the batch file that such an install writes, as
    /// `package-fonts` stands in for the packages' fonts there.
    pub fn of(targets: &Targets) -> Folders {
        if targets.is_system() {
            Folders::system()
        } else {
            Folders {
                package: targets.configdir.join("package-lists"),
                admin: targets.configdir.join(LISTS_FOLDER),
            }
        }
    }
}

/// What vtium builds the batch from besides the batch file: the list of
/// a fresh install, which takes the batch file's place where vtium reads
/// none, and the two folders of lists, read under `trust`, the check the
/// batch file passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub fresh: Batch,
    pub folders: Folders,
    pub trust: Trust,
}

impl Start {
    /// This machine's, read under root's trust, as the console reads it.
    pub fn system() -> Start {
        Start {
            fresh: Batch::fresh(&Places::system()),
            folders: Folders::system(),
            trust: Trust::running_as(0),
        }
    }

    /// The start of the batch for an install to `targets`, whose files
    /// the trust rule accepts from root and from the user `euid`
    /// ([`super::rule_user`]).
    pub fn of(targets: &Targets, euid: u32) -> Start {
        Start {
            fresh: Batch::fresh(&Places::system()),
            folders: Folders::of(targets),
            trust: Trust::running_as(euid),
        }
    }
}

/// The start of the batch for an install to the directories `paths`
/// names, read under the trust rule of the user that install accepts
/// files from ([`Start::of`]).
pub fn start(paths: &crate::places::Paths) -> Start {
    let targets = paths.targets();
    Start::of(&targets, super::rule_user(&targets, &paths.sudo))
}

/// How root turned a path off, so that vtium does not draw from it
/// although a list names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Off {
    /// A `-PATH` line in batch.conf.
    BatchFile,
    /// Root's entry at this path, in root's folder of lists, turns off or
    /// replaces the package's list that names the path.
    List(PathBuf),
}

/// The batch as vtium reads it: the batch file, or the list of a fresh
/// install in its place, with the lists of both folders added, less what
/// batch.conf turns off. Building it reads each list once, so a screen
/// builds it when it opens and after a save, not on a keypress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    read: Read,
    /// Each path of a package's list that root's entry of the same name
    /// turns off or replaces, and that nothing else activates, with root's
    /// entry.
    masked: BTreeMap<PathBuf, PathBuf>,
}

impl View {
    /// The batch vtium reads from `installed`, the batch file's bytes, or
    /// from `start`'s list of a fresh install where vtium reads no batch
    /// file, with the lists in `start`'s folders.
    pub fn new(installed: Option<&[u8]>, start: &Start) -> View {
        let mut read = match installed {
            Some(bytes) => Batch::parse(bytes, Path::new("")),
            None => Read {
                batch: start.fresh.clone(),
                findings: Vec::new(),
                more: 0,
                list_files: Vec::new(),
                list_findings: Vec::new(),
            },
        };
        read.add_lists(&start.folders.package, &start.folders.admin, &start.trust);
        let masked = masked(&read, &start.trust);
        View { read, masked }
    }

    /// The active resources, and for each where its activation comes
    /// from.
    pub fn batch(&self) -> &Batch {
        &self.read.batch
    }

    /// Each entry in the two folders of lists and its state, in the order
    /// vtium reads them.
    pub fn list_files(&self) -> &[ListFile] {
        &self.read.list_files
    }

    /// What vtium found wrong reading the lists, which `vtium
    /// check-config` shows.
    pub fn list_findings(&self) -> &[ListFinding] {
        &self.read.list_findings
    }

    /// How root turned `path` off, or `None` when root did not, or the
    /// batch holds the path.
    pub fn off(&self, path: &Path) -> Option<Off> {
        let batch = self.batch();
        if batch.turns_off(path) {
            return Some(Off::BatchFile);
        }
        if batch.holds(path) {
            return None;
        }
        let path = super::plain(path.to_str()?)?;
        self.masked.get(&path).cloned().map(Off::List)
    }
}

/// Each path of a package's list that root's entry turns off or
/// replaces, and that the batch does not hold, with root's entry. vtium
/// reads no such list, so this reads each one as vtium would read it,
/// under `trust` and within [`BATCH_MAX`], to find the paths root turned
/// off. A list that does not read turns nothing off on purpose.
fn masked(read: &Read, trust: &Trust) -> BTreeMap<PathBuf, PathBuf> {
    let mut out = BTreeMap::new();
    for file in &read.list_files {
        let masked = matches!(file.state, ListState::TurnedOff | ListState::Replaced);
        if file.folder != Folder::Package || !masked {
            continue;
        }
        let Ok(Some(bytes)) = vtium_settings::read_limited(&file.path, trust, BATCH_MAX) else {
            continue;
        };
        for Entry::Path(path) in Batch::parse(&bytes, &file.path).batch.entries() {
            if !read.batch.holds(path) {
                out.entry(path.clone())
                    .or_insert_with(|| file.set_by.clone());
            }
        }
    }
    out
}

/// Where a resource's state comes from, as the fourth column of `vtium
/// config active` names it: `batch.conf`, `fresh install`, or the name of
/// the list that activates it, such as `vtium-fonts`.
pub fn origin_words(origin: Origin<'_>) -> &str {
    match origin {
        Origin::BatchFile => "batch.conf",
        Origin::FreshInstall => "fresh install",
        Origin::List(list) => &list.name,
    }
}

/// The state `vtium config active` prints for a path that a list
/// activates and where no file is installed. The batch does not judge
/// such a path, because a package's list names optional fonts on
/// purpose, so vtium-config looks for the file itself.
pub const NOT_INSTALLED: &str = "not installed";

/// Whether no file, folder or link is at `path`.
pub fn not_installed(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_err()
}

/// One line of `vtium config active` for each entry in the two folders
/// of lists: the kind `list`, its path, its state and the entry that sets
/// the state, separated by tabs.
pub fn list_lines(view: &View) -> Vec<String> {
    view.list_files()
        .iter()
        .map(|f| {
            format!(
                "list\t{}\t{}\t{}",
                f.path.display(),
                f.state,
                f.set_by.display()
            )
        })
        .collect()
}

/// The detail line that says where the activation of a resource that the
/// tabs show comes from, as in "Active: the vtium-fonts package activates
/// it.", or `None` when nothing activates it.
pub fn activated_by(origin: &crate::found::Origin) -> Option<String> {
    use crate::found::Origin as Found;
    Some(match origin {
        Found::Nothing => return None,
        Found::BatchFile => "Active: batch.conf lists it.".to_owned(),
        Found::FreshInstall => "Active: the list of a fresh install activates it.".to_owned(),
        Found::List {
            name,
            folder: Folder::Package,
        } => format!("Active: the {name} package activates it."),
        Found::List {
            name,
            folder: Folder::Admin,
        } => format!("Active: root's list {name} in /etc/vtium/batch.d activates it."),
        Found::TurnedOff { list: Some(name) } => {
            format!("Not active: a line of batch.conf turns off what the {name} list activates.")
        }
        Found::TurnedOff { list: None } => {
            "Not active: a line of batch.conf turns it off.".to_owned()
        }
    })
}
