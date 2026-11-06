//! The holder, which keeps the sessions' pty masters on a host without
//! systemd's file-descriptor store (ADR-1401 D13), as dtach keeps a
//! session's terminal for a program that detaches from it.
//!
//! When vtium runs as root and finds no store, it starts the holder, its
//! own binary run as `vtium hold`, before it blocks any signal. The holder
//! leads a session of its own, so no signal meant for vtium's session or
//! process group reaches it, and it listens on a socket at
//! `/run/vtium/<tty>/hold` that only root may open; it also checks that
//! whoever connects runs as its own user. vtium binds the socket and hands
//! it to the holder as its standard input, so the socket is ready before
//! the holder runs.
//!
//! vtium sends the holder a copy of each session's master with
//! `keep NAME`, and `forget NAME` when the session ends. A vtium that
//! starts on that VT finds the socket, connects, and sends `take`; the
//! holder answers with `kept NAME` and a copy of each master it holds,
//! then `end`, and goes on holding them. The holder serves one vtium at a
//! time, and when that vtium's connection closes, because it exited or
//! crashed, the holder waits for the next one. It exits once a connection
//! closes and it holds no master, so a vtium whose sessions have all ended
//! leaves no holder behind. A vtium that is stopped and never started
//! again leaves its sessions running in the holder until someone ends the
//! holder, which hangs them up.
//!
//! The messages are packets on a `SOCK_SEQPACKET` socket, each a line of
//! text with at most one descriptor attached.

use std::ffi::OsString;
use std::io::{self, IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use rustix::net::{
    AddressFamily, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags, SocketAddrUnix, SocketFlags, SocketType, accept_with, bind,
    connect, listen, recvmsg, sendmsg, socket_with, sockopt,
};
use rustix::process::geteuid;

use super::{NAME_MAX, Name};

/// The most masters the holder keeps, as many as the most consoles vtium
/// opens (ADR-1401 D10), which the unit's `FileDescriptorStoreMax=` also
/// sets for systemd's store.
pub const MAX_KEPT: usize = 50;

/// How long a starting vtium waits for the holder's answer to `take`. The
/// bound only keeps a holder that has stopped answering from hanging
/// vtium's start; the holder answers at once.
const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// The longest packet either side sends: a word, a space and a name.
const PACKET_MAX: usize = NAME_MAX + 16;

/// The holder's socket in the VT's run directory `dir`.
pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join("hold")
}

fn seqpacket() -> io::Result<OwnedFd> {
    Ok(socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )?)
}

/// Whether the peer on `conn` runs as this process's user.
fn same_user(conn: BorrowedFd<'_>) -> bool {
    sockopt::socket_peercred(conn).is_ok_and(|c| c.uid == geteuid())
}

/// Sends `text`, with `fd` attached when there is one.
fn send(conn: BorrowedFd<'_>, text: &str, fd: Option<BorrowedFd<'_>>) -> io::Result<()> {
    let fds: Vec<BorrowedFd<'_>> = fd.into_iter().collect();
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !fds.is_empty() && !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err(io::Error::from(io::ErrorKind::OutOfMemory));
    }
    sendmsg(
        conn,
        &[IoSlice::new(text.as_bytes())],
        &mut control,
        SendFlags::NOSIGNAL,
    )?;
    Ok(())
}

/// The next packet's text and the descriptors attached to it; `None` once
/// the peer has closed the connection.
fn receive(conn: BorrowedFd<'_>) -> io::Result<Option<(String, Vec<OwnedFd>)>> {
    let mut bytes = [0_u8; PACKET_MAX];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(4))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let got = recvmsg(
        conn,
        &mut [IoSliceMut::new(&mut bytes)],
        &mut control,
        RecvFlags::CMSG_CLOEXEC,
    )?;
    let mut fds = Vec::new();
    for message in control.drain() {
        if let RecvAncillaryMessage::ScmRights(received) = message {
            fds.extend(received);
        }
    }
    if got.bytes == 0 && fds.is_empty() {
        return Ok(None);
    }
    let text = bytes
        .get(..got.bytes)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    Ok(Some((text, fds)))
}

/// The holder's side: serves vtium after vtium on `listener` until a
/// connection closes with no master held. A peer that runs as another
/// user is turned away.
pub fn serve(listener: OwnedFd) -> io::Result<()> {
    let mut held: Vec<(String, OwnedFd)> = Vec::new();
    loop {
        let conn = accept_with(&listener, SocketFlags::CLOEXEC)?;
        if !same_user(conn.as_fd()) {
            continue;
        }
        serve_one(conn.as_fd(), &mut held);
        if held.is_empty() {
            return Ok(());
        }
    }
}

/// Serves one vtium on `conn` until it closes the connection.
fn serve_one(conn: BorrowedFd<'_>, held: &mut Vec<(String, OwnedFd)>) {
    loop {
        let (text, mut fds) = match receive(conn) {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(_) => return,
        };
        let (word, name) = text.split_once(' ').unwrap_or((text.as_str(), ""));
        match word {
            "keep" if fds.len() == 1 && Name::parse(name).is_some() => {
                held.retain(|(n, _)| n != name);
                if held.len() < MAX_KEPT
                    && let Some(fd) = fds.pop()
                {
                    held.push((name.to_owned(), fd));
                }
            }
            "forget" => held.retain(|(n, _)| n != name),
            "take" => {
                for (name, fd) in held.iter() {
                    if send(conn, &format!("kept {name}"), Some(fd.as_fd())).is_err() {
                        return;
                    }
                }
                if send(conn, "end", None).is_err() {
                    return;
                }
            }
            // Anything else is not vtium's, and is dropped with any
            // descriptor it carried.
            _ => {}
        }
    }
}

/// vtium's side of the holder.
pub struct Client {
    conn: OwnedFd,
}

impl Client {
    /// Connects to the holder at `path`, which must run as vtium's user.
    pub fn connect(path: &Path) -> io::Result<Client> {
        let conn = seqpacket()?;
        connect(&conn, &SocketAddrUnix::new(path)?)?;
        if !same_user(conn.as_fd()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is held by another user", path.display()),
            ));
        }
        Ok(Client { conn })
    }

    /// Takes a copy of every master the holder keeps, with its name.
    pub fn take(&self) -> io::Result<Vec<(String, OwnedFd)>> {
        sockopt::set_socket_timeout(&self.conn, sockopt::Timeout::Recv, Some(ANSWER_WAIT))?;
        send(self.conn.as_fd(), "take", None)?;
        let mut kept = Vec::new();
        loop {
            let Some((text, mut fds)) = receive(self.conn.as_fd())? else {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            };
            if text == "end" {
                break;
            }
            if let (Some(name), Some(fd)) = (text.strip_prefix("kept "), fds.pop())
                && kept.len() < MAX_KEPT
            {
                kept.push((name.to_owned(), fd));
            }
        }
        sockopt::set_socket_timeout(&self.conn, sockopt::Timeout::Recv, None)?;
        Ok(kept)
    }

    /// Gives the holder a copy of `fd` under `name`.
    pub fn keep(&self, name: &str, fd: BorrowedFd<'_>) -> io::Result<()> {
        send(self.conn.as_fd(), &format!("keep {name}"), Some(fd))
    }

    /// Tells the holder to close its copy kept under `name`.
    pub fn forget(&self, name: &str) -> io::Result<()> {
        send(self.conn.as_fd(), &format!("forget {name}"), None)
    }
}

/// Makes the VT's run directory `dir`, owned by vtium's user and writable
/// by no one else (ADR-1401 D10), or checks the one there.
fn run_dir(dir: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(dir)?;
    let meta = std::fs::symlink_metadata(dir)?;
    let mine = meta.uid() == geteuid().as_raw();
    if !meta.is_dir() || !mine || meta.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a directory that only vtium's user can write",
                dir.display()
            ),
        ));
    }
    Ok(())
}

/// Connects to the holder of the VT whose run directory is `dir`, or,
/// when none answers there, starts one by running `exe hold`. Returns the
/// connection and the masters the holder kept from an earlier vtium.
pub fn start(dir: &Path, exe: &Path) -> io::Result<(Client, Vec<(String, OwnedFd)>)> {
    run_dir(dir)?;
    let path = socket_path(dir);
    match Client::connect(&path) {
        Ok(client) => {
            let kept = client.take()?;
            return Ok((client, kept));
        }
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Err(e),
        // No holder: none was started, or the last one exited and left
        // its socket.
        Err(_) => {}
    }
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let listener = seqpacket()?;
    bind(&listener, &SocketAddrUnix::new(&path)?)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listen(&listener, 4)?;
    vtium_platform::cloexec_above_stdio()?;
    Command::new(exe)
        .arg("hold")
        .stdin(Stdio::from(listener))
        .stdout(Stdio::null())
        .spawn()?;
    let client = Client::connect(&path)?;
    Ok((client, Vec::new()))
}

/// `vtium hold`, which vtium runs to start the holder: serves the socket
/// on standard input in a session of its own. Returns the exit status, or
/// `None` for any other command line.
pub fn subcommand(args: &[OsString]) -> Option<i32> {
    if args.first().is_none_or(|a| a != "hold") {
        return None;
    }
    if args.len() > 1 {
        log::error!("vtium hold takes no arguments; vtium starts it on its own");
        return Some(2);
    }
    // A session of its own, so that no signal for vtium's session or
    // process group reaches the holder.
    if let Err(e) = rustix::process::setsid() {
        log::warn!("the holder stays in vtium's session: {e}");
    }
    let listener = match io::stdin().as_fd().try_clone_to_owned() {
        Ok(fd) => fd,
        Err(e) => {
            log::error!("the holder has no socket: {e}");
            return Some(1);
        }
    };
    match serve(listener) {
        Ok(()) => Some(0),
        Err(e) => {
            log::error!("the holder stops: {e}");
            Some(1)
        }
    }
}
