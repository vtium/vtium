//! Whether a filesystem stays mounted for the system's life, read from the
//! mount table.
//!
//! vtium activates a resource only from a filesystem that is mounted at
//! boot and stays mounted. A filesystem that can go away or change under
//! vtium while it runs fails: a network filesystem, a FUSE filesystem
//! served by a user's program, an automounted filesystem, and a removable
//! disk. vtium reads the kernel's mount table, `/proc/self/mountinfo`, and
//! each block device's folder in sysfs. A filesystem such as btrfs, which
//! reports an anonymous device number, is judged by the device its mount's
//! source names. A device-mapper or md device is
//! judged by the disks beneath it, and a loop device by the filesystem
//! that holds its backing file. A disk counts as removable when its
//! `removable` flag is 1 or when it sits on a USB bus. When vtium cannot
//! read a disk's folder, the file fails. [`Mounts::parse`] takes the table
//! and the sysfs folder as parameters, so a test feeds it a mount table
//! and a sysfs folder of its own.
//!
//! vtium finds the mount that holds an open file by the mount ID that
//! `statx` reports for the file's descriptor, which is the first field of
//! the mount table's line. The device number that `stat` reports is not
//! enough: on btrfs it names the subvolume, and the mount table names the
//! whole filesystem, so the two differ. On a kernel older than 5.8, which
//! has no mount ID, vtium falls back on the device number and then on the
//! longest mount point above the file's path.
//!
//! References:
//! - ADR-1205 D4: a resource is activated only from a filesystem that is mounted at boot and stays mounted, and the note of 2026-11-19 states how vtium tells one.

use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The kernel's mount table of the reading process's mount namespace.
pub const MOUNTINFO: &str = "/proc/self/mountinfo";

/// The folder sysfs is mounted on, where vtium reads whether a disk is
/// removable.
pub const SYS: &str = "/sys";

/// The filesystem types of network filesystems, whose files a server
/// elsewhere can change or withdraw. `virtiofs` and `9p` are included
/// because they share a host's folder with a virtual machine, the same
/// arrangement seen from the guest.
const NETWORK: [&str; 18] = [
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "ceph",
    "9p",
    "afs",
    "ncpfs",
    "coda",
    "glusterfs",
    "lustre",
    "gpfs",
    "beegfs",
    "orangefs",
    "pvfs2",
    "virtiofs",
    "davfs",
];

/// Whether a disk on a USB bus counts as removable even when its own
/// `removable` flag is 0, as many USB disks report. Setting this to `false`
/// leaves the flag alone to decide.
const USB_IS_REMOVABLE: bool = true;

/// How many devices deep vtium follows a stack of loop, device-mapper and
/// md devices before it gives up and refuses the file.
const STACK_MAX: usize = 8;

/// One line of the mount table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// The mount's ID, the line's first field.
    pub id: u64,
    /// The ID of the mount this one sits on.
    pub parent: u64,
    /// The major and minor number of the filesystem's device.
    pub dev: (u32, u32),
    /// Where the filesystem is mounted.
    pub point: PathBuf,
    /// The filesystem's type, such as `ext4`, `nfs4` or `fuse.sshfs`.
    pub fstype: String,
    /// The mount's source, such as `/dev/sdb1` or `server:/export`.
    pub source: String,
}

/// The mount table and the sysfs folder to read disks' attributes from.
///
/// A table read from a file, as [`Mounts::system`] reads it, may be read
/// before the files it judges are opened. A file on a mount made after
/// that has a mount ID the table lacks, so [`Mounts::judge`] reads the
/// file again once when it cannot find a file's mount, and refuses the
/// file if the mount is still missing. A table that [`Mounts::parse`]
/// made from text has no file to read again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mounts {
    list: Vec<Mount>,
    sys: PathBuf,
    source: Option<PathBuf>,
}

/// Where a file sits, as the mount check needs it: the mount ID `statx`
/// reports, when the kernel reports one, the device number, and the path
/// the file was reached by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// The mount ID of the file's mount, from `statx` (Linux 5.8 and
    /// later).
    pub mount_id: Option<u64>,
    /// The major and minor number of the file's device, from `stat`.
    pub dev: (u32, u32),
    /// The path vtium reached the file by, with no symbolic link on it.
    pub path: PathBuf,
}

impl Place {
    /// Where the file at `path` sits, looked up by its path without
    /// following a symbolic link at its end.
    pub fn at(path: &Path) -> std::io::Result<Place> {
        use rustix::fs::{AtFlags, CWD, StatxFlags};
        let x = rustix::fs::statx(
            CWD,
            path,
            AtFlags::SYMLINK_NOFOLLOW,
            StatxFlags::BASIC_STATS | StatxFlags::MNT_ID,
        )?;
        Ok(Place {
            mount_id: (x.stx_mask & StatxFlags::MNT_ID.bits() != 0).then_some(x.stx_mnt_id),
            dev: (x.stx_dev_major, x.stx_dev_minor),
            path: path.to_path_buf(),
        })
    }

    /// Where the file open on `fd` sits, reached by `path`.
    pub fn of(fd: impl rustix::fd::AsFd, path: &Path) -> std::io::Result<Place> {
        use rustix::fs::{AtFlags, StatxFlags};
        let fd = fd.as_fd();
        let stat = rustix::fs::fstat(fd)?;
        let mount_id = match rustix::fs::statx(fd, "", AtFlags::EMPTY_PATH, StatxFlags::MNT_ID) {
            Ok(x) if x.stx_mask & StatxFlags::MNT_ID.bits() != 0 => Some(x.stx_mnt_id),
            _ => None,
        };
        Ok(Place {
            mount_id,
            dev: (
                rustix::fs::major(stat.st_dev),
                rustix::fs::minor(stat.st_dev),
            ),
            path: path.to_path_buf(),
        })
    }
}

/// Why a filesystem fails the rule that it stays mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsteady {
    /// A network filesystem, mounted at `point`.
    Network { point: PathBuf, fstype: String },
    /// A FUSE filesystem, which a program serves, mounted at `point`.
    Fuse { point: PathBuf, fstype: String },
    /// An automount point.
    Automount { point: PathBuf },
    /// A filesystem mounted below the automount point `automount`.
    BelowAutomount { point: PathBuf, automount: PathBuf },
    /// A filesystem on the removable disk `disk`, such as `/dev/sdb`,
    /// directly or beneath a device-mapper, md or partition device.
    Removable { point: PathBuf, disk: String },
    /// A filesystem on the disk `disk`, which sits on a USB bus and can be
    /// unplugged, whatever its `removable` flag says.
    UsbDisk { point: PathBuf, disk: String },
    /// A filesystem on a loop device whose backing file `file` fails the
    /// mount check for the reason `why`.
    Backing {
        point: PathBuf,
        file: PathBuf,
        why: Box<Unsteady>,
    },
    /// vtium cannot read from sysfs whether the device `source`, mounted
    /// at `point`, is on a removable disk, so it cannot tell whether the
    /// filesystem stays mounted.
    UnknownDisk { point: PathBuf, source: String },
    /// No line of the mount table holds the file, so vtium cannot tell
    /// what holds it.
    Unknown { path: PathBuf },
}

impl fmt::Display for Unsteady {
    /// What follows "because" in a refusal.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unsteady::Network { point, fstype } => write!(
                f,
                "it sits on {}, a network filesystem ({fstype}), which can change or go away while vtium runs",
                point.display()
            ),
            Unsteady::Fuse { point, fstype } => write!(
                f,
                "it sits on {}, a FUSE filesystem ({fstype}), which a program serves and can change while vtium runs",
                point.display()
            ),
            Unsteady::Automount { point } => write!(
                f,
                "it sits on {}, an automount point, which the system mounts only on demand",
                point.display()
            ),
            Unsteady::BelowAutomount { point, automount } => write!(
                f,
                "it sits on {}, which is mounted below the automount point {}, so the system mounts it only on demand",
                point.display(),
                automount.display()
            ),
            Unsteady::Removable { point, disk } => write!(
                f,
                "it sits on {}, which is on the removable disk {disk}",
                point.display()
            ),
            Unsteady::UsbDisk { point, disk } => write!(
                f,
                "it sits on {}, which is on {disk}, a disk on a USB bus that can be unplugged",
                point.display()
            ),
            Unsteady::Backing { point, file, why } => write!(
                f,
                "it sits on {}, a loop device that reads the file {}, and that file fails the same check: {why}",
                point.display(),
                file.display()
            ),
            Unsteady::UnknownDisk { point, source } => write!(
                f,
                "it sits on {}, and vtium cannot read from sysfs whether {source} is on a removable disk",
                point.display()
            ),
            Unsteady::Unknown { path } => write!(
                f,
                "vtium cannot find the filesystem that holds {} in the mount table",
                path.display()
            ),
        }
    }
}

/// The device number of the folder or file at `path`, without following a
/// symbolic link.
fn device_of(path: &Path) -> Option<(u32, u32)> {
    let dev = std::os::unix::fs::MetadataExt::dev(&std::fs::symlink_metadata(path).ok()?);
    Some((rustix::fs::major(dev), rustix::fs::minor(dev)))
}

/// Undoes the mount table's octal escapes, `\040` for a space, `\011` for
/// a tab, `\012` for a newline and `\134` for a backslash.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0_usize;
    while let Some(&b) = bytes.get(i) {
        let code = bytes
            .get(i.saturating_add(1)..i.saturating_add(4))
            .and_then(|d| {
                d.iter().try_fold(0_u8, |n, &c| {
                    (b'0'..=b'7')
                        .contains(&c)
                        .then(|| n.checked_mul(8)?.checked_add(c.wrapping_sub(b'0')))
                        .flatten()
                })
            });
        match code {
            Some(c) if b == b'\\' => {
                out.push(c);
                i = i.saturating_add(4);
            }
            _ => {
                out.push(b);
                i = i.saturating_add(1);
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads one line of the mount table: `None` for a line of another shape.
fn line(text: &str) -> Option<Mount> {
    let (head, tail) = text.split_once(" - ")?;
    let mut head = head.split(' ');
    let id = head.next()?.parse().ok()?;
    let parent = head.next()?.parse().ok()?;
    let (major, minor) = head.next()?.split_once(':')?;
    let dev = (major.parse().ok()?, minor.parse().ok()?);
    let _root = head.next()?;
    let point = PathBuf::from(unescape(head.next()?));
    let mut tail = tail.split(' ');
    let fstype = unescape(tail.next()?);
    let source = unescape(tail.next()?);
    Some(Mount {
        id,
        parent,
        dev,
        point,
        fstype,
        source,
    })
}

impl Mounts {
    /// Reads the mount table's text, skipping any line of another shape,
    /// with `sys` as the folder sysfs is mounted on.
    pub fn parse(mountinfo: &str, sys: &Path) -> Mounts {
        Mounts {
            list: mountinfo.lines().filter_map(line).collect(),
            sys: sys.to_path_buf(),
            source: None,
        }
    }

    /// Reads the mount table from the file `mountinfo`, with `sys` as the
    /// folder sysfs is mounted on. [`Mounts::judge`] reads the file again
    /// when it cannot find a file's mount.
    pub fn read(mountinfo: &Path, sys: &Path) -> std::io::Result<Mounts> {
        let text = std::fs::read_to_string(mountinfo)?;
        Ok(Mounts {
            source: Some(mountinfo.to_path_buf()),
            ..Mounts::parse(&text, sys)
        })
    }

    /// The running system's mount table, [`MOUNTINFO`], and sysfs at
    /// [`SYS`].
    pub fn system() -> std::io::Result<Mounts> {
        Mounts::read(Path::new(MOUNTINFO), Path::new(SYS))
    }

    /// The mounts, in the table's order.
    pub fn list(&self) -> &[Mount] {
        &self.list
    }

    /// The mount that holds the file at `place`, or `None` when this table
    /// has no line that is surely the file's own mount.
    ///
    /// When the kernel reports a mount ID, only the line with that ID
    /// counts, and only while its mount point lies above the path. A line
    /// whose mount point does not is a stale line whose ID the kernel has
    /// since given to another mount. Without a mount ID, the line with the
    /// file's device number and the longest mount point above the path
    /// counts. btrfs gives each subvolume a device number of its own, which
    /// the table does not hold, so the line with the longest mount point
    /// above the path then counts, but only when the mount point's own
    /// folder has the file's device number. Of two mounts on one point,
    /// the later one sits on top and wins.
    pub fn find(&self, place: &Place) -> Option<&Mount> {
        let above = |m: &&Mount| place.path.starts_with(&m.point);
        if let Some(id) = place.mount_id {
            return self.list.iter().filter(|m| m.id == id).find(above);
        }
        let deepest = |m: &&Mount| m.point.components().count();
        let by_dev = self
            .list
            .iter()
            .filter(|m| m.dev == place.dev)
            .filter(above)
            .max_by_key(deepest);
        if by_dev.is_some() {
            return by_dev;
        }
        self.list
            .iter()
            .filter(above)
            .max_by_key(deepest)
            .filter(|m| device_of(&m.point) == Some(place.dev))
    }

    /// Whether the filesystem holding the file at `place` stays mounted:
    /// it is neither a network filesystem, a FUSE filesystem, an
    /// automount point, a filesystem below one, nor a filesystem on a
    /// removable disk. The filesystem mounted on `/` is never refused as
    /// removable, because the root filesystem stays mounted for the
    /// system's life whatever disk holds it, as on a system that boots
    /// from a USB stick.
    ///
    /// When this table holds no line for the file's mount, vtium reads the
    /// table's file again once, since the mount may be newer than the
    /// table, and refuses the file if the mount is still missing.
    pub fn judge(&self, place: &Place) -> Result<(), Unsteady> {
        self.judge_at(place, 0)
    }

    /// [`Mounts::judge`] for a file `depth` steps down a stack of loop
    /// devices.
    fn judge_at(&self, place: &Place, depth: usize) -> Result<(), Unsteady> {
        let unknown = || Unsteady::Unknown {
            path: place.path.clone(),
        };
        let again;
        let (table, mount) = match self.find(place) {
            Some(mount) => (self, mount),
            None => {
                again = self.read_again().ok_or_else(unknown)?;
                let mount = again.find(place).ok_or_else(unknown)?;
                (&again, mount)
            }
        };
        table.verdict(mount, depth)
    }

    /// The mount table read again from its file, or `None` for a table
    /// made from text or a file vtium cannot read.
    fn read_again(&self) -> Option<Mounts> {
        Mounts::read(self.source.as_deref()?, &self.sys).ok()
    }

    /// Whether the filesystem of `mount`, a line of this table, stays
    /// mounted.
    fn verdict(&self, mount: &Mount, depth: usize) -> Result<(), Unsteady> {
        let point = || mount.point.clone();
        let fstype = mount.fstype.as_str();
        if NETWORK.contains(&fstype) || fstype.starts_with("nfs") {
            return Err(Unsteady::Network {
                point: point(),
                fstype: fstype.to_owned(),
            });
        }
        if fstype == "fuse" || fstype == "fuseblk" || fstype.starts_with("fuse.") {
            return Err(Unsteady::Fuse {
                point: point(),
                fstype: fstype.to_owned(),
            });
        }
        if fstype == "autofs" {
            return Err(Unsteady::Automount { point: point() });
        }
        if let Some(automount) = self.automount_above(mount) {
            return Err(Unsteady::BelowAutomount {
                point: point(),
                automount: automount.point.clone(),
            });
        }
        // The root filesystem stays mounted whatever disk holds it.
        if mount.point == Path::new("/") {
            return Ok(());
        }
        // A major number of 0 is an anonymous device. btrfs reports one
        // even on a disk, so vtium then looks for the disk by the device
        // the mount's source names. A filesystem whose source names no
        // device, such as tmpfs, is on no disk.
        let found = if mount.dev.0 != 0 {
            self.device(mount.dev, depth)
        } else if mount.source.starts_with("/dev/") {
            self.named(Path::new(&mount.source), depth)
        } else {
            Ok(())
        };
        found.map_err(|found| match found {
            Found::Removable(disk) => Unsteady::Removable {
                point: point(),
                disk,
            },
            Found::Usb(disk) => Unsteady::UsbDisk {
                point: point(),
                disk,
            },
            Found::Backing(file, why) => Unsteady::Backing {
                point: point(),
                file,
                why: Box::new(why),
            },
            Found::Unread => Unsteady::UnknownDisk {
                point: point(),
                source: mount.source.clone(),
            },
        })
    }

    /// The nearest automount point among the mounts that `mount` sits on,
    /// following the parent IDs at most once per line of the table, so a
    /// table whose parents loop ends.
    fn automount_above(&self, mount: &Mount) -> Option<&Mount> {
        let mut here = mount;
        for _ in 0..self.list.len() {
            let parent = self
                .list
                .iter()
                .find(|m| m.id == here.parent && m.id != here.id)?;
            if parent.fstype == "autofs" {
                return Some(parent);
            }
            here = parent;
        }
        None
    }

    /// Whether the block device `dev` is on a removable disk, read from
    /// its folder in sysfs.
    fn device(&self, (major, minor): (u32, u32), depth: usize) -> Result<(), Found> {
        let link = self.sys.join(format!("dev/block/{major}:{minor}"));
        let dir = std::fs::canonicalize(link).map_err(|_| Found::Unread)?;
        self.device_at(&dir, depth)
    }

    /// Whether the block device at `source`, a path in `/dev`, is on a
    /// removable disk. sysfs knows the device by the name of the node that
    /// `source` leads to, such as `dm-0` for `/dev/mapper/home`. When vtium
    /// cannot resolve `source`, as inside a sandbox without `/dev`, it uses
    /// the name `source` gives.
    fn named(&self, source: &Path, depth: usize) -> Result<(), Found> {
        let node = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        let name = node.file_name().ok_or(Found::Unread)?;
        let dir = std::fs::canonicalize(self.sys.join("class/block").join(name))
            .map_err(|_| Found::Unread)?;
        self.device_at(&dir, depth)
    }

    /// Whether the block device whose sysfs folder is `dir` is on a
    /// removable disk. A partition is judged by its disk. A loop device is
    /// judged by the filesystem that holds its backing file, which the
    /// whole mount check judges in turn. A device-mapper or md device is
    /// judged by every device beneath it, which sysfs lists in its
    /// `slaves` folder, down to the disks. A disk is removable when its
    /// `removable` flag is 1, or when it sits on a USB bus.
    fn device_at(&self, dir: &Path, depth: usize) -> Result<(), Found> {
        if depth > STACK_MAX {
            return Err(Found::Unread);
        }
        let below = depth.saturating_add(1);
        let dir = if dir.join("partition").exists() {
            dir.parent().ok_or(Found::Unread)?
        } else {
            dir
        };
        match std::fs::read(dir.join("loop/backing_file")) {
            Ok(bytes) => return self.backing(&bytes, below),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(Found::Unread),
            Err(_) => {}
        }
        let slaves = match std::fs::read_dir(dir.join("slaves")) {
            Ok(entries) => entries
                .map(|e| e.and_then(|e| std::fs::canonicalize(e.path())))
                .collect::<std::io::Result<Vec<PathBuf>>>()
                .map_err(|_| Found::Unread)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err(Found::Unread),
        };
        if !slaves.is_empty() {
            return slaves.iter().try_for_each(|s| self.device_at(s, below));
        }
        let flag = std::fs::read_to_string(dir.join("removable")).map_err(|_| Found::Unread)?;
        let name = dir
            .file_name()
            .ok_or(Found::Unread)?
            .to_string_lossy()
            .replace('!', "/");
        let disk = format!("/dev/{name}");
        if flag.trim() == "1" {
            return Err(Found::Removable(disk));
        }
        if USB_IS_REMOVABLE && self.on_usb(dir) {
            return Err(Found::Usb(disk));
        }
        Ok(())
    }

    /// Judges the backing file of a loop device, whose path the kernel
    /// gives as `bytes` followed by a newline, `depth` steps down.
    fn backing(&self, bytes: &[u8], depth: usize) -> Result<(), Found> {
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let file = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
        let place = Place::at(&file).map_err(|_| Found::Unread)?;
        self.judge_at(&place, depth)
            .map_err(|why| Found::Backing(file, why))
    }

    /// Whether the device folder `dir` sits below a USB host controller in
    /// sysfs's device tree, whose folders are named `usb1`, `usb2` and on.
    fn on_usb(&self, dir: &Path) -> bool {
        let sys = std::fs::canonicalize(&self.sys).unwrap_or_else(|_| self.sys.clone());
        dir.strip_prefix(&sys).unwrap_or(dir).components().any(|c| {
            c.as_os_str()
                .to_str()
                .and_then(|n| n.strip_prefix("usb"))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
    }
}

/// What the walk down a block device's stack found.
enum Found {
    /// A disk whose `removable` flag is 1, by its name in `/dev`.
    Removable(String),
    /// A disk on a USB bus, by its name in `/dev`.
    Usb(String),
    /// A loop device's backing file, which fails the mount check.
    Backing(PathBuf, Unsteady),
    /// vtium cannot read the device's folder in sysfs, or the stack is
    /// deeper than [`STACK_MAX`].
    Unread,
}
