//! The trust rule on one resource file: a font file, a color map, a
//! shader, a palette or a pointer theme's file, which the records call a
//! cullet.
//!
//! vtium uses a resource only when all of these hold:
//!
//! - root owns the file and every folder above it, and neither the group
//!   nor other users can write any of them;
//! - neither the file nor any folder on its path is a symbolic link;
//! - the filesystem that holds the file is mounted at boot and stays
//!   mounted, which [`crate::mounts`] decides.
//!
//! The private `open` walks the path from `/` one name at a time on descriptors.
//! It opens each folder relative to the one before with `O_NOFOLLOW`, and
//! checks each folder's owner and mode on its descriptor, so the folders
//! checked are the folders crossed. The file is opened in the last folder
//! checked, and its owner, mode and filesystem are checked on the
//! descriptor that the caller then reads from, so nobody can swap the
//! file between the check and the read. A file that fails any check
//! counts as a file that does not exist, and the [`Refused`] verdict
//! names the file and the check it failed, in words `vtium check-config`
//! can show.
//!
//! A pointer theme's folder holds symbolic links, such as Adwaita's
//! `xterm`, which names `text`. Inside a theme folder, vtium passes a link
//! while the link resolves to a file inside the same theme folder, and
//! refuses a link that leaves it. The theme folder itself, and every folder
//! above it, passes the plain rule.
//!
//! Every kind of resource passes the same [`Gate`]: the trust rule on the
//! file vtium opened, and the batch, which lists the resources that are
//! active ([`crate::batch`]). The batch lists what a person activates: a
//! file, or a pointer theme's folder. The rule on a theme's links is the
//! only difference between kinds. Every resource is read through the
//! gate, and a test of the workspace fails when code opens one past it.
//!
//! When vtium runs unprivileged, as the tests and a run by hand do, its
//! own effective user counts as root: [`Rule::running_as`] takes the same
//! [`Trust`] that vtium.conf is read under.
//!
//! References:
//! - ADR-1205 D4: the rule, the pointer themes' links, the check on the opened file, and a failed file counting as missing.
//! - ADR-1401 D10: the owner and mode check that vtium.conf passes, which this rule extends.

use std::ffi::OsString;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use rustix::fd::OwnedFd;
use rustix::fs::{AtFlags, FileType, Mode, OFlags};
use rustix::io::Errno;

use crate::Trust;
use crate::batch::{BATCH_FILE, Batch};
use crate::error::Untrusted;
use crate::mounts::{Mounts, Place, Unsteady};

/// The symbolic links one resolution inside a theme folder follows at
/// most, the kernel's own bound (`MAXSYMLINKS`).
const LINKS_MAX: usize = 40;

/// Whose files pass the rule, and where the folder check stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    trust: Trust,
    top: Option<PathBuf>,
}

impl Rule {
    /// The rule of a vtium that runs as `euid`: root's files, and when
    /// `euid` is not root, its own files too, as vtium.conf is read.
    pub fn running_as(euid: u32) -> Rule {
        Rule {
            trust: Trust::running_as(euid),
            top: None,
        }
    }

    /// The rule under `trust`, checking every folder up to `/`.
    pub fn new(trust: Trust) -> Rule {
        Rule { trust, top: None }
    }

    /// This rule with the owner and mode check of folders stopping at
    /// `top`, for a test whose fixture sits under a folder others can
    /// write, such as `/tmp`. `top` itself is checked, and the folders
    /// above it are not. Symbolic links are refused everywhere.
    #[must_use]
    pub fn below(self, top: &Path) -> Rule {
        Rule {
            top: Some(top.to_path_buf()),
            ..self
        }
    }

    /// Whether the owner and mode check reads the folder at `dir`.
    fn covers(&self, dir: &Path) -> bool {
        self.top
            .as_deref()
            .is_none_or(|top| dir == top || !top.starts_with(dir))
    }
}

/// The check a resource file failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The path is not absolute, or holds `..`.
    NotPlain,
    /// Nothing exists at the path.
    Missing,
    /// The file, or a folder on its path, is a symbolic link.
    Link { at: PathBuf },
    /// Inside a theme folder, a symbolic link whose target leaves the
    /// theme folder.
    LeavesTheme { link: PathBuf, theme: PathBuf },
    /// A folder on the path, the file, or the theme folder belongs to
    /// another user, or others can write it.
    Untrusted(Untrusted),
    /// The path names something other than a regular file, or a name on
    /// the way names something other than a folder.
    NotAFile { at: PathBuf },
    /// The file or a folder at `at` was swapped for another while vtium
    /// opened it: the open found something other than what the look
    /// before it found.
    Swapped { at: PathBuf },
    /// The filesystem holding the file does not stay mounted.
    Unsteady(Unsteady),
    /// vtium could not read a folder or the file.
    Unreadable {
        at: PathBuf,
        kind: std::io::ErrorKind,
    },
    /// The batch does not list the file, or the pointer theme's folder,
    /// so it is not active.
    Inactive,
}

impl fmt::Display for Check {
    /// The check, as what follows "because" in a refusal, ending with a
    /// period.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Check::NotPlain => f.write_str(
                "its path is not an absolute path without `..`, which a resource needs.",
            ),
            Check::Missing => f.write_str("no file exists at that path."),
            Check::Link { at } => write!(
                f,
                "{} is a symbolic link, and vtium uses no resource reached through one.",
                at.display()
            ),
            Check::LeavesTheme { link, theme } => write!(
                f,
                "the symbolic link {} leads outside the pointer theme's folder {}.",
                link.display(),
                theme.display()
            ),
            Check::Untrusted(why) => write!(f, "{}", why.reason()),
            Check::NotAFile { at } => {
                write!(f, "{} is not a regular file or folder.", at.display())
            }
            Check::Swapped { at } => write!(
                f,
                "{} was swapped for another file or folder while vtium opened it.",
                at.display()
            ),
            Check::Unsteady(why) => write!(f, "{why}."),
            Check::Unreadable { at, kind } => {
                write!(f, "vtium cannot read {}: {kind}.", at.display())
            }
            Check::Inactive => write!(f, "it is not active: {BATCH_FILE} does not list it."),
        }
    }
}

/// A resource file that fails the rule, which counts as a file that does
/// not exist: the file, and the check it failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The file vtium was asked to open.
    pub path: PathBuf,
    /// The check the file failed.
    pub check: Check,
}

impl fmt::Display for Refused {
    /// The refusal as `vtium check-config` and the log show it: the file
    /// first, then why vtium treats it as missing.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: vtium treats this file as missing, because {}",
            self.path.display(),
            self.check
        )
    }
}

/// A resource file that passed the rule, open for reading. Read from
/// [`Opened::file`], whose descriptor is the one the checks ran on.
#[derive(Debug)]
pub struct Opened {
    /// The open file.
    pub file: std::fs::File,
    /// The file's metadata, from its descriptor.
    pub meta: std::fs::Metadata,
}

/// A folder the walk has crossed: a descriptor on it, opened with
/// `O_PATH`, and its path from `/`.
struct Crossed {
    fd: OwnedFd,
    path: PathBuf,
}

/// The walk's state: the rule, the mount table, the file asked for, and
/// the folders crossed so far.
struct Walk<'a> {
    rule: &'a Rule,
    mounts: &'a Mounts,
    asked: &'a Path,
    crossed: Vec<Crossed>,
}

/// The flags of a folder the walk crosses.
const FOLDER: OFlags = OFlags::PATH
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// The steps of `path`, one for each name and `None` for each `..`. The
/// root and each `.` are no step.
fn steps(path: &Path) -> Vec<Option<OsString>> {
    let mut out = Vec::new();
    for c in path.components() {
        match c {
            Component::Normal(n) => out.push(Some(n.to_owned())),
            Component::ParentDir => out.push(None),
            _ => {}
        }
    }
    out
}

/// The names of a plain absolute path, or `None` for a relative path or
/// one that holds `..`.
fn names(path: &Path) -> Option<Vec<OsString>> {
    if !path.is_absolute() {
        return None;
    }
    let mut out = Vec::new();
    for step in steps(path) {
        out.push(step?);
    }
    Some(out)
}

impl Walk<'_> {
    /// The refusal of the file asked for, for the failed `check`. The walk
    /// refuses from some 25 places, so the copy of the asked path stays in
    /// this one function instead of being inlined into each of them.
    #[inline(never)]
    fn refuse(&self, check: Check) -> Refused {
        Refused {
            path: self.asked.to_path_buf(),
            check,
        }
    }

    fn unreadable(&self, at: &Path, e: Errno) -> Refused {
        self.refuse(Check::Unreadable {
            at: at.to_path_buf(),
            kind: std::io::Error::from(e).kind(),
        })
    }

    fn here(&self) -> Result<&Crossed, Refused> {
        self.crossed
            .last()
            .ok_or_else(|| self.refuse(Check::Missing))
    }

    /// Checks a folder's owner and mode on its descriptor, unless it sits
    /// above the rule's top.
    fn check_folder(&self, folder: &Crossed) -> Result<(), Refused> {
        if !self.rule.covers(&folder.path) {
            return Ok(());
        }
        let st = rustix::fs::fstat(&folder.fd).map_err(|e| self.unreadable(&folder.path, e))?;
        self.rule
            .trust
            .check(&folder.path, st.st_uid, st.st_mode, true)
            .map_err(|why| self.refuse(Check::Untrusted(why)))
    }

    /// Opens `/` and checks it.
    fn start(&mut self) -> Result<(), Refused> {
        let root = Path::new("/");
        let fd =
            rustix::fs::open(root, FOLDER, Mode::empty()).map_err(|e| self.unreadable(root, e))?;
        let top = Crossed {
            fd,
            path: root.to_path_buf(),
        };
        self.check_folder(&top)?;
        self.crossed.push(top);
        Ok(())
    }

    /// What `name` is in the current folder, without following a link.
    fn stat(&self, name: &OsString) -> Result<rustix::fs::Stat, Refused> {
        let here = self.here()?;
        let at = here.path.join(name);
        match rustix::fs::statat(&here.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(st) => Ok(st),
            Err(Errno::NOENT) => Err(self.refuse(Check::Missing)),
            Err(e) => Err(self.unreadable(&at, e)),
        }
    }

    /// Enters the folder `name` of the current folder, which
    /// [`Walk::stat`] found to be a folder, and checks it. A symbolic link
    /// or anything but a folder swapped in after the look is refused as a
    /// swap, because the folder is opened with `O_NOFOLLOW | O_DIRECTORY`
    /// and checked on its descriptor.
    fn enter(&mut self, name: &OsString) -> Result<(), Refused> {
        let here = self.here()?;
        let at = here.path.join(name);
        let fd = match rustix::fs::openat(&here.fd, name, FOLDER, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOTDIR | Errno::LOOP) => {
                return Err(self.refuse(Check::Swapped { at }));
            }
            Err(Errno::NOENT) => return Err(self.refuse(Check::Missing)),
            Err(e) => return Err(self.unreadable(&at, e)),
        };
        let next = Crossed { fd, path: at };
        let st = rustix::fs::fstat(&next.fd).map_err(|e| self.unreadable(&next.path, e))?;
        if FileType::from_raw_mode(st.st_mode) != FileType::Directory {
            return Err(self.refuse(Check::Swapped { at: next.path }));
        }
        self.check_folder(&next)?;
        self.crossed.push(next);
        Ok(())
    }

    /// Opens the regular file `name` in the current folder, as `stat`
    /// found it, and checks it on its descriptor: the same file the stat
    /// found, a regular file, its owner and mode, and its filesystem.
    fn open_file(&self, name: &OsString, found: &rustix::fs::Stat) -> Result<Opened, Refused> {
        let here = self.here()?;
        let at = here.path.join(name);
        let flags =
            OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match rustix::fs::openat(&here.fd, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::LOOP) => return Err(self.refuse(Check::Link { at })),
            Err(Errno::NOENT) => return Err(self.refuse(Check::Missing)),
            Err(e) => return Err(self.unreadable(&at, e)),
        };
        let file = std::fs::File::from(fd);
        let meta = file
            .metadata()
            .map_err(|e| self.unreadable(&at, Errno::from_io_error(&e).unwrap_or(Errno::IO)))?;
        if !meta.is_file() || (meta.dev(), meta.ino()) != (found.st_dev, found.st_ino) {
            return Err(self.refuse(Check::Swapped { at }));
        }
        self.rule
            .trust
            .check(&at, meta.uid(), meta.mode(), false)
            .map_err(|why| self.refuse(Check::Untrusted(why)))?;
        let place = Place::of(&file, &at)
            .map_err(|e| self.unreadable(&at, Errno::from_io_error(&e).unwrap_or(Errno::IO)))?;
        self.mounts
            .judge(&place)
            .map_err(|why| self.refuse(Check::Unsteady(why)))?;
        Ok(Opened { file, meta })
    }

    /// Looks at the name `name` on the way to the file and enters it when
    /// it is a folder. A symbolic link or anything but a folder is
    /// refused.
    fn folder(&mut self, name: &OsString) -> Result<(), Refused> {
        let st = self.stat(name)?;
        let at = || self.here().map(|h| h.path.join(name));
        match FileType::from_raw_mode(st.st_mode) {
            FileType::Symlink => Err(self.refuse(Check::Link { at: at()? })),
            FileType::Directory => self.enter(name),
            _ => Err(self.refuse(Check::NotAFile { at: at()? })),
        }
    }

    /// Walks the plain names `names` from the current folder, refusing a
    /// symbolic link anywhere, and opens the last as the file.
    fn plain(&mut self, names: &[OsString]) -> Result<Opened, Refused> {
        let Some((last, folders)) = names.split_last() else {
            return Err(self.refuse(Check::NotAFile { at: "/".into() }));
        };
        for name in folders {
            self.folder(name)?;
        }
        let st = self.stat(last)?;
        let at = || self.here().map(|h| h.path.join(last));
        match FileType::from_raw_mode(st.st_mode) {
            FileType::Symlink => Err(self.refuse(Check::Link { at: at()? })),
            FileType::RegularFile => self.open_file(last, &st),
            _ => Err(self.refuse(Check::NotAFile { at: at()? })),
        }
    }
}

/// What every resource passes before vtium reads it, whatever its kind:
/// a font, a color scheme, a color map, a shader, an SVG pointer file or a
/// pointer theme. The batch must list the resource, and the file vtium
/// opens must pass the trust rule on its descriptor. The batch lists a
/// file by its path and a pointer theme by its folder, and vtium checks
/// the batch before it opens anything, so a file the batch does not list
/// is never opened.
///
/// References:
/// - ADR-1205 D4: one gate for every kind of resource: the batch and the trust rule on the opened file.
/// - ADR-1205 D6: a pointer theme is activated by its folder.
#[derive(Clone, Debug)]
pub struct Gate {
    /// Whose files pass the rule's owner check.
    pub rule: Rule,
    /// The mount table the rule judges a filesystem by. `None` reads the
    /// system's table at each open, so a filesystem mounted since counts as
    /// it is now; a table vtium cannot read is empty, and refuses every
    /// file, which is the safe way to fail.
    pub mounts: Option<Mounts>,
    /// The batch, which lists the active resources.
    pub batch: Batch,
}

impl Gate {
    /// The gate of `rule` and `batch`, judging filesystems by `mounts`.
    pub fn new(rule: Rule, mounts: Option<Mounts>, batch: Batch) -> Gate {
        Gate {
            rule,
            mounts,
            batch,
        }
    }

    /// Whether the batch lists `path`. A search through a folder of
    /// resources takes only the files the batch lists as candidates.
    pub fn holds(&self, path: &Path) -> bool {
        self.batch.holds(path)
    }

    /// Runs `f` with the gate's mount table, or the system's.
    fn with_mounts<T>(&self, f: impl FnOnce(&Mounts) -> T) -> T {
        match &self.mounts {
            Some(mounts) => f(mounts),
            None => f(&Mounts::system().unwrap_or_default()),
        }
    }

    /// Why vtium refuses `path`, which the batch does not list: nothing
    /// exists there, or it is not active. vtium only looks at the path,
    /// and opens nothing.
    fn unlisted(path: &Path) -> Refused {
        let check = if names(path).is_none() {
            Check::NotPlain
        } else if path.symlink_metadata().is_err() {
            Check::Missing
        } else {
            Check::Inactive
        };
        Refused {
            path: path.to_path_buf(),
            check,
        }
    }

    /// Opens the resource file at `path`, which the batch must list, under
    /// the trust rule. A file that fails counts as missing, and the
    /// refusal names the check it failed: [`Check::Missing`] when nothing
    /// exists at the path.
    pub fn open(&self, path: &Path) -> Result<Opened, Refused> {
        if !self.batch.holds(path) {
            return Err(Gate::unlisted(path));
        }
        self.with_mounts(|mounts| open(path, &self.rule, mounts))
    }

    /// Opens the file at `path` inside the pointer theme folder `theme`,
    /// which the batch must list, under the trust rule with the theme's
    /// links followed while they stay inside `theme`.
    pub fn open_in_theme(&self, theme: &Path, path: &Path) -> Result<Opened, Refused> {
        if !self.batch.holds(theme) {
            return Err(Gate::unlisted(theme));
        }
        self.with_mounts(|mounts| open_in_theme(theme, path, &self.rule, mounts))
    }

    /// Checks the pointer theme folder `theme` alone: the batch lists it,
    /// and it and every folder above it pass the plain rule on a filesystem
    /// that stays mounted. When the folder fails, every file in it fails
    /// the same way, so vtium reports the folder once instead of each file.
    pub fn theme(&self, theme: &Path) -> Result<(), Refused> {
        if !self.batch.holds(theme) {
            return Err(Gate::unlisted(theme));
        }
        self.with_mounts(|mounts| folder(theme, &self.rule, mounts))
    }
}

/// Checks the resource file at `path` under `rule`, judging its filesystem
/// by `mounts`, before a person activates it, so the batch need not list
/// it yet. vtium-config asks this before it adds the file to the batch.
/// The file is opened and closed, and nothing is read from it, so this
/// gives no way to read a resource past the gate.
///
/// References:
/// - ADR-1205 D4: vtium-config checks a resource under the trust rule before it activates it.
pub fn check(path: &Path, rule: &Rule, mounts: &Mounts) -> Result<(), Refused> {
    open(path, rule, mounts).map(drop)
}

/// [`check`] for the file at `path` inside the pointer theme folder
/// `theme`, with the theme's links followed while they stay inside it.
pub fn check_in_theme(
    theme: &Path,
    path: &Path,
    rule: &Rule,
    mounts: &Mounts,
) -> Result<(), Refused> {
    open_in_theme(theme, path, rule, mounts).map(drop)
}

/// Checks the folder at `path` under the rule: it and every folder above it
/// pass the owner and mode check, none of them is a symbolic link, and
/// `mounts` judges that its filesystem stays mounted.
fn folder(path: &Path, rule: &Rule, mounts: &Mounts) -> Result<(), Refused> {
    let mut walk = Walk {
        rule,
        mounts,
        asked: path,
        crossed: Vec::new(),
    };
    let Some(names) = names(path) else {
        return Err(walk.refuse(Check::NotPlain));
    };
    walk.start()?;
    for name in &names {
        walk.folder(name)?;
    }
    let here = walk.here()?;
    let place = Place::of(&here.fd, &here.path)
        .map_err(|e| walk.unreadable(path, Errno::from_io_error(&e).unwrap_or(Errno::IO)))?;
    walk.mounts
        .judge(&place)
        .map_err(|why| walk.refuse(Check::Unsteady(why)))
}

/// Opens the resource file at `path` under the rule, and checks it on
/// the descriptor it returns: the file and every folder above it pass the
/// owner and mode check, none of them is a symbolic link, and `mounts`
/// judges that the file's filesystem stays mounted. The file is opened without
/// blocking and without making a tty the controlling one, and only once
/// a look without following links has found a regular file, since
/// opening a FIFO or a device acts on it. A refused file counts as
/// missing.
///
/// Every read goes through [`Gate::open`], which checks the batch first,
/// so this is private to the module. A test of the workspace,
/// `gate_guard`, fails when it becomes public or when code outside this
/// module calls it.
fn open(path: &Path, rule: &Rule, mounts: &Mounts) -> Result<Opened, Refused> {
    let mut walk = Walk {
        rule,
        mounts,
        asked: path,
        crossed: Vec::new(),
    };
    let Some(names) = names(path) else {
        return Err(walk.refuse(Check::NotPlain));
    };
    walk.start()?;
    walk.plain(&names)
}

/// Opens the file at `path` inside the pointer theme folder `theme` under
/// the rule. `theme` and every folder above it pass the plain rule, and
/// so does each folder and the file inside it, but inside the theme a
/// symbolic link passes while it resolves to a place inside `theme`. A
/// link that leaves `theme`, by an absolute target outside it or by `..`
/// past it, fails with [`Check::LeavesTheme`], and the file counts as
/// missing. `path` names the file by the theme's own path, such as
/// `/usr/share/icons/Adwaita/cursors/xterm`; a path outside `theme`
/// fails as [`Check::NotPlain`]. A theme's file is read through
/// [`Gate::open_in_theme`], and this is private, as [`open`] is.
fn open_in_theme(
    theme: &Path,
    path: &Path,
    rule: &Rule,
    mounts: &Mounts,
) -> Result<Opened, Refused> {
    let mut walk = Walk {
        rule,
        mounts,
        asked: path,
        crossed: Vec::new(),
    };
    let (Some(root), Some(all)) = (names(theme), names(path)) else {
        return Err(walk.refuse(Check::NotPlain));
    };
    let Some(inside) = all.strip_prefix(root.as_slice()) else {
        return Err(walk.refuse(Check::NotPlain));
    };
    walk.start()?;
    for name in &root {
        walk.folder(name)?;
    }
    // the theme folder is the bottom of the stack from here on: `..` may
    // not pop it, and an absolute target goes back to it
    let depth = walk.crossed.len();
    // the steps left to take, the next one last, so that a link's target
    // goes on top of the steps that follow the link
    let mut todo: Vec<Option<OsString>> = Vec::with_capacity(inside.len());
    for name in inside.iter().rev() {
        todo.push(Some(name.clone()));
    }
    let mut links = 0_usize;
    // the link whose target is being walked, which `..` past the theme
    // folder names; `names` refuses `..` in the path itself
    let mut last_link = PathBuf::new();
    while let Some(next) = todo.pop() {
        let Some(name) = next else {
            if walk.crossed.len() <= depth {
                return Err(walk.refuse(Check::LeavesTheme {
                    link: last_link,
                    theme: theme.to_path_buf(),
                }));
            }
            walk.crossed.pop();
            continue;
        };
        let st = walk.stat(&name)?;
        match FileType::from_raw_mode(st.st_mode) {
            FileType::Symlink => {
                let here = walk.here()?;
                let at = here.path.join(&name);
                links = links.saturating_add(1);
                if links > LINKS_MAX {
                    return Err(walk.unreadable(&at, Errno::LOOP));
                }
                let target = rustix::fs::readlinkat(&here.fd, &name, Vec::new())
                    .map_err(|e| walk.unreadable(&at, e))?;
                let target = PathBuf::from(std::ffi::OsStr::from_bytes(target.as_bytes()));
                let leaves = || Check::LeavesTheme {
                    link: at.clone(),
                    theme: theme.to_path_buf(),
                };
                let steps: Vec<Option<OsString>> = if target.is_absolute() {
                    let Some(t) = names(&target).filter(|t| t.starts_with(&root)) else {
                        return Err(walk.refuse(leaves()));
                    };
                    walk.crossed.truncate(depth);
                    let mut below = Vec::with_capacity(t.len());
                    for name in t.into_iter().skip(root.len()) {
                        below.push(Some(name));
                    }
                    below
                } else {
                    steps(&target)
                };
                todo.extend(steps.into_iter().rev());
                last_link = at;
            }
            FileType::Directory if !todo.is_empty() => walk.enter(&name)?,
            FileType::RegularFile if todo.is_empty() => return walk.open_file(&name, &st),
            _ => {
                let at = walk.here()?.path.join(&name);
                return Err(walk.refuse(Check::NotAFile { at }));
            }
        }
    }
    Err(walk.refuse(Check::NotAFile {
        at: path.to_path_buf(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    // A file swapped in between the look that found a regular file and
    // the open is refused: the open compares the opened descriptor's
    // device and inode with the file the look found, so the checks never
    // run on one file while the caller reads another.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_file_swapped_between_the_look_and_the_open_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("cullet-swap");
        let top = std::fs::canonicalize(&*dir).unwrap();
        std::fs::set_permissions(&top, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (name, text) in [("a.bdf", "checked"), ("b.bdf", "swapped")] {
            std::fs::write(top.join(name), text).unwrap();
            std::fs::set_permissions(top.join(name), std::fs::Permissions::from_mode(0o644))
                .unwrap();
        }
        let uid = std::fs::metadata(&top).unwrap().uid();
        let rule = Rule::new(Trust::owner(uid)).below(&top);
        let mounts = Mounts::system().unwrap();
        let file = top.join("a.bdf");
        let mut walk = Walk {
            rule: &rule,
            mounts: &mounts,
            asked: &file,
            crossed: Vec::new(),
        };
        walk.start().unwrap();
        let names = names(&top).unwrap();
        for name in &names {
            walk.enter(name).unwrap();
        }
        let name = OsString::from("a.bdf");
        let found = walk.stat(&name).unwrap();
        std::fs::rename(top.join("b.bdf"), &file).unwrap();
        assert_eq!(
            walk.open_file(&name, &found).unwrap_err().check,
            Check::Swapped { at: file.clone() }
        );
    }

    // A folder swapped for a symbolic link between the look and the open
    // is refused as a swap, and the walk never crosses the link.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_folder_swapped_for_a_link_between_the_look_and_the_open_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("cullet-swap-folder");
        let top = std::fs::canonicalize(&*dir).unwrap();
        std::fs::set_permissions(&top, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir(top.join("fonts")).unwrap();
        std::fs::create_dir(top.join("elsewhere")).unwrap();
        let uid = std::fs::metadata(&top).unwrap().uid();
        let rule = Rule::new(Trust::owner(uid)).below(&top);
        let mounts = Mounts::system().unwrap();
        let file = top.join("fonts/a.bdf");
        let mut walk = Walk {
            rule: &rule,
            mounts: &mounts,
            asked: &file,
            crossed: Vec::new(),
        };
        walk.start().unwrap();
        for name in &names(&top).unwrap() {
            walk.enter(name).unwrap();
        }
        let name = OsString::from("fonts");
        assert_eq!(
            FileType::from_raw_mode(walk.stat(&name).unwrap().st_mode),
            FileType::Directory
        );
        std::fs::remove_dir(top.join("fonts")).unwrap();
        std::os::unix::fs::symlink(top.join("elsewhere"), top.join("fonts")).unwrap();
        assert_eq!(
            walk.enter(&name).unwrap_err().check,
            Check::Swapped {
                at: top.join("fonts")
            }
        );
    }
}
