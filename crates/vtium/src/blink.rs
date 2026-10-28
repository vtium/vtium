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
//! both phases stay shown. With `blink-text=off` there is no text timer,
//! and the text's phase stays shown.
//!
//! What the text's off phase draws is the renderer's, from
//! [`text_blink`]: `blink-text` and `blink-decorations`.

use std::time::Duration;

use vtium_render::{BlinkStyle, TextBlink};

use crate::config::Config;
use crate::config::keys::{BLINK, BLINK_DECORATIONS, BLINK_TEXT};

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

/// The renderer's text blink options from `blink-text` and
/// `blink-decorations`.
pub fn text_blink(config: &Config) -> TextBlink {
    let style = match config.text(&BLINK_TEXT) {
        Some("partial") => BlinkStyle::Partial,
        Some("off") => BlinkStyle::Off,
        _ => BlinkStyle::Full,
    };
    TextBlink {
        style,
        decorations: config.switch(&BLINK_DECORATIONS),
    }
}

/// The blink phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    enabled: bool,
    /// The text timer runs: `blink-text` is not `off`.
    text: bool,
    text_shown: bool,
    cursor_shown: bool,
}

impl Blink {
    /// The phases for the `blink` key's value, both shown.
    pub fn new(enabled: bool) -> Blink {
        Blink {
            enabled,
            text: true,
            text_shown: true,
            cursor_shown: true,
        }
    }

    /// The phases `blink` and `blink-text` ask for, both shown.
    pub fn configured(config: &Config) -> Blink {
        Blink {
            text: text_blink(config).style != BlinkStyle::Off,
            ..Blink::new(config.switch(&BLINK))
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

    /// The timers to arm when the terminal activates: none with `blink`
    /// off, and no text timer with `blink-text` off.
    pub fn activate(&self) -> Option<impl Iterator<Item = (Timer, Arm)> + use<>> {
        let every = Arm {
            first: TEXT_PERIOD,
            every: TEXT_PERIOD,
        };
        let text = self.text;
        self.enabled.then(|| {
            [(Timer::Text, every), (Timer::Cursor, every)]
                .into_iter()
                .filter(move |&(t, _)| text || t == Timer::Cursor)
        })
    }

    /// `timer` fired. Returns whether every display redraws.
    pub fn fired(&mut self, timer: Timer, awake: bool) -> bool {
        if !self.enabled || !awake || (timer == Timer::Text && !self.text) {
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

    fn config(args: &[&str]) -> Config {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        crate::config::load(&args, None, &mut |_: &std::path::Path| Ok(None)).config
    }

    fn armed(b: &Blink) -> Vec<(Timer, Arm)> {
        b.activate().into_iter().flatten().collect()
    }

    #[test]
    fn blink_off_has_no_timer_and_shows_both() {
        let mut b = Blink::new(false);
        assert_eq!(armed(&b), []);
        assert_eq!(b.key(Duration::from_millis(530)), None);
        assert!(!b.fired(Timer::Text, true));
        assert!(b.text_shown() && b.cursor_shown());
    }

    #[test]
    fn both_timers_run_every_500_ms_while_active() {
        let b = Blink::new(true);
        assert_eq!(
            armed(&b),
            [(Timer::Text, EVERY_500), (Timer::Cursor, EVERY_500)]
        );
        assert_eq!(Blink::configured(&config(&[])), b);
    }

    #[test]
    fn blink_text_off_runs_the_cursor_timer_alone() {
        let mut b = Blink::configured(&config(&["--blink-text=off"]));
        assert_eq!(armed(&b), [(Timer::Cursor, EVERY_500)]);
        assert!(!b.fired(Timer::Text, true));
        assert!(b.text_shown());
        assert!(b.fired(Timer::Cursor, true));
        assert!(!b.cursor_shown());
        // blink off stops both whatever blink-text says
        let b = Blink::configured(&config(&["--no-blink", "--blink-text=partial"]));
        assert_eq!(armed(&b), []);
        let b = Blink::configured(&config(&["--blink-text=partial"]));
        assert_eq!(armed(&b).len(), 2);
    }

    #[test]
    fn the_keys_choose_the_renderers_options() {
        let full = TextBlink {
            style: BlinkStyle::Full,
            decorations: false,
        };
        assert_eq!(text_blink(&config(&[])), full);
        assert_eq!(text_blink(&config(&[])), TextBlink::default());
        for (word, style) in [
            ("full", BlinkStyle::Full),
            ("partial", BlinkStyle::Partial),
            ("off", BlinkStyle::Off),
        ] {
            let arg = format!("--blink-text={word}");
            assert_eq!(text_blink(&config(&[&arg])), TextBlink { style, ..full });
            let both = config(&[&arg, "--blink-decorations"]);
            let want = TextBlink {
                style,
                decorations: true,
            };
            assert_eq!(text_blink(&both), want, "{word}");
        }
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
