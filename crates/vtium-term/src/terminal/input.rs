//! What the terminal sends the program for the keyboard, the pointer,
//! focus and pastes, as the host reports them: xterm's encodings.

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
    /// NumLock is on: a state, as Lock is, which mode 1035 reads.
    pub const NUM: Self = Self(32);

    pub const fn empty() -> Self {
        Self(0)
    }

    /// From the bits above; others are dropped.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 63)
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
    pub const KP_BEGIN: u32 = 0xff9d;
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

/// A keypad key that makes a character with NumLock on: the space, Tab,
/// Enter, the operators, the digits and `=`, which mode 1035 sends as
/// characters whatever DECKPAM says, as xterm's numLock does.
fn keypad_char_key(keysym: u32) -> bool {
    matches!(
        keysym,
        sym::KP_SPACE | sym::KP_TAB | sym::KP_ENTER | sym::KP_EQUAL
    ) || (sym::KP_MULTIPLY..=sym::KP_9).contains(&keysym)
}

/// The code of F5 to F20 in `CSI code ~` (ctlseqs, "PC-Style Function
/// Keys"); F1 to F4 send `SS3 P` to `SS3 S` instead. `n` counts from 0
/// for F1.
fn function_code(n: u32) -> Option<u8> {
    const CODES: [u8; 16] = [
        15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
    ];
    let i = usize::try_from(n.checked_sub(4)?).ok()?;
    CODES.get(i).copied()
}

/// A key xterm sends as `CSI final`, or `SS3 final` where `ss3` says so,
/// and with a modifier as `CSI 1 ; m final` (ctlseqs, "PC-Style Function
/// Keys", with modifyCursorKeys and modifyFunctionKeys at xterm's default,
/// 2).
fn lettered(final_byte: u8, ss3: bool, mods: Mods) -> Vec<u8> {
    match (mods.param(), ss3) {
        (Some(m), _) => format!("\x1b[1;{m}{}", char::from(final_byte)).into_bytes(),
        (None, true) => vec![0x1b, b'O', final_byte],
        (None, false) => vec![0x1b, b'[', final_byte],
    }
}

/// A key xterm sends as `CSI code ~`, and with a modifier as
/// `CSI code ; m ~`.
fn numbered(code: u8, mods: Mods) -> Vec<u8> {
    match mods.param() {
        Some(m) => format!("\x1b[{code};{m}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

/// A keypad key in application mode: `SS3 final`, and with a modifier
/// `SS3 m final`, as xterm sends it with modifyKeypadKeys at its default,
/// 0.
fn keypad_application(final_byte: u8, mods: Mods) -> Vec<u8> {
    match mods.param() {
        Some(m) => vec![0x1b, b'O', b'0'.saturating_add(m), final_byte],
        None => vec![0x1b, b'O', final_byte],
    }
}

impl State {
    /// xterm's modifyOtherKeys: at level 1 or 2, a key
    /// pressed with Shift, Alt or Ctrl is sent as `CSI 27 ; mod ; code ~`
    /// where the rules say so; `None` leaves the key to the usual
    /// encoding. The rules follow ctlseqs ("Alt and Meta Keys") and xterm's
    /// "Modified Other Keys (us-pc105)" table:
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

    /// A key's bytes, and whether Alt may act on them; `None` for
    /// a key that sends nothing.
    fn key_bytes(
        &self,
        keysym: u32,
        sym: u32,
        mods: Mods,
        unicode: Option<char>,
    ) -> Option<(Vec<u8>, bool)> {
        if self.modify_other_keys > 0
            && let Some(bytes) = self.modified_key(keysym, sym, mods, unicode)
        {
            return Some((bytes, false));
        }
        if mods.contains(Mods::CTRL)
            && let Some(byte) = ctrl_byte(sym)
        {
            return Some((vec![byte], true));
        }
        let app = self.modes.keypad_application()
            && !(self.modes.num_lock() && mods.contains(Mods::NUM) && keypad_char_key(keysym));
        // Alt never puts ESC before the keypad's keys or the editing,
        // cursor and function keys. xterm carries Alt in their modifier
        // parameter, and drops it on a keypad key in numeric mode.
        let keypad = |app_final: u8, plain: u8| {
            if app {
                keypad_application(app_final, mods)
            } else {
                vec![plain]
            }
        };
        let ckm = self.modes.cursor_keys();
        let bytes = match keysym {
            sym::KP_ENTER if app => keypad_application(b'M', mods),
            sym::KP_ENTER if self.modes.newline() => b"\r\n".to_vec(),
            sym::KP_ENTER => vec![0x0d],
            sym::KP_TAB => keypad(b'I', b'\t'),
            sym::KP_SPACE => keypad(b' ', b' '),
            sym::ISO_LEFT_TAB => b"\x1b[Z".to_vec(),
            sym::FIND => numbered(1, mods),
            // With NumLock off the keypad's Insert and Delete are the
            // editing keys, as xterm sends them.
            sym::INSERT | sym::KP_INSERT => numbered(2, mods),
            sym::DELETE | sym::KP_DELETE => numbered(3, mods),
            sym::SELECT => numbered(4, mods),
            sym::PAGE_UP | sym::KP_PAGE_UP => numbered(5, mods),
            sym::PAGE_DOWN | sym::KP_PAGE_DOWN => numbered(6, mods),
            sym::UP | sym::KP_UP => lettered(b'A', ckm, mods),
            sym::DOWN | sym::KP_DOWN => lettered(b'B', ckm, mods),
            sym::RIGHT | sym::KP_RIGHT => lettered(b'C', ckm, mods),
            sym::LEFT | sym::KP_LEFT => lettered(b'D', ckm, mods),
            sym::HOME | sym::KP_HOME => lettered(b'H', ckm, mods),
            sym::END | sym::KP_END => lettered(b'F', ckm, mods),
            sym::KP_BEGIN => lettered(b'E', ckm, mods),
            sym::KP_0..=sym::KP_9 => {
                let digit = u8::try_from(keysym.saturating_sub(sym::KP_0)).unwrap_or(0);
                keypad(b'p'.saturating_add(digit), b'0'.saturating_add(digit))
            }
            sym::KP_MULTIPLY => keypad(b'j', b'*'),
            sym::KP_ADD => keypad(b'k', b'+'),
            sym::KP_SEPARATOR => keypad(b'l', b','),
            sym::KP_SUBTRACT => keypad(b'm', b'-'),
            sym::KP_DECIMAL => keypad(b'n', b'.'),
            sym::KP_DIVIDE => keypad(b'o', b'/'),
            sym::KP_EQUAL => keypad(b'X', b'='),
            // PF1 to PF4 are SS3 keys in either keypad mode.
            sym::KP_F1..=sym::KP_F4 => {
                let n = u8::try_from(keysym.saturating_sub(sym::KP_F1)).unwrap_or(0);
                keypad_application(b'P'.saturating_add(n), mods)
            }
            sym::F1..=sym::F20 => {
                let n = keysym.saturating_sub(sym::F1);
                match function_code(n) {
                    Some(code) => numbered(code, mods),
                    None => {
                        let letter = b'P'.saturating_add(u8::try_from(n).unwrap_or(0));
                        lettered(letter, true, mods)
                    }
                }
            }
            _ => {
                let bytes = match keysym {
                    sym::BACKSPACE if self.host.backspace_delete => vec![0x7f],
                    sym::BACKSPACE => vec![0x08],
                    sym::TAB => vec![0x09],
                    sym::LINEFEED => vec![0x0a],
                    sym::CLEAR => vec![0x0b],
                    sym::SYS_REQ => vec![0x15],
                    sym::ESCAPE => vec![0x1b],
                    sym::RETURN if self.modes.newline() => b"\r\n".to_vec(),
                    sym::RETURN => vec![0x0d],
                    _ => {
                        let mut text = Vec::with_capacity(4);
                        self.push_char(unicode?, &mut text);
                        text
                    }
                };
                return Some((bytes, true));
            }
        };
        Some((bytes, false))
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
            // The top left cell, and pixel, is 1,1 (ctlseqs, the SGR and SGR-Pixels encodings).
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
            // X10's byte in decimal, a release button 3, and the cell
            // counted from 1 with no limit.
            MouseEncoding::Urxvt => {
                let cb = if released { 3 | mod_bits } else { cb };
                format!(
                    "\x1b[{};{};{}M",
                    cb.saturating_add(0x20),
                    col.saturating_add(1),
                    row.saturating_add(1)
                )
                .into_bytes()
            }
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

impl State {
    /// What Alt makes of a key's bytes where xterm lets it act: ESC before
    /// them while mode 1036 is set; else, while mode 1034 is set, a key of
    /// one byte below 0x80 with its eighth bit set, sent as the character
    /// U+0080 to U+00FF in the input's encoding, except in 7-bit input,
    /// which cannot carry it; else the bytes as they are.
    fn meta_bytes(&self, bytes: Vec<u8>) -> Vec<u8> {
        if self.modes.meta_escape() {
            let mut out = Vec::with_capacity(bytes.len().saturating_add(1));
            out.push(0x1b);
            out.extend(bytes);
            return out;
        }
        match bytes.as_slice() {
            [b] if *b < 0x80
                && self.modes.eight_bit_meta()
                && self.input_mode != InputMode::SevenBit =>
            {
                let mut out = Vec::with_capacity(2);
                self.push_char(char::from(*b | 0x80), &mut out);
                out
            }
            _ => bytes,
        }
    }
}

impl Terminal {
    /// Sends what a key press makes, as the host reports it: `keysym` the
    /// X11 keysym after Shift, `ascii` the keysym an ASCII layout gives
    /// the same key (0 for none), `unicode` the character it makes, and
    /// `mods` the modifiers held, with [`Mods::NUM`] while NumLock is on.
    /// Alt acts where xterm lets it, as modes 1034 and 1036 say, and Left
    /// and Right swap as [`arrow_keysym`](Self::arrow_keysym) says. Returns
    /// whether anything was sent.
    pub fn key(&mut self, keysym: u32, ascii: u32, mods: Mods, unicode: Option<char>) -> bool {
        let (keysym, ascii) = (self.arrow_keysym(keysym), self.arrow_keysym(ascii));
        // Ctrl reads the ASCII layout's keysym, so Ctrl+C works whatever
        // layout is active.
        let sym = if ascii == 0 { keysym } else { ascii };
        let Some((bytes, alt_acts)) = self.state.key_bytes(keysym, sym, mods, unicode) else {
            return false;
        };
        let out = if alt_acts && mods.contains(Mods::ALT) {
            self.state.meta_bytes(bytes)
        } else {
            bytes
        };
        self.send(&out);
        true
    }

    /// A wheel scroll of `lines` the host does not report to the program:
    /// while mode 1007 is set, the alternate screen is shown and the
    /// program tracks no pointer events, it is sent as that many cursor-up
    /// or cursor-down keys, at most the screen's rows, as DECCKM encodes
    /// them, and true is returned, so the host scrolls nothing. Otherwise
    /// nothing is sent and the host scrolls its view.
    pub fn alternate_scroll(&mut self, up: bool, lines: usize) -> bool {
        let s = &self.state;
        if !s.modes.alternate_scroll()
            || !s.screen.alternate()
            || s.modes.tracking() != MouseTracking::Off
        {
            return false;
        }
        let key: &[u8] = match (s.modes.cursor_keys(), up) {
            (true, true) => b"\x1bOA",
            (true, false) => b"\x1bOB",
            (false, true) => b"\x1b[A",
            (false, false) => b"\x1b[B",
        };
        let out = key.repeat(lines.min(s.screen.rows()));
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
