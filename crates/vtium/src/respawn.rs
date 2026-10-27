//! Whether the session's command starts again when it exits.
//!
//! It starts again unless `oneshot` is set, or it has exited within
//! [`QUICK`] of its start more than [`QUICK_RESTARTS`] times running:
//! the C reference's limit of 5 restarts within 2 seconds
//! (`src/misc/pty.c:52-53, 737-768` at kmscon b1f7a7c5). The time is an
//! argument, read from the monotonic clock by the loop when the child
//! exits, so the policy is a function of the exit times it is given.

use std::time::Duration;

/// An exit sooner than this after the start is quick.
pub const QUICK: Duration = Duration::from_secs(2);

/// The most quick exits running that still start the command again.
pub const QUICK_RESTARTS: u32 = 5;

/// What follows an exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Start the command again.
    Restart,
    /// Exit vtium: `oneshot` is set.
    Oneshot,
    /// Exit vtium: the command exited quickly too many times running.
    TooQuick,
}

/// The command's starts and quick exits.
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
