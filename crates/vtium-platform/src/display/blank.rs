//! The DPMS blank: after a time with no key or pointer event, the displays
//! are powered down, and the next event powers them up.
//!
//! [`Blank`] keeps the time of the last input and says when the timeout
//! has passed. The owner powers the displays down when its loop timer
//! fires ([`Blank::insert`]) and up when [`Blank::input`] says they were
//! blanked, at a key or pointer event or a program's wake, through each device's `unblank`, which tries again on its own
//! where it fails, since no later input reaches it. Every key or pointer
//! event also goes to each device's `input`, which tries again once those
//! retries have run out. A wake-up
//! ([`super::Event::Awake`]) after the user's VT switch counts as input:
//! its modeset brings every display on. A wake-up after a VT re-take does
//! not; the host keeps the blank and its countdown. A hotplug does not: while blanked it commits nothing, and
//! `unblank` runs the modeset it left owed. The timeout is the
//! configuration's `dpms-timeout`, 600 s by default as in the C reference
//! (config.c:1388), until the owner changes it ([`Blank::set_timeout`]),
//! as a program may with the Linux console's `CSI 9 ; n ]`.

use std::time::{Duration, Instant};

use calloop::timer::{TimeoutAction, Timer};
use calloop::{LoopHandle, RegistrationToken};

/// The default `dpms-timeout`.
pub const DPMS_TIMEOUT: Duration = Duration::from_secs(600);

/// When the displays blank.
#[derive(Debug)]
pub struct Blank {
    /// `None`: never.
    timeout: Option<Duration>,
    last: Instant,
    blanked: bool,
    /// The timer [`Blank::insert`] put in the loop last.
    timer: Option<RegistrationToken>,
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
            timer: None,
        }
    }

    /// A key or pointer event, or a program's wake, came at `now`. Returns
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
    /// blank in force; it returns true, and the owner puts the timer in
    /// the loop again ([`Blank::insert`]).
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

    /// Puts the blank's timer in the loop behind `handle`, in place of
    /// the one put there before, and none while it never blanks. It
    /// looks first a timeout from now. `access` finds the blank in the
    /// loop's data; `blank` powers the displays down.
    pub fn insert<D, A, B>(&mut self, handle: &LoopHandle<'static, D>, access: A, mut blank: B)
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Blank> + 'static,
        B: FnMut(&mut D) + 'static,
    {
        // A token of a timer that has left the loop removes nothing.
        if let Some(timer) = self.timer.take() {
            handle.remove(timer);
        }
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
        match inserted {
            Ok(timer) => self.timer = Some(timer),
            Err(e) => log::error!("cannot arm the DPMS blank: {e}"),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "a test's instants start from the clock once"
)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    reason = "test arithmetic on bounded values; a test fails by panicking"
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

    #[derive(Default)]
    struct Data {
        blank: Option<Blank>,
        blanked: u32,
        /// How often a timer looked at the blank.
        looks: u32,
    }

    /// Runs the loop for `wait`, and returns how often it blanked and how
    /// often a timer looked.
    fn run(
        lp: &mut calloop::EventLoop<'static, Data>,
        data: &mut Data,
        wait: Duration,
    ) -> (u32, u32) {
        let end = Instant::now() + wait;
        let before = (data.blanked, data.looks);
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            lp.dispatch(Some(left), data).expect("dispatch");
        }
        (data.blanked - before.0, data.looks - before.1)
    }

    fn arm(lp: &calloop::EventLoop<'static, Data>, data: &mut Data) {
        let blank = data.blank.as_mut().expect("a blank");
        blank.insert(
            &lp.handle(),
            |d: &mut Data| {
                d.looks += 1;
                d.blank.as_mut()
            },
            |d: &mut Data| d.blanked += 1,
        );
    }

    #[test]
    fn insert_replaces_the_timer_in_the_loop() {
        let ms = Duration::from_millis;
        let mut lp = calloop::EventLoop::try_new().expect("a loop");
        let mut data = Data {
            blank: Some(Blank::new(Some(ms(200)), Instant::now())),
            ..Data::default()
        };
        arm(&lp, &mut data);
        // A shorter timeout blanks by its own time, not the first's.
        let b = data.blank.as_mut().expect("a blank");
        assert!(b.set_timeout(Some(ms(20)), Instant::now()));
        arm(&lp, &mut data);
        assert_eq!(run(&mut lp, &mut data, ms(100)).0, 1, "blanked at 20 ms");
        // Off: the timer leaves the loop, and nothing blanks.
        let b = data.blank.as_mut().expect("a blank");
        b.input(Instant::now());
        assert!(b.set_timeout(None, Instant::now()));
        arm(&lp, &mut data);
        assert_eq!(run(&mut lp, &mut data, ms(300)), (0, 0), "no timer");
        // On again: one timer, which blanks once in its timeout and looks
        // at the blank twice as it does.
        let b = data.blank.as_mut().expect("a blank");
        assert!(b.set_timeout(Some(ms(150)), Instant::now()));
        arm(&lp, &mut data);
        arm(&lp, &mut data);
        assert_eq!(run(&mut lp, &mut data, ms(250)), (1, 2), "one timer");
    }

    #[test]
    fn the_default_is_the_c_references() {
        assert_eq!(DPMS_TIMEOUT, T);
    }
}
