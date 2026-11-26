//! The kernel's messages, which vtium's root process relays to its drawing
//! process.
//!
//! The drawing process cannot open `/dev/kmsg` under
//! `kernel.dmesg_restrict`. A descriptor of `/dev/kmsg` that root opened
//! would let it read the kernel's messages, but it would also let it seek
//! back to the oldest record the kernel keeps, which `dmesg_restrict`
//! keeps from an account without root. So the root process keeps
//! `/dev/kmsg` to itself, opened at its end, and writes each new record to
//! a socket whose other end the drawing process reads. The socket keeps
//! the records apart, so each read gives one record, as a read of
//! `/dev/kmsg` does, and it cannot seek. The root process never reads the
//! socket and never waits on it: a record the drawing process has no room
//! for is dropped, as the kernel drops records nobody read in time.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker relays the kernel's
//!   messages instead of passing its descriptor of `/dev/kmsg`.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::net::{AddressFamily, SendFlags, Shutdown, SocketFlags, SocketType};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, Mode, PostAction};
use vtium_platform::event_loop::Loop;

use super::watch::Seen;

/// The root process's side of the relay: `/dev/kmsg` and its end of the
/// socket.
pub struct Relay {
    kmsg: OwnedFd,
    ours: OwnedFd,
}

/// The relay from `kmsg`, a descriptor of `/dev/kmsg` opened at its end,
/// and the drawing process's end of its socket, which the drawing process
/// can only read.
pub fn relay(kmsg: OwnedFd) -> io::Result<(Relay, OwnedFd)> {
    let (ours, theirs) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )?;
    // neither end carries anything back to the root process
    rustix::net::shutdown(&ours, Shutdown::Read)?;
    rustix::net::shutdown(&theirs, Shutdown::Write)?;
    Ok((Relay { kmsg, ours }, theirs))
}

impl Relay {
    /// Passes on each new record from the loop behind `lp` from now on.
    pub fn listen(self, lp: &Loop<Seen>) -> io::Result<()> {
        let Relay { kmsg, ours } = self;
        lp.handle()
            .insert_source(
                Generic::new(kmsg, Interest::READ, Mode::Level),
                move |_, kmsg, _| {
                    Ok(if pass_on(kmsg.as_fd(), ours.as_fd()) {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
            .map(drop)
            .map_err(|e| io::Error::other(e.error))
    }
}

/// Passes on each record waiting in `kmsg` to `socket`, and tells whether
/// the drawing process still reads the socket.
fn pass_on(kmsg: BorrowedFd<'_>, socket: BorrowedFd<'_>) -> bool {
    match vtium::kernel::drain(kmsg) {
        Ok(records) => send_all(socket, &records),
        Err(e) => {
            log::debug!(target: "broker", "vtium's root process cannot read the kernel's messages: {e}");
            true
        }
    }
}

/// Sends each of `records` as one message on `socket`, and tells whether
/// the drawing process still reads it. A record the socket has no room for
/// is dropped.
fn send_all(socket: BorrowedFd<'_>, records: &[Vec<u8>]) -> bool {
    let flags = SendFlags::DONTWAIT | SendFlags::NOSIGNAL;
    for record in records {
        match rustix::net::send(socket, record, flags) {
            Ok(_) | Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// A stand-in for `/dev/kmsg` opened at its end: a memory file that
    /// holds an older record, read from past it, and that a reader can
    /// seek back in, as a reader of `/dev/kmsg` can. Each record the test
    /// logs afterwards is written past the end, where the next read finds
    /// it.
    fn stand_in() -> OwnedFd {
        let kmsg = rustix::fs::memfd_create("kmsg", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
        rustix::io::write(&kmsg, b"6,0,0,-;an older record\n").unwrap();
        kmsg
    }

    fn log(kmsg: &OwnedFd, record: &[u8]) {
        let end = rustix::fs::fstat(kmsg).unwrap().st_size;
        rustix::io::pwrite(kmsg, record, u64::try_from(end).unwrap()).unwrap();
    }

    /// A new record reaches the drawing process's end, and that end can
    /// neither seek back to the older record nor send anything to the
    /// root process.
    #[test]
    fn the_drawer_reads_each_new_record_and_cannot_go_back() {
        let kmsg = stand_in();
        let (relay, theirs) = relay(kmsg.try_clone().unwrap()).unwrap();
        log(&kmsg, b"4,1,0,-;one\n");
        assert!(pass_on(relay.kmsg.as_fd(), relay.ours.as_fd()));
        assert_eq!(
            vtium::kernel::drain(theirs.as_fd()).unwrap(),
            [b"4,1,0,-;one\n".to_vec()]
        );
        assert_eq!(
            rustix::fs::seek(&theirs, rustix::fs::SeekFrom::Start(0)).err(),
            Some(rustix::io::Errno::SPIPE),
            "the drawer's end seeks back to older records"
        );
        assert!(
            rustix::net::send(&theirs, b"x", SendFlags::DONTWAIT | SendFlags::NOSIGNAL).is_err(),
            "the drawer's end sends to the root process"
        );
        // once the drawer has closed its end, the relay ends
        drop(theirs);
        log(&kmsg, b"4,2,0,-;two\n");
        assert!(!pass_on(relay.kmsg.as_fd(), relay.ours.as_fd()));
    }
}
