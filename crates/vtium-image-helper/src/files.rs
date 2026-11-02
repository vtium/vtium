//! The file rules (ADR-1101 D5): which files and shared-memory objects a
//! worker may open, how it opens them so that it opens nothing but a
//! regular file, and how a temporary file or a shared-memory object is
//! removed so that only the file that was checked goes.
//!
//! A path is resolved first, following symlinks as the kitty spec asks,
//! and refused in `/proc`, `/sys`, and `/dev` other than `/dev/shm` before
//! anything is opened, since opening some files has effects. The resolved
//! path is then opened as a path-only descriptor with symlinks refused,
//! which opens nothing; only once `fstat` shows a regular file and
//! `fstatfs` shows no kernel pseudo-filesystem is the file itself opened,
//! read-only, through `/proc/self/fd`, and its device and inode must be
//! the path-only descriptor's. The opened descriptor's own path must pass
//! the location check again, which catches a symlink swapped in between.
//! Every refusal is one answer, `UNREADABLE`; its reason goes only to
//! vtium's debug log.

use std::ffi::OsStr;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, ResolveFlags};

use crate::bounds::{SHM_DIR, SHM_NAME_MAX, TEMP_MARK};

/// A file's identity: its device and inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
}

/// A regular file a worker opened under the rules.
#[derive(Debug)]
pub struct Opened {
    pub file: OwnedFd,
    pub id: Identity,
    /// Its size in bytes.
    pub size: u64,
    /// Its resolved path, which a removal names.
    pub resolved: PathBuf,
}

/// The magic numbers of the kernel's pseudo-filesystems, from
/// `<linux/magic.h>` and, for configfs and rpc_pipefs, the kernel's own
/// sources: a file on one of them is refused wherever it is mounted.
const PSEUDO_FS: [u32; 19] = [
    0x9fa0,      // procfs
    0x6265_6572, // sysfs
    0x6462_6720, // debugfs
    0x7472_6163, // tracefs
    0x7363_6673, // securityfs
    0x0027_e0eb, // cgroup
    0x6367_7270, // cgroup2
    0x1cd1,      // devpts
    0x6265_6570, // configfs
    0xcafe_4a11, // bpf
    0x6165_676c, // pstore
    0xde5e_81e4, // efivarfs
    0xf97c_ff8c, // selinuxfs
    0x4341_5d53, // smackfs
    0x4249_4e4d, // binfmt_misc
    0x6e73_6673, // nsfs
    0x0904_1934, // anonymous inodes
    0x9fa2,      // usbfs
    0x6759_6969, // rpc_pipefs
];

/// `path` is `dir` or lies beneath it; `dir` has no trailing slash.
fn in_dir(path: &[u8], dir: &[u8]) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.first() == Some(&b'/'))
}

/// A resolved path that may be opened: not in `/proc`, `/sys`, or `/dev`
/// other than `/dev/shm`.
pub fn location_ok(path: &Path) -> bool {
    let p = path.as_os_str().as_bytes();
    !in_dir(p, b"/proc")
        && !in_dir(p, b"/sys")
        && (!in_dir(p, b"/dev") || in_dir(p, SHM_DIR.as_bytes()))
}

/// A resolved path a temporary file may have: in `/tmp`, `/dev/shm` or
/// `tmpdir`, vtium's own `TMPDIR`, and holding [`TEMP_MARK`].
pub fn temp_ok(path: &Path, tmpdir: Option<&Path>) -> bool {
    let p = path.as_os_str().as_bytes();
    let place = in_dir(p, b"/tmp")
        || in_dir(p, SHM_DIR.as_bytes())
        || tmpdir.is_some_and(|t| in_dir(p, t.as_os_str().as_bytes()));
    place
        && p.windows(TEMP_MARK.len())
            .any(|w| w == TEMP_MARK.as_bytes())
}

/// The filesystem type of `fd`, as `<linux/magic.h>` numbers it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a filesystem magic is 32 bits, which f_type holds signed on some architectures"
)]
fn fs_type(fd: impl AsFd) -> Option<u32> {
    rustix::fs::fstatfs(fd).ok().map(|fs| fs.f_type as u32)
}

/// Whether `fd` lies on a kernel pseudo-filesystem, wherever it is
/// mounted; a failure to tell counts as one.
pub fn pseudo_fs(fd: impl AsFd) -> bool {
    fs_type(fd).is_none_or(|t| PSEUDO_FS.contains(&t))
}

/// The identity and size of an open file, if it is a regular one.
fn regular(fd: impl AsFd) -> Option<(Identity, u64)> {
    let st = rustix::fs::fstat(fd).ok()?;
    (FileType::from_raw_mode(st.st_mode) == FileType::RegularFile).then_some(())?;
    #[expect(
        clippy::useless_conversion,
        reason = "st_dev and st_ino are u64 on some architectures and narrower on others"
    )]
    let id = Identity {
        dev: u64::from(st.st_dev),
        ino: u64::from(st.st_ino),
    };
    Some((id, u64::try_from(st.st_size).unwrap_or(0)))
}

/// Opens `path` under `dir`, resolved as `resolve` says, without opening
/// anything but a regular file: a path-only descriptor first, then, once
/// it shows a regular file on no pseudo-filesystem, the file itself
/// through `/proc/self/fd`, which must be the same file.
fn open_regular(
    dir: impl AsFd,
    path: &Path,
    resolve: ResolveFlags,
) -> Result<(OwnedFd, Identity, u64), String> {
    let pfd = rustix::fs::openat2(
        dir,
        path,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        resolve,
    )
    .map_err(|e| format!("cannot reach {}: {e}", path.display()))?;
    let Some((id, _)) = regular(&pfd) else {
        return Err(format!("{} is not a regular file", path.display()));
    };
    if pseudo_fs(&pfd) {
        return Err(format!(
            "{} is on a kernel pseudo-filesystem",
            path.display()
        ));
    }
    let proc = format!("/proc/self/fd/{}", pfd.as_raw_fd());
    let file = rustix::fs::open(
        proc.as_str(),
        OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    drop(pfd);
    match regular(&file) {
        Some((opened, size)) if opened == id => Ok((file, id, size)),
        _ => Err(format!(
            "the file opened for {} is not the file checked",
            path.display()
        )),
    }
}

/// The path the kernel gives the open `fd`.
fn fd_path(fd: impl AsFd) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_fd().as_raw_fd())).ok()
}

/// Opens the file `name` of a `t=f` or `t=t` request: resolved, refused in
/// the places [`location_ok`] refuses, then opened by the regular-file
/// rule with no symlink on the resolved path, and its opened path checked
/// again. With `temp`, both paths must also pass [`temp_ok`]. It returns
/// whether both pass [`temp_ok`] beside the file, for a discard.
pub fn open_file(name: &[u8], temp: bool, tmpdir: Option<&Path>) -> Result<(Opened, bool), String> {
    let resolved = std::fs::canonicalize(OsStr::from_bytes(name))
        .map_err(|e| format!("cannot resolve the path: {e}"))?;
    if !location_ok(&resolved) {
        return Err(format!("refused location {}", resolved.display()));
    }
    if temp && !temp_ok(&resolved, tmpdir) {
        return Err(format!("not a temporary file: {}", resolved.display()));
    }
    let (file, id, size) = open_regular(CWD, &resolved, ResolveFlags::NO_SYMLINKS)?;
    let opened = fd_path(&file);
    let both = opened
        .as_deref()
        .is_some_and(|o| location_ok(o) && temp_ok(o, tmpdir) && temp_ok(&resolved, tmpdir));
    if !opened.as_deref().is_some_and(location_ok) || (temp && !both) {
        return Err(format!(
            "the file opened for {} is not the file checked",
            resolved.display()
        ));
    }
    Ok((
        Opened {
            file,
            id,
            size,
            resolved,
        },
        both,
    ))
}

/// The object name of a shared-memory name as a program gives it: one
/// leading `/` or none, then a name with no other `/`, not empty, `.` or
/// `..`, of at most [`SHM_NAME_MAX`] bytes.
pub fn shm_name(name: &[u8]) -> Option<&[u8]> {
    let bare = name.strip_prefix(b"/").unwrap_or(name);
    let fits = !bare.is_empty()
        && bare.len() <= SHM_NAME_MAX
        && !bare.contains(&b'/')
        && !bare.contains(&0)
        && bare != b"."
        && bare != b"..";
    fits.then_some(bare)
}

/// Opens the shared-memory object `name` of a `t=s` request beneath
/// `/dev/shm`, with no symlink followed and no escape from the directory,
/// by the regular-file rule.
pub fn open_shm(name: &[u8]) -> Result<Opened, String> {
    let bare = shm_name(name).ok_or("refused shared-memory name")?;
    let dir = rustix::fs::open(
        SHM_DIR,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("cannot open {SHM_DIR}: {e}"))?;
    let path = Path::new(OsStr::from_bytes(bare));
    let (file, id, size) = open_regular(
        &dir,
        path,
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS,
    )?;
    Ok(Opened {
        file,
        id,
        size,
        resolved: Path::new(SHM_DIR).join(path),
    })
}

/// Removes the checked file at `resolved`, whose identity is `id`: only
/// the entry with that device and inode, in a parent directory reached
/// with no symlink, and only when this process's own account owns it. A
/// root helper ignores the sticky bit of `/tmp` and `/dev/shm`, so without
/// the owner check it could be made to remove another user's file.
pub fn remove_checked(resolved: &Path, id: Identity) -> Result<(), String> {
    let (Some(parent), Some(base)) = (resolved.parent(), resolved.file_name()) else {
        return Err(format!("no file name in {}", resolved.display()));
    };
    let dir = rustix::fs::openat2(
        CWD,
        parent,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS,
    )
    .map_err(|e| format!("cannot open {} without symlinks: {e}", parent.display()))?;
    let entry = rustix::fs::statat(&dir, base, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|e| format!("{} is gone: {e}", resolved.display()))?;
    #[expect(
        clippy::useless_conversion,
        reason = "st_dev and st_ino are u64 on some architectures and narrower on others"
    )]
    let same = u64::from(entry.st_dev) == id.dev && u64::from(entry.st_ino) == id.ino;
    if !same {
        return Err(format!("{} is no longer the file read", resolved.display()));
    }
    let me = rustix::process::geteuid().as_raw();
    if entry.st_uid != me {
        return Err(format!(
            "{} belongs to uid {}, not the helper's {me}",
            resolved.display(),
            entry.st_uid
        ));
    }
    rustix::fs::unlinkat(&dir, base, AtFlags::empty())
        .map_err(|e| format!("cannot remove {}: {e}", resolved.display()))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn the_refused_places_and_the_temporary_file_rule() {
        for refused in ["/proc", "/proc/self/mem", "/sys/kernel", "/dev/sda", "/dev"] {
            assert!(!location_ok(Path::new(refused)), "{refused}");
        }
        for ok in [
            "/devices",
            "/tmp/x",
            "/dev/shm/x",
            "/procfs",
            "/home/u/a.png",
        ] {
            assert!(location_ok(Path::new(ok)), "{ok}");
        }
        let tmpdir = Some(Path::new("/run/user/1000/t"));
        assert!(temp_ok(Path::new("/tmp/tty-graphics-protocol-1"), None));
        assert!(temp_ok(Path::new("/dev/shm/a-tty-graphics-protocol"), None));
        assert!(temp_ok(
            Path::new("/run/user/1000/t/tty-graphics-protocol"),
            tmpdir
        ));
        assert!(!temp_ok(Path::new("/tmp/picture.png"), None));
        assert!(!temp_ok(Path::new("/tmpx/tty-graphics-protocol"), None));
        assert!(!temp_ok(Path::new("/home/tty-graphics-protocol"), tmpdir));
    }

    #[test]
    fn a_shared_memory_name_with_or_without_its_slash() {
        assert_eq!(shm_name(b"/abc"), Some(&b"abc"[..]));
        assert_eq!(shm_name(b"abc"), Some(&b"abc"[..]));
        for refused in [&b""[..], b"/", b"/.", b"..", b"/a/b", b"//a", b"a\0"] {
            assert_eq!(shm_name(refused), None, "{refused:?}");
        }
        assert!(shm_name(&[b'a'; SHM_NAME_MAX]).is_some());
        assert!(shm_name(&[b'a'; SHM_NAME_MAX + 1]).is_none());
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vtium-image-helper-files-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_symlink_into_a_refused_place_a_fifo_and_a_directory_are_refused() {
        let dir = scratch("refusals");
        let link = dir.join("mem");
        std::os::unix::fs::symlink("/proc/self/status", &link).unwrap();
        let why = open_file(link.as_os_str().as_bytes(), false, None).unwrap_err();
        assert!(why.contains("refused location /proc"), "{why}");
        let fifo = dir.join("fifo");
        rustix::fs::mknodat(CWD, &fifo, FileType::Fifo, Mode::RUSR | Mode::WUSR, 0).unwrap();
        let why = open_file(fifo.as_os_str().as_bytes(), false, None).unwrap_err();
        assert!(why.contains("not a regular file"), "{why}");
        let why = open_file(dir.as_os_str().as_bytes(), false, None).unwrap_err();
        assert!(why.contains("not a regular file"), "{why}");
        let file = dir.join("a");
        std::fs::write(&file, b"12345").unwrap();
        let (opened, temp) = open_file(file.as_os_str().as_bytes(), false, None).unwrap();
        assert_eq!(opened.size, 5);
        assert!(!temp, "{} is no temporary file", file.display());
        let why = open_file(file.as_os_str().as_bytes(), true, None).unwrap_err();
        assert!(why.contains("not a temporary file"), "{why}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removal_takes_only_the_file_that_was_read() {
        let dir = scratch("removal");
        let file = dir.join("a");
        std::fs::write(&file, b"x").unwrap();
        let (opened, _) = open_file(file.as_os_str().as_bytes(), false, None).unwrap();
        // another file put in its place stays
        std::fs::remove_file(&file).unwrap();
        std::fs::write(&file, b"y").unwrap();
        let why = remove_checked(&opened.resolved, opened.id).unwrap_err();
        assert!(why.contains("no longer the file read"), "{why}");
        assert!(file.exists());
        let (opened, _) = open_file(file.as_os_str().as_bytes(), false, None).unwrap();
        remove_checked(&opened.resolved, opened.id).unwrap();
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
