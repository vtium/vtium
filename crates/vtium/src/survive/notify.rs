//! systemd's file-descriptor store, reached through the unit's
//! notification socket (sd_notify(3), sd_pid_notify_with_fds(3)).
//!
//! The unit enables the store with `FileDescriptorStoreMax=`, and systemd
//! then sets `FDSTORE` to that number and `NOTIFY_SOCKET` to its socket in
//! vtium's environment. vtium stores a master by sending the datagram
//! `FDSTORE=1`, `FDNAME=<name>` and `FDPOLL=0` with the master attached,
//! and drops one with `FDSTOREREMOVE=1` and `FDNAME=<name>`. Without
//! `FDPOLL=0`, systemd would close a stored master as soon as the master
//! reports a hangup. vtium decides when a session has ended, by its
//! program's exit, and drops each master itself then, so a hangup the
//! session lives through, such as one `login` may cause when it calls
//! vhangup(2) on its terminal, never costs the session its copy.
//!
//! When systemd starts vtium again, it passes the stored masters from
//! descriptor 3 up, with `LISTEN_PID`, `LISTEN_FDS` and `LISTEN_FDNAMES`
//! (sd_listen_fds(3)), which [`passed`] reads.
//!
//! vtium writes the datagrams itself, so it links no libsystemd.

use std::ffi::{OsStr, OsString};
use std::io::{self, IoSlice};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;

use rustix::net::{
    AddressFamily, SendAncillaryBuffer, SendAncillaryMessage, SendFlags, SocketAddrUnix,
    SocketFlags, SocketType, sendmsg_addr, socket_with,
};

use super::storable;

/// The unit's notification socket, while the unit enables the store.
pub struct Store {
    socket: OwnedFd,
    addr: SocketAddrUnix,
    /// How many descriptors the store takes, `FDSTORE`.
    capacity: usize,
}

/// The value of `name` in `env`.
fn var<'a>(env: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    env.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_os_str())
}

impl Store {
    /// The store `env` names: `NOTIFY_SOCKET`, with `FDSTORE` above zero,
    /// which systemd sets only when the unit enables the store. `None`
    /// without either, or when the socket cannot be opened, which is
    /// logged.
    pub fn from_env(env: &[(OsString, OsString)]) -> Option<Store> {
        let capacity: usize = var(env, "FDSTORE")?.to_str()?.parse().ok()?;
        if capacity == 0 {
            return None;
        }
        let path = var(env, "NOTIFY_SOCKET")?;
        match Store::open(path.as_bytes(), capacity) {
            Ok(store) => Some(store),
            Err(e) => {
                log::warn!(
                    "cannot reach systemd's notification socket, so the sessions end if vtium restarts: {e}"
                );
                None
            }
        }
    }

    /// The store of `capacity` descriptors behind the notification socket
    /// at `path`: a file's path, or, starting `@`, a name in the abstract
    /// namespace.
    pub fn open(path: &[u8], capacity: usize) -> io::Result<Store> {
        let addr = match path.strip_prefix(b"@") {
            Some(name) => SocketAddrUnix::new_abstract_name(name)?,
            None if path.starts_with(b"/") => SocketAddrUnix::new(path)?,
            None => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
        };
        let socket = socket_with(
            AddressFamily::UNIX,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC,
            None,
        )?;
        Ok(Store {
            socket,
            addr,
            capacity,
        })
    }

    /// How many descriptors the store takes.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Stores a copy of `fd` under `name`.
    pub fn keep(&self, name: &str, fd: BorrowedFd<'_>) -> io::Result<()> {
        self.send(
            &format!("FDSTORE=1\nFDNAME={name}\nFDPOLL=0\n"),
            name,
            Some(fd),
        )
    }

    /// Closes the copy stored under `name`.
    pub fn forget(&self, name: &str) -> io::Result<()> {
        self.send(&format!("FDSTOREREMOVE=1\nFDNAME={name}\n"), name, None)
    }

    fn send(&self, text: &str, name: &str, fd: Option<BorrowedFd<'_>>) -> io::Result<()> {
        if name.is_empty() || !storable(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("systemd takes no descriptor named {name:?}"),
            ));
        }
        let fds: Vec<BorrowedFd<'_>> = fd.into_iter().collect();
        let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        if !fds.is_empty() && !control.push(SendAncillaryMessage::ScmRights(&fds)) {
            return Err(io::Error::from(io::ErrorKind::OutOfMemory));
        }
        let sent = sendmsg_addr(
            &self.socket,
            &self.addr,
            &[IoSlice::new(text.as_bytes())],
            &mut control,
            SendFlags::NOSIGNAL,
        )?;
        if sent != text.len() {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        Ok(())
    }
}

/// The descriptors systemd passed vtium at its start, each with its name:
/// none unless `LISTEN_PID` is vtium's own `pid`, and `LISTEN_FDS` of
/// them from descriptor 3 up, which `take` takes. A name missing from
/// `LISTEN_FDNAMES` is `unknown`, as sd_listen_fds_with_names(3) gives it.
/// A count or a pid that cannot be read takes nothing, which is logged.
pub fn passed(
    env: &[(OsString, OsString)],
    pid: u32,
    take: impl FnOnce(usize) -> io::Result<Vec<OwnedFd>>,
) -> Vec<(String, OwnedFd)> {
    let number = |name| var(env, name).and_then(|v| v.to_str()?.parse::<u64>().ok());
    let Some(count) = number("LISTEN_FDS") else {
        return Vec::new();
    };
    if number("LISTEN_PID") != Some(u64::from(pid)) {
        log::warn!("systemd passed descriptors to another process, so vtium takes none of them");
        return Vec::new();
    }
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    let fds = match take(count) {
        Ok(fds) => fds,
        Err(e) => {
            log::warn!("cannot take the {count} descriptors systemd passed: {e}");
            return Vec::new();
        }
    };
    let names = var(env, "LISTEN_FDNAMES")
        .and_then(OsStr::to_str)
        .unwrap_or("");
    let mut names = names.split(':').filter(|_| !names.is_empty());
    fds.into_iter()
        .map(|fd| (names.next().unwrap_or("unknown").to_owned(), fd))
        .collect()
}
