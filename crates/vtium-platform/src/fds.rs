//! Descriptors kept from a child: every descriptor above stdio marked
//! close-on-exec, so a program started afterwards holds only what its
//! spawn hands it. Rust opens its own descriptors close-on-exec; the ones
//! the process inherited may lack the flag.

use std::io;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

/// Marks every descriptor from 3 up close-on-exec, those the process
/// inherited included. It goes through `close_range(2)` with
/// `CLOSE_RANGE_CLOEXEC`, or, on a kernel without that flag (before
/// Linux 5.11) or a filter that refuses the call, through each descriptor
/// `/proc/self/fd` lists. The descriptors stay open and usable here.
pub fn cloexec_above_stdio() -> io::Result<()> {
    cloexec_after(close_range_cloexec)
}

/// The first descriptor a service manager passes, `SD_LISTEN_FDS_START`
/// (sd_listen_fds(3)).
const LISTEN_FDS_START: RawFd = 3;

/// Whether [`take_passed`] has run in this process.
static TAKEN: AtomicBool = AtomicBool::new(false);

/// Takes ownership of the `count` descriptors a service manager passed
/// the process from descriptor 3 up, as systemd passes the descriptors of
/// a service's file-descriptor store when it starts the service again
/// (sd_listen_fds(3), ADR-1401 D13). Each is marked close-on-exec, so no
/// child inherits it. It runs once in a process: a second call fails,
/// and so does a call for a descriptor that is not open, which takes
/// none.
pub fn take_passed(count: usize) -> io::Result<Vec<OwnedFd>> {
    if TAKEN.swap(true, Ordering::SeqCst) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the passed descriptors were already taken",
        ));
    }
    let last = RawFd::try_from(count)
        .ok()
        .and_then(|n| LISTEN_FDS_START.checked_add(n))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let fds: Vec<RawFd> = (LISTEN_FDS_START..last).collect();
    for &fd in &fds {
        #[expect(
            unsafe_code,
            reason = "fcntl(2) on a descriptor the process inherited and nothing owns yet"
        )]
        // SAFETY: F_SETFD sets the descriptor's flags, of which
        // FD_CLOEXEC is the only one, and touches no memory; a number that
        // is not open gives EBADF.
        let done = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        if done != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(fds
        .into_iter()
        .map(|fd| {
            #[expect(
                unsafe_code,
                reason = "taking ownership of a descriptor the process inherited"
            )]
            // SAFETY: the descriptor is open, as its fcntl above showed.
            // The service manager passed it, so nothing in this process
            // opened it or owns it, and TAKEN makes this the only call that
            // takes it: no other owner closes it under this one.
            unsafe {
                OwnedFd::from_raw_fd(fd)
            }
        })
        .collect())
}

/// [`cloexec_above_stdio`] before a child is started, its error naming
/// the marking, so a spawn that fails on it says so.
pub(crate) fn cloexec_before_spawn() -> io::Result<()> {
    // In the unit tests, a test that starts a child marks every descriptor
    // here, so it waits while this file's tests check that a descriptor
    // they opened is still unmarked.
    #[cfg(test)]
    let _one = tests::one_at_a_time();
    cloexec_above_stdio().map_err(named)
}

fn named(e: io::Error) -> io::Error {
    io::Error::new(
        e.kind(),
        format!("cannot mark the inherited descriptors close-on-exec: {e}"),
    )
}

/// `first`, or where it fails, the walk of `/proc/self/fd`.
fn cloexec_after(first: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    first().or_else(|_| cloexec_listed())
}

fn close_range_cloexec() -> io::Result<()> {
    let (first, last): (libc::c_uint, libc::c_uint) = (3, libc::c_uint::MAX);
    #[expect(unsafe_code, reason = "close_range(2), which rustix does not wrap")]
    // SAFETY: with CLOSE_RANGE_CLOEXEC, close_range closes nothing and
    // reads or writes no memory of the process: it sets the close-on-exec
    // flag on each open descriptor in the range, which changes only what
    // a later exec keeps.
    let done = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            first,
            last,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    if done == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn cloexec_listed() -> io::Result<()> {
    // listed first, so the directory's own descriptor is closed before
    // any flag is set
    let listed: Vec<RawFd> = std::fs::read_dir("/proc/self/fd")?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .filter(|&fd| fd > 2)
        .collect();
    for fd in listed {
        #[expect(
            unsafe_code,
            reason = "fcntl(2) on a descriptor this code does not own"
        )]
        // SAFETY: F_SETFD sets the descriptor's flags, of which
        // FD_CLOEXEC is the only one, and touches no memory. A descriptor
        // closed since it was listed, the directory's own among them,
        // gives EBADF; one reopened under the same number by another
        // thread is only marked close-on-exec.
        let done = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        if done != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EBADF) {
                return Err(error);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use rustix::io::{FdFlags, dup, fcntl_getfd};
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::sync::{Mutex, MutexGuard};

    /// Held by each test here, and by [`cloexec_before_spawn`] in every
    /// unit test that starts a child, so no other test's marking reaches a
    /// descriptor before its test sees it unmarked.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    pub(super) fn one_at_a_time() -> MutexGuard<'static, ()> {
        ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn inherited() -> OwnedFd {
        // dup's copy is not close-on-exec, as an inherited descriptor may
        // not be
        let fd = dup(std::fs::File::open("/dev/null").unwrap()).unwrap();
        assert!(!cloexec(fd.as_fd()));
        fd
    }

    fn cloexec(fd: BorrowedFd<'_>) -> bool {
        fcntl_getfd(fd).unwrap().contains(FdFlags::CLOEXEC)
    }

    fn stdio() -> Vec<FdFlags> {
        [
            std::io::stdin().as_fd(),
            std::io::stdout().as_fd(),
            std::io::stderr().as_fd(),
        ]
        .into_iter()
        .map(|fd| fcntl_getfd(fd).unwrap())
        .collect()
    }

    #[test]
    fn close_range_marks_an_inherited_descriptor() {
        let _one = one_at_a_time();
        let fd = inherited();
        let before = stdio();
        if let Err(e) = close_range_cloexec() {
            // a kernel before 5.11 refuses the flag, one before 5.9 the
            // call, and a seccomp filter may refuse either
            let refused = [libc::EINVAL, libc::ENOSYS, libc::EPERM].map(Some);
            assert!(refused.contains(&e.raw_os_error()), "{e}");
            println!("skipped: this kernel refuses close_range's CLOEXEC flag: {e}");
            return;
        }
        assert!(cloexec(fd.as_fd()));
        assert_eq!(stdio(), before);
    }

    #[test]
    fn a_refused_close_range_falls_back_to_the_listing() {
        let _one = one_at_a_time();
        let fd = inherited();
        let before = stdio();
        // as a kernel before 5.11 answers the CLOEXEC flag
        cloexec_after(|| Err(io::Error::from_raw_os_error(libc::EINVAL))).unwrap();
        assert!(cloexec(fd.as_fd()));
        assert_eq!(stdio(), before);
    }

    #[test]
    fn a_failed_marking_is_named() {
        let e = named(io::Error::from_raw_os_error(libc::ENOENT));
        assert!(
            e.to_string()
                .starts_with("cannot mark the inherited descriptors close-on-exec: "),
            "{e}"
        );
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    // The descriptors a service manager passed are taken once: a second
    // call would make a second owner of each. Zero descriptors are taken
    // here, since the test binary's descriptors from 3 up are its own.
    #[test]
    fn the_passed_descriptors_are_taken_once() {
        assert_eq!(take_passed(0).unwrap().len(), 0);
        let again = take_passed(0).map(|fds| fds.len());
        assert_eq!(
            again.map_err(|e| e.kind()),
            Err(io::ErrorKind::AlreadyExists)
        );
    }

    #[test]
    fn a_marked_descriptor_still_works() {
        let _one = one_at_a_time();
        use std::io::{Read, Write};
        let (mut a, mut b) = std::os::unix::net::UnixStream::pair().unwrap();
        let fd = inherited();
        cloexec_above_stdio().unwrap();
        assert!(cloexec(fd.as_fd()));
        a.write_all(b"x").unwrap();
        let mut byte = [0];
        b.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"x");
    }
}
