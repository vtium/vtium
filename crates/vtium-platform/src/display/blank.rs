//! The DPMS blank: after a time with none of the user's input, the
//! displays are powered down, and the next input powers them up.
//!
//! [`Blank`] keeps the time of the last input and says when the timeout
//! has passed. The owner looks at it on its own clock: it wakes at
//! [`Blank::deadline`] and powers the displays down when [`Blank::fire`]
//! says so. It powers them up when [`Blank::input`] says they were
//! blanked, at the user's key press or pointer action (a move, a button
//! or the wheel) or a program's wake, through each device's `unblank`,
//! which tries again on its own where it fails, since no later input
//! reaches it. Each such press or action also goes to each device's
//! `input`, which tries again once those retries have run out. A wake-up
//! ([`super::Event::Awake`]) after the user's VT switch counts as input:
//! its modeset brings every display on. A wake-up after a VT re-take does
//! not; the host keeps the blank and its countdown. A hotplug does not:
//! while blanked it commits nothing, and `unblank` runs the modeset it
//! left owed. The timeout is the configuration's `dpms-timeout`, 600 s by
//! default as in the C reference (config.c:1388), until the owner changes
//! it ([`Blank::set_timeout`]), as a program may with the Linux console's
//! `CSI 9 ; n ]`.

use std::time::{Duration, Instant};

/// The default `dpms-timeout`.
pub const DPMS_TIMEOUT: Duration = Duration::from_secs(600);

/// When the displays blank.
#[derive(Debug)]
pub struct Blank {
    /// `None`: never.
    timeout: Option<Duration>,
    last: Instant,
    blanked: bool,
}

/// What the timer finds.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Due {
    /// Not yet: look again after this long.
    In(Duration),
    /// Blank the displays now.
    Now,
    /// Nothing to do until the next input.
    Never,
}

impl Blank {
    /// A blank after `timeout` with no input, counted from `now`; `None`
    /// never blanks.
    pub fn new(timeout: Option<Duration>, now: Instant) -> Blank {
        Blank {
            timeout: timeout.filter(|t| !t.is_zero()),
            last: now,
            blanked: false,
        }
    }

    /// The user's key press or pointer action, or a program's wake, came
    /// at `now`. Returns
    /// whether the displays were blanked, so the owner powers them up.
    pub fn input(&mut self, now: Instant) -> bool {
        self.last = now;
        std::mem::take(&mut self.blanked)
    }

    pub fn blanked(&self) -> bool {
        self.blanked
    }

    /// The displays blank after `timeout` with no input, `None` or zero
    /// never. A change restarts the countdown from `now` and keeps a
    /// blank in force; it returns whether the timeout changed.
    pub fn set_timeout(&mut self, timeout: Option<Duration>, now: Instant) -> bool {
        let timeout = timeout.filter(|t| !t.is_zero());
        if timeout == self.timeout {
            return false;
        }
        self.timeout = timeout;
        self.last = now;
        true
    }

    pub(super) fn due(&mut self, now: Instant) -> Due {
        let Some(timeout) = self.timeout else {
            return Due::Never;
        };
        if self.blanked {
            return Due::In(timeout);
        }
        match timeout.checked_sub(now.saturating_duration_since(self.last)) {
            Some(left) if !left.is_zero() => Due::In(left),
            _ => {
                self.blanked = true;
                Due::Now
            }
        }
    }

    /// When the displays blank, unless input comes first: `None` while
    /// blanking is off or the displays are blanked already.
    pub fn deadline(&self) -> Option<Instant> {
        let timeout = self.timeout.filter(|_| !self.blanked)?;
        self.last.checked_add(timeout)
    }

    /// Whether the displays blank at `now`: the owner powers them down
    /// when it says so, once a blank.
    pub fn fire(&mut self, now: Instant) -> bool {
        self.due(now) == Due::Now
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "a test's instants start from the clock once"
)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(600);

    #[test]
    fn the_displays_blank_after_the_timeout_without_input() {
        let start = Instant::now();
        let mut b = Blank::new(Some(T), start);
        assert_eq!(b.due(start + T / 2), Due::In(T / 2));
        assert!(!b.input(start + T / 2));
        assert_eq!(b.due(start + T), Due::In(T / 2));
        assert_eq!(b.due(start + T + T / 2), Due::Now);
        assert!(b.blanked());
        assert_eq!(b.due(start + 2 * T), Due::In(T));
        assert!(b.input(start + 2 * T));
        assert!(!b.blanked());
        assert!(!b.input(start + 2 * T));
    }

    #[test]
    fn a_zero_or_absent_timeout_never_blanks() {
        let start = Instant::now();
        for timeout in [None, Some(Duration::ZERO)] {
            let mut b = Blank::new(timeout, start);
            assert_eq!(b.due(start + 10 * T), Due::Never);
            assert!(!b.blanked());
        }
    }

    #[test]
    fn a_timeout_change_restarts_the_countdown() {
        let start = Instant::now();
        let minute = Duration::from_secs(60);
        let mut b = Blank::new(Some(T), start);
        assert!(b.set_timeout(Some(minute), start + T / 2));
        assert_eq!(b.due(start + T / 2 + minute / 2), Due::In(minute / 2));
        assert_eq!(b.due(start + T / 2 + minute), Due::Now);
        assert!(b.blanked());
    }

    #[test]
    fn the_timeout_in_force_again_changes_nothing() {
        let start = Instant::now();
        let mut b = Blank::new(Some(T), start);
        assert!(!b.set_timeout(Some(T), start + T / 2));
        assert_eq!(b.due(start + T), Due::Now, "the countdown kept");
        let mut b = Blank::new(None, start);
        assert!(!b.set_timeout(Some(Duration::ZERO), start + T));
    }

    #[test]
    fn a_zero_or_absent_timeout_turns_blanking_off_and_back_on() {
        let start = Instant::now();
        for off in [None, Some(Duration::ZERO)] {
            let mut b = Blank::new(Some(T), start);
            assert!(b.set_timeout(off, start + T / 2));
            assert_eq!(b.due(start + 10 * T), Due::Never);
            assert!(!b.blanked());
            assert!(b.set_timeout(Some(T), start + 10 * T));
            assert_eq!(b.due(start + 11 * T), Due::Now);
        }
    }

    #[test]
    fn a_timeout_change_keeps_a_blank_in_force() {
        let start = Instant::now();
        let mut b = Blank::new(Some(T), start);
        assert_eq!(b.due(start + T), Due::Now);
        assert!(b.set_timeout(Some(2 * T), start + T));
        assert!(b.blanked(), "a timeout is no wake");
        assert_eq!(b.due(start + 2 * T), Due::In(2 * T));
    }

    #[test]
    fn a_programs_wake_ends_the_blank_and_restarts_the_countdown() {
        let start = Instant::now();
        let mut b = Blank::new(Some(T), start);
        assert_eq!(b.due(start + T), Due::Now);
        assert!(b.input(start + 2 * T), "the displays were blanked");
        assert!(!b.blanked());
        assert_eq!(b.due(start + 2 * T + T / 2), Due::In(T / 2));
        assert!(!b.input(start + 2 * T + T / 2), "lit already");
        assert_eq!(b.due(start + 3 * T), Due::In(T / 2));
    }

    #[test]
    fn the_deadline_follows_the_last_input_and_the_timeout() {
        let start = Instant::now();
        let mut b = Blank::new(Some(T), start);
        assert_eq!(b.deadline(), Some(start + T));
        assert!(!b.fire(start + T / 2));
        b.input(start + T / 2);
        assert_eq!(b.deadline(), Some(start + T + T / 2));
        assert!(!b.fire(start + T), "the input restarted the countdown");
        assert!(b.fire(start + T + T / 2));
        assert_eq!(b.deadline(), None, "blanked: nothing until input");
        assert!(!b.fire(start + 3 * T), "one blank");
        assert!(b.input(start + 3 * T));
        assert!(b.set_timeout(None, start + 3 * T));
        assert_eq!(b.deadline(), None, "off");
        assert!(!b.fire(start + 10 * T));
    }

    #[test]
    fn the_default_is_the_c_references() {
        assert_eq!(DPMS_TIMEOUT, T);
    }
}
