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
//! beside it as `NAME.bak`, one generation.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

mod sections;

pub use sections::{
    Of, SectionEdit, add_rule, add_section, move_rule, placement, remove_rule, remove_section,
    rename_section, rule_lines, set_key, set_rule,
};

/// The path beside `path` named `NAME` with `suffix` after it.
fn beside(path: &Path, prefix: &str, suffix: &str) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"))?;
    let mut file = std::ffi::OsString::from(prefix);
    file.push(name);
    file.push(suffix);
    Ok(path.with_file_name(file))
}

/// A new file at `path`, never through a link and never one that exists.
fn create_new(path: &Path, mode: u32) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)
}

/// `bytes` written whole to a new file at `path` with `mode`, flushed to
/// disk; on a failure the new file is removed.
fn write_new(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let written = create_new(path, mode).and_then(|mut f| {
        f.write_all(bytes)?;
        // the process's umask narrowed the mode it was created with
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.sync_all()
    });
    if written.is_err() {
        let _ = fs::remove_file(path);
    }
    written
}

/// What is at `path`, without following a link: its mode, or `None`
/// when nothing is there; a link or anything but a regular file is
/// refused, so nothing is read or replaced through a link.
fn regular(path: &Path) -> io::Result<Option<u32>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => Ok(Some(m.permissions().mode() & 0o7777)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not a regular file, and only one is replaced",
                path.display()
            ),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// A copy of the regular file at `path`, with its mode, as a new file at
/// `staging`.
fn copy_backup(path: &Path, staging: &Path) -> io::Result<()> {
    let mode =
        regular(path)?.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "nothing to keep"))?;
    let bytes = {
        let mut f = OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
            .open(path)?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut bytes)?;
        bytes
    };
    write_new(staging, &bytes, mode)
}

/// Keeps the regular file at `path` as `NAME.bak`, replacing an older one
/// in one rename: a second name for the same file where the filesystem
/// allows it, else a copy with the same mode.
fn keep_backup(path: &Path, staging: &Path) -> io::Result<()> {
    let _ = fs::remove_file(staging);
    if fs::hard_link(path, staging).is_err() {
        copy_backup(path, staging)?;
    }
    let renamed = fs::rename(staging, beside(path, "", ".bak")?);
    if renamed.is_err() {
        let _ = fs::remove_file(staging);
    }
    renamed
}

/// Replaces the file at `path` with `bytes`, mode `mode`, in one rename,
/// keeping the file it replaces as `NAME.bak`.
///
/// The new bytes go to a new file beside `path`, created without following
/// a link, flushed to disk, and renamed over `path`, so `path` is the old
/// file or the new one and never half of each. Anything at `path` but a
/// regular file, a symbolic link among them, is refused and left, so
/// nothing is read or replaced through a link. A failure before the
/// rename leaves the old file as it was and removes the new one. Run as
/// root, the new file is root's.
pub fn install_file(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let pid = std::process::id();
    let existing = regular(path)?;
    let staging = beside(path, ".", &format!(".{pid}.new"))?;
    let _ = fs::remove_file(&staging);
    write_new(&staging, bytes, mode)?;
    let result = (|| {
        if existing.is_some() {
            keep_backup(path, &beside(path, ".", &format!(".{pid}.bak"))?)?;
        }
        fs::rename(&staging, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
        return result;
    }
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    File::open(dir)?.sync_all()
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("vtium-settings-copy-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_copied_backup_keeps_the_old_file_s_mode() {
        let d = dir("mode");
        let file = d.join("vtium.conf");
        fs::write(&file, "old\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        copy_backup(&file, &d.join("copy")).unwrap();
        assert_eq!(fs::read(d.join("copy")).unwrap(), b"old\n");
        let mode = fs::metadata(d.join("copy")).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copied_backup_never_reads_through_a_link() {
        let d = dir("link");
        fs::write(d.join("secret"), "theirs\n").unwrap();
        std::os::unix::fs::symlink(d.join("secret"), d.join("vtium.conf")).unwrap();
        assert!(copy_backup(&d.join("vtium.conf"), &d.join("copy")).is_err());
        assert!(!d.join("copy").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
