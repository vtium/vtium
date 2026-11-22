//! Reading vtium.conf from disk with its checks (ADR-1401 D3, D10;
//! ADR-1403 D3).
//!
//! vtium reads the file only when it is a regular file once symlinks are
//! followed, root owns it and neither its group nor others can write it,
//! and each folder the path crosses to reach it, through every symlink on
//! the way, is owned by root and writable by neither, as `sshd`'s
//! `StrictModes` refuses a writable file. The path is walked one component
//! at a time on descriptors, each folder checked as it is crossed, and the
//! file is opened in the last folder checked, so a link swapped during the
//! walk cannot lead the read elsewhere ([`walk`]). [`folder`] walks to a
//! folder the same way, for a program that writes files there for the
//! console to read. A file that fails the ownership check is never read.
//! [`text`] checks a trusted file's bytes whole: bounded, UTF-8, and free
//! of control characters but tab, newline and a carriage return ending a
//! line, so nothing the parser quotes can carry one;
//! [`reading`](crate::reading) checks them line by line instead, so a bad
//! line drops what its scope takes.
//!
//! References:
//! - ADR-1401 D7: a vtium.conf that fails the ownership check is never
//!   read, not even its top-level keys.

pub use crate::error::Untrusted;

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::bounds::FILE_MAX;
use crate::error::{Problem, Quoted, Rejected};

/// The installed vtium.conf, which holds the whole configuration
/// (ADR-1403 D3): one file, with no drop-in directory beside it. The
/// notices and the next steps name it.
pub const CONF_FILE: &str = "/etc/vtium/vtium.conf";

/// The user the head file and its directories must belong to: root, or
/// in a test the test's own user, since no test runs as root; and the
/// directory the check stops at, `/` but in a test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trust {
    owner: u32,
    /// Whether root's files pass too, beside `owner`'s.
    or_root: bool,
    top: Option<PathBuf>,
}

impl Trust {
    /// root, every directory up to `/`, as vtium reads its files.
    pub const ROOT: Trust = Trust {
        owner: 0,
        or_root: true,
        top: None,
    };

    /// `uid`'s files and root's directories, for a test.
    pub fn owner(uid: u32) -> Trust {
        Trust {
            owner: uid,
            or_root: false,
            top: None,
        }
    }

    /// The trust of a vtium that runs as `euid` (ADR-1403 D3): root's
    /// files, and when `euid` is not root, its own files too, each in
    /// directories root or `euid` owns, under the same check of who else
    /// can write them, as `sshd`'s `StrictModes` takes a user's own files.
    /// Run as root, it is [`Trust::ROOT`].
    pub fn running_as(euid: u32) -> Trust {
        Trust {
            owner: euid,
            or_root: true,
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
        let owned = uid == self.owner || ((dir || self.or_root) && uid == 0);
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
/// bound (`MAXSYMLINKS`).
const LINKS_MAX: usize = 40;

/// A folder the walk has crossed: a descriptor on it, opened with
/// `O_PATH`, and its path from `/` as crossed.
struct Crossed {
    fd: rustix::fd::OwnedFd,
    path: PathBuf,
}

/// The file a path names, as [`walk`] found it: the folder holding it,
/// its name there, its owner and mode as found, and the verdict on the
/// folders crossed to reach it.
struct Found {
    holder: Crossed,
    name: std::ffi::OsString,
    stat: rustix::fs::Stat,
    folders: Result<(), Untrusted>,
}

/// Where [`walk`] ends: at a regular file, or at a folder, with the
/// verdict on the folders crossed to reach it, that folder included.
enum Walked {
    File(Found),
    Folder(Crossed, Result<(), Untrusted>),
}

/// Walks `path` one component at a time from `/`, on descriptors, and
/// checks each folder it crosses under `trust` as it crosses it. A folder
/// is opened with `O_PATH | O_NOFOLLOW` relative to the folder before it,
/// and the descriptor's own owner and mode are checked, so the folders
/// checked are the folders crossed, whatever links lead there. A symlink
/// is read from the folder holding it, which the walk has checked, since
/// whoever can write that folder can swap the link; its target is then
/// walked, from `/` for an absolute target and from that folder for a
/// relative one, at most [`LINKS_MAX`] links in all. `..` goes back to
/// the folder crossed before. The walk reads on past a folder the check
/// refuses, and keeps the first refusal, since `vtium check-config` reads
/// a file it does not trust. `None` when a component does not exist. A
/// path that ends at a folder ends the walk there, and the reader of a
/// file refuses it; [`folder`] takes it.
fn walk(path: &Path, trust: &Trust) -> Result<Option<Walked>, Rejected> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};
    use std::os::unix::ffi::OsStrExt;
    let io = |e: rustix::io::Errno| unreadable(&e.into());
    let names = |p: &Path| -> std::collections::VecDeque<std::ffi::OsString> {
        p.components()
            .filter_map(|c| match c {
                std::path::Component::Normal(n) => Some(n.to_owned()),
                std::path::Component::ParentDir => Some("..".into()),
                _ => None,
            })
            .collect()
    };
    let folder = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let root = || -> Result<Crossed, Rejected> {
        let fd = rustix::fs::open("/", folder, Mode::empty()).map_err(io)?;
        Ok(Crossed {
            fd,
            path: PathBuf::from("/"),
        })
    };
    let check = |crossed: &Crossed, folders: &mut Result<(), Untrusted>| -> Result<(), Rejected> {
        if folders.is_ok() && trust.covers(&crossed.path) {
            let st = rustix::fs::fstat(&crossed.fd).map_err(io)?;
            *folders = trust.check(&crossed.path, st.st_uid, st.st_mode, true);
        }
        Ok(())
    };
    let absolute = std::path::absolute(path).map_err(|e| unreadable(&e))?;
    let mut todo = names(&absolute);
    let top = root()?;
    let mut folders = Ok(());
    check(&top, &mut folders)?;
    let mut crossed = vec![top];
    let mut links = 0_usize;
    while let Some(name) = todo.pop_front() {
        if name.as_bytes() == b".." {
            if crossed.len() > 1 {
                crossed.pop();
            }
            continue;
        }
        let Some(here) = crossed.last() else {
            return Err(unreadable(&rustix::io::Errno::NOENT.into()));
        };
        let stat = match rustix::fs::statat(&here.fd, &name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(io(e)),
        };
        match FileType::from_raw_mode(stat.st_mode) {
            FileType::Symlink => {
                links = links.saturating_add(1);
                if links > LINKS_MAX {
                    return Err(io(rustix::io::Errno::LOOP));
                }
                let target = rustix::fs::readlinkat(&here.fd, &name, Vec::new()).map_err(io)?;
                let target = PathBuf::from(std::ffi::OsStr::from_bytes(target.as_bytes()));
                if target.is_absolute() {
                    crossed.truncate(1);
                }
                for n in names(&target).into_iter().rev() {
                    todo.push_front(n);
                }
            }
            FileType::Directory => {
                let fd = rustix::fs::openat(&here.fd, &name, folder, Mode::empty()).map_err(io)?;
                let next = Crossed {
                    fd,
                    path: here.path.join(&name),
                };
                check(&next, &mut folders)?;
                crossed.push(next);
            }
            _ if !todo.is_empty() => return Err(io(rustix::io::Errno::NOTDIR)),
            FileType::RegularFile => {
                let Some(holder) = crossed.pop() else {
                    return Err(unreadable(&rustix::io::Errno::NOENT.into()));
                };
                return Ok(Some(Walked::File(Found {
                    holder,
                    name,
                    stat,
                    folders,
                })));
            }
            _ => return Err(Rejected::one(None, Problem::NotAFile)),
        }
    }
    // the path ends at a folder
    let Some(end) = crossed.pop() else {
        return Err(unreadable(&rustix::io::Errno::NOENT.into()));
    };
    Ok(Some(Walked::Folder(end, folders)))
}

/// Opens the file [`walk`] found, in the folder the walk checked and never
/// by its path again, so a link swapped after the walk cannot lead the
/// read elsewhere: without following a symlink, without blocking and
/// without making a tty the controlling one, and only when what opens is
/// the regular file the walk found there.
fn open_found(found: &Found) -> Result<(std::fs::File, std::fs::Metadata), Rejected> {
    use rustix::fs::{Mode, OFlags};
    let flags =
        OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let fd = rustix::fs::openat(&found.holder.fd, &found.name, flags, Mode::empty())
        .map_err(|e| unreadable(&e.into()))?;
    let file = std::fs::File::from(fd);
    let meta = file.metadata().map_err(|e| unreadable(&e))?;
    if !meta.is_file() || (meta.dev(), meta.ino()) != (found.stat.st_dev, found.stat.st_ino) {
        return Err(Rejected::one(None, Problem::NotAFile));
    }
    Ok((file, meta))
}

/// A head file's bytes as read, and whether the ownership check trusts
/// it: [`inspect`]'s answer, which [`read`] turns into the start's
/// verdict, and which `vtium check-config` reads apart to check a
/// candidate before it is installed (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspected {
    /// The bytes read, at most [`FILE_MAX`] and one, for
    /// [`reading`](crate::reading).
    pub bytes: Vec<u8>,
    /// The file's text once [`text`]'s checks pass.
    pub text: Result<String, Rejected>,
    /// The ownership check of the file and every directory above it.
    pub trusted: Result<(), Untrusted>,
}

/// Reads the file at `path` with the ownership check beside its text:
/// `None` when it does not exist, the compiled-in configuration's case.
/// The path is looked up first, following symlinks, and anything but a
/// regular file is refused before it is opened, since opening a FIFO, a
/// watchdog or a serial port acts on it. It is opened without blocking,
/// in case it was swapped since, so that a tty never becomes the
/// controlling one, as the console opens vtium.conf, and at most
/// [`FILE_MAX`] bytes and one are read.
pub fn inspect(path: &Path, trust: &Trust) -> Result<Option<Inspected>, Rejected> {
    let Some((bytes, trusted)) = read_up_to(path, trust, FILE_MAX)? else {
        return Ok(None);
    };
    Ok(Some(Inspected {
        text: text(&bytes).map(str::to_owned),
        bytes,
        trusted,
    }))
}

/// The bytes read from a file and the ownership check's verdict on it.
type Taken = (Vec<u8>, Result<(), Untrusted>);

/// Walks to the file at `path`, opens it as [`inspect`] does, and reads at
/// most `limit` bytes and one, with the ownership check's verdict beside
/// them: `None` when the file does not exist.
fn read_up_to(path: &Path, trust: &Trust, limit: usize) -> Result<Option<Taken>, Rejected> {
    let found = match walk(path, trust)? {
        None => return Ok(None),
        Some(Walked::File(found)) => found,
        Some(Walked::Folder(..)) => return Err(Rejected::one(None, Problem::NotAFile)),
    };
    let (file, meta) = open_found(&found)?;
    // the file's own verdict first, then the first folder's refusal
    let trusted = trust
        .check(path, meta.uid(), meta.mode(), false)
        .and(found.folders);
    let mut bytes = Vec::new();
    let limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(&e))?;
    Ok(Some((bytes, trusted)))
}

/// A folder that [`folder`] walked to: a descriptor on it, and the
/// ownership check's verdict on it and on every folder crossed to reach
/// it.
#[derive(Debug)]
pub struct Folder {
    /// The folder, opened with `O_PATH`. It serves as the directory of
    /// the `*at` calls that open, create, rename and remove entries in it,
    /// and as the `dir` of [`install_file_at`](crate::install_file_at), so
    /// they act in the folder the walk checked, whatever happens to the
    /// path afterwards. Linux refuses to read the folder's entries or to
    /// flush it through an `O_PATH` descriptor, so a caller that needs
    /// either opens `.` in it for reading first.
    pub fd: rustix::fd::OwnedFd,
    /// The verdict on the folder and on every folder the walk crossed to
    /// reach it: the first one that fails, or `Ok`.
    pub trusted: Result<(), Untrusted>,
}

/// Walks to the folder at `path` as the console walks to a file it reads,
/// and checks every folder on the way under `trust`, the folder itself
/// and each folder that holds a symbolic link included. A program that
/// writes files for the console to read checks the folder this way, so
/// it approves exactly the folders the console reads from. `None` when a
/// component does not exist; [`Problem::NotAFile`] when the path ends at
/// anything but a folder.
pub fn folder(path: &Path, trust: &Trust) -> Result<Option<Folder>, Rejected> {
    match walk(path, trust)? {
        None => Ok(None),
        Some(Walked::Folder(end, trusted)) => Ok(Some(Folder {
            fd: end.fd,
            trusted,
        })),
        Some(Walked::File(_)) => Err(Rejected::one(None, Problem::NotAFile)),
    }
}

/// Reads the bytes of the file at `path` under the same checks as
/// vtium.conf, with a size limit that the caller chooses. The console
/// reads a pointer file this way, whose limit is far larger than
/// vtium.conf's [`FILE_MAX`]. The walk follows symlinks and checks every
/// folder it crosses, the file must pass the ownership check, and a file
/// longer than `limit` bytes is refused with [`Problem::TooLarge`] after
/// at most `limit` bytes and one are read. `None` when the file does not
/// exist. The bytes are not checked as text.
///
/// References:
/// - ADR-1309 D5: the console reads each pointer file under the ownership check that a font set and vtium.conf pass, with a size limit of its own.
pub fn read_limited(path: &Path, trust: &Trust, limit: usize) -> Result<Option<Vec<u8>>, Rejected> {
    let Some((bytes, trusted)) = read_up_to(path, trust, limit)? else {
        return Ok(None);
    };
    trusted.map_err(|why| Rejected::one(None, Problem::Untrusted(why)))?;
    if bytes.len() > limit {
        return Err(Rejected::one(None, Problem::TooLarge));
    }
    Ok(Some(bytes))
}

/// Reads the head file at `path` as the start does: `None` when it does
/// not exist, and its text once every check of [`inspect`] passes, the
/// ownership check first.
pub fn read(path: &Path, trust: &Trust) -> Result<Option<String>, Rejected> {
    let Some(inspected) = inspect(path, trust)? else {
        return Ok(None);
    };
    inspected
        .trusted
        .map_err(|why| Rejected::one(None, Problem::Untrusted(why)))?;
    inspected.text.map(Some)
}

/// Checks a head file's bytes: at most [`FILE_MAX`], UTF-8, and no
/// control character but tab, newline and a carriage return ending a
/// line, each line holding one named.
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
    let mut lines = s.split('\n').peekable();
    let mut n = 0_usize;
    while let Some(line) = lines.next() {
        n = n.saturating_add(1);
        let line = match lines.peek() {
            Some(_) => line.strip_suffix('\r').unwrap_or(line),
            None => line,
        };
        if let Some(c) = line.chars().find(|&c| c.is_control() && c != '\t') {
            rejected.push(Some(n), Problem::ControlCharacter(c));
        }
    }
    if rejected.is_empty() {
        Ok(s)
    } else {
        Err(rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    // The checked read with the caller's limit: a file of exactly the
    // limit reads whole, one byte more is refused as too large, a file
    // its group can write is refused before its size counts, and a
    // missing file is no error.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn read_limited_takes_the_callers_limit_and_the_ownership_check() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("settings-limited");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&dir).unwrap());
        let trust = Trust::owner(uid).below(&dir);
        let limit = FILE_MAX + 10;
        let (at, past, open) = (dir.join("at"), dir.join("past"), dir.join("open"));
        std::fs::write(&at, vec![0u8; limit]).unwrap();
        std::fs::write(&past, vec![0u8; limit + 1]).unwrap();
        std::fs::write(&open, b"x").unwrap();
        for f in [&at, &past] {
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o664)).unwrap();
        assert_eq!(
            read_limited(&at, &trust, limit).unwrap().map(|b| b.len()),
            Some(limit)
        );
        let problem = |r: Result<Option<Vec<u8>>, Rejected>| {
            r.unwrap_err().errors.first().map(|e| e.problem.clone())
        };
        assert_eq!(
            problem(read_limited(&past, &trust, limit)),
            Some(Problem::TooLarge)
        );
        assert!(matches!(
            problem(read_limited(&open, &trust, limit)),
            Some(Problem::Untrusted(Untrusted::Writable { .. }))
        ));
        assert_eq!(
            read_limited(&dir.join("none"), &trust, limit).unwrap(),
            None
        );
    }

    // A link or a folder swapped between the walk and the read cannot
    // lead the read elsewhere: the read opens the file in the folder the
    // walk crossed and checked, by its descriptor, so it reads the file
    // the walk checked, not the one a path names now
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn a_link_swapped_after_the_walk_cannot_redirect_the_read() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("settings-swap");
        let (checked, other) = (dir.join("checked"), dir.join("other"));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (folder, text) in [(&checked, "version = 1\n"), (&other, "secret\n")] {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::write(folder.join("vtium.conf"), text).unwrap();
        }
        std::os::unix::fs::symlink(&checked, dir.join("link")).unwrap();
        let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&dir).unwrap());
        let trust = Trust::owner(uid).below(&dir);
        let found = walk(&dir.join("link/vtium.conf"), &trust)
            .unwrap()
            .and_then(|walked| match walked {
                Walked::File(found) => Some(found),
                Walked::Folder(..) => None,
            })
            .unwrap();
        // the swaps, as whoever could write these folders would make them:
        // the link now names `other`, and `other` now sits where the
        // folder the walk crossed was, so neither the path given nor the
        // path crossed leads to the file the walk checked
        std::fs::remove_file(dir.join("link")).unwrap();
        std::os::unix::fs::symlink(&other, dir.join("link")).unwrap();
        std::fs::rename(&checked, dir.join("kept")).unwrap();
        std::fs::rename(&other, &checked).unwrap();
        let (mut file, _) = open_found(&found).unwrap();
        let mut read = String::new();
        file.read_to_string(&mut read).unwrap();
        assert_eq!(read, "version = 1\n");
    }
}
