//! The udev side of the privilege split: the drawing process's probe of
//! each card and framebuffer through sysfs, and the root process's checks
//! of each device node it opens for the drawing process. Only a build
//! with the `split` feature carries them.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker opens only its seat's
//!   cards, framebuffers and event devices.

use std::ffi::OsStr;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::OFlags;
use rustix::io::Errno;

use super::{DeviceKind, Probe, UDEV_CONTROL, numbered};

/// Reads each card's driver and each framebuffer's id from sysfs, under
/// the node's device number in `/sys/dev/char`, and opens no node. Every
/// user may read these files and `stat` a node.
#[derive(Debug)]
pub(crate) struct Sysfs {
    /// sysfs's `/sys/dev/char`, or a test's stand-in.
    pub(crate) root: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Sysfs {
        Sysfs {
            root: PathBuf::from("/sys/dev/char"),
        }
    }
}

impl Sysfs {
    /// The node's directory in sysfs, `MAJOR:MINOR` under [`Sysfs::root`].
    fn dir(&self, node: &Path) -> io::Result<PathBuf> {
        let rdev = rustix::fs::stat(node)?.st_rdev;
        let (major, minor) = (rustix::fs::major(rdev), rustix::fs::minor(rdev));
        Ok(self.root.join(format!("{major}:{minor}")))
    }
}

impl Probe for Sysfs {
    /// The name of the driver the card's device is bound to: the last
    /// part of the link `device/driver`.
    fn drm_driver(&mut self, node: &Path) -> io::Result<String> {
        let link = std::fs::read_link(self.dir(node)?.join("device/driver"))?;
        link.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
    }

    /// The framebuffer's `name`, which the kernel fills from
    /// `fb_fix_screeninfo.id` (`name_show` in
    /// `drivers/video/fbdev/core/fbsysfs.c`).
    fn fb_id(&mut self, node: &Path) -> io::Result<String> {
        let name = std::fs::read_to_string(self.dir(node)?.join("name"))?;
        Ok(name.trim_end_matches('\n').to_owned())
    }
}

/// The major device number of DRM's nodes, `DRM_MAJOR` in the kernel's
/// `include/uapi/linux/major.h`.
pub const DRM_MAJOR: u32 = 226;
/// The major device number of framebuffers, `FB_MAJOR` in
/// `include/uapi/linux/major.h`.
pub const FB_MAJOR: u32 = 29;
/// The major device number of input devices, `INPUT_MAJOR` in
/// `include/uapi/linux/major.h`.
pub const INPUT_MAJOR: u32 = 13;

/// What the broker reads of a device node before it opens the node for
/// the drawer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeFacts {
    /// The node is a character device, and not a link to one.
    pub(crate) char_device: bool,
    /// The node's major device number.
    pub(crate) major: u32,
    /// udev's subsystem for the node's device number.
    pub(crate) subsystem: Option<String>,
    /// The kernel's name for the device, such as `card0` or `event3`.
    pub(crate) sysname: String,
    /// `ID_SEAT`; for an event device, its parent input device's.
    pub(crate) seat: Option<String>,
    /// Whether udev tagged the device for a seat: the tag `seat` on the
    /// device, or on an event device's parent input device. `None` where
    /// no udev daemon runs, so no device carries tags.
    pub(crate) tagged: Option<bool>,
}

/// Why the broker refuses to open a node for the drawer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeRefusal {
    /// The node is not a character device, or it is a symbolic link.
    NotADevice,
    /// The node's major number is none of DRM's, fbdev's and input's.
    Major(u32),
    /// The node is of the right major number but not a card, a
    /// framebuffer or an event device, such as a render node or a mouse
    /// device.
    Kind(String),
    /// The device belongs to another seat.
    Seat(String),
    /// udev runs and has not tagged the device for a seat.
    Untagged,
    /// The node changed between its check and its open.
    Moved,
}

impl std::fmt::Display for NodeRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeRefusal::NotADevice => write!(f, "it is not a character device"),
            NodeRefusal::Major(major) => write!(
                f,
                "its major number {major} is not a DRM card's, a framebuffer's or an input device's"
            ),
            NodeRefusal::Kind(name) => {
                write!(
                    f,
                    "{name} is not a DRM card, a framebuffer or an event device"
                )
            }
            NodeRefusal::Seat(seat) => write!(f, "it belongs to seat {seat}"),
            NodeRefusal::Untagged => write!(f, "udev has not tagged it as a seat's device"),
            NodeRefusal::Moved => write!(f, "it changed while vtium opened it"),
        }
    }
}

/// Decides whether the broker opens a node for the drawer, from what it
/// read of the node: a character device of DRM's, fbdev's or input's
/// major number, which udev knows as a card, a framebuffer or an event
/// device, of the broker's `seat`, and tagged `seat` where udev runs.
/// Returns the device's kind.
///
/// References:
/// - ADR-1400 D4, "What the broker does": only a device node of those
///   kinds that belongs to its seat, whatever path the drawer names.
pub(crate) fn admit(node: &NodeFacts, seat: &str) -> Result<DeviceKind, NodeRefusal> {
    if !node.char_device {
        return Err(NodeRefusal::NotADevice);
    }
    let kind = match (node.major, node.subsystem.as_deref()) {
        (DRM_MAJOR, Some("drm")) if numbered(&node.sysname, "card") => DeviceKind::Drm,
        (FB_MAJOR, Some("graphics")) if numbered(&node.sysname, "fb") => DeviceKind::Fbdev,
        (INPUT_MAJOR, Some("input")) if numbered(&node.sysname, "event") => DeviceKind::Input,
        (DRM_MAJOR | FB_MAJOR | INPUT_MAJOR, _) => {
            return Err(NodeRefusal::Kind(node.sysname.clone()));
        }
        (major, _) => return Err(NodeRefusal::Major(major)),
    };
    let theirs = node.seat.as_deref().unwrap_or("seat0");
    if theirs != seat {
        return Err(NodeRefusal::Seat(theirs.to_owned()));
    }
    if node.tagged == Some(false) {
        return Err(NodeRefusal::Untagged);
    }
    Ok(kind)
}

/// Whether udev's `TAGS` property names the tag `seat`.
fn seat_tagged(dev: &udev::Device) -> bool {
    dev.property_value("TAGS")
        .is_some_and(|tags| tags.to_string_lossy().split(':').any(|t| t == "seat"))
}

/// What the broker reads of the node at `path`, without following a
/// symbolic link, and its device number.
fn node_facts(path: &Path) -> io::Result<(NodeFacts, u64)> {
    let st = rustix::fs::lstat(path)?;
    let char_device =
        rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::CharacterDevice;
    let mut facts = NodeFacts {
        char_device,
        major: rustix::fs::major(st.st_rdev),
        ..NodeFacts::default()
    };
    if !char_device {
        return Ok((facts, st.st_rdev));
    }
    let text = |s: Option<&OsStr>| s.map(|s| s.to_string_lossy().into_owned());
    let dev = udev::Device::from_devnum(udev::DeviceType::Character, st.st_rdev)?;
    let input_parent = dev.parent_with_subsystem("input").ok().flatten();
    let event = dev.subsystem() == Some(OsStr::new("input"));
    facts.subsystem = text(dev.subsystem());
    facts.sysname = dev.sysname().to_string_lossy().into_owned();
    facts.seat = if event {
        input_parent
            .as_ref()
            .and_then(|p| text(p.property_value("ID_SEAT")))
    } else {
        text(dev.property_value("ID_SEAT"))
    };
    facts.tagged = Path::new(UDEV_CONTROL)
        .exists()
        .then(|| seat_tagged(&dev) || (event && input_parent.as_ref().is_some_and(seat_tagged)));
    Ok((facts, st.st_rdev))
}

/// Opens the node at `path` for the drawer, read-write, non-blocking and
/// close-on-exec, once the checks of `admit` allow it for `seat`. The broker reads
/// the node without following a link before it opens it, so a refused
/// node is never opened, and it checks after the open that the open file
/// is the device it read. A refusal is logged with its reason and answers
/// `EACCES`.
pub fn open_seat_node(path: &Path, seat: &str) -> io::Result<(DeviceKind, OwnedFd)> {
    let refuse = |why: NodeRefusal| {
        log::warn!(target: "broker", "vtium does not open {} for its drawing process, because {why}", path.display());
        io::Error::from(Errno::ACCESS)
    };
    let (facts, rdev) = node_facts(path)?;
    let kind = admit(&facts, seat).map_err(refuse)?;
    let fd = rustix::fs::open(
        path,
        OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?;
    let opened = rustix::fs::fstat(&fd)?;
    let same = rustix::fs::FileType::from_raw_mode(opened.st_mode)
        == rustix::fs::FileType::CharacterDevice
        && opened.st_rdev == rdev;
    if !same {
        return Err(refuse(NodeRefusal::Moved));
    }
    Ok((kind, fd))
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use crate::udev::fb_id_flags;

    /// The drawer's probe reads a card's driver from the link
    /// `device/driver` and a framebuffer's id from `name`, under the node's
    /// device number in sysfs, and opens no node: `/dev/null`, whose
    /// number the stand-in sysfs is laid out for, is never opened.
    #[test]
    fn the_sysfs_probe_reads_the_driver_and_the_id_without_opening_the_node() {
        let root = Scratch::new("sysfs-probe");
        let st = rustix::fs::stat("/dev/null").unwrap_or_else(|e| panic!("stat: {e}"));
        let dir = root.join(format!(
            "{}:{}",
            rustix::fs::major(st.st_rdev),
            rustix::fs::minor(st.st_rdev)
        ));
        std::fs::create_dir_all(dir.join("device"))
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        std::os::unix::fs::symlink("../../../bus/pci/drivers/i915", dir.join("device/driver"))
            .unwrap_or_else(|e| panic!("symlink: {e}"));
        std::fs::write(dir.join("name"), "inteldrmfb\n").unwrap_or_else(|e| panic!("name: {e}"));
        let mut probe = Sysfs {
            root: root.to_path_buf(),
        };
        let node = Path::new("/dev/null");
        assert_eq!(probe.drm_driver(node).ok().as_deref(), Some("i915"));
        assert_eq!(probe.fb_id(node).ok().as_deref(), Some("inteldrmfb"));
        assert!(fb_id_flags(probe.fb_id(node).ok().as_deref()).drm_backed);
        // A node with no entry in sysfs has no driver.
        let missing = Sysfs {
            root: root.join("none"),
        };
        assert!(Sysfs::drm_driver(&mut { missing }, node).is_err());
    }
}
