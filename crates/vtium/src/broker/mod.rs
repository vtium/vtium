//! The privilege split: a broker that keeps root, and a drawer that runs
//! everything else as the system account `vtium`.
//!
//! The broker starts the drawer, waits for the drawer to say it runs,
//! serves the drawer's requests, and stops it when systemd stops the unit.
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

pub mod protocol;
pub mod start;
pub mod watch;

use std::ffi::OsString;
use std::io;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use vtium_platform::event_loop::{Loop, Signal};

use protocol::{Errno, Got, Link, Order, Request};
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
/// ready, which the broker's loop handles itself. An error of the kind
/// [`io::ErrorKind::WouldBlock`] from an answer's send means that the
/// drawer reads none of its orders, so `serve` passes the link's error on
/// as it is, and the broker's loop ends that drawer.
pub trait Serve {
    fn serve(&mut self, request: Request, link: &Link) -> io::Result<()>;
}

/// The broker before it serves any request: each request that waits for
/// an answer gets a refusal, and the others do nothing. The cards that
/// move the devices, the VT, the sessions and the helpers into the broker
/// replace it.
pub struct Unserved;

impl Serve for Unserved {
    fn serve(&mut self, request: Request, link: &Link) -> io::Result<()> {
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
        link.order(&answer, None)
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
    let mut lp: Loop<Seen> = Loop::new()?;
    for signal in [Signal::SIGTERM, Signal::SIGINT] {
        lp.on_signal(signal, |seen: &mut Seen| seen.stop = true)?;
    }
    // A write to a drawer gone returns an error in place of ending the
    // broker.
    lp.on_signal(Signal::SIGPIPE, |_| {})?;
    let (link, theirs) = Link::pair()?;
    let arg0 = std::env::args_os()
        .next()
        .unwrap_or_else(|| OsString::from("vtium"));
    let drawer = start::spawn(&Program::vtium(arg0, args), account, theirs, true)?;
    log::info!(
        target: "broker",
        "vtium runs as two processes: this root process, and drawing process {}, which runs everything else as the account {ACCOUNT}",
        drawer.pid.as_raw_nonzero()
    );
    // The broker does not hold the VT yet, so there is nothing to put
    // back. The card that moves the VT into the broker passes its restore
    // here.
    let mut restore = || {};
    Ok(run(
        &mut lp,
        &link,
        &drawer,
        &mut Unserved,
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
/// - The broker serves each other request through `serve`. A message that
///   the protocol's reader refuses comes only from a drawer that is broken
///   or taken over, so the broker kills that drawer and exits with a
///   failure. A drawer whose socket is too full to take an answer reads
///   none of its orders, and the broker ends it the same way.
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
    let pidfd = drawer.pidfd.as_fd();
    let mut back = Restore {
        restore,
        done: false,
    };
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
        let now = clock();
        let deadline = stop_by.or((!ready).then_some(ready_by));
        let mut seen = Seen::default();
        if let Err(e) = lp.dispatch(
            deadline.map(|d| d.saturating_duration_since(now)),
            &mut seen,
        ) {
            log::error!(target: "broker", "vtium's root process cannot wait for events, so vtium stops its drawing process: {e}");
            watch::kill(pidfd);
            back.run();
            return FAILURE;
        }
        // The socket first, so a ready that the drawer sent before it
        // exited counts.
        if (seen.socket || seen.exited) && socket.is_some() {
            loop {
                match link.next_request() {
                    Ok(Got::Message(Ok(Request::Ready))) => ready = true,
                    Ok(Got::Message(Ok(request))) => match serve.serve(request, link) {
                        Ok(()) => {}
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            log::error!(
                                target: "broker",
                                "vtium's drawing process reads none of the answers that vtium's root process sends it, so vtium stops the drawing process"
                            );
                            watch::kill(pidfd);
                            back.run();
                            return FAILURE;
                        }
                        Err(e) => {
                            log::warn!(target: "broker", "vtium's root process cannot answer its drawing process: {e}");
                        }
                    },
                    Ok(Got::Message(Err(refusal))) => {
                        log::error!(
                            target: "broker",
                            "vtium's drawing process sent a message that vtium refuses, so vtium stops the drawing process: {refusal}"
                        );
                        watch::kill(pidfd);
                        back.run();
                        return FAILURE;
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
        if (seen.exited || socket.is_none())
            && let Ok(Some(ended)) = watch::ended(pidfd)
        {
            back.run();
            return status(ended, ready, failed);
        }
        // A stop that finds the drawer's socket full is not sent, and the
        // stop bound ends that drawer.
        if seen.stop && stop_by.is_none() {
            let _ = link.order(&Order::Stop, None);
            stop_by = stop_at(clock());
        }
        if !ready && stop_by.is_none() && clock() >= ready_by {
            log::error!(
                target: "broker",
                "vtium's drawing process did not start within {} ms, so vtium stops it",
                bounds.start.as_millis()
            );
            failed = true;
            let _ = link.order(&Order::Stop, None);
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
    use vtium_platform::calloop::ping::{Ping, make_ping};

    const MODE: &str = "VTIUM_FAKE_DRAWER";

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
        fn serve(&mut self, request: Request, link: &Link) -> io::Result<()> {
            if matches!(request, Request::Power { .. }) {
                self.0.ping();
                return Ok(());
            }
            Unserved.serve(request, link)
        }
    }

    /// What one run of the broker gave: its exit status, and how often it
    /// put the VT back.
    fn broker(mode: &str, bounds: Bounds) -> (u8, u32) {
        let (status, restored, _) = broker_watching(mode, bounds);
        (status, restored)
    }

    /// As [`broker`], and also whether the drawer was still running when
    /// the broker first put the VT back.
    fn broker_watching(mode: &str, bounds: Bounds) -> (u8, u32, Option<bool>) {
        let mut lp: Loop<Seen> = Loop::new().unwrap();
        let (ping, source) = make_ping().unwrap();
        lp.handle()
            .insert_source(source, |_, _, seen: &mut Seen| seen.stop = true)
            .unwrap();
        let (link, theirs) = Link::pair().unwrap();
        let drawer = start::spawn(&stand_in(mode), &me(), theirs, false).unwrap();
        let pidfd = drawer.pidfd.try_clone().unwrap();
        let restored = Rc::new(Cell::new(0));
        let running = Rc::new(Cell::new(None));
        let (count, first) = (Rc::clone(&restored), Rc::clone(&running));
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
            }
        };
        let status = run(
            &mut lp,
            &link,
            &drawer,
            &mut StopOnPower(ping),
            &mut restore,
            bounds,
            &now,
        );
        (status, restored.get(), running.get())
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

    /// A message that the protocol refuses ends the drawer and fails the
    /// broker, before the broker acts on the message or any after it. The
    /// stand-in follows the refused message with a power key and then
    /// stops with status 7 when told, so a broker that let the refused
    /// message pass would exit with 7.
    #[test]
    fn a_refused_message_ends_the_drawer() {
        assert_eq!(broker("refused:7", LONG), (FAILURE, 1));
    }

    /// An empty message from a drawer that keeps its socket open is a
    /// message of the wrong size, not a closed socket: the broker kills
    /// the drawer before it puts the VT back. A broker that took the empty
    /// message for a close would put the VT back while the drawer still
    /// runs, and one that passed over it would stop the drawer at the
    /// power key that follows, and exit with the drawer's 7. The stop
    /// bound only shortens the run of a broken broker.
    #[test]
    fn an_empty_message_is_refused_and_not_a_close() {
        let bounds = Bounds {
            start: LONG.start,
            stop: Duration::from_secs(2),
        };
        assert_eq!(
            broker_watching("empty:7", bounds),
            (FAILURE, 1, Some(false))
        );
    }

    /// A drawer that floods the broker with requests and never reads the
    /// answers fills its socket. The broker never waits on the full
    /// socket: it ends that drawer as it ends one that sends a refused
    /// message, so it stays free to stop and to put the VT back. The
    /// broker runs on a thread of its own, and the wait for it is bounded
    /// only so that a broker stuck in a send fails the test instead of
    /// hanging it.
    #[test]
    fn a_drawer_that_reads_no_answers_cannot_hold_the_broker() {
        let (done, ran) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(broker_watching("flood", LONG));
        });
        let ran = ran
            .recv_timeout(Duration::from_secs(30))
            .expect("the broker is stuck sending to a drawer that reads nothing");
        assert_eq!(ran, (FAILURE, 1, Some(false)));
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
            "stop" | "stubborn" | "refused" | "empty" => {
                if mode == "refused" {
                    // a VT of 0, which the reader refuses
                    let mut bytes = Request::SwitchVt { vt: 1 }.encode();
                    bytes[4] = 0;
                    rustix::net::send(&link, &bytes, rustix::net::SendFlags::empty()).unwrap();
                }
                if mode == "empty" {
                    rustix::net::send(&link, &[], rustix::net::SendFlags::empty()).unwrap();
                }
                link.request(&Request::Power {
                    key: protocol::PowerKey::Reboot,
                })
                .unwrap();
                if mode == "stubborn" {
                    hang();
                }
                loop {
                    let mut fds = [rustix::event::PollFd::new(
                        &link,
                        rustix::event::PollFlags::IN,
                    )];
                    rustix::event::poll(&mut fds, None).unwrap();
                    if let Got::Message(Ok((Order::Stop, _))) = link.next_order().unwrap() {
                        std::process::exit(code);
                    }
                }
            }
            "close" => {
                drop(link);
                hang();
            }
            "flood" => {
                let path = protocol::DevicePath::new("/dev/dri/card0").unwrap();
                for device in 1.. {
                    let open = Request::OpenDevice {
                        device,
                        path: path.clone(),
                    };
                    if link.request(&open).is_err() {
                        break;
                    }
                }
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
