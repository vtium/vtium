//! The VT through vtium's broker, for the drawer: the process that draws
//! without root under the privilege split.
//!
//! The broker runs as root. It holds the VT, opens the devices, holds DRM
//! master and revokes the input devices at a switch ([`Holder`]). The
//! drawer runs as the account `vtium` and reaches all of that through
//! [`BrokerVt`], which speaks the protocol of the crate
//! `vtium-broker-protocol` over the socket the broker passed it. It works
//! the way `LibseatVt` works with a seat manager:
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
//! - **The broker's end.** A closed socket reports [`VtEvent::Hangup`].
//!
//! The broker also sends orders that are not the VT's: a session's pty, a
//! helper's socket, a process's exit, blank, wake and stop.
//! [`BrokerVt::orders`] hands those to the owner, and
//! [`BrokerVt::request`] sends the owner's own requests.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": the requests and orders, and
//!   `LibseatVt` as the model of the broker's backend.
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
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use calloop::generic::Generic;
use calloop::ping::{Ping, make_ping};
use calloop::{Interest, Mode, PostAction};
use rustix::io::Errno;
use vtium_broker_protocol::{DevicePath, Got, Link, Order, Request};

use super::{MasterKeeper, Switch, Vt, VtEvent, switch_target};
use crate::event_loop::Loop;
use crate::input::{Evdev, EventDevice, KeyEvent, Open, Opening};

/// How long the drawer waits for the broker's answer to a request that
/// waits for one: a device's descriptor, or whether master was taken. The
/// broker answers at once, so only a broker that has stopped serving makes
/// the drawer wait this long.
pub const ANSWER_BOUND: Duration = Duration::from_millis(5_000);

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
    link: Link,
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
    /// The orders that are not the VT's, for the owner.
    others: Vec<(Order, Option<OwnedFd>)>,
    /// Wakes the loop when a wait for an answer read orders the loop has
    /// not seen, so the owner acts on them at the next turn.
    ping: Option<Ping>,
    bound: Duration,
    /// The test's stand-in broker, which answers while the drawer waits.
    #[cfg(test)]
    pump: Option<Rc<dyn Fn()>>,
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
        self.link.request(request)
    }

    /// Acts on one order: the VT's are taken here, and the others wait for
    /// the owner.
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
                (Ok(()), Some(fd)) => {
                    self.fresh.insert(device, fd);
                }
                (Err(e), _) => {
                    log::debug!(target: "vt", "vtium's root process could not open device {device} again: error {}", e.0);
                }
                (Ok(()), None) => {}
            },
            Order::Master { device, .. } => {
                log::debug!(target: "vt", "an answer about device {device}'s DRM master came late");
            }
            other => self.others.push((other, fd)),
        }
    }

    /// Reads every order waiting, and reports a closed socket once.
    fn read(&mut self) {
        loop {
            match self.link.next_order() {
                Ok(Got::Message(Ok((order, fd)))) => self.take(order, fd),
                Ok(Got::Message(Err(why))) => {
                    log::warn!(target: "vt", "vtium ignores a message from its root process: {why}");
                }
                Ok(Got::Nothing) => return,
                Ok(Got::Closed) | Err(_) => {
                    if !self.gone {
                        log::error!(target: "vt", "vtium's root process has closed its socket");
                        self.gone = true;
                        self.events.push(VtEvent::Hangup);
                    }
                    return;
                }
            }
        }
    }

    /// Sends `request` and waits up to [`ANSWER_BOUND`] for the order that
    /// `answers` picks. Each other order read meanwhile is acted on, and the
    /// loop is woken to see it.
    fn ask(
        &mut self,
        request: &Request,
        answers: impl Fn(&Order) -> bool,
    ) -> io::Result<(Order, Option<OwnedFd>)> {
        self.send(request)?;
        #[expect(
            clippy::disallowed_methods,
            reason = "the bound on a wait for the broker's answer reads the clock"
        )]
        let start = Instant::now();
        let by = start.checked_add(self.bound).unwrap_or(start);
        let seen = (self.events.len(), self.others.len());
        let answer = loop {
            #[cfg(test)]
            if let Some(pump) = self.pump.clone() {
                pump();
            }
            match self.link.next_order() {
                Ok(Got::Message(Ok((order, fd)))) if answers(&order) => break Ok((order, fd)),
                Ok(Got::Message(Ok((order, fd)))) => self.take(order, fd),
                Ok(Got::Message(Err(why))) => {
                    log::warn!(target: "vt", "vtium ignores a message from its root process: {why}");
                }
                Ok(Got::Nothing) => {
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "the bound on a wait for the broker's answer reads the clock"
                    )]
                    let left = by.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break Err(io::Error::from(io::ErrorKind::TimedOut));
                    }
                    wait_readable(&self.link, left)?;
                }
                Ok(Got::Closed) | Err(_) => {
                    if !self.gone {
                        self.gone = true;
                        self.events.push(VtEvent::Hangup);
                    }
                    break Err(io::Error::from(Errno::PIPE));
                }
            }
        };
        if (self.events.len(), self.others.len()) != seen
            && let Some(ping) = &self.ping
        {
            ping.ping();
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

/// The VT through vtium's broker, held from [`BrokerVt::start`] until
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

/// Waits until the socket is readable, or `left` has passed.
fn wait_readable(link: &Link, left: Duration) -> io::Result<()> {
    use rustix::event::{PollFd, PollFlags, Timespec};
    let mut fds = [PollFd::new(link, PollFlags::IN)];
    let bound = Timespec::try_from(left).unwrap_or(Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    });
    match rustix::event::poll(&mut fds, Some(&bound)) {
        Ok(_) | Err(Errno::INTR) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl BrokerVt {
    /// The VT `num` through the broker at the other end of `link`, in the
    /// loop. Store the VT where `access` finds it before the loop
    /// dispatches again.
    pub fn start<D, A>(lp: &mut Loop<D>, link: Link, num: u16, access: A) -> io::Result<BrokerVt>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut BrokerVt> + Clone + 'static,
    {
        let vt = BrokerVt::with(link, num, Box::new(tty0_active));
        let fd = vt.shared.borrow().link.as_fd().try_clone_to_owned()?;
        let read = access.clone();
        lp.handle()
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |_, _, data: &mut D| {
                    let Some(vt) = read(data) else {
                        return Ok(PostAction::Remove);
                    };
                    vt.readable();
                    Ok(if vt.shared.borrow().gone {
                        PostAction::Remove
                    } else {
                        PostAction::Continue
                    })
                },
            )
            .map_err(|e| io::Error::from(e.error))?;
        let (ping, source) = make_ping()?;
        lp.handle()
            .insert_source(source, move |(), (), data: &mut D| {
                // The orders were read already; the turn that follows lets
                // the owner see them.
                let _ = access(data);
            })
            .map_err(|e| io::Error::from(e.error))?;
        vt.shared.borrow_mut().ping = Some(ping);
        Ok(vt)
    }

    /// The VT `num` through the broker at the other end of `link`, reading
    /// the VT shown through `shown`, outside any loop.
    pub(crate) fn with(link: Link, num: u16, shown: Shown) -> BrokerVt {
        BrokerVt {
            shared: Rc::new(RefCell::new(Shared {
                link,
                state: State::Inactive,
                back_owed: false,
                gone: false,
                events: Vec::new(),
                ids: BTreeMap::new(),
                next_id: 1,
                fresh: BTreeMap::new(),
                others: Vec::new(),
                ping: None,
                bound: ANSWER_BOUND,
                #[cfg(test)]
                pump: None,
            })),
            num,
            back: None,
            shown,
            bell_told: false,
            pending: Vec::new(),
        }
    }

    /// The test's stand-in broker, which answers each time the drawer waits.
    #[cfg(test)]
    pub(crate) fn pumped_by(&self, pump: Rc<dyn Fn()>) {
        self.shared.borrow_mut().pump = Some(pump);
    }

    /// Reads every order waiting on the socket.
    pub(crate) fn readable(&mut self) {
        self.shared.borrow_mut().read();
    }

    /// The orders that are not the VT's, since the last call: a session's
    /// pty, a helper's socket, a process's exit, blank, wake and stop.
    pub fn orders(&mut self) -> Vec<(Order, Option<OwnedFd>)> {
        std::mem::take(&mut self.shared.borrow_mut().others)
    }

    /// Sends one of the owner's own requests to the broker.
    pub fn request(&self, request: &Request) -> io::Result<()> {
        self.shared.borrow().send(request)
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
