//! The broker's side: the VT, and the broker's own copy of each device it
//! opened for the drawer.
//!
//! The broker runs as root. It holds the VT through a [`KernelVt`], opens
//! each card, framebuffer and input device the drawer asks for once the
//! node passes the seat's checks ([`crate::udev::open_seat_node`]), and
//! keeps its own copy of each. DRM master and `EVIOCREVOKE` act on the open
//! file, not on one descriptor, so what the broker does to its copy
//! reaches the drawer's copy too.
//!
//! - **Master.** The broker sets master on its copy of a card when the
//!   drawer asks, only while the VT is the drawer's, and drops it when the
//!   drawer asks.
//! - **A release.** When the kernel asks for the VT, the broker sends
//!   [`Order::Release`] and waits for [`Request::Released`] until
//!   [`RELEASE_BOUND`] has passed. Then, answered or not, it drops master
//!   on each card, revokes each input device, and lets the VT go. A hung
//!   drawer cannot keep the VT.
//! - **An acquire.** The broker sets master on each card again, opens each
//!   input device anew, since a revoked descriptor never works again, and
//!   sends one [`Order::Device`] for each with the drawer's id, then
//!   [`Order::Back`].
//! - **The drawer's end.** [`Holder::give_back`] puts the VT back and lets
//!   the devices go.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the VT, the devices, master, the
//!   release and its bound, and the acquire.
//! - ADR-1400 D4, "When the drawer dies": the restore and its order.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use vtium_broker_protocol::{DevicePath, ERRNO_MAX, Errno, Order, Request};

use crate::event_loop::Loop;
use crate::udev::DeviceKind;
use crate::vt::kernel::Reach;
use crate::vt::{Console, KernelVt, Tty, Vt, VtEvent};

/// How long the broker waits for the drawer to let its devices go after it
/// tells the drawer the VT is asked for: [`crate::bounds::RELEASE_BOUND_MS`].
/// A drawer that has not answered by then loses the devices all the same.
pub const RELEASE_BOUND: Duration = Duration::from_millis(crate::bounds::RELEASE_BOUND_MS as u64);

/// The most devices the broker holds for the drawer at once: its cards,
/// its framebuffers and its input devices together. A machine has a few
/// cards and seldom more than a few dozen input devices. The cap keeps a
/// drawing process that has been taken over from making the broker hold
/// copies until the broker runs out of descriptors, which would stop it
/// from starting sessions and helpers.
pub const DEVICES_MAX: usize = 128;

/// The broker's calls on the device nodes, so the tests can record them.
pub(crate) trait Nodes {
    /// Opens the node at `path` for the drawer, once it passes the seat's
    /// checks; a node that fails them is refused and never opened.
    fn open(&mut self, path: &Path, seat: &str) -> io::Result<(DeviceKind, OwnedFd)>;
    /// `DRM_IOCTL_SET_MASTER` on the broker's copy of a card.
    fn set_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()>;
    /// `DRM_IOCTL_DROP_MASTER` on the broker's copy of a card.
    fn drop_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()>;
    /// `EVIOCREVOKE` on the broker's copy of an input device.
    fn revoke(&mut self, input: BorrowedFd<'_>) -> io::Result<()>;
}

/// The kernel's device nodes.
#[derive(Debug, Default)]
pub struct Kernel;

impl Nodes for Kernel {
    fn open(&mut self, path: &Path, seat: &str) -> io::Result<(DeviceKind, OwnedFd)> {
        crate::udev::open_seat_node(path, seat)
    }

    fn set_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()> {
        drm_ffi::auth::acquire_master(card)
    }

    fn drop_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()> {
        drm_ffi::auth::release_master(card)
    }

    fn revoke(&mut self, input: BorrowedFd<'_>) -> io::Result<()> {
        crate::ioctl::evdev_revoke(input)
    }
}

/// One device the broker opened for the drawer.
#[derive(Debug)]
struct Held {
    kind: DeviceKind,
    path: DevicePath,
    /// The broker's own copy. An input device's is `None` from the release
    /// that revoked it until the acquire that opens it anew.
    fd: Option<OwnedFd>,
}

/// What the broker sends the drawer: each order, with its descriptor when
/// it carries one.
pub type Orders = Vec<(Order, Option<OwnedFd>)>;

/// The VT and the devices the broker holds for the drawer. The broker's
/// loop shares it through [`Holder::start`].
#[expect(
    private_bounds,
    reason = "the console is a Tty and the nodes the kernel's outside the crate"
)]
pub struct Holder<C: Console = Tty, N: Nodes = Kernel> {
    vt: KernelVt<C>,
    nodes: N,
    seat: String,
    devices: BTreeMap<u32, Held>,
    /// The drawer was told to release the VT, and the time by which the
    /// broker lets the devices go whatever the drawer answers.
    releasing: Option<Instant>,
    bound: Duration,
    hung_up: bool,
}

impl<C: Console, N: Nodes> std::fmt::Debug for Holder<C, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Holder")
            .field("vt", &self.vt.number())
            .field("devices", &self.devices.keys().collect::<Vec<_>>())
            .field("releasing", &self.releasing)
            .finish_non_exhaustive()
    }
}

impl Holder<Tty, Kernel> {
    /// Takes VT `num` through `tty` in the broker's loop, as
    /// [`KernelVt::new`] does, for the drawer on `seat`. Call it before any
    /// thread starts and before the drawer starts. The loop's signal and
    /// tty handlers reach the VT through the returned holder.
    pub fn start<D: 'static>(
        lp: &mut Loop<D>,
        tty: Tty,
        num: u16,
        seat: &str,
    ) -> io::Result<Rc<RefCell<Holder>>> {
        let slot: Rc<RefCell<Weak<RefCell<Holder>>>> = Rc::default();
        let found = Rc::clone(&slot);
        let reach = move |_: &mut D, f: Reach<'_, Tty>| {
            let Some(holder) = found.borrow().upgrade() else {
                return;
            };
            match holder.try_borrow_mut() {
                Ok(mut holder) => f(&mut holder.vt),
                Err(_) => {
                    log::warn!(target: "broker", "the VT is busy, so vtium misses one of its events")
                }
            }
        };
        let vt = KernelVt::start_with(lp, tty, num, reach)?;
        let holder = Rc::new(RefCell::new(Holder::with(vt, Kernel, seat)));
        *slot.borrow_mut() = Rc::downgrade(&holder);
        Ok(holder)
    }
}

#[expect(
    private_bounds,
    reason = "the console is a Tty and the nodes the kernel's outside the crate"
)]
impl<C: Console, N: Nodes> Holder<C, N> {
    /// The holder of `vt`, opening devices through `nodes` for `seat`.
    pub(crate) fn with(vt: KernelVt<C>, nodes: N, seat: &str) -> Holder<C, N> {
        Holder {
            vt,
            nodes,
            seat: seat.to_owned(),
            devices: BTreeMap::new(),
            releasing: None,
            bound: RELEASE_BOUND,
            hung_up: false,
        }
    }

    /// The broker's VT, for the tests and the loop's signals.
    #[cfg(test)]
    pub(crate) fn vt(&mut self) -> &mut KernelVt<C> {
        &mut self.vt
    }

    /// Whether `request` is one the holder serves: the devices, master,
    /// the release, the switches and the VT's modes.
    pub fn serves(request: &Request) -> bool {
        matches!(
            request,
            Request::OpenDevice { .. }
                | Request::CloseDevice { .. }
                | Request::TakeMaster { .. }
                | Request::DropMaster { .. }
                | Request::Released
                | Request::SwitchVt { .. }
                | Request::RestoreVt
        )
    }

    /// Whether the VT is the drawer's to use: ours, with no release
    /// waiting.
    fn drawers(&self) -> bool {
        self.vt.is_active() && self.releasing.is_none()
    }

    /// Serves one of the drawer's requests that [`Holder::serves`] names
    /// at `now`, and returns the answers and the orders that follow.
    pub fn serve(&mut self, request: Request, now: Instant) -> Orders {
        let mut out = Orders::new();
        match request {
            Request::OpenDevice { device, path } => {
                let opened = self.open(device, path);
                out.push(match opened {
                    Ok(fd) => (
                        Order::Device {
                            device,
                            opened: Ok(()),
                        },
                        Some(fd),
                    ),
                    Err(e) => (
                        Order::Device {
                            device,
                            opened: Err(errno(&e)),
                        },
                        None,
                    ),
                });
            }
            Request::CloseDevice { device } => self.close(device),
            Request::TakeMaster { device } => {
                let taken = self.take_master(device);
                out.push((Order::Master { device, taken }, None));
            }
            Request::DropMaster { device } => {
                if let Some(Held {
                    kind: DeviceKind::Drm,
                    fd: Some(fd),
                    ..
                }) = self.devices.get(&device)
                    && let Err(e) = self.nodes.drop_master(fd.as_fd())
                {
                    log::debug!(target: "broker", "cannot drop DRM master on device {device}: {e}");
                }
            }
            Request::Released => {
                if self.releasing.is_some() {
                    self.let_go(&mut out, now);
                }
            }
            Request::SwitchVt { vt } => {
                if let Err(e) = self.vt.switch_to(u16::from(vt)) {
                    log::warn!(target: "broker", "vtium cannot switch to VT {vt} for its drawing process: {e}");
                }
            }
            Request::RestoreVt => {
                if self.vt.is_active()
                    && let Err(e) = self.vt.restore()
                {
                    log::warn!(target: "broker", "vtium cannot put the VT's modes back: {e}");
                }
            }
            other => {
                log::debug!(target: "broker", "the VT's holder does not serve {other:?}");
            }
        }
        out
    }

    /// Opens a device for the drawer and keeps the broker's copy. An input
    /// device opens only while the VT is the drawer's, so the drawer never
    /// reads keys typed on another VT; it is refused `EPERM` otherwise, and
    /// the drawer asks again after the next acquire.
    /// An input device whose open at the last acquire failed keeps its id
    /// with no copy, and opens again under that id.
    /// A node the broker holds under another id is refused `EEXIST`, and
    /// past [`DEVICES_MAX`] devices one more is refused `EMFILE`, so the
    /// broker holds one copy of each node and a bounded number in all.
    fn open(&mut self, device: u32, path: DevicePath) -> io::Result<OwnedFd> {
        if let Some(held) = self.devices.get(&device) {
            if held.fd.is_some() || held.path != path {
                return Err(io::Error::from(rustix::io::Errno::EXIST));
            }
            self.devices.remove(&device);
        }
        if self.devices.values().any(|held| held.path == path) {
            return Err(io::Error::from(rustix::io::Errno::EXIST));
        }
        if self.devices.len() >= DEVICES_MAX {
            return Err(io::Error::from(rustix::io::Errno::MFILE));
        }
        let (kind, fd) = self.nodes.open(Path::new(path.as_str()), &self.seat)?;
        if kind == DeviceKind::Input && !self.drawers() {
            log::debug!(target: "broker", "{} waits for the VT", path.as_str());
            return Err(io::Error::from(rustix::io::Errno::PERM));
        }
        let theirs = fd.try_clone()?;
        log::debug!(target: "broker", "opened {} for the drawing process", path.as_str());
        self.devices.insert(
            device,
            Held {
                kind,
                path,
                fd: Some(fd),
            },
        );
        Ok(theirs)
    }

    /// Forgets a device: master dropped on a card, an input device
    /// revoked, so the drawer's copy keeps nothing the broker no longer
    /// tracks, and then the broker's copy closed.
    fn close(&mut self, device: u32) {
        if let Some(held) = self.devices.remove(&device) {
            self.let_go_of(device, &held);
        }
    }

    /// Master on a card, only while the VT is the drawer's.
    fn take_master(&mut self, device: u32) -> bool {
        let Some(Held {
            kind: DeviceKind::Drm,
            fd: Some(fd),
            ..
        }) = self.devices.get(&device)
        else {
            return false;
        };
        if !self.drawers() {
            log::debug!(target: "broker", "device {device} waits for the VT before it takes DRM master");
            return false;
        }
        match self.nodes.set_master(fd.as_fd()) {
            Ok(()) => true,
            Err(e) => {
                log::debug!(target: "broker", "cannot take DRM master on device {device}: {e}");
                false
            }
        }
    }

    /// Master dropped on a card, an input device revoked and its copy
    /// closed. A card keeps its copy, which an acquire takes master on
    /// again.
    fn let_go_of(&mut self, device: u32, held: &Held) {
        let Some(fd) = held.fd.as_ref() else {
            return;
        };
        let done = match held.kind {
            DeviceKind::Drm => self.nodes.drop_master(fd.as_fd()),
            DeviceKind::Input => self.nodes.revoke(fd.as_fd()),
            DeviceKind::Fbdev => Ok(()),
        };
        // A card without master refuses the drop with EINVAL, which is
        // what the broker wants.
        if let Err(e) = done {
            log::debug!(target: "broker", "cannot let go of device {device}: {e}");
        }
    }

    /// Drops master on each card and revokes each input device, closing
    /// the broker's copy of each input device.
    fn let_go_all(&mut self) {
        let ids: Vec<u32> = self.devices.keys().copied().collect();
        for id in ids {
            let Some(mut held) = self.devices.remove(&id) else {
                continue;
            };
            self.let_go_of(id, &held);
            if held.kind == DeviceKind::Input {
                held.fd = None;
            }
            self.devices.insert(id, held);
        }
    }

    /// The end of a release, the drawer's answer or the bound's: every
    /// device let go, then the VT acknowledged.
    fn let_go(&mut self, out: &mut Orders, now: Instant) {
        self.releasing = None;
        self.let_go_all();
        if let Err(e) = self.vt.acknowledge_release() {
            log::warn!(target: "broker", "vtium cannot let the VT go: {e}");
        }
        self.events(out, now);
    }

    /// The VT is the drawer's again: master on each card, each input
    /// device opened anew under its old id, then [`Order::Back`].
    fn acquire(&mut self, out: &mut Orders) {
        let ids: Vec<u32> = self.devices.keys().copied().collect();
        for id in ids {
            let Some(held) = self.devices.get_mut(&id) else {
                continue;
            };
            match held.kind {
                DeviceKind::Drm => {
                    if let Some(fd) = held.fd.as_ref()
                        && let Err(e) = self.nodes.set_master(fd.as_fd())
                    {
                        log::debug!(target: "broker", "cannot take DRM master on device {id}: {e}");
                    }
                }
                DeviceKind::Fbdev => {}
                DeviceKind::Input => {
                    held.fd = None;
                    let opened = self
                        .nodes
                        .open(Path::new(held.path.as_str()), &self.seat)
                        .and_then(|(_, fd)| {
                            let theirs = fd.try_clone()?;
                            Ok((fd, theirs))
                        });
                    match opened {
                        Ok((fd, theirs)) => {
                            held.fd = Some(fd);
                            out.push((
                                Order::Device {
                                    device: id,
                                    opened: Ok(()),
                                },
                                Some(theirs),
                            ));
                        }
                        Err(e) => {
                            log::warn!(target: "broker", "vtium cannot open {} again: {e}", held.path.as_str());
                            out.push((
                                Order::Device {
                                    device: id,
                                    opened: Err(errno(&e)),
                                },
                                None,
                            ));
                        }
                    }
                }
            }
        }
        out.push((Order::Back, None));
    }

    /// Acts on what the VT reported, until it reports nothing more.
    fn events(&mut self, out: &mut Orders, now: Instant) {
        loop {
            let events: Vec<VtEvent> = self.vt.events().collect();
            if events.is_empty() {
                return;
            }
            for event in events {
                match event {
                    VtEvent::Acquired => {
                        // An acquire ends a release still waiting.
                        self.releasing = None;
                        self.acquire(out);
                    }
                    VtEvent::Release => self.begin_release(out, now),
                    // The VT was taken again while the drawer holds it:
                    // the drawer sets its displays up again. Its devices
                    // are untouched, so the order is Back alone.
                    VtEvent::Retaken => out.push((Order::Back, None)),
                    VtEvent::Hangup => self.hung_up = true,
                }
            }
        }
    }

    /// Tells the drawer to let its devices go, and starts the bound.
    fn begin_release(&mut self, out: &mut Orders, now: Instant) {
        if self.releasing.is_some() {
            return;
        }
        self.releasing = Some(now.checked_add(self.bound).unwrap_or(now));
        out.push((Order::Release, None));
    }

    /// Runs once a turn of the broker's loop: acts on the VT's events, and
    /// lets the devices go once a release has waited [`RELEASE_BOUND`].
    pub fn turn(&mut self, now: Instant) -> Orders {
        let mut out = Orders::new();
        self.events(&mut out, now);
        if let Some(by) = self.releasing
            && now >= by
        {
            log::warn!(
                target: "broker",
                "vtium's drawing process did not let the VT go within {} ms, so vtium takes its devices away",
                self.bound.as_millis()
            );
            self.let_go(&mut out, now);
        }
        out
    }

    /// When the broker's loop must run [`Holder::turn`] again: the end of
    /// a release's bound.
    pub fn due(&self) -> Option<Instant> {
        self.releasing
    }

    /// A [`crate::vt::Restore`] for the broker's panic hook, which puts the
    /// VT back where a panic runs no drop.
    pub fn restorer(&self) -> io::Result<crate::vt::Restore<C>> {
        self.vt.restorer()
    }

    /// Whether the VT hung up, so the drawer must stop.
    pub fn hung_up(&self) -> bool {
        self.hung_up
    }

    /// After the drawer's end: text mode, `VT_AUTO` and the keyboard mode
    /// saved at the start, then master dropped on each card and each input
    /// device revoked, in that order, and every device closed.
    pub fn give_back(&mut self) {
        self.vt.give_back();
        self.releasing = None;
        let devices = std::mem::take(&mut self.devices);
        for (id, held) in &devices {
            self.let_go_of(*id, held);
        }
    }
}

/// The errno of an error, for the drawer: the kernel's own, or `EIO`.
fn errno(e: &io::Error) -> Errno {
    let raw = e
        .raw_os_error()
        .unwrap_or(rustix::io::Errno::IO.raw_os_error());
    Errno(
        u16::try_from(raw)
            .ok()
            .filter(|n| (1..=ERRNO_MAX).contains(n))
            .unwrap_or(5),
    )
}
