//! A macro's key as the TUI gets it from a terminal. The player hands
//! vtium-term the key, takes the bytes a vtium console sends for it, and
//! decodes them with the TUI's own decoder. The key the TUI gets is the
//! one key the decoder makes of those bytes. A key whose bytes the
//! decoder makes into no key, or into more than one, cannot be pressed,
//! and the player refuses it when it parses the macro.
//!
//! A letter after a modifier names the key, and only Shift makes it a
//! capital, so `Alt+L` sends `ESC l` and `Alt+Shift+L` sends `ESC L`. A
//! character with no modifier is the character as the macro wrote it.
//!
//! References:
//! - ADR-1208 D3, "A key reaches the TUI as a terminal sends it": the
//!   bytes of a vtium console, the TUI's decoder, and the refusal at
//!   parse.
//! - ADR-1208 D2: a letter after a modifier names the key.

use vtium_config_macro::{Character, Key as MacroKey, KeyPress, Mods as MacroMods};
use vtium_config_widgets::keys::{Button, Code, Decoder, Event, Key, Mods, Mouse};
use vtium_term::{Key as TermKey, MouseButton, MouseEvent, Terminal};

use crate::port::MOUSE_ON;

/// A vtium console's terminal, which turns a key into the bytes it sends.
/// Its screen is the TUI's default, 100×30, and mouse reporting is on, as
/// the TUI turns it on for every screen.
pub struct Encoder {
    term: Terminal,
}

impl Default for Encoder {
    fn default() -> Encoder {
        Encoder::new()
    }
}

impl Encoder {
    /// A fresh console with mouse reporting on.
    pub fn new() -> Encoder {
        let mut term = Terminal::new(100, 30, 0);
        let mut rest = MOUSE_ON;
        while !rest.is_empty() {
            let n = term.input(rest).max(1);
            rest = rest.get(n..).unwrap_or_default();
        }
        let _ = term.take_replies();
        Encoder { term }
    }

    /// The bytes a vtium console sends for `press`, empty where it sends
    /// nothing.
    pub fn bytes(&mut self, press: KeyPress) -> Vec<u8> {
        let _ = self.term.take_replies();
        if self.term.key(&term_key(press)) {
            self.term.take_replies()
        } else {
            Vec::new()
        }
    }

    /// The key the TUI gets for `press`, or why a terminal cannot send it
    /// as a key, in a sentence that starts with the key as the macro
    /// writes it.
    pub fn key(&mut self, press: KeyPress) -> Result<Key, String> {
        let bytes = self.bytes(press);
        if bytes.is_empty() {
            return Err(format!(
                "{press} cannot be pressed: a terminal sends nothing for it."
            ));
        }
        let events = Decoder::new().feed(&bytes);
        match events.as_slice() {
            [Event::Key(key)] => Ok(*key),
            _ => Err(format!(
                "{press} cannot be pressed: a terminal sends it as {}, which the TUI reads as {}.",
                shown(&bytes),
                read_as(&events)
            )),
        }
    }

    /// The events the TUI gets for a click of the left button at the
    /// screen's cell `col`, `row`, counted from 0: the press and the
    /// release, as a vtium console reports them while mouse reporting is
    /// on. Empty where the console reports nothing.
    pub fn click(&mut self, col: u16, row: u16) -> Vec<Event> {
        let _ = self.term.take_replies();
        let (col, row) = (usize::from(col), usize::from(row));
        let none = vtium_term::Mods::empty();
        let pressed = self
            .term
            .mouse(col, row, 0, 0, MouseButton::Left, MouseEvent::Press, none);
        let released =
            self.term
                .mouse(col, row, 0, 0, MouseButton::Left, MouseEvent::Release, none);
        if !pressed && !released {
            return Vec::new();
        }
        Decoder::new().feed(&self.term.take_replies())
    }
}

/// The X11 keysym of each named key a macro presses.
fn keysym(key: MacroKey) -> u32 {
    match key {
        MacroKey::Enter => 0xff0d,
        MacroKey::Escape => 0xff1b,
        MacroKey::Tab => 0xff09,
        MacroKey::Space => 0x20,
        MacroKey::Backspace => 0xff08,
        MacroKey::Up => 0xff52,
        MacroKey::Down => 0xff54,
        MacroKey::Left => 0xff51,
        MacroKey::Right => 0xff53,
        MacroKey::PageUp => 0xff55,
        MacroKey::PageDown => 0xff56,
        MacroKey::Home => 0xff50,
        MacroKey::End => 0xff57,
        MacroKey::F1 => 0xffbe,
        MacroKey::F2 => 0xffbf,
        MacroKey::F3 => 0xffc0,
        MacroKey::F4 => 0xffc1,
        MacroKey::F5 => 0xffc2,
        MacroKey::F6 => 0xffc3,
        MacroKey::F7 => 0xffc4,
        MacroKey::F8 => 0xffc5,
        MacroKey::F9 => 0xffc6,
        MacroKey::F10 => 0xffc7,
        MacroKey::F11 => 0xffc8,
        MacroKey::F12 => 0xffc9,
        MacroKey::Char(c) => char_keysym(c.get()),
    }
}

/// The keysym of a character: its code point in Latin-1, and the code
/// point with 0x1000000 added beyond it.
fn char_keysym(c: char) -> u32 {
    let cp = u32::from(c);
    if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        cp
    } else {
        cp | 0x0100_0000
    }
}

/// The key a host reports to vtium-term for `press`.
fn term_key(press: KeyPress) -> TermKey {
    let mods = term_mods(press.mods);
    match press.key {
        MacroKey::Char(c) => {
            let c = sent_char(press.mods, c.get());
            let sym = char_keysym(c);
            let ascii = if c.is_ascii() { sym } else { 0 };
            TermKey::press(sym, ascii, mods, Some(c))
        }
        MacroKey::Space => TermKey::press(0x20, 0x20, mods, Some(' ')),
        // Shift and Tab is the keysym ISO_Left_Tab, as a keymap gives it
        MacroKey::Tab if press.mods.shift => TermKey::press(0xfe20, 0, mods, None),
        key => TermKey::press(keysym(key), 0, mods, None),
    }
}

/// The character a key step types: after a modifier, a letter names the
/// key and Shift alone makes it a capital; with no modifier, the
/// character as written.
fn sent_char(mods: MacroMods, c: char) -> char {
    if mods.is_empty() || !c.is_ascii_alphabetic() {
        c
    } else if mods.shift {
        c.to_ascii_uppercase()
    } else {
        c.to_ascii_lowercase()
    }
}

fn term_mods(mods: MacroMods) -> vtium_term::Mods {
    let mut out = vtium_term::Mods::empty();
    for (on, m) in [
        (mods.shift, vtium_term::Mods::SHIFT),
        (mods.ctrl, vtium_term::Mods::CTRL),
        (mods.alt, vtium_term::Mods::ALT),
    ] {
        if on {
            out = out.union(m);
        }
    }
    out
}

/// The bytes as a person reads them: ESC, a printable character as
/// itself, and any other byte in hex, with a space between each.
fn shown(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x1b => "ESC".to_owned(),
            0x21..=0x7e => char::from(b).to_string(),
            0x20 => "SP".to_owned(),
            _ => format!("0x{b:02X}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the decoder made of the bytes, for the refusal.
fn read_as(events: &[Event]) -> String {
    let keys: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key(k) => Some(k.bar_form()),
            _ => None,
        })
        .collect();
    match (events.len(), keys.len()) {
        (0, _) => "no key".to_owned(),
        (_, 0) => "no key, but a report or a reply".to_owned(),
        (n, _) => format!("{n} inputs, {}", keys.join(" then ")),
    }
}

/// The key a macro writes for `key`, as the TUI decodes it, or `None` for
/// a key that a macro has no name for, such as Insert and Delete. A
/// [`Encoder::key`] of the result gives `key` again for every key the
/// decoder makes from a vtium console's bytes.
pub fn press_of(key: &Key) -> Option<KeyPress> {
    let named = match key.code {
        Code::Up => MacroKey::Up,
        Code::Down => MacroKey::Down,
        Code::Left => MacroKey::Left,
        Code::Right => MacroKey::Right,
        Code::Home => MacroKey::Home,
        Code::End => MacroKey::End,
        Code::PageUp => MacroKey::PageUp,
        Code::PageDown => MacroKey::PageDown,
        Code::Enter => MacroKey::Enter,
        Code::Tab => MacroKey::Tab,
        Code::Backspace => MacroKey::Backspace,
        Code::Escape => MacroKey::Escape,
        Code::F(n) => *MacroKey::NAMED
            .get(usize::from(n).checked_add(12)?)
            .map(|(_, k)| k)
            .filter(|_| (1..=12).contains(&n))?,
        Code::Char(' ') => MacroKey::Space,
        Code::Char(c) => MacroKey::Char(Character::new(c)?),
        Code::Insert | Code::Delete => return None,
    };
    Some(KeyPress {
        mods: MacroMods {
            ctrl: key.mods.contains(Mods::CTRL),
            alt: key.mods.contains(Mods::ALT),
            shift: key.mods.contains(Mods::SHIFT),
        },
        key: named,
    })
}

/// The key the action bar writes in its brackets, such as `Ctrl+S`,
/// `PgDn` or `Space`, as a macro's key, or `None` for a key a macro has
/// no name for.
pub fn press_of_bar(bar_key: &str) -> Option<KeyPress> {
    let mut name = bar_key.to_owned();
    for (bar, mac) in [("PgUp", "PageUp"), ("PgDn", "PageDown")] {
        if let Some(stem) = name.strip_suffix(bar) {
            name = format!("{stem}{mac}");
        }
    }
    if name.ends_with("Del") || name.ends_with("Ins") {
        return None;
    }
    let path: vtium_config_macro::KeyPath = name.parse().ok()?;
    match path.keys() {
        [one] => Some(*one),
        _ => None,
    }
}

/// Whether `event` is the press of the left button.
pub fn is_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Mouse(Mouse {
            button: Button::Press(0),
            ..
        })
    )
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn press(text: &str) -> KeyPress {
        let path: vtium_config_macro::KeyPath = text.parse().expect("a key");
        *path.keys().first().expect("one key")
    }

    #[test]
    fn a_letter_after_a_modifier_names_the_key_and_shift_makes_it_a_capital() {
        let mut e = Encoder::new();
        assert_eq!(e.bytes(press("Alt+L")), b"\x1bl");
        assert_eq!(e.bytes(press("Alt+Shift+L")), b"\x1bL");
        assert_eq!(e.key(press("Alt+L")).unwrap(), Key::alt('l'));
        assert_eq!(e.key(press("Ctrl+S")).unwrap(), Key::ctrl('s'));
        assert_eq!(e.key(press("q")).unwrap(), Key::char('q'));
        assert_eq!(e.key(press("A")).unwrap(), Key::char('A'));
    }

    #[test]
    fn the_named_keys_arrive_as_the_decoder_reads_them() {
        let mut e = Encoder::new();
        let plain = |c| Key::plain(c);
        for (name, want) in [
            ("Enter", plain(Code::Enter)),
            ("Escape", Key::ESCAPE),
            ("Tab", plain(Code::Tab)),
            ("Shift+Tab", plain(Code::Tab).with(Mods::SHIFT)),
            ("Space", Key::char(' ')),
            ("Backspace", plain(Code::Backspace)),
            ("Up", plain(Code::Up)),
            ("PageDown", plain(Code::PageDown)),
            ("Home", plain(Code::Home)),
            ("F1", plain(Code::F(1))),
            ("F10", plain(Code::F(10))),
            ("Ctrl+Up", plain(Code::Up).with(Mods::CTRL)),
        ] {
            assert_eq!(e.key(press(name)), Ok(want), "{name}");
        }
    }

    #[test]
    fn a_chord_that_arrives_as_two_keys_or_none_is_refused_with_its_bytes() {
        let mut e = Encoder::new();
        assert_eq!(
            e.key(press("Alt+Escape")).unwrap_err(),
            "Alt+Escape cannot be pressed: a terminal sends it as ESC ESC, which the TUI reads as 2 inputs, Esc then Esc."
        );
        assert_eq!(
            e.key(press("Ctrl+Space")).unwrap_err(),
            "Ctrl+Space cannot be pressed: a terminal sends it as 0x00, which the TUI reads as no key."
        );
        // ESC and a control byte in one write is Ctrl+Alt and the letter
        assert_eq!(
            e.key(press("Ctrl+Alt+X")),
            Ok(Key::ctrl('x').with(Mods::ALT))
        );
    }

    #[test]
    fn every_decoded_key_writes_a_press_that_arrives_as_it_again() {
        let mut e = Encoder::new();
        for name in [
            "Alt+N",
            "Ctrl+S",
            "F10",
            "Down",
            "Shift+Tab",
            "Space",
            "q",
            "A",
            "Enter",
            "Escape",
            "PageUp",
            "Ctrl+Z",
            "?",
        ] {
            let key = e.key(press(name)).unwrap();
            let back = press_of(&key).expect("a name");
            assert_eq!(e.key(back), Ok(key), "{name}");
        }
    }

    #[test]
    fn the_bar_s_keys_become_a_macro_s_keys() {
        assert_eq!(press_of_bar("Ctrl+S"), Some(press("Ctrl+S")));
        assert_eq!(press_of_bar("PgDn"), Some(press("PageDown")));
        assert_eq!(press_of_bar("Space"), Some(press("Space")));
        assert_eq!(press_of_bar("Del"), None);
    }

    #[test]
    fn a_click_arrives_as_a_press_and_a_release_at_its_cell() {
        let mut e = Encoder::new();
        let events = e.click(4, 7);
        assert_eq!(events.len(), 2, "{events:?}");
        let Some(Event::Mouse(m)) = events.first() else {
            panic!("a mouse report: {events:?}");
        };
        assert_eq!((m.col, m.row), (4, 7));
        assert!(events.first().is_some_and(is_press));
    }
}
