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
//! `removable` flag is 1 or when it sits on a USB bus, unless it is a disk
//! beneath the root filesystem. When vtium cannot
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
use std::os::unix::ffi::{OsStrExt, OsStringExt};
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
///
/// References:
/// - ADR-1205 D4, the note of 2026-11-19: the maintainer's decision that a USB disk counts as removable unless it holds the root filesystem.
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
    /// What follows "because" in a refusal. Every reason but `Unknown`
    /// opens with the mount point, which one `write!` prints for all of
    /// them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let point = match self {
            Unsteady::Unknown { path } => {
                return write!(
                    f,
                    "vtium cannot find the filesystem that holds {} in the mount table",
                    path.display()
                );
            }
            Unsteady::Network { point, .. }
            | Unsteady::Fuse { point, .. }
            | Unsteady::Automount { point }
            | Unsteady::BelowAutomount { point, .. }
            | Unsteady::Removable { point, .. }
            | Unsteady::UsbDisk { point, .. }
            | Unsteady::Backing { point, .. }
            | Unsteady::UnknownDisk { point, .. } => point,
        };
        write!(f, "it sits on {}", point.display())?;
        match self {
            Unsteady::Network { fstype, .. } => write!(
                f,
                ", a network filesystem ({fstype}), which can change or go away while vtium runs"
            ),
            Unsteady::Fuse { fstype, .. } => write!(
                f,
                ", a FUSE filesystem ({fstype}), which a program serves and can change while vtium runs"
            ),
            Unsteady::Automount { .. } => {
                f.write_str(", an automount point, which the system mounts only on demand")
            }
            Unsteady::BelowAutomount { automount, .. } => write!(
                f,
                ", which is mounted below the automount point {}, so the system mounts it only on demand",
                automount.display()
            ),
            Unsteady::Removable { disk, .. } => {
                write!(f, ", which is on the removable disk {disk}")
            }
            Unsteady::UsbDisk { disk, .. } => write!(
                f,
                ", which is on {disk}, a disk on a USB bus that can be unplugged"
            ),
            Unsteady::Backing { file, why, .. } => write!(
                f,
                ", a loop device that reads the file {}, and that file fails the same check: {why}",
                file.display()
            ),
            Unsteady::UnknownDisk { source, .. } => write!(
                f,
                ", and vtium cannot read from sysfs whether {source} is on a removable disk"
            ),
            Unsteady::Unknown { .. } => Ok(()),
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
/// a tab, `\012` for a newline and `\134` for a backslash. The kernel
/// passes every other byte through as it is, so the result is bytes, which
/// need not be UTF-8.
fn unescape(bytes: &[u8]) -> Vec<u8> {
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
    out
}

/// A field of the mount table as text, for the fields vtium compares with
/// names it knows. A byte that is not UTF-8 becomes U+FFFD.
fn text(field: &[u8]) -> String {
    String::from_utf8_lossy(&unescape(field)).into_owned()
}

/// A number field of the mount table.
fn number<T: std::str::FromStr>(field: &[u8]) -> Option<T> {
    std::str::from_utf8(field).ok()?.parse().ok()
}

/// Reads one line of the mount table: `None` for a line of another shape.
/// The mount point stays bytes, since a folder's name need not be UTF-8.
fn line(bytes: &[u8]) -> Option<Mount> {
    let split = bytes.windows(3).position(|w| w == b" - ")?;
    let (head, tail) = (bytes.get(..split)?, bytes.get(split.checked_add(3)?..)?);
    let mut head = head.split(|&b| b == b' ');
    let id = number(head.next()?)?;
    let parent = number(head.next()?)?;
    let mut dev = head.next()?.splitn(2, |&b| b == b':');
    let dev = (number(dev.next()?)?, number(dev.next()?)?);
    let _root = head.next()?;
    let point = PathBuf::from(std::ffi::OsString::from_vec(unescape(head.next()?)));
    let mut tail = tail.split(|&b| b == b' ');
    let fstype = text(tail.next()?);
    let source = text(tail.next()?);
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
    /// Reads the mount table's bytes, skipping any line of another shape,
    /// with `sys` as the folder sysfs is mounted on.
    pub fn parse(mountinfo: impl AsRef<[u8]>, sys: &Path) -> Mounts {
        let mut list = Vec::new();
        for bytes in mountinfo.as_ref().split(|&b| b == b'\n') {
            if let Some(mount) = line(bytes) {
                list.push(mount);
            }
        }
        Mounts {
            list,
            sys: sys.to_path_buf(),
            source: None,
        }
    }

    /// Reads the mount table from the file `mountinfo`, with `sys` as the
    /// folder sysfs is mounted on. [`Mounts::judge`] reads the file again
    /// when it cannot find a file's mount.
    pub fn read(mountinfo: &Path, sys: &Path) -> std::io::Result<Mounts> {
        let bytes = std::fs::read(mountinfo)?;
        Ok(Mounts {
            source: Some(mountinfo.to_path_buf()),
            ..Mounts::parse(bytes, sys)
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
        if let Some(id) = place.mount_id {
            return self
                .list
                .iter()
                .find(|m| m.id == id && place.path.starts_with(&m.point));
        }
        self.deepest_above(place, true).or_else(|| {
            self.deepest_above(place, false)
                .filter(|m| device_of(&m.point) == Some(place.dev))
        })
    }

    /// The line with the longest mount point above the path of `place`,
    /// counting only lines with the file's device number when `same_dev`
    /// holds. Of two lines whose points are equally long, the later one
    /// wins. One loop serves both of `find`'s searches, which keeps the
    /// console's copy of this code small.
    fn deepest_above(&self, place: &Place, same_dev: bool) -> Option<&Mount> {
        let mut best: Option<(&Mount, usize)> = None;
        for m in &self.list {
            if (same_dev && m.dev != place.dev) || !place.path.starts_with(&m.point) {
                continue;
            }
            let depth = m.point.components().count();
            if best.is_none_or(|(_, deepest)| depth >= deepest) {
                best = Some((m, depth));
            }
        }
        best.map(|(m, _)| m)
    }

    /// Whether the filesystem holding the file at `place` stays mounted:
    /// it is neither a network filesystem, a FUSE filesystem, an
    /// automount point, a filesystem below one, nor a filesystem on a
    /// removable disk. The root filesystem stays mounted for the system's
    /// life whatever disk holds it, as on a system that boots from a USB
    /// stick. So the filesystem mounted on `/` is never refused as
    /// removable, and neither is a filesystem whose disks all lie beneath
    /// `/`, such as `/home` on another partition of the boot stick. When
    /// vtium cannot find the disks beneath `/`, only `/` is exempt.
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
        let found = match self.block_dir(mount) {
            Err(found) => Err(found),
            // a filesystem with no block device, such as tmpfs
            Ok(None) => Ok(()),
            // the other filesystems on the disk that holds the root
            // filesystem stay mounted with it, as on a USB boot stick
            Ok(Some(dir)) if self.on_root_disks(&dir) => Ok(()),
            Ok(Some(dir)) => self.device_at(&dir, depth),
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

    /// The sysfs folder of the block device that holds `mount`, or `None`
    /// for a filesystem on no block device, such as tmpfs. A major number
    /// of 0 is an anonymous device. btrfs reports one even on a disk, so
    /// vtium then finds the device by the `/dev` path in the mount's
    /// source. sysfs knows the device by the name of the node that the
    /// path leads to, such as `dm-0` for `/dev/mapper/home`. When vtium
    /// cannot resolve the path, as inside a sandbox without `/dev`, it uses
    /// the name the path gives.
    fn block_dir(&self, mount: &Mount) -> Result<Option<PathBuf>, Found> {
        let (major, minor) = mount.dev;
        let link = if major != 0 {
            self.sys.join(format!("dev/block/{major}:{minor}"))
        } else if mount.source.starts_with("/dev/") {
            let source = Path::new(&mount.source);
            let node = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
            let name = node.file_name().ok_or(Found::Unread)?;
            self.sys.join("class/block").join(name)
        } else {
            return Ok(None);
        };
        std::fs::canonicalize(link)
            .map(Some)
            .map_err(|_| Found::Unread)
    }

    /// Whether every disk beneath the device folder `dir` also lies beneath
    /// the filesystem mounted on `/`. `false` when vtium cannot tell, for
    /// either device, so then only `/` itself stays exempt.
    fn on_root_disks(&self, dir: &Path) -> bool {
        let Some(root) = self.list.iter().rev().find(|m| m.point == Path::new("/")) else {
            return false;
        };
        let Ok(Some(root_dir)) = self.block_dir(root) else {
            return false;
        };
        let (mut roots, mut mine) = (Vec::new(), Vec::new());
        if disks(&root_dir, 0, &mut roots).is_none() || disks(dir, 0, &mut mine).is_none() {
            return false;
        }
        !mine.is_empty() && mine.iter().all(|d| roots.contains(d))
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
        let dir = disk_of(dir).ok_or(Found::Unread)?;
        match std::fs::read(dir.join("loop/backing_file")) {
            Ok(bytes) => return self.backing(&bytes, below),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(Found::Unread),
            Err(_) => {}
        }
        let slaves = slaves(dir).ok_or(Found::Unread)?;
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

/// Adds to `out` the sysfs folders of the disks beneath the device folder
/// `dir`: the disk of a partition, and every disk beneath a device-mapper
/// or md device's `slaves`. `None` when vtium cannot read a folder, meets a
/// loop device, or follows the stack deeper than [`STACK_MAX`].
fn disks(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Option<()> {
    if depth > STACK_MAX {
        return None;
    }
    let dir = disk_of(dir)?;
    if dir.join("loop").exists() {
        return None;
    }
    let slaves = slaves(dir)?;
    if slaves.is_empty() {
        out.push(dir.to_path_buf());
        return Some(());
    }
    slaves
        .iter()
        .try_for_each(|s| disks(s, depth.saturating_add(1), out))
}

/// The sysfs folder that stands for the device folder `dir` in the walk
/// down a device stack: the disk's folder for a partition, which sysfs
/// places inside its disk's folder, and `dir` itself for any other device.
/// `None` when a partition's folder has no parent.
fn disk_of(dir: &Path) -> Option<&Path> {
    if dir.join("partition").exists() {
        dir.parent()
    } else {
        Some(dir)
    }
}

/// The sysfs folders of the devices beneath the device folder `dir`, which
/// sysfs lists as links in its `slaves` folder, with each link resolved.
/// The list is empty for a device with no `slaves` folder, such as a disk,
/// and `None` when vtium cannot read the folder or resolve a link. The walk
/// up from a filesystem and the walk that lists the root's disks share
/// this one reader, so the console carries one copy of it.
fn slaves(dir: &Path) -> Option<Vec<PathBuf>> {
    let list = match std::fs::read_dir(dir.join("slaves")) {
        Ok(list) => list,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let mut out = Vec::new();
    for entry in list {
        out.push(std::fs::canonicalize(entry.ok()?.path()).ok()?);
    }
    Some(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Of two mounts on one point, the later line of the table sits on top,
    // so the file is on that mount.
    #[test]
    fn the_later_of_two_mounts_on_one_point_wins() {
        let table = "\
22 1 8:2 / / rw - ext4 /dev/sda2 rw
40 22 8:17 / /mnt rw - ext4 /dev/sdb1 rw
41 22 8:17 / /mnt rw - ext4 /dev/sdb1 rw
";
        let mounts = Mounts::parse(table, Path::new("/nonexistent-sys"));
        let place = Place {
            mount_id: None,
            dev: (8, 17),
            path: "/mnt/a.ttf".into(),
        };
        assert_eq!(mounts.find(&place).map(|m| m.id), Some(41));
    }
}
