//! Which chords a key step may press, judged by the bytes a terminal sends
//! for them. A terminal sends Ctrl and a letter as one control byte, and
//! Alt and a key as ESC and the key. Some of those bytes belong to another
//! key, and some open a control sequence or a control string that a
//! terminal's reader takes as the start of a command, never as a key.
//!
//! The crate knows nothing of the TUI, so it judges only the chords whose
//! bytes are the same in every terminal. The player in vtium-config still
//! runs every key through the TUI's own reader and refuses what that
//! reader does not make into a key.
//!
//! References:
//! - ADR-1208 D3: a key reaches the TUI as a terminal sends it, Ctrl+I,
//!   Ctrl+M, Ctrl+H and Ctrl+[ arrive as Tab, Enter, Backspace and Escape,
//!   and Alt+[, Alt+Shift+P, Alt+] and Alt+Shift+_ fail when the macro is
//!   parsed.

use crate::{Key, KeyPress, Mods};

/// Why a key step cannot press a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unpressable {
    /// `#` with no modifier, which a line of a macro reads as a comment. A
    /// macro types it with `Type "#"`.
    Hash,
    /// Alt and a character that a terminal sends as ESC and that byte,
    /// which opens a control sequence or a control string. It holds the
    /// byte after ESC: `[`, `P`, `]` or `_`.
    Opens(char),
}

impl Unpressable {
    /// The message for a key step written as `written`, as in `Alt+[
    /// cannot be pressed: a terminal sends it as ESC [, which opens a
    /// control sequence.`
    pub fn message(self, written: &str) -> String {
        match self {
            Unpressable::Hash => "Type # with Type \"#\".".to_owned(),
            Unpressable::Opens(c) => {
                let what = match c {
                    '[' => "a control sequence",
                    'P' => "a device control string",
                    ']' => "an operating system command",
                    _ => "an application program command",
                };
                format!(
                    "{written} cannot be pressed: a terminal sends it as ESC {c}, which opens {what}."
                )
            }
        }
    }
}

/// Ctrl with each character that a terminal sends as the byte of another
/// key, and that key: Ctrl+I is the tab byte, Ctrl+M the carriage return,
/// Ctrl+H the backspace byte, and Ctrl+[ ESC. A terminal sends Ctrl and a
/// letter the same way in either case.
const SAME_BYTE: [(char, Key); 7] = [
    ('I', Key::Tab),
    ('i', Key::Tab),
    ('M', Key::Enter),
    ('m', Key::Enter),
    ('H', Key::Backspace),
    ('h', Key::Backspace),
    ('[', Key::Escape),
];

/// The bytes after ESC that open a control sequence or a control string:
/// CSI, DCS, OSC and APC in their 7-bit form.
const OPENERS: [char; 4] = ['[', 'P', ']', '_'];

/// The key a terminal sends for a chord, as the TUI would get it: the
/// chord itself, the key that shares its byte, or the reason no terminal
/// can send it as a key.
pub(crate) fn as_sent(press: KeyPress) -> Result<KeyPress, Unpressable> {
    let Key::Char(c) = press.key else {
        return Ok(press);
    };
    let c = c.get();
    let ctrl_only = Mods {
        ctrl: true,
        ..Mods::NONE
    };
    if press.mods == ctrl_only
        && let Some((_, key)) = SAME_BYTE.iter().find(|(k, _)| *k == c)
    {
        return Ok(KeyPress::plain(*key));
    }
    if press.mods.alt && !press.mods.ctrl {
        // Shift and a letter sends the capital letter.
        let sent = if press.mods.shift {
            c.to_ascii_uppercase()
        } else {
            c
        };
        if OPENERS.contains(&sent) {
            return Err(Unpressable::Opens(sent));
        }
    }
    if press.mods.is_empty() && c == '#' {
        return Err(Unpressable::Hash);
    }
    Ok(press)
}
