//! The descriptors that the drawing process might send the root process.
//!
//! The drawer never has a reason to send a descriptor, and one it sends
//! can hang the root process. Closing a file can block. For example, the
//! last close of a TCP socket with `SO_LINGER` set and data still unsent
//! waits out the whole linger time, which the kernel lets be decades long,
//! and a file on a FUSE file system waits for the file system's flush. A
//! drawer could send such a descriptor with any request and then close its
//! own copy, so the root process's close would be the last one. While the
//! root process's loop waits on that close, it serves no VT switch, and
//! nobody can switch away from vtium's VT.
//!
//! So the root process turns off `SO_PASSRIGHTS` on its end of the socket
//! where the kernel supports it, on Linux 6.16 and later. The kernel then
//! refuses the drawer's send with `EPERM`, and no descriptor reaches the
//! root process. On an older kernel, the root process refuses the request
//! that came with a descriptor and closes the descriptor on a thread of
//! its own, so its loop never waits on the close.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": only the broker sends
//!   descriptors, the broker refuses a request that comes with one, and
//!   the broker never waits for the drawer.

use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::sync::mpsc::{self, Sender};

use super::protocol::Link;

/// `SO_PASSRIGHTS`, which neither rustix nor the libc crate names yet. Its
/// number is the kernel's generic one, apart from SPARC's. Other
/// architectures whose number differs go without it, as an older kernel
/// does.
#[cfg(any(target_arch = "sparc", target_arch = "sparc64"))]
const SO_PASSRIGHTS: Option<libc::c_int> = Some(0x5c);
#[cfg(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "arm",
    target_arch = "aarch64",
    target_arch = "riscv32",
    target_arch = "riscv64",
    target_arch = "loongarch64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x"
))]
const SO_PASSRIGHTS: Option<libc::c_int> = Some(83);
#[cfg(not(any(
    target_arch = "sparc",
    target_arch = "sparc64",
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "arm",
    target_arch = "aarch64",
    target_arch = "riscv32",
    target_arch = "riscv64",
    target_arch = "loongarch64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "s390x"
)))]
const SO_PASSRIGHTS: Option<libc::c_int> = None;

/// The socket between the root process and the drawer: the root process's
/// end, on which the kernel refuses descriptors where it can, and the
/// drawer's end.
pub fn link() -> io::Result<(Link, OwnedFd)> {
    let (link, theirs) = Link::pair()?;
    if let Err(e) = refuse(&link) {
        log::info!(
            target: "broker",
            "This kernel cannot stop vtium's drawing process from sending descriptors to the root process, so the root process closes any it receives on a thread of its own. Linux 6.16 and later can stop them. The kernel's answer: {e}"
        );
    }
    Ok((link, theirs))
}

/// Turns off `SO_PASSRIGHTS` on `link`, so that the kernel refuses any
/// message that carries descriptors to it. A kernel older than Linux 6.16
/// answers `ENOPROTOOPT`.
pub fn refuse(link: &Link) -> io::Result<()> {
    let name = SO_PASSRIGHTS.ok_or_else(|| io::Error::from_raw_os_error(libc::ENOPROTOOPT))?;
    let off: libc::c_int = 0;
    let len = libc::socklen_t::try_from(size_of_val(&off)).map_err(io::Error::other)?;
    #[expect(
        unsafe_code,
        reason = "setsockopt(2) with SO_PASSRIGHTS, which rustix does not wrap"
    )]
    // SAFETY: setsockopt reads `len` bytes from `off`, a live local of
    // exactly that size, and touches no other memory of this process.
    let set = unsafe {
        libc::setsockopt(
            link.as_fd().as_raw_fd(),
            libc::SOL_SOCKET,
            name,
            (&raw const off).cast(),
            len,
        )
    };
    if set != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Closes the descriptors that came with refused requests, on a thread of
/// its own, so that a close that blocks never holds up the broker's loop.
/// The thread starts at the first descriptor, so a kernel that refuses
/// descriptors never starts it.
///
/// The thread inherits the signal mask of the loop that starts it, which
/// blocks the signals the loop reads, so no signal meant for the loop
/// reaches the thread. Descriptors wait in a queue while a close blocks.
/// Each one waiting holds a slot of the root process's descriptor limit,
/// so the queue is bounded by that limit.
#[derive(Debug, Default)]
pub struct Closer {
    queue: Option<Sender<Vec<OwnedFd>>>,
}

impl Closer {
    /// Closes `fds` on the closing thread. When that thread cannot start,
    /// the broker leaves the descriptors open rather than close them on
    /// its loop.
    pub fn close(&mut self, fds: Vec<OwnedFd>) {
        if fds.is_empty() {
            return;
        }
        if self.queue.is_none() {
            match start() {
                Ok(queue) => self.queue = Some(queue),
                Err(e) => log::warn!(
                    target: "broker",
                    "vtium's root process cannot start the thread that closes the descriptors its drawing process sends, so it leaves them open: {e}"
                ),
            }
        }
        let unsent = match &self.queue {
            Some(queue) => queue.send(fds).err().map(|e| e.0),
            None => Some(fds),
        };
        // A close here could block the loop, and the process ends soon
        // after anyway, so the descriptors stay open.
        std::mem::forget(unsent);
    }
}

/// Starts the closing thread, and gives the queue that feeds it.
fn start() -> io::Result<Sender<Vec<OwnedFd>>> {
    let (queue, closing) = mpsc::channel::<Vec<OwnedFd>>();
    std::thread::Builder::new()
        .name("vtium-close".into())
        .spawn(move || {
            for fds in closing {
                drop(fds);
            }
        })?;
    Ok(queue)
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
    use std::io::IoSlice;
    use std::mem::MaybeUninit;

    use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags};

    use crate::broker::protocol::{Got, Order, Request};

    /// Sends `request` with `fd` from the drawer's end, as a hostile
    /// drawer would.
    fn send_with(drawer: &OwnedFd, request: &Request, fd: &OwnedFd) -> rustix::io::Result<usize> {
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        let fds = [fd.as_fd()];
        assert!(control.push(SendAncillaryMessage::ScmRights(&fds)));
        rustix::net::sendmsg(
            drawer,
            &[IoSlice::new(&request.encode())],
            &mut control,
            SendFlags::empty(),
        )
    }

    /// On a kernel that supports `SO_PASSRIGHTS`, the broker's end of the
    /// socket takes no descriptor: the kernel refuses the drawer's send
    /// with `EPERM`, and nothing reaches the broker. The broker still
    /// passes descriptors to the drawer, and the drawer's requests without
    /// one still arrive. A kernel older than Linux 6.16 skips the test.
    #[test]
    fn the_kernel_refuses_descriptors_from_the_drawer() {
        let (link, theirs) = Link::pair().unwrap();
        if let Err(e) = refuse(&link) {
            assert_eq!(e.raw_os_error(), Some(libc::ENOPROTOOPT), "{e}");
            eprintln!("skipped: this kernel has no SO_PASSRIGHTS");
            return;
        }
        let (_keep, file) = Link::pair().unwrap();
        assert_eq!(
            send_with(&theirs, &Request::Ready, &file),
            Err(rustix::io::Errno::PERM)
        );
        assert!(matches!(link.next_request().unwrap(), Got::Nothing));

        let order = Order::Device {
            device: 1,
            opened: Ok(()),
        };
        link.order(&order, Some(file.as_fd())).unwrap();
        let drawer = Link::passed(theirs).unwrap();
        assert!(matches!(
            drawer.next_order().unwrap(),
            Got::Message(Ok((_, Some(_))))
        ));
        drawer.request(&Request::Ready).unwrap();
        assert!(matches!(
            link.next_request().unwrap(),
            Got::Message(Ok(Request::Ready))
        ));
    }
}
