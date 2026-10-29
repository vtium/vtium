//! Reading the head file from disk with its checks (ADR-1401 D3, D10).
//!
//! vtium reads the file only when it is a regular file once symlinks are
//! followed, root owns it and neither its group nor others can write it,
//! and each directory above it, on the path given and above every link
//! of its symlink chain, is owned by root and writable by neither, as `sshd`'s
//! `StrictModes` refuses a writable file. Its bytes are then bounded,
//! UTF-8, and free of control characters but tab and newline, so nothing
//! the parser quotes can carry one. A file that fails any check is invalid
//! and rejected whole (D7).

pub use crate::error::Untrusted;

use std::collections::BTreeSet;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::bounds::FILE_MAX;
use crate::error::{Problem, Quoted, Rejected};

/// The user the head file and its directories must belong to: root, or
/// in a test the test's own user, since no test runs as root; and the
/// directory the check stops at, `/` but in a test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trust {
    owner: u32,
    top: Option<PathBuf>,
}

impl Trust {
    /// root, every directory up to `/`, as vtium reads its files.
    pub const ROOT: Trust = Trust {
        owner: 0,
        top: None,
    };

    /// `uid`'s files and root's directories, for a test.
    pub fn owner(uid: u32) -> Trust {
        Trust {
            owner: uid,
            top: None,
        }
    }

    /// This trust with the directory check stopping at `top`, for a test
    /// whose fixture directory sits under a directory others can write,
    /// such as `/tmp`: `top` is checked, and the directories above it are
    /// not.
    #[must_use]
    pub fn below(self, top: &Path) -> Trust {
        let top = std::fs::canonicalize(top).unwrap_or_else(|_| top.to_path_buf());
        Trust {
            top: Some(top),
            ..self
        }
    }

    /// Whether the directory check reads `dir`: every directory but those
    /// above `top`.
    fn covers(&self, dir: &Path) -> bool {
        self.top
            .as_deref()
            .is_none_or(|top| dir == top || !top.starts_with(dir))
    }

    /// Checks one entry's owner and mode: a file must be the trusted
    /// user's, a directory the trusted user's or root's, and neither
    /// writable by its group or by others.
    pub fn check(&self, path: &Path, uid: u32, mode: u32, dir: bool) -> Result<(), Untrusted> {
        let owned = uid == self.owner || (dir && uid == 0);
        let quoted = || Quoted::path(path);
        if !owned {
            return Err(Untrusted::Owner {
                path: quoted(),
                uid,
            });
        }
        if mode & 0o022 != 0 {
            return Err(Untrusted::Writable {
                path: quoted(),
                mode: mode & 0o7777,
            });
        }
        Ok(())
    }
}

fn unreadable(e: &std::io::Error) -> Rejected {
    Rejected::one(None, Problem::Unreadable(e.kind()))
}

/// The links one resolution of a path follows at most, the kernel's
/// bound (`MAXSYMLINKS`); `open` has followed the chain within it already.
const LINKS_MAX: usize = 40;

/// The directories above `path` as given, and above each link of its
/// symlink chain and the file it ends at, each resolved: the directory
/// holding every link is checked, since whoever can write it can swap
/// the link.
fn directories(path: &Path) -> Result<BTreeSet<PathBuf>, Rejected> {
    let mut current = std::path::absolute(path).map_err(|e| unreadable(&e))?;
    let mut dirs: BTreeSet<PathBuf> = current
        .parent()
        .map(|p| p.ancestors().map(Path::to_path_buf).collect())
        .unwrap_or_default();
    for _ in 0..=LINKS_MAX {
        let parent = current.parent().unwrap_or(Path::new("/"));
        let holder = std::fs::canonicalize(parent).map_err(|e| unreadable(&e))?;
        dirs.extend(holder.ancestors().map(Path::to_path_buf));
        let meta = std::fs::symlink_metadata(&current).map_err(|e| unreadable(&e))?;
        if !meta.file_type().is_symlink() {
            return Ok(dirs);
        }
        let target = std::fs::read_link(&current).map_err(|e| unreadable(&e))?;
        // a relative target is read from the link's own directory
        current = holder.join(target);
    }
    Err(unreadable(&rustix::io::Errno::LOOP.into()))
}

/// Reads the head file at `path`: `None` when it does not exist, the
/// compiled-in configuration's case, and its text once every check
/// passes. It is opened without blocking, so a FIFO is refused at once,
/// and so that a tty never becomes the controlling one, as the console
/// opens kmscon.conf.
pub fn read(path: &Path, trust: &Trust) -> Result<Option<String>, Rejected> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::open(path, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(e) if e == rustix::io::Errno::NOENT => return Ok(None),
        Err(e) => return Err(unreadable(&e.into())),
    };
    let file = std::fs::File::from(fd);
    let meta = file.metadata().map_err(|e| unreadable(&e))?;
    if !meta.is_file() {
        return Err(Rejected::one(None, Problem::NotAFile));
    }
    let untrusted = |why| Rejected::one(None, Problem::Untrusted(why));
    trust
        .check(path, meta.uid(), meta.mode(), false)
        .map_err(untrusted)?;
    for dir in directories(path)?.into_iter().filter(|d| trust.covers(d)) {
        let meta = std::fs::metadata(&dir).map_err(|e| unreadable(&e))?;
        trust
            .check(&dir, meta.uid(), meta.mode(), true)
            .map_err(untrusted)?;
    }
    let mut bytes = Vec::new();
    let limit = u64::try_from(FILE_MAX)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(&e))?;
    text(&bytes).map(|t| Some(t.to_owned()))
}

/// Checks a head file's bytes: at most [`FILE_MAX`], UTF-8, and no
/// control character but tab and newline, each line holding one named.
pub fn text(bytes: &[u8]) -> Result<&str, Rejected> {
    if bytes.len() > FILE_MAX {
        return Err(Rejected::one(None, Problem::TooLarge));
    }
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            let valid = bytes.get(..e.valid_up_to()).unwrap_or_default();
            let line = valid
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                .saturating_add(1);
            return Err(Rejected::one(Some(line), Problem::NotUtf8));
        }
    };
    let mut rejected = Rejected::default();
    for (n, line) in s.split('\n').enumerate() {
        if let Some(c) = line.chars().find(|&c| c.is_control() && c != '\t') {
            rejected.push(Some(n.saturating_add(1)), Problem::ControlCharacter(c));
        }
    }
    if rejected.is_empty() {
        Ok(s)
    } else {
        Err(rejected)
    }
}
