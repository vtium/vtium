//! The drawer's one reader of its socket to the broker.
//!
//! The drawer has one socket to the broker, and several parts of the
//! drawer talk over it: the VT and its devices, the sessions, the helpers
//! and the power keys. Each order the broker sends must reach the part it
//! belongs to, so one reader takes every order from the socket: the
//! [`Courier`]. A request that waits for its answer blocks in
//! [`Courier::call`] until that answer comes. Every other order the
//! courier reads meanwhile waits in a queue, with its descriptor, in the
//! order it came, and [`Courier::next`] hands the queued orders on before
//! it reads the socket again.
//!
//! Orders in the queue do not make the socket readable, so a loop that
//! waits for the socket would not wake for them. The courier calls the
//! loop's waker when a call leaves orders in the queue.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": one socket between the broker
//!   and the drawer, its requests and its orders.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::event::{PollFd, PollFlags, Timespec};

use crate::{Got, Link, Order, Refusal, Request};

/// How long [`Courier::call`] waits with nothing to read before it gives
/// up on the answer, in milliseconds. The broker answers each request
/// within a few milliseconds, so the bound only ends the wait for a broker
/// that has hung.
///
/// The bound is on silence, not on the call's whole time: each order that
/// arrives during the wait starts it again. The crate reads no clock,
/// since time enters vtium only in vtium-platform and vtium-render, and a
/// wait on silence needs none. So a broker that keeps sending other orders
/// can hold a call open past 5,000 ms. The broker is vtium's own root
/// process, and the orders it sends unasked are bounded: one exit for each
/// session and helper, and a blank or wake.
///
/// References:
/// - docs/architecture.md, "Determinism": where time enters vtium.
pub const CALL_BOUND_MS: u32 = 5_000;

/// An order and the descriptor that came with it.
pub type Arrived = (Order, Option<OwnedFd>);

/// The drawer's end of its socket to the broker, and the orders read while
/// a request waited for its answer. Every method takes `&self`, so the
/// parts of the drawer share one courier.
pub struct Courier {
    link: Link,
    queue: RefCell<VecDeque<Arrived>>,
    /// Set once a read found the socket closed.
    closed: Cell<bool>,
    /// Set once [`Courier::next`] has reported the close.
    told_closed: Cell<bool>,
    /// What wakes the drawer's loop when orders wait in the queue.
    wake: RefCell<Option<Box<dyn Fn()>>>,
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
            queue: RefCell::new(VecDeque::new()),
            closed: Cell::new(false),
            told_closed: Cell::new(false),
            wake: RefCell::new(None),
        }
    }

    /// Sets what wakes the drawer's loop when a call leaves orders in the
    /// queue, such as a calloop ping.
    pub fn on_queued(&self, wake: Box<dyn Fn()>) {
        *self.wake.borrow_mut() = Some(wake);
    }

    /// Whether orders wait in the queue.
    pub fn pending(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    /// Sends `request`, without waiting for anything.
    pub fn send(&self, request: &Request) -> io::Result<()> {
        if self.closed.get() {
            return Err(gone());
        }
        self.link.request(request)
    }

    /// Sends `request` and waits for the order that `matches` accepts,
    /// which it returns with its descriptor. Every other order read
    /// meanwhile waits in the queue for [`Courier::next`].
    ///
    /// The wait fails with [`io::ErrorKind::TimedOut`] when nothing comes
    /// to read for [`CALL_BOUND_MS`], a bound on silence that each order
    /// read starts again, and with
    /// [`io::ErrorKind::BrokenPipe`] when the broker has closed its socket.
    pub fn call(&self, request: &Request, matches: impl Fn(&Order) -> bool) -> io::Result<Arrived> {
        self.call_within(request, matches, CALL_BOUND_MS)
    }

    fn call_within(
        &self,
        request: &Request,
        matches: impl Fn(&Order) -> bool,
        bound_ms: u32,
    ) -> io::Result<Arrived> {
        self.send(request)?;
        let queued = self.queue.borrow().len();
        let answer = self.wait_for(&matches, bound_ms);
        if self.queue.borrow().len() > queued
            && let Some(wake) = self.wake.borrow().as_ref()
        {
            wake();
        }
        answer
    }

    fn wait_for(&self, matches: &dyn Fn(&Order) -> bool, bound_ms: u32) -> io::Result<Arrived> {
        loop {
            match self.link.next_order()? {
                Got::Message(Ok((order, fd))) if matches(&order) => return Ok((order, fd)),
                Got::Message(Ok(other)) => self.queue.borrow_mut().push_back(other),
                // The broker never sends a message the reader refuses; one
                // that does is passed over.
                Got::Message(Err(_)) => {}
                Got::Nothing => {
                    if !readable_within(self.link.as_fd(), bound_ms)? {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "vtium's root process did not answer",
                        ));
                    }
                }
                Got::Closed => {
                    self.closed.set(true);
                    return Err(gone());
                }
            }
        }
    }

    /// The next order: the oldest in the queue, else one read from the
    /// socket now. [`Got::Nothing`] means no order is there now. The
    /// socket's close is [`Got::Closed`] once, and [`Got::Nothing`] after
    /// it; the orders queued before the close come first.
    pub fn next(&self) -> io::Result<Got<Arrived, Refusal>> {
        if let Some(queued) = self.queue.borrow_mut().pop_front() {
            return Ok(Got::Message(Ok(queued)));
        }
        if self.closed.get() {
            return Ok(self.close_once());
        }
        match self.link.next_order()? {
            Got::Closed => {
                self.closed.set(true);
                Ok(self.close_once())
            }
            other => Ok(other),
        }
    }

    fn close_once(&self) -> Got<Arrived, Refusal> {
        if self.told_closed.replace(true) {
            Got::Nothing
        } else {
            Got::Closed
        }
    }
}

fn gone() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "vtium's root process has closed its socket",
    )
}

/// Whether `fd` has something to read, or has hung up, within `bound_ms`.
fn readable_within(fd: BorrowedFd<'_>, bound_ms: u32) -> io::Result<bool> {
    let bound = Timespec {
        tv_sec: i64::from(bound_ms / 1000),
        tv_nsec: i64::from(bound_ms % 1000).saturating_mul(1_000_000),
    };
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    loop {
        match rustix::event::poll(&mut fds, Some(&bound)) {
            Ok(n) => return Ok(n > 0),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
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
    use super::*;
    use std::rc::Rc;

    use crate::{ConsoleRef, Head, Status};

    fn console(id: u64) -> ConsoleRef {
        ConsoleRef {
            head: Head::new("left").unwrap(),
            console: id,
        }
    }

    fn exit(id: u64) -> Order {
        Order::SessionExit {
            console: console(id),
            status: Status::Code(0),
        }
    }

    fn courier() -> (Link, Courier) {
        let (broker, theirs) = Link::pair().unwrap();
        (broker, Courier::new(Link::passed(theirs).unwrap()))
    }

    /// The device number and inode of `fd`, which name the file it opens.
    fn file(fd: &OwnedFd) -> (u64, u64) {
        let stat = rustix::fs::fstat(fd).unwrap();
        (stat.st_dev, stat.st_ino)
    }

    fn take_master() -> Request {
        Request::TakeMaster { device: 4 }
    }

    fn is_master(o: &Order) -> bool {
        matches!(o, Order::Master { device: 4, .. })
    }

    #[test]
    fn a_call_returns_only_its_answer_and_keeps_the_rest_in_order() {
        let (broker, courier) = courier();
        let (pipe, _other) = rustix::pipe::pipe().unwrap();
        let passed = file(&pipe);
        let device = Order::Device {
            device: 9,
            opened: Ok(()),
        };
        broker.order(&Order::Blank, None).unwrap();
        broker.order(&device, Some(pipe.as_fd())).unwrap();
        broker.order(&exit(1), None).unwrap();
        let answer = Order::Master {
            device: 4,
            taken: true,
        };
        broker.order(&answer, None).unwrap();
        broker.order(&exit(2), None).unwrap();

        let (got, fd) = courier.call(&take_master(), is_master).unwrap();
        assert_eq!(got, answer);
        assert!(fd.is_none());
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::TakeMaster { device: 4 }))
        ));
        let mut drained = Vec::new();
        while let Got::Message(Ok((order, fd))) = courier.next().unwrap() {
            drained.push((order, fd.as_ref().map(file)));
        }
        assert_eq!(
            drained,
            [
                (Order::Blank, None),
                (device, Some(passed)),
                (exit(1), None),
                (exit(2), None),
            ],
            "an order read during the call was lost or moved"
        );
    }

    #[test]
    fn orders_sent_while_a_call_waits_are_never_lost_or_reordered() {
        let (broker, courier) = courier();
        // A stand-in broker on a thread of its own: once it has read the
        // request, it sends orders around the answer.
        let stand_in = std::thread::spawn(move || {
            assert!(readable_within(broker.as_fd(), 10_000).unwrap());
            assert!(matches!(
                broker.next_request().unwrap(),
                Got::Message(Ok(Request::TakeMaster { device: 4 }))
            ));
            for id in 0..50 {
                broker.order(&exit(id), None).unwrap();
            }
            let answer = Order::Master {
                device: 4,
                taken: false,
            };
            broker.order(&answer, None).unwrap();
            for id in 50..60 {
                broker.order(&exit(id), None).unwrap();
            }
            broker
        });
        let (got, _) = courier.call(&take_master(), is_master).unwrap();
        assert!(matches!(got, Order::Master { taken: false, .. }));
        let broker = stand_in.join().unwrap();
        let mut ids = Vec::new();
        while let Got::Message(Ok((Order::SessionExit { console, .. }, _))) =
            courier.next().unwrap()
        {
            ids.push(console.console);
        }
        assert_eq!(ids, (0..60).collect::<Vec<u64>>());
        drop(broker);
    }

    #[test]
    fn the_socket_s_close_is_told_once_after_the_queued_orders() {
        let (broker, courier) = courier();
        // The stand-in reads the request, sends two orders and closes its
        // socket, so the call queues both and then meets the close.
        let stand_in = std::thread::spawn(move || {
            assert!(readable_within(broker.as_fd(), 10_000).unwrap());
            assert!(matches!(
                broker.next_request().unwrap(),
                Got::Message(Ok(_))
            ));
            broker.order(&Order::Wake, None).unwrap();
            broker.order(&Order::Stop, None).unwrap();
        });
        let e = courier.call(&take_master(), is_master).unwrap_err();
        stand_in.join().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
        assert!(matches!(
            courier.next().unwrap(),
            Got::Message(Ok((Order::Wake, None)))
        ));
        assert!(matches!(
            courier.next().unwrap(),
            Got::Message(Ok((Order::Stop, None)))
        ));
        assert!(matches!(courier.next().unwrap(), Got::Closed));
        assert!(matches!(courier.next().unwrap(), Got::Nothing));
        assert!(matches!(courier.next().unwrap(), Got::Nothing));
        assert_eq!(
            courier.send(&Request::Ready).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn a_call_with_no_answer_times_out() {
        let (broker, courier) = courier();
        broker.order(&Order::Blank, None).unwrap();
        let e = courier
            .call_within(&take_master(), is_master, 50)
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(matches!(
            courier.next().unwrap(),
            Got::Message(Ok((Order::Blank, None)))
        ));
        drop(broker);
    }

    #[test]
    fn the_loop_is_woken_only_when_a_call_leaves_orders_queued() {
        let (broker, courier) = courier();
        let woken = Rc::new(Cell::new(0u32));
        let w = Rc::clone(&woken);
        courier.on_queued(Box::new(move || w.set(w.get().saturating_add(1))));
        let answer = Order::Master {
            device: 4,
            taken: true,
        };
        broker.order(&answer, None).unwrap();
        courier.call(&take_master(), is_master).unwrap();
        assert_eq!(woken.get(), 0, "nothing was queued");
        assert!(!courier.pending());
        broker.order(&Order::Blank, None).unwrap();
        broker.order(&answer, None).unwrap();
        courier.call(&take_master(), is_master).unwrap();
        assert_eq!(woken.get(), 1);
        assert!(courier.pending());
    }
}
