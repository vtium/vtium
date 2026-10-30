//! When a head's sessions first start ([`first_start_due`]), and whether
//! the command of a head's last console starts again when it exits.
//!
//! It starts again unless `oneshot` is set, or it has exited within
//! [`QUICK`] of its start more than [`QUICK_RESTARTS`] times running:
//! the C reference's limit of 5 restarts within 2 seconds
//! (`src/misc/pty.c:52-53, 737-768` at kmscon b1f7a7c5). The time is an
//! argument, read from the monotonic clock by the loop when the child
//! exits, so the policy is a function of the exit times it is given.

use std::time::Duration;

/// Whether a head's command's first start is due at a turn's end, with
/// the VT `shown` or not and a `display` of the head set up or not. It is
/// due once the VT is shown and a display is set up, so the child starts
/// on the grid of every display set up by then and sees no SIGWINCH as it
/// starts; a display set up a turn later, as one udev brings, resizes the
/// session. The C reference opened the pty as its VT was activated, at the
/// first display it activated, and resized it for each display after
/// (`src/seat.c:179-212, 244-250, 438-447` and `src/terminal.c:1711-1737,
/// 1764-1778` at kmscon b1f7a7c5). A head whose displays never appear
/// starts no session, under `display-wait=forever` too (ADR-1401 D1).
pub fn first_start_due(shown: bool, display: bool) -> bool {
    shown && display
}

/// An exit sooner than this after the start is quick.
pub const QUICK: Duration = Duration::from_secs(2);

/// The most quick exits running that still start the command again.
pub const QUICK_RESTARTS: u32 = 5;

/// What follows an exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Start the command again.
    Restart,
    /// End the head: `oneshot` is set.
    Oneshot,
    /// End the head: the command exited quickly too many times running.
    TooQuick,
}

/// A console's command's starts and quick exits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Respawn {
    started: Duration,
    quick: u32,
}

impl Respawn {
    /// The command's first start, at `now`.
    pub fn new(now: Duration) -> Respawn {
        Respawn {
            started: now,
            quick: 0,
        }
    }

    /// The command exited at `now`. On [`Next::Restart`], `now` is its
    /// next start.
    pub fn exited(&mut self, now: Duration, oneshot: bool) -> Next {
        if oneshot {
            return Next::Oneshot;
        }
        // A clock that reads earlier than the start gives zero: quick.
        if now.saturating_sub(self.started) < QUICK {
            self.quick = self.quick.saturating_add(1);
        } else {
            self.quick = 0;
        }
        if self.quick > QUICK_RESTARTS {
            return Next::TooQuick;
        }
        self.started = now;
        Next::Restart
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_start_waits_for_the_vt_and_a_display() {
        // (shown, display) and whether the start is due.
        let table = [
            ((false, false), false),
            ((false, true), false),
            ((true, false), false),
            ((true, true), true),
        ];
        for ((shown, display), due) in table {
            assert_eq!(
                first_start_due(shown, display),
                due,
                "shown {shown}, display {display}"
            );
        }
    }

    #[test]
    fn a_session_that_runs_starts_again() {
        let mut r = Respawn::new(ms(0));
        for n in 1..=20 {
            assert_eq!(r.exited(ms(n * 60_000), false), Next::Restart);
        }
    }

    #[test]
    fn five_quick_restarts_then_exit() {
        let mut r = Respawn::new(ms(1_000));
        for n in 1..=5 {
            assert_eq!(
                r.exited(ms(1_000 + n * 100), false),
                Next::Restart,
                "exit {n}"
            );
        }
        assert_eq!(r.exited(ms(1_600), false), Next::TooQuick);
    }

    #[test]
    fn quick_is_under_two_seconds_from_the_start() {
        let mut r = Respawn::new(ms(0));
        // 1,999 ms after each start is quick; 2,000 is not.
        let mut t = 0;
        for _ in 0..5 {
            t += 1_999;
            assert_eq!(r.exited(ms(t), false), Next::Restart);
        }
        let mut slow = r;
        assert_eq!(r.exited(ms(t + 1_999), false), Next::TooQuick);
        assert_eq!(slow.exited(ms(t + 2_000), false), Next::Restart);
    }

    #[test]
    fn a_slow_exit_starts_the_count_again() {
        let mut r = Respawn::new(ms(0));
        let mut t = 0;
        for _ in 0..5 {
            t += 10;
            assert_eq!(r.exited(ms(t), false), Next::Restart);
        }
        // The sixth child ran a minute: it starts again, and the count
        // starts over. The C reference exited here, whatever the timing.
        t += 60_000;
        assert_eq!(r.exited(ms(t), false), Next::Restart);
        for _ in 0..5 {
            t += 10;
            assert_eq!(r.exited(ms(t), false), Next::Restart);
        }
        assert_eq!(r.exited(ms(t + 10), false), Next::TooQuick);
    }

    #[test]
    fn each_start_is_timed_from_its_own_start() {
        // A session that ran an hour, then a command that fails at once:
        // the failures are quick, timed from each new start. The C
        // reference timed them from an older start and never stopped.
        let mut r = Respawn::new(ms(0));
        let mut t = 3_600_000;
        assert_eq!(r.exited(ms(t), false), Next::Restart);
        for _ in 0..5 {
            t += 5;
            assert_eq!(r.exited(ms(t), false), Next::Restart);
        }
        assert_eq!(r.exited(ms(t + 5), false), Next::TooQuick);
    }

    #[test]
    fn oneshot_exits_at_the_first_exit() {
        let mut r = Respawn::new(ms(0));
        assert_eq!(r.exited(ms(60_000), true), Next::Oneshot);
        let mut r = Respawn::new(ms(0));
        assert_eq!(r.exited(ms(1), true), Next::Oneshot);
    }

    #[test]
    fn a_clock_that_reads_earlier_counts_as_quick() {
        let mut r = Respawn::new(ms(5_000));
        assert_eq!(r.exited(ms(4_000), false), Next::Restart);
        assert_eq!(
            r,
            Respawn {
                started: ms(4_000),
                quick: 1
            }
        );
    }
}
