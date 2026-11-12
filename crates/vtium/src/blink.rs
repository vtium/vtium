//! The blink timers: text with SGR 5, text with SGR 6 and the blinking
//! cursor.
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
//! A third timer, the rapid timer, blinks the text SGR 6 marks. ECMA-48
//! §8.3.117 defines SGR 5 as "slowly blinking (less than 150 per minute)"
//! and SGR 6 as "rapidly blinking (150 per minute or more)". The text
//! timer's 500 ms phases blink 60 times a minute. The rapid timer flips
//! its phase every [`RAPID_PERIOD`], 200 ms, which blinks 150 times a
//! minute: the slowest rate ECMA-48 allows for SGR 6. At that rate the
//! text flashes 2.5 times a second, under the three flashes a second that
//! WCAG 2.2's success criterion 2.3.1 sets as the limit for content that
//! flashes. The rapid timer runs only while a cell the terminal shows
//! carries SGR 6, so a screen without rapidly blinking text never wakes
//! vtium five times a second. It stops on what stops the text timer:
//! `blink` off, `blink-text=off`, or a terminal that is not awake. When it
//! stops, its phase returns to shown.
//!
//! What the text's off phase draws is the renderer's, from
//! [`text_blink`]: `blink-text` and `blink-decorations`.

use std::time::{Duration, Instant};

use vtium_render::{BlinkStyle, TextBlink};
use vtium_term::{Age, Attrs, Terminal};

use crate::config::Config;
use crate::config::keys::{BLINK, BLINK_DECORATIONS, BLINK_TEXT};

/// The text timer's period, and the cursor timer's until the first key.
pub const TEXT_PERIOD: Duration = Duration::from_millis(500);

/// The rapid timer's period: each phase of SGR 6's blink, 150 blinks a
/// minute (ECMA-48 §8.3.117).
pub const RAPID_PERIOD: Duration = Duration::from_millis(200);

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
        Some("full") => BlinkStyle::Full,
        Some("off") => BlinkStyle::Off,
        _ => BlinkStyle::Partial,
    };
    TextBlink {
        style,
        decorations: config.switch(&BLINK_DECORATIONS),
    }
}

/// Whether a cell `terminal` shows carries SGR 6, rapid blink: what the
/// rapid timer runs for. It reads each shown line's styles, the scrollback
/// a view shows included.
pub fn rapid_on_screen(terminal: &Terminal) -> bool {
    let screen = terminal.screen();
    let frame = screen.frame(terminal.palette(), Age::NEVER);
    let cols = frame.cols();
    (0..frame.rows()).filter_map(|r| frame.line(r)).any(|line| {
        line.cells()
            .iter()
            .take(cols)
            .any(|&cell| line.style(cell).attrs.contains(Attrs::RAPID_BLINK))
    })
}

/// The blink phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    enabled: bool,
    /// The text timer runs: `blink-text` is not `off`.
    text: bool,
    text_shown: bool,
    cursor_shown: bool,
    /// The rapid timer's phase.
    rapid_shown: bool,
    /// When the rapid timer next fires, while it runs.
    rapid_due: Option<Instant>,
    /// A cell the terminal shows carries SGR 6, as last seen.
    rapid_cells: bool,
}

impl Blink {
    /// The phases for the `blink` key's value, both shown.
    pub fn new(enabled: bool) -> Blink {
        Blink {
            enabled,
            text: true,
            text_shown: true,
            cursor_shown: true,
            rapid_shown: true,
            rapid_due: None,
            rapid_cells: false,
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

    /// Whether rapidly blinking text, SGR 6, is drawn this frame.
    pub fn rapid_shown(&self) -> bool {
        self.rapid_shown
    }

    /// When the rapid timer next fires, or `None` while it is stopped.
    pub fn rapid_due(&self) -> Option<Instant> {
        self.rapid_due
    }

    /// Records whether a cell the terminal shows carries SGR 6, which
    /// [`rapid_on_screen`] reads. The rapid timer runs only while one does.
    pub fn rapid_seen(&mut self, on_screen: bool) {
        self.rapid_cells = on_screen;
    }

    /// The rapid timer's turn at `now`. While it should run, which needs
    /// `blink` on, `blink-text` not `off`, an `awake` terminal and a cell
    /// that carries SGR 6, it is armed [`RAPID_PERIOD`] ahead, and when it
    /// is due it flips the phase and is armed again. Otherwise it stops,
    /// and its phase returns to shown. Returns whether every display
    /// redraws: only when the phase flipped.
    pub fn rapid_turn(&mut self, now: Instant, awake: bool) -> bool {
        if !(self.enabled && self.text && awake && self.rapid_cells) {
            self.rapid_stop();
            return false;
        }
        let ahead = |t: Instant| t.checked_add(RAPID_PERIOD).unwrap_or(t);
        match self.rapid_due {
            None => {
                self.rapid_due = Some(ahead(now));
                false
            }
            Some(due) if due <= now => {
                self.rapid_shown = !self.rapid_shown;
                // a timer left behind, by a console shown again after a
                // while, flips once and is armed from now
                let next = ahead(due);
                self.rapid_due = Some(if next <= now { ahead(now) } else { next });
                true
            }
            Some(_) => false,
        }
    }

    /// Stops the rapid timer, its phase shown: the terminal stops drawing.
    pub fn rapid_stop(&mut self) {
        self.rapid_due = None;
        self.rapid_shown = true;
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
        crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
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
        let partial = TextBlink {
            style: BlinkStyle::Partial,
            decorations: false,
        };
        assert_eq!(text_blink(&config(&[])), partial);
        assert_eq!(text_blink(&config(&[])), TextBlink::default());
        for (word, style) in [
            ("full", BlinkStyle::Full),
            ("partial", BlinkStyle::Partial),
            ("off", BlinkStyle::Off),
        ] {
            let arg = format!("--blink-text={word}");
            assert_eq!(text_blink(&config(&[&arg])), TextBlink { style, ..partial });
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

    // ECMA-48 §8.3.117: "150 per minute or more". Five turns 200 ms apart
    // flip the phase five times, 2.5 blinks a second, as fast as the text
    // timer's phase would take 1250 ms to.
    // covers: sgr.rapid-blink
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn the_rapid_phase_flips_every_200_ms_while_a_cell_carries_sgr_6() {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let mut b = Blink::new(true);
        assert!(!b.rapid_turn(t, true));
        assert_eq!(b.rapid_due(), None, "no cell, no timer");
        b.rapid_seen(true);
        assert!(!b.rapid_turn(t, true), "arming draws nothing");
        assert_eq!(b.rapid_due(), Some(ms(200)));
        assert!(!b.rapid_turn(ms(199), true));
        let mut flips = 0;
        for n in (200..=1000).step_by(200) {
            let was = b.rapid_shown();
            assert!(b.rapid_turn(ms(n), true), "a flip at {n} ms redraws");
            assert_ne!(b.rapid_shown(), was);
            assert_eq!(b.rapid_due(), Some(ms(n + 200)));
            flips += 1;
        }
        assert_eq!(flips, 5);
        // a blink, both phases, in 400 ms or less: 150 a minute or more
        assert!(RAPID_PERIOD * 2 <= Duration::from_secs(60) / 150);
        // the text and the cursor keep their own phases
        assert!(b.text_shown() && b.cursor_shown());
    }

    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn the_rapid_timer_stops_shown_with_no_cell_or_asleep_and_catches_up() {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let mut b = Blink::new(true);
        b.rapid_seen(true);
        b.rapid_turn(t, true);
        b.rapid_turn(ms(200), true);
        assert!(!b.rapid_shown());
        b.rapid_seen(false);
        assert!(!b.rapid_turn(ms(250), true));
        assert_eq!((b.rapid_due(), b.rapid_shown()), (None, true));
        b.rapid_seen(true);
        b.rapid_turn(ms(300), true);
        assert!(!b.rapid_turn(ms(500), false), "asleep, nothing draws");
        assert_eq!((b.rapid_due(), b.rapid_shown()), (None, true));
        // a timer left far behind flips once and is armed from now
        b.rapid_turn(ms(600), true);
        assert!(b.rapid_turn(ms(5000), true));
        assert_eq!(b.rapid_due(), Some(ms(5200)));
        // off and blink-text=off run no rapid timer
        for mut b in [
            Blink::new(false),
            Blink::configured(&config(&["--blink-text=off"])),
        ] {
            b.rapid_seen(true);
            assert!(!b.rapid_turn(t, true));
            assert!(!b.rapid_turn(ms(200), true));
            assert_eq!((b.rapid_due(), b.rapid_shown()), (None, true));
        }
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
