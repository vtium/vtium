//! OSC 50: a program asks for a font by name, or asks which font is in
//! use (ADR-1200 D5). The terminal keeps a name asked for until the host
//! takes it with
//! [`Terminal::take_font_request`](crate::Terminal::take_font_request);
//! the host looks the name up among its frits and built-in fonts,
//! switches, and says which font is in use with
//! [`Terminal::set_font_name`](crate::Terminal::set_font_name).
//!
//! - `OSC 50 ; Pt ST`: Pt names a frit or a built-in font, as
//!   `vtium config set` sends it, or an outline font family, or is a font
//!   file's path, which the host checks before it takes the font. A
//!   frit's name has 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, not
//!   starting with `.`. A family's name has 1 to 128 bytes of letters and
//!   digits of any script, spaces and `- _ . + ' &`, starts with a letter
//!   or a digit and does not end in a space. A path holds `/` and has at
//!   most 4095 bytes of UTF-8. Any other Pt is ignored. The input call
//!   returns after a request, as after a synchronized update, so the host
//!   takes the request before the next byte is parsed.
//! - `OSC 50 ; ? ST`: the reply is `OSC 50 ; NAME ST`, the font in use as
//!   the host named it, ended as the query was. NAME is empty until the
//!   host names a font, and once the host says the font in use has no
//!   name the query can report
//!   ([`Terminal::clear_font_name`](crate::Terminal::clear_font_name)).
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

/// The longest font file path OSC 50 takes, in bytes. It is 4095, the
/// longest path the kernel opens: the kernel's `PATH_MAX` is 4096 bytes,
/// and it counts the zero byte that ends the path.
const PATH_MAX: usize = 4095;

/// The longest outline family's name OSC 50 takes, in bytes. It is far
/// above the longest family name a font in common use carries, and it
/// keeps the query's reply short.
const FAMILY_MAX: usize = 128;

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
/// report. It is the path in its plain form: it starts with `/`, and each
/// component between slashes is neither empty, `.` nor `..`, so it holds
/// no `//` and does not end in `/`. It also holds no control character
/// and no `#`, and has no whitespace at either end. The host's first
/// check of a path refuses every other path, because the frit reader
/// would cut the line at `#` or trim the spaces and read another file.
/// So every path the query reports is one a switch can take back.
///
/// References:
/// - ADR-1200 D5: every name the query reports is one a switch takes, and
///   "A qualifying font" gives the first check.
pub(super) fn is_reported_path(name: &str) -> bool {
    name.strip_prefix('/')
        .is_some_and(|rest| rest.split('/').all(|part| !matches!(part, "" | "." | "..")))
        && is_font_path(name.as_bytes())
        && !name.chars().any(char::is_control)
        && !name.contains('#')
        && name.trim() == name
}

/// Whether `name` can be an outline font family's name, which the host
/// makes a frit of when the head's configuration gives the family. The
/// name has 1 to [`FAMILY_MAX`] bytes. It starts with a letter or a digit
/// and does not end in a space. Its other characters are letters and
/// digits of any script, the space, and `- _ . + ' &`. The rule leaves out
/// every character that could end the query's reply or confuse a program
/// or a person that reads it: control and format characters such as
/// ESC, DEL and the bidirectional overrides, and `;`. It also leaves out
/// `/`, which makes a path, `#` and `=`, which a frit's line reads as
/// syntax, and every space but U+0020.
///
/// References:
/// - ADR-1200 D5: the query reports a frit made from a family by the
///   family's name, and a switch takes that name back.
pub(super) fn is_family(name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric();
    name.len() <= FAMILY_MAX
        && name.chars().next().is_some_and(word)
        && !name.ends_with(' ')
        && name
            .chars()
            .all(|c| word(c) || matches!(c, ' ' | '-' | '_' | '.' | '+' | '\'' | '&'))
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
        } else if is_set_name(pt)
            || core::str::from_utf8(pt).is_ok_and(is_family)
            || is_font_path(pt)
        {
            self.font_request = core::str::from_utf8(pt).ok().map(str::to_owned);
            // The input call returns here, so the host switches before
            // the next byte, and a query that follows names the new font.
            self.pause = true;
        }
    }
}
