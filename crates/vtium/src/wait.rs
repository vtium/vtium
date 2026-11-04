//! The start's wait for a display (the operator's decision on
//! vtium/vtium#21): once the VT is shown, vtium waits `display-wait` for
//! a frame on a display's screen, and with none by then it ends. Once one
//! has shown the wait is over for good, and displays coming and going are
//! hotplug. Under `forever` no wait runs, and vtium never ends for want
//! of a display.
//!
//! The wait runs only while the VT is vtium's. Another program can take
//! the VT before vtium's first frame shows, as X starting on another VT
//! does at boot (vtium/vtium#270). The release stops the wait, and vtium
//! waits for the VT to come back, as it does for a switch away later in
//! its run. The next acquire sets every display up afresh, so the wait
//! starts again from the full `display-wait`. The C reference had no such
//! wait: on a release its seat went to the background and slept until the
//! VT came back (`src/seat.c:438-462` at kmscon b1f7a7c5).
//!
//! A frame reaches the screen at its flip, which the turn after its draw
//! reads at the earliest. So the wait runs out at the end of a turn after
//! the due time, never the turn it falls due in, and while a frame is on
//! its way to a screen it waits up to [`FLIP_WAIT`] more for the flip.
//! Under `display-wait=0` a display set up when the VT is shown is drawn,
//! and its flip read, before the wait runs out.

use std::time::{Duration, Instant};

/// How long a frame on its way to a screen is waited for past the due
/// time: the C reference's wait for a page flip
/// (`src/video/drm_shared.c:834-860` at kmscon b1f7a7c5).
pub const FLIP_WAIT: Duration = Duration::from_secs(1);

/// Where the wait stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The VT is not vtium's, and no frame has shown yet: no display can
    /// be set up, and the wait of the `Duration` starts when the VT is
    /// shown.
    Before(Duration),
    /// Waiting until `due`, for `wait` in all.
    Until { due: Instant, wait: Duration },
    /// Past the due time with a frame on its way to a screen: waiting for
    /// its flip until `due`.
    Flipping { due: Instant, wait: Duration },
    /// A display came, the wait ran out, or it is `forever`.
    Over,
}

impl Wait {
    /// A wait of `wait`; `None` is `forever`.
    pub fn new(wait: Option<Duration>) -> Wait {
        wait.map_or(Wait::Over, Wait::Before)
    }

    /// The VT is shown: the wait starts, unless it runs or is over.
    pub fn shown(&mut self, now: Instant) {
        if let Wait::Before(wait) = *self {
            *self = Wait::Until {
                due: now.checked_add(wait).unwrap_or(now),
                wait,
            };
        }
    }

    /// The VT is let go before a frame showed: the wait stops until the
    /// VT is shown again.
    pub fn released(&mut self) {
        if let Wait::Until { wait, .. } | Wait::Flipping { wait, .. } = *self {
            *self = Wait::Before(wait);
        }
    }

    /// A turn's end, with or without a frame shown on a display so far,
    /// and with or without a frame `flipping`, on its way to a screen:
    /// the wait it ran when it ran out with none shown.
    pub fn turn(&mut self, now: Instant, shown: bool, flipping: bool) -> Option<Duration> {
        match *self {
            Wait::Before(_) | Wait::Over => None,
            _ if shown => {
                *self = Wait::Over;
                None
            }
            Wait::Until { due, .. } if now <= due => None,
            Wait::Until { wait, .. } if flipping => {
                *self = Wait::Flipping {
                    due: now.checked_add(FLIP_WAIT).unwrap_or(now),
                    wait,
                };
                None
            }
            Wait::Flipping { due, .. } if flipping && now < due => None,
            Wait::Until { wait, .. } | Wait::Flipping { wait, .. } => {
                *self = Wait::Over;
                Some(wait)
            }
        }
    }

    /// When the wait runs out, while it runs.
    pub fn due(&self) -> Option<Instant> {
        match self {
            Wait::Until { due, .. } | Wait::Flipping { due, .. } => Some(*due),
            Wait::Before(_) | Wait::Over => None,
        }
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "a fixed origin for the times")]
mod tests {
    use super::*;

    const TEN: Duration = Duration::from_secs(10);
    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn a_second_show_while_the_wait_runs_keeps_its_due_time() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        assert_eq!(w.turn(t0 + TEN * 2, false, false), None, "not shown yet");
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
        assert_eq!(
            w.turn(t0 + TEN - Duration::from_millis(1), false, false),
            None
        );
        assert_eq!(
            w.turn(t0 + TEN, false, false),
            None,
            "the turn it falls due in"
        );
        assert_eq!(w.turn(t0 + TEN + MS, false, false), Some(TEN));
        assert_eq!(w.turn(t0 + TEN * 2, false, false), None, "once");
        assert_eq!(w.due(), None);
    }

    #[test]
    fn a_display_ends_the_wait_for_good() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0, true, false), None);
        assert_eq!(
            w.turn(t0 + TEN * 2, false, false),
            None,
            "gone later: hotplug"
        );
        w.shown(t0 + TEN * 3);
        assert_eq!(w.due(), None);
    }

    #[test]
    fn no_wait_runs_out_at_the_turn_after_the_vt_is_shown() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(Duration::ZERO));
        w.shown(t0);
        assert_eq!(w.turn(t0, false, false), None, "the turn it was shown in");
        assert_eq!(w.due(), Some(t0), "the loop turns again at once");
        assert_eq!(w.turn(t0 + MS, false, false), Some(Duration::ZERO));
    }

    #[test]
    fn no_wait_with_a_frame_drawn_reads_its_flip() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(Duration::ZERO));
        w.shown(t0);
        assert_eq!(w.turn(t0, false, true), None);
        assert_eq!(w.turn(t0 + MS, true, false), None, "the flip read");
        assert_eq!(w.turn(t0 + TEN, false, false), None, "over for good");
    }

    #[test]
    fn a_frame_on_its_way_is_waited_for_up_to_the_flip_wait() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0 + TEN + MS, false, true), None);
        assert_eq!(w.due(), Some(t0 + TEN + MS + FLIP_WAIT));
        assert_eq!(w.turn(t0 + TEN + FLIP_WAIT, false, true), None);
        assert_eq!(w.turn(t0 + TEN + MS + FLIP_WAIT, false, true), Some(TEN));
    }

    #[test]
    fn a_flip_done_with_no_frame_shown_runs_the_wait_out() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0 + TEN + MS, false, true), None);
        assert_eq!(w.turn(t0 + TEN + MS * 2, false, false), Some(TEN));
    }

    #[test]
    fn a_release_stops_the_wait_and_the_next_show_starts_it_whole() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        w.released();
        assert_eq!(w.due(), None, "stopped while the VT is another's");
        assert_eq!(w.turn(t0 + TEN * 5, false, false), None, "never runs out");
        w.shown(t0 + TEN * 6);
        assert_eq!(w.due(), Some(t0 + TEN * 7), "the full wait from the return");
        assert_eq!(w.turn(t0 + TEN * 7 + MS, false, false), Some(TEN));
    }

    #[test]
    fn a_release_while_a_flip_is_awaited_stops_the_wait_too() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0 + TEN + MS, false, true), None);
        w.released();
        assert_eq!(w.due(), None);
        assert_eq!(w.turn(t0 + TEN * 5, false, false), None);
    }

    #[test]
    fn a_release_after_a_frame_showed_changes_nothing() {
        let t0 = Instant::now();
        let mut w = Wait::new(Some(TEN));
        w.shown(t0);
        assert_eq!(w.turn(t0, true, false), None);
        w.released();
        w.shown(t0 + TEN);
        assert_eq!(w, Wait::Over, "a display came: the wait is over for good");
    }

    #[test]
    fn forever_never_runs_out() {
        let t0 = Instant::now();
        let mut w = Wait::new(None);
        w.shown(t0);
        assert_eq!(w.due(), None);
        assert_eq!(w.turn(t0 + TEN * 1000, false, true), None);
    }
}
