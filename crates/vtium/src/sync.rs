//! When the terminal presents: synchronized output (mode 2026) and the
//! coalescing of the child's output without it (`src/terminal_sync.h`
//! and `src/terminal_sync.c` at kmscon b1f7a7c5).
//!
//! **The hold.** A program brackets a frame in `CSI ? 2026 h` and
//! `CSI ? 2026 l`. While the hold is in force the displays keep the last
//! complete frame: a draw is owed, and nothing is drawn. When the frame
//! ends the terminal presents at once. A hold that lasts `sync-timeout`
//! is ended through the terminal, so DECRQM reports the mode reset; when
//! the pty still holds output read and not parsed, the end may be in it,
//! so the timeout waits for it, up to [`SYNC_WAIT`] after the hold began
//! or `sync-timeout` when longer. A display that lost its picture draws
//! the state as it is, and the hold goes on.
//!
//! **Coalescing.** Outside a hold, the child's output is not drawn while
//! a turn of the loop parsed [`COALESCE_BYTES`] or more, or left output
//! unparsed. It draws at the first turn that finds neither, or once a
//! refresh has passed since the display's last present, whichever comes
//! first; output that began after an idle refresh counts the refresh from
//! its beginning. Keys, the blink timers, the pointer, a frame's end and a
//! lost picture draw at once.
//!
//! The time is an argument: the loop reads the clock.

use std::time::{Duration, Instant};

/// The longest a timeout waits for output read and not parsed, unless
/// `sync-timeout` is longer.
pub const SYNC_WAIT: Duration = Duration::from_secs(1);

/// Bytes parsed in one turn that count as part way through a write.
pub const COALESCE_BYTES: usize = 16_384;

/// The refresh taken for a display that reports none, or one outside 20
/// to 1,000 Hz.
pub const REFRESH: Duration = Duration::from_nanos(1_000_000_000 / 60);
const REFRESH_MIN: Duration = Duration::from_millis(1);
const REFRESH_MAX: Duration = Duration::from_millis(50);

/// The terminal's side of mode 2026.
pub trait Hold {
    /// Mode 2026 is set.
    fn synchronized(&self) -> bool;
    /// The holds that have ended, by any cause.
    fn sync_frames(&self) -> u64;
    /// Ends the hold, as the timeout does.
    fn end_sync(&mut self);
}

impl Hold for vtium_term::Terminal {
    fn synchronized(&self) -> bool {
        vtium_term::Terminal::synchronized(self)
    }
    fn sync_frames(&self) -> u64 {
        vtium_term::Terminal::sync_frames(self)
    }
    fn end_sync(&mut self) {
        vtium_term::Terminal::end_sync(self);
    }
}

/// What the terminal does after the hold's rules ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Wake at [`Sync::deadline`].
    Arm,
    /// The frame ended: present.
    Present,
    /// The timeout ended the hold: present as drawn.
    Expired,
}

/// The hold of mode 2026.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sync {
    /// `None`: mode 2026 is not offered.
    timeout: Option<Duration>,
    frames: u64,
    held: bool,
    waiting: bool,
    began: Option<Instant>,
    deadline: Option<Instant>,
}

impl Sync {
    /// The hold for `sync-timeout`, `timeout`; zero offers no mode 2026.
    /// `frames` is the terminal's count of holds ended.
    pub fn new(timeout: Duration, frames: u64) -> Sync {
        Sync {
            timeout: (!timeout.is_zero()).then_some(timeout),
            frames,
            held: false,
            waiting: false,
            began: None,
            deadline: None,
        }
    }

    /// Whether mode 2026 is offered.
    pub fn enabled(&self) -> bool {
        self.timeout.is_some()
    }

    /// When the hold's timer is due, while one runs.
    pub fn deadline(&self) -> Option<Instant> {
        self.held.then_some(self.deadline).flatten()
    }

    /// A hold is in force: present nothing but a display that lost its
    /// picture.
    pub fn holding(&self, term: &impl Hold) -> bool {
        self.held && term.synchronized()
    }

    fn begin(&mut self, now: Instant) -> Action {
        self.held = true;
        self.waiting = false;
        self.began = Some(now);
        self.deadline = self.timeout.and_then(|t| now.checked_add(t));
        Action::Arm
    }

    /// The timeout ends the hold: the terminal reports the mode reset.
    fn expire(&mut self, term: &mut impl Hold) -> Action {
        term.end_sync();
        self.frames = term.sync_frames();
        self.held = false;
        self.waiting = false;
        Action::Expired
    }

    /// A hold ended since the count was last read.
    fn ended(&mut self, term: &impl Hold) -> bool {
        let frames = term.sync_frames();
        if frames == self.frames {
            return false;
        }
        self.frames = frames;
        self.held = false;
        self.waiting = false;
        true
    }

    /// After an end: present, or hold the frame the same input began.
    fn after_end(&mut self, term: &impl Hold, now: Instant) -> Action {
        if term.synchronized() {
            self.begin(now)
        } else {
            Action::Present
        }
    }

    /// The terminal took input at `now`; `unparsed`, the pty holds output
    /// it left.
    pub fn input(&mut self, term: &mut impl Hold, now: Instant, unparsed: bool) -> Action {
        if self.timeout.is_none() {
            return Action::None;
        }
        if self.ended(term) {
            return self.after_end(term, now);
        }
        if !self.held {
            return if term.synchronized() {
                self.begin(now)
            } else {
                Action::None
            };
        }
        // The timeout passed and the output it waited for is parsed.
        if self.waiting && !unparsed {
            return self.expire(term);
        }
        Action::None
    }

    /// The timer fired at `now`.
    pub fn timer(&mut self, term: &mut impl Hold, now: Instant, unparsed: bool) -> Action {
        if !self.held {
            return Action::None;
        }
        if self.ended(term) {
            return self.after_end(term, now);
        }
        if self.deadline.is_some_and(|d| now < d) {
            return Action::Arm;
        }
        let wait = self.timeout.unwrap_or_default().max(SYNC_WAIT);
        let until = self.began.and_then(|b| b.checked_add(wait));
        if unparsed && until.is_some_and(|u| now < u) {
            self.waiting = true;
            self.deadline = until;
            return Action::Arm;
        }
        self.expire(term)
    }
}

/// The child's output in the turn in progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Coalesce {
    bytes: usize,
    /// When this turn's first input call began, and the time busy then.
    first: Option<(Instant, Option<Instant>)>,
    /// When the last input call or draw ended.
    busy: Option<Instant>,
}

/// One display's wait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wait {
    presented: Option<Instant>,
    due: Option<Instant>,
}

impl Wait {
    /// When the display's waiting draw is due.
    pub fn due(&self) -> Option<Instant> {
        self.due
    }

    /// The wait's time came, so the draw it held is asked for now, as the
    /// C reference's timer ended its coalescing when it fired
    /// (`coalesce_end`). A draw that cannot run yet waits on what holds
    /// it, a flip or a hold, and not on this time.
    pub fn end(&mut self) {
        self.due = None;
    }
}

impl Coalesce {
    /// An input call parsed `parsed` bytes from `start` to `end`.
    pub fn input(&mut self, start: Instant, end: Instant, parsed: usize) {
        if self.first.is_none() {
            self.first = Some((start, self.busy));
        }
        self.bytes = self.bytes.saturating_add(parsed);
        self.busy = self.busy.max(Some(end));
    }

    /// A draw of a display ran from `start` to `end`; `presented`: it
    /// queued a frame. It ends the display's wait.
    pub fn drawn(&mut self, wait: &mut Wait, start: Instant, end: Instant, presented: bool) {
        wait.due = None;
        // From the draw's start, so presents come a refresh apart.
        if presented {
            wait.presented = Some(start);
        }
        self.busy = self.busy.max(Some(end));
    }

    /// The turn ended with the screen changed at `now`. Returns `None` to
    /// draw now, or the time by which to draw if no turn has by then.
    /// `refresh` is the display's, if it reports one.
    pub fn decide(
        &self,
        holding: bool,
        wait: &mut Wait,
        unparsed: bool,
        now: Instant,
        refresh: Option<Duration>,
    ) -> Option<Instant> {
        if (self.bytes < COALESCE_BYTES && !unparsed) || holding {
            wait.due = None;
            return None;
        }
        let refresh = refresh
            .filter(|r| (REFRESH_MIN..=REFRESH_MAX).contains(r))
            .unwrap_or(REFRESH);
        // A wait keeps the deadline its first turn set.
        let due = wait.due.or_else(|| {
            let (began, before) = self.first.unwrap_or((now, self.busy));
            // After an idle refresh nothing was late until the output began.
            let idle = before.is_none_or(|b| b.checked_add(refresh).is_some_and(|r| began >= r));
            if idle {
                began.checked_add(refresh)
            } else {
                wait.presented.and_then(|p| p.checked_add(refresh))
            }
        });
        wait.due = due.filter(|&d| now < d);
        wait.due
    }

    /// The turn ended: the next input call begins another.
    pub fn turn_end(&mut self) {
        self.bytes = 0;
        self.first = None;
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::arithmetic_side_effects,
    reason = "a fixed origin for the times, and test arithmetic"
)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Term {
        mode: bool,
        frames: u64,
    }

    impl Hold for Term {
        fn synchronized(&self) -> bool {
            self.mode
        }
        fn sync_frames(&self) -> u64 {
            self.frames
        }
        fn end_sync(&mut self) {
            if self.mode {
                self.mode = false;
                self.frames += 1;
            }
        }
    }

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn a_hold_arms_and_its_end_presents() {
        let t0 = Instant::now();
        let mut term = Term::default();
        let mut s = Sync::new(Duration::from_millis(200), 0);
        assert_eq!(s.input(&mut term, t0, false), Action::None);
        term.mode = true;
        assert_eq!(s.input(&mut term, ms(t0, 5), false), Action::Arm);
        assert_eq!(s.deadline(), Some(ms(t0, 205)));
        assert!(s.holding(&term));
        assert_eq!(s.input(&mut term, ms(t0, 10), false), Action::None);
        term.end_sync();
        assert_eq!(s.input(&mut term, ms(t0, 20), false), Action::Present);
        assert!(!s.holding(&term));
    }

    #[test]
    fn a_frame_begun_in_the_same_input_is_held_from_now() {
        let t0 = Instant::now();
        let mut term = Term {
            mode: true,
            frames: 0,
        };
        let mut s = Sync::new(Duration::from_millis(200), 0);
        assert_eq!(s.input(&mut term, t0, false), Action::Arm);
        // One input ended the frame and began the next.
        term.frames = 1;
        assert_eq!(s.input(&mut term, ms(t0, 50), false), Action::Arm);
        assert_eq!(s.deadline(), Some(ms(t0, 250)));
    }

    #[test]
    fn the_timeout_ends_the_hold() {
        let t0 = Instant::now();
        let mut term = Term {
            mode: true,
            frames: 0,
        };
        let mut s = Sync::new(Duration::from_millis(200), 0);
        s.input(&mut term, t0, false);
        assert_eq!(s.timer(&mut term, ms(t0, 100), false), Action::Arm);
        assert_eq!(s.timer(&mut term, ms(t0, 200), false), Action::Expired);
        assert!(!term.mode, "DECRQM reads the mode reset");
        assert_eq!(term.frames, 1);
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn the_timeout_waits_for_unparsed_output_up_to_a_second() {
        let t0 = Instant::now();
        let mut term = Term {
            mode: true,
            frames: 0,
        };
        let mut s = Sync::new(Duration::from_millis(200), 0);
        s.input(&mut term, t0, false);
        assert_eq!(s.timer(&mut term, ms(t0, 200), true), Action::Arm);
        assert_eq!(s.deadline(), Some(ms(t0, 1_000)));
        // Parsed, still held: it expires at once.
        assert_eq!(s.input(&mut term, ms(t0, 300), false), Action::Expired);
        let mut s = Sync::new(Duration::from_millis(200), 1);
        term.mode = true;
        s.input(&mut term, t0, false);
        s.timer(&mut term, ms(t0, 200), true);
        assert_eq!(s.timer(&mut term, ms(t0, 1_000), true), Action::Expired);
    }

    #[test]
    fn zero_offers_no_mode() {
        let t0 = Instant::now();
        let mut term = Term {
            mode: true,
            frames: 0,
        };
        let mut s = Sync::new(Duration::ZERO, 0);
        assert!(!s.enabled());
        assert_eq!(s.input(&mut term, t0, false), Action::None);
        assert!(!s.holding(&term));
    }

    #[test]
    fn a_short_write_draws_at_once() {
        let t0 = Instant::now();
        let mut c = Coalesce::default();
        let mut w = Wait::default();
        c.input(t0, ms(t0, 1), 100);
        assert_eq!(c.decide(false, &mut w, false, ms(t0, 1), None), None);
    }

    #[test]
    fn a_long_write_waits_at_most_a_refresh_from_the_last_present() {
        let t0 = Instant::now();
        let mut c = Coalesce::default();
        let mut w = Wait::default();
        let refresh = Some(Duration::from_millis(16));
        // A present at t0; output that never went idle.
        c.drawn(&mut w, t0, ms(t0, 2), true);
        c.input(ms(t0, 3), ms(t0, 5), COALESCE_BYTES);
        assert_eq!(
            c.decide(false, &mut w, false, ms(t0, 5), refresh),
            Some(ms(t0, 16))
        );
        assert_eq!(w.due(), Some(ms(t0, 16)));
        c.turn_end();
        // Unparsed output waits too, and keeps its deadline.
        c.input(ms(t0, 6), ms(t0, 8), 10);
        assert_eq!(
            c.decide(false, &mut w, true, ms(t0, 8), refresh),
            Some(ms(t0, 16))
        );
        c.turn_end();
        // Past the deadline it draws.
        c.input(ms(t0, 17), ms(t0, 18), COALESCE_BYTES);
        assert_eq!(c.decide(false, &mut w, false, ms(t0, 18), refresh), None);
    }

    #[test]
    fn output_after_an_idle_refresh_counts_from_its_start() {
        let t0 = Instant::now();
        let mut c = Coalesce::default();
        let mut w = Wait::default();
        let refresh = Some(Duration::from_millis(16));
        c.drawn(&mut w, t0, ms(t0, 1), true);
        c.input(ms(t0, 500), ms(t0, 502), COALESCE_BYTES);
        assert_eq!(
            c.decide(false, &mut w, false, ms(t0, 502), refresh),
            Some(ms(t0, 516))
        );
    }

    #[test]
    fn a_hold_decides_for_itself_and_an_odd_refresh_is_60_hz() {
        let t0 = Instant::now();
        let mut c = Coalesce::default();
        let mut w = Wait::default();
        c.drawn(&mut w, t0, ms(t0, 1), true);
        c.input(ms(t0, 2), ms(t0, 3), COALESCE_BYTES);
        assert_eq!(c.decide(true, &mut w, false, ms(t0, 3), None), None);
        assert_eq!(
            c.decide(
                false,
                &mut w,
                false,
                ms(t0, 3),
                Some(Duration::from_secs(1))
            ),
            Some(t0 + REFRESH)
        );
    }
}
