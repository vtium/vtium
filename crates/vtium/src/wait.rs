//! The start's wait for a display (the operator's decision on
//! vtium/vtium#21): from the first time the VT is shown, vtium waits
//! `display-wait` for a frame on a display's screen, and with none by
//! then it ends. Once one has shown the wait is over for good, and displays coming
//! and going are hotplug. Under `forever` no wait runs, and vtium never
//! ends for want of a display.

use std::time::{Duration, Instant};

/// Where the wait stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The VT has not been shown yet: no display can be set up.
    Before(Duration),
    /// Waiting until `due`, for `wait` in all.
    Until { due: Instant, wait: Duration },
    /// A display came, the wait ran out, or it is `forever`.
    Over,
}

impl Wait {
    /// A wait of `wait`; `None` is `forever`.
    pub fn new(wait: Option<Duration>) -> Wait {
        wait.map_or(Wait::Over, Wait::Before)
    }

    /// The VT is shown: the first time, the wait starts.
    pub fn shown(&mut self, now: Instant) {
        if let Wait::Before(wait) = *self {
            *self = Wait::Until {
                due: now.checked_add(wait).unwrap_or(now),
                wait,
            };
        }
    }

    /// A turn's end, with or without a frame shown on a display so far:
    /// the wait it ran when it ran out with none.
    pub fn turn(&mut self, now: Instant, shown: bool) -> Option<Duration> {
        let Wait::Until { due, wait } = *self else {
            return None;
        };
        if shown {
            *self = Wait::Over;
            None
        } else if now >= due {
            *self = Wait::Over;
            Some(wait)
        } else {
            None
        }
    }

    /// When the wait runs out, while it runs.
    pub fn due(&self) -> Option<Instant> {
        match self {
            Wait::Until { due, .. } => Some(*due),
            Wait::Before(_) | Wait::Over => None,
        }
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "a fixed origin for the times")]
mod tests {
    use super::*;

    const TEN: Duration = Duration::from_secs(10);

    #[test]
    fn the_wait_starts_when_the_vt_is_first_shown() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        assert_eq!(w.turn(t0 + TEN * 2, false), None, "not shown yet");
        assert_eq!(w.due(), None);
        w.shown(t0 + TEN);
        w.shown(t0 + TEN * 3);
        assert_eq!(w.due(), Some(t0 + TEN * 2), "from the first time");
    }

    #[test]
    fn with_no_display_by_the_due_time_the_wait_runs_out_once() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0 + TEN - Duration::from_millis(1), false), None);
        assert_eq!(w.turn(t0 + TEN, false), Some(TEN));
        assert_eq!(w.turn(t0 + TEN * 2, false), None, "once");
        assert_eq!(w.due(), None);
    }

    #[test]
    fn a_display_ends_the_wait_for_good() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0, true), None);
        assert_eq!(w.turn(t0 + TEN * 2, false), None, "gone later: hotplug");
        w.shown(t0 + TEN * 3);
        assert_eq!(w.due(), None);
    }

    #[test]
    fn no_wait_runs_out_at_the_first_turn_without_a_display() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(Duration::ZERO));
        w.shown(t0);
        assert_eq!(w.turn(t0, false), Some(Duration::ZERO));
    }

    #[test]
    fn forever_never_runs_out() {
        let t0 = Instant::now();
        let mut w = Wait::new(None);
        w.shown(t0);
        assert_eq!(w.due(), None);
        assert_eq!(w.turn(t0 + TEN * 1000, false), None);
    }
}
