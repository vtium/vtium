//! The log lines about the messages the broker refuses.
//!
//! A refused message comes from a drawer that is broken or taken over,
//! and such a drawer can send refused messages in a loop. One line for
//! each would fill the journal, and journald would then drop vtium's other
//! lines too, since it limits how many lines each unit may write. So the
//! broker logs the first refusal in full. After it, the broker writes at
//! most one line every [`QUIET`], which counts the refusals since the last
//! line and gives the reason for the latest.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": the broker refuses a message,
//!   logs it, and keeps serving the drawer.

use std::fmt;
use std::time::{Duration, Instant};

use super::protocol::Refusal;

/// The shortest time between two lines about refused messages. journald
/// lets a unit write 10,000 lines in 30 s by default, and one line a
/// second stays far below that, while it still shows a flood as it goes
/// on.
pub const QUIET: Duration = Duration::from_secs(1);

/// A line to log about refused messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// The first message the broker refused.
    First(Refusal),
    /// How many messages the broker refused since the last line, and why
    /// it refused the latest of them.
    Since { count: u64, latest: Refusal },
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Line::First(why) => write!(
                f,
                "vtium's drawing process sent a message that vtium refuses, so vtium ignores the message: {why}"
            ),
            Line::Since { count: 1, latest } => write!(
                f,
                "vtium's drawing process sent 1 more message that vtium refuses and ignores: {latest}"
            ),
            Line::Since { count, latest } => write!(
                f,
                "vtium's drawing process sent {count} more messages that vtium refuses and ignores; the latest: {latest}"
            ),
        }
    }
}

/// The refusals the broker has seen and not yet logged.
#[derive(Debug, Default)]
pub struct Refusals {
    /// When the broker wrote its last line about refused messages.
    last: Option<Instant>,
    /// The refusals since that line.
    held: u64,
    latest: Option<Refusal>,
}

impl Refusals {
    /// Notes a refusal at `now`, and gives the line to log, if any.
    pub fn note(&mut self, why: Refusal, now: Instant) -> Option<Line> {
        if self.last.is_none() {
            self.last = Some(now);
            return Some(Line::First(why));
        }
        self.held = self.held.saturating_add(1);
        self.latest = Some(why);
        self.tick(now)
    }

    /// When the line that counts the held refusals is due.
    pub fn due(&self) -> Option<Instant> {
        let last = self.last?;
        (self.held > 0).then(|| last.checked_add(QUIET).unwrap_or(last))
    }

    /// The line that counts the held refusals, once it is due at `now`.
    pub fn tick(&mut self, now: Instant) -> Option<Line> {
        if self.due().is_some_and(|due| now >= due) {
            self.last = Some(now);
            self.take()
        } else {
            None
        }
    }

    /// The line that counts the held refusals, due or not, for the end of
    /// the drawer.
    pub fn end(&mut self) -> Option<Line> {
        self.take()
    }

    fn take(&mut self) -> Option<Line> {
        let latest = self.latest.take()?;
        let count = std::mem::take(&mut self.held);
        Some(Line::Since { count, latest })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[expect(
        clippy::disallowed_methods,
        reason = "a starting point for the test's own clock, which the test moves by hand"
    )]
    fn start() -> Instant {
        Instant::now()
    }

    /// A drawer that sends 100,000 refused messages over 3.5 s gets one
    /// line in full and then at most one line a second, and the lines
    /// count every refusal.
    #[test]
    fn a_flood_of_refusals_gets_one_line_a_second() {
        let t0 = start();
        let mut refusals = Refusals::default();
        let mut lines = Vec::new();
        let total: u64 = 100_000;
        for i in 0..total {
            let now = t0 + Duration::from_micros(i * 35);
            lines.extend(refusals.note(Refusal::Field("vt"), now));
        }
        lines.extend(refusals.end());
        assert_eq!(lines.first(), Some(&Line::First(Refusal::Field("vt"))));
        assert!(lines.len() <= 1 + 3 + 1, "{lines:?}");
        let counted: u64 = lines
            .iter()
            .map(|line| match line {
                Line::First(_) => 1,
                Line::Since { count, .. } => *count,
            })
            .sum();
        assert_eq!(counted, total);
    }

    /// A refusal after the first waits for its line until a second has
    /// passed since the last line; the line comes with the next refusal
    /// or at the due time, whichever the broker's loop reaches first.
    #[test]
    fn a_held_refusal_is_logged_when_its_line_is_due() {
        let t0 = start();
        let mut refusals = Refusals::default();
        assert_eq!(refusals.due(), None);
        assert!(refusals.note(Refusal::Padding, t0).is_some());
        assert_eq!(refusals.due(), None, "nothing is held");
        assert_eq!(
            refusals.note(Refusal::Size(0), t0 + Duration::from_millis(10)),
            None
        );
        assert_eq!(refusals.due(), Some(t0 + QUIET));
        assert_eq!(refusals.tick(t0 + Duration::from_millis(999)), None);
        assert_eq!(
            refusals.tick(t0 + QUIET),
            Some(Line::Since {
                count: 1,
                latest: Refusal::Size(0),
            })
        );
        assert_eq!(refusals.due(), None);
        assert_eq!(refusals.end(), None, "nothing is left to log");
    }

    /// Each line starts with its topic and says what vtium does.
    #[test]
    fn the_lines_say_what_vtium_does() {
        let first = Line::First(Refusal::Field("vt")).to_string();
        assert!(
            first.starts_with("vtium's drawing process sent a message"),
            "{first}"
        );
        assert!(first.contains("ignores"), "{first}");
        let one = Line::Since {
            count: 1,
            latest: Refusal::Padding,
        };
        assert!(one.to_string().contains(" 1 more message that"), "{one}");
        let many = Line::Since {
            count: 9_999,
            latest: Refusal::Padding,
        };
        assert!(many.to_string().contains(" 9999 more messages"), "{many}");
    }
}
