//! Descriptors kept from a child: every descriptor above stdio marked
//! close-on-exec, so a program started afterwards holds only what its
//! spawn hands it. Rust opens its own descriptors close-on-exec; the ones
//! the process inherited may lack the flag.

use std::io;
use std::os::fd::RawFd;

/// Marks every descriptor from 3 up close-on-exec, those the process
/// inherited included. It goes through `close_range(2)` with
/// `CLOSE_RANGE_CLOEXEC`, or, on a kernel without that flag (before
/// Linux 5.11) or a filter that refuses the call, through each descriptor
/// `/proc/self/fd` lists. The descriptors stay open and usable here.
pub fn cloexec_above_stdio() -> io::Result<()> {
    cloexec_after(close_range_cloexec)
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
pub(crate) mod tests {
    use super::*;
    use rustix::io::{FdFlags, dup, fcntl_getfd};
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::sync::{Mutex, MutexGuard};

    /// Held by each test that marks descriptors or plants one unmarked,
    /// the spawns' tests in other modules included, so another test's
    /// marking cannot reach a descriptor before its test sees it unmarked.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    pub(crate) fn one_at_a_time() -> MutexGuard<'static, ()> {
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
