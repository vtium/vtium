//! Real-time signals, `SIGRTMIN+n`: taken through a signalfd of their own
//! in the loop, and sent to another process through its pidfd. calloop's
//! signal source names the standard signals alone, and rustix sends a
//! real-time signal only through an unsafe constructor whose contract
//! forbids sending it, so both go through libc here. `SIGRTMIN` is libc's: 34 under glibc,
//! which keeps the kernel's 32 and 33 for its threads.
//!
//! A signal taken here is blocked in the thread that makes the source, as
//! the loop's own are ([`crate::event_loop`]), so it is made before any
//! other thread starts; the pty's child clears the mask before it runs.

use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use calloop::generic::Generic;
use calloop::{EventSource, Interest, Mode, Poll, PostAction, Readiness, Token, TokenFactory};
use rustix::process::{Pid, PidfdFlags, pidfd_open};

/// `SIGRTMIN+offset`, or `None` past `SIGRTMAX`.
fn number(offset: u8) -> Option<libc::c_int> {
    let n = libc::SIGRTMIN().checked_add(libc::c_int::from(offset))?;
    (n <= libc::SIGRTMAX()).then_some(n)
}

fn invalid(offset: u8) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("SIGRTMIN+{offset} is past SIGRTMAX"),
    )
}

/// The size of the kernel's `struct signalfd_siginfo`, whose first field
/// is the signal's number.
const SIGINFO: usize = 128;

/// A calloop source for real-time signals: each read gives its offset
/// from `SIGRTMIN`.
#[derive(Debug)]
pub struct RtSignals {
    fd: Generic<OwnedFd>,
}

impl RtSignals {
    /// Takes `SIGRTMIN+n` for each offset n: blocks them in this thread and
    /// reads them from a signalfd.
    pub fn new(offsets: &[u8]) -> io::Result<RtSignals> {
        let mut set = MaybeUninit::<libc::sigset_t>::uninit();
        #[expect(
            unsafe_code,
            reason = "libc's sigset_t is filled only through its sigemptyset"
        )]
        // SAFETY: sigemptyset initializes the set it is given, which then
        // holds no signal.
        let mut set = unsafe {
            libc::sigemptyset(set.as_mut_ptr());
            set.assume_init()
        };
        for &offset in offsets {
            let n = number(offset).ok_or_else(|| invalid(offset))?;
            #[expect(unsafe_code, reason = "libc's sigaddset takes a raw pointer")]
            // SAFETY: the set is initialized and n is a valid signal.
            let added = unsafe { libc::sigaddset(&mut set, n) };
            if added != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[expect(
            unsafe_code,
            reason = "rustix wraps neither pthread_sigmask with real-time signals nor signalfd"
        )]
        // SAFETY: the set is initialized; the old mask is not asked for.
        let blocked = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) };
        if blocked != 0 {
            return Err(io::Error::from_raw_os_error(blocked));
        }
        #[expect(
            unsafe_code,
            reason = "rustix wraps neither pthread_sigmask with real-time signals nor signalfd"
        )]
        // SAFETY: signalfd(-1, ...) makes a new descriptor from an
        // initialized set; a negative return is an error.
        let raw = unsafe { libc::signalfd(-1, &set, libc::SFD_NONBLOCK | libc::SFD_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        #[expect(unsafe_code, reason = "signalfd's descriptor is owned from here")]
        // SAFETY: raw is a new descriptor nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        Ok(RtSignals {
            fd: Generic::new(fd, Interest::READ, Mode::Level),
        })
    }
}

impl EventSource for RtSignals {
    /// The signal's offset from `SIGRTMIN`.
    type Event = u8;
    type Metadata = ();
    type Ret = ();
    type Error = io::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut callback: F,
    ) -> Result<PostAction, Self::Error>
    where
        F: FnMut(u8, &mut ()),
    {
        self.fd.process_events(readiness, token, |_, fd| {
            let mut info = [0u8; SIGINFO];
            loop {
                match rustix::io::read(&**fd, &mut info) {
                    Ok(SIGINFO) => {
                        let [a, b, c, d, ..] = info;
                        let signo = u32::from_ne_bytes([a, b, c, d]);
                        let offset = i64::from(signo).saturating_sub(i64::from(libc::SIGRTMIN()));
                        if let Ok(offset) = u8::try_from(offset) {
                            callback(offset, &mut ());
                        }
                    }
                    Ok(_) | Err(rustix::io::Errno::AGAIN) => break,
                    Err(rustix::io::Errno::INTR) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(PostAction::Continue)
        })
    }

    fn register(&mut self, poll: &mut Poll, factory: &mut TokenFactory) -> calloop::Result<()> {
        self.fd.register(poll, factory)
    }

    fn reregister(&mut self, poll: &mut Poll, factory: &mut TokenFactory) -> calloop::Result<()> {
        self.fd.reregister(poll, factory)
    }

    fn unregister(&mut self, poll: &mut Poll) -> calloop::Result<()> {
        self.fd.unregister(poll)
    }
}

/// A process held by its pidfd, so a signal reaches it, or no process,
/// and never another that took its pid since.
#[derive(Debug)]
pub struct Process(OwnedFd);

impl Process {
    /// Holds process `pid` from here on.
    pub fn open(pid: u32) -> io::Result<Process> {
        let pid = i32::try_from(pid)
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        Ok(Process(pidfd_open(pid, PidfdFlags::empty())?))
    }

    /// Sends the process `SIGRTMIN+offset`. kill(2)'s rule decides who
    /// may: root, or a user whose real or effective id is the process's
    /// real or saved one.
    pub fn signal(&self, offset: u8) -> io::Result<()> {
        let n = number(offset).ok_or_else(|| invalid(offset))?;
        #[expect(
            unsafe_code,
            reason = "rustix sends no real-time signal without an unsafe constructor whose contract forbids sending it"
        )]
        // SAFETY: pidfd_send_signal(2) takes a pidfd this value owns, a
        // signal number, no siginfo and no flags, and touches no memory of
        // this process; an invalid one is an error it returns.
        let sent = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                n,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        };
        if sent == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

/// Raises `SIGRTMIN+offset` in the calling thread alone, as a test does
/// to reach its own loop without signalling any other thread.
pub fn raise(offset: u8) -> io::Result<()> {
    let n = number(offset).ok_or_else(|| invalid(offset))?;
    #[expect(unsafe_code, reason = "libc's pthread_kill takes a raw thread handle")]
    // SAFETY: pthread_self is this live thread; pthread_kill touches no
    // memory and returns an error number.
    let raised = unsafe { libc::pthread_kill(libc::pthread_self(), n) };
    if raised == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(raised))
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use std::time::Duration;

    use super::*;

    #[cfg(target_env = "gnu")]
    #[test]
    fn blank_and_wake_are_35_and_36_under_glibc() {
        assert_eq!(number(1), Some(35));
        assert_eq!(number(2), Some(36));
        assert_eq!(number(u8::MAX), None);
    }

    /// A signal raised in the thread is read from the loop, by its offset;
    /// of several pending, the kernel gives the lowest first.
    #[test]
    fn a_raised_signal_reaches_the_loop_as_its_offset() {
        let mut lp = calloop::EventLoop::<Vec<u8>>::try_new().expect("a loop");
        let source = RtSignals::new(&[1, 2]).expect("the source");
        lp.handle()
            .insert_source(source, |offset, _, seen: &mut Vec<u8>| seen.push(offset))
            .expect("inserted");
        raise(2).expect("raised");
        raise(1).expect("raised");
        let mut seen = Vec::new();
        for _ in 0..10 {
            lp.dispatch(Some(Duration::from_millis(50)), &mut seen)
                .expect("dispatch");
            if seen.len() == 2 {
                break;
            }
        }
        assert_eq!(seen, [1, 2]);
    }

    /// A process held by its pidfd takes the signal; `SIGRTMIN+1` at its
    /// default action ends the child the test started.
    #[test]
    fn a_signal_through_the_pidfd_reaches_the_process() {
        use std::os::unix::process::ExitStatusExt;
        let mut child = std::process::Command::new("sleep")
            .arg("10")
            .spawn()
            .expect("a child");
        let process = Process::open(child.id()).expect("its pidfd");
        process.signal(1).expect("signalled");
        let status = child.wait().expect("its exit");
        assert_eq!(status.signal(), number(1));
    }

    #[test]
    fn an_offset_past_sigrtmax_is_refused() {
        assert!(RtSignals::new(&[u8::MAX]).is_err());
        let me = Process::open(std::process::id()).expect("this process's pidfd");
        assert!(me.signal(u8::MAX).is_err());
        assert!(raise(u8::MAX).is_err());
    }
}
