//! vtium.conf's section writer (ADR-1201 D6, ADR-1403 D8), and the one
//! way a configuration file is replaced (ADR-1201 D5).
//!
//! The section writer, in [`sections`], keeps every byte of the file it
//! does not need to change: comments, blank lines, order, line endings and
//! a byte-order mark. It edits only sections vtium reads without an error,
//! and parses its own result before it returns it, so it never hands back
//! a file vtium would refuse. vtium-config links it, so the sections have
//! one writer.
//!
//! [`install_file`] replaces a file in one rename: the new bytes go to a
//! new file in the same directory, flushed to disk, and the old file stays
//! beside it as `NAME.bak`, one generation. [`install_file_at`] takes the
//! same steps in a folder the caller holds open, so a caller that checked
//! the folder writes there even when a link on its path moves afterwards.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{AtFlags, FileType, Mode, OFlags};

mod sections;

pub use sections::{
    Of, SectionEdit, add_rule, add_section, move_rule, placement, remove_rule, remove_section,
    rename_section, rule_lines, set_key, set_rule,
};

/// `.NAME` with `suffix` after it, the name of a file staged beside
/// `NAME`.
fn dotted(name: &OsStr, suffix: &str) -> OsString {
    let mut out = OsString::from(".");
    out.push(name);
    out.push(suffix);
    out
}

/// `NAME.bak`, the name the file a replacement replaces is kept under.
fn bak(name: &OsStr) -> OsString {
    let mut out = name.to_owned();
    out.push(".bak");
    out
}

/// A new file `name` in the folder `dir`, never through a link and never
/// one that exists.
fn create_new(dir: &impl AsFd, name: &OsStr, mode: u32) -> io::Result<File> {
    let fd = rustix::fs::openat(
        dir,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(mode),
    )?;
    Ok(File::from(fd))
}

/// `bytes` written whole to a new file `name` in `dir` with `mode`,
/// flushed to disk; on a failure the new file is removed.
fn write_new(dir: &impl AsFd, name: &OsStr, bytes: &[u8], mode: u32) -> io::Result<()> {
    let written = create_new(dir, name, mode).and_then(|mut f| {
        f.write_all(bytes)?;
        // the process's umask narrowed the mode it was created with
        rustix::fs::fchmod(&f, Mode::from_raw_mode(mode))?;
        f.sync_all()
    });
    if written.is_err() {
        let _ = rustix::fs::unlinkat(dir, name, AtFlags::empty());
    }
    written
}

/// What is at `name` in `dir`, without following a link: its mode, or
/// `None` when nothing is there; a link or anything but a regular file is
/// refused, so nothing is read or replaced through a link. `shown` is the
/// file as the refusal names it.
fn regular(dir: &impl AsFd, name: &OsStr, shown: &Path) -> io::Result<Option<u32>> {
    match rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::RegularFile => {
            Ok(Some(st.st_mode & 0o7777))
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not a regular file, so it cannot be replaced",
                shown.display()
            ),
        )),
        Err(rustix::io::Errno::NOENT) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// A copy of the regular file `name` in `dir`, with its mode, as a new
/// file `staging` there.
fn copy_backup(dir: &impl AsFd, name: &OsStr, staging: &OsStr) -> io::Result<()> {
    let fd = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let st = rustix::fs::fstat(&fd)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the earlier file is not a regular file, so it is not kept",
        ));
    }
    let mut bytes = Vec::new();
    File::from(fd).read_to_end(&mut bytes)?;
    write_new(dir, staging, &bytes, st.st_mode & 0o7777)
}

/// Keeps the regular file `name` in `dir` as `NAME.bak`, replacing an
/// older one in one rename: a second name for the same file, `staging`,
/// where the filesystem allows it, else a copy with the same mode.
fn keep_backup(dir: &impl AsFd, name: &OsStr, staging: &OsStr) -> io::Result<()> {
    let _ = rustix::fs::unlinkat(dir, staging, AtFlags::empty());
    if rustix::fs::linkat(dir, name, dir, staging, AtFlags::empty()).is_err() {
        copy_backup(dir, name, staging)?;
    }
    let renamed = rustix::fs::renameat(dir, staging, dir, bak(name));
    if renamed.is_err() {
        let _ = rustix::fs::unlinkat(dir, staging, AtFlags::empty());
    }
    renamed.map_err(io::Error::from)
}

/// [`install_file_at`]'s steps, with `shown` naming the file in a refusal.
fn install(dir: &impl AsFd, name: &OsStr, shown: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let pid = std::process::id();
    let existing = regular(dir, name, shown)?;
    let staging = dotted(name, &format!(".{pid}.new"));
    let _ = rustix::fs::unlinkat(dir, &staging, AtFlags::empty());
    write_new(dir, &staging, bytes, mode)?;
    let result = (|| {
        if existing.is_some() {
            keep_backup(dir, name, &dotted(name, &format!(".{pid}.bak")))?;
        }
        rustix::fs::renameat(dir, &staging, dir, name).map_err(io::Error::from)
    })();
    if result.is_err() {
        let _ = rustix::fs::unlinkat(dir, &staging, AtFlags::empty());
        return result;
    }
    rustix::fs::fsync(dir).map_err(io::Error::from)
}

/// Replaces the file `name` in the open folder `dir` with `bytes`, mode
/// `mode`, in one rename, keeping the file it replaces as `NAME.bak`.
///
/// Every step works relative to `dir`, so a caller that has checked the
/// folder and holds it open writes into that folder even when a link on
/// the folder's path changes afterwards. The new bytes go to a new file
/// `.NAME.PID.new` in the folder, created without following a link and
/// flushed to disk, and that file is renamed over `NAME`, so `NAME` is the
/// old file or the new one and never half of each. The folder is flushed
/// to disk after the rename. Anything at `NAME` but a regular file, a
/// symbolic link among them, is refused and left, so nothing is read or
/// replaced through a link. A failure before the rename leaves the old
/// file as it was and removes the new one. Run as root, the new file is
/// root's. `name` is one name in `dir`, with no `/` in it.
pub fn install_file_at(dir: &impl AsFd, name: &OsStr, bytes: &[u8], mode: u32) -> io::Result<()> {
    install(dir, name, Path::new(name), bytes, mode)
}

/// Replaces the file at `path` with `bytes`, mode `mode`, in one rename,
/// keeping the file it replaces as `NAME.bak`.
///
/// It opens the folder that holds `path` and runs [`install_file_at`]'s
/// steps there: the new bytes go to a new file beside `path`, created
/// without following a link, flushed to disk, and renamed over `path`, so
/// `path` is the old file or the new one and never half of each. Anything
/// at `path` but a regular file, a symbolic link among them, is refused
/// and left, so nothing is read or replaced through a link. A failure
/// before the rename leaves the old file as it was and removes the new
/// one. Run as root, the new file is root's.
pub fn install_file(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let dir: OwnedFd = rustix::fs::open(
        parent,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    install(&dir, name, path, bytes, mode)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    /// A directory of the test's own, removed when the test ends.
    fn dir(name: &str) -> Scratch {
        Scratch::new(&format!("settings-copy-{name}"))
    }

    #[test]
    fn a_copied_backup_keeps_the_old_file_s_mode() {
        let d = dir("mode");
        let file = d.join("vtium.conf");
        fs::write(&file, "old\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let held = File::open(&*d).unwrap();
        copy_backup(&held, OsStr::new("vtium.conf"), OsStr::new("copy")).unwrap();
        assert_eq!(fs::read(d.join("copy")).unwrap(), b"old\n");
        let mode = fs::metadata(d.join("copy")).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_copied_backup_never_reads_through_a_link() {
        let d = dir("link");
        fs::write(d.join("secret"), "theirs\n").unwrap();
        std::os::unix::fs::symlink(d.join("secret"), d.join("vtium.conf")).unwrap();
        let held = File::open(&*d).unwrap();
        assert!(copy_backup(&held, OsStr::new("vtium.conf"), OsStr::new("copy")).is_err());
        assert!(!d.join("copy").exists());
    }
}
