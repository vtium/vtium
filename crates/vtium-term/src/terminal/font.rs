//! OSC 50: a program asks for a font by name, or asks which font is in
//! use (ADR-1200 D5). The terminal keeps a name asked for until the host
//! takes it with
//! [`Terminal::take_font_request`](crate::Terminal::take_font_request);
//! the host looks the name up among its built sets, switches, and says
//! which set is in use with
//! [`Terminal::set_font_name`](crate::Terminal::set_font_name).
//!
//! - `OSC 50 ; Pt ST`: Pt names a set, as `vtium-vitrify list` spells it.
//!   A Pt that cannot be a set's name is ignored; ADR-1200 D1 allows 1 to
//!   64 characters of `A-Z a-z 0-9 . _ + -`, not starting with `.`.
//! - `OSC 50 ; ? ST`: the reply is `OSC 50 ; NAME ST`, the set in use as
//!   the host named it, ended as the query was.
//! - xterm's font menu, a Pt starting with `#`, is ignored: vtium has no
//!   menu.
//!
//! Until the host turns font operations on, OSC 50 and its query are
//! ignored, as xterm ignores them under `allowFontOps: false`.

use vtium_parse::StringEnd;

use super::State;
use super::reply::{Query, osc_end};

/// The longest name a set takes (ADR-1200 D1).
const NAME_MAX: usize = 64;

/// Whether `name` can name a set: ADR-1200 D1's characters and length.
pub(super) fn is_set_name(name: &[u8]) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && name.first() != Some(&b'.')
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

impl State {
    /// OSC 50 from its `Pt`, the string past `50;`.
    pub(super) fn osc_font(&mut self, pt: &[u8], end: StringEnd) {
        if !self.host.font_ops {
            return;
        }
        if pt == b"?" {
            let reply = format!(
                "\x1b]50;{}{}",
                self.host.font_name.as_deref().unwrap_or_default(),
                osc_end(end)
            );
            self.reply(Query::Font, reply.as_bytes());
        } else if is_set_name(pt) {
            self.font_request = core::str::from_utf8(pt).ok().map(str::to_owned);
        }
    }
}
