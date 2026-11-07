//! The seat through libseat, with the `libseat` feature (ADR-1400's L2
//! layer, vtium/vtium#34).
//!
//! A seat manager, seatd or logind, owns the seat's display devices and its
//! VT switching, and vtium is one of its clients. [`LibseatVt`] is the
//! [`Vt`] backend that asks it through libseat. libseat calls
//! vtium back from [`Seat::dispatch`] with two messages:
//!
//! - **enable**: the seat is vtium's. vtium reports [`VtEvent::Acquired`],
//!   and puts its VT, when it has one, in graphics mode with the keyboard
//!   off, since logind leaves the keyboard on.
//! - **disable**: the seat is asked for. vtium reports [`VtEvent::Release`],
//!   and acknowledges it to the seat manager once the owner has put the
//!   displays to sleep ([`Vt::acknowledge_release`]). An enable that comes
//!   while that acknowledgement waits is reported after it, so the owner
//!   always sees the release finish before the next acquire.
//!
//! Nothing here waits on the loop. Where the seat manager answers that the
//! seat or a device is busy, vtium keeps it pending and tries again from a
//! timer in the loop every [`crate::bounds::SEAT_RETRY_MS`], so signals,
//! timers and the other sources are served in between:
//!
//! - **The seat** is tried up to [`crate::bounds::SEAT_OPENS_MAX`] times
//!   while libseat's answer says the seat is still held by its previous
//!   client: `EBUSY` or `EBADF`, and while seatd listens also `ENOSYS`,
//!   `EPIPE` or `EIO`, since seatd refuses such a client by hanging up
//!   ([`busy`]). Past that, or on any other failure once vtium has chosen
//!   the seat, vtium logs an error and reports [`VtEvent::Hangup`].
//! - **A device** is tried up to [`crate::bounds::DEVICE_OPENS_MAX`] times
//!   while libseat fails with `ENODEV`, `EACCES` or `EBUSY`, which udev's
//!   report of a device can bring before the seat manager is ready for it.
//!   [`Vt::open_device`] answers [`io::ErrorKind::WouldBlock`] meanwhile,
//!   and [`Vt::settled`] names the device once it has opened or failed. A
//!   device asked for before the seat is enabled waits for the enable.
//!
//! seatd gives a new client the VT shown when it opens the seat. So when
//! vtium's VT is not shown at the start and seatd listens, vtium opens the
//! seat only once its VT is shown, looking every
//! [`crate::bounds::SEAT_RETRY_MS`]; [`Vt::activate`] asks the kernel to show
//! it. logind takes the session from vtium's login session instead, so the
//! seat opens at once.
//!
//! [`LibseatVt::start`] tries the seat. When no seat manager answers, it
//! hands the VT back, and the caller runs on the kernel's VT, as a build
//! without the feature does.
//!
//! libseat is reached through the `libseat` crate ([`Libseat`]); the tests
//! stand in for it through the [`Manager`] and [`Seat`] traits.

mod binding;
#[cfg(test)]
mod stand_in;
#[cfg(test)]
mod tests;

pub use binding::Libseat;

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, Mode, PostAction, RegistrationToken};
use rustix::io::Errno;

use crate::event_loop::Loop;
use crate::input::KeyEvent;
use crate::vt::switch_target;
use crate::vt::{Console, DisplayMode, KbMode, Switch, Tty, Vt, VtEvent};

/// What the seat manager tells vtium through libseat's listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Told {
    /// The seat is vtium's.
    Enable,
    /// The seat is asked for; vtium must acknowledge it.
    Disable,
}

/// Where libseat's listener puts what the seat manager tells, for the
/// VT to read after each dispatch.
pub(crate) type Inbox = Rc<RefCell<VecDeque<Told>>>;

/// libseat's calls on an open seat.
pub(crate) trait Seat {
    /// A device opened on the seat, kept until it is closed.
    type Device;
    /// A descriptor for the loop to watch: readable when libseat has
    /// something to dispatch.
    fn fd(&mut self) -> io::Result<OwnedFd>;
    /// Reads what the seat manager sent and calls the listener, which
    /// puts it in the inbox; never waits.
    fn dispatch(&mut self) -> io::Result<()>;
    /// Acknowledges a disable.
    fn disable(&mut self) -> io::Result<()>;
    /// Opens the device at `path`, giving the device to keep and a
    /// descriptor of it for the owner.
    fn open_device(&mut self, path: &Path) -> io::Result<(Self::Device, OwnedFd)>;
    fn close_device(&mut self, device: Self::Device) -> io::Result<()>;
    /// Asks the seat manager to switch to session `session`, the VT of that
    /// number on a seat with VTs.
    fn switch_session(&mut self, session: u16) -> io::Result<()>;
    /// The seat's name.
    fn name(&mut self) -> String;
}

/// Opens seats: libseat, or the tests' stand-in.
pub(crate) trait Manager {
    type Seat: Seat;
    /// Opens the seat, with its listener putting what it is told in
    /// `inbox`.
    fn open(&mut self, inbox: Inbox) -> io::Result<Self::Seat>;
    /// Whether seatd listens on its socket, which decides whether a VT not
    /// shown must be waited for before the seat opens.
    fn seatd_listens(&self) -> bool;
}

/// What [`LibseatVt::start`] chose.
#[derive(Debug)]
pub enum Chosen<V, C> {
    /// A seat manager answered: the seat is vtium's VT backend.
    Seat(V),
    /// No seat manager answered: the VT given, handed back for the
    /// kernel's VT backend.
    Kernel(Option<(C, u16)>),
}

/// How often and how many times the seat and its devices are tried; the
/// bounds', or a test's.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tries {
    pub(crate) every: Duration,
    pub(crate) seat: usize,
    pub(crate) device: usize,
}

impl Tries {
    pub(crate) fn bounds() -> Tries {
        let ms = u64::try_from(crate::bounds::SEAT_RETRY_MS).unwrap_or(u64::MAX);
        Tries {
            every: Duration::from_millis(ms),
            seat: crate::bounds::SEAT_OPENS_MAX,
            device: crate::bounds::DEVICE_OPENS_MAX,
        }
    }
}

/// Where the seat's connection stands.
enum Conn<S> {
    /// Waiting for the VT to be shown before the seat opens.
    Shown,
    /// The seat manager answered busy this many times.
    Opening(usize),
    Open(S),
    /// The seat could not open, or the seat manager went away.
    Gone,
}

/// Whether the seat is vtium's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Inactive,
    Active,
    /// The seat manager asked for the seat, and the owner has not answered
    /// yet.
    Releasing,
}

/// A device asked for and not yet given to the owner.
enum Pending<D> {
    /// Waiting for the seat to be enabled.
    Seat,
    /// Tried this many times; tried again at the next tick.
    Tries(usize),
    /// Opened by a later try, for the owner to take.
    Opened(D, OwnedFd),
    /// Failed, for the owner to be told.
    Failed(io::Error),
}

/// What [`LibseatVt::start_with`] found of the seat.
struct Found<S> {
    conn: Conn<S>,
    inbox: Inbox,
    /// seatd listens.
    seatd: bool,
}

/// What the VT asks of the loop it runs in, made by [`LibseatVt::start`]
/// with the loop's data type inside.
struct Hooks {
    /// Calls [`LibseatVt::tick`] once, after the time given.
    arm: Box<dyn Fn(Duration) -> io::Result<()>>,
    /// Watches the seat's descriptor, dispatching it when readable.
    watch: Box<dyn Fn(OwnedFd) -> io::Result<RegistrationToken>>,
    unwatch: Box<dyn Fn(RegistrationToken)>,
}

/// The seat through a seat manager, held from [`LibseatVt::start`] until
/// dropped. Dropping it closes the devices it holds and the seat, and puts
/// its VT, when it has one, back in text mode with the keyboard mode found
/// at the start.
#[expect(
    private_bounds,
    reason = "the manager is Libseat and the console a Tty outside the crate"
)]
pub struct LibseatVt<M: Manager = Libseat, C: Console = Tty> {
    manager: M,
    conn: Conn<M::Seat>,
    inbox: Inbox,
    /// seatd listened at the start.
    seatd: bool,
    /// The VT the seat runs on, when it has VTs, and its number.
    tty: Option<(C, u16)>,
    /// The VT shown at the start, which [`Vt::leave`] goes back to.
    saved: Option<u16>,
    /// The keyboard mode found at the start, restored when the seat is
    /// let go.
    kb_mode: KbMode,
    state: State,
    /// An enable that came while a release waited, reported once the owner
    /// acknowledged it.
    enable_owed: bool,
    events: Vec<VtEvent>,
    /// The devices asked for and not yet given.
    pending: BTreeMap<PathBuf, Pending<<M::Seat as Seat>::Device>>,
    /// The devices given to the owner.
    open: BTreeMap<PathBuf, <M::Seat as Seat>::Device>,
    settled: Vec<PathBuf>,
    name: Option<String>,
    tries: Tries,
    hooks: Hooks,
    /// A tick is due.
    armed: bool,
    watched: Option<RegistrationToken>,
}

impl<M: Manager, C: Console> std::fmt::Debug for LibseatVt<M, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibseatVt")
            .field("number", &self.number())
            .field("state", &self.state)
            .field("pending", &self.pending.keys().collect::<Vec<_>>())
            .field("open", &self.open.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl LibseatVt<Libseat, Tty> {
    /// Opens the seat through libseat in the loop, on the VT `tty` with its
    /// number when the seat has VTs. Store the VT where `access` finds it
    /// before the loop dispatches again. Returns the VT handed back when no
    /// seat manager answers.
    pub fn start<D, A>(
        lp: &mut Loop<D>,
        tty: Option<(Tty, u16)>,
        access: A,
    ) -> io::Result<Chosen<LibseatVt<Libseat, Tty>, Tty>>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut LibseatVt<Libseat, Tty>> + Clone + 'static,
    {
        LibseatVt::start_with(lp, Libseat, tty, Tries::bounds(), access)
    }
}

#[expect(
    private_bounds,
    reason = "the manager is Libseat and the console a Tty outside the crate"
)]
impl<M: Manager + 'static, C: Console + 'static> LibseatVt<M, C> {
    /// [`LibseatVt::start`] with any manager, console and tries.
    pub(crate) fn start_with<D, A>(
        lp: &mut Loop<D>,
        manager: M,
        tty: Option<(C, u16)>,
        tries: Tries,
        access: A,
    ) -> io::Result<Chosen<LibseatVt<M, C>, C>>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut LibseatVt<M, C>> + Clone + 'static,
    {
        let shown = match &tty {
            Some((console, num)) => Some(console.active()? == *num),
            None => None,
        };
        let seatd = manager.seatd_listens();
        let inbox = Inbox::default();
        let mut manager = manager;
        let conn = if shown == Some(false) && seatd {
            log::info!(target: "seat", "seatd listens: the seat opens once the VT is shown");
            Conn::Shown
        } else {
            match manager.open(Rc::clone(&inbox)) {
                Ok(seat) => Conn::Open(seat),
                Err(e) if busy(&e, seatd) => {
                    log::info!(target: "seat", "the seat is busy, waiting for its previous client to let it go: {e}");
                    Conn::Opening(1)
                }
                Err(e) => {
                    log::info!(target: "seat", "no seat manager answered, so vtium uses the kernel's VT: {e}");
                    return Ok(Chosen::Kernel(tty));
                }
            }
        };
        let found = Found { conn, inbox, seatd };
        Ok(Chosen::Seat(LibseatVt::new(
            lp, manager, found, tty, tries, access,
        )?))
    }

    fn new<D, A>(
        lp: &mut Loop<D>,
        manager: M,
        found: Found<M::Seat>,
        tty: Option<(C, u16)>,
        tries: Tries,
        access: A,
    ) -> io::Result<LibseatVt<M, C>>
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut LibseatVt<M, C>> + Clone + 'static,
    {
        let handle = lp.handle();
        let (h, a) = (handle.clone(), access.clone());
        let arm = Box::new(move |after: Duration| {
            let a = a.clone();
            h.insert_source(Timer::from_duration(after), move |_, _, data: &mut D| {
                if let Some(vt) = a(data) {
                    vt.tick();
                }
                TimeoutAction::Drop
            })
            .map(|_| ())
            .map_err(|e| io::Error::from(e.error))
        });
        let h = handle.clone();
        let watch = Box::new(move |fd: OwnedFd| {
            let a = access.clone();
            h.insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |_, _, data: &mut D| {
                    let keep = a(data).is_some_and(|vt| vt.readable());
                    Ok(if keep {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
            .map_err(|e| io::Error::from(e.error))
        });
        let unwatch = Box::new(move |token| handle.remove(token));
        let (saved, kb_mode) = match &tty {
            Some((console, num)) => {
                let saved = console.active().ok();
                let kb_mode = match console.keyboard_mode() {
                    Ok(KbMode::Off | KbMode::Raw | KbMode::MediumRaw) | Err(_) => {
                        log::debug!(target: "seat", "VT {num}'s keyboard mode is restored as K_UNICODE");
                        KbMode::Unicode
                    }
                    Ok(mode) => mode,
                };
                (saved, kb_mode)
            }
            None => (None, KbMode::Unicode),
        };
        let Found { conn, inbox, seatd } = found;
        let mut vt = LibseatVt {
            manager,
            conn,
            inbox,
            seatd,
            tty,
            saved,
            kb_mode,
            state: State::Inactive,
            enable_owed: false,
            events: Vec::new(),
            pending: BTreeMap::new(),
            open: BTreeMap::new(),
            settled: Vec::new(),
            name: None,
            tries,
            hooks: Hooks {
                arm,
                watch,
                unwatch,
            },
            armed: false,
            watched: None,
        };
        if matches!(vt.conn, Conn::Open(_)) {
            vt.opened();
        }
        vt.rearm();
        Ok(vt)
    }
}

#[expect(
    private_bounds,
    reason = "the manager is Libseat and the console a Tty outside the crate"
)]
impl<M: Manager, C: Console> LibseatVt<M, C> {
    /// The loop's timer: the VT looked at, the seat and the devices tried
    /// again, and the timer set again while any still waits.
    pub(crate) fn tick(&mut self) {
        self.armed = false;
        match self.conn {
            Conn::Shown if self.shown() => self.connect(),
            Conn::Opening(_) => self.connect(),
            Conn::Shown | Conn::Open(_) | Conn::Gone => {}
        }
        self.try_devices();
        self.rearm();
    }

    /// Whether the VT is shown; a VT whose state cannot be read is taken
    /// as not shown, with a warning.
    fn shown(&self) -> bool {
        let Some((console, num)) = &self.tty else {
            return true;
        };
        match console.active() {
            Ok(active) => active == *num,
            Err(e) => {
                log::warn!(target: "seat", "cannot find the VT on screen: {e}");
                false
            }
        }
    }

    /// One try at the seat.
    fn connect(&mut self) {
        let tried = match self.conn {
            Conn::Opening(n) => n,
            _ => 0,
        };
        match self.manager.open(Rc::clone(&self.inbox)) {
            Ok(seat) => {
                self.conn = Conn::Open(seat);
                self.opened();
            }
            Err(e) if busy(&e, self.seatd) && tried.saturating_add(1) < self.tries.seat => {
                if tried == 0 {
                    log::info!(target: "seat", "the seat is busy, waiting for its previous client to let it go: {e}");
                }
                self.conn = Conn::Opening(tried.saturating_add(1));
            }
            Err(e) => {
                log::error!(target: "seat", "cannot open the seat after {} tries: {e}", tried.saturating_add(1));
                self.gone();
            }
        }
    }

    /// The seat opened: its descriptor in the loop, and anything libseat
    /// queued while opening dispatched now.
    fn opened(&mut self) {
        let Conn::Open(seat) = &mut self.conn else {
            return;
        };
        let name = seat.name();
        log::info!(target: "seat", "the seat {name} is open");
        self.name = Some(name);
        let watched = seat.fd().and_then(|fd| (self.hooks.watch)(fd));
        match watched {
            Ok(token) => self.watched = Some(token),
            Err(e) => {
                log::error!(target: "seat", "cannot watch the seat: {e}");
                self.gone();
                return;
            }
        }
        self.readable();
    }

    /// The seat's descriptor is readable: libseat dispatches, and what it
    /// was told is acted on. Returns whether to keep watching it.
    pub(crate) fn readable(&mut self) -> bool {
        let Conn::Open(seat) = &mut self.conn else {
            return false;
        };
        if let Err(e) = seat.dispatch() {
            log::error!(target: "seat", "the seat manager's connection failed: {e}");
            self.gone();
            return false;
        }
        loop {
            let told = self.inbox.borrow_mut().pop_front();
            match told {
                Some(Told::Enable) => self.enable(),
                Some(Told::Disable) => self.disable(),
                None => break,
            }
        }
        self.try_devices();
        self.rearm();
        true
    }

    fn enable(&mut self) {
        log::debug!(target: "seat", "the seat is enabled");
        match self.state {
            State::Inactive => {
                self.tty_graphics();
                self.state = State::Active;
                self.events.push(VtEvent::Acquired);
            }
            State::Releasing => self.enable_owed = true,
            State::Active => {}
        }
    }

    fn disable(&mut self) {
        log::debug!(target: "seat", "the seat is disabled");
        match self.state {
            State::Active => {
                self.state = State::Releasing;
                self.events.push(VtEvent::Release);
            }
            // A disable of a seat the owner holds nothing of is
            // acknowledged at once.
            State::Inactive => self.let_go(),
            State::Releasing => self.enable_owed = false,
        }
        // A device tried while the seat is enabled waits for it again.
        for p in self.pending.values_mut() {
            if matches!(p, Pending::Tries(_)) {
                *p = Pending::Seat;
            }
        }
    }

    /// The disable acknowledged, and the VT in text mode.
    fn let_go(&mut self) {
        if let Conn::Open(seat) = &mut self.conn
            && let Err(e) = seat.disable()
        {
            log::warn!(target: "seat", "cannot let the seat go: {e}");
        }
        self.tty_text();
        self.state = State::Inactive;
    }

    /// The seat could not open or is lost: reported as a hangup, and every
    /// pending device failed.
    fn gone(&mut self) {
        if let Some(token) = self.watched.take() {
            (self.hooks.unwatch)(token);
        }
        self.conn = Conn::Gone;
        self.events.push(VtEvent::Hangup);
        for (path, p) in &mut self.pending {
            if !matches!(p, Pending::Opened(..) | Pending::Failed(_)) {
                *p = Pending::Failed(io::Error::from(io::ErrorKind::NotConnected));
                self.settled.push(path.clone());
            }
        }
    }

    /// One try at each device waiting, while the seat is enabled.
    fn try_devices(&mut self) {
        if self.state != State::Active {
            return;
        }
        let Conn::Open(seat) = &mut self.conn else {
            return;
        };
        for (path, p) in &mut self.pending {
            let tried = match p {
                Pending::Seat => 0,
                Pending::Tries(n) => *n,
                Pending::Opened(..) | Pending::Failed(_) => continue,
            };
            let tried = tried.saturating_add(1);
            match seat.open_device(path) {
                Ok((device, fd)) => {
                    log::debug!(target: "seat", "opened {} after {tried} tries", path.display());
                    *p = Pending::Opened(device, fd);
                    self.settled.push(path.clone());
                }
                Err(e) if retryable(&e) && tried < self.tries.device => *p = Pending::Tries(tried),
                Err(e) => {
                    log::warn!(target: "seat", "cannot open {} through the seat manager after {tried} tries: {e}", path.display());
                    *p = Pending::Failed(e);
                    self.settled.push(path.clone());
                }
            }
        }
    }

    /// Sets the timer while the VT, the seat or a device waits.
    fn rearm(&mut self) {
        let waiting = matches!(self.conn, Conn::Shown | Conn::Opening(_))
            || self
                .pending
                .values()
                .any(|p| matches!(p, Pending::Tries(_)));
        if !waiting || self.armed {
            return;
        }
        match (self.hooks.arm)(self.tries.every) {
            Ok(()) => self.armed = true,
            Err(e) => {
                log::error!(target: "seat", "cannot set the seat's timer: {e}");
                self.gone();
            }
        }
    }

    /// Graphics mode and the keyboard off on the VT, which logind leaves
    /// on; seatd sets them too.
    fn tty_graphics(&self) {
        let Some((console, num)) = &self.tty else {
            return;
        };
        if let Err(e) = console.set_display_mode(DisplayMode::Graphics) {
            log::warn!(target: "seat", "cannot put VT {num} in graphics mode: {e}");
        }
        if let Err(e) = console.set_keyboard_mode(KbMode::Off) {
            log::warn!(target: "seat", "cannot turn VT {num}'s keyboard off: {e}");
        }
    }

    /// The keyboard mode found at the start and text mode on the VT.
    fn tty_text(&self) {
        let Some((console, num)) = &self.tty else {
            return;
        };
        if let Err(e) = console.set_keyboard_mode(self.kb_mode) {
            log::warn!(target: "seat", "cannot restore VT {num}'s keyboard mode: {e}");
        }
        if let Err(e) = console.set_display_mode(DisplayMode::Text) {
            log::warn!(target: "seat", "cannot put VT {num} back in text mode: {e}");
        }
    }

    fn seat_mut(&mut self) -> io::Result<&mut M::Seat> {
        match &mut self.conn {
            Conn::Open(seat) => Ok(seat),
            _ => Err(io::Error::from(io::ErrorKind::NotConnected)),
        }
    }
}

impl<M: Manager, C: Console> Vt for LibseatVt<M, C> {
    fn events(&mut self) -> std::vec::Drain<'_, VtEvent> {
        self.events.drain(..)
    }

    /// The disable acknowledged to the seat manager; an enable that came
    /// meanwhile is reported now.
    fn acknowledge_release(&mut self) -> io::Result<()> {
        if self.state != State::Releasing {
            return Ok(());
        }
        self.let_go();
        if std::mem::take(&mut self.enable_owed) {
            self.enable();
        }
        self.try_devices();
        self.rearm();
        Ok(())
    }

    /// libseat cannot refuse a disable, and the seat manager waits for its
    /// acknowledgement, so a refusal acknowledges it.
    fn refuse_release(&mut self) -> io::Result<()> {
        log::debug!(target: "seat", "the seat manager takes no refusal; letting the seat go");
        self.acknowledge_release()
    }

    /// The VT shown through the kernel, when the seat has one and it is not
    /// shown; the seat manager enables the seat once it is.
    fn activate(&mut self) -> io::Result<Switch> {
        if self.state == State::Active {
            return Ok(Switch::Done);
        }
        if let Some((console, num)) = &self.tty
            && console.active()? != *num
        {
            console.activate(*num).inspect_err(|e| {
                log::warn!(target: "seat", "cannot enter VT {num}: {e}");
            })?;
        }
        Ok(Switch::Pending)
    }

    /// A switch through the seat manager to the VT shown at the start.
    fn leave(&mut self) -> io::Result<Switch> {
        if self.state != State::Active {
            return Ok(Switch::Done);
        }
        let own = self.number();
        match self.saved {
            Some(saved) if Some(saved) != own => {
                self.seat_mut()?.switch_session(saved)?;
                log::debug!(target: "seat", "leaving for VT {saved}");
                Ok(Switch::Pending)
            }
            _ => Ok(Switch::Done),
        }
    }

    fn wait(&mut self) -> io::Result<Switch> {
        if matches!(self.conn, Conn::Gone) {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        }
        if self.state == State::Active {
            return Ok(Switch::Done);
        }
        log::info!(target: "seat", "waiting for the seat manager to give vtium the seat");
        Ok(Switch::Pending)
    }

    fn is_active(&self) -> bool {
        self.state == State::Active
    }

    /// Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys ask the seat manager
    /// for that session while the seat is vtium's; this VT's own key is
    /// taken and does nothing.
    fn key(&mut self, key: &KeyEvent) -> bool {
        let Some(target) = switch_target(key) else {
            return false;
        };
        if self.state != State::Active {
            return false;
        }
        if Some(target) != self.number() {
            log::debug!(target: "seat", "switching to session {target} on a key");
            if let Err(e) = self.seat_mut().and_then(|seat| seat.switch_session(target)) {
                log::warn!(target: "seat", "cannot switch to session {target}: {e}");
            }
        }
        true
    }

    fn is_own_key(&self, key: &KeyEvent) -> bool {
        self.number().is_some() && switch_target(key) == self.number() && self.is_active()
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.state == State::Active {
            self.tty_graphics();
        }
        Ok(())
    }

    fn bell(&mut self) {
        if let Some((console, num)) = &self.tty
            && let Err(e) = console.bell()
        {
            log::warn!(target: "seat", "cannot ring VT {num}'s bell: {e}");
        }
    }

    fn number(&self) -> Option<u16> {
        self.tty.as_ref().map(|(_, num)| *num)
    }

    fn seat(&self) -> &str {
        self.name.as_deref().unwrap_or("seat0")
    }

    fn open_device(&mut self, path: &Path) -> io::Result<OwnedFd> {
        if self.open.contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        match self.pending.remove(path) {
            Some(Pending::Opened(device, fd)) => {
                self.open.insert(path.to_owned(), device);
                return Ok(fd);
            }
            Some(Pending::Failed(e)) => return Err(e),
            Some(p) => {
                self.pending.insert(path.to_owned(), p);
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            None => {}
        }
        if matches!(self.conn, Conn::Gone) {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        }
        if self.state != State::Active {
            log::debug!(target: "seat", "{} waits for the seat", path.display());
            self.pending.insert(path.to_owned(), Pending::Seat);
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        match self.seat_mut()?.open_device(path) {
            Ok((device, fd)) => {
                self.open.insert(path.to_owned(), device);
                Ok(fd)
            }
            Err(e) if retryable(&e) => {
                log::debug!(target: "seat", "{} is not ready, trying again: {e}", path.display());
                self.pending.insert(path.to_owned(), Pending::Tries(1));
                self.rearm();
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
            Err(e) => Err(e),
        }
    }

    /// A device given to the owner, or opened by a retry and not yet
    /// taken, is closed through the seat manager; a pending open is
    /// dropped. With the seat gone, the device's own descriptor closes as it
    /// drops.
    fn close_device(&mut self, path: &Path) {
        self.settled.retain(|p| p != path);
        let device = match self.pending.remove(path) {
            Some(Pending::Opened(device, _)) => Some(device),
            _ => self.open.remove(path),
        };
        let Some(device) = device else {
            return;
        };
        if let Err(e) = self.seat_mut().and_then(|seat| seat.close_device(device)) {
            log::warn!(target: "seat", "cannot close {} through the seat manager: {e}", path.display());
        }
    }

    fn settled(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.settled)
    }
}

impl<M: Manager, C: Console> Drop for LibseatVt<M, C> {
    /// The devices closed, those opened by a retry and never taken
    /// included, the seat's descriptor out of the loop, the VT in
    /// text mode and the seat closed.
    fn drop(&mut self) {
        let paths: Vec<PathBuf> = self
            .open
            .keys()
            .chain(self.pending.keys())
            .cloned()
            .collect();
        for path in paths {
            self.close_device(&path);
        }
        if let Some(token) = self.watched.take() {
            (self.hooks.unwatch)(token);
        }
        if self.state != State::Inactive {
            self.tty_text();
        }
    }
}

/// libseat's answers while a seat is still being handed over from its
/// previous client. seatd 0.9.3 refuses a client while another holds the
/// seat by hanging up without an error (seatd/client.c, `handle_open_seat`),
/// which libseat answers `EPIPE` when `LIBSEAT_BACKEND` names seatd, and
/// otherwise `ENOSYS`, once its other backends have failed too
/// (libseat/libseat.c, `libseat_open_seat`); a read that finds the
/// connection already closed answers `EIO` (libseat/backend/seatd.c,
/// `poll_connection`). So while seatd listens those three mean
/// busy. `EBUSY` and `EBADF` are busy whoever answers them.
fn busy(e: &io::Error, seatd: bool) -> bool {
    errno(e).is_some_and(|n| {
        n == Errno::BUSY
            || n == Errno::BADF
            || (seatd && (n == Errno::NOSYS || n == Errno::PIPE || n == Errno::IO))
    })
}

/// The seat manager's answers for a device it is not ready to give yet.
fn retryable(e: &io::Error) -> bool {
    errno(e).is_some_and(|n| n == Errno::NODEV || n == Errno::ACCESS || n == Errno::BUSY)
}

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}
