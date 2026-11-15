//! The Linux console's private `CSI Ps ; n ]` sequences (console_codes(4))
//! that vtium acts on: the screen blank's timeout, the wake, and the
//! cursor's blink interval. The terminal keeps what was asked until the
//! host takes or reads it. The host powers the displays and decides whether
//! a program may, through
//! [`Terminal::take_blank_request`](crate::Terminal::take_blank_request),
//! and it blinks the cursor at
//! [`Terminal::cursor_blink_ms`](crate::Terminal::cursor_blink_ms).
//!
//! - `CSI 9 ; n ]`: the blank timeout is n minutes, at most 60; 0, or none,
//!   turns blanking off. The screen wakes too, as on the Linux console.
//! - `CSI 13 ]`: the screen wakes, as at a key.
//! - `CSI 14 ; n ]`, the VESA powerdown interval: ignored, since vtium has
//!   one DPMS step, which `CSI 9` times.
//! - `CSI 16 ; n ]`: the cursor blinks with phases of n milliseconds, n
//!   from 50 to 65535. Any other n, or none, returns the interval to the
//!   default, as the Linux console does. vtium's default is the `blink`
//!   key's interval, which blinking text keeps whatever the sequence says.
//!
//! RIS returns the timeout to the host's configured one and the cursor's
//! interval to the default, as the Linux console's reset does. DECSTR keeps
//! both, since the Linux console has no DECSTR.

use vtium_parse::Params;

use super::State;

/// The Linux console's cap on the blank timeout, in minutes.
const MINUTES_MAX: u8 = 60;

/// The cursor blink intervals the Linux console takes, in milliseconds.
/// It returns any other interval to its default.
const BLINK_MS: core::ops::RangeInclusive<u32> = 50..=65_535;

/// What a program asked of the screen blank since the host last took it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlankRequest {
    /// The timeout set last.
    pub timeout: Option<BlankTimeout>,
    /// The screen is to wake: the blank ends, and its countdown starts
    /// again.
    pub wake: bool,
}

/// A blank timeout a program set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlankTimeout {
    /// `CSI 9 ; n ]`: n minutes, 0 to 60; 0 never blanks.
    Minutes(u8),
    /// RIS: the host's configured timeout.
    Configured,
}

impl State {
    /// `CSI Ps ; n ]`: the blank sequences and the cursor blink interval.
    pub(super) fn console_private(&mut self, params: &Params) {
        match params.get(0) {
            Some(9) => {
                let n = params.get(1).unwrap_or(0);
                let minutes = u8::try_from(n).map_or(MINUTES_MAX, |m| m.min(MINUTES_MAX));
                self.blank = BlankRequest {
                    timeout: Some(BlankTimeout::Minutes(minutes)),
                    wake: true,
                };
            }
            Some(13) => self.blank.wake = true,
            Some(16) => {
                self.cursor_blink_ms = params.get(1).filter(|n| BLINK_MS.contains(n));
            }
            _ => {}
        }
    }

    /// RIS's part: the cursor blinks at the default interval again.
    pub(super) fn reset_cursor_blink_interval(&mut self) {
        self.cursor_blink_ms = None;
    }

    /// RIS's part: the host's timeout again, with no wake.
    pub(super) fn reset_blank_timeout(&mut self) {
        self.blank.timeout = Some(BlankTimeout::Configured);
    }
}
