//! The seat's devices through udev: DRM cards, framebuffers and input
//! devices, found at start and followed as they come and go
//! (monitor.c).
//!
//! [`Udev`] enumerates the `drm`, `graphics` and `input` subsystems once
//! ([`Udev::scan`]) and then watches udev's monitor socket. Each device is
//! kept when its `ID_SEAT` (an input device's from its parent input device)
//! is the seat's, `seat0` when unset, and reported as a [`DeviceEvent`] the
//! owner drains once a turn: a card or framebuffer goes to the displays, an
//! event node to [`crate::input::Input`]. A DRM `change` carrying
//! `HOTPLUG=1` asks the card to look at its connectors again
//! ([`crate::display::Drm2d::poll`]). A device whose `ID_SEAT` changes is
//! removed and added again.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction};
use rustix::fs::OFlags;

use crate::ioctl;

/// What kind of device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Drm,
    Fbdev,
    Input,
}

/// What the seat needs to know of a card or framebuffer
/// (monitor.c:166-188).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceFlags {
    /// The GPU the system booted on: a PCI card with `boot_vga`, or a
    /// platform display controller that is not on USB.
    pub primary: bool,
    /// A hotpluggable helper with display controllers, as a USB display.
    pub aux: bool,
    /// A framebuffer a DRM driver backs, which the seat skips in favour of
    /// the card; also when that cannot be ruled out.
    pub drm_backed: bool,
}

/// A device of the seat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub kind: DeviceKind,
    pub node: PathBuf,
    pub flags: DeviceFlags,
    /// udev's `ID_PATH` for a card, which `mode=` lines may name.
    pub id_path: Option<String>,
}

/// What udev reported about the seat's devices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    Added(Device),
    Removed {
        kind: DeviceKind,
        node: PathBuf,
    },
    /// A card's connectors changed.
    Hotplug {
        kind: DeviceKind,
        node: PathBuf,
    },
}

/// What a udev event did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Add,
    Remove,
    Change,
    Other,
}

/// One device as udev describes it: what the monitor reads of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Uevent {
    pub(crate) subsystem: Option<String>,
    pub(crate) sysname: String,
    pub(crate) syspath: String,
    pub(crate) devnode: Option<PathBuf>,
    /// `ID_SEAT`; for an input device, its parent input device's.
    pub(crate) seat: Option<String>,
    pub(crate) id_path: Option<String>,
    /// `HOTPLUG=1`.
    pub(crate) hotplug: bool,
    /// The PCI parent's `boot_vga`, when it has a PCI parent.
    pub(crate) pci_boot_vga: Option<bool>,
    pub(crate) usb_parent: bool,
    pub(crate) platform_parent: bool,
    /// An input device has a parent in the input subsystem.
    pub(crate) input_parent: bool,
}

/// What the monitor reads from a card or framebuffer it opens, so the
/// tests can answer for one.
pub(crate) trait Probe {
    /// The DRM driver's name.
    fn drm_driver(&mut self, node: &Path) -> io::Result<String>;
    /// The framebuffer's id, `fb_fix_screeninfo.id`.
    fn fb_id(&mut self, node: &Path) -> io::Result<String>;
}

/// The seat's devices, as the events so far leave them.
#[derive(Debug)]
pub(crate) struct Monitor {
    seat: String,
    known: BTreeMap<PathBuf, DeviceKind>,
    events: Vec<DeviceEvent>,
}

impl Monitor {
    pub(crate) fn new(seat: &str) -> Monitor {
        Monitor {
            seat: seat.to_owned(),
            known: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    pub(crate) fn events(&mut self) -> std::vec::Drain<'_, DeviceEvent> {
        self.events.drain(..)
    }

    /// One event from the monitor socket (monitor.c:470-499).
    pub(crate) fn handle(&mut self, action: Action, dev: &Uevent, probe: &mut dyn Probe) {
        match action {
            Action::Add => self.add(dev, probe),
            Action::Remove => self.remove(dev),
            Action::Change => self.change(dev, probe),
            Action::Other => {}
        }
    }

    /// A device found by the scan or added (monitor.c:352-428).
    pub(crate) fn add(&mut self, dev: &Uevent, probe: &mut dyn Probe) {
        let Some(node) = dev.devnode.as_ref() else {
            return;
        };
        if self.known.contains_key(node) {
            log::debug!(target: "monitor", "not adding {} again", dev.syspath);
            return;
        }
        let kind = match dev.subsystem.as_deref() {
            Some("drm") if numbered(&dev.sysname, "card") => DeviceKind::Drm,
            Some("graphics") if numbered(&dev.sysname, "fb") => DeviceKind::Fbdev,
            Some("input") if dev.sysname.starts_with("event") && dev.input_parent => {
                DeviceKind::Input
            }
            Some(other) => {
                log::debug!(target: "monitor", "not adding {} ({other})", dev.syspath);
                return;
            }
            None => {
                log::debug!(target: "monitor", "not adding {}: no subsystem", dev.syspath);
                return;
            }
        };
        if dev.seat.as_deref().unwrap_or("seat0") != self.seat {
            log::debug!(target: "monitor", "not adding {}: another seat's", dev.syspath);
            return;
        }
        let flags = match kind {
            DeviceKind::Drm => drm_flags(dev, node, probe),
            DeviceKind::Fbdev => fb_flags(node, probe),
            DeviceKind::Input => DeviceFlags::default(),
        };
        self.known.insert(node.clone(), kind);
        self.events.push(DeviceEvent::Added(Device {
            kind,
            node: node.clone(),
            flags,
            id_path: (kind == DeviceKind::Drm)
                .then(|| dev.id_path.clone())
                .flatten(),
        }));
    }

    fn remove(&mut self, dev: &Uevent) {
        let Some(node) = dev.devnode.as_ref() else {
            return;
        };
        match self.known.remove(node) {
            Some(kind) => {
                log::debug!(target: "monitor", "removing {} from seat {}", node.display(), self.seat);
                self.events.push(DeviceEvent::Removed {
                    kind,
                    node: node.clone(),
                });
            }
            None => log::debug!(target: "monitor", "removing an unknown device"),
        }
    }

    /// A known device that moved to another seat is removed and added
    /// again; an unknown one may have moved to this seat; a known card
    /// reports a hotplug (monitor.c:442-468).
    fn change(&mut self, dev: &Uevent, probe: &mut dyn Probe) {
        let Some(node) = dev.devnode.as_ref() else {
            return;
        };
        let Some(&kind) = self.known.get(node) else {
            self.add(dev, probe);
            return;
        };
        if dev.seat.as_deref().unwrap_or("seat0") != self.seat {
            self.remove(dev);
            self.add(dev, probe);
            return;
        }
        if dev.hotplug {
            self.events.push(DeviceEvent::Hotplug {
                kind,
                node: node.clone(),
            });
        }
    }
}

/// Whether `name` is `prefix` and a number, as `card0` or `fb1`.
fn numbered(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A card is primary when its PCI parent was the boot VGA device, or when
/// it is a platform display controller not on USB; it is auxiliary when
/// its driver is `udl` (monitor.c:232-350).
fn drm_flags(dev: &Uevent, node: &Path, probe: &mut dyn Probe) -> DeviceFlags {
    let primary = match dev.pci_boot_vga {
        Some(boot_vga) => boot_vga,
        None => !dev.usb_parent && dev.platform_parent,
    };
    let aux = match probe.drm_driver(node) {
        Ok(driver) => {
            log::debug!(target: "monitor", "DRM device {} uses driver {driver}", node.display());
            driver == "udl"
        }
        Err(e) => {
            log::warn!(target: "monitor", "cannot get the driver of {}: {e}", node.display());
            false
        }
    };
    DeviceFlags {
        primary,
        aux,
        drm_backed: false,
    }
}

/// A framebuffer is DRM-backed unless its id says otherwise: a DRM
/// driver's ends in `drmfb`, or is `nouveaufb` or `psbfb`
/// (monitor.c:190-230).
fn fb_flags(node: &Path, probe: &mut dyn Probe) -> DeviceFlags {
    let mut flags = DeviceFlags {
        drm_backed: true,
        ..DeviceFlags::default()
    };
    match probe.fb_id(node) {
        Ok(id) => {
            if !id.ends_with("drmfb") && id != "nouveaufb" && id != "psbfb" {
                flags.drm_backed = false;
            }
            flags.aux = id == "udlfb";
            flags.primary = id == "VESA VGA";
        }
        Err(e) => {
            log::warn!(target: "monitor", "cannot read the id of {}: {e}", node.display());
        }
    }
    flags
}

/// Opens each card and framebuffer to read its driver or id, read-write
/// as the C reference opened them.
struct Nodes;

impl Probe for Nodes {
    fn drm_driver(&mut self, node: &Path) -> io::Result<String> {
        let fd = open(node)?;
        let mut name = Vec::new();
        drm_ffi::get_version(fd.as_fd(), Some(&mut name), None, None)?;
        let bytes: Vec<u8> = name.iter().map(|&c| c.to_ne_bytes()[0]).collect();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn fb_id(&mut self, node: &Path) -> io::Result<String> {
        let fd = open(node)?;
        let fix = ioctl::fb_get_fix(fd.as_fd())?;
        let end = fix.id.iter().position(|&b| b == 0).unwrap_or(fix.id.len());
        Ok(String::from_utf8_lossy(fix.id.get(..end).unwrap_or_default()).into_owned())
    }
}

fn open(node: &Path) -> io::Result<OwnedFd> {
    Ok(rustix::fs::open(
        node,
        OFlags::RDWR | OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?)
}

/// udev's view of the seat: the scan and the monitor socket.
pub struct Udev {
    socket: udev::MonitorSocket,
    monitor: Monitor,
}

impl std::fmt::Debug for Udev {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Udev")
            .field("monitor", &self.monitor)
            .finish_non_exhaustive()
    }
}

impl Udev {
    /// Listens to udev for the `drm` (`drm_minor`), `graphics` and `input`
    /// subsystems on behalf of `seat` (monitor.c:502-603). Nothing is
    /// reported until [`Udev::scan`].
    pub fn new(seat: &str) -> io::Result<Udev> {
        let socket = udev::MonitorBuilder::new()?
            .match_subsystem_devtype("drm", "drm_minor")?
            .match_subsystem("graphics")?
            .match_subsystem("input")?
            .listen()?;
        let flags = rustix::fs::fcntl_getfl(&socket)?;
        rustix::fs::fcntl_setfl(&socket, flags | OFlags::NONBLOCK)?;
        Ok(Udev {
            socket,
            monitor: Monitor::new(seat),
        })
    }

    /// Reports every device of the seat present now (monitor.c:638-703).
    pub fn scan(&mut self) -> io::Result<()> {
        self.scan_with(&mut Nodes)
    }

    pub(crate) fn scan_with(&mut self, probe: &mut dyn Probe) -> io::Result<()> {
        let mut enumerator = udev::Enumerator::new()?;
        for subsystem in ["drm", "graphics", "input"] {
            enumerator.match_subsystem(subsystem)?;
        }
        for dev in enumerator.scan_devices()? {
            self.monitor.add(&uevent(&dev), probe);
        }
        Ok(())
    }

    /// What changed since the last call.
    pub fn events(&mut self) -> std::vec::Drain<'_, DeviceEvent> {
        self.monitor.events()
    }

    /// Reads every event waiting on the monitor socket.
    pub fn receive(&mut self) {
        for event in self.socket.iter() {
            let action = match event.action().and_then(OsStr::to_str) {
                Some("add") => Action::Add,
                Some("remove") => Action::Remove,
                Some("change") => Action::Change,
                _ => Action::Other,
            };
            self.monitor.handle(action, &uevent(&event), &mut Nodes);
        }
    }

    /// Puts the monitor socket in the loop behind `handle`. `access` finds
    /// it in the loop's data, or `None` once it is gone.
    pub fn insert<D, A>(&self, handle: &LoopHandle<'static, D>, access: A) -> io::Result<()>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Udev> + 'static,
    {
        let fd = self.socket.as_fd().try_clone_to_owned()?;
        handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |readiness, _, data: &mut D| {
                    let Some(udev) = access(data) else {
                        return Ok(PostAction::Remove);
                    };
                    if readiness.error {
                        log::warn!(target: "monitor", "the udev monitor closed");
                        return Ok(PostAction::Remove);
                    }
                    udev.receive();
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| io::Error::from(e.error))?;
        Ok(())
    }
}

/// What the monitor reads of a udev device.
fn uevent(dev: &udev::Device) -> Uevent {
    let text = |s: Option<&OsStr>| s.map(|s| s.to_string_lossy().into_owned());
    let input_parent = dev.parent_with_subsystem("input").ok().flatten();
    let seat = if dev.subsystem() == Some(OsStr::new("input")) {
        input_parent
            .as_ref()
            .and_then(|p| text(p.property_value("ID_SEAT")))
    } else {
        text(dev.property_value("ID_SEAT"))
    };
    let parent = |subsystem: &str| dev.parent_with_subsystem(subsystem).ok().flatten();
    let pci = parent("pci");
    Uevent {
        subsystem: text(dev.subsystem()),
        sysname: dev.sysname().to_string_lossy().into_owned(),
        syspath: dev.syspath().to_string_lossy().into_owned(),
        devnode: dev.devnode().map(Path::to_owned),
        seat,
        id_path: text(dev.property_value("ID_PATH")),
        hotplug: dev.property_value("HOTPLUG") == Some(OsStr::new("1")),
        pci_boot_vga: pci
            .as_ref()
            .map(|p| p.attribute_value("boot_vga") == Some(OsStr::new("1"))),
        usb_parent: parent("usb").is_some(),
        platform_parent: parent("platform").is_some(),
        input_parent: input_parent.is_some(),
    }
}
