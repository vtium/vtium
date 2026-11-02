//! The one module of the helper that holds `unsafe` (ADR-1101 D1), which a
//! reviewer of any change to it audits line by line: taking the socket
//! vtium put on descriptor 3, asking whether a descriptor is open, and
//! forking a worker.
//!
//! Every Rust binding of `fork` is `unsafe`, since a child forked from a
//! process with threads may call only what is safe after a fork. The
//! helper has one thread, which [`one_thread`] checks before anything is
//! forked, so its child may call anything; the child closes the
//! descriptors it must not hold and then returns to safe Rust.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};

use rustix::fs::FileType;
use rustix::process::Pid;

use crate::bounds::SOCKET_FD;

/// Which side of a fork returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Forked {
    Child,
    Parent(Pid),
}

/// The socket vtium put on descriptor 3, or `None` when descriptor 3 is
/// not open or is no socket.
pub fn socket() -> Option<OwnedFd> {
    if !is_open(SOCKET_FD) {
        return None;
    }
    #[expect(
        unsafe_code,
        reason = "taking ownership of the descriptor vtium handed over"
    )]
    // SAFETY: descriptor 3 is open, as just checked, and nothing else in
    // this process owns it: vtium puts the socket there before the exec,
    // the Rust runtime opens only descriptors 0 to 2 when they are closed,
    // and this is the helper's first call, made once.
    let fd = unsafe { OwnedFd::from_raw_fd(SOCKET_FD) };
    let st = rustix::fs::fstat(&fd).ok()?;
    (FileType::from_raw_mode(st.st_mode) == FileType::Socket).then_some(fd)
}

/// Whether descriptor `fd` is open in this process.
pub fn is_open(fd: RawFd) -> bool {
    #[expect(
        unsafe_code,
        reason = "fcntl(2) on a descriptor number this process may not hold"
    )]
    // SAFETY: F_GETFD reads the descriptor's flags and touches no memory;
    // a number that is not open answers EBADF.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    flags >= 0
}

/// Whether this process has one thread, as [`fork`] requires: the
/// entries of `/proc/self/task`.
pub fn one_thread() -> bool {
    std::fs::read_dir("/proc/self/task").is_ok_and(|tasks| tasks.count() == 1)
}

/// Forks the helper. The child closes each descriptor of `close_in_child`
/// before it returns, such as the socket to vtium and the parent's end of
/// the worker's channel; the parent keeps them.
///
/// The caller must have checked [`one_thread`] once, and must start no
/// thread: the helper never does.
pub fn fork(close_in_child: &[RawFd]) -> std::io::Result<Forked> {
    #[expect(unsafe_code, reason = "fork(2), which every Rust binding marks unsafe")]
    // SAFETY: the helper has one thread, which `one_thread` checked before
    // the first fork and which no code of the helper changes, so the
    // child holds no lock another thread took and may call anything.
    let pid = unsafe { libc::fork() };
    match pid {
        -1 => Err(std::io::Error::last_os_error()),
        0 => {
            for &fd in close_in_child {
                #[expect(
                    unsafe_code,
                    reason = "closing the child's copy of a descriptor its parent owns"
                )]
                // SAFETY: the child's copy of each descriptor is its own;
                // the `OwnedFd`s that name them belong to the parent's
                // frames, which the child leaves only through
                // `std::process::exit`, so none is dropped or used again.
                unsafe {
                    libc::close(fd);
                }
            }
            Ok(Forked::Child)
        }
        n => Pid::from_raw(n)
            .map(Forked::Parent)
            .ok_or_else(|| std::io::Error::other("fork returned a pid below 1")),
    }
}
