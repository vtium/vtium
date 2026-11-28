//! The keys of the rebuilt TUI and the decoder that reads them from what
//! the terminal sends. A key is a set of modifiers, Ctrl, Alt and Shift in
//! any combination, and a named key, a function key from F1 to F12 or a
//! character. [`Decoder`] turns the terminal's bytes into keys, mouse
//! reports and replies, as [`Event`]s.
//!
//! A key's Shift is set only where the terminal's sequence says so, as in
//! Shift+Tab or Shift+F10. A character carries Shift in its case, so
//! Shift and `a` arrive as `A`. An Alt letter matches regardless of case
//! ([`Key::is_alt`]), because Caps Lock and Shift turn Alt+o into Alt+O.
//!
//! References:
//! - ADR-1208 D10: the key type, its modifiers and its keys.
//! - ADR-1206 D2, the notes of 2026-11-27: the keys the decoder reads, and
//!   an Alt letter that matches regardless of case.

mod decode;

pub use decode::Decoder;

pub use crate::input::{Button, Mouse, Reply};

/// A set of modifiers: Ctrl, Alt and Shift, in any combination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    /// No modifier.
    pub const NONE: Mods = Mods(0);
    /// Shift.
    pub const SHIFT: Mods = Mods(1);
    /// Alt, which a terminal sends as ESC before the key.
    pub const ALT: Mods = Mods(2);
    /// Ctrl.
    pub const CTRL: Mods = Mods(4);

    /// Whether every modifier of `other` is in the set.
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the set holds no modifier.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The set with the modifiers of `other` added.
    pub const fn with(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    /// The modifiers of xterm's parameter `m`, as in `CSI 1 ; m A`: `m`
    /// less one is a sum of 1 for Shift, 2 for Alt and 4 for Ctrl. Meta,
    /// 8, is left out. `None` for a parameter below 1.
    pub(crate) fn of_param(m: u16) -> Option<Mods> {
        let bits = m.checked_sub(1)?;
        Some(Mods(u8::try_from(bits & 7).unwrap_or(0)))
    }
}

/// What was pressed, without its modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Enter,
    Tab,
    Backspace,
    /// The Esc key.
    Escape,
    /// A function key, from F1 to F12.
    F(u8),
    /// A character, its case as typed.
    Char(char),
}

/// A key and its modifiers, as the TUI acts on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub mods: Mods,
    pub code: Code,
}

impl Key {
    /// The Esc key.
    pub const ESCAPE: Key = Key::plain(Code::Escape);

    /// `code` with no modifier.
    pub const fn plain(code: Code) -> Key {
        Key {
            mods: Mods::NONE,
            code,
        }
    }

    /// The character `c` with no modifier.
    pub const fn char(c: char) -> Key {
        Key::plain(Code::Char(c))
    }

    /// Ctrl and the letter `c`, as a terminal sends Ctrl+S as 0x13.
    pub const fn ctrl(c: char) -> Key {
        Key {
            mods: Mods::CTRL,
            code: Code::Char(c),
        }
    }

    /// Alt and the character `c`.
    pub const fn alt(c: char) -> Key {
        Key {
            mods: Mods::ALT,
            code: Code::Char(c),
        }
    }

    /// The key with the modifiers of `mods` added.
    pub const fn with(self, mods: Mods) -> Key {
        Key {
            mods: self.mods.with(mods),
            code: self.code,
        }
    }

    /// Whether the key is a character typed with no Ctrl or Alt, which an
    /// entry takes as text.
    pub fn typed(&self) -> Option<char> {
        match self.code {
            Code::Char(c) if !self.mods.contains(Mods::CTRL) && !self.mods.contains(Mods::ALT) => {
                Some(c)
            }
            _ => None,
        }
    }

    /// The letter of an Alt chord, in lowercase, such as `o` for Alt+o and
    /// for Alt+O. `None` for any other key, Ctrl+Alt and a letter
    /// included.
    pub fn alt_letter(&self) -> Option<char> {
        match self.code {
            Code::Char(c)
                if self.mods.contains(Mods::ALT)
                    && !self.mods.contains(Mods::CTRL)
                    && c.is_alphabetic() =>
            {
                c.to_lowercase().next()
            }
            _ => None,
        }
    }

    /// Whether the key is Alt and `letter`, in either case, so Caps Lock
    /// and Shift change nothing: Alt+o and Alt+O both answer `true` for
    /// `o` and for `O`.
    pub fn is_alt(&self, letter: char) -> bool {
        self.alt_letter().is_some() && self.alt_letter() == letter.to_lowercase().next()
    }

    /// The key as the action bar writes it: modifiers joined with `+` and
    /// a capital after them, as `Ctrl+S` and `Alt+O`, `Del` for Delete,
    /// and the key table's names for the rest, such as `PgUp` and
    /// `Space`.
    pub fn bar_form(&self) -> String {
        let mut out = String::new();
        for (m, name) in [
            (Mods::CTRL, "Ctrl+"),
            (Mods::ALT, "Alt+"),
            (Mods::SHIFT, "Shift+"),
        ] {
            if self.mods.contains(m) {
                out.push_str(name);
            }
        }
        let modified = !out.is_empty();
        match self.code {
            Code::Char(' ') => out.push_str("Space"),
            Code::Char(c) if modified => out.extend(c.to_uppercase()),
            Code::Char(c) => out.push(c),
            Code::F(n) => {
                out.push('F');
                out.push_str(&n.to_string());
            }
            code => out.push_str(NAMES.iter().find(|(_, c)| *c == code).map_or("", |n| n.0)),
        }
        out
    }

    /// The key that `name` names in the key table's form, such as `Ctrl-S`,
    /// `Alt-O`, `Shift-Tab`, `F10`, `PgUp`, `Delete`, `Space` or `q`.
    /// `None` for a name that is no single key, such as `any letter`.
    pub fn named(name: &str) -> Option<Key> {
        let mut mods = Mods::NONE;
        let mut rest = name;
        while let Some((m, after)) = rest.split_once('-').filter(|(_, a)| !a.is_empty()) {
            mods = mods.with(match m {
                "Ctrl" => Mods::CTRL,
                "Alt" => Mods::ALT,
                "Shift" => Mods::SHIFT,
                _ => return None,
            });
            rest = after;
        }
        let code = if let Some((_, code)) = NAMES.iter().find(|(n, _)| *n == rest) {
            *code
        } else if rest == "Delete" {
            Code::Delete
        } else if rest == "Space" {
            Code::Char(' ')
        } else if let Some(n) = rest
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=12).contains(n))
        {
            Code::F(n)
        } else {
            let mut chars = rest.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            // a letter after Ctrl or Alt is written as a capital, and
            // the terminal sends the letter itself
            if mods.is_empty() {
                Code::Char(c)
            } else {
                Code::Char(c.to_lowercase().next().unwrap_or(c))
            }
        };
        Some(Key { mods, code })
    }
}

/// The named keys as the action bar writes them.
const NAMES: [(&str, Code); 14] = [
    ("Up", Code::Up),
    ("Down", Code::Down),
    ("Left", Code::Left),
    ("Right", Code::Right),
    ("Home", Code::Home),
    ("End", Code::End),
    ("PgUp", Code::PageUp),
    ("PgDn", Code::PageDown),
    ("Ins", Code::Insert),
    ("Del", Code::Delete),
    ("Enter", Code::Enter),
    ("Tab", Code::Tab),
    ("Backspace", Code::Backspace),
    ("Esc", Code::Escape),
];

/// One thing the terminal sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(Key),
    /// A mouse report, as modes 1000 and 1006 send it.
    Mouse(Mouse),
    /// A reply to one of the TUI's requests.
    Reply(Reply),
}
