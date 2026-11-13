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
//! Nothing here waits on the loop. vtium waits for the events that end a
//! wait where the kernel or the seat manager sends one. Where neither
//! sends one, vtium keeps the seat or the device pending and tries again
//! from a timer in the loop every [`crate::bounds::SEAT_RETRY_MS`], so
//! signals, timers and the other sources are served in between:
//!
//! - **The seat** is tried up to [`crate::bounds::SEAT_OPENS_MAX`] times
//!   while libseat's answer says the seat is still held by its previous
//!   client: `EBUSY` or `EBADF`, and while seatd listens also `ENOSYS`,
//!   `EPIPE` or `EIO`, since seatd refuses such a client by hanging up
//!   ([`busy`]). seatd sends nothing to a process that is not its client,
//!   so no event says when the previous client is gone. Past the tries, or
//!   on any other failure once vtium has chosen the seat, vtium logs an
//!   error and reports [`VtEvent::Hangup`].
//! - **A device** is tried up to [`crate::bounds::DEVICE_OPENS_MAX`] times
//!   while libseat fails with `EBUSY`, as logind's open of a card does
//!   while another process holds the card's DRM master. The kernel sends no
//!   event when that master is dropped. [`Vt::open_device`] answers
//!   [`io::ErrorKind::WouldBlock`] meanwhile, and [`Vt::settled`] names the
//!   device once it has opened or failed. `ENODEV` and `EACCES` are final
//!   ([`retryable`]). A device asked for before the seat is enabled, or
//!   refused `EPERM` while the seat manager disables vtium, waits for the
//!   next enable.
//!
//! seatd gives a new client the VT shown when it opens the seat. So when
//! vtium's VT is not shown at the start and seatd listens, vtium opens the
//! seat only once its VT is shown. vtium learns that from tty0's `active`
//! attribute, which the kernel notifies at each switch
//! ([`Console::watch_shown`]), and opens the seat at the loop's turn that
//! reads the notification. [`Vt::activate`] asks the kernel to show the VT.
//! logind takes the session from vtium's login session instead, so the
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

pub use binding::{Libseat, LibseatSeat};

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{Interest, Mode, PostAction, RegistrationToken};
use rustix::io::Errno;

use crate::event_loop::Loop;
use crate::input::{Evdev, EventDevice, KeyEvent, Open, Opening};
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
    /// The errors libseat reported while the last [`Manager::open`] ran,
    /// which it holds back so that vtium decides how to log them
    /// ([`report`]).
    fn said(&mut self) -> Vec<String>;
    /// Whether seatd listens on its socket, which decides whether a VT not
    /// shown must be waited for before the seat opens.
    fn seatd_listens(&self) -> bool;
    /// The backend `LIBSEAT_BACKEND` names, when it names one.
    fn backend_named(&self) -> Option<OsString>;
    /// Whether logind runs: its seats' directory exists.
    fn logind_runs(&self) -> bool;
    /// Whether vtium is in a logind session of its own, the session
    /// libseat's logind backend takes the seat from.
    fn own_session(&self) -> bool;
}

/// What vtium logs for an error that libseat, the library vtium opens the
/// seat through, reports in its own words, `text`.
pub(crate) fn libseat_error(text: &str) -> String {
    format!("libseat, the seat library, reports an error: {text}")
}

/// Logs the errors libseat reported while vtium opened the seat, `said`.
/// When the open ends as vtium expects, they are logged at the debug
/// level, and vtium's own line says what happened: the seat opened,
/// perhaps through the second seat manager libseat tried; the seat is busy
/// and vtium waits for it; or no seat manager runs, so vtium uses the
/// kernel's VT, as on every machine without seatd, where libseat reports
/// that seatd's socket is missing. Otherwise each is a warning, so a real
/// error stays in the log.
fn report(said: Vec<String>, expected: bool) {
    for text in said {
        if expected {
            log::debug!(target: "seat", "{}", libseat_error(&text));
        } else {
            log::warn!(target: "seat", "{}", libseat_error(&text));
        }
    }
}

/// What vtium logs when it leaves libseat's logind backend alone because
/// vtium is in no logind session of its own and `LIBSEAT_BACKEND` names
/// logind or no backend (ADR-1400 D3).
pub const NO_SESSION: &str = "vtium is in no logind session of its own, so it does not take the seat from logind, and it uses the kernel's VT";

/// What vtium logs when [`keep_from_logind`] pins libseat to seatd, as in
/// direct mode (ADR-1400 D3).
pub const SEATD_ONLY: &str = "vtium is in no logind session of its own, so it takes the seat from seatd alone, and never from logind";

/// Whether vtium sets `LIBSEAT_BACKEND=seatd` itself: when it is in no
/// logind session of its own and `named`, the backend `LIBSEAT_BACKEND`
/// names, is none. Then libseat would try logind whenever seatd refuses an
/// open (libseat/libseat.c lines 65-75 at seatd 92b1617c), and logind's
/// backend would take the user's display session.
pub(crate) fn pin_needed(named: Option<&OsStr>, own_session: bool) -> bool {
    named.is_none() && !own_session
}

/// Whether vtium pinned libseat to seatd at its start.
static PINNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Pins libseat to seatd when vtium is in no logind session of its own and
/// `LIBSEAT_BACKEND` names no backend ([`pin_needed`]), so that no open of
/// the seat, the first, a retry or one that waits for the VT to be shown,
/// reaches logind's backend. Call it once at the start, while vtium has
/// one thread, and after reading the environment that sessions inherit, so
/// that no login sees the variable. Returns whether it pinned.
pub fn keep_from_logind() -> bool {
    let manager = binding::Libseat::default();
    if !pin_needed(manager.backend_named().as_deref(), manager.own_session()) {
        return false;
    }
    #[expect(
        unsafe_code,
        reason = "setting LIBSEAT_BACKEND, which libseat reads from the environment"
    )]
    // SAFETY: the caller calls this while the process has one thread, so no
    // other thread reads or writes the environment meanwhile.
    unsafe {
        std::env::set_var("LIBSEAT_BACKEND", "seatd");
    }
    PINNED.store(true, std::sync::atomic::Ordering::Relaxed);
    log::info!(target: "seat", "{SEATD_ONLY}");
    true
}

/// Whether [`keep_from_logind`] pinned libseat to seatd.
pub fn pinned() -> bool {
    PINNED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the control groups `cgroup`, as `/proc/PID/cgroup` lists them,
/// put the process in a logind session's scope, `session-ID.scope`. sd-login
/// finds a process's session the same way: from the systemd unit of its
/// control group on the unified hierarchy (`0::`), or on systemd's own
/// named hierarchy (`name=systemd`) on a host that mounts both. The unit is
/// the first part of the path that is not a slice.
pub(crate) fn in_session_scope(cgroup: &str) -> bool {
    cgroup.lines().any(|line| {
        let mut fields = line.splitn(3, ':');
        let (_, controllers, path) = (fields.next(), fields.next(), fields.next());
        matches!(controllers, Some("" | "name=systemd"))
            && path
                .and_then(|p| {
                    p.split('/')
                        .find(|c| !c.is_empty() && !c.ends_with(".slice"))
                })
                .is_some_and(|unit| unit.starts_with("session-") && unit.ends_with(".scope"))
    })
}

/// Whether libseat opens the seat through its logind backend. libseat tries
/// only the backend `LIBSEAT_BACKEND` names, when it names one, and
/// otherwise seatd, then logind, then its built-in seat manager
/// (libseat/libseat.c at seatd 92b1617c, lines 18-28 and 45). seatd answers
/// when its socket accepts a connection, and logind when it runs.
pub(crate) fn logind_backend(named: Option<&OsStr>, seatd: bool, logind_runs: bool) -> bool {
    match named {
        Some(name) => name == "logind",
        None => !seatd && logind_runs,
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conn {
    /// Waiting for the VT to be shown before the seat opens.
    Shown,
    /// The seat manager answered busy this many times.
    Opening(usize),
    /// The seat is open, in [`Shared::seat`].
    Open,
    /// The seat could not open, or the seat manager went away.
    Gone,
}

/// What the VT shares with the keyboards and pointers it opens through the
/// seat ([`SeatInput`]).
struct Shared<S: Seat> {
    /// The seat, while it is open.
    seat: Option<S>,
    /// The seat is vtium's: enabled, with no release waiting.
    active: bool,
    /// The keyboards and pointers opened through the seat, by node.
    inputs: BTreeMap<PathBuf, S::Device>,
}

impl<S: Seat> Shared<S> {
    /// Closes every keyboard and pointer through the seat, then the seat.
    fn close(&mut self) {
        let inputs = std::mem::take(&mut self.inputs);
        for (path, device) in inputs {
            if let Some(seat) = self.seat.as_mut()
                && let Err(e) = seat.close_device(device)
            {
                log::warn!(target: "seat", "cannot close {} through the seat manager: {e}", path.display());
            }
        }
        self.active = false;
        self.seat = None;
    }
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
    conn: Conn,
    /// The seat, when it opened at once.
    seat: Option<S>,
    inbox: Inbox,
    /// seatd listens.
    seatd: bool,
    /// libseat's logind backend opens the seat.
    logind: bool,
}

/// What the VT asks of the loop it runs in, made by [`LibseatVt::start`]
/// with the loop's data type inside.
struct Hooks {
    /// Calls [`LibseatVt::tick`] once, after the time given.
    arm: Box<dyn Fn(Duration) -> io::Result<()>>,
    /// Watches the seat's descriptor, dispatching it when readable.
    watch: Box<dyn Fn(OwnedFd) -> io::Result<RegistrationToken>>,
    /// Watches tty0's `active`, calling [`LibseatVt::shown_notified`] at
    /// each notification of a switch.
    watch_shown: Box<dyn Fn(OwnedFd) -> io::Result<RegistrationToken>>,
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
    conn: Conn,
    /// The seat, shared with the keyboards and pointers opened through it.
    shared: Rc<RefCell<Shared<M::Seat>>>,
    inbox: Inbox,
    /// seatd listened at the start.
    seatd: bool,
    /// libseat's logind backend opens the seat, from vtium's own logind
    /// session ([`Vt::logind_seat`]).
    logind: bool,
    /// The VT the seat runs on, when it has VTs, and its number.
    tty: Option<(C, u16)>,
    /// The VT shown when [`Vt::activate`] asked the kernel for this one,
    /// which [`Vt::leave`] goes back to. `None` while vtium has not
    /// switched to its VT itself, as when it waits for the seat
    /// (`--no-switchvt`): then the VT shown at exit stays shown.
    back: Option<u16>,
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
    /// tty0's `active` in the loop, while the seat waits for the VT to be
    /// shown.
    shown_watched: Option<RegistrationToken>,
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
        LibseatVt::start_with(lp, Libseat::default(), tty, Tries::bounds(), access)
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
        let logind = logind_backend(
            manager.backend_named().as_deref(),
            seatd,
            manager.logind_runs(),
        );
        // Outside a session of its own, libseat's logind backend would take
        // the user's display session, such as root's login on another VT
        // (libseat/backend/logind.c, `get_display_session`). libseat reaches
        // that backend when LIBSEAT_BACKEND names it, and, when it names no
        // backend, whenever seatd refuses an open, as a busy seatd does. So
        // outside a session vtium opens the seat only through a backend
        // named otherwise, as keep_from_logind names seatd in direct mode.
        let named = manager.backend_named();
        let reaches_logind = named.as_deref().is_none_or(|n| n == "logind");
        if reaches_logind && !manager.own_session() {
            log::info!(target: "seat", "{NO_SESSION}");
            return Ok(Chosen::Kernel(tty));
        }
        let inbox = Inbox::default();
        let mut manager = manager;
        let (conn, seat) = if shown == Some(false) && seatd {
            log::info!(target: "seat", "seatd listens: the seat opens once the VT is shown");
            (Conn::Shown, None)
        } else {
            let opened = manager.open(Rc::clone(&inbox));
            let said = manager.said();
            match opened {
                Ok(seat) => {
                    report(said, true);
                    (Conn::Open, Some(seat))
                }
                Err(e) if busy(&e, seatd) => {
                    report(said, true);
                    log::info!(target: "seat", "the seat is busy, waiting for its previous client to let it go: {e}");
                    (Conn::Opening(1), None)
                }
                Err(e) => {
                    // No seat manager libseat reaches runs: seatd does not
                    // listen, and libseat would not try logind.
                    report(said, !seatd && !logind);
                    log::info!(target: "seat", "no seat manager answered, so vtium uses the kernel's VT: {e}");
                    return Ok(Chosen::Kernel(tty));
                }
            }
        };
        let found = Found {
            conn,
            seat,
            inbox,
            seatd,
            logind,
        };
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
        let (h, a) = (handle.clone(), access.clone());
        // Edge-triggered: sysfs reports the attribute readable at every
        // poll, and wakes its pollers at each notification.
        let watch_shown = Box::new(move |fd: OwnedFd| {
            let a = a.clone();
            h.insert_source(
                Generic::new(fd, Interest::READ, Mode::Edge),
                move |_, fd, data: &mut D| {
                    let keep = a(data).is_some_and(|vt| vt.shown_notified(fd.as_fd()));
                    Ok(if keep {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
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
        let kb_mode = match &tty {
            Some((console, num)) => match console.keyboard_mode() {
                Ok(KbMode::Off | KbMode::Raw | KbMode::MediumRaw) | Err(_) => {
                    log::debug!(target: "seat", "VT {num}'s keyboard mode is restored as K_UNICODE");
                    KbMode::Unicode
                }
                Ok(mode) => mode,
            },
            None => KbMode::Unicode,
        };
        let Found {
            conn,
            seat,
            inbox,
            seatd,
            logind,
        } = found;
        let mut vt = LibseatVt {
            manager,
            conn,
            shared: Rc::new(RefCell::new(Shared {
                seat,
                active: false,
                inputs: BTreeMap::new(),
            })),
            inbox,
            seatd,
            logind,
            tty,
            back: None,
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
                watch_shown,
                unwatch,
            },
            armed: false,
            watched: None,
            shown_watched: None,
        };
        match vt.conn {
            Conn::Open => vt.opened(),
            Conn::Shown => vt.watch_shown(),
            Conn::Opening(_) | Conn::Gone => {}
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
    /// The loop's timer: the seat and the devices tried again, and the
    /// timer set again while any still waits.
    pub(crate) fn tick(&mut self) {
        self.armed = false;
        match self.conn {
            Conn::Opening(_) => self.connect(),
            Conn::Shown | Conn::Open | Conn::Gone => {}
        }
        self.try_devices();
        self.rearm();
    }

    /// Puts tty0's `active` in the loop while the seat waits for the VT to
    /// be shown. The loop's first turn reads the attribute, so a switch
    /// made since the start is not missed. Without the watch vtium cannot
    /// learn when its VT is shown. Opening the seat anyway would give vtium
    /// the session of the VT shown, so vtium logs an error and hangs up.
    fn watch_shown(&mut self) {
        let fd = self
            .tty
            .as_ref()
            .and_then(|(console, _)| console.watch_shown())
            .unwrap_or_else(|| Err(io::Error::from(io::ErrorKind::Unsupported)));
        match fd.and_then(|fd| (self.hooks.watch_shown)(fd)) {
            Ok(token) => self.shown_watched = Some(token),
            Err(e) => {
                log::error!(target: "seat", "vtium cannot watch which VT is shown, so it cannot tell when seatd can give it the seat: {e}");
                self.gone();
            }
        }
    }

    /// tty0's `active` notified a switch. The VT shown is read, and once it
    /// is vtium's, vtium stops watching and opens the seat. Returns whether
    /// to keep watching.
    pub(crate) fn shown_notified(&mut self, watched: BorrowedFd<'_>) -> bool {
        let Some((console, num)) = self.tty.as_ref().filter(|_| self.conn == Conn::Shown) else {
            self.shown_watched = None;
            return false;
        };
        let num = *num;
        match console.shown(watched) {
            Ok(shown) if shown == num => {}
            Ok(_) => return true,
            Err(e) => {
                log::warn!(target: "seat", "cannot read the VT shown: {e}");
                return true;
            }
        }
        self.shown_watched = None;
        log::info!(target: "seat", "VT {num} is shown, so vtium opens the seat now");
        self.connect();
        self.try_devices();
        self.rearm();
        false
    }

    /// One try at the seat.
    fn connect(&mut self) {
        let tried = match self.conn {
            Conn::Opening(n) => n,
            _ => 0,
        };
        let opened = self.manager.open(Rc::clone(&self.inbox));
        let said = self.manager.said();
        match opened {
            Ok(seat) => {
                report(said, true);
                self.shared.borrow_mut().seat = Some(seat);
                self.conn = Conn::Open;
                self.opened();
            }
            Err(e) if busy(&e, self.seatd) && tried.saturating_add(1) < self.tries.seat => {
                report(said, true);
                if tried == 0 {
                    log::info!(target: "seat", "the seat is busy, waiting for its previous client to let it go: {e}");
                }
                self.conn = Conn::Opening(tried.saturating_add(1));
            }
            Err(e) => {
                report(said, false);
                log::error!(target: "seat", "cannot open the seat after {} tries: {e}", tried.saturating_add(1));
                self.gone();
            }
        }
    }

    /// The seat opened: its descriptor in the loop, and anything libseat
    /// queued while opening dispatched now.
    fn opened(&mut self) {
        let fd = {
            let mut shared = self.shared.borrow_mut();
            let Some(seat) = shared.seat.as_mut() else {
                return;
            };
            let name = seat.name();
            log::info!(target: "seat", "the seat {name} is open");
            self.name = Some(name);
            seat.fd()
        };
        let watched = fd.and_then(|fd| (self.hooks.watch)(fd));
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
        let dispatched = match self.shared.borrow_mut().seat.as_mut() {
            Some(seat) => seat.dispatch(),
            None => return false,
        };
        if let Err(e) = dispatched {
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
                self.set_state(State::Active);
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
                self.set_state(State::Releasing);
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
        let disabled = self.shared.borrow_mut().seat.as_mut().map(Seat::disable);
        if let Some(Err(e)) = disabled {
            log::warn!(target: "seat", "cannot let the seat go: {e}");
        }
        self.tty_text();
        self.set_state(State::Inactive);
    }

    /// Whether the seat is vtium's, which the keyboards and pointers opened
    /// through it read too.
    fn set_state(&mut self, state: State) {
        self.state = state;
        self.shared.borrow_mut().active = state == State::Active;
    }

    /// The seat could not open or is lost: reported as a hangup, and every
    /// pending device failed.
    fn gone(&mut self) {
        if let Some(token) = self.watched.take() {
            (self.hooks.unwatch)(token);
        }
        if let Some(token) = self.shown_watched.take() {
            (self.hooks.unwatch)(token);
        }
        self.conn = Conn::Gone;
        self.shared.borrow_mut().close();
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
        let mut shared = self.shared.borrow_mut();
        let Some(seat) = shared.seat.as_mut() else {
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
                // A disable the seat manager has begun and the loop has
                // not yet read: the device waits for the next enable.
                Err(e) if refused_while_disabling(&e) => *p = Pending::Seat,
                Err(e) if retryable(&e) && tried < self.tries.device => *p = Pending::Tries(tried),
                Err(e) => {
                    log::warn!(target: "seat", "cannot open {} through the seat manager after {tried} tries: {e}", path.display());
                    *p = Pending::Failed(e);
                    self.settled.push(path.clone());
                }
            }
        }
    }

    /// Sets the timer while the seat or a device waits for a retry.
    fn rearm(&mut self) {
        let waiting = matches!(self.conn, Conn::Opening(_))
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

    /// Asks the open seat with `ask`.
    fn with_seat<T>(&self, ask: impl FnOnce(&mut M::Seat) -> io::Result<T>) -> io::Result<T> {
        match self.shared.borrow_mut().seat.as_mut() {
            Some(seat) => ask(seat),
            None => Err(io::Error::from(io::ErrorKind::NotConnected)),
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
        if let Some((console, num)) = &self.tty {
            let shown = console.active()?;
            if shown != *num {
                console.activate(*num).inspect_err(|e| {
                    log::warn!(target: "seat", "cannot enter VT {num}: {e}");
                })?;
                self.back = Some(shown);
            }
        }
        Ok(Switch::Pending)
    }

    /// A switch through the seat manager to the VT shown when
    /// [`Vt::activate`] asked for this one; nothing when it never asked.
    fn leave(&mut self) -> io::Result<Switch> {
        if self.state != State::Active {
            return Ok(Switch::Done);
        }
        let own = self.number();
        match self.back {
            Some(back) if Some(back) != own => {
                self.with_seat(|seat| seat.switch_session(back))?;
                log::debug!(target: "seat", "leaving for VT {back}");
                Ok(Switch::Pending)
            }
            _ => Ok(Switch::Done),
        }
    }

    fn wait(&mut self) -> io::Result<Switch> {
        if self.conn == Conn::Gone {
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
            if let Err(e) = self.with_seat(|seat| seat.switch_session(target)) {
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

    fn logind_seat(&self) -> bool {
        self.logind
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
        if self.conn == Conn::Gone {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        }
        if self.state != State::Active {
            log::debug!(target: "seat", "{} waits for the seat", path.display());
            self.pending.insert(path.to_owned(), Pending::Seat);
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        match self.with_seat(|seat| seat.open_device(path)) {
            Ok((device, fd)) => {
                self.open.insert(path.to_owned(), device);
                Ok(fd)
            }
            Err(e) if refused_while_disabling(&e) => {
                log::debug!(target: "seat", "{} waits for the seat: {e}", path.display());
                self.pending.insert(path.to_owned(), Pending::Seat);
                Err(io::Error::from(io::ErrorKind::WouldBlock))
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
        if let Err(e) = self.with_seat(|seat| seat.close_device(device)) {
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
        if let Some(token) = self.shown_watched.take() {
            (self.hooks.unwatch)(token);
        }
        if self.state != State::Inactive {
            self.tty_text();
        }
        // The keyboards and pointers opened through the seat close with
        // it, and a later close of one of them closes only its own
        // descriptor.
        self.shared.borrow_mut().close();
    }
}

/// Makes the input device from the descriptor the seat manager gave for a
/// node: an [`Evdev`], or a test's stand-in.
type MakeDevice<E> = Box<dyn Fn(&Path, OwnedFd) -> io::Result<E>>;

/// The keyboards and pointers opened through the seat manager, for
/// [`crate::input::Input::open_through`] (ADR-1400's L2 layer). It shares
/// the seat with the [`LibseatVt`] it came from, and opens nothing itself:
///
/// - While the seat is vtium's, each device opens through the seat
///   manager, which gives vtium a descriptor of it; logind revokes that
///   descriptor when it takes the seat away, and seatd does too.
/// - A device refused `EBUSY` is tried again from the input's timer every
///   [`crate::bounds::SEAT_RETRY_MS`], up to
///   [`crate::bounds::DEVICE_OPENS_MAX`] tries, as a display device is.
/// - A device asked for while the seat is not vtium's, or refused `EPERM`
///   while the seat manager takes the seat away, waits for the input's
///   next wake-up, which follows the next enable.
/// - Once the seat is gone, a device cannot open.
#[expect(
    private_bounds,
    reason = "the seat is libseat's outside the crate, or the tests' stand-in"
)]
pub struct SeatInput<S: Seat = LibseatSeat, E = Evdev> {
    shared: Rc<RefCell<Shared<S>>>,
    make: MakeDevice<E>,
    /// The tries at each device that waits for the timer.
    tries: BTreeMap<PathBuf, usize>,
    limit: Tries,
}

impl<S: Seat, E> std::fmt::Debug for SeatInput<S, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeatInput")
            .field("tries", &self.tries)
            .finish_non_exhaustive()
    }
}

impl LibseatVt<Libseat, Tty> {
    /// The keyboards and pointers, opened through this seat from here on.
    pub fn input(&self) -> SeatInput {
        self.input_with(Box::new(|_, fd| Ok(Evdev::new(fd))))
    }
}

#[expect(
    private_bounds,
    reason = "the manager is Libseat and the console a Tty outside the crate"
)]
impl<M: Manager, C: Console> LibseatVt<M, C> {
    /// [`LibseatVt::input`], with each device made by `make`.
    pub(crate) fn input_with<E>(&self, make: MakeDevice<E>) -> SeatInput<M::Seat, E> {
        SeatInput {
            shared: Rc::clone(&self.shared),
            make,
            tries: BTreeMap::new(),
            limit: self.tries,
        }
    }
}

impl<S: Seat, E: EventDevice> Open for SeatInput<S, E> {
    type Device = E;

    fn open(&mut self, node: &Path) -> io::Result<Opening<E>> {
        let mut shared = self.shared.borrow_mut();
        if shared.inputs.contains_key(node) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        if !shared.active {
            self.tries.remove(node);
            return match shared.seat {
                Some(_) => Ok(Opening::Wake),
                None => Err(io::Error::from(io::ErrorKind::NotConnected)),
            };
        }
        let Some(seat) = shared.seat.as_mut() else {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        };
        let tried = self.tries.remove(node).unwrap_or(0).saturating_add(1);
        let (device, fd) = match seat.open_device(node) {
            Ok(opened) => opened,
            // A disable the seat manager has begun and the loop has not
            // yet read: the device waits for the next enable.
            Err(e) if refused_while_disabling(&e) => return Ok(Opening::Wake),
            Err(e) if retryable(&e) && tried < self.limit.device => {
                self.tries.insert(node.to_owned(), tried);
                return Ok(Opening::Retry);
            }
            Err(e) => {
                log::warn!(target: "seat", "cannot open {} through the seat manager after {tried} tries: {e}", node.display());
                return Err(e);
            }
        };
        // vtium reads a device until it has no more events, so the
        // descriptor must not block. It shares its open file with the seat
        // manager's.
        let made = rustix::fs::fcntl_getfl(&fd)
            .and_then(|flags| rustix::fs::fcntl_setfl(&fd, flags | rustix::fs::OFlags::NONBLOCK))
            .map_err(io::Error::from)
            .and_then(|()| (self.make)(node, fd));
        match made {
            Ok(dev) => {
                log::info!(target: "seat", "opened {} through the seat manager", node.display());
                shared.inputs.insert(node.to_owned(), device);
                Ok(Opening::Open(dev))
            }
            Err(e) => {
                let _ = seat.close_device(device);
                Err(e)
            }
        }
    }

    /// The device's own descriptor closes first, then the seat manager
    /// closes its own. With the seat gone, only the device's closes.
    fn close(&mut self, node: &Path, dev: E) {
        drop(dev);
        let mut shared = self.shared.borrow_mut();
        let Some(device) = shared.inputs.remove(node) else {
            return;
        };
        if let Some(seat) = shared.seat.as_mut()
            && let Err(e) = seat.close_device(device)
        {
            log::warn!(target: "seat", "cannot close {} through the seat manager: {e}", node.display());
        }
    }

    fn retry_every(&self) -> Duration {
        self.limit.every
    }

    /// The tries at a device that goes while its retry waits go with it.
    fn forget(&mut self, node: &Path) {
        self.tries.remove(node);
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

/// The seat manager's answer for a device it may give later: `EBUSY`, as
/// logind answers while another process holds a card's DRM master
/// (src/login/logind-session-device.c, `session_device_open`, systemd
/// v262). Both seat managers open a node as root, so neither answers
/// `EACCES` for a while and then opens. A card the kernel is still adding
/// answers `ENODEV`, and the device monitor reports a card only at the
/// event that finds it open ([`crate::udev`]). The kernel adds an input
/// device's character device before it sends the node's event
/// (drivers/input/evdev.c, `evdev_connect`), and logind processes a device
/// it has not seen yet when it is asked for one (`session_device_verify`).
/// So `ENODEV` from the seat manager means the device is gone, and udev
/// reports it removed.
fn retryable(e: &io::Error) -> bool {
    errno(e) == Some(Errno::BUSY)
}

/// The answer for a device asked of a client the seat manager has begun
/// to disable: seatd 0.9.3 opens devices only for an active client and
/// answers `EPERM` otherwise (seatd/seat.c, `seat_open_device`). The
/// disable reaches vtium at its next dispatch, and the device waits for
/// the enable after it.
fn refused_while_disabling(e: &io::Error) -> bool {
    errno(e) == Some(Errno::PERM)
}

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}
