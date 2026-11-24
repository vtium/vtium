//! The broker's orders to the drawer, and the queue that holds them while
//! the drawer's socket is full.
//!
//! The broker's end of the socket never blocks, because a send that waited
//! for room would stop the broker's loop, with its signals, its bounds and
//! the VT's restore. A drawer that stops reading its orders fills the
//! socket. The broker then keeps each order it cannot send yet in a queue,
//! in the order it made them, and sends them as the socket makes room. It
//! neither waits for the drawer nor ends it for this.
//!
//! The queue holds at most [`QUEUE`] orders. While it is full, the broker
//! reads none of the drawer's requests. Each request gets at most one
//! answer, so a drawer that keeps sending requests and never reads cannot
//! make the queue grow past its bound. The broker's [`Order::Stop`] is the
//! one order that may go past it, since a stop must reach a drawer that
//! starts reading again.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": the broker refuses what it does
//!   not accept and keeps serving the drawer, and a drawer that reads no
//!   answers neither blocks the broker nor is killed for it.

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, OwnedFd};

use super::protocol::{Link, Order};

/// The most orders the queue holds. The socket itself holds a few hundred
/// orders before it is full, so the queue only takes the answers to the
/// requests already read when the socket fills. 64 orders keep the queue
/// small, a few kilobytes and at most 64 descriptors, so a drawer that
/// reads nothing makes the broker hold little and can never use up the
/// broker's descriptors.
pub const QUEUE: usize = 64;

/// The broker's orders on their way to the drawer.
pub struct Outbox<'a> {
    link: &'a Link,
    queue: VecDeque<(Order, Option<OwnedFd>)>,
}

impl<'a> Outbox<'a> {
    pub fn new(link: &'a Link) -> Outbox<'a> {
        Outbox {
            link,
            queue: VecDeque::new(),
        }
    }

    /// Sends `order`, with `fd` exactly when the order carries one, or
    /// queues it behind the orders that wait. A full queue refuses the
    /// order, which the broker's loop prevents by reading no request while
    /// the queue is full.
    pub fn order(&mut self, order: Order, fd: Option<OwnedFd>) -> io::Result<()> {
        if order.carries_descriptor() != fd.is_some() {
            return Err(io::Error::other(
                "an order goes with a descriptor exactly when it carries one",
            ));
        }
        if self.full() {
            return Err(io::Error::other(format!(
                "{QUEUE} orders already wait for vtium's drawing process to read them"
            )));
        }
        self.queue.push_back((order, fd));
        self.flush()
    }

    /// Sends [`Order::Stop`], or queues it even when the queue is full.
    pub fn stop(&mut self) -> io::Result<()> {
        self.queue.push_back((Order::Stop, None));
        self.flush()
    }

    /// Sends the orders that wait, from the oldest, until the socket is
    /// full again. Any error other than a full socket means that the
    /// drawer's end is gone, so the orders that wait are dropped, and
    /// their descriptors closed.
    pub fn flush(&mut self) -> io::Result<()> {
        while let Some((order, fd)) = self.queue.front() {
            match self.link.order(order, fd.as_ref().map(AsFd::as_fd)) {
                Ok(()) => {
                    self.queue.pop_front();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => {
                    self.queue.clear();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Whether the queue holds [`QUEUE`] orders, so the broker reads no
    /// more requests.
    pub fn full(&self) -> bool {
        self.queue.len() >= QUEUE
    }

    /// Whether any order waits for room in the socket.
    pub fn waiting(&self) -> bool {
        !self.queue.is_empty()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::broker::protocol::Got;

    /// A socket pair: the broker's link and the drawer's. The broker's
    /// end must not block, or each test below would wait forever in its
    /// first send to a full socket instead of failing.
    fn pair() -> (Link, Link) {
        let (broker, theirs) = Link::pair().unwrap();
        let flags = rustix::fs::fcntl_getfl(&broker).unwrap();
        assert!(
            flags.contains(rustix::fs::OFlags::NONBLOCK),
            "the broker's end blocks"
        );
        (broker, Link::passed(theirs).unwrap())
    }

    /// The orders a drawer reads, until nothing waits.
    fn drain(drawer: &Link) -> Vec<Order> {
        let mut read = Vec::new();
        while let Got::Message(Ok((order, _))) = drawer.next_order().unwrap() {
            read.push(order);
        }
        read
    }

    /// Orders that meet a full socket wait in the queue, in their order,
    /// up to its bound; a stop goes past the bound; and once the drawer
    /// reads, every order arrives, the queued ones after those the socket
    /// took, and none twice.
    #[test]
    fn orders_wait_for_room_and_arrive_in_order() {
        let (broker, drawer) = pair();
        let mut out = Outbox::new(&broker);
        let mut sent = 0u32;
        while !out.waiting() {
            sent += 1;
            out.order(
                Order::Master {
                    device: sent,
                    taken: false,
                },
                None,
            )
            .unwrap();
        }
        while !out.full() {
            sent += 1;
            out.order(
                Order::Master {
                    device: sent,
                    taken: false,
                },
                None,
            )
            .unwrap();
        }
        assert!(
            out.order(
                Order::Master {
                    device: 1,
                    taken: true
                },
                None
            )
            .is_err(),
            "a full queue takes no more orders"
        );
        out.stop().unwrap();
        let mut read = Vec::new();
        while out.waiting() {
            read.extend(drain(&drawer));
            out.flush().unwrap();
        }
        read.extend(drain(&drawer));
        let mut wanted: Vec<Order> = (1..=sent)
            .map(|device| Order::Master {
                device,
                taken: false,
            })
            .collect();
        wanted.push(Order::Stop);
        assert_eq!(read, wanted);
    }

    /// An order that carries a descriptor keeps it while it waits, and
    /// the drawer gets it with the order.
    #[test]
    fn a_queued_order_keeps_its_descriptor() {
        let (broker, drawer) = pair();
        let mut out = Outbox::new(&broker);
        let mut device = 0;
        while !out.waiting() {
            device += 1;
            out.order(
                Order::Master {
                    device,
                    taken: false,
                },
                None,
            )
            .unwrap();
        }
        let (file, _keep) = Link::pair().unwrap();
        let opened = Order::Device {
            device: 9,
            opened: Ok(()),
        };
        out.order(
            opened.clone(),
            Some(file.as_fd().try_clone_to_owned().unwrap()),
        )
        .unwrap();
        let mut last = None;
        while out.waiting() || last.is_none() {
            out.flush().unwrap();
            while let Got::Message(Ok(got)) = drawer.next_order().unwrap() {
                last = Some(got);
            }
        }
        match last {
            Some((order, Some(_fd))) => assert_eq!(order, opened),
            other => panic!("{other:?}"),
        }
        assert!(
            out.order(opened, None).is_err(),
            "an opened device goes with its descriptor"
        );
    }

    /// Once the drawer's end is gone, the queue is dropped, and the error
    /// says why.
    #[test]
    fn a_closed_drawer_drops_the_queue() {
        let (broker, drawer) = pair();
        let mut out = Outbox::new(&broker);
        let mut device = 0;
        while !out.waiting() {
            device += 1;
            out.order(
                Order::Master {
                    device,
                    taken: false,
                },
                None,
            )
            .unwrap();
        }
        drop(drawer);
        broker.wait_for_close();
        assert!(out.flush().is_err());
        assert!(!out.waiting());
    }
}
