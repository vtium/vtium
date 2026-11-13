//! The kitty keyboard protocol's legacy section, "Legacy key event
//! encoding", which decides each key while the flags in force are not 0
//! and hold neither flag 1 nor flag 8 (ADR-1005, D6). A key the section's
//! tables give bytes keeps them on a press and a repeat. A key held with
//! modifiers the tables have no entry for is sent in the standard `CSI
//! number u` encoding, as "Any key events not described in this section
//! are sent using the standard CSI u encoding" asks. Flag 2 adds a repeat's
//! and a release's event type to a key whose legacy form is an escape code.

use super::super::State;
use super::super::input::{Key, sym};
use super::{
    ALT, CTRL, Code, Event, Kind, LOCKS, PRIVATE_USE, SHIFT, SUPER_HYPER_META, csi, functional,
    key_code, kind, made_char,
};

/// F3, whose table form is `CSI 13 ~`.
const F3: u32 = 0xffc0;

/// A key's form in "Legacy key event encoding", where that form is an
/// escape code with a modifiers field, and whether the form is the key's
/// press as well, with `mods` held, the locks left out.
///
/// - The cursor, editing and function keys keep the press xterm's
///   encoding gives them, so only their repeat and release take the form
///   here, in the forms of "Functional key definitions".
/// - F3 held with a modifier is the exception: its press is `CSI 13 ;
///   modifiers ~` too. The legacy table's `SS3 R` would become `CSI 1 ;
///   modifiers R`, and the table of functional keys removes that form
///   because a program can take it for a cursor position report.
/// - The keypad's cursor and editing keys are reported as their
///   equivalent non-keypad keys, as the section says of every keypad key.
/// - Menu is the section's `CSI 29 ~`.
/// - A key the section does not describe and the table numbers from
///   57344, such as F13 to F35, is sent in the standard `CSI number u`
///   encoding, its press included.
///
/// References: the maintainer's decision of 2026-11-13 on F3 (ADR-1005,
/// D6).
pub(super) fn legacy_code(keysym: u32, mods: u8) -> Option<(Code, bool)> {
    use Code::{Letter, Tilde, U};
    let main = match keysym {
        sym::KP_LEFT => Letter(b'D'),
        sym::KP_RIGHT => Letter(b'C'),
        sym::KP_UP => Letter(b'A'),
        sym::KP_DOWN => Letter(b'B'),
        sym::KP_HOME => Letter(b'H'),
        sym::KP_END => Letter(b'F'),
        sym::KP_BEGIN => Letter(b'E'),
        sym::KP_PAGE_UP => Tilde(5),
        sym::KP_PAGE_DOWN => Tilde(6),
        sym::KP_INSERT => Tilde(2),
        sym::KP_DELETE => Tilde(3),
        sym::MENU => return Some((Tilde(29), true)),
        F3 => return Some((Tilde(13), mods != 0)),
        // The keypad's Enter is Enter, whose legacy byte is CR.
        sym::KP_ENTER => return None,
        _ => {
            return match kind(keysym)? {
                Kind::Functional(code @ (Tilde(_) | Letter(_))) => Some((code, false)),
                Kind::Functional(U(n)) if n >= PRIVATE_USE => Some((U(n), true)),
                _ => None,
            };
        }
    };
    Some((main, false))
}

/// The keys the section's table of C0 controls lists, Enter, Escape,
/// Backspace, Tab and Space, with the numbers "Functional key definitions"
/// gives them. Tab with Shift arrives as ISO_Left_Tab.
fn c0_key(keysym: u32) -> Option<u32> {
    match keysym {
        sym::RETURN => Some(13),
        sym::ESCAPE => Some(27),
        sym::BACKSPACE => Some(127),
        sym::TAB | sym::ISO_LEFT_TAB => Some(9),
        0x20 => Some(32),
        _ => None,
    }
}

/// The number of a key held with modifiers the section's tables have no
/// entry for, which the section sends as `CSI number ; modifiers u`, or
/// `None` for a key the tables give bytes. `mods` leave out the locks.
///
/// - The table of C0 controls has columns for no modifier, Ctrl, Alt,
///   Shift, Ctrl+Shift, Alt+Shift and Ctrl+Alt. Super, Hyper, Meta, or Ctrl,
///   Alt and Shift together, have none.
/// - "Legacy text keys" encodes a text key with no modifier, Shift, Alt,
///   Ctrl, Shift+Alt and Ctrl+Alt. "Any other combination of modifiers with
///   these keys is output as the appropriate CSI u escape code": Ctrl and
///   Shift together, or Super, Hyper or Meta. vtium reads the section's
///   list of ASCII keys as the rule for every key that makes text. The
///   code is the key's unshifted key ("Key codes").
/// - A keypad key that makes text is its equivalent non-keypad key.
pub(super) fn legacy_csi_u(key: &Key, keysym: u32, unshifted: u32) -> Option<u32> {
    let mods = key.mods.kitty() & !LOCKS;
    let other = mods & SUPER_HYPER_META != 0;
    let ctrl_shift = mods & (CTRL | SHIFT) == CTRL | SHIFT;
    if let Some(number) = c0_key(keysym) {
        return (other || (ctrl_shift && mods & ALT != 0)).then_some(number);
    }
    if !(other || ctrl_shift) {
        return None;
    }
    match kind(keysym) {
        Some(Kind::KeypadText(ch, _)) => Some(u32::from(ch)),
        None => Some(key_code(keysym, unshifted, made_char(key, keysym)?)),
        _ => None,
    }
}

/// The byte Ctrl makes of an ASCII key, from the table of "Legacy ctrl
/// mapping of ASCII keys", or `None` for a key "not in the table", which
/// "must be left untouched by ctrl". The table's 0, 1 and 9 make
/// themselves. A capital letter is its key's letter, since Caps Lock is no
/// modifier of the section's algorithm. `sym` is the ASCII layout's keysym.
fn ctrl_byte(sym: u32) -> Option<u8> {
    let c = u8::try_from(sym).ok()?;
    Some(match c {
        b'a'..=b'z' | b'A'..=b'Z' => c & 0x1f,
        b' ' | b'2' | b'@' => 0x00,
        b'3' | b'[' => 0x1b,
        b'4' | b'\\' => 0x1c,
        b'5' | b']' => 0x1d,
        b'6' | b'^' | b'~' => 0x1e,
        b'7' | b'/' | b'_' => 0x1f,
        b'8' | b'?' => 0x7f,
        _ => return None,
    })
}

/// A release the program is owed under flags without flag 1 or flag 8, in
/// the legacy form: the key's code from [`legacy_code`], or `CSI code u`
/// for Escape, the space and a key that makes text. Both carry the legacy
/// encoding's modifiers, which leave out the locks. A keypad key that
/// makes text is its equivalent non-keypad key, as "Legacy key event
/// encoding" says. Enter, Tab and Backspace are never owed a release
/// here.
pub(in crate::terminal) fn legacy_release(
    key: &Key,
    keysym: u32,
    unshifted: u32,
) -> Option<Vec<u8>> {
    let mods = key.mods.kitty() & !LOCKS;
    if let Some((code, _)) = legacy_code(keysym, mods) {
        return Some(functional(code, mods, Event::Release));
    }
    let code = match (c0_key(keysym), kind(keysym)) {
        (Some(number), _) => number,
        (None, Some(Kind::KeypadText(ch, _))) => u32::from(ch),
        (None, None) => key_code(keysym, unshifted, made_char(key, keysym)?),
        _ => return None,
    };
    Some(csi(code, mods, Event::Release, b'u'))
}

impl State {
    /// A key's press or repeat under flags that are not 0 and hold neither
    /// flag 1 nor flag 8, where the legacy section decides the key. `sym`
    /// is the ASCII layout's keysym, which Ctrl reads.
    ///
    /// - A key [`legacy_code`] gives a form sends that form on its repeat,
    ///   and on its press where the form is the press.
    /// - A key held with modifiers the tables have no entry for is `CSI
    ///   number ; modifiers u` ([`legacy_csi_u`]).
    /// - Enter, Escape, Backspace, Tab and the space send the table of C0
    ///   controls' bytes. Backspace sends DEL or BS as the host's
    ///   Backspace setting says, and Ctrl+Backspace sends the other byte,
    ///   since "Backspace and ctrl+Backspace are swapped in some
    ///   terminals".
    /// - A key that makes text follows "Legacy text keys": Alt puts ESC
    ///   first; Ctrl maps the key by [`ctrl_byte`] and leaves a key not in
    ///   its table untouched; else the key sends its text, Shift's
    ///   included.
    ///
    /// Under flag 2 a repeat carries its event type wherever the key is
    /// an escape code. `None` leaves the key to xterm's encoding: a press
    /// of a key whose press is xterm's, and the keypad's keys in the
    /// combinations the tables encode. modifyOtherKeys and modes 1034,
    /// 1035 and 1036 are not consulted.
    ///
    /// References: ADR-1005, D6.
    pub(in crate::terminal) fn legacy_key_bytes(
        &self,
        key: &Key,
        keysym: u32,
        unshifted: u32,
        sym: u32,
        flags: u8,
    ) -> Option<Vec<u8>> {
        let event = Event::of(key, flags)?;
        let mods = key.mods.kitty() & !LOCKS;
        if let Some((code, own_press)) = legacy_code(keysym, mods) {
            return (event != Event::Press || own_press).then(|| functional(code, mods, event));
        }
        if let Some(number) = legacy_csi_u(key, keysym, unshifted) {
            return Some(csi(number, mods, event, b'u'));
        }
        let made = made_char(key, keysym);
        // Only the table of C0 controls and the text keys are left.
        if c0_key(keysym).is_none() && (kind(keysym).is_some() || made.is_none()) {
            return None;
        }
        let ctrl = mods & CTRL != 0;
        let mut out = Vec::with_capacity(4);
        if mods & ALT != 0 {
            out.push(0x1b);
        }
        match keysym {
            sym::RETURN if self.modes.newline() => out.extend_from_slice(b"\r\n"),
            sym::RETURN => out.push(0x0d),
            sym::ESCAPE => out.push(0x1b),
            // Ctrl turns the byte the Backspace setting chooses into the
            // other one.
            sym::BACKSPACE if self.host.backspace_delete != ctrl => out.push(0x7f),
            sym::BACKSPACE => out.push(0x08),
            sym::ISO_LEFT_TAB => out.extend_from_slice(b"\x1b[Z"),
            sym::TAB if mods & SHIFT != 0 => out.extend_from_slice(b"\x1b[Z"),
            sym::TAB => out.push(0x09),
            _ => match ctrl_byte(sym).filter(|_| ctrl) {
                Some(byte) => out.push(byte),
                None => {
                    let ch = key.text.filter(|c| !c.is_control()).or(made)?;
                    self.push_char(ch, &mut out);
                }
            },
        }
        Some(out)
    }
}
