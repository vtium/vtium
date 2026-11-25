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

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::rc::Rc;

use vtium_broker_protocol::{
    ConsoleRef, Courier, Display, Got, Grid, Head, Link, Order, PowerKey, Request,
};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::ping::make_ping;
use vtium_platform::calloop::{Interest, Mode, PostAction};
use vtium_platform::pty::{Pty, Remote, Size};
use vtium_platform::vt::BrokerVt;

use super::{App, Exit};
use crate::blank;
use crate::child::Display as SessionDisplay;

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
            // Each console watches its session's leader for the exit, so
            // the report only adds the status to the log.
            Order::SessionExit { console, status } => log::debug!(
                target: "broker",
                "head {}: console {}: the session ended: {status:?}",
                console.head.as_str(),
                console.console
            ),
            Order::HelperExit { helper, status } => log::debug!(
                target: "broker",
                "helper {helper} ended: {status:?}"
            ),
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

    use vtium_broker_protocol::{CELL_MAX, Errno, Status};
    use vtium_platform::pty::{Erase, open_session as start_on_pty};

    use crate::app::tests::{Still, no_fonts, seat_with, shown};

    /// What the stand-in saw, in order, until the drawing process closed
    /// its socket.
    fn stand_in(link: Link, sends: Vec<Order>) -> std::thread::JoinHandle<Vec<Request>> {
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
        assert!(pty.adopted(), "the session is not the root process's");
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
