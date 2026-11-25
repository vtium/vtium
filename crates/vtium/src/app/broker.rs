//! The drawing process's side of the privilege split's process jobs.
//!
//! Under the split, vtium's root process starts every process vtium runs,
//! and the drawing process asks it over their socket: a console's session,
//! a power key, a session's hangup and `SIGWINCH`. The root process
//! answers each request that waits for an answer, and it also sends orders
//! no request asked for: a session's or a helper's exit, `vtium blank`,
//! `vtium wake`, and stop.
//!
//! The drawing process has one socket to the root process, so one reader
//! takes every order from it: the protocol crate's [`Courier`], which the
//! VT's backend shares. A request that waits for its answer blocks until
//! that answer comes, and the courier keeps each other order it reads
//! meanwhile for the loop, which takes them in the order they came.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the requests and the orders of
//!   the process jobs.
//! - ADR-1400 D4, "The device protocol": one socket, fixed-size messages
//!   and descriptors passed with them.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::process::ExitStatus;
use std::rc::Rc;
use std::sync::mpsc;

use vtium_broker_protocol::{
    ConsoleRef, Courier, Display, Got, Grid, Head, Helper, Link, Order, PowerKey, Request, Status,
};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::ping::make_ping;
use vtium_platform::calloop::{Interest, Mode, PostAction};
#[cfg(feature = "images")]
use vtium_platform::image_helper;
use vtium_platform::pty::{Pty, Remote, Size};
use vtium_platform::vt::BrokerVt;

use super::{App, Exit};
use crate::blank;
use crate::child::Display as SessionDisplay;
use crate::helper::Launched;

/// Asks the root process behind `courier` for the session of `console`
/// at `size`, with the two hints, and gives the pty's master.
pub fn open_session(
    courier: &Courier,
    console: &ConsoleRef,
    size: Size,
    display: SessionDisplay,
    truecolor: bool,
) -> io::Result<OwnedFd> {
    let request = Request::OpenSession {
        console: console.clone(),
        grid: Grid {
            cols: size.cols,
            rows: size.rows,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        },
        display: match display {
            SessionDisplay::Kms => Display::Kms,
            SessionDisplay::Fbdev => Display::Fbdev,
        },
        truecolor,
    };
    let answers = |o: &Order| matches!(o, Order::Pty { console: c, .. } if c == console);
    match courier.call(&request, answers)? {
        (Order::Pty { opened: Ok(()), .. }, Some(master)) => Ok(master),
        (
            Order::Pty {
                opened: Err(errno), ..
            },
            _,
        ) => Err(io::Error::from_raw_os_error(i32::from(errno.0))),
        _ => Err(io::Error::other(
            "vtium's root process answered without the pty's master",
        )),
    }
}

/// `status`, as the process's own exit status would read it.
fn exit_status(status: Status) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(match status {
        Status::Code(code) => i32::from(code) << 8,
        Status::Signal(signal) => i32::from(signal),
    })
}

/// The root process, as the process that started a console's session: it
/// hangs the session up and sends it `SIGWINCH` for the drawing process.
struct Starter {
    courier: Rc<Courier>,
    console: ConsoleRef,
}

impl Remote for Starter {
    fn hang_up(&self) {
        let request = Request::CloseSession {
            console: self.console.clone(),
        };
        if let Err(e) = self.courier.send(&request) {
            log::warn!(target: "broker", "vtium cannot ask its root process to hang up a session: {e}");
        }
    }

    fn winch(&self) -> io::Result<()> {
        self.courier.send(&Request::Winch {
            console: self.console.clone(),
        })
    }
}

/// The helpers that vtium's root process starts for the drawing process:
/// the next id to name one by, and where each one's exit status goes once
/// the root process reports it.
///
/// References:
/// - ADR-1400 D4, "What the broker does": the broker starts and stops the
///   helpers, passes each helper's socket and reports each exit.
pub struct RootHelpers {
    courier: Rc<Courier>,
    next: Cell<u32>,
    waiting: RefCell<BTreeMap<u32, mpsc::Sender<ExitStatus>>>,
}

impl RootHelpers {
    pub fn new(courier: Rc<Courier>) -> RootHelpers {
        RootHelpers {
            courier,
            next: Cell::new(1),
            waiting: RefCell::default(),
        }
    }

    /// Asks the root process to start a helper of `kind`, and gives the
    /// id it runs under, this process's end of its socket, and where its
    /// exit status comes once the root process has reaped it.
    pub fn start(&self, kind: Helper) -> io::Result<Launched> {
        let id = self.next.get();
        // ids are never 0, and one wraps only after 2^32 starts
        self.next.set(id.checked_add(1).unwrap_or(1));
        let request = Request::StartHelper { helper: id, kind };
        let answers = |o: &Order| matches!(o, Order::Helper { helper, .. } if *helper == id);
        match self.courier.call(&request, answers)? {
            (
                Order::Helper {
                    started: Ok(()), ..
                },
                Some(socket),
            ) => {
                let (tell, exit) = mpsc::channel();
                self.waiting.borrow_mut().insert(id, tell);
                Ok(Launched { id, socket, exit })
            }
            (
                Order::Helper {
                    started: Err(errno),
                    ..
                },
                _,
            ) => Err(io::Error::from_raw_os_error(i32::from(errno.0))),
            _ => Err(io::Error::other(
                "vtium's root process answered without the helper's socket",
            )),
        }
    }

    /// Asks the root process to stop helper `id`. Its exit is reported
    /// when the root process reaps it.
    pub fn stop(&self, id: u32) {
        if let Err(e) = self.courier.send(&Request::StopHelper { helper: id }) {
            log::warn!(target: "broker", "vtium cannot ask its root process to stop a helper: {e}");
        }
    }

    /// The root process reaped helper `id`, which ended with `status`.
    fn exited(&self, id: u32, status: Status) {
        if let Some(tell) = self.waiting.borrow_mut().remove(&id) {
            // the helper's reader may have gone already
            let _ = tell.send(exit_status(status));
        }
    }
}

impl crate::helper::Root for RootHelpers {
    fn start_font(&self) -> io::Result<Launched> {
        self.start(Helper::Font)
    }

    fn stop(&self, id: u32) {
        RootHelpers::stop(self, id);
    }
}

/// The image helpers of one console, which vtium's root process starts
/// for each request as the account its rules choose.
///
/// References:
/// - ADR-1400 D4, "What the broker does": the broker chooses each image
///   helper's user.
/// - ADR-1101 D2: the image helper runs as the session's user.
#[cfg(feature = "images")]
pub struct ImagesThroughRoot {
    helpers: Rc<RootHelpers>,
    console: ConsoleRef,
    /// The root process has no image helper to start, as it answered once.
    none: bool,
}

#[cfg(feature = "images")]
impl ImagesThroughRoot {
    pub fn new(helpers: Rc<RootHelpers>, console: ConsoleRef) -> ImagesThroughRoot {
        ImagesThroughRoot {
            helpers,
            console,
            none: false,
        }
    }
}

#[cfg(feature = "images")]
impl image_helper::Launch for ImagesThroughRoot {
    /// The root process chooses the account itself. This process tells a
    /// change of account by the owner of the console's pty, which `login`
    /// changes when someone logs in, so a helper of the prompt's account
    /// never serves the user's session.
    fn account(
        &mut self,
        session: &image_helper::Session,
        _: bool,
    ) -> (
        Result<image_helper::Account, image_helper::Refusal>,
        Option<(u32, u32)>,
    ) {
        let account = match &session.pty_owner {
            Ok(owner) => Ok(image_helper::Account {
                uid: *owner,
                gid: *owner,
                groups: None,
            }),
            Err(why) => Err(image_helper::Refusal::NoAccount(format!(
                "the pty's owner cannot be read: {why}"
            ))),
        };
        (account, None)
    }

    fn can_start(&self) -> bool {
        !self.none
    }

    fn start(
        &mut self,
        _: &image_helper::Account,
        file: bool,
        max_bytes: u64,
        largest_file: u64,
    ) -> Result<Box<dyn image_helper::Process>, String> {
        let kind = Helper::Image {
            console: self.console.clone(),
            max_bytes,
            largest_file,
            file,
        };
        match self.helpers.start(kind) {
            Ok(launched) => Ok(Box::new(RootImage {
                helpers: Rc::clone(&self.helpers),
                launched,
                ended: Cell::new(None),
                stopped: Cell::new(false),
            })),
            Err(e) => {
                if e.raw_os_error() == Some(libc::ENOEXEC) {
                    self.none = true;
                }
                Err(format!(
                    "vtium's root process starts no image helper for this request: {e}"
                ))
            }
        }
    }
}

/// An image helper that vtium's root process started, reaps and reports.
#[cfg(feature = "images")]
struct RootImage {
    helpers: Rc<RootHelpers>,
    launched: Launched,
    /// Its exit status, once the root process has reported it.
    ended: Cell<Option<ExitStatus>>,
    /// Whether this process has asked the root process to stop it.
    stopped: Cell<bool>,
}

#[cfg(feature = "images")]
impl RootImage {
    fn status(&self) -> Option<ExitStatus> {
        if self.ended.get().is_none() {
            self.ended.set(self.launched.exit.try_recv().ok());
        }
        self.ended.get()
    }

    fn stop(&self) {
        if !self.stopped.replace(true) {
            self.helpers.stop(self.launched.id);
        }
    }
}

#[cfg(feature = "images")]
impl std::fmt::Debug for RootImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootImage")
            .field("id", &self.launched.id)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "images")]
impl image_helper::Process for RootImage {
    fn socket(&self) -> std::os::fd::BorrowedFd<'_> {
        self.launched.socket.as_fd()
    }

    fn pid(&self) -> Option<u32> {
        None
    }

    fn exited(&self) -> bool {
        self.status().is_some()
    }

    fn kill(self: Box<Self>) {
        self.stop();
    }

    /// A helper that closed its socket but still runs is stopped, as
    /// this process stops its own, and its end is known once the root
    /// process reports it.
    fn finish(self: Box<Self>) -> image_helper::Finish {
        match self.status() {
            Some(status) if status.success() => image_helper::Finish::Clean,
            Some(_) => image_helper::Finish::Failed,
            None => {
                self.stop();
                image_helper::Finish::Unknown(self)
            }
        }
    }
}

/// The protocol's name of head `head`'s console `id`, or an error for a
/// head name the protocol cannot carry, which vtium.conf never gives.
fn console_ref(head: &str, id: u64) -> io::Result<ConsoleRef> {
    let head = Head::new(head).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the head's name is not one vtium.conf allows",
        )
    })?;
    Ok(ConsoleRef { head, console: id })
}

/// The drawing process's courier to the root process at the other end
/// of `link`, and the VT numbered `vt` through that root process, which
/// the drawing process opens no tty for. The VT goes to [`App::new`] and
/// the courier to [`App::set_broker`].
pub fn through_broker(link: Link, vt: u16) -> (Rc<Courier>, BrokerVt) {
    let courier = Rc::new(Courier::new(link));
    let vt = BrokerVt::new(Rc::clone(&courier), vt);
    (courier, vt)
}

impl App {
    /// Makes the root process that `courier` reaches start this vtium's
    /// sessions and run its power keys, and takes the root process's
    /// orders through the loop: the VT's to the VT through the root
    /// process, the rest here. The courier pings the loop for the orders a
    /// call left in its queue.
    pub fn set_broker(&mut self, courier: Rc<Courier>) -> io::Result<()> {
        let watched = courier.as_fd().try_clone_to_owned()?;
        self.handle
            .insert_source(
                Generic::new(watched, Interest::READ, Mode::Level),
                |_, _, app: &mut App| {
                    app.take_orders();
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| io::Error::other(e.error))?;
        // Orders a call left in the courier's queue do not make the socket
        // readable, so the courier pings the loop for them.
        let (ping, queued) = make_ping()?;
        self.handle
            .insert_source(queued, |(), &mut (), app: &mut App| app.take_orders())
            .map_err(|e| io::Error::other(e.error))?;
        courier.on_queued(Box::new(move || ping.ping()));
        self.root_helpers = Some(Rc::new(RootHelpers::new(Rc::clone(&courier))));
        // The font helper runs as `nobody` for the root process, so the
        // files of root, which vtium runs as, pass its trust rule beside
        // the system's, and not those of the drawing process's account.
        self.launcher = self
            .launcher
            .clone()
            .map(|l| l.with_active(0, &crate::font::gate().batch));
        self.broker = Some(courier);
        Ok(())
    }

    /// Asks the root process for the session of head `h`'s console `id`
    /// at `size`, and makes a pty of the master it passes.
    pub(super) fn broker_session(
        &self,
        courier: &Rc<Courier>,
        h: usize,
        id: u64,
        size: Size,
        display: SessionDisplay,
        truecolor: bool,
    ) -> io::Result<Pty> {
        let head = self
            .heads
            .get(h)
            .map(|head| head.name())
            .unwrap_or_default();
        let console = console_ref(head, id)?;
        let master = open_session(courier, &console, size, display, truecolor)?;
        let starter = Starter {
            courier: Rc::clone(courier),
            console,
        };
        Pty::passed(master, Box::new(starter))
    }

    /// Asks the root process to run the power key `key`, which vtium.conf
    /// binds. Returns `false` when this vtium runs as one process.
    pub(super) fn broker_power(&self, key: crate::power::PowerKey) -> bool {
        let Some(courier) = &self.broker else {
            return false;
        };
        let key = match key {
            crate::power::PowerKey::Reboot => PowerKey::Reboot,
            crate::power::PowerKey::PowerOff => PowerKey::PowerOff,
            crate::power::PowerKey::Suspend => PowerKey::Suspend,
        };
        if let Err(e) = courier.send(&Request::Power { key }) {
            log::error!(target: "broker", "vtium cannot ask its root process to run a power key: {e}");
        }
        true
    }

    /// Reads the kernel's messages from `system` through the loop, for the
    /// app to show, as vtium in one process reads them.
    ///
    /// References:
    /// - ADR-1401 D15: the kernel's messages on the console.
    fn watch_kernel(&mut self, system: crate::kernel::System) {
        let watched = system.watched().and_then(|fd| {
            self.handle
                .insert_source(
                    Generic::new(fd, Interest::READ, Mode::Level),
                    |_, _, app: &mut App| {
                        Ok(if app.kernel_ready() {
                            PostAction::Continue
                        } else {
                            PostAction::Remove
                        })
                    },
                )
                .map_err(|e| io::Error::other(e.error))
        });
        match watched {
            Ok(_) => self.set_kernel(Box::new(system)),
            Err(e) => log::info!(
                "vtium cannot read the kernel's messages from {}: {e}. It does not show them on screen.",
                crate::kernel::KMSG
            ),
        }
    }

    /// Takes each order the root process has sent.
    fn take_orders(&mut self) {
        let Some(courier) = self.broker.clone() else {
            return;
        };
        loop {
            match courier.next() {
                Ok(Got::Message(Ok((order, fd)))) if BrokerVt::takes(&order) => {
                    match self.vt.get::<BrokerVt>() {
                        Some(vt) => vt.take(order, fd),
                        None => log::debug!(
                            target: "broker",
                            "vtium's drawing process has no VT through its root process for {order:?}"
                        ),
                    }
                }
                Ok(Got::Message(Ok((order, fd)))) => self.broker_order(order, fd),
                Ok(Got::Message(Err(why))) => log::warn!(
                    target: "broker",
                    "vtium's drawing process ignores a message from vtium's root process: {why}"
                ),
                Ok(Got::Nothing) => return,
                Ok(Got::Closed) | Err(_) => {
                    // The VT through the root process reports the hangup,
                    // and vtium ends as at a hangup of its own VT.
                    match self.vt.get::<BrokerVt>() {
                        Some(vt) => vt.closed(),
                        None => {
                            log::error!(target: "broker", "vtium's drawing process lost its root process, so it stops");
                            self.request_exit(Exit::Signal);
                        }
                    }
                    return;
                }
            }
        }
    }

    /// The root process reaped the session of `console`, which ended with
    /// `status`. The console's pty feeds the output the session left to
    /// the console's terminal, and then the console ends as at the exit
    /// of a session vtium started itself, with the session's own status.
    /// A console that has closed since is not looked for.
    ///
    /// References:
    /// - ADR-1400 D4, "What the broker does": the broker reaps every
    ///   process it starts and reports each exit to the drawer.
    fn session_exit(&mut self, console: &ConsoleRef, status: Status) {
        log::debug!(
            target: "broker",
            "head {}: console {}: the session ended: {status:?}",
            console.head.as_str(),
            console.console
        );
        let Some(head) = self
            .heads
            .iter_mut()
            .find(|h| h.name() == console.head.as_str())
        else {
            return;
        };
        if let Some(crate::head::console::Console {
            pty: Some(pty),
            term,
            ..
        }) = head.console_mut(console.console)
        {
            pty.ended(exit_status(status), term);
        }
    }

    /// Acts on one order from the root process.
    fn broker_order(&mut self, order: Order, fd: Option<OwnedFd>) {
        match order {
            Order::Kmsg => {
                if let Some(kmsg) = fd {
                    self.watch_kernel(crate::kernel::System::with_kmsg(kmsg));
                }
            }
            Order::Font(font) => match fd.map(|file| font.read(&file)) {
                Some(Ok((data, pairs))) => {
                    self.set_console_font(vtium_platform::vt::font::ConsoleFont {
                        width: u32::from(font.width),
                        height: u32::from(font.height),
                        charcount: u32::from(font.glyphs),
                        data,
                        unimap: pairs
                            .into_iter()
                            .map(|(unicode, fontpos)| vtium_platform::vt::font::UniPair {
                                unicode,
                                fontpos,
                            })
                            .collect(),
                    });
                }
                Some(Err(e)) => log::warn!(
                    target: "broker",
                    "vtium's drawing process cannot read the kernel's font from its root process: {e}"
                ),
                None => {}
            },
            Order::Blank => self.order(blank::Order::Blank),
            Order::Wake => self.order(blank::Order::Wake),
            Order::Stop => self.request_exit(Exit::Signal),
            Order::SessionExit { console, status } => self.session_exit(&console, status),
            Order::HelperExit { helper, status } => {
                log::debug!(target: "broker", "helper {helper} ended: {status:?}");
                if let Some(root) = &self.root_helpers {
                    root.exited(helper, status);
                }
            }
            other => log::debug!(
                target: "broker",
                "vtium's drawing process ignores the order {other:?}"
            ),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    //! The drawing process's side against a stand-in root process on a
    //! thread of its own, over a real socket. The stand-in checks a
    //! session's request as the root process does: the grid and the cell
    //! within the allowed values, and a head vtium.conf knows.

    use super::*;
    use std::path::PathBuf;

    use vtium_broker_protocol::{CELL_MAX, Errno};
    use vtium_platform::pty::{Erase, open_session as start_on_pty};

    use crate::app::tests::{Still, no_fonts, seat_with, shown};

    /// What the stand-in saw, in order, until the drawing process closed
    /// its socket.
    fn stand_in(link: Link, sends: Vec<Order>) -> std::thread::JoinHandle<Vec<Request>> {
        stand_in_ending(link, sends, None)
    }

    /// [`stand_in`], whose first session it reports ended with `exit`
    /// once it has passed the session's master, when `exit` is given.
    fn stand_in_ending(
        link: Link,
        sends: Vec<Order>,
        mut exit: Option<Status>,
    ) -> std::thread::JoinHandle<Vec<Request>> {
        std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut sessions = Vec::new();
            for order in sends {
                link.order(&order, None).unwrap();
            }
            loop {
                let mut fds = [rustix::event::PollFd::new(
                    &link,
                    rustix::event::PollFlags::IN,
                )];
                let bound = rustix::event::Timespec {
                    tv_sec: 30,
                    tv_nsec: 0,
                };
                assert_eq!(
                    rustix::event::poll(&mut fds, Some(&bound)).unwrap(),
                    1,
                    "the drawer went quiet"
                );
                match link.next_request().unwrap() {
                    Got::Message(Ok(request)) => {
                        if let Request::OpenSession { console, grid, .. } = &request {
                            let cell = 1..=CELL_MAX;
                            let allowed = grid.cols >= 1
                                && grid.rows >= 1
                                && cell.contains(&grid.cell_width)
                                && cell.contains(&grid.cell_height);
                            let opened = if !allowed {
                                Err(Errno(22))
                            } else if console.head.as_str() != "console" {
                                Err(Errno(2))
                            } else {
                                let mut command = std::process::Command::new("/bin/sleep");
                                command.arg("1000");
                                let size = Size {
                                    cols: grid.cols,
                                    rows: grid.rows,
                                    cell_width: grid.cell_width,
                                    cell_height: grid.cell_height,
                                };
                                Ok(start_on_pty(command, size, Erase::Delete).unwrap())
                            };
                            match opened {
                                Ok(session) => {
                                    let order = Order::Pty {
                                        console: console.clone(),
                                        opened: Ok(()),
                                    };
                                    link.order(&order, Some(session.master.as_fd())).unwrap();
                                    sessions.push(session);
                                    if let Some(status) = exit.take() {
                                        let order = Order::SessionExit {
                                            console: console.clone(),
                                            status,
                                        };
                                        link.order(&order, None).unwrap();
                                    }
                                }
                                Err(errno) => {
                                    let order = Order::Pty {
                                        console: console.clone(),
                                        opened: Err(errno),
                                    };
                                    link.order(&order, None).unwrap();
                                }
                            }
                        }
                        seen.push(request);
                    }
                    Got::Message(Err(refused)) => {
                        panic!("the drawer sent a refused message: {refused:?}")
                    }
                    Got::Nothing => {}
                    Got::Closed => break,
                }
            }
            for session in sessions {
                let mut child = session.child;
                let _ = child.kill();
                let _ = child.wait();
            }
            seen
        })
    }

    /// Waits, for at most 30 s, until the other end of `fd` has closed.
    /// A child another test forks holds a copy of every descriptor of this
    /// process until it execs, so the close of the stand-in's end reaches
    /// the drawer's end only once no such copy is left.
    fn hung_up(fd: std::os::fd::BorrowedFd<'_>) {
        // With no message waiting, the socket polls ready only once it
        // has hung up.
        let mut fds = [rustix::event::PollFd::new(
            &fd,
            rustix::event::PollFlags::IN,
        )];
        let bound = rustix::event::Timespec {
            tv_sec: 30,
            tv_nsec: 0,
        };
        loop {
            match rustix::event::poll(&mut fds, Some(&bound)) {
                Ok(0) => panic!("the stand-in's end of the socket stays open"),
                Ok(_) => {
                    assert!(fds[0].revents().contains(rustix::event::PollFlags::HUP));
                    return;
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => panic!("{e}"),
            }
        }
    }

    #[test]
    fn a_session_s_call_keeps_the_other_orders_for_the_loop() {
        let (broker, theirs) = Link::pair().unwrap();
        let courier = Courier::new(Link::passed(theirs).unwrap());
        let console = console_ref("console", 1).unwrap();
        broker.order(&Order::Blank, None).unwrap();
        let exit = Order::SessionExit {
            console: console.clone(),
            status: Status::Code(0),
        };
        broker.order(&exit, None).unwrap();
        let refused = Order::Pty {
            console: console.clone(),
            opened: Err(Errno(24)),
        };
        broker.order(&refused, None).unwrap();
        let size = Size {
            cols: 80,
            rows: 25,
            cell_width: 8,
            cell_height: 16,
        };
        let e = open_session(&courier, &console, size, SessionDisplay::Kms, true).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(24), "the root process's error");
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::OpenSession { .. }))
        ));
        let order = |got| match got {
            Got::Message(Ok((order, _))) => Some(order),
            _ => None,
        };
        assert_eq!(order(courier.next().unwrap()), Some(Order::Blank));
        assert_eq!(order(courier.next().unwrap()), Some(exit));
        assert!(matches!(courier.next().unwrap(), Got::Nothing));
    }

    #[test]
    fn under_the_split_the_root_process_starts_and_hangs_up_the_session() {
        let (broker, theirs) = Link::pair().unwrap();
        // The blank reaches the socket before the session's answer, so the
        // session's call leaves it queued.
        let seen = stand_in(broker, vec![Order::Blank]);
        let (mut lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Rc::new(Courier::new(Link::passed(theirs).unwrap())))
            .unwrap();
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        app.start();
        shown(&mut app);
        let id = app.heads.first().unwrap().console.id;
        let pty = app.heads.first().unwrap().console.pty.as_ref().unwrap();
        assert!(
            pty.started_elsewhere(),
            "the session is not the root process's"
        );
        assert_eq!(app.order, None);
        // The courier pings the loop for the order its call queued.
        lp.dispatch(Some(std::time::Duration::ZERO), &mut app)
            .unwrap();
        assert_eq!(app.order, Some(blank::Order::Blank));
        assert!(app.broker_power(crate::power::PowerKey::Reboot));
        // the loop holds its own copy of the socket
        drop((app, lp));
        let seen = seen.join().unwrap();
        let console = console_ref("console", id).unwrap();
        assert_eq!(seen.len(), 3, "{seen:?}");
        // The stand-in display is not a DRM device, so the session is told
        // fbdev.
        assert!(
            matches!(&seen[0], Request::OpenSession { console: c, display: Display::Fbdev, truecolor: true, .. } if *c == console),
            "{seen:?}"
        );
        assert_eq!(
            seen[1..],
            [
                Request::Power {
                    key: PowerKey::Reboot
                },
                Request::CloseSession { console }
            ]
        );
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test's seat reads the host's clock, as the seat tests' do"
    )]
    fn a_session_ends_with_the_status_the_root_process_reports() {
        crate::getty::tests::logged();
        let (broker, theirs) = Link::pair().unwrap();
        let seen = stand_in_ending(broker, vec![], Some(Status::Code(3)));
        let (mut lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Rc::new(Courier::new(Link::passed(theirs).unwrap())))
            .unwrap();
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        app.start();
        shown(&mut app);
        assert!(app.heads.first().unwrap().console.pty.is_some());
        // The session's program still runs in the stand-in, so only the
        // root process's report can end the console's session.
        let mut ended = Vec::new();
        for _ in 0..50 {
            lp.dispatch(Some(std::time::Duration::from_millis(20)), &mut app)
                .unwrap();
            app.turn(std::time::Instant::now());
            ended.extend(
                crate::getty::tests::logged()
                    .into_iter()
                    .filter(|l| l.starts_with("child exited")),
            );
            if !ended.is_empty() {
                break;
            }
        }
        assert_eq!(ended, ["child exited: exit status: 3"]);
        drop((app, lp));
        let seen = seen.join().unwrap();
        let opened = seen
            .iter()
            .filter(|r| matches!(r, Request::OpenSession { .. }))
            .count();
        assert_eq!(opened, 2, "the console's next session starts: {seen:?}");
    }

    /// A stand-in root process that answers each helper's start with the
    /// next of `answers`, a socket when it is `Ok`, and then reports the
    /// helper ended with `exit`. It gives the requests it saw once the
    /// drawing process closes its socket.
    fn helper_root(
        link: Link,
        answers: Vec<Result<(), Errno>>,
        exit: Status,
    ) -> std::thread::JoinHandle<Vec<Request>> {
        std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut answers = answers.into_iter();
            let mut kept = Vec::new();
            loop {
                let mut fds = [rustix::event::PollFd::new(
                    &link,
                    rustix::event::PollFlags::IN,
                )];
                let bound = rustix::event::Timespec {
                    tv_sec: 30,
                    tv_nsec: 0,
                };
                assert_eq!(rustix::event::poll(&mut fds, Some(&bound)).unwrap(), 1);
                match link.next_request().unwrap() {
                    Got::Message(Ok(request)) => {
                        if let Request::StartHelper { helper, .. } = &request {
                            let started = answers.next().unwrap();
                            let (ours, theirs) = rustix::net::socketpair(
                                rustix::net::AddressFamily::UNIX,
                                rustix::net::SocketType::STREAM,
                                rustix::net::SocketFlags::CLOEXEC,
                                None,
                            )
                            .unwrap();
                            let order = Order::Helper {
                                helper: *helper,
                                started,
                            };
                            let fd = started.is_ok().then(|| theirs.as_fd());
                            link.order(&order, fd).unwrap();
                            if started.is_ok() {
                                let order = Order::HelperExit {
                                    helper: *helper,
                                    status: exit,
                                };
                                link.order(&order, None).unwrap();
                            }
                            kept.push(ours);
                        }
                        seen.push(request);
                    }
                    Got::Message(Err(refused)) => panic!("{refused:?}"),
                    Got::Nothing => {}
                    Got::Closed => break,
                }
            }
            seen
        })
    }

    #[test]
    fn the_root_process_starts_each_helper_and_reports_its_exit() {
        let (broker, theirs) = Link::pair().unwrap();
        let seen = helper_root(broker, vec![Ok(()), Ok(())], Status::Signal(9));
        let (lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Rc::new(Courier::new(Link::passed(theirs).unwrap())))
            .unwrap();
        let root = Rc::clone(app.root_helpers.as_ref().unwrap());
        let first = root.start(Helper::Font).unwrap();
        let second = root.start(Helper::Font).unwrap();
        assert_eq!(
            (first.id, second.id),
            (1, 2),
            "each helper has an id of its own"
        );
        assert!(first.exit.try_recv().is_err(), "no exit before the report");
        app.take_orders();
        use std::os::unix::process::ExitStatusExt;
        for launched in [&first, &second] {
            let status = launched.exit.try_recv().unwrap();
            assert_eq!(status.signal(), Some(9), "the root process's status");
        }
        root.stop(first.id);
        drop((root, app, lp));
        let seen = seen.join().unwrap();
        assert_eq!(
            seen,
            [
                Request::StartHelper {
                    helper: 1,
                    kind: Helper::Font
                },
                Request::StartHelper {
                    helper: 2,
                    kind: Helper::Font
                },
                Request::StopHelper { helper: 1 },
            ]
        );
    }

    #[cfg(feature = "images")]
    #[test]
    fn a_consoles_image_helpers_start_through_the_root_process() {
        use vtium_platform::image_helper::{Launch, Session};
        let (broker, theirs) = Link::pair().unwrap();
        let seen = helper_root(
            broker,
            vec![Err(Errno(13)), Ok(()), Err(Errno(8))],
            Status::Code(0),
        );
        let courier = Rc::new(Courier::new(Link::passed(theirs).unwrap()));
        let root = Rc::new(RootHelpers::new(Rc::clone(&courier)));
        let console = console_ref("console", 2).unwrap();
        let mut launch = ImagesThroughRoot::new(Rc::clone(&root), console.clone());
        let session = Session {
            child: None,
            pty_owner: Ok(1000),
        };
        let (account, _) = launch.account(&session, true);
        let account = account.unwrap();
        assert_eq!(
            account.uid, 1000,
            "a new owner of the pty asks for a new helper"
        );
        // EACCES: no account may read this request, such as a file before
        // anyone has logged in
        assert!(launch.start(&account, true, 4096, 8192).is_err());
        assert!(launch.can_start());
        let process = launch.start(&account, false, 4096, 8192).unwrap();
        assert_eq!(process.pid(), None, "this process never signals it");
        assert!(!process.exited());
        // the root process's report, which the loop takes
        let Got::Message(Ok((order, _))) = courier.next().unwrap() else {
            panic!("no report");
        };
        let Order::HelperExit { helper, status } = order else {
            panic!("{order:?}");
        };
        root.exited(helper, status);
        assert!(process.exited());
        assert!(matches!(
            process.finish(),
            vtium_platform::image_helper::Finish::Clean
        ));
        // ENOEXEC: the root process has no image helper to start
        assert!(launch.start(&account, false, 4096, 8192).is_err());
        assert!(!launch.can_start(), "no helper is asked for again");
        drop((launch, root, courier));
        let kinds: Vec<Request> = seen.join().unwrap();
        let image = |helper, file| Request::StartHelper {
            helper,
            kind: Helper::Image {
                console: console.clone(),
                max_bytes: 4096,
                largest_file: 8192,
                file,
            },
        };
        assert_eq!(kinds, [image(1, true), image(2, false), image(3, false)]);
    }

    #[test]
    fn the_kernels_log_and_font_come_from_the_root_process() {
        let (broker, theirs) = Link::pair().unwrap();
        let (lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Rc::new(Courier::new(Link::passed(theirs).unwrap())))
            .unwrap();
        let glyphs = vec![0x5a; 16 * 256];
        let (font, file) = vtium_broker_protocol::KernelFont::file(8, 16, &glyphs, &[(0x41, 65)])
            .unwrap()
            .unwrap();
        broker
            .order(&Order::Font(font), Some(file.as_fd()))
            .unwrap();
        // a pipe stands in for /dev/kmsg: readable once the kernel logs
        let (kmsg, _log) = rustix::pipe::pipe().unwrap();
        broker.order(&Order::Kmsg, Some(kmsg.as_fd())).unwrap();
        assert!(app.kernel.is_none());
        app.take_orders();
        let got = app.console_font.as_ref().unwrap();
        assert_eq!((got.width, got.height, got.charcount), (8, 16, 256));
        assert_eq!(got.data, glyphs);
        assert_eq!(
            got.unimap,
            [vtium_platform::vt::font::UniPair {
                unicode: 0x41,
                fontpos: 65
            }]
        );
        assert!(app.kernel.is_some(), "the kernel's messages are not read");
        drop((app, lp, broker));
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test's seat reads the host's clock, as the seat tests' do"
    )]
    fn the_vts_orders_and_the_close_go_to_the_vt_through_the_root_process() {
        use vtium_platform::vt::VtEvent;
        let (broker, theirs) = Link::pair().unwrap();
        let (courier, vt) = through_broker(Link::passed(theirs).unwrap(), 7);
        let lp: vtium_platform::event_loop::Loop<App> =
            vtium_platform::event_loop::Loop::new().unwrap();
        let args: Vec<std::ffi::OsString> = ["--login", "--", "/bin/true"]
            .iter()
            .map(std::ffi::OsString::from)
            .collect();
        let config = crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config;
        let mut app = App::new(
            config,
            Box::new(vt),
            lp.handle(),
            Rc::new(std::time::Instant::now),
            Vec::new(),
        );
        app.set_broker(courier).unwrap();
        // A release while the VT is not shown: the VT lets go at once and
        // says so to the root process.
        broker.order(&Order::Release, None).unwrap();
        app.take_orders();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::Released))
        ));
        assert_eq!(app.exit, None);
        drop(broker);
        let courier = Rc::clone(app.broker.as_ref().unwrap());
        hung_up(courier.as_fd());
        app.take_orders();
        let events: Vec<VtEvent> = app.vt.events().collect();
        assert!(events.contains(&VtEvent::Hangup), "{events:?}");
        drop((app, lp));
    }

    #[test]
    fn the_root_processs_orders_blank_wake_and_stop() {
        let (broker, theirs) = Link::pair().unwrap();
        // Both orders are in the socket before the drawer reads it.
        broker.order(&Order::Blank, None).unwrap();
        broker.order(&Order::Wake, None).unwrap();
        let (lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Rc::new(Courier::new(Link::passed(theirs).unwrap())))
            .unwrap();
        app.take_orders();
        assert_eq!(
            app.order,
            Some(blank::Order::Wake),
            "of two in one turn the last holds"
        );
        assert_eq!(app.exit, None);
        app.take_orders();
        app.broker_order(Order::Stop, None);
        assert_eq!(app.exit, Some(Exit::Signal));
        assert!(
            matches!(broker.next_request().unwrap(), Got::Nothing),
            "the drawer asked something"
        );
        drop((app, lp));
    }
}
