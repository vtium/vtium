//! The lists of active resources in the two list folders: the packages'
//! folder, such as `/usr/share/vtium/batch.d`, and root's, such as
//! `/etc/vtium/batch.d`.
//!
//! A list is a file named `NAME.conf` in the batch file's format, one
//! absolute path on each line, without the `-` lines that only the batch
//! file may hold. In each folder vtium takes the entries whose names end
//! in `.conf`, at most [`LISTS_MAX`] of them in the byte order of the
//! names, whatever their type, and passes over every other name, so
//! pacman's `.pacnew` and `.pacsave` copies activate nothing. Each entry
//! past the count activates nothing and turns no list off, and a finding
//! names it. vtium then reads the lists of both folders in the byte order
//! of their names.
//!
//! An entry in root's folder takes the place of the package's list of the
//! same name, whatever the entry's type, because turning a list off can
//! only make less active. A regular file with paths of its own replaces
//! the package's list; an empty one, one of comments, one vtium refuses,
//! and an entry that is not a regular file, such as a symbolic link to
//! `/dev/null`, turn the package's list off. An entry in root's folder
//! whose name no package's list uses is a list of root's own.
//!
//! vtium reads a list only from an entry that is a regular file once
//! links are followed, exactly as it reads the batch file: through
//! [`read_limited`](crate::file::read_limited), under the batch file's
//! owner and mode check of the file and every folder it crosses, links
//! included, and at most [`BATCH_MAX`] bytes. A list that fails activates
//! nothing, and a finding names the file and why. A list only adds to the
//! batch, so a refused list leaves the rest of the batch as it is. The
//! paths of the merged batch come to at most [`MERGED_BATCH_MAX`] bytes:
//! the list that would pass the bound is left out, with every list after
//! it, and a finding names each one.
//!
//! References:
//! - ADR-1205 D4, as amended on 2026-11-27: where the lists live, how vtium reads them, the limits, their trust rule, a listed file that is not installed, and turning a list off from /etc, by name over every entry, while only regular files are read.
//! - ADR-1401 D10: the owner and mode check each list passes, as the batch file does.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::{Entry, LISTS_MAX, LineFault, MERGED_BATCH_MAX, Read, Unread, lines};
use crate::Trust;
use crate::bounds::{BATCH_MAX, ERRORS_MAX};

/// The ending of a list's file name.
const ENDING: &[u8] = b".conf";

/// The most names vtium keeps from one folder: the entries it takes and
/// those it names past them. It counts the others without keeping them,
/// so a folder of many files costs a bounded set.
const NAMES_KEPT: usize = LISTS_MAX + ERRORS_MAX;

/// The folder a list lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    /// The packages' folder, such as `/usr/share/vtium/batch.d`, whose
    /// files the package manager owns.
    Package,
    /// Root's folder, such as `/etc/vtium/batch.d`.
    Admin,
}

/// A list that adds to the batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// The list's name: its file's name without `.conf`, such as
    /// `vtium-fonts`, the package's name for a package's list.
    pub name: String,
    /// The file vtium read the list from.
    pub file: PathBuf,
    /// The folder the file is in.
    pub folder: Folder,
}

/// The state of one entry in the list folders, as `vtium config active`
/// prints it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListState {
    /// vtium read the list, and its paths are active.
    Read,
    /// The entry activates nothing and takes a package's list of its name
    /// with it: a package's list that root's entry of the same name turns
    /// off, or root's entry that is not a regular file, such as a link to
    /// `/dev/null`.
    TurnedOff,
    /// A file of the same name in root's folder, with paths of its own,
    /// takes the package's list's place.
    Replaced,
    /// vtium refused the file, it comes past [`LISTS_MAX`], or it would
    /// take the merged batch past [`MERGED_BATCH_MAX`], so it activates
    /// nothing.
    Refused,
}

impl fmt::Display for ListState {
    /// The state as one or two words: `read`, `turned off`, `replaced` or
    /// `refused`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ListState::Read => "read",
            ListState::TurnedOff => "turned off",
            ListState::Replaced => "replaced",
            ListState::Refused => "refused",
        })
    }
}

/// One entry in the list folders: its path, its folder, its state, and
/// the entry that sets the state, which is the entry itself or root's
/// entry of the same name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListFile {
    pub path: PathBuf,
    pub folder: Folder,
    pub state: ListState,
    pub set_by: PathBuf,
}

/// What vtium found reading the list folders, which `vtium check-config`
/// shows. A listed path where no file is installed is no finding, because
/// a package's list names optional fonts on purpose, and nor is an entry
/// that is not a regular file, such as a link to `/dev/null` that turns a
/// package's list off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListFinding {
    /// vtium refused the list, which activates nothing. `masks` names the
    /// package's list that the refused file's name still turns off.
    Refused {
        path: PathBuf,
        why: Unread,
        masks: Option<PathBuf>,
    },
    /// The entry's name comes past the first [`LISTS_MAX`] of its folder,
    /// so it activates nothing and turns no list off.
    PastLimit { path: PathBuf },
    /// The count of the entries past [`LISTS_MAX`] that the findings do
    /// not name.
    MoreLists { count: usize },
    /// The list would take the merged batch past [`MERGED_BATCH_MAX`], or
    /// comes after one that would, so it activates nothing.
    PastBound { path: PathBuf },
    /// A line of a list that vtium ignores, numbered from 1.
    Line {
        path: PathBuf,
        line: usize,
        fault: LineFault,
    },
    /// The count of the lines of a list that vtium ignores past the first
    /// [`ERRORS_MAX`].
    MoreLines { path: PathBuf, count: usize },
    /// vtium cannot list the folder's entries, so nothing in it activates
    /// anything.
    Folder {
        path: PathBuf,
        why: std::io::ErrorKind,
    },
}

impl fmt::Display for ListFinding {
    /// The finding as `vtium check-config` shows it: the file first, then
    /// what vtium does and what to do.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListFinding::Refused { path, why, masks } => {
                write!(
                    f,
                    "{}: vtium activates nothing from this list of active fonts, color schemes, effects and pointer themes, because {why}",
                    path.display()
                )?;
                if let Some(masked) = masks {
                    write!(
                        f,
                        " Its name still turns off the package's list {}.",
                        masked.display()
                    )?;
                }
                Ok(())
            }
            ListFinding::PastLimit { path } => write!(
                f,
                "{}: vtium activates nothing from this list of active resources, because vtium takes at most {LISTS_MAX} entries from its folder, in the byte order of their names, and this one comes after them. Remove the lists this machine does not need.",
                path.display()
            ),
            ListFinding::MoreLists { count } => write!(
                f,
                "vtium also activates nothing from {count} more lists of active resources past the first {LISTS_MAX} of their folders. Only the first {ERRORS_MAX} of those are listed."
            ),
            ListFinding::PastBound { path } => write!(
                f,
                "{}: vtium activates nothing from this list of active resources, because the paths of the lists before it already come close to {MERGED_BATCH_MAX} bytes, the most vtium holds, and this list would pass that. Remove the lists this machine does not need.",
                path.display()
            ),
            ListFinding::Line { path, line, fault } => write!(
                f,
                "{}:{line}: {fault}. Because of this, vtium ignores the line.",
                path.display()
            ),
            ListFinding::MoreLines { path, count } => write!(
                f,
                "{}: vtium also ignores {count} more lines of this file. Only the first {ERRORS_MAX} are listed.",
                path.display()
            ),
            ListFinding::Folder { path, why } => write!(
                f,
                "{}: vtium cannot list the entries in this folder of lists of active resources: {why}. vtium activates nothing from it. Check that root can read it.",
                path.display()
            ),
        }
    }
}

/// The list names in one folder: the entries vtium takes, and those past
/// [`LISTS_MAX`] that the findings name.
struct Names {
    taken: BTreeSet<Vec<u8>>,
    past: Vec<Vec<u8>>,
    /// The count of the entries past those `past` names.
    more: usize,
}

/// The list names in `folder`: the entry names that end in `.conf` after
/// at least one other byte, whatever the entries' types. vtium takes the
/// first [`LISTS_MAX`] in byte order, names the next [`ERRORS_MAX`] and
/// counts the rest. A folder that does not exist holds none; one vtium
/// cannot list adds a finding.
fn names(folder: &Path, findings: &mut Vec<ListFinding>) -> Names {
    let mut kept = BTreeSet::new();
    let mut more = 0_usize;
    match std::fs::read_dir(folder) {
        Ok(list) => {
            for entry in list {
                let Ok(entry) = entry else { continue };
                let name = entry.file_name();
                let bytes = name.as_bytes();
                if bytes.len() <= ENDING.len() || !bytes.ends_with(ENDING) {
                    continue;
                }
                kept.insert(bytes.to_vec());
                if kept.len() > NAMES_KEPT {
                    kept.pop_last();
                    more = more.saturating_add(1);
                }
            }
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                findings.push(ListFinding::Folder {
                    path: folder.to_path_buf(),
                    why: e.kind(),
                });
            }
        }
    }
    let mut taken = kept;
    let past = match taken.iter().nth(LISTS_MAX).cloned() {
        Some(first_past) => taken.split_off(&first_past).into_iter().collect(),
        None => Vec::new(),
    };
    Names { taken, past, more }
}

/// The list's name: its file name without `.conf`.
fn stem(name: &[u8]) -> String {
    let stem = name
        .get(..name.len().saturating_sub(ENDING.len()))
        .unwrap_or(name);
    String::from_utf8_lossy(stem).into_owned()
}

/// What became of one entry that vtium took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// vtium read the list, and it added paths.
    Added,
    /// vtium read the list, which names no path, or the file is gone.
    Empty,
    /// The entry is not a regular file once links are followed, so vtium
    /// reads nothing from it.
    NotRegular,
    /// vtium refused the file, with a finding.
    Refused,
    /// The list would take the merged batch past the bound, or comes
    /// after one that would, with a finding.
    LeftOut,
}

impl Outcome {
    /// The entry's own state.
    fn state(self) -> ListState {
        match self {
            Outcome::Added | Outcome::Empty => ListState::Read,
            Outcome::NotRegular => ListState::TurnedOff,
            Outcome::Refused | Outcome::LeftOut => ListState::Refused,
        }
    }
}

impl Read {
    /// Adds the lists in `package`, the packages' list folder, and in
    /// `admin`, root's, each read under `trust`, the batch file's check.
    /// Each path a list names becomes active unless the batch file turns
    /// it off. [`Read::list_files`] gains each entry's state, and
    /// [`Read::list_findings`] what vtium refused, left out or ignored.
    ///
    /// References:
    /// - ADR-1205 D4, as amended on 2026-11-27: how vtium reads the lists, the limits, the trust rule for a list, and turning a package's list off from /etc.
    pub fn add_lists(&mut self, package: &Path, admin: &Path, trust: &Trust) {
        let theirs = names(package, &mut self.list_findings);
        let ours = names(admin, &mut self.list_findings);
        let mut total = 0_usize;
        for Entry::Path(path) in self.batch.entries() {
            total = total.saturating_add(path.as_os_str().len());
        }
        let mut full = false;
        let all: BTreeSet<&Vec<u8>> = theirs.taken.iter().chain(&ours.taken).collect();
        for name in all {
            let os = OsStr::from_bytes(name);
            let their = theirs.taken.contains(name).then(|| package.join(os));
            let Some(our) = ours.taken.contains(name).then(|| admin.join(os)) else {
                if let Some(their) = their {
                    let read = Reading {
                        trust,
                        total: &mut total,
                        full: &mut full,
                    };
                    let outcome = self.read_list(&their, Folder::Package, name, None, read);
                    self.file(&their, Folder::Package, outcome.state(), &their);
                }
                continue;
            };
            let read = Reading {
                trust,
                total: &mut total,
                full: &mut full,
            };
            let outcome = self.read_list(&our, Folder::Admin, name, their.as_deref(), read);
            if let Some(their) = &their {
                let state = if outcome == Outcome::Added {
                    ListState::Replaced
                } else {
                    ListState::TurnedOff
                };
                self.file(their, Folder::Package, state, &our);
            }
            self.file(&our, Folder::Admin, outcome.state(), &our);
        }
        let mut named = 0_usize;
        let mut more = theirs.more.saturating_add(ours.more);
        for (folder, dir, past) in [
            (Folder::Package, package, &theirs.past),
            (Folder::Admin, admin, &ours.past),
        ] {
            for name in past {
                let path = dir.join(OsStr::from_bytes(name));
                if named < ERRORS_MAX {
                    named = named.saturating_add(1);
                    self.list_findings
                        .push(ListFinding::PastLimit { path: path.clone() });
                } else {
                    more = more.saturating_add(1);
                }
                self.file(&path, folder, ListState::Refused, &path);
            }
        }
        if more > 0 {
            self.list_findings
                .push(ListFinding::MoreLists { count: more });
        }
    }

    /// Adds an entry's line to [`Read::list_files`].
    fn file(&mut self, path: &Path, folder: Folder, state: ListState, set_by: &Path) {
        self.list_files.push(ListFile {
            path: path.to_path_buf(),
            folder,
            state,
            set_by: set_by.to_path_buf(),
        });
    }

    /// Reads the list `file` of `folder`, named `name`, and adds its paths
    /// to the batch while the merged batch stays within its bound. A
    /// refusal's finding names `masks`, the package's list the file's
    /// name turns off.
    fn read_list(
        &mut self,
        file: &Path,
        folder: Folder,
        name: &[u8],
        masks: Option<&Path>,
        reading: Reading<'_>,
    ) -> Outcome {
        match std::fs::metadata(file) {
            Ok(meta) if !meta.is_file() => return Outcome::NotRegular,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Outcome::NotRegular,
            _ => {}
        }
        if *reading.full {
            self.list_findings.push(ListFinding::PastBound {
                path: file.to_path_buf(),
            });
            return Outcome::LeftOut;
        }
        let read = crate::file::read_limited(file, reading.trust, BATCH_MAX).map_err(|rejected| {
            rejected
                .errors
                .into_iter()
                .next()
                .map_or(Unread::NotAFile, |e| e.problem.into())
        });
        let bytes = match read {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Outcome::Empty,
            Err(why) => {
                self.list_findings.push(ListFinding::Refused {
                    path: file.to_path_buf(),
                    why,
                    masks: masks.map(Path::to_path_buf),
                });
                return Outcome::Refused;
            }
        };
        let lines = lines(&bytes, file, false);
        for finding in lines.findings {
            if let super::Finding::Line { path, line, fault } = finding {
                self.list_findings
                    .push(ListFinding::Line { path, line, fault });
            }
        }
        if lines.more > 0 {
            self.list_findings.push(ListFinding::MoreLines {
                path: file.to_path_buf(),
                count: lines.more,
            });
        }
        let mut added = 0_usize;
        let mut new = BTreeSet::new();
        for path in &lines.paths {
            if !self.batch.holds(path) && !self.batch.turns_off(path) && new.insert(path) {
                added = added.saturating_add(path.as_os_str().len());
            }
        }
        if reading.total.saturating_add(added) > MERGED_BATCH_MAX {
            *reading.full = true;
            self.list_findings.push(ListFinding::PastBound {
                path: file.to_path_buf(),
            });
            return Outcome::LeftOut;
        }
        *reading.total = reading.total.saturating_add(added);
        if lines.paths.is_empty() {
            return Outcome::Empty;
        }
        self.batch.add_list(
            List {
                name: stem(name),
                file: file.to_path_buf(),
                folder,
            },
            lines.paths,
        );
        Outcome::Added
    }
}

/// What reading one list shares with the others: the trust rule, the
/// bytes of paths the merged batch holds so far, and whether a list has
/// already been left out for the bound.
struct Reading<'a> {
    trust: &'a Trust,
    total: &'a mut usize,
    full: &'a mut bool,
}
