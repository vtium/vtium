//! The tests' stand-in for libseat and the seat manager behind it.
//!
//! It refuses what seatd 0.9.3 refuses, with seatd's errno, so a test that
//! passes against it asks nothing of the seat that the real one would
//! refuse (seatd/seat.c, seatd/client.c):
//!
//! - a device opened while the client is not enabled: `EPERM`;
//! - a disable acknowledged when none was asked: `EBUSY`;
//! - a session switch asked while the client is not enabled: `EPERM`;
//! - a device closed that is not open: `ENOENT`;
//! - anything on a seat closed, or once the connection failed: `ENOTCONN`,
//!   as libseat's seatd backend answers.
//!
//! With `logind` set it answers as libseat's logind backend instead, whose
//! acknowledgement of a disable always succeeds.
//!
//! The seat manager's messages reach the client as libseat's do: each one
//! makes the connection's descriptor readable, and the next dispatch calls
//! the listener with it.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rustix::io::Errno;

use super::{Inbox, Manager, Seat, Told};

/// The seat manager's side, which the test drives and reads.
#[derive(Debug, Default)]
pub(crate) struct Server {
    /// The answers the next opens of the seat get, in order, before
    /// [`Server::held`] is looked at: `ENOSYS` where no seat manager answers.
    pub(crate) open_answers: VecDeque<Errno>,
    /// The next opens that find the seat held by another client, which
    /// seatd 0.9.3 refuses by hanging up without an error
    /// (seatd/client.c, `handle_open_seat` and `client_handle_connection`).
    /// libseat then answers `EPIPE` when `LIBSEAT_BACKEND` names seatd
    /// ([`Server::backend_named`]), and otherwise `ENOSYS`, once its other
    /// backends have failed too (libseat/libseat.c, `libseat_open_seat`).
    pub(crate) held: usize,
    /// `LIBSEAT_BACKEND` names seatd.
    pub(crate) backend_named: bool,
    /// The opens of the seat asked so far.
    pub(crate) opens: usize,
    /// Whether seatd's socket accepts a connection.
    pub(crate) seatd: bool,
    /// Whether an open enables the client at once, as seatd does when the
    /// client's VT is shown and no other client holds the seat.
    pub(crate) enables_at_open: bool,
    /// The answers the next opens of each device get, in order.
    pub(crate) device_answers: BTreeMap<PathBuf, VecDeque<Errno>>,
    /// The opens of each device asked so far.
    pub(crate) device_opens: BTreeMap<PathBuf, usize>,
    /// The devices open, by id.
    pub(crate) devices: BTreeMap<u32, PathBuf>,
    /// The inode of the open file each device was last opened as, which
    /// the descriptor given to the client shares.
    pub(crate) files: BTreeMap<PathBuf, u64>,
    next_id: u32,
    /// The sessions the client asked to switch to.
    pub(crate) switches: Vec<u16>,
    /// The disables the client acknowledged.
    pub(crate) acknowledged: usize,
    /// The client's state, as seatd keeps it.
    pub(crate) client: Client,
    /// The stand-in answers as libseat's logind backend: an acknowledged
    /// disable is never refused, since its `disable_seat` does nothing
    /// (libseat/backend/logind.c), and the session's enable can come at
    /// any time ([`StandIn::tell`]).
    pub(crate) logind: bool,
    /// A dispatch fails from here on, as on a connection seatd closed.
    pub(crate) broken: bool,
    /// What waits to be dispatched, and the seat manager's end of the
    /// connection, written once for each.
    queued: VecDeque<Told>,
    end: Option<UnixStream>,
}

/// The client's state on the seat, seatd's `client->state`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Client {
    #[default]
    Closed,
    /// Opened and not yet enabled.
    New,
    Active,
    PendingDisable,
    Disabled,
}

/// A handle on the stand-in, shared by the test, the manager and the seat.
#[derive(Clone, Debug, Default)]
pub(crate) struct StandIn(pub(crate) Rc<RefCell<Server>>);

impl StandIn {
    /// A seat manager that enables its client at the open.
    pub(crate) fn new() -> StandIn {
        let s = StandIn::default();
        s.0.borrow_mut().enables_at_open = true;
        s
    }

    /// Sends `told` whatever the client's state, as libseat's logind
    /// backend does on each change of its session's `Active`, so an enable
    /// can come while a disable waits for its acknowledgement.
    pub(crate) fn tell(&self, told: Told) {
        let mut s = self.0.borrow_mut();
        s.client = match told {
            Told::Enable => Client::Active,
            Told::Disable => Client::PendingDisable,
        };
        s.send(told);
    }

    /// The seat manager enables the client, as seatd does when its VT is
    /// shown and the seat is free.
    pub(crate) fn enable(&self) {
        let mut s = self.0.borrow_mut();
        if matches!(s.client, Client::New | Client::Disabled) {
            s.client = Client::Active;
            s.send(Told::Enable);
        }
    }

    /// The seat manager asks the client for the seat, as on a switch away.
    pub(crate) fn disable(&self) {
        let mut s = self.0.borrow_mut();
        if s.client == Client::Active {
            s.client = Client::PendingDisable;
            s.send(Told::Disable);
        }
    }
}

impl Server {
    fn send(&mut self, told: Told) {
        self.queued.push_back(told);
        if let Some(end) = &mut self.end {
            let _ = end.write_all(&[1]);
        }
    }

    fn connected(&self) -> io::Result<()> {
        if self.client == Client::Closed || self.broken {
            Err(Errno::NOTCONN.into())
        } else {
            Ok(())
        }
    }
}

/// The client's end of an open seat.
#[derive(Debug)]
pub(crate) struct StandInSeat {
    server: StandIn,
    inbox: Inbox,
    conn: UnixStream,
}

/// A device the stand-in opened: its id, and the open file both the seat
/// and the owner hold.
#[derive(Debug)]
pub(crate) struct StandInDevice {
    id: u32,
    _file: OwnedFd,
}

impl Manager for StandIn {
    type Seat = StandInSeat;

    fn open(&mut self, inbox: Inbox) -> io::Result<StandInSeat> {
        let mut s = self.0.borrow_mut();
        s.opens = s.opens.saturating_add(1);
        if let Some(e) = s.open_answers.pop_front() {
            return Err(e.into());
        }
        if s.held > 0 {
            s.held = s.held.saturating_sub(1);
            return Err(if s.backend_named {
                Errno::PIPE
            } else {
                Errno::NOSYS
            }
            .into());
        }
        let (conn, end) = UnixStream::pair()?;
        conn.set_nonblocking(true)?;
        s.end = Some(end);
        s.client = Client::New;
        // seatd's backend reads the enable that answers the open while it
        // waits for the open's reply, and calls the listener inside
        // libseat_open_seat (libseat/backend/seatd.c, `open_seat`), so the
        // enable is in the inbox before the open returns, and nothing makes
        // the descriptor readable for it.
        if s.enables_at_open {
            s.client = Client::Active;
            inbox.borrow_mut().push_back(Told::Enable);
        }
        Ok(StandInSeat {
            server: self.clone(),
            inbox,
            conn,
        })
    }

    fn seatd_listens(&self) -> bool {
        self.0.borrow().seatd
    }

    fn backend_named(&self) -> Option<std::ffi::OsString> {
        self.0.borrow().backend_named.then(|| "seatd".into())
    }

    fn logind_runs(&self) -> bool {
        self.0.borrow().logind
    }
}

impl Seat for StandInSeat {
    type Device = StandInDevice;

    fn fd(&mut self) -> io::Result<OwnedFd> {
        Ok(OwnedFd::from(self.conn.try_clone()?))
    }

    /// Everything queued, never waiting.
    fn dispatch(&mut self) -> io::Result<()> {
        self.server.0.borrow().connected()?;
        let mut buf = [0; 64];
        loop {
            match self.conn.read(&mut buf) {
                Ok(0) => return Err(Errno::PIPE.into()),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        let queued: Vec<Told> = self.server.0.borrow_mut().queued.drain(..).collect();
        self.inbox.borrow_mut().extend(queued);
        Ok(())
    }

    fn disable(&mut self) -> io::Result<()> {
        let mut s = self.server.0.borrow_mut();
        s.connected()?;
        if s.logind {
            s.acknowledged = s.acknowledged.saturating_add(1);
            return Ok(());
        }
        if s.client != Client::PendingDisable {
            return Err(Errno::BUSY.into());
        }
        s.client = Client::Disabled;
        s.acknowledged = s.acknowledged.saturating_add(1);
        Ok(())
    }

    fn open_device(&mut self, path: &Path) -> io::Result<(StandInDevice, OwnedFd)> {
        let mut s = self.server.0.borrow_mut();
        s.connected()?;
        let opens = s.device_opens.entry(path.to_owned()).or_default();
        *opens = opens.saturating_add(1);
        if s.client != Client::Active {
            return Err(Errno::PERM.into());
        }
        if let Some(e) = s.device_answers.get_mut(path).and_then(VecDeque::pop_front) {
            return Err(e.into());
        }
        let (file, _) = UnixStream::pair()?;
        let file = OwnedFd::from(file);
        let owner = file.try_clone()?;
        let inode = std::fs::File::from(file.try_clone()?).metadata()?.ino();
        s.files.insert(path.to_owned(), inode);
        let id = s.next_id;
        s.next_id = s.next_id.saturating_add(1);
        s.devices.insert(id, path.to_owned());
        Ok((StandInDevice { id, _file: file }, owner))
    }

    fn close_device(&mut self, device: StandInDevice) -> io::Result<()> {
        let mut s = self.server.0.borrow_mut();
        s.connected()?;
        s.devices
            .remove(&device.id)
            .map(|_| ())
            .ok_or_else(|| Errno::NOENT.into())
    }

    fn switch_session(&mut self, session: u16) -> io::Result<()> {
        let mut s = self.server.0.borrow_mut();
        s.connected()?;
        if s.client != Client::Active {
            return Err(Errno::PERM.into());
        }
        s.switches.push(session);
        Ok(())
    }

    fn name(&mut self) -> String {
        "seat0".to_owned()
    }
}

impl Drop for StandInSeat {
    /// libseat_close_seat: the client leaves the seat.
    fn drop(&mut self) {
        let mut s = self.server.0.borrow_mut();
        s.client = Client::Closed;
        s.end = None;
    }
}
