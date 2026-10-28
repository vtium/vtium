//! [`Store`]: the console's check of the root-owned store (ADR-1200 D3).
//!
//! The store directory is opened with `O_DIRECTORY | O_NOFOLLOW`, and it
//! and each of its parents must be owned by the expected owner or root and
//! writable by neither group nor others. The manifest and each set are
//! opened relative to that directory with `O_NOFOLLOW`, and each must be a
//! regular file owned by the expected owner and writable by no one else. A
//! set is read once, start to end, and its SHA-256 must be its manifest
//! row's; each glyph chunk is then checked against its own hash as it is
//! read. The expected owner is root in the console, and a parameter so the
//! tests run unprivileged.

use std::fmt;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustix::fs::{Mode, OFlags};

use super::manifest::{Manifest, ManifestError, ManifestRow};
use super::read::{Set, SetError};
use crate::bounds::MANIFEST_BYTES_MAX;

/// Why the store, or a set in it, was refused.
#[derive(Debug)]
#[non_exhaustive]
pub enum StoreError {
    /// The path could not be opened or read.
    Io { path: PathBuf, error: io::Error },
    /// The store's path is not absolute.
    Relative(PathBuf),
    /// The path is a symbolic link.
    Symlink(PathBuf),
    /// The store's path is not a directory.
    NotDirectory(PathBuf),
    /// The manifest or set is not a regular file.
    NotRegular(PathBuf),
    /// The path's owner is not the one expected.
    Owner {
        path: PathBuf,
        uid: u32,
        expected: u32,
    },
    /// Group or others may write the path.
    Writable { path: PathBuf, mode: u32 },
    /// The manifest is malformed.
    Manifest { path: PathBuf, error: ManifestError },
    /// The manifest lists no set of that name.
    NoSuchSet(String),
    /// The set file was refused.
    Set { path: PathBuf, error: SetError },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            StoreError::Relative(p) => {
                write!(f, "{}: the store's path is not absolute", p.display())
            }
            StoreError::Symlink(p) => write!(f, "{}: a symbolic link", p.display()),
            StoreError::NotDirectory(p) => write!(f, "{}: not a directory", p.display()),
            StoreError::NotRegular(p) => write!(f, "{}: not a regular file", p.display()),
            StoreError::Owner {
                path,
                uid,
                expected,
            } => write!(
                f,
                "{}: owned by uid {uid}, not uid {expected}",
                path.display()
            ),
            StoreError::Writable { path, mode } => write!(
                f,
                "{}: mode {:o}, writable by group or others",
                path.display(),
                mode & 0o7777
            ),
            StoreError::Manifest { path, error } => write!(f, "{}: {error}", path.display()),
            StoreError::NoSuchSet(name) => write!(f, "the store holds no set named {name}"),
            StoreError::Set { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io { error, .. } => Some(error),
            StoreError::Manifest { error, .. } => Some(error),
            StoreError::Set { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Group- or other-writable bits.
const WRITABLE: u32 = 0o022;

/// An open store, its directory checked.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    dir: OwnedFd,
    uid: u32,
}

/// `error` from opening `path`, a link named as one.
fn open_error(path: &Path, error: io::Error) -> StoreError {
    if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) {
        StoreError::Symlink(path.to_owned())
    } else if error.raw_os_error() == Some(rustix::io::Errno::NOTDIR.raw_os_error()) {
        StoreError::NotDirectory(path.to_owned())
    } else {
        StoreError::Io {
            path: path.to_owned(),
            error,
        }
    }
}

/// Checks `meta` of `path`: owned by one of `owners`, and writable by
/// neither group nor others.
fn check_owner(path: &Path, meta: &Metadata, owners: &[u32]) -> Result<(), StoreError> {
    if !owners.contains(&meta.uid()) {
        return Err(StoreError::Owner {
            path: path.to_owned(),
            uid: meta.uid(),
            expected: owners.first().copied().unwrap_or(0),
        });
    }
    if meta.mode() & WRITABLE != 0 {
        return Err(StoreError::Writable {
            path: path.to_owned(),
            mode: meta.mode(),
        });
    }
    Ok(())
}

impl Store {
    /// Opens the store at `path`, which must be absolute, expecting `uid`
    /// to own it: 0 in the console.
    pub fn open(path: impl AsRef<Path>, uid: u32) -> Result<Store, StoreError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(StoreError::Relative(path.to_owned()));
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let dir = rustix::fs::open(path, flags, Mode::empty()).map_err(|e| {
            // O_DIRECTORY meets the link itself, so a link to a directory
            // fails as not one
            if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink()) {
                StoreError::Symlink(path.to_owned())
            } else {
                open_error(path, e.into())
            }
        })?;
        let meta = File::from(dir.try_clone().map_err(|e| open_error(path, e))?)
            .metadata()
            .map_err(|e| open_error(path, e))?;
        check_owner(path, &meta, &[uid])?;
        for parent in path.ancestors().skip(1) {
            let meta = std::fs::metadata(parent).map_err(|e| open_error(parent, e))?;
            check_owner(parent, &meta, &[uid, 0])?;
        }
        Ok(Store {
            path: path.to_owned(),
            dir,
            uid,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens `name` in the store without following a link, and checks it
    /// is a regular file of the expected owner and mode.
    fn open_file(&self, name: &str) -> Result<(File, PathBuf, u64), StoreError> {
        let path = self.path.join(name);
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | OFlags::NOCTTY
            // a FIFO put in the store fails the check rather than blocking
            | OFlags::NONBLOCK;
        let fd = rustix::fs::openat(&self.dir, name, flags, Mode::empty())
            .map_err(|e| open_error(&path, e.into()))?;
        let file = File::from(fd);
        let meta = file.metadata().map_err(|e| open_error(&path, e))?;
        if !meta.is_file() {
            return Err(StoreError::NotRegular(path));
        }
        check_owner(&path, &meta, &[self.uid])?;
        Ok((file, path, meta.len()))
    }

    /// Reads the manifest, within `MANIFEST_BYTES_MAX`.
    pub fn manifest(&self) -> Result<Manifest, StoreError> {
        let (file, path, _) = self.open_file("manifest")?;
        let mut text = String::new();
        file.take(u64::from(MANIFEST_BYTES_MAX).saturating_add(1))
            .read_to_string(&mut text)
            .map_err(|error| StoreError::Io {
                path: path.clone(),
                error,
            })?;
        Manifest::parse(&text).map_err(|error| StoreError::Manifest { path, error })
    }

    /// The set named `name`, from the manifest.
    pub fn load(&self, name: &str) -> Result<Arc<Set>, StoreError> {
        let manifest = self.manifest()?;
        let row = manifest
            .row(name)
            .ok_or_else(|| StoreError::NoSuchSet(name.to_owned()))?;
        self.load_row(row)
    }

    /// The set `row` names: its file read once and hashed, and refused
    /// unless the hash is the row's.
    pub fn load_row(&self, row: &ManifestRow) -> Result<Arc<Set>, StoreError> {
        let (file, path, len) = self.open_file(&row.file_name())?;
        Set::from_file(file, len, row.sha256)
            .map(Arc::new)
            .map_err(|error| StoreError::Set { path, error })
    }
}
