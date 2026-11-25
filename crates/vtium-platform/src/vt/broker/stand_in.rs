//! The tests' stand-in for vtium's broker: the real [`Holder`] over the
//! stand-in console and stand-in device nodes, at the other end of a real
//! socket.
//!
//! The stand-in refuses what the real broker refuses, through the same
//! code: the protocol's reader refuses a path outside `/dev/`, and
//! [`crate::udev::admit`] refuses a node of the wrong major number, of the
//! wrong kind, of another seat, or without udev's tag `seat`, from the
//! facts the test gives each node. A refused node is never opened, and the
//! answer is `EACCES`, as `open_seat_node` answers.
//!
//! Each device the stand-in opens is a memfd named after its node, so a
//! test can tell from a descriptor's inode which open file it is. The
//! stand-in records each call on a node with the number of calls the
//! stand-in console had seen by then, so a test can check the order of the
//! two.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use vtium_broker_protocol::{Got, Link, Request};

use super::holder::{Holder, Nodes};
use crate::udev::{DeviceKind, INPUT_MAJOR, NodeFacts, admit};
use crate::vt::fake::FakeConsole;
use crate::vt::{KbMode, KernelVt};

/// One call on a node, by the node's path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NodeCall {
    Open(PathBuf),
    SetMaster(PathBuf),
    DropMaster(PathBuf),
    Revoke(PathBuf),
}

/// The stand-in nodes' state, shared with the test.
#[derive(Debug, Default)]
pub(crate) struct NodeState {
    /// The facts of each node the test made, by path.
    pub(crate) facts: BTreeMap<PathBuf, NodeFacts>,
    /// Each call, with how many calls the console had seen then.
    pub(crate) calls: Vec<(NodeCall, usize)>,
    /// The inode of each open file the stand-in opened, by path, the last
    /// open of each.
    pub(crate) files: BTreeMap<PathBuf, u64>,
    /// The paths of each inode the stand-in opened.
    by_inode: BTreeMap<u64, PathBuf>,
    /// The cards whose master the stand-in holds.
    pub(crate) masters: Vec<PathBuf>,
}

/// The stand-in device nodes.
#[derive(Clone, Debug)]
pub(crate) struct FakeNodes {
    pub(crate) state: Rc<RefCell<NodeState>>,
    console: FakeConsole,
}

impl FakeNodes {
    fn record(&self, call: NodeCall) {
        let seen = self.console.calls().len();
        self.state.borrow_mut().calls.push((call, seen));
    }

    fn path_of(&self, fd: BorrowedFd<'_>) -> io::Result<PathBuf> {
        let inode = rustix::fs::fstat(fd)?.st_ino;
        self.state
            .borrow()
            .by_inode
            .get(&inode)
            .cloned()
            .ok_or_else(|| io::Error::from(rustix::io::Errno::BADF))
    }
}

impl Nodes for FakeNodes {
    fn open(&mut self, path: &Path, seat: &str) -> io::Result<(DeviceKind, OwnedFd)> {
        let facts = self
            .state
            .borrow()
            .facts
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(rustix::io::Errno::NOENT))?;
        let kind = admit(&facts, seat).map_err(|_| io::Error::from(rustix::io::Errno::ACCESS))?;
        self.record(NodeCall::Open(path.to_owned()));
        let fd = rustix::fs::memfd_create(
            path.as_os_str().as_encoded_bytes(),
            rustix::fs::MemfdFlags::CLOEXEC,
        )?;
        let inode = rustix::fs::fstat(&fd)?.st_ino;
        let mut state = self.state.borrow_mut();
        state.files.insert(path.to_owned(), inode);
        state.by_inode.insert(inode, path.to_owned());
        Ok((kind, fd))
    }

    fn set_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()> {
        let path = self.path_of(card)?;
        self.record(NodeCall::SetMaster(path.clone()));
        let mut state = self.state.borrow_mut();
        if !state.masters.contains(&path) {
            state.masters.push(path);
        }
        Ok(())
    }

    /// Refused with `EINVAL` on a card without master, as the kernel
    /// refuses it (`drm_dropmaster_ioctl`).
    fn drop_master(&mut self, card: BorrowedFd<'_>) -> io::Result<()> {
        let path = self.path_of(card)?;
        self.record(NodeCall::DropMaster(path.clone()));
        let mut state = self.state.borrow_mut();
        let before = state.masters.len();
        state.masters.retain(|p| *p != path);
        if state.masters.len() == before {
            return Err(io::Error::from(rustix::io::Errno::INVAL));
        }
        Ok(())
    }

    fn revoke(&mut self, input: BorrowedFd<'_>) -> io::Result<()> {
        let path = self.path_of(input)?;
        self.record(NodeCall::Revoke(path));
        Ok(())
    }
}

/// The facts of a character device of `major`, `subsystem` and `sysname`,
/// on seat0 and tagged for a seat, as udev leaves a seat's device.
pub(crate) fn seat_node(major: u32, subsystem: &str, sysname: &str) -> NodeFacts {
    NodeFacts {
        char_device: true,
        major,
        subsystem: Some(subsystem.to_owned()),
        sysname: sysname.to_owned(),
        seat: None,
        tagged: Some(true),
    }
}

pub(crate) const CARD: &str = "/dev/dri/card0";
pub(crate) const FB: &str = "/dev/fb0";
pub(crate) const KEYBOARD: &str = "/dev/input/event3";
pub(crate) const MOUSE: &str = "/dev/input/event4";

/// The stand-in broker, which the test drives.
pub(crate) struct StandIn {
    pub(crate) holder: Holder<FakeConsole, FakeNodes>,
    pub(crate) console: FakeConsole,
    pub(crate) nodes: Rc<RefCell<NodeState>>,
    /// The broker's end of the socket.
    pub(crate) link: Link,
    /// The time the stand-in's turns run at, which the test moves.
    pub(crate) now: Cell<Instant>,
    /// The requests the stand-in read, in order, refused ones left out.
    pub(crate) requests: Vec<Request>,
}

impl StandIn {
    /// The broker on VT `num` while VT `on_screen` is shown, its console
    /// in `keyboard` mode, with a card, a framebuffer and two input
    /// devices of seat0; and the drawer's end of the socket.
    pub(crate) fn new(num: u16, on_screen: u16, keyboard: KbMode) -> (StandIn, Link) {
        let console = FakeConsole::new(num, on_screen, keyboard);
        let state = Rc::new(RefCell::new(NodeState::default()));
        {
            let mut s = state.borrow_mut();
            for (path, facts) in [
                (CARD, seat_node(crate::udev::DRM_MAJOR, "drm", "card0")),
                (FB, seat_node(crate::udev::FB_MAJOR, "graphics", "fb0")),
                (KEYBOARD, seat_node(INPUT_MAJOR, "input", "event3")),
                (MOUSE, seat_node(INPUT_MAJOR, "input", "event4")),
            ] {
                s.facts.insert(PathBuf::from(path), facts);
            }
        }
        let nodes = FakeNodes {
            state: Rc::clone(&state),
            console: console.clone(),
        };
        let vt = KernelVt::open(console.clone(), num)
            .unwrap_or_else(|e| panic!("the stand-in console opens: {e}"));
        let holder = Holder::with(vt, nodes, "seat0");
        let (link, theirs) =
            Link::pair().unwrap_or_else(|e| panic!("a socket pair for the stand-in: {e}"));
        let drawer =
            Link::passed(theirs).unwrap_or_else(|e| panic!("the drawer's end of the socket: {e}"));
        #[expect(
            clippy::disallowed_methods,
            reason = "the stand-in's clock starts at the test's start"
        )]
        let now = Instant::now();
        (
            StandIn {
                holder,
                console,
                nodes: state,
                link,
                now: Cell::new(now),
                requests: Vec::new(),
            },
            drawer,
        )
    }

    /// Serves every request waiting, as the broker's loop does, then runs
    /// the holder's turn.
    pub(crate) fn pump(&mut self) {
        let now = self.now.get();
        loop {
            match self.link.next_request() {
                Ok(Got::Message(Ok(request))) => {
                    self.requests.push(request.clone());
                    if Holder::<FakeConsole, FakeNodes>::serves(&request) {
                        let orders = self.holder.serve(request, now);
                        self.send(orders);
                    }
                }
                Ok(Got::Message(Err(refused))) => {
                    if let Some(answer) = refused.answer {
                        self.send(vec![(answer, None)]);
                    }
                }
                Ok(Got::Nothing | Got::Closed) | Err(_) => break,
            }
        }
        let orders = self.holder.turn(now);
        self.send(orders);
    }

    fn send(&self, orders: super::Orders) {
        for (order, fd) in orders {
            self.link
                .order(&order, fd.as_ref().map(AsFd::as_fd))
                .unwrap_or_else(|e| panic!("the stand-in sends {order:?}: {e}"));
        }
    }

    /// The kernel asks for the VT, to switch to VT `to`.
    pub(crate) fn switch_away(&mut self, to: u16) {
        self.console.0.borrow_mut().pending_switch = Some(to);
        self.holder.vt().release_signal();
    }

    /// The kernel shows the stand-in's VT again.
    pub(crate) fn switch_back(&mut self) {
        let num = self.console.0.borrow().num;
        self.console.show(num);
        self.holder.vt().acquire_signal();
    }

    /// The calls on the nodes, without the console's count.
    pub(crate) fn node_calls(&self) -> Vec<NodeCall> {
        self.nodes
            .borrow()
            .calls
            .iter()
            .map(|(c, _)| c.clone())
            .collect()
    }
}

/// A job the stand-in broker's thread runs on the stand-in.
type Job = Box<dyn FnOnce(&mut StandIn) + Send>;

/// The stand-in broker on a thread of its own, as the real broker is a
/// process of its own: it serves the drawer's requests while the drawer
/// waits in [`vtium_broker_protocol::Courier::call`]. Between the test's
/// jobs the thread serves every few milliseconds.
pub(crate) struct Remote {
    jobs: RefCell<Option<std::sync::mpsc::Sender<Job>>>,
    thread: RefCell<Option<std::thread::JoinHandle<()>>>,
}

impl Remote {
    /// The stand-in broker of [`StandIn::new`] on its thread, and the
    /// drawer's end of the socket. The stand-in's first turn has run, so its
    /// first orders wait on the socket.
    pub(crate) fn start(num: u16, on_screen: u16, keyboard: KbMode) -> (Remote, Link) {
        let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
        let (give, drawer) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (mut s, link) = StandIn::new(num, on_screen, keyboard);
            s.pump();
            let _ = give.send(link);
            loop {
                match inbox.recv_timeout(std::time::Duration::from_millis(2)) {
                    Ok(job) => job(&mut s),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                }
                s.pump();
            }
        });
        let link = drawer
            .recv()
            .unwrap_or_else(|e| panic!("the stand-in broker starts: {e}"));
        (
            Remote {
                jobs: RefCell::new(Some(jobs)),
                thread: RefCell::new(Some(thread)),
            },
            link,
        )
    }

    /// Serves the requests the drawer has sent, runs `f` on the stand-in,
    /// then a turn of the stand-in's loop, and returns what `f` gave. The
    /// orders the turn sends are on the socket when this returns.
    pub(crate) fn with<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut StandIn) -> R + Send + 'static,
    ) -> R {
        let (give, take) = std::sync::mpsc::channel();
        let job: Job = Box::new(move |s| {
            s.pump();
            let r = f(s);
            s.pump();
            let _ = give.send(r);
        });
        self.jobs
            .borrow()
            .as_ref()
            .unwrap_or_else(|| panic!("the stand-in broker runs"))
            .send(job)
            .unwrap_or_else(|e| panic!("the stand-in broker takes a job: {e}"));
        take.recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("the stand-in broker answers: {e}"))
    }

    /// Ends the stand-in broker, which closes its end of the socket.
    pub(crate) fn stop(&self) {
        self.jobs.borrow_mut().take();
        if let Some(thread) = self.thread.borrow_mut().take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.stop();
    }
}
