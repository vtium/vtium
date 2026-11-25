//! The privilege split: a broker that keeps root, and a drawer that runs
//! everything else as the system account `vtium`.
//!
//! The broker starts the drawer, waits for the drawer to say it runs,
//! serves the drawer's requests, and stops it when systemd stops the unit.
//! A drawer that sends messages the broker refuses, or reads none of the
//! broker's answers, is not stopped for it: the broker ignores what it
//! refuses and never waits for the drawer.
//! When the drawer ends, the broker puts the VT back and exits with the
//! drawer's status. vtium splits only when it starts as root and the
//! account `vtium` exists. Otherwise it runs as one process, as it always
//! has.
//!
//! The split is off in every build until the broker also holds the
//! devices, the VT and the sessions. A drawer without root cannot open a
//! display today, so [`SPLIT`] stays `false`, and nothing a user runs
//! reaches the broker. The tests run the broker against stand-in drawers.
//!
//! References:
//! - ADR-1400 D4: the broker, the drawer, when vtium runs as one process,
//!   the drawer's setup, and its start, stop and exit.

// Determinism under hostile input (docs/architecture.md, "Determinism"):
// the broker runs as root and reads what the drawer sends, so nothing the
// drawer sends can panic it.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]

pub mod devices;
pub mod outbox;
pub mod refusals;
pub mod start;
pub mod watch;

// The process jobs: sessions, helpers, the power keys and the store.
pub mod helpers;
pub mod power;
pub mod sessions;
pub mod store;

pub use vtium_broker_protocol as protocol;

use std::ffi::OsString;
use std::io;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use vtium_platform::event_loop::{Loop, Signal};

use outbox::Outbox;
use protocol::{Errno, Got, Link, Order, Request};
use refusals::Refusals;
use start::{ACCOUNT, Account, DRAWER, Program, Started};
use watch::{FAILURE, Seen};

/// The internal switch that turns the privilege split on. It stays off
/// until the cards that move the devices and the VT, and the sessions and
/// helpers, into the broker have landed, and the card that installs the
/// account `vtium` turns it on. Until then vtium runs as one process on
/// every path a user can reach.
pub const SPLIT: bool = false;

/// How long the broker waits for the drawer's ready after it starts the
/// drawer. A drawer that is not ready by then is stopped, and the broker
/// exits with a failure.
pub const START_BOUND: Duration = Duration::from_millis(10_000);

/// How long the broker waits for the drawer to exit after it tells the
/// drawer to stop, or after the drawer closes its socket. A drawer still
/// running then gets `SIGKILL`. systemd's own stop timeout is 90 s by
/// default, well above it.
pub const STOP_BOUND: Duration = Duration::from_millis(5_000);

/// The broker's two bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub start: Duration,
    pub stop: Duration,
}

impl Default for Bounds {
    fn default() -> Bounds {
        Bounds {
            start: START_BOUND,
            stop: STOP_BOUND,
        }
    }
}

/// What decides whether vtium splits: whether it runs as root, and the
/// account the drawer runs as.
pub trait Facts {
    fn root(&self) -> bool;
    fn account(&self, name: &str) -> Option<Account>;
}

/// The running system.
struct System;

impl Facts for System {
    fn root(&self) -> bool {
        rustix::process::geteuid().is_root()
    }

    fn account(&self, name: &str) -> Option<Account> {
        start::lookup(name)
    }
}

/// Whether vtium splits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// vtium runs as one process, and logs the line, if any.
    OneProcess(Option<String>),
    /// vtium starts a broker and a drawer that runs as the account.
    Split(Account),
}

/// vtium splits only when it runs as root and the account `vtium` exists.
/// A vtium started without root has no privilege to drop, so it runs as
/// one process and says nothing. A vtium started as root without the
/// account also runs as one process, and logs one line that names the
/// account.
pub fn plan(facts: &dyn Facts) -> Plan {
    if !facts.root() {
        return Plan::OneProcess(None);
    }
    match facts.account(ACCOUNT) {
        Some(account) => Plan::Split(account),
        None => Plan::OneProcess(Some(format!(
            "The system account {ACCOUNT} does not exist, so vtium runs as one process, as root."
        ))),
    }
}

/// What the broker does for each of the drawer's requests other than
/// ready, which the broker's loop handles itself. `serve` sends its answer
/// through `out`, which queues the answer while the drawer's socket is
/// full.
pub trait Serve {
    fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()>;

    /// Runs once a turn of the broker's loop, at `now`: acts on what the
    /// loop saw outside the drawer's requests, such as the VT's switches,
    /// and sends what follows through `out`. Returns whether the drawer
    /// must stop, as when the VT hung up.
    fn turn(&mut self, now: Instant, out: &mut Outbox<'_>) -> bool {
        let _ = (now, out);
        false
    }

    /// When the loop must run [`Serve::turn`] again, at the latest.
    fn due(&self) -> Option<Instant> {
        None
    }

    /// Sends the orders that no request asked for, such as a session's
    /// exit, once each turn of the loop while the drawer's socket is open.
    /// It sends nothing while the queue of orders is full, and keeps what
    /// it holds for a later turn.
    fn tick(&mut self, _out: &mut Outbox<'_>) {}
}

/// The broker's requests as the broker serves them: the devices and the VT
/// through [`devices::Devices`], and the rest through [`Jobs`].
pub struct Served {
    pub devices: devices::Devices,
    pub jobs: Jobs,
}

impl Serve for Served {
    fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
        // The devices and the VT.
        if devices::Devices::serves(&request) {
            return self.devices.serve(request, now(), out);
        }
        self.jobs.serve(request, out)
    }

    fn tick(&mut self, out: &mut Outbox<'_>) {
        self.jobs.tick(out);
    }

    fn turn(&mut self, now: Instant, out: &mut Outbox<'_>) -> bool {
        self.devices.turn(now, out)
    }

    fn due(&self) -> Option<Instant> {
        self.devices.due()
    }
}

/// The broker before it serves any request: each request that waits for
/// an answer gets a refusal, and the others do nothing. The cards that
/// move the devices, the VT, the sessions and the helpers into the broker
/// replace it.
pub struct Unserved;

impl Serve for Unserved {
    fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
        let refused = Err(Errno(
            u16::try_from(libc::ENOSYS).unwrap_or(protocol::ERRNO_MAX),
        ));
        let answer = match request {
            Request::OpenDevice { device, .. } => Order::Device {
                device,
                opened: refused,
            },
            Request::TakeMaster { device } => Order::Master {
                device,
                taken: false,
            },
            Request::OpenSession { console, .. } => Order::Pty {
                console,
                opened: refused,
            },
            Request::StartHelper { helper, .. } => Order::Helper {
                helper,
                started: refused,
            },
            other => {
                log::debug!(target: "broker", "vtium's root process does not serve {other:?} yet");
                return Ok(());
            }
        };
        out.order(answer, None)
    }
}

/// The broker's jobs: each request goes to the job that serves it.
pub struct Jobs {
    pub processes: Processes,
}

impl Serve for Jobs {
    fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
        match request {
            // The process jobs: sessions, helpers and the power keys.
            Request::OpenSession { .. }
            | Request::CloseSession { .. }
            | Request::Winch { .. }
            | Request::StartHelper { .. }
            | Request::StopHelper { .. }
            | Request::Power { .. } => self.processes.serve(request, out),
            other => Unserved.serve(other, out),
        }
    }

    fn tick(&mut self, out: &mut Outbox<'_>) {
        self.processes.tick(out);
    }
}

/// The broker's process jobs: the sessions, the helpers, the power keys,
/// and `vtium blank` and `vtium wake`, which reach the broker as signals
/// because the broker holds the VT open.
///
/// References:
/// - ADR-1400 D4, "What the broker does": the sessions, the helpers, the
///   power keys, reaping, the exits reported, and `vtium blank` and
///   `vtium wake` passed on.
pub struct Processes {
    pub sessions: sessions::Sessions,
    pub helpers: helpers::Helpers,
    pub power: power::PowerKeys,
    /// The orders no request asked for, in the order they came: the exits
    /// of sessions and helpers, and blank and wake. They are at most as
    /// many as the sessions and helpers that can end, and one blank or
    /// wake.
    unasked: std::collections::VecDeque<Order>,
    /// The blanks and wakes that signals asked for since the last turn.
    signalled: std::rc::Rc<std::cell::RefCell<Vec<vtium::blank::Order>>>,
}

impl Processes {
    pub fn new(
        sessions: sessions::Sessions,
        helpers: helpers::Helpers,
        power: power::PowerKeys,
    ) -> Processes {
        Processes {
            sessions,
            helpers,
            power,
            unasked: std::collections::VecDeque::new(),
            signalled: std::rc::Rc::default(),
        }
    }

    /// Takes `SIGRTMIN+1` and `SIGRTMIN+2`, which `vtium blank` and
    /// `vtium wake` send to the process that holds the VT, through `lp`,
    /// and passes each on to the drawer as [`Order::Blank`] or
    /// [`Order::Wake`].
    pub fn listen(&self, lp: &Loop<Seen>) -> io::Result<()> {
        let signalled = std::rc::Rc::clone(&self.signalled);
        vtium::blank::listen_with(&lp.handle(), move |order, _: &mut Seen| {
            signalled.borrow_mut().push(order);
        })
    }

    fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
        match request {
            Request::OpenSession {
                console,
                grid,
                display,
                truecolor,
            } => match self.sessions.open(&console, &grid, display, truecolor) {
                Ok(master) => out.order(
                    Order::Pty {
                        console,
                        opened: Ok(()),
                    },
                    Some(master),
                ),
                Err(errno) => out.order(
                    Order::Pty {
                        console,
                        opened: Err(errno),
                    },
                    None,
                ),
            },
            Request::CloseSession { console } => {
                self.sessions.close(&console);
                Ok(())
            }
            Request::Winch { console } => {
                self.sessions.winch(&console);
                Ok(())
            }
            Request::StartHelper { helper, kind } => {
                match self.helpers.start(helper, &kind, &self.sessions) {
                    Ok(socket) => out.order(
                        Order::Helper {
                            helper,
                            started: Ok(()),
                        },
                        Some(socket),
                    ),
                    Err(errno) => out.order(
                        Order::Helper {
                            helper,
                            started: Err(errno),
                        },
                        None,
                    ),
                }
            }
            Request::StopHelper { helper } => {
                self.helpers.stop(helper);
                Ok(())
            }
            Request::Power { key } => {
                self.power.run(key);
                Ok(())
            }
            other => Unserved.serve(other, out),
        }
    }

    fn tick(&mut self, out: &mut Outbox<'_>) {
        for (console, status) in self.sessions.reap() {
            self.unasked
                .push_back(Order::SessionExit { console, status });
        }
        for (helper, status) in self.helpers.reap() {
            self.unasked.push_back(Order::HelperExit { helper, status });
        }
        // Of several blanks and wakes still unsent, the last holds, as it
        // does for vtium in one process, so a flood of signals while the
        // drawer reads nothing keeps one order waiting.
        if let Some(order) = self.signalled.borrow_mut().drain(..).next_back() {
            self.unasked
                .retain(|o| !matches!(o, Order::Blank | Order::Wake));
            self.unasked.push_back(match order {
                vtium::blank::Order::Blank => Order::Blank,
                vtium::blank::Order::Wake => Order::Wake,
            });
        }
        while !out.full()
            && let Some(order) = self.unasked.pop_front()
        {
            if let Err(e) = out.order(order, None) {
                log::warn!(target: "broker", "vtium's root process cannot tell its drawing process: {e}");
            }
        }
    }
}

/// What the root process reads and sets up for its process jobs before
/// it starts the drawer.
///
/// References:
/// - ADR-1400 D4, "vtium.conf under the split": the broker reads the login
///   command, the power keys and the closed policy from vtium.conf itself.
/// - ADR-1401 D9: the closed policy's marker, written before the loop and
///   the VT.
/// - ADR-1401 D10: the VT's run directory.
/// - ADR-1401 D13: the keeper of the pty masters.
pub struct Begun {
    loaded: vtium::config::Loaded,
    vt: Option<u16>,
    env: Vec<(OsString, OsString)>,
    store: store::Store,
    /// The orders that pass the drawer `/dev/kmsg` and the kernel's font,
    /// each with its descriptor.
    kernel: Vec<(Order, std::os::fd::OwnedFd)>,
}

impl Begun {
    /// Reads vtium.conf through its ownership check, finds the VT, writes
    /// or removes the closed policy's marker, makes the VT's run directory
    /// and starts the keeper of the pty masters.
    pub fn read(args: &[OsString]) -> Begun {
        use std::path::Path;
        let loaded = vtium::config::load(args, &mut vtium::config::read_file);
        // Only the VT's path is read from the start's settings.
        let tty = vtium::start::Start::new(&loaded.config, String::new())
            .vt
            .or_else(|| vtium_platform::vt::Tty::find().ok().flatten());
        let vt = tty.as_deref().and_then(vtium::getty::vt_number);
        let dir = Path::new(vtium::getty::DIR);
        let policy = vtium::head::rules(&loaded.path, &loaded.file)
            .on_vt(vt)
            .outcome
            .policy;
        vtium::getty::mark(dir, tty.as_deref(), policy);
        if let Some(vt) = vt
            && let Err(e) = vtium::survive::hold::run_dir(&dir.join(format!("tty{vt}")))
        {
            log::warn!(target: "broker", "vtium cannot make its run directory for tty{vt}: {e}");
        }
        let mut kernel = Vec::new();
        match vtium::kernel::open_kmsg() {
            Ok(kmsg) => kernel.push((Order::Kmsg, kmsg)),
            Err(e) => log::info!(
                target: "broker",
                "vtium cannot read the kernel's messages from {}: {e}. It does not show them on screen.",
                vtium::kernel::KMSG
            ),
        }
        if let Some(font) = tty.as_deref().and_then(kernel_font) {
            kernel.push(font);
        }
        let env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let (keeper, kept) = vtium::survive::start(
            &env,
            tty.as_deref(),
            dir,
            Path::new("/proc/self/exe"),
            vtium_platform::take_passed,
        );
        Begun {
            loaded,
            vt,
            env,
            store: store::Store::new(keeper, kept),
            kernel,
        }
    }

    /// The orders that pass the drawer `/dev/kmsg` and the kernel's font,
    /// which the broker sends first, before any answer.
    pub fn kernel(&mut self) -> Vec<(Order, std::os::fd::OwnedFd)> {
        std::mem::take(&mut self.kernel)
    }

    /// The process jobs, whose processes `lp` watches.
    pub fn processes(self, lp: &Loop<Seen>) -> Processes {
        let wake = |lp: &Loop<Seen>| -> sessions::Wake {
            let handle = lp.handle();
            Box::new(move |pidfd| sessions::wake_at_exit(&handle, pidfd))
        };
        let config = &self.loaded.config;
        let font = match vtium::helper::Launcher::configured(config) {
            Ok(launcher) => Some(launcher.with_locale(&vtium::app::locale(&self.env))),
            Err(why) => {
                log::warn!(target: "broker", "{why}");
                None
            }
        };
        #[cfg(feature = "images")]
        let images = match helpers::Images::configured(config, &self.env) {
            Ok(images) => Some(images),
            Err(why) => {
                log::warn!(target: "broker", "{why}; vtium's root process starts no image helper");
                None
            }
        };
        #[cfg(not(feature = "images"))]
        let images = None;
        let power =
            power::PowerKeys::new(config, Box::new(vtium::power::System), Some(lp.handle()));
        let conf = sessions::Conf::read(&self.loaded, self.vt, self.env);
        Processes::new(
            sessions::Sessions::new(conf, Box::new(self.store), wake(lp)),
            helpers::Helpers::new(font, images, wake(lp)),
            power,
        )
    }
}

/// The kernel's font on the VT at `tty`, read while the VT is still in
/// text mode, as the order that passes it and its sealed file. A VT in
/// graphics mode, or a font past the protocol's bounds, gives none, logged
/// at debug.
///
/// References:
/// - ADR-1200 D8: the kernel's font, read once at start, which a frit's
///   `console:` source draws from.
fn kernel_font(tty: &std::path::Path) -> Option<(Order, std::os::fd::OwnedFd)> {
    use vtium_platform::vt::font::{ConsoleTty, read};
    let font = ConsoleTty::open(tty)
        .map_err(|e| e.to_string())
        .and_then(|t| read(&t).map_err(|e| e.to_string()));
    let font = match font {
        Ok(font) => font,
        Err(e) => {
            log::debug!(target: "broker", "the kernel's font is not read from {}: {e}", tty.display());
            return None;
        }
    };
    let pairs: Vec<(u16, u16)> = font.unimap.iter().map(|p| (p.unicode, p.fontpos)).collect();
    match protocol::KernelFont::file(font.width, font.height, &font.data, &pairs) {
        Ok(Some((shape, file))) => Some((Order::Font(shape), file)),
        Ok(None) => {
            log::debug!(target: "broker", "the kernel's font is past what vtium's drawing process takes");
            None
        }
        Err(e) => {
            log::debug!(target: "broker", "the kernel's font cannot be passed on: {e}");
            None
        }
    }
}

/// When the drawer is running as the drawer, its first step: it takes
/// [`DRAWER`] off the front of `args`, so the rest of vtium reads the
/// broker's own arguments, and returns its link to the broker. Any other
/// run returns `None` and leaves `args` as they are.
pub fn drawer(args: &mut Vec<OsString>) -> io::Result<Option<Link>> {
    if args.first().is_none_or(|a| a != DRAWER) {
        return Ok(None);
    }
    args.remove(0);
    start::begin().map(Some)
}

/// Splits vtium when [`plan`] says so: starts the drawer, runs the broker
/// until the drawer ends, and returns the broker's exit status. Returns
/// `None` when vtium runs as one process.
pub fn split(args: &[OsString]) -> Option<u8> {
    let account = match plan(&System) {
        Plan::OneProcess(line) => {
            if let Some(line) = line {
                log::warn!(target: "broker", "{line}");
            }
            return None;
        }
        Plan::Split(account) => account,
    };
    Some(match broker(args, &account) {
        Ok(status) => status,
        Err(e) => {
            log::error!(target: "broker", "vtium cannot start its drawing process: {e}");
            FAILURE
        }
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "the broker's loop reads the clock and passes it in (docs/architecture.md, Determinism)"
)]
fn now() -> Instant {
    Instant::now()
}

fn broker(args: &[OsString], account: &Account) -> io::Result<u8> {
    // Before the loop blocks any signal, since a holder of the pty masters
    // started here inherits the mask.
    let mut begun = Begun::read(args);
    let mut lp: Loop<Seen> = Loop::new()?;
    for signal in [Signal::SIGTERM, Signal::SIGINT] {
        lp.on_signal(signal, |seen: &mut Seen| seen.stop = true)?;
    }
    // A write to a drawer gone returns an error in place of ending the
    // broker.
    lp.on_signal(Signal::SIGPIPE, |_| {})?;
    let env = begun.env.clone();
    let kernel = begun.kernel();
    let processes = begun.processes(&lp);
    processes.listen(&lp)?;
    // The broker holds the VT before it says it is ready and before the
    // drawer starts, and puts the VT back after any end of the drawer.
    let mut served = Served {
        devices: devices::Devices::open(&mut lp, &devices::vt_path(args)?)?,
        jobs: Jobs { processes },
    };
    let mut restore = served.devices.give_back();
    let (link, theirs) = Link::pair()?;
    let arg0 = std::env::args_os()
        .next()
        .unwrap_or_else(|| OsString::from("vtium"));
    // systemd starts the display manager only once vtium holds its VT, so
    // the broker says it is ready once it holds the VT open, and before
    // the drawer starts.
    if let Err(e) = vtium::survive::notify::ready(&env) {
        log::warn!(target: "broker", "vtium cannot tell systemd that it is ready: {e}");
    }
    let drawer = start::spawn(&Program::vtium(arg0, args), account, theirs, true)?;
    // The socket is empty, so these first orders fit in it.
    for (order, fd) in &kernel {
        if let Err(e) = link.order(order, Some(fd.as_fd())) {
            log::warn!(target: "broker", "vtium cannot pass its drawing process {order:?}: {e}");
        }
    }
    log::info!(
        target: "broker",
        "vtium runs as two processes: this root process, and drawing process {}, which runs everything else as the account {ACCOUNT}",
        drawer.pid.as_raw_nonzero()
    );
    Ok(run(
        &mut lp,
        &link,
        &drawer,
        &mut served,
        &mut restore,
        Bounds::default(),
        &now,
    ))
}

/// Runs the broker's loop for the started `drawer` until the drawer ends,
/// and returns the broker's exit status.
///
/// - The drawer has [`Bounds::start`] to say it runs. A drawer that exits
///   first, or is not ready by then, is a failed start: the broker stops
///   it, puts the VT back and exits with a failure.
/// - The broker serves each other request through `serve`.
/// - The broker ignores each message that the protocol's reader refuses,
///   answers it with an error when its kind waits for an answer, and goes
///   on serving the drawer. [`refusals`] keeps the log of such messages
///   short.
/// - The broker never waits for room in the drawer's socket. [`outbox`]
///   holds the orders the socket cannot take yet, and the broker reads no
///   request while that queue is full.
/// - When something asks the broker to stop, it sends the drawer
///   [`Order::Stop`] and waits [`Bounds::stop`] for the drawer to exit.
/// - When the drawer closes its socket, the broker puts the VT back at
///   once, and waits the same bound for the drawer to exit.
/// - A drawer still running at the end of the stop bound gets `SIGKILL`.
///
/// `restore` puts the VT back. The broker calls it exactly once, after
/// any end of the drawer. Once the drawer has said it runs, the broker
/// exits with the drawer's own status, or with a failure when a signal
/// killed the drawer.
pub fn run(
    lp: &mut Loop<Seen>,
    link: &Link,
    drawer: &Started,
    serve: &mut dyn Serve,
    restore: &mut dyn FnMut(),
    bounds: Bounds,
    clock: &dyn Fn() -> Instant,
) -> u8 {
    let mut refusals = Refusals::default();
    let status = turns(
        lp,
        link,
        drawer,
        serve,
        restore,
        bounds,
        clock,
        &mut refusals,
    );
    if let Some(line) = refusals.end() {
        log::warn!(target: "broker", "{line}");
    }
    status
}

/// The turns of [`run`]'s loop, which note each refused message in
/// `refusals`.
#[expect(
    clippy::too_many_arguments,
    reason = "run's arguments and the log of refused messages that run writes out at the end"
)]
fn turns(
    lp: &mut Loop<Seen>,
    link: &Link,
    drawer: &Started,
    serve: &mut dyn Serve,
    restore: &mut dyn FnMut(),
    bounds: Bounds,
    clock: &dyn Fn() -> Instant,
    refusals: &mut Refusals,
) -> u8 {
    let pidfd = drawer.pidfd.as_fd();
    let mut back = Restore {
        restore,
        done: false,
    };
    let mut out = Outbox::new(link);
    let mut socket = match watch::watch(lp, link.as_fd(), pidfd) {
        Ok(watched) => Some(watched),
        Err(e) => {
            log::error!(target: "broker", "vtium cannot watch its drawing process, so vtium stops it: {e}");
            watch::kill(pidfd);
            back.run();
            return FAILURE;
        }
    };
    let started = clock();
    let ready_by = started.checked_add(bounds.start).unwrap_or(started);
    let mut ready = false;
    // set once the start fails, whatever the drawer's own status
    let mut failed = false;
    let mut stop_by: Option<Instant> = None;
    let stop_at = |now: Instant| Some(now.checked_add(bounds.stop).unwrap_or(now));
    let status = |ended: watch::Ended, ready: bool, failed: bool| {
        if ready && !failed {
            ended.status()
        } else {
            FAILURE
        }
    };
    loop {
        // What the loop saw besides the drawer's requests, such as the VT's
        // switches, before the loop waits again; and whether that asks the
        // drawer to stop.
        let stop_asked = serve.turn(clock(), &mut out);
        // While the queue of orders is full, the broker reads no requests
        // and waits for room; while any order waits, it watches for room.
        if let Some(watched) = socket.as_mut()
            && let Err(e) = watched.want(lp, !out.full(), out.waiting())
        {
            log::error!(target: "broker", "vtium cannot watch its drawing process, so vtium stops it: {e}");
            watch::kill(pidfd);
            back.run();
            return FAILURE;
        }
        let mut seen = Seen {
            stop: stop_asked,
            ..Seen::default()
        };
        let now = clock();
        let deadline = [
            stop_by,
            (!ready).then_some(ready_by),
            refusals.due(),
            serve.due(),
        ]
        .into_iter()
        .flatten()
        .min();
        if let Err(e) = lp.dispatch(
            deadline.map(|d| d.saturating_duration_since(now)),
            &mut seen,
        ) {
            log::error!(target: "broker", "vtium's root process cannot wait for events, so vtium stops its drawing process: {e}");
            watch::kill(pidfd);
            back.run();
            return FAILURE;
        }
        if seen.room
            && let Err(e) = out.flush()
        {
            log::warn!(target: "broker", "vtium's root process cannot send to its drawing process: {e}");
        }
        // The socket first, so a ready that the drawer sent before it
        // exited counts.
        if (seen.socket || seen.exited) && socket.is_some() {
            while !out.full() {
                match link.next_request() {
                    Ok(Got::Message(Ok(Request::Ready))) => ready = true,
                    Ok(Got::Message(Ok(request))) => {
                        if let Err(e) = serve.serve(request, &mut out) {
                            log::warn!(target: "broker", "vtium's root process cannot answer its drawing process: {e}");
                        }
                    }
                    Ok(Got::Message(Err(refused))) => {
                        if let Some(line) = refusals.note(refused.why, clock()) {
                            log::warn!(target: "broker", "{line}");
                        }
                        if let Some(answer) = refused.answer
                            && let Err(e) = out.order(answer, None)
                        {
                            log::warn!(target: "broker", "vtium's root process cannot answer its drawing process: {e}");
                        }
                    }
                    Ok(Got::Nothing) => break,
                    Ok(Got::Closed) | Err(_) => {
                        if let Some(watched) = socket.take() {
                            watched.unwatch(lp);
                        }
                        // The drawer has let go of the socket, and with
                        // it the VT, so the VT comes back now.
                        back.run();
                        stop_by = stop_by.or(stop_at(clock()));
                        break;
                    }
                }
            }
        }
        if socket.is_some() {
            serve.tick(&mut out);
        }
        if (seen.exited || socket.is_none())
            && let Ok(Some(ended)) = watch::ended(pidfd)
        {
            back.run();
            return status(ended, ready, failed);
        }
        // A stop that finds the drawer's socket full waits in the queue,
        // and the stop bound ends a drawer that never reads it.
        if seen.stop && stop_by.is_none() {
            let _ = out.stop();
            stop_by = stop_at(clock());
        }
        if !ready && stop_by.is_none() && clock() >= ready_by {
            log::error!(
                target: "broker",
                "vtium's drawing process did not start within {} ms, so vtium stops it",
                bounds.start.as_millis()
            );
            failed = true;
            let _ = out.stop();
            stop_by = stop_at(clock());
        }
        if let Some(by) = stop_by
            && clock() >= by
        {
            log::warn!(
                target: "broker",
                "vtium's drawing process did not stop within {} ms, so vtium kills it",
                bounds.stop.as_millis()
            );
            let ended = watch::kill(pidfd);
            back.run();
            return status(ended, ready, failed);
        }
        if let Some(line) = refusals.tick(clock()) {
            log::warn!(target: "broker", "{line}");
        }
    }
}

/// The VT's restore, run at most once.
struct Restore<'a> {
    restore: &'a mut dyn FnMut(),
    done: bool,
}

impl Restore<'_> {
    fn run(&mut self) {
        if !self.done {
            self.done = true;
            (self.restore)();
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    //! The broker's loop against stand-in drawers. Each stand-in is this
    //! test binary, started through the drawer's real start and setup as
    //! the user running the tests, which runs only [`fake_drawer`]; the
    //! variable `VTIUM_FAKE_DRAWER` says how the stand-in behaves.

    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use vtium_platform::calloop::ping::{Ping, make_ping};

    const MODE: &str = "VTIUM_FAKE_DRAWER";

    /// The named pipe through which a test tells the stand-in `backlog`
    /// to start reading its orders.
    const FIFO: &str = "VTIUM_FAKE_DRAWER_FIFO";

    /// How many requests the stand-in `backlog` sends before it reads.
    const BACKLOG: u32 = 2_000;

    /// A request to open a device, which [`Unserved`] answers.
    fn open_device(device: u32) -> Request {
        Request::OpenDevice {
            device,
            path: protocol::DevicePath::new("/dev/dri/card0").unwrap(),
        }
    }

    /// A request to switch to VT 0, which the reader refuses.
    fn refused_vt() -> [u8; protocol::SIZE] {
        let mut bytes = Request::SwitchVt { vt: 1 }.encode();
        bytes[4] = 0;
        bytes
    }

    /// Bounds long enough that no test meets them unless it sets out to.
    const LONG: Bounds = Bounds {
        start: Duration::from_secs(60),
        stop: Duration::from_secs(60),
    };

    /// The user running the tests, as the drawer's account, so the real
    /// setup runs without root.
    fn me() -> Account {
        Account {
            uid: rustix::process::getuid().as_raw(),
            gid: rustix::process::getgid().as_raw(),
            groups: None,
        }
    }

    /// This test binary as a stand-in drawer that behaves as `mode` says.
    fn stand_in(mode: &str) -> Program {
        Program {
            path: std::env::current_exe().unwrap(),
            arg0: "vtium".into(),
            args: [
                "--exact",
                "broker::tests::fake_drawer",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
                "-q",
            ]
            .map(OsString::from)
            .to_vec(),
            env: vec![(MODE.into(), mode.into())],
        }
    }

    /// Serves the stand-ins' requests: a power key asks the broker to
    /// stop, the way systemd's SIGTERM does, so a test stops the broker
    /// once its drawer is ready and without a timer. Every other request
    /// gets the answer [`Unserved`] gives it.
    struct StopOnPower(Ping);

    impl Serve for StopOnPower {
        fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
            if matches!(request, Request::Power { .. }) {
                self.0.ping();
                return Ok(());
            }
            Unserved.serve(request, out)
        }
    }

    /// Asks the broker to stop once the drawer's orders wait in the
    /// broker's queue because the drawer reads none of them, and serves
    /// every request as [`StopOnPower`] does until then.
    struct StopWhenFull(Ping);

    impl Serve for StopWhenFull {
        fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
            Unserved.serve(request, out)?;
            if out.full() {
                self.0.ping();
            }
            Ok(())
        }
    }

    /// Tells the stand-in, through the named pipe at `fifo`, to start
    /// reading its orders once they wait in the broker's queue, and
    /// otherwise serves as [`StopOnPower`] does.
    struct ReadAgainWhenFull {
        stop: StopOnPower,
        fifo: std::path::PathBuf,
        told: Rc<Cell<bool>>,
    }

    impl Serve for ReadAgainWhenFull {
        fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
            let served = self.stop.serve(request, out);
            if out.full() && !self.told.get() {
                use std::io::Write;
                self.told.set(true);
                let mut fifo = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&self.fifo)
                    .unwrap();
                fifo.write_all(b"r").unwrap();
            }
            served
        }
    }

    /// What one run of the broker gave.
    #[derive(Debug, PartialEq, Eq)]
    struct Ran {
        /// The broker's exit status.
        status: u8,
        /// How often the broker put the VT back.
        restored: u32,
        /// Whether the drawer was still running when the broker first put
        /// the VT back.
        running: Option<bool>,
        /// Whether the broker's loop had seen a request to stop when it
        /// first put the VT back.
        asked_to_stop: Option<bool>,
    }

    /// A run of the broker against the stand-in `mode`: its exit status,
    /// and how often it put the VT back.
    fn broker(mode: &str, bounds: Bounds) -> (u8, u32) {
        let (status, restored, _) = broker_watching(mode, bounds);
        (status, restored)
    }

    /// As [`broker`], and also whether the drawer was still running when
    /// the broker first put the VT back.
    fn broker_watching(mode: &str, bounds: Bounds) -> (u8, u32, Option<bool>) {
        let ran = broker_serving(mode, &[], bounds, |ping, _| Box::new(StopOnPower(ping)));
        (ran.status, ran.restored, ran.running)
    }

    /// A run of the broker against the stand-in `mode`, with `env` added
    /// to the stand-in's environment, serving through what `serve` makes
    /// from the ping that asks the broker to stop.
    fn broker_serving(
        mode: &str,
        env: &[(&str, &std::path::Path)],
        bounds: Bounds,
        serve: impl FnOnce(Ping, &Started) -> Box<dyn Serve>,
    ) -> Ran {
        let mut lp: Loop<Seen> = Loop::new().unwrap();
        let (ping, source) = make_ping().unwrap();
        let asked = Rc::new(Cell::new(false));
        let asking = Rc::clone(&asked);
        lp.handle()
            .insert_source(source, move |_, _, seen: &mut Seen| {
                asking.set(true);
                seen.stop = true;
            })
            .unwrap();
        let (link, theirs) = Link::pair().unwrap();
        let mut program = stand_in(mode);
        program.env.extend(
            env.iter()
                .map(|(k, v)| (OsString::from(k), v.as_os_str().to_owned())),
        );
        let drawer = start::spawn(&program, &me(), theirs, false).unwrap();
        let mut serve = serve(ping, &drawer);
        let pidfd = drawer.pidfd.try_clone().unwrap();
        let restored = Rc::new(Cell::new(0));
        let running = Rc::new(Cell::new(None));
        let asked_first = Rc::new(Cell::new(None));
        let (count, first, stop_first) = (
            Rc::clone(&restored),
            Rc::clone(&running),
            Rc::clone(&asked_first),
        );
        let mut restore = move || {
            count.set(count.get() + 1);
            if first.get().is_none() {
                // waits for nothing and reaps nothing
                let options = rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT;
                let exited =
                    rustix::process::waitid(rustix::process::WaitId::PidFd(pidfd.as_fd()), options);
                first.set(Some(matches!(exited, Ok(None))));
                stop_first.set(Some(asked.get()));
            }
        };
        let status = run(
            &mut lp,
            &link,
            &drawer,
            serve.as_mut(),
            &mut restore,
            bounds,
            &now,
        );
        Ran {
            status,
            restored: restored.get(),
            running: running.get(),
            asked_to_stop: asked_first.get(),
        }
    }

    /// The log lines this thread wrote since the last call. The tests of
    /// the binary that read the log share this one logger, since a process
    /// sets its logger once.
    fn logged() -> Vec<String> {
        use std::cell::RefCell;
        use std::sync::Once;
        thread_local! {
            static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        }
        struct Capture;
        impl log::Log for Capture {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, record: &log::Record<'_>) {
                LINES.with(|l| l.borrow_mut().push(record.args().to_string()));
            }
            fn flush(&self) {}
        }
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            log::set_logger(&Capture).unwrap();
            log::set_max_level(log::LevelFilter::Info);
        });
        LINES.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    /// Runs `broker` on a thread of its own and waits at most 30 s for it,
    /// so that a broker stuck in a send fails the test instead of hanging
    /// it.
    fn bounded<T: Send + 'static>(broker: impl FnOnce() -> T + Send + 'static) -> T {
        let (done, ran) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(broker());
        });
        ran.recv_timeout(Duration::from_secs(30))
            .expect("the broker is stuck")
    }

    /// A drawer that has said it runs and exits with status N makes the
    /// broker exit with N, and the VT is put back once.
    #[test]
    fn a_drawers_status_is_the_brokers() {
        assert_eq!(broker("exit:0", LONG), (0, 1));
        assert_eq!(broker("exit:3", LONG), (3, 1));
        assert_eq!(broker("exit:255", LONG), (255, 1));
    }

    /// A drawer that a signal kills makes the broker exit with a failure.
    #[test]
    fn a_drawer_killed_by_a_signal_is_a_failure() {
        assert_eq!(broker("signal", LONG), (FAILURE, 1));
    }

    /// A drawer that is not ready within the start bound is stopped, and
    /// killed when it does not stop, and the broker fails. A drawer that
    /// exits before it is ready fails the start too, even with status 0.
    #[test]
    fn no_ready_within_the_bound_stops_the_drawer_and_fails() {
        let now = Bounds {
            start: Duration::ZERO,
            stop: Duration::ZERO,
        };
        assert_eq!(broker("silent", now), (FAILURE, 1));
        assert_eq!(broker("early:0", LONG), (FAILURE, 1));
    }

    /// A stop sends the drawer stop, and the drawer's own status is the
    /// broker's. A drawer that does not stop within the stop bound is
    /// killed, which is a failure.
    #[test]
    fn a_stop_ends_the_drawer_within_its_bound() {
        assert_eq!(broker("stop:7", LONG), (7, 1));
        let short = Bounds {
            start: LONG.start,
            stop: Duration::ZERO,
        };
        assert_eq!(broker("stubborn", short), (FAILURE, 1));
    }

    /// Serves as [`Unserved`] does, and stands in for the VT's holder: a
    /// power key sets a time 20 ms on, and the first turn at or after that
    /// time reports a hung-up VT. Nothing else wakes the loop then, so only
    /// a loop that waits for [`Serve::due`] and runs [`Serve::turn`] stops
    /// the drawer.
    struct HangUpWhenDue(Option<Instant>);

    impl Serve for HangUpWhenDue {
        fn serve(&mut self, request: Request, out: &mut Outbox<'_>) -> io::Result<()> {
            if matches!(request, Request::Power { .. }) {
                self.0 = Some(now() + Duration::from_millis(20));
                return Ok(());
            }
            Unserved.serve(request, out)
        }

        fn turn(&mut self, now: Instant, _: &mut Outbox<'_>) -> bool {
            self.0.is_some_and(|due| now >= due)
        }

        fn due(&self) -> Option<Instant> {
            self.0
        }
    }

    /// The loop runs the devices' turn when it is due, with no event from
    /// the drawer or the kernel, and a VT that hung up stops the drawer,
    /// whose own status is then the broker's.
    #[test]
    fn the_loop_runs_the_devices_turn_when_due_and_a_hangup_stops_the_drawer() {
        let ran = bounded(move || {
            broker_serving("stop:7", &[], LONG, |_, _| Box::new(HangUpWhenDue(None)))
        });
        assert_eq!((ran.status, ran.restored), (7, 1));
    }

    /// A message that the protocol refuses is dropped, and the broker
    /// goes on serving the drawer. The stand-in follows the refused message
    /// with a request that waits for an answer, checks the answer, and
    /// then sends a power key and stops with status 7 when told. A broker
    /// that ended the drawer at the refused message would exit with a
    /// failure, and a wrong answer makes the stand-in exit with 20.
    #[test]
    fn a_refused_message_leaves_the_drawer_served() {
        assert_eq!(broker("refused:7", LONG), (7, 1));
    }

    /// A refused request whose kind waits for an answer gets an answer
    /// with an error, under the request's own id, so the drawer never
    /// waits for it. The stand-in exits with 21 when the answer is not
    /// the device's refusal with `EINVAL`.
    #[test]
    fn a_refused_request_gets_an_error_answer() {
        assert_eq!(broker("answer:7", LONG), (7, 1));
    }

    /// An empty message from a drawer that keeps its socket open is a
    /// message of the wrong size, not a closed socket: the broker refuses
    /// it and goes on serving, so the power key that follows stops the
    /// drawer with its own 7. A broker that took the empty message for a
    /// close would put the VT back before it read the power key, so before
    /// anything asked it to stop.
    ///
    /// The test does not check whether the drawer still ran at the
    /// restore. The stand-in's exit closes its socket before the kernel
    /// reports the exit, and the broker puts the VT back at the close, so
    /// the drawer can still be running then.
    #[test]
    fn an_empty_message_is_refused_and_not_a_close() {
        let bounds = Bounds {
            start: LONG.start,
            stop: Duration::from_secs(2),
        };
        let ran = broker_serving("empty:7", &[], bounds, |ping, _| {
            Box::new(StopOnPower(ping))
        });
        assert_eq!(
            (ran.status, ran.restored, ran.asked_to_stop),
            (7, 1, Some(true))
        );
    }

    /// A drawer that sends refused messages in a loop writes only a few
    /// lines to the log: the first refusal in full, and then at most one
    /// line a second that counts the rest. The stand-in sends 10,000
    /// refused messages and then a power key, so the broker also serves
    /// it to the end.
    #[test]
    fn a_flood_of_refused_messages_writes_few_log_lines() {
        logged();
        let started = now();
        assert_eq!(broker("refusals:7", LONG), (7, 1));
        let seconds = u32::try_from(started.elapsed().as_secs()).unwrap();
        let lines = logged();
        let refusals = lines.iter().filter(|l| l.contains("refuse")).count();
        let bound = 2 + usize::try_from(seconds).unwrap();
        assert!(
            (1..=bound).contains(&refusals),
            "{refusals} lines about refused messages in {seconds} s: {lines:#?}"
        );
    }

    /// A drawer that sends requests and never reads the answers fills its
    /// socket. The broker never waits on the full socket: it keeps the
    /// answers in its queue, stops reading the drawer's requests while the
    /// queue is full, and stays free to stop the drawer. The broker is
    /// asked to stop once its queue is full; the stop cannot reach this
    /// drawer, so the stop bound ends it.
    #[test]
    fn a_drawer_that_reads_no_answers_is_stopped_within_its_bound() {
        let bounds = Bounds {
            start: LONG.start,
            stop: Duration::from_millis(500),
        };
        let ran = bounded(move || {
            broker_serving("flood", &[], bounds, |ping, _| Box::new(StopWhenFull(ping)))
        });
        assert_eq!(
            ran,
            Ran {
                status: FAILURE,
                restored: 1,
                running: Some(false),
                asked_to_stop: Some(true),
            }
        );
    }

    /// A drawer that stops reading its answers and later reads again is
    /// served again. The stand-in sends 2,000 requests without reading,
    /// far more than the socket and the broker's queue hold together. Once
    /// the queue is full, the test tells the stand-in through a named pipe
    /// to read. The stand-in then reads every answer, sends a power key,
    /// and stops with status 7 when told, or exits with 22 when an answer
    /// is missing.
    #[test]
    fn a_drawer_that_reads_again_is_served_again() {
        /// The named pipe, removed when the test ends, whether it passes
        /// or fails.
        struct Fifo(std::path::PathBuf);
        impl Drop for Fifo {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let fifo =
            Fifo(std::env::temp_dir().join(format!("vtium-broker-backlog-{}", std::process::id())));
        let _ = std::fs::remove_file(&fifo.0);
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo.0,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let told = Arc::new(AtomicBool::new(false));
        let (path, seen) = (fifo.0.clone(), Arc::clone(&told));
        let ran = bounded(move || {
            let full = Rc::new(Cell::new(false));
            let ran = broker_serving("backlog:7", &[(FIFO, &path)], LONG, |ping, _| {
                Box::new(ReadAgainWhenFull {
                    stop: StopOnPower(ping),
                    fifo: path.clone(),
                    told: Rc::clone(&full),
                })
            });
            seen.store(full.get(), Ordering::Relaxed);
            (ran.status, ran.restored)
        });
        assert!(
            told.load(Ordering::Relaxed),
            "the broker's queue never filled"
        );
        assert_eq!(ran, (7, 1));
    }

    /// A drawer that closes its socket and hangs has the VT put back at
    /// the close, while it still runs, and is killed at the stop bound.
    /// A drawer that never says it runs is killed first, and the VT comes
    /// back after it.
    #[test]
    fn a_closed_socket_puts_the_vt_back_and_ends_the_drawer() {
        let short = Bounds {
            start: LONG.start,
            stop: Duration::ZERO,
        };
        assert_eq!(broker_watching("close", short), (FAILURE, 1, Some(true)));
        let now = Bounds {
            start: Duration::ZERO,
            stop: Duration::ZERO,
        };
        assert_eq!(broker_watching("silent", now), (FAILURE, 1, Some(false)));
    }

    /// The drawer's real setup, checked from inside the drawer after the
    /// exec: it is not dumpable, it has no-new-privileges, and it holds no
    /// capability. The stand-in exits 0 only when each holds.
    #[test]
    fn the_drawer_runs_set_up() {
        assert_eq!(broker("check", LONG), (0, 1));
    }

    /// The kernel kills the drawer when the broker's thread that started
    /// it ends. The stand-in never ends by itself, so only that signal
    /// ends it; the wait is bounded only so that the test fails instead of
    /// hanging.
    #[test]
    fn the_drawer_dies_with_its_broker() {
        let (_link, theirs) = Link::pair().unwrap();
        let drawer = std::thread::spawn(move || {
            start::spawn(&stand_in("silent"), &me(), theirs, false).unwrap()
        })
        .join()
        .unwrap();
        let mut fds = [rustix::event::PollFd::new(
            &drawer.pidfd,
            rustix::event::PollFlags::IN,
        )];
        let bound = rustix::time::Timespec {
            tv_sec: 30,
            tv_nsec: 0,
        };
        rustix::event::poll(&mut fds, Some(&bound)).unwrap();
        let ended = watch::ended(drawer.pidfd.as_fd()).unwrap();
        if ended.is_none() {
            // a stand-in still running would hold the test's output open
            watch::kill(drawer.pidfd.as_fd());
        }
        assert_eq!(ended, Some(watch::Ended::Signal(9)));
    }

    /// A stand-in drawer, run only by the tests above in a process of its
    /// own, as `VTIUM_FAKE_DRAWER` says.
    #[test]
    #[ignore = "a stand-in drawer that the broker's tests start"]
    fn fake_drawer() {
        let Ok(mode) = std::env::var(MODE) else {
            return;
        };
        let (mode, code) = mode.split_once(':').unwrap_or((&mode, "0"));
        let code: i32 = code.parse().unwrap();
        let hang = || -> ! {
            loop {
                std::thread::park();
            }
        };
        if mode == "silent" {
            hang();
        }
        if mode == "early" {
            std::process::exit(code);
        }
        let link = start::begin().unwrap();
        match mode {
            "exit" => std::process::exit(code),
            "signal" => {
                let _ = rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::KILL,
                );
                hang();
            }
            "stop" | "stubborn" | "refused" | "answer" | "empty" | "refusals" => {
                let send = |bytes: &[u8]| {
                    rustix::net::send(&link, bytes, rustix::net::SendFlags::empty()).unwrap();
                };
                // the next order, waiting for it
                let next = || loop {
                    let mut fds = [rustix::event::PollFd::new(
                        &link,
                        rustix::event::PollFlags::IN,
                    )];
                    rustix::event::poll(&mut fds, None).unwrap();
                    match link.next_order().unwrap() {
                        Got::Message(Ok((order, _))) => return order,
                        Got::Nothing => {}
                        other => panic!("the stand-in read {other:?}"),
                    }
                };
                let power = || {
                    link.request(&Request::Power {
                        key: protocol::PowerKey::Reboot,
                    })
                    .unwrap();
                };
                // The power key goes before the answer is read, so a
                // broker that sends no answer sends the stop first, and
                // the stand-in fails at once instead of waiting.
                let answered = |device: u32, errno: i32, fail: i32| {
                    power();
                    let wanted = Order::Device {
                        device,
                        opened: Err(Errno(u16::try_from(errno).unwrap())),
                    };
                    if next() != wanted {
                        std::process::exit(fail);
                    }
                };
                match mode {
                    "refused" => {
                        send(&refused_vt());
                        link.request(&open_device(1)).unwrap();
                        answered(1, libc::ENOSYS, 20);
                    }
                    "answer" => {
                        // a device path that does not start with /dev/
                        let mut bytes = open_device(5).encode();
                        bytes[9] = b'e';
                        send(&bytes);
                        answered(5, libc::EINVAL, 21);
                    }
                    "empty" => {
                        send(&[]);
                        power();
                    }
                    "refusals" => {
                        for _ in 0..10_000 {
                            send(&refused_vt());
                        }
                        power();
                    }
                    _ => power(),
                }
                if mode == "stubborn" {
                    hang();
                }
                loop {
                    if next() == Order::Stop {
                        std::process::exit(code);
                    }
                }
            }
            "close" => {
                drop(link);
                hang();
            }
            "flood" => {
                for device in 1.. {
                    if link.request(&open_device(device)).is_err() {
                        break;
                    }
                }
                hang();
            }
            "backlog" => {
                std::thread::scope(|s| {
                    s.spawn(|| {
                        // waits until the test says the broker's queue is full
                        let fifo = std::env::var_os(FIFO).unwrap();
                        let mut word = [0u8; 1];
                        std::io::Read::read_exact(
                            &mut std::fs::File::open(fifo).unwrap(),
                            &mut word,
                        )
                        .unwrap();
                        let mut answers = 0u32;
                        loop {
                            let mut fds = [rustix::event::PollFd::new(
                                &link,
                                rustix::event::PollFlags::IN,
                            )];
                            rustix::event::poll(&mut fds, None).unwrap();
                            match link.next_order().unwrap() {
                                Got::Message(Ok((Order::Device { .. }, _))) => answers += 1,
                                Got::Message(Ok((Order::Stop, _))) => {
                                    std::process::exit(if answers == BACKLOG { code } else { 22 })
                                }
                                Got::Nothing => {}
                                _ => std::process::exit(23),
                            }
                        }
                    });
                    for device in 1..=BACKLOG {
                        link.request(&open_device(device)).unwrap();
                    }
                    link.request(&Request::Power {
                        key: protocol::PowerKey::Reboot,
                    })
                    .unwrap();
                });
                hang();
            }
            "check" => {
                use rustix::process::DumpableBehavior;
                let status = std::fs::read_to_string("/proc/self/status").unwrap();
                let field = |name: &str| {
                    status
                        .lines()
                        .find_map(|l| l.strip_prefix(name))
                        .map(str::trim)
                        .unwrap_or("missing")
                        .to_owned()
                };
                let checks = [
                    rustix::process::dumpable_behavior().unwrap() == DumpableBehavior::NotDumpable,
                    field("NoNewPrivs:") == "1",
                    field("CapEff:") == "0000000000000000",
                    field("CapPrm:") == "0000000000000000",
                    field("CapInh:") == "0000000000000000",
                    field("CapAmb:") == "0000000000000000",
                ];
                let failed = checks.iter().position(|ok| !ok);
                std::process::exit(failed.map_or(0, |i| 10 + i32::try_from(i).unwrap()));
            }
            other => panic!("no stand-in drawer is called {other}"),
        }
    }

    struct Stand {
        root: bool,
        account: Option<Account>,
    }

    impl Facts for Stand {
        fn root(&self) -> bool {
            self.root
        }
        fn account(&self, name: &str) -> Option<Account> {
            assert_eq!(name, "vtium");
            self.account.clone()
        }
    }

    /// vtium splits only as root with the account `vtium`. Without root
    /// it runs as one process and logs nothing; as root without the
    /// account it runs as one process and logs one line naming the
    /// account.
    #[test]
    fn vtium_splits_only_as_root_with_the_account() {
        let account = Account {
            uid: 970,
            gid: 970,
            groups: Some(vec![970]),
        };
        let split = Stand {
            root: true,
            account: Some(account.clone()),
        };
        assert_eq!(plan(&split), Plan::Split(account.clone()));
        let user = Stand {
            root: false,
            account: Some(account),
        };
        assert_eq!(plan(&user), Plan::OneProcess(None));
        let missing = Stand {
            root: true,
            account: None,
        };
        let Plan::OneProcess(Some(line)) = plan(&missing) else {
            panic!("one process with a line");
        };
        assert!(line.contains("account vtium does not exist"), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    /// The tests run as a user without root, so the split's own entry
    /// takes the one-process path and starts no drawer.
    #[test]
    fn without_root_split_starts_no_drawer() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        assert_eq!(split(&[]), None);
    }

    /// Only a command line that starts with `drawer` makes this process
    /// the drawer; any other leaves the arguments alone.
    #[test]
    fn only_the_drawers_word_makes_a_drawer() {
        let mut args: Vec<OsString> = vec!["--vt=tty3".into()];
        assert!(drawer(&mut args).unwrap().is_none());
        assert_eq!(args, [OsString::from("--vt=tty3")]);
    }
}
