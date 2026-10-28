//! The Linux console's blank sequences (console_codes(4)), which a program
//! sends to set the screen blank's timeout and to wake the screen. The
//! terminal keeps what was asked until the host takes it with
//! [`Terminal::take_blank_request`](crate::Terminal::take_blank_request);
//! the host powers the displays, and decides whether a program may.
//!
//! - `CSI 9 ; n ]`: the blank timeout is n minutes, at most 60; 0, or none,
//!   turns blanking off. The screen wakes too, as on the Linux console.
//! - `CSI 13 ]`: the screen wakes, as at a key.
//! - `CSI 14 ; n ]`, the VESA powerdown interval: ignored, since vtium has
//!   one DPMS step, which `CSI 9` times.
//!
//! RIS returns the timeout to the host's configured one.

use vtium_parse::Params;

use super::State;

/// The Linux console's cap on the blank timeout, in minutes.
const MINUTES_MAX: u8 = 60;

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
    /// `CSI Ps ; n ]` but the cursor blink's: the blank sequences.
    pub(super) fn console_blank(&mut self, params: &Params) {
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
            _ => {}
        }
    }

    /// RIS's part: the host's timeout again, with no wake.
    pub(super) fn reset_blank_timeout(&mut self) {
        self.blank.timeout = Some(BlankTimeout::Configured);
    }
}
