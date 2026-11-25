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
//! takes every order from it: the [`Courier`]. A request that waits for
//! its answer blocks until that answer comes, and the courier keeps each
//! other order it reads meanwhile for the loop, which takes them in the
//! order they came.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the requests and the orders of
//!   the process jobs.
//! - ADR-1400 D4, "The device protocol": one socket, fixed-size messages
//!   and descriptors passed with them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::rc::Rc;

use vtium_broker_protocol::{ConsoleRef, Display, Got, Grid, Head, Link, Order, PowerKey, Request};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, Mode, PostAction};
use vtium_platform::pty::{Pty, Remote, Size};

use super::{App, Exit};
use crate::blank;
use crate::child::Display as SessionDisplay;

/// An order and the descriptor that came with it.
pub type Received = (Order, Option<OwnedFd>);

/// The drawing process's end of its socket to the root process, and the
/// orders read while a request waited for its answer.
pub struct Courier {
    link: Link,
    waiting: RefCell<VecDeque<Received>>,
}

impl AsFd for Courier {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.link.as_fd()
    }
}

impl Courier {
    pub fn new(link: Link) -> Courier {
        Courier {
            link,
            waiting: RefCell::new(VecDeque::new()),
        }
    }

    /// Sends `request`, which waits for no answer.
    pub fn send(&self, request: &Request) -> io::Result<()> {
        self.link.request(request)
    }

    /// Sends `request` and waits for the order that `answers` accepts.
    /// Every other order read meanwhile waits for [`Courier::next`]. The
    /// root process answers each such request once, and its socket's close
    /// ends the wait with an error.
    pub fn call(
        &self,
        request: &Request,
        answers: impl Fn(&Order) -> bool,
    ) -> io::Result<Received> {
        self.link.request(request)?;
        loop {
            match self.link.next_order()? {
                Got::Message(Ok((order, fd))) if answers(&order) => return Ok((order, fd)),
                Got::Message(Ok(other)) => self.waiting.borrow_mut().push_back(other),
                Got::Message(Err(why)) => {
                    log::warn!(target: "broker", "vtium's drawing process ignores a message from vtium's root process: {why}");
                }
                Got::Nothing => wait_readable(self.link.as_fd())?,
                Got::Closed => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "vtium's root process has closed its socket",
                    ));
                }
            }
        }
    }

    /// The next order, the oldest that waited first, or `None` when no
    /// order is there now. The socket's close is an error.
    pub fn next(&self) -> io::Result<Option<Received>> {
        if let Some(waited) = self.waiting.borrow_mut().pop_front() {
            return Ok(Some(waited));
        }
        loop {
            match self.link.next_order()? {
                Got::Message(Ok(order)) => return Ok(Some(order)),
                Got::Message(Err(why)) => {
                    log::warn!(target: "broker", "vtium's drawing process ignores a message from vtium's root process: {why}");
                }
                Got::Nothing => return Ok(None),
                Got::Closed => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "vtium's root process has closed its socket",
                    ));
                }
            }
        }
    }

    /// Asks the root process for the session of `head`'s console
    /// `console` at `size`, with the two hints, and gives the pty's master.
    pub fn open_session(
        &self,
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
        match self.call(&request, answers)? {
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
}

/// Waits until `fd` has something to read, or has hung up.
fn wait_readable(fd: BorrowedFd<'_>) -> io::Result<()> {
    let mut fds = [rustix::event::PollFd::new(
        &fd,
        rustix::event::PollFlags::IN,
    )];
    loop {
        match rustix::event::poll(&mut fds, None) {
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
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

impl App {
    /// Makes the root process at the other end of `link` start this
    /// vtium's sessions and run its power keys, and takes the root
    /// process's orders through the loop.
    pub fn set_broker(&mut self, link: Link) -> io::Result<()> {
        let courier = Rc::new(Courier::new(link));
        let watched = courier.link.as_fd().try_clone_to_owned()?;
        self.handle
            .insert_source(
                Generic::new(watched, Interest::READ, Mode::Level),
                |_, _, app: &mut App| {
                    app.take_orders();
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| io::Error::other(e.error))?;
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
        let master = courier.open_session(&console, size, display, truecolor)?;
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

    /// Takes each order the root process has sent.
    fn take_orders(&mut self) {
        let Some(courier) = self.broker.clone() else {
            return;
        };
        loop {
            match courier.next() {
                Ok(Some((order, fd))) => self.broker_order(order, fd),
                Ok(None) => return,
                Err(e) => {
                    log::error!(target: "broker", "vtium's drawing process lost its root process, so it stops: {e}");
                    self.request_exit(Exit::Signal);
                    return;
                }
            }
        }
    }

    /// Acts on one order from the root process.
    fn broker_order(&mut self, order: Order, _fd: Option<OwnedFd>) {
        match order {
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
                "vtium's drawing process takes no devices from its root process yet: {other:?}"
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
    use vtium_platform::pty::{Erase, open_session};

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
                                Ok(open_session(command, size, Erase::Delete).unwrap())
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

    #[test]
    fn a_call_keeps_the_other_orders_for_the_loop() {
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
        let e = courier
            .open_session(&console, size, SessionDisplay::Kms, true)
            .unwrap_err();
        assert_eq!(e.raw_os_error(), Some(24), "the root process's error");
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::OpenSession { .. }))
        ));
        assert_eq!(courier.next().unwrap().map(|o| o.0), Some(Order::Blank));
        assert_eq!(courier.next().unwrap().map(|o| o.0), Some(exit));
        assert!(courier.next().unwrap().is_none());
        drop(broker);
        assert!(
            courier.next().is_err(),
            "the root process's close is an error"
        );
    }

    #[test]
    fn under_the_split_the_root_process_starts_and_hangs_up_the_session() {
        let (broker, theirs) = Link::pair().unwrap();
        let seen = stand_in(broker, Vec::new());
        let (lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Link::passed(theirs).unwrap()).unwrap();
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A")])),
        );
        app.start();
        shown(&mut app);
        let id = app.heads.first().unwrap().console.id;
        let pty = app.heads.first().unwrap().console.pty.as_ref().unwrap();
        assert!(pty.adopted(), "the session is not the root process's");
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
    fn the_root_processs_orders_blank_wake_and_stop() {
        let (broker, theirs) = Link::pair().unwrap();
        let seen = stand_in(broker, vec![Order::Blank, Order::Wake]);
        let (lp, mut app) = seat_with(&[], None, no_fonts());
        app.set_broker(Link::passed(theirs).unwrap()).unwrap();
        app.take_orders();
        assert_eq!(
            app.order,
            Some(blank::Order::Wake),
            "of two in one turn the last holds"
        );
        assert_eq!(app.exit, None);
        app.broker_order(Order::Stop, None);
        assert_eq!(app.exit, Some(Exit::Signal));
        drop((app, lp));
        assert!(seen.join().unwrap().is_empty());
    }
}
