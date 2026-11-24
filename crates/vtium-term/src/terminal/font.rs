//! OSC 50: a program asks for a font by name, or asks which font is in
//! use (ADR-1200 D5). The terminal keeps a name asked for until the host
//! takes it with
//! [`Terminal::take_font_request`](crate::Terminal::take_font_request);
//! the host looks the name up among its frits and built-in fonts,
//! switches, and says which font is in use with
//! [`Terminal::set_font_name`](crate::Terminal::set_font_name).
//!
//! - `OSC 50 ; Pt ST`: Pt names a frit or a built-in font, as
//!   `vtium config set` sends it, or is a font file's path, which the host
//!   checks before it takes the font. A frit's name has 1 to 64 characters
//!   of `A-Z a-z 0-9 . _ + -`, not starting with `.`. A path holds `/`
//!   and has at most `PATH_MAX` bytes of UTF-8. Any other Pt is ignored.
//!   The input call returns after a request, as after a synchronized
//!   update, so the host takes the request before the next byte is parsed.
//! - `OSC 50 ; ? ST`: the reply is `OSC 50 ; NAME ST`, the font in use as
//!   the host named it, ended as the query was. NAME is empty until the
//!   host names a set, and once the host says the font in use has no set's
//!   name ([`Terminal::clear_font_name`](crate::Terminal::clear_font_name)).
//! - xterm's font menu, a Pt starting with `#`, is ignored: vtium has no
//!   menu.
//!
//! Until the host turns font operations on, OSC 50 and its query are
//! ignored, as xterm ignores them under `allowFontOps: false`.

use vtium_parse::StringEnd;

use super::State;
use super::reply::{Query, osc_end};

/// The longest name a frit takes (ADR-1200 D1).
const NAME_MAX: usize = 64;

/// The longest font file path OSC 50 takes, in bytes: the kernel's
/// `PATH_MAX`.
const PATH_MAX: usize = 4096;

/// Whether `pt` can be a font file's path, which the host checks before it
/// takes the font: UTF-8 text of at most [`PATH_MAX`] bytes that holds
/// `/`. The parser gives each byte of the string that is not UTF-8 as
/// U+FFFD, so a `pt` holding U+FFFD came from bytes that were not UTF-8,
/// and no font file has that name. A relative path passes here, so the
/// host can say why it refuses one.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": the host's checks of a path.
pub(super) fn is_font_path(pt: &[u8]) -> bool {
    pt.len() <= PATH_MAX
        && pt.contains(&b'/')
        && core::str::from_utf8(pt).is_ok_and(|text| !text.contains('\u{fffd}'))
}

/// Whether `name` is a font file's absolute path that the query can
/// report: a path that starts with `/` and holds no control character.
pub(super) fn is_reported_path(name: &str) -> bool {
    name.starts_with('/') && is_font_path(name.as_bytes()) && !name.chars().any(char::is_control)
}

/// Whether `name` can name a frit: ADR-1200 D1's characters and length.
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
        } else if is_set_name(pt) || is_font_path(pt) {
            self.font_request = core::str::from_utf8(pt).ok().map(str::to_owned);
            // The input call returns here, so the host switches before
            // the next byte, and a query that follows names the new font.
            self.pause = true;
        }
    }
}
