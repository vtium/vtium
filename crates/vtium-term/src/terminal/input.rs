//! What the terminal sends the program for the keyboard, the pointer,
//! focus and pastes, as the host reports them: xterm's encodings, ported
//! from the C reference's flow.

use super::modes::{MouseEncoding, MouseTracking};
use super::{State, Terminal};
use vtium_parse::InputMode;

/// The modifiers held with a key or a pointer event, as the replay
/// interface numbers them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const SHIFT: Self = Self(1);
    pub const LOCK: Self = Self(2);
    pub const CTRL: Self = Self(4);
    pub const ALT: Self = Self(8);
    pub const LOGO: Self = Self(16);

    pub const fn empty() -> Self {
        Self(0)
    }

    /// From the bits above; others are dropped.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 31)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Shift, Alt and Ctrl, the modifiers xterm encodes.
    const fn encoded(self) -> Self {
        Self(self.0 & (Self::SHIFT.0 | Self::ALT.0 | Self::CTRL.0))
    }

    /// xterm's modifier parameter: 1 plus Shift 1, Alt 2, Ctrl 4; `None`
    /// with none of them.
    fn param(self) -> Option<u8> {
        let m = self.encoded();
        let bits = u8::from(m.contains(Self::SHIFT))
            | u8::from(m.contains(Self::ALT)) << 1
            | u8::from(m.contains(Self::CTRL)) << 2;
        (bits != 0).then(|| bits.saturating_add(1))
    }
}

/// A pointer button, as the host reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    /// A wheel step, reported as a press.
    WheelUp,
    WheelDown,
    /// No button: motion with nothing held.
    None,
}

/// A pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    Press,
    Release,
    /// Motion; its button is the one held, or none.
    Move,
}

/// The X11 keysyms the encoding names (xkbcommon's
/// `xkbcommon-keysyms.h`).
mod sym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const LINEFEED: u32 = 0xff0a;
    pub const CLEAR: u32 = 0xff0b;
    pub const RETURN: u32 = 0xff0d;
    pub const SYS_REQ: u32 = 0xff15;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const SELECT: u32 = 0xff60;
    pub const INSERT: u32 = 0xff63;
    pub const FIND: u32 = 0xff68;
    pub const KP_SPACE: u32 = 0xff80;
    pub const KP_TAB: u32 = 0xff89;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const KP_F1: u32 = 0xff91;
    pub const KP_F4: u32 = 0xff94;
    pub const KP_HOME: u32 = 0xff95;
    pub const KP_LEFT: u32 = 0xff96;
    pub const KP_UP: u32 = 0xff97;
    pub const KP_RIGHT: u32 = 0xff98;
    pub const KP_DOWN: u32 = 0xff99;
    pub const KP_PAGE_UP: u32 = 0xff9a;
    pub const KP_PAGE_DOWN: u32 = 0xff9b;
    pub const KP_END: u32 = 0xff9c;
    pub const KP_INSERT: u32 = 0xff9e;
    pub const KP_DELETE: u32 = 0xff9f;
    pub const KP_MULTIPLY: u32 = 0xffaa;
    pub const KP_ADD: u32 = 0xffab;
    pub const KP_SEPARATOR: u32 = 0xffac;
    pub const KP_SUBTRACT: u32 = 0xffad;
    pub const KP_DECIMAL: u32 = 0xffae;
    pub const KP_DIVIDE: u32 = 0xffaf;
    pub const KP_0: u32 = 0xffb0;
    pub const KP_9: u32 = 0xffb9;
    pub const KP_EQUAL: u32 = 0xffbd;
    pub const F1: u32 = 0xffbe;
    pub const F20: u32 = 0xffd1;
    pub const DELETE: u32 = 0xffff;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
}

/// The C0 byte Ctrl turns `sym` into, or `None` for a key Ctrl leaves as it
/// is: xterm's bytes (its us-pc105 table, mode 0). `sym` is the ASCII
/// layout's keysym, so Ctrl+C works whatever layout is active.
fn ctrl_byte(sym: u32) -> Option<u8> {
    let c = u8::try_from(sym).ok()?;
    Some(match c {
        b'a'..=b'z' | b'A'..=b'Z' => c & 0x1f,
        b'2' | b' ' | b'@' | b'`' => 0x00,
        b'3' | b'[' | b'{' => 0x1b,
        b'4' | b'\\' | b'|' => 0x1c,
        b'5' | b']' | b'}' => 0x1d,
        b'6' | b'^' | b'~' => 0x1e,
        b'7' | b'/' | b'_' => 0x1f,
        b'8' | b'?' => 0x7f,
        _ => return None,
    })
}

/// The character a printable key made, after Shift: from the keysym where
/// it is Latin-1, else the host's character; none for function, keypad and
/// other keysyms from 0xFD00 up (ctlseqs, "Alt and Meta Keys").
fn key_char(keysym: u32, unicode: Option<char>) -> Option<u32> {
    if (0x20..=0x7e).contains(&keysym) || (0xa0..=0xff).contains(&keysym) {
        return Some(keysym);
    }
    if (0xfd00..=0xffff).contains(&keysym) {
        return None;
    }
    let c = u32::from(unicode?);
    (c >= 0x20 && c != 0x7f).then_some(c)
}

/// The function keys' codes, `CSI code ~`, and Shift's code for F1 to
/// F10 (xterm's F11 to F20 with no modifier, as the C reference sent
/// them).
fn function_key(n: u32, shift: bool) -> Vec<u8> {
    const CODES: [u8; 20] = [
        11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
    ];
    let code = |i: u32| {
        usize::try_from(i)
            .ok()
            .and_then(|i| CODES.get(i))
            .copied()
            .unwrap_or(0)
    };
    match (n, shift) {
        (0..=3, false) => vec![
            0x1b,
            b'O',
            b'P'.saturating_add(u8::try_from(n).unwrap_or(0)),
        ],
        (0..=9, true) => format!("\x1b[{}~", code(n.saturating_add(10))).into_bytes(),
        (_, true) => format!("\x1b[{};2~", code(n)).into_bytes(),
        (_, false) => format!("\x1b[{}~", code(n)).into_bytes(),
    }
}

impl State {
    /// xterm's modifyOtherKeys (aaronsb/libtsm#25): at level 1 or 2, a key
    /// pressed with Shift, Alt or Ctrl is sent as `CSI 27 ; mod ; code ~`
    /// where the rules say so; `None` leaves the key to the usual
    /// encoding. The rules follow ctlseqs ("Alt and Meta Keys") and xterm's
    /// "Modified Other Keys (us-pc105)" table, as the C reference read
    /// them:
    ///
    /// - Level 1 encodes a printable key with Alt, or with Ctrl where Ctrl
    ///   makes no control byte, and Ctrl+?; space and Escape only with
    ///   Alt; Tab and Return with any modifier; never BackSpace.
    /// - Level 2 encodes all of them with any modifier, but BackSpace not
    ///   with Ctrl alone, and with Shift alone only space and `@` to `~`.
    /// - Shift+Tab arrives as ISO_Left_Tab: `CSI Z` at level 1, and Tab
    ///   with its modifiers at level 2.
    fn modified_key(
        &self,
        keysym: u32,
        sym: u32,
        mods: Mods,
        unicode: Option<char>,
    ) -> Option<Vec<u8>> {
        let m = mods.encoded();
        let param = m.param()?;
        let level2 = self.modify_other_keys == 2;
        let alt = m.contains(Mods::ALT);
        let code = match keysym {
            sym::TAB => 9,
            sym::RETURN => 13,
            sym::ISO_LEFT_TAB if level2 => 9,
            sym::ISO_LEFT_TAB => return None,
            sym::ESCAPE if level2 || alt => 27,
            sym::ESCAPE => return None,
            sym::BACKSPACE if !level2 || m == Mods::CTRL => return None,
            sym::BACKSPACE if self.host.backspace_delete => 127,
            sym::BACKSPACE => 8,
            _ => {
                let code = key_char(keysym, unicode)?;
                if level2 {
                    let shifted = code == 0x20 || (0x40..=0x7e).contains(&code);
                    if m == Mods::SHIFT && !shifted {
                        return None;
                    }
                } else if !alt
                    && (!m.contains(Mods::CTRL)
                        || (ctrl_byte(sym).is_some() && sym != u32::from(b'?')))
                {
                    return None;
                }
                code
            }
        };
        Some(format!("\x1b[27;{param};{code}~").into_bytes())
    }

    /// A key's bytes, and whether Alt may put ESC before them; `None` for
    /// a key that sends nothing.
    fn key_bytes(
        &self,
        keysym: u32,
        sym: u32,
        mods: Mods,
        unicode: Option<char>,
    ) -> Option<(Vec<u8>, bool)> {
        if self.modify_other_keys > 0 {
            if let Some(bytes) = self.modified_key(keysym, sym, mods, unicode) {
                return Some((bytes, false));
            }
        }
        if mods.contains(Mods::CTRL) {
            if let Some(byte) = ctrl_byte(sym) {
                return Some((vec![byte], true));
            }
        }
        let app = self.keypad_application;
        let keypad = |app_final: u8, plain: u8| {
            if app {
                vec![0x1b, b'O', app_final]
            } else {
                vec![plain]
            }
        };
        let cursor = |final_byte: u8| match (mods.param(), self.modes.cursor_keys()) {
            (Some(m), _) => (format!("\x1b[1;{m}").into_bytes(), false, final_byte),
            (None, true) => (b"\x1bO".to_vec(), true, final_byte),
            (None, false) => (b"\x1b[".to_vec(), true, final_byte),
        };
        let page = |code: u8| {
            let ctrl = mods.contains(Mods::CTRL);
            let shift = mods.contains(Mods::SHIFT);
            let m = match (ctrl, shift) {
                (true, true) => ";6",
                (true, false) => ";5",
                (false, true) => ";2",
                (false, false) => "",
            };
            format!("\x1b[{code}{m}~").into_bytes()
        };
        let bytes = match keysym {
            sym::BACKSPACE if self.host.backspace_delete => vec![0x7f],
            sym::BACKSPACE => vec![0x08],
            sym::TAB | sym::KP_TAB => vec![0x09],
            sym::ISO_LEFT_TAB => b"\x1b[Z".to_vec(),
            sym::LINEFEED => vec![0x0a],
            sym::CLEAR => vec![0x0b],
            sym::SYS_REQ => vec![0x15],
            sym::ESCAPE => vec![0x1b],
            sym::KP_ENTER if app => b"\x1bOM".to_vec(),
            sym::KP_ENTER | sym::RETURN if self.modes.newline() => b"\r\n".to_vec(),
            sym::KP_ENTER | sym::RETURN => vec![0x0d],
            sym::FIND => b"\x1b[1~".to_vec(),
            sym::INSERT => b"\x1b[2~".to_vec(),
            sym::DELETE => b"\x1b[3~".to_vec(),
            sym::SELECT => b"\x1b[4~".to_vec(),
            sym::PAGE_UP | sym::KP_PAGE_UP => page(5),
            sym::PAGE_DOWN | sym::KP_PAGE_DOWN => page(6),
            sym::UP
            | sym::KP_UP
            | sym::DOWN
            | sym::KP_DOWN
            | sym::RIGHT
            | sym::KP_RIGHT
            | sym::LEFT
            | sym::KP_LEFT => {
                let final_byte = match keysym {
                    sym::UP | sym::KP_UP => b'A',
                    sym::DOWN | sym::KP_DOWN => b'B',
                    sym::RIGHT | sym::KP_RIGHT => b'C',
                    _ => b'D',
                };
                let (mut bytes, alt_esc, final_byte) = cursor(final_byte);
                bytes.push(final_byte);
                return Some((bytes, alt_esc));
            }
            sym::KP_INSERT => keypad(b'p', b'0'),
            sym::KP_0..=sym::KP_9 => {
                let digit = u8::try_from(keysym.saturating_sub(sym::KP_0)).unwrap_or(0);
                keypad(b'p'.saturating_add(digit), b'0'.saturating_add(digit))
            }
            sym::KP_SUBTRACT => keypad(b'm', b'-'),
            sym::KP_SEPARATOR => keypad(b'l', b','),
            sym::KP_DELETE | sym::KP_DECIMAL => keypad(b'n', b'.'),
            sym::KP_EQUAL | sym::KP_DIVIDE => keypad(b'j', b'/'),
            sym::KP_MULTIPLY => keypad(b'o', b'*'),
            sym::KP_ADD => keypad(b'k', b'+'),
            sym::HOME | sym::KP_HOME if self.modes.cursor_keys() => b"\x1bOH".to_vec(),
            sym::HOME | sym::KP_HOME => b"\x1b[H".to_vec(),
            sym::END | sym::KP_END if self.modes.cursor_keys() => b"\x1bOF".to_vec(),
            sym::END | sym::KP_END => b"\x1b[F".to_vec(),
            sym::KP_SPACE => vec![b' '],
            sym::KP_F1..=sym::KP_F4 => function_key(
                keysym.saturating_sub(sym::KP_F1),
                mods.contains(Mods::SHIFT),
            ),
            sym::F1..=sym::F20 => {
                function_key(keysym.saturating_sub(sym::F1), mods.contains(Mods::SHIFT))
            }
            _ => {
                let mut text = Vec::with_capacity(4);
                self.push_char(unicode?, &mut text);
                text
            }
        };
        Some((bytes, true))
    }

    /// Appends a typed or pasted character in the input's encoding: one
    /// byte in 7-bit and 8-bit input, `?` for a character they cannot
    /// carry.
    fn push_char(&self, c: char, out: &mut Vec<u8>) {
        let max = match self.input_mode {
            InputMode::Utf8 => {
                let mut buf = [0; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                return;
            }
            InputMode::SevenBit => 0x7f,
            InputMode::EightBit => 0xff,
        };
        out.push(
            u8::try_from(u32::from(c))
                .ok()
                .filter(|&b| u32::from(b) <= max)
                .unwrap_or(b'?'),
        );
    }

    /// A pointer event's report, as the tracking and the encoding the
    /// program set ask; `None` when it is not reported.
    fn mouse_bytes(
        &mut self,
        cell: (usize, usize),
        pixel: (u32, u32),
        button: MouseButton,
        event: MouseEvent,
        mods: Mods,
    ) -> Option<Vec<u8>> {
        let tracking = self.modes.tracking();
        let encoding = self.modes.encoding();
        let moved = event == MouseEvent::Move;
        let released = event == MouseEvent::Release;
        let held = matches!(
            button,
            MouseButton::Left | MouseButton::Middle | MouseButton::Right
        );
        // 9 reports presses, 1000 releases too, 1002 motion with a button
        // held, and 1003 all motion.
        match tracking {
            MouseTracking::Off => return None,
            MouseTracking::X10 if moved || released => return None,
            MouseTracking::Normal if moved => return None,
            MouseTracking::Button if moved && !held => return None,
            _ => {}
        }
        let mut code: u8 = match (button, event) {
            (MouseButton::Left, _) => 0,
            (MouseButton::Middle, _) => 1,
            (MouseButton::Right, _) => 2,
            // A wheel step has no release, and moves nothing.
            (MouseButton::WheelUp, MouseEvent::Press) => 64,
            (MouseButton::WheelDown, MouseEvent::Press) => 65,
            // Motion with no button is button 3, a release.
            (MouseButton::None, MouseEvent::Move) => 3,
            _ => return None,
        };
        if moved {
            code = code.saturating_add(32);
        }
        // X10 has codes for three buttons and no modifiers.
        let mut mods = mods.encoded();
        if tracking == MouseTracking::X10 {
            if code >= 64 {
                return None;
            }
            mods = Mods::empty();
        }
        // Motion is reported when it enters a cell other than the last one
        // reported, by any event; in pixels, whenever it moves.
        if encoding != MouseEncoding::Pixels {
            if moved && self.mouse_last == Some(cell) {
                return None;
            }
            self.mouse_last = Some(cell);
        }
        let mod_bits = u8::from(mods.contains(Mods::SHIFT)) << 2
            | u8::from(mods.contains(Mods::ALT)) << 3
            | u8::from(mods.contains(Mods::CTRL)) << 4;
        let cb = code | mod_bits;
        let end = if released { 'm' } else { 'M' };
        let (col, row) = cell;
        Some(match encoding {
            // The top left cell, and pixel, is 1,1 (aaronsb/libtsm#67).
            MouseEncoding::Sgr => format!(
                "\x1b[<{cb};{};{}{end}",
                col.saturating_add(1),
                row.saturating_add(1)
            )
            .into_bytes(),
            MouseEncoding::Pixels => format!(
                "\x1b[<{cb};{};{}{end}",
                pixel.0.saturating_add(1),
                pixel.1.saturating_add(1)
            )
            .into_bytes(),
            // One byte each, plus 32: a release is button 3, and the cell
            // counts from 1 and stops at 223.
            MouseEncoding::X10 => {
                let cb = if released { 3 | mod_bits } else { cb };
                let pos = |n: usize| u8::try_from(n.saturating_add(0x21)).unwrap_or(0xff);
                vec![
                    0x1b,
                    b'[',
                    b'M',
                    cb.saturating_add(0x20),
                    pos(col),
                    pos(row),
                ]
            }
        })
    }
}

/// What a pasted character becomes: with xterm's paste filtering, its
/// `allowPasteControls` false, each C0 control but HT, LF and CR, DEL, and
/// each C1 control is one space; and LF is CR, as xterm sends it with
/// `pasteLiteralNl` off (`button.c`, `_WriteSelectionData`).
fn pasted(c: char) -> char {
    match c {
        '\n' => '\r',
        '\0'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\x7f'..='\u{9f}' => ' ',
        _ => c,
    }
}

impl State {
    /// Pasted text: read as UTF-8, ill-formed bytes as U+FFFD, each
    /// character as [`pasted`] makes it, and sent in the input's encoding
    /// as a typed character is, so in 8-bit input "é" is the byte 0xE9 and
    /// "€" is `?`.
    fn paste_bytes(&self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        for c in String::from_utf8_lossy(data).chars() {
            self.push_char(pasted(c), &mut out);
        }
        out
    }
}

impl Terminal {
    /// Sends what a key press makes, as the host reports it: `keysym` the
    /// X11 keysym after Shift, `ascii` the keysym an ASCII layout gives
    /// the same key (0 for none), `unicode` the character it makes. Alt
    /// puts ESC before the key's bytes where xterm does. Returns whether
    /// anything was sent.
    pub fn key(&mut self, keysym: u32, ascii: u32, mods: Mods, unicode: Option<char>) -> bool {
        // Ctrl reads the ASCII layout's keysym, so Ctrl+C works whatever
        // layout is active.
        let sym = if ascii == 0 { keysym } else { ascii };
        let Some((bytes, alt_esc)) = self.state.key_bytes(keysym, sym, mods, unicode) else {
            return false;
        };
        let mut out = Vec::with_capacity(bytes.len().saturating_add(1));
        if alt_esc && mods.contains(Mods::ALT) {
            out.push(0x1b);
        }
        out.extend_from_slice(&bytes);
        self.send(&out);
        true
    }

    /// Reports a pointer event at cell `col`, `row` and pixel `px`, `py`,
    /// counted from 0 at the top left of the text area, as the program's
    /// tracking and encoding ask. Of `mods`, Shift, Alt and Ctrl are
    /// reported. Returns whether anything was sent.
    #[allow(
        clippy::too_many_arguments,
        reason = "an event's position, button and modifiers"
    )]
    pub fn mouse(
        &mut self,
        col: usize,
        row: usize,
        px: u32,
        py: u32,
        button: MouseButton,
        event: MouseEvent,
        mods: Mods,
    ) -> bool {
        let Some(bytes) = self
            .state
            .mouse_bytes((col, row), (px, py), button, event, mods)
        else {
            return false;
        };
        self.send(&bytes);
        true
    }

    /// Reports that the terminal gained or lost focus, while the program
    /// asked for it (mode 1004). Returns whether anything was sent.
    pub fn focus(&mut self, focused: bool) -> bool {
        if !self.state.modes.focus() {
            return false;
        }
        self.send(if focused { b"\x1b[I" } else { b"\x1b[O" });
        true
    }

    /// Pastes `data`, UTF-8 text, filtered as xterm filters a paste, each
    /// LF as CR, in the input's encoding, and bracketed by `CSI 200 ~` and
    /// `CSI 201 ~` while the program asked for it (mode 2004). The
    /// filtering turns ESC and the C1 controls into spaces, so a bracketed
    /// paste never carries its own end marker (ADR-1000).
    pub fn paste(&mut self, data: &[u8]) {
        let text = self.state.paste_bytes(data);
        if !self.state.modes.bracketed_paste() {
            self.send(&text);
            return;
        }
        let mut out = b"\x1b[200~".to_vec();
        out.extend(text);
        out.extend_from_slice(b"\x1b[201~");
        self.send(&out);
    }
}
