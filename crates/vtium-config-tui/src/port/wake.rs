//! What wakes the loop, and the wait for it: the terminal's bytes, a
//! resize, a signal the loop holds, or the end of a wait the update asked
//! for.
//!
//! References:
//! - ADR-1206 D2: the loop turns every wake into a message.
//! - ADR-1201 D7: the signals are held and read in the loop.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::time::{Duration, Instant};

use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, Mode, PostAction};
use vtium_platform::event_loop::Loop;

/// What woke the loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wake {
    /// Bytes the terminal sent.
    Bytes(Vec<u8>),
    /// The terminal changed its size.
    Resize,
    /// One of the signals the loop holds, by its number. The end of the
    /// terminal's input comes as a hangup, 1.
    Signal(i32),
    /// The wait the update asked for ended.
    Timeout,
}

/// What the loop's sources leave for the wait to return.
#[derive(Debug, Default)]
pub struct Woken {
    pub wakes: Vec<Wake>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the TUI's waits are bounded in time, and the terminal's port is where time enters"
)]
pub(crate) fn now() -> Instant {
    Instant::now()
}

/// Reads the terminal's input on `input` into the loop's wakes, each read
/// as [`Wake::Bytes`], and the end of the input as a hangup.
pub fn watch_input(events: &Loop<Woken>, input: OwnedFd) -> io::Result<()> {
    let source = Generic::new(input, Interest::READ, Mode::Level);
    events
        .handle()
        .insert_source(source, |_, fd, w: &mut Woken| {
            let mut buf = [0u8; 4096];
            match rustix::io::read(fd.as_fd(), &mut buf) {
                Ok(0) => {
                    // the terminal is gone
                    w.wakes.push(Wake::Signal(1));
                    Ok(PostAction::Remove)
                }
                Ok(n) => {
                    w.wakes
                        .push(Wake::Bytes(buf.get(..n).unwrap_or_default().to_vec()));
                    Ok(PostAction::Continue)
                }
                Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => Ok(PostAction::Continue),
                // a pty whose other side closed reads EIO: the terminal
                // is gone
                Err(rustix::io::Errno::IO) => {
                    w.wakes.push(Wake::Signal(1));
                    Ok(PostAction::Remove)
                }
                Err(e) => Err(e.into()),
            }
        })
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(())
}

/// Waits on `events` until a source leaves something in `woken`, or until
/// `deadline` passes, which ends in [`Wake::Timeout`] and clears it.
///
/// The deadline ends the wait only after a dispatch that finds nothing
/// waiting, so input that came while the TUI was busy past its deadline
/// is read first.
pub fn wait_on(
    events: &mut Loop<Woken>,
    deadline: &mut Option<Instant>,
    woken: &mut Woken,
) -> io::Result<()> {
    loop {
        let timeout = deadline.map(|d| d.saturating_duration_since(now()));
        events.dispatch(timeout, woken)?;
        if !woken.wakes.is_empty() {
            return Ok(());
        }
        if timeout == Some(Duration::ZERO) {
            *deadline = None;
            woken.wakes.push(Wake::Timeout);
            return Ok(());
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use std::io::Write;

    use super::*;

    /// The terminal's input that already waits comes before the end of a
    /// wait whose deadline has passed.
    #[test]
    fn input_that_waits_comes_before_a_deadline_that_has_passed() {
        let mut events = Loop::<Woken>::new().expect("an event loop");
        let (read, mut write) = std::io::pipe().expect("a pipe");
        watch_input(&events, read.into()).expect("the input is watched");
        write.write_all(b"q").expect("a key");
        let mut deadline = Some(now());
        let mut woken = Woken::default();
        wait_on(&mut events, &mut deadline, &mut woken).expect("a wait");
        assert_eq!(woken.wakes, [Wake::Bytes(b"q".to_vec())]);
        let mut woken = Woken::default();
        wait_on(&mut events, &mut deadline, &mut woken).expect("a wait");
        assert_eq!(woken.wakes, [Wake::Timeout]);
        assert_eq!(deadline, None);
    }
}
