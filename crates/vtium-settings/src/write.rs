//! The head file's one writer (ADR-1201 D6), and the one way a
//! configuration file is replaced (ADR-1201 D5).
//!
//! [`set_profile`] sets a head's `profile =` line and keeps every other
//! byte of the file: comments, blank lines, order, line endings and a
//! byte-order mark. It edits only a file vtium reads without an error,
//! finds the lines to change by the line numbers the parser gives, and
//! parses its own result before it returns it, so it never hands back a
//! file vtium would refuse. vtium-config links it to assign a head its
//! profile, and every other tool that changes a head's profile links the
//! same function, so the file has one writer.
//!
//! [`install_file`] replaces a file in one rename: the new bytes go to a
//! new file in the same directory, flushed to disk, and the old file stays
//! beside it as `NAME.bak`, one generation.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::config::Head;
use crate::error::Rejected;
use crate::parse::{good_name, parse};

/// Why [`set_profile`] changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileEdit {
    /// The file holds errors, so vtium reads it only in part or not at all;
    /// the writer edits only a file vtium reads whole.
    Unreadable(Rejected),
    /// No `[head NAME]` section of this name.
    NoHead(String),
    /// The profile's name breaks the name rule, so vtium would refuse it.
    BadName(String),
}

/// The text of the line that sets `profile`, as the writer adds it.
fn profile_line(profile: &str) -> String {
    format!("{} = {profile}", crate::messages::syntax::PROFILE)
}

/// `line` with the value after its `=` replaced by `value`: the blanks
/// around it, a comment after it and the line's ending kept.
fn replace_value(line: &str, value: &str) -> Option<String> {
    let eq = line.find('=')?;
    let (head, rest) = line.split_at(eq.checked_add(1)?);
    let lead = rest.find(|c| c != ' ' && c != '\t').unwrap_or(rest.len());
    let (blanks, body) = rest.split_at(lead);
    let end = body.find([' ', '\t', '\r', '\n']).unwrap_or(body.len());
    let tail = body.get(end..)?;
    Some(format!("{head}{blanks}{value}{tail}"))
}

/// The ending of `line`, `\r\n` or `\n`, or none on a last line without
/// one.
fn ending(line: &str) -> &'static str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

/// Sets the profile of the head `head` in the head file `text` to
/// `profile`, and returns the whole file with only that line changed.
///
/// A head that has a `profile =` line has its value replaced in place, the
/// blanks and any comment after it kept. A head with none gains the line
/// `profile = NAME` directly below its `[head NAME]` heading, ended as the
/// heading is.
pub fn set_profile(text: &str, head: &str, profile: &str) -> Result<String, ProfileEdit> {
    if !good_name(profile) {
        return Err(ProfileEdit::BadName(profile.to_owned()));
    }
    let parsed = parse(text).map_err(ProfileEdit::Unreadable)?;
    let found: &Head = parsed
        .config
        .heads
        .iter()
        .find(|h| h.name == head)
        .ok_or_else(|| ProfileEdit::NoHead(head.to_owned()))?;
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
    // the parser numbers lines from 1, as `split_inclusive` gives them
    if let Some(index) = found.profile_line.checked_sub(1) {
        let edited = lines.get(index).and_then(|l| replace_value(l, profile));
        if let (Some(slot), Some(edited)) = (lines.get_mut(index), edited) {
            *slot = edited;
        }
    } else if let Some(index) = found.line.checked_sub(1) {
        let heading = lines.get(index).map_or("", String::as_str);
        let (before, after) = match ending(heading) {
            "" => ("\n", ""),
            end => ("", end),
        };
        let line = format!("{before}{}{after}", profile_line(profile));
        lines.insert(index.saturating_add(1).min(lines.len()), line);
    }
    let out = lines.concat();
    // the writer's own result reads as vtium reads it, with the profile set
    match parse(&out) {
        Ok(p)
            if p.config
                .heads
                .iter()
                .any(|h| h.name == head && h.profile == profile) =>
        {
            Ok(out)
        }
        Ok(_) => Err(ProfileEdit::NoHead(head.to_owned())),
        Err(rejected) => Err(ProfileEdit::Unreadable(rejected)),
    }
}

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
