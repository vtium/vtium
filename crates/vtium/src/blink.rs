//! The blink timers: text with SGR 5 and the blinking cursor
//! (`src/terminal.c:163-165, 715-724, 821-830, 1357-1374, 1764-1795` at
//! kmscon b1f7a7c5).
//!
//! With `blink` on, two timers run while the terminal is active. The text
//! timer fires every [`TEXT_PERIOD`]; the cursor timer every
//! [`TEXT_PERIOD`] too until a key arrives, then [`TYPING_PAUSE`] after
//! each key and at the cursor's own rate from there. Each firing while the
//! terminal is awake flips that timer's phase and redraws every display; a
//! key shows the cursor at once. With `blink` off there are no timers and
//! both phases stay shown.

use std::time::Duration;

/// The text timer's period, and the cursor timer's until the first key.
pub const TEXT_PERIOD: Duration = Duration::from_millis(500);

/// How long the cursor stays shown after a key.
pub const TYPING_PAUSE: Duration = Duration::from_secs(1);

/// Which timer fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    Text,
    Cursor,
}

/// When a timer next fires: after `first`, then every `every`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arm {
    pub first: Duration,
    pub every: Duration,
}

/// The blink phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    enabled: bool,
    text_shown: bool,
    cursor_shown: bool,
}

impl Blink {
    /// The phases for the `blink` key's value, both shown.
    pub fn new(enabled: bool) -> Blink {
        Blink {
            enabled,
            text_shown: true,
            cursor_shown: true,
        }
    }

    /// Whether blinking text is drawn this frame.
    pub fn text_shown(&self) -> bool {
        self.text_shown
    }

    /// Whether a blinking cursor is drawn this frame.
    pub fn cursor_shown(&self) -> bool {
        self.cursor_shown
    }

    /// The timers to arm when the terminal activates; none with `blink`
    /// off.
    pub fn activate(&self) -> Option<[(Timer, Arm); 2]> {
        let every = Arm {
            first: TEXT_PERIOD,
            every: TEXT_PERIOD,
        };
        self.enabled
            .then_some([(Timer::Text, every), (Timer::Cursor, every)])
    }

    /// `timer` fired. Returns whether every display redraws.
    pub fn fired(&mut self, timer: Timer, awake: bool) -> bool {
        if !self.enabled || !awake {
            return false;
        }
        let phase = match timer {
            Timer::Text => &mut self.text_shown,
            Timer::Cursor => &mut self.cursor_shown,
        };
        *phase = !*phase;
        true
    }

    /// A key reached the terminal, whose cursor blinks at `rate`: the
    /// cursor shows, and its timer is armed again; none with `blink` off.
    pub fn key(&mut self, rate: Duration) -> Option<Arm> {
        if !self.enabled {
            return None;
        }
        self.cursor_shown = true;
        Some(Arm {
            first: TYPING_PAUSE,
            every: rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_500: Arm = Arm {
        first: TEXT_PERIOD,
        every: TEXT_PERIOD,
    };

    #[test]
    fn blink_off_has_no_timer_and_shows_both() {
        let mut b = Blink::new(false);
        assert_eq!(b.activate(), None);
        assert_eq!(b.key(Duration::from_millis(530)), None);
        assert!(!b.fired(Timer::Text, true));
        assert!(b.text_shown() && b.cursor_shown());
    }

    #[test]
    fn both_timers_run_every_500_ms_while_active() {
        let b = Blink::new(true);
        assert_eq!(
            b.activate(),
            Some([(Timer::Text, EVERY_500), (Timer::Cursor, EVERY_500)])
        );
    }

    #[test]
    fn each_firing_flips_its_phase_and_redraws() {
        let mut b = Blink::new(true);
        assert!(b.fired(Timer::Text, true));
        assert!(!b.text_shown() && b.cursor_shown());
        assert!(b.fired(Timer::Cursor, true));
        assert!(!b.cursor_shown());
        assert!(b.fired(Timer::Text, true));
        assert!(b.text_shown());
    }

    #[test]
    fn asleep_a_firing_changes_and_draws_nothing() {
        let mut b = Blink::new(true);
        assert!(!b.fired(Timer::Text, false));
        assert!(!b.fired(Timer::Cursor, false));
        assert!(b.text_shown() && b.cursor_shown());
    }

    #[test]
    fn a_key_shows_the_cursor_and_pauses_its_blink() {
        let mut b = Blink::new(true);
        b.fired(Timer::Cursor, true);
        assert!(!b.cursor_shown());
        let rate = Duration::from_millis(530);
        assert_eq!(
            b.key(rate),
            Some(Arm {
                first: TYPING_PAUSE,
                every: rate
            })
        );
        assert!(b.cursor_shown());
        // The text's phase is the text timer's alone.
        b.fired(Timer::Text, true);
        b.key(rate);
        assert!(!b.text_shown());
    }
}
