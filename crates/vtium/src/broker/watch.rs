//! Watching the drawer: its pidfd tells the broker when the drawer exits,
//! and its socket tells the broker when the drawer sends a message or
//! closes its end. The broker reaps the drawer through the pidfd and maps
//! the way it ended to the unit's exit status.
//!
//! References:
//! - ADR-1400 D4, "Start, stop and exit": the drawer's own status, and a
//!   failure for a drawer that a signal killed.
//! - ADR-1400 D4, "When the drawer dies": the socket's close or the exit,
//!   whichever the broker sees first.

use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};

use rustix::process::{WaitId, WaitIdOptions};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, Mode, PostAction, RegistrationToken};
use vtium_platform::event_loop::Loop;

/// The unit's exit status for a failure.
pub const FAILURE: u8 = 1;

/// How the drawer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It exited with this status.
    Code(u8),
    /// This signal killed it.
    Signal(i32),
}

impl Ended {
    /// The broker's own exit status: the drawer's status, or a failure
    /// when a signal killed the drawer. A clean exit of the drawer is then
    /// a clean exit of the unit, and the unit's failure handling judges a
    /// failure as it does for vtium in one process.
    pub fn status(self) -> u8 {
        match self {
            Ended::Code(code) => code,
            Ended::Signal(_) => FAILURE,
        }
    }

    fn from_wait(status: &rustix::process::WaitIdStatus) -> Ended {
        match (status.exit_status(), status.terminating_signal()) {
            (Some(code), _) => Ended::Code(u8::try_from(code).unwrap_or(FAILURE)),
            (None, Some(signal)) => Ended::Signal(signal),
            // waitid with only EXITED reports an exit or a death by signal
            (None, None) => Ended::Signal(0),
        }
    }
}

/// Reaps the drawer through `pidfd` when it has ended, without waiting.
pub fn ended(pidfd: BorrowedFd<'_>) -> io::Result<Option<Ended>> {
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG;
    loop {
        match rustix::process::waitid(WaitId::PidFd(pidfd), options) {
            Ok(status) => return Ok(status.as_ref().map(Ended::from_wait)),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Kills the drawer with `SIGKILL` and reaps it, waiting for its exit,
/// which the kernel makes prompt. A drawer that ended meanwhile keeps the
/// way it ended.
pub fn kill(pidfd: BorrowedFd<'_>) -> Ended {
    // fails only for a drawer already ended, which the wait reaps
    let _ = rustix::process::pidfd_send_signal(pidfd, rustix::process::Signal::KILL);
    loop {
        match rustix::process::waitid(WaitId::PidFd(pidfd), WaitIdOptions::EXITED) {
            Ok(Some(status)) => return Ended::from_wait(&status),
            Err(rustix::io::Errno::INTR) => {}
            // reaped already, or never the broker's: nothing more to learn
            Ok(None) | Err(_) => return Ended::Signal(0),
        }
    }
}

/// What one turn of the broker's loop saw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The socket has a message, or the drawer closed it.
    pub socket: bool,
    /// The drawer's pidfd is readable: the drawer has exited.
    pub exited: bool,
    /// Something asked the broker to stop: systemd's `SIGTERM`, or a
    /// test.
    pub stop: bool,
}

/// The socket's place in the broker's loop, which the broker removes once
/// the drawer closes its end, since a closed socket stays readable.
pub struct Watched(RegistrationToken);

impl Watched {
    /// Stops watching the socket.
    pub fn unwatch(self, lp: &Loop<Seen>) {
        lp.handle().remove(self.0);
    }
}

/// Watches the socket and the pidfd in `lp`, each through a copy of its
/// descriptor, so each turn marks in [`Seen`] which of them is ready.
pub fn watch(
    lp: &Loop<Seen>,
    socket: BorrowedFd<'_>,
    pidfd: BorrowedFd<'_>,
) -> io::Result<Watched> {
    let insert = |fd: OwnedFd, mark: fn(&mut Seen)| {
        lp.handle()
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                move |_, _, seen: &mut Seen| {
                    mark(seen);
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| e.error)
    };
    let token = insert(socket.try_clone_to_owned()?, |seen| seen.socket = true)?;
    insert(pidfd.try_clone_to_owned()?, |seen| seen.exited = true)?;
    Ok(Watched(token))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// The drawer's own status passes through, and a death by signal is a
    /// failure, never 0.
    #[test]
    fn the_exit_status_is_the_drawers_and_a_signal_is_a_failure() {
        assert_eq!(Ended::Code(0).status(), 0);
        assert_eq!(Ended::Code(3).status(), 3);
        assert_eq!(Ended::Code(255).status(), 255);
        assert_eq!(Ended::Signal(9).status(), FAILURE);
        assert_eq!(Ended::Signal(11).status(), FAILURE);
        assert_ne!(Ended::Signal(0).status(), 0);
    }
}
