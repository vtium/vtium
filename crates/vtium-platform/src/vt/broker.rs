//! The VT through vtium's broker, for the drawer: the process that draws
//! without root under the privilege split.
//!
//! The broker runs as root. It holds the VT, opens the devices, holds DRM
//! master and revokes the input devices at a switch ([`Holder`]). The
//! drawer runs as the account `vtium` and reaches all of that through
//! [`BrokerVt`], over the drawer's one socket to the broker. The drawer
//! reads that socket in one place, its [`Courier`]: the backend sends its
//! requests through the courier and never reads the socket itself. The
//! owner reads every order through the courier and hands the VT's orders
//! (`Device`, `Master`, `Release` and `Back`) to [`BrokerVt::take`], and
//! the socket's close to [`BrokerVt::closed`]. The backend works the way
//! `LibseatVt` works with a seat manager:
//!
//! - **Devices.** [`Vt::open_device`] asks the broker to open a card or a
//!   framebuffer and waits for the answer, which carries the broker's
//!   descriptor. The keyboards and pointers open the same way through
//!   [`BrokerInput`], which [`crate::input::Input::open_through_broker`]
//!   takes. Each device has an id the drawer chooses, never 0.
//! - **Master.** The drawer never sets or drops DRM master itself, since
//!   the kernel refuses its own call. A card asks the broker through the
//!   [`MasterKeeper`] that [`Vt::master_keeper`] gives.
//! - **A release.** [`Order::Release`] reports [`VtEvent::Release`]. Once
//!   the owner has put the displays and the input devices to sleep,
//!   [`Vt::acknowledge_release`] sends [`Request::Released`]. A drawer
//!   that takes longer than the broker's bound loses the devices all the
//!   same.
//! - **An acquire.** The broker sends a new descriptor for each input
//!   device the drawer had open, then [`Order::Back`], which reports
//!   [`VtEvent::Acquired`]. The input devices then open from those
//!   descriptors. A `Back` while the VT is already the drawer's reports
//!   [`VtEvent::Retaken`]: the broker took the VT again after another
//!   program changed its modes.
//! - **Switches.** The switch keys, [`Vt::activate`] and [`Vt::leave`] ask
//!   the broker with [`Request::SwitchVt`], and [`Vt::restore`] with
//!   [`Request::RestoreVt`]. The drawer reads the VT shown from tty0's
//!   `active` attribute, which every user may read.
//! - **The broker's end.** The socket's close reports [`VtEvent::Hangup`].
//!
//! References:
//! - ADR-1400 D4, "The device protocol": the requests and orders, one
//!   socket, and `LibseatVt` as the model of the broker's backend.
//! - ADR-1400 D4, "What the drawer can no longer do": the drawer asks the
//!   broker for master and switches.

mod holder;
#[cfg(test)]
mod stand_in;
#[cfg(test)]
mod tests;

pub use holder::{Holder, Kernel, Orders, RELEASE_BOUND};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rustix::io::Errno;
use vtium_broker_protocol::{Courier, DevicePath, Order, Request};

use super::{MasterKeeper, Switch, Vt, VtEvent, switch_target};
use crate::input::{Evdev, EventDevice, KeyEvent, Open, Opening};

/// Whether the VT is the drawer's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Inactive,
    Active,
    /// The broker asked for the devices, and the owner has not answered
    /// yet.
    Releasing,
}

/// Reads the VT shown: tty0's `active`, or a test's stand-in.
type Shown = Box<dyn Fn() -> io::Result<u16>>;

/// What the VT shares with the keyboards and pointers and the cards'
/// master keepers.
struct Shared {
    courier: Rc<Courier>,
    state: State,
    /// A `Back` that came while a release waited, reported once the owner
    /// acknowledged the release.
    back_owed: bool,
    /// The broker's socket closed.
    gone: bool,
    events: Vec<VtEvent>,
    /// The id of each device the drawer named to the broker, by node.
    ids: BTreeMap<PathBuf, u32>,
    next_id: u32,
    /// The input devices' new descriptors, which the broker sent at an
    /// acquire, by id, until the input opens them.
    fresh: BTreeMap<u32, OwnedFd>,
}

impl Shared {
    /// The id of the device at `node`, a new one the first time.
    fn id(&mut self, node: &Path) -> u32 {
        if let Some(&id) = self.ids.get(node) {
            return id;
        }
        let id = self.next_id.max(1);
        self.next_id = id.checked_add(1).unwrap_or(1);
        self.ids.insert(node.to_owned(), id);
        id
    }

    /// Sends a request, failing once the broker is gone.
    fn send(&self, request: &Request) -> io::Result<()> {
        if self.gone {
            return Err(io::Error::from(Errno::PIPE));
        }
        self.courier.send(request)
    }

    /// The broker's socket closed: reported once as a hangup.
    fn closed(&mut self) {
        if !self.gone {
            log::error!(target: "vt", "vtium's root process has closed its socket");
            self.gone = true;
            self.events.push(VtEvent::Hangup);
        }
    }

    /// Acts on one of the VT's orders.
    fn take(&mut self, order: Order, fd: Option<OwnedFd>) {
        match order {
            Order::Release => match self.state {
                State::Active => {
                    self.state = State::Releasing;
                    self.events.push(VtEvent::Release);
                }
                // The drawer holds nothing of the VT, so it lets go at once.
                State::Inactive => {
                    if let Err(e) = self.send(&Request::Released) {
                        log::warn!(target: "vt", "cannot tell vtium's root process that the devices are let go: {e}");
                    }
                }
                State::Releasing => {}
            },
            Order::Back => match self.state {
                State::Inactive => {
                    self.state = State::Active;
                    self.events.push(VtEvent::Acquired);
                }
                State::Active => self.events.push(VtEvent::Retaken),
                State::Releasing => self.back_owed = true,
            },
            Order::Device { device, opened } => match (opened, fd) {
                // an input device opened anew at an acquire, under an id
                // the drawer holds
                (Ok(()), Some(fd)) if self.ids.values().any(|&id| id == device) => {
                    self.fresh.insert(device, fd);
                }
                // The answer to an open whose call gave up: the drawer
                // dropped the id, but the broker still holds its copy, so
                // the drawer tells it to let the device go.
                (Ok(()), Some(_)) => {
                    log::debug!(target: "vt", "vtium's root process opened device {device} after the drawing process stopped waiting, so the drawing process closes it");
                    if let Err(e) = self.send(&Request::CloseDevice { device }) {
                        log::debug!(target: "vt", "cannot tell vtium's root process to close device {device}: {e}");
                    }
                }
                (Err(e), _) => {
                    log::debug!(target: "vt", "vtium's root process could not open device {device} again: error {}", e.0);
                }
                (Ok(()), None) => {}
            },
            Order::Master { device, .. } => {
                log::debug!(target: "vt", "an answer about device {device}'s DRM master came late");
            }
            other => log::debug!(target: "vt", "the VT takes no {other:?}"),
        }
    }

    /// Sends `request` through the courier and waits for the order that
    /// `answers` picks. A closed socket is a hangup.
    fn ask(
        &mut self,
        request: &Request,
        answers: impl Fn(&Order) -> bool,
    ) -> io::Result<(Order, Option<OwnedFd>)> {
        if self.gone {
            return Err(io::Error::from(Errno::PIPE));
        }
        let answer = self.courier.call(request, answers);
        if let Err(e) = &answer
            && e.kind() == io::ErrorKind::BrokenPipe
        {
            self.closed();
        }
        answer
    }

    /// Asks the broker to open the device at `node`, and returns its
    /// descriptor. The broker's refusal comes back as its errno.
    fn open(&mut self, node: &Path) -> io::Result<OwnedFd> {
        let path = node
            .to_str()
            .and_then(DevicePath::new)
            .ok_or_else(|| io::Error::from(Errno::INVAL))?;
        let device = self.id(node);
        let asked = self.ask(
            &Request::OpenDevice { device, path },
            |o| matches!(o, Order::Device { device: d, .. } if *d == device),
        );
        let answer = match asked {
            Ok((Order::Device { opened: Ok(()), .. }, Some(fd))) => Ok(fd),
            Ok((Order::Device { opened: Err(e), .. }, _)) => {
                Err(io::Error::from_raw_os_error(i32::from(e.0)))
            }
            Ok(_) => Err(io::Error::from(Errno::PROTO)),
            Err(e) => Err(e),
        };
        if answer.is_err() {
            self.ids.remove(node);
        }
        answer
    }

    /// Tells the broker the drawer no longer uses the device at `node`.
    fn close(&mut self, node: &Path) {
        let Some(device) = self.ids.remove(node) else {
            return;
        };
        self.fresh.remove(&device);
        if let Err(e) = self.send(&Request::CloseDevice { device }) {
            log::debug!(target: "vt", "cannot tell vtium's root process to close {}: {e}", node.display());
        }
    }
}

/// The VT through vtium's broker, held from [`BrokerVt::new`] until
/// dropped.
pub struct BrokerVt {
    shared: Rc<RefCell<Shared>>,
    num: u16,
    /// The VT shown when [`Vt::activate`] asked for this one, which
    /// [`Vt::leave`] goes back to. `None` while vtium has not switched to
    /// its VT itself.
    back: Option<u16>,
    shown: Shown,
    /// The bell was asked for and could not ring, which is logged once.
    bell_told: bool,
    /// The events [`Vt::events`] hands out.
    pending: Vec<VtEvent>,
}

impl std::fmt::Debug for BrokerVt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shared = self.shared.borrow();
        f.debug_struct("BrokerVt")
            .field("number", &self.num)
            .field("state", &shared.state)
            .field("devices", &shared.ids)
            .finish_non_exhaustive()
    }
}

/// tty0's `active`, the VT the kernel shows, as `ttyN`.
fn tty0_active() -> io::Result<u16> {
    let text = std::fs::read_to_string("/sys/class/tty/tty0/active")?;
    text.trim()
        .strip_prefix("tty")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
}

impl BrokerVt {
    /// The VT `num` through the broker that `courier` reaches. The owner
    /// hands the VT's orders to [`BrokerVt::take`] as the courier reads
    /// them, and the socket's close to [`BrokerVt::closed`].
    pub fn new(courier: Rc<Courier>, num: u16) -> BrokerVt {
        BrokerVt::with(courier, num, Box::new(tty0_active))
    }

    /// [`BrokerVt::new`], reading the VT shown through `shown`.
    pub(crate) fn with(courier: Rc<Courier>, num: u16, shown: Shown) -> BrokerVt {
        BrokerVt {
            shared: Rc::new(RefCell::new(Shared {
                courier,
                state: State::Inactive,
                back_owed: false,
                gone: false,
                events: Vec::new(),
                ids: BTreeMap::new(),
                next_id: 1,
                fresh: BTreeMap::new(),
            })),
            num,
            back: None,
            shown,
            bell_told: false,
            pending: Vec::new(),
        }
    }

    /// Whether `order` is one of the VT's, which the owner hands to
    /// [`BrokerVt::take`].
    pub fn takes(order: &Order) -> bool {
        matches!(
            order,
            Order::Device { .. } | Order::Master { .. } | Order::Release | Order::Back
        )
    }

    /// Acts on one of the VT's orders, with its descriptor, as the courier
    /// read it. The next [`Vt::events`] reports what it changed.
    pub fn take(&mut self, order: Order, fd: Option<OwnedFd>) {
        self.shared.borrow_mut().take(order, fd);
    }

    /// The broker's socket closed. The next [`Vt::events`] reports
    /// [`VtEvent::Hangup`], once.
    pub fn closed(&mut self) {
        self.shared.borrow_mut().closed();
    }

    /// The keyboards and pointers, opened through the broker from here on.
    pub fn input(&self) -> BrokerInput {
        self.input_with(Box::new(|_, fd| Ok(Evdev::new(fd))))
    }

    /// [`BrokerVt::input`], with each device made by `make`.
    pub(crate) fn input_with<E>(&self, make: MakeDevice<E>) -> BrokerInput<E> {
        BrokerInput {
            shared: Rc::clone(&self.shared),
            make,
        }
    }

    fn state(&self) -> State {
        self.shared.borrow().state
    }

    fn send(&self, request: &Request) -> io::Result<()> {
        self.shared.borrow().send(request)
    }

    /// Asks the broker to switch to VT `target`.
    fn switch(&self, target: u16) -> io::Result<()> {
        let vt = u8::try_from(target).map_err(|_| io::Error::from(Errno::INVAL))?;
        self.send(&Request::SwitchVt { vt })
    }

    fn hung_up(&self) -> io::Result<()> {
        if self.shared.borrow().gone {
            Err(io::Error::from(Errno::PIPE))
        } else {
            Ok(())
        }
    }
}

impl Vt for BrokerVt {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        let events = std::mem::take(&mut self.shared.borrow_mut().events);
        self.pending = events;
        self.pending.drain(..)
    }

    fn acknowledge_release(&mut self) -> io::Result<()> {
        let mut shared = self.shared.borrow_mut();
        if shared.state != State::Releasing {
            return Ok(());
        }
        shared.state = State::Inactive;
        let sent = shared.send(&Request::Released);
        if std::mem::take(&mut shared.back_owed) {
            shared.state = State::Active;
            shared.events.push(VtEvent::Acquired);
        }
        sent
    }

    /// The broker takes the devices at its bound whatever the drawer
    /// answers, so the drawer cannot keep the VT.
    fn refuse_release(&mut self) -> io::Result<()> {
        if self.state() != State::Releasing {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "vtium's root process lets the VT go at its bound, so the VT cannot be kept",
        ))
    }

    fn activate(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        if self.state() == State::Active {
            return Ok(Switch::Done);
        }
        let shown = (self.shown)().ok();
        self.switch(self.num)?;
        log::debug!(target: "vt", "entering VT {} through vtium's root process", self.num);
        if shown != Some(self.num) {
            self.back = shown;
        }
        Ok(Switch::Pending)
    }

    fn leave(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        let Some(back) = self.back else {
            return Ok(Switch::Done);
        };
        if (self.shown)()? != self.num || back == self.num {
            return Ok(Switch::Done);
        }
        self.switch(back)?;
        log::debug!(target: "vt", "leaving VT {} for VT {back} through vtium's root process", self.num);
        Ok(Switch::Pending)
    }

    fn wait(&mut self) -> io::Result<Switch> {
        self.hung_up()?;
        if self.state() == State::Active {
            return Ok(Switch::Done);
        }
        log::info!(target: "vt", "waiting for VT {} to be shown", self.num);
        Ok(Switch::Pending)
    }

    fn is_active(&self) -> bool {
        self.state() == State::Active
    }

    /// Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys ask the broker for
    /// that VT while this one is the drawer's; this VT's own key is taken
    /// and does nothing.
    fn key(&mut self, key: &KeyEvent) -> bool {
        let Some(target) = switch_target(key) else {
            return false;
        };
        if self.state() != State::Active {
            return false;
        }
        if target != self.num
            && let Err(e) = self.switch(target)
        {
            log::warn!(target: "vt", "cannot leave VT {} for VT {target}: {e}", self.num);
        }
        true
    }

    fn is_own_key(&self, key: &KeyEvent) -> bool {
        switch_target(key) == Some(self.num) && self.state() == State::Active
    }

    fn restore(&mut self) -> io::Result<()> {
        self.send(&Request::RestoreVt)
    }

    /// The bell needs the VT itself, which only the broker holds, and the
    /// protocol has no request for it, so the drawer's bell is silent.
    fn bell(&mut self) {
        if !std::mem::replace(&mut self.bell_told, true) {
            log::debug!(target: "vt", "the console's bell is silent while vtium runs as two processes");
        }
    }

    fn number(&self) -> Option<u16> {
        Some(self.num)
    }

    fn open_device(&mut self, path: &Path) -> io::Result<OwnedFd> {
        self.shared.borrow_mut().open(path)
    }

    fn close_device(&mut self, path: &Path) {
        self.shared.borrow_mut().close(path);
    }

    fn master_keeper(&self, path: &Path) -> Option<Box<dyn MasterKeeper>> {
        let device = *self.shared.borrow().ids.get(path)?;
        Some(Box::new(BrokerMaster {
            shared: Rc::clone(&self.shared),
            device,
        }))
    }
}

/// DRM master on one card, which the broker sets and drops on its own copy
/// of the card. Master belongs to the open file, so the drawer's copy holds
/// it too.
struct BrokerMaster {
    shared: Rc<RefCell<Shared>>,
    device: u32,
}

impl MasterKeeper for BrokerMaster {
    /// Asks the broker for master and waits for its answer. A refusal, as
    /// while the VT is not the drawer's, reads as `EBUSY`, the kernel's
    /// answer while another file holds master.
    fn take(&self) -> io::Result<()> {
        let device = self.device;
        let (answer, _) = self.shared.borrow_mut().ask(
            &Request::TakeMaster { device },
            |o| matches!(o, Order::Master { device: d, .. } if *d == device),
        )?;
        match answer {
            Order::Master { taken: true, .. } => Ok(()),
            _ => Err(io::Error::from(Errno::BUSY)),
        }
    }

    fn let_go(&self) -> io::Result<()> {
        self.shared.borrow().send(&Request::DropMaster {
            device: self.device,
        })
    }
}

/// Makes the input device from the descriptor the broker gave for a node:
/// an [`Evdev`], or a test's stand-in.
type MakeDevice<E> = Box<dyn Fn(&Path, OwnedFd) -> io::Result<E>>;

/// The keyboards and pointers opened through the broker, for
/// [`crate::input::Input::open_through_broker`]. It shares the socket with
/// the [`BrokerVt`] it came from:
///
/// - After an acquire, each device opens from the new descriptor the
///   broker sent for it.
/// - A device asked for while the VT is not the drawer's waits for the
///   input's next wake-up, which follows the next acquire; so does one the
///   broker refuses with `EPERM`, as it refuses an input device while a
///   release waits.
/// - A device closed while the VT is the drawer's is closed at the broker
///   too. One closed by a release keeps its id, so the broker opens it
///   anew at the acquire.
pub struct BrokerInput<E = Evdev> {
    shared: Rc<RefCell<Shared>>,
    make: MakeDevice<E>,
}

impl<E> std::fmt::Debug for BrokerInput<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerInput").finish_non_exhaustive()
    }
}

impl<E: EventDevice> Open for BrokerInput<E> {
    type Device = E;

    fn open(&mut self, node: &Path) -> io::Result<Opening<E>> {
        let fd = {
            let mut shared = self.shared.borrow_mut();
            let fresh = shared
                .ids
                .get(node)
                .copied()
                .and_then(|id| shared.fresh.remove(&id));
            match fresh {
                Some(fd) => fd,
                None if shared.gone => return Err(io::Error::from(Errno::PIPE)),
                None if shared.state != State::Active => return Ok(Opening::Wake),
                None => match shared.open(node) {
                    Ok(fd) => fd,
                    Err(e) if e.raw_os_error() == Some(Errno::PERM.raw_os_error()) => {
                        return Ok(Opening::Wake);
                    }
                    Err(e) => return Err(e),
                },
            }
        };
        // vtium reads a device until it has no more events, so the
        // descriptor must not block. It shares its open file with the
        // broker's.
        rustix::fs::fcntl_getfl(&fd)
            .and_then(|flags| rustix::fs::fcntl_setfl(&fd, flags | rustix::fs::OFlags::NONBLOCK))?;
        match (self.make)(node, fd) {
            Ok(dev) => Ok(Opening::Open(dev)),
            Err(e) => {
                self.shared.borrow_mut().close(node);
                Err(e)
            }
        }
    }

    fn close(&mut self, node: &Path, dev: E) {
        drop(dev);
        let mut shared = self.shared.borrow_mut();
        if shared.state == State::Active {
            shared.close(node);
        }
    }

    fn forget(&mut self, node: &Path) {
        self.shared.borrow_mut().close(node);
    }
}
