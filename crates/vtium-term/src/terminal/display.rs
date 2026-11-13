//! setBackground and setForeground: kmscon's private OSC strings, with
//! which a program hands the display to a graphical program and takes it
//! back (ADR-1000, "Replies and policy"). No DEC or ECMA specification
//! defines them. A program writes each one as an OSC string with no
//! number, `OSC setBackground ST` and `OSC setForeground ST`, as the
//! released kmscon reads them (`src/terminal.c`, `osc_event`). BEL ends
//! each one too. The terminal keeps the request until the host takes it
//! with [`Terminal::take_display_request`](crate::Terminal::take_display_request),
//! and the host decides what the hand-over gives up (ADR-1401 D16).
//!
//! - The input call returns after each request, as after OSC 50, so the
//!   host takes the requests one at a time and in order. A setBackground
//!   and a setForeground in the same write each reach the host.
//! - Until the host turns display operations on, both strings are
//!   ignored, as vtium's `allow-display-ops` off ignores them.
//! - Neither string gets a reply. Any other text, such as one with
//!   parameters, is not one of the two strings.

use super::State;

/// What a program asked of the display with setBackground or
/// setForeground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayRequest {
    /// setBackground: the program hands the display to a graphical
    /// program.
    Background,
    /// setForeground: the program takes the display back.
    Foreground,
}

impl State {
    /// setBackground or setForeground from an OSC string's text: whether
    /// the text is one of them. Each waits for the host while display
    /// operations are on.
    pub(super) fn osc_display(&mut self, data: &[u8]) -> bool {
        let request = match data {
            b"setBackground" => DisplayRequest::Background,
            b"setForeground" => DisplayRequest::Foreground,
            _ => return false,
        };
        if self.host.display_ops {
            self.display_request = Some(request);
            // The input call returns here, so the host takes this request
            // before the next byte, and a second one in the same write is
            // taken in its own turn.
            self.pause = true;
        }
        true
    }
}
