//! Text that vtium-config prints to a terminal from a source it does not
//! control: a file's path, a reason the font helper gave, a trust rule's
//! verdict that names a file.
//!
//! A file name may hold any byte but `/` and NUL, so a font file in a
//! user's folder can be named with an escape sequence that recolors the
//! terminal or moves its cursor. The font helper reads untrusted fonts,
//! and its reasons can repeat what a font says about itself. [`text`]
//! replaces each control character, and each character that turns text
//! around or hides it, with U+FFFD, and cuts the text to
//! [`SHOWN_BYTES_MAX`] bytes, so the terminal shows such text as text.

use std::path::Path;

/// The most bytes [`text`] gives: the longest path Linux opens, 4,096
/// bytes, fits.
pub const SHOWN_BYTES_MAX: usize = 4096;

/// What [`text`] puts in place of a character it hides.
pub const REPLACEMENT: char = '\u{fffd}';

/// Whether `c` is a character [`text`] replaces: a control character, a
/// zero-width space or joiner, a byte-order mark, or a character that
/// turns the direction of text around.
pub fn hides(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'
        )
}

/// `text` safe to print to a terminal: each character [`hides`] names
/// replaced by U+FFFD, and the text cut at a character's edge to at most
/// [`SHOWN_BYTES_MAX`] bytes, ending in `…` where it is cut.
pub fn text(text: &str) -> String {
    let mut out = String::new();
    let room = SHOWN_BYTES_MAX.saturating_sub('…'.len_utf8());
    for c in text.chars() {
        let c = if hides(c) { REPLACEMENT } else { c };
        if out.len().saturating_add(c.len_utf8()) > room {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

/// The path `path` as [`text`] gives it, each byte that is not UTF-8
/// shown as U+FFFD.
pub fn path(path: &Path) -> String {
    text(&path.to_string_lossy())
}

/// Bytes the font helper sent as text, as [`text`] gives them.
pub fn bytes(bytes: &[u8]) -> String {
    text(&String::from_utf8_lossy(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn an_escape_sequence_in_a_path_is_shown_as_text() {
        let name = std::ffi::OsStr::from_bytes(b"/home/u/.fonts/\x1b[31mred\x1b[0m\xff\t.ttf");
        assert_eq!(
            path(Path::new(name)),
            "/home/u/.fonts/\u{fffd}[31mred\u{fffd}[0m\u{fffd}\u{fffd}.ttf"
        );
        assert_eq!(text("a\u{202e}b\nc\u{9b}d"), "a\u{fffd}b\u{fffd}c\u{fffd}d");
    }

    #[test]
    fn a_long_text_is_cut_at_a_characters_edge() {
        let long = "é".repeat(SHOWN_BYTES_MAX);
        let shown = text(&long);
        assert!(shown.len() <= SHOWN_BYTES_MAX, "{}", shown.len());
        assert!(shown.ends_with("é…"));
        assert_eq!(text("short"), "short");
        assert_eq!(
            bytes(&vec![b'x'; 20 * SHOWN_BYTES_MAX]).len(),
            SHOWN_BYTES_MAX
        );
    }
}
