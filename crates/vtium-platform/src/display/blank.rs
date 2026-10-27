//! The DPMS blank: after a time with no key or pointer event, the displays
//! are powered down, and the next event powers them up.
//!
//! [`Blank`] keeps the time of the last input and says when the timeout
//! has passed. The owner powers the displays down when its loop timer
//! fires ([`Blank::insert`]) and up when [`Blank::input`] says they were
//! blanked. A wake-up ([`super::Event::Awake`]) and a hotplug count as
//! input: their modeset brings every display on. The timeout is the configuration's `dpms-timeout`, 600 s by
//! default as in the C reference (config.c:1388).

use std::time::{Duration, Instant};

use calloop::LoopHandle;
use calloop::timer::{TimeoutAction, Timer};

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

    /// A key or pointer event came at `now`. Returns whether the displays
    /// were blanked, so the owner powers them up.
    pub fn input(&mut self, now: Instant) -> bool {
        self.last = now;
        std::mem::take(&mut self.blanked)
    }

    pub fn blanked(&self) -> bool {
        self.blanked
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

    /// Puts the blank's timer in the loop behind `handle`. `access` finds
    /// it in the loop's data; `blank` powers the displays down.
    pub fn insert<D, A, B>(&self, handle: &LoopHandle<'static, D>, access: A, mut blank: B)
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Blank> + 'static,
        B: FnMut(&mut D) + 'static,
    {
        let Some(first) = self.timeout else {
            return;
        };
        let inserted = handle.insert_source(Timer::from_duration(first), move |_, _, data| {
            #[expect(
                clippy::disallowed_methods,
                reason = "the DPMS blank is one of the timers vtium-platform holds"
            )]
            let now = Instant::now();
            let Some(due) = access(data).map(|b| b.due(now)) else {
                return TimeoutAction::Drop;
            };
            match due {
                Due::In(left) => TimeoutAction::ToDuration(left),
                Due::Now => {
                    blank(data);
                    access(data)
                        .and_then(|b| b.timeout)
                        .map_or(TimeoutAction::Drop, TimeoutAction::ToDuration)
                }
                Due::Never => TimeoutAction::Drop,
            }
        });
        if let Err(e) = inserted {
            log::error!("cannot arm the DPMS blank: {e}");
        }
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
    fn the_default_is_the_c_references() {
        assert_eq!(DPMS_TIMEOUT, T);
    }
}
