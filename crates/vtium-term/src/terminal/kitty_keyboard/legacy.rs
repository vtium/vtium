//! The kitty keyboard protocol's legacy section, "Legacy key event
//! encoding", which decides each key while the flags in force are not 0
//! and hold neither flag 1 nor flag 8 (ADR-1005, D6). A key the section's
//! tables give bytes keeps them on a press and a repeat. A key held with
//! modifiers the tables have no entry for is sent in the standard `CSI
//! number u` encoding, as "Any key events not described in this section
//! are sent using the standard CSI u encoding" asks. Flag 2 adds a repeat's
//! and a release's event type to a key whose legacy form is an escape code.
//! Every keypad key is reported as its equivalent non-keypad key
//! ([`main_key`]).

use super::super::State;
use super::super::input::{CtrlTable, Key, ctrl_byte, sym};
use super::{
    ALT, CTRL, Code, Event, Kind, LOCKS, PRIVATE_USE, SHIFT, SUPER_HYPER_META, csi, functional,
    kind, made_char, text_code,
};

/// The non-keypad key a keypad key is, as "All keypad keys are reported as
/// their equivalent non-keypad keys" asks, or `keysym` itself for any
/// other key. The keypad's text keys are the keys of their characters,
/// its Enter, Tab and space are the main ones, PF1 to PF4 are F1 to F4,
/// and its cursor and editing keys are the main ones, which DECCKM still
/// changes. So DECKPAM and mode 66 never reach a key here. The keypad's
/// Begin stays itself, since the main keys' Begin has no form of its own:
/// [`legacy_code`] gives it `CSI E`'s forms.
pub(in crate::terminal) fn main_key(keysym: u32) -> u32 {
    match keysym {
        sym::KP_ENTER => sym::RETURN,
        sym::KP_TAB => sym::TAB,
        sym::KP_F1..=sym::KP_F4 => sym::F1.saturating_add(keysym.saturating_sub(sym::KP_F1)),
        sym::KP_HOME => sym::HOME,
        sym::KP_LEFT => sym::LEFT,
        sym::KP_UP => sym::UP,
        sym::KP_RIGHT => sym::RIGHT,
        sym::KP_DOWN => sym::DOWN,
        sym::KP_PAGE_UP => sym::PAGE_UP,
        sym::KP_PAGE_DOWN => sym::PAGE_DOWN,
        sym::KP_END => sym::END,
        sym::KP_INSERT => sym::INSERT,
        sym::KP_DELETE => sym::DELETE,
        // The digits, the operators, the decimal point, the separator, =
        // and the space: each character's keysym is the character.
        _ => match kind(keysym) {
            Some(Kind::KeypadText(ch, _)) => u32::from(ch),
            _ => keysym,
        },
    }
}

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
/// - The keypad's Begin has the forms of `CSI E`.
/// - Menu is the section's `CSI 29 ~`.
/// - A key the section does not describe and the table numbers from
///   57344, such as F13 to F35, is sent in the standard `CSI number u`
///   encoding, its press included.
///
/// `keysym` is a key [`main_key`] has already read.
///
/// References: the maintainer's decision of 2026-11-13 on F3 (ADR-1005,
/// D6).
pub(super) fn legacy_code(keysym: u32, mods: u8) -> Option<(Code, bool)> {
    use Code::{Letter, Tilde, U};
    match keysym {
        sym::KP_BEGIN => Some((Letter(b'E'), false)),
        sym::MENU => Some((Tilde(29), true)),
        sym::F3 => Some((Tilde(13), mods != 0)),
        _ => match kind(keysym)? {
            Kind::Functional(code @ (Tilde(_) | Letter(_))) => Some((code, false)),
            Kind::Functional(U(n)) if n >= PRIVATE_USE => Some((U(n), true)),
            _ => None,
        },
    }
}

/// Escape, the one functional key whose number is below 57344, which
/// [`legacy_code`] leaves out: `Some(27)`.
fn escape_number(keysym: u32) -> Option<u32> {
    match kind(keysym)? {
        Kind::Functional(Code::U(n)) if n < PRIVATE_USE => Some(n),
        _ => None,
    }
}

/// The keys the section's table of C0 controls lists, Enter, Escape,
/// Backspace, Tab and Space, with the numbers their `CSI u` forms use:
/// the numbers "Functional key definitions" gives, and the space's
/// character. Tab with Shift arrives as ISO_Left_Tab.
fn c0_key(keysym: u32) -> Option<u32> {
    match kind(keysym) {
        Some(Kind::Legacy(number)) => Some(number),
        _ if keysym == 0x20 => Some(0x20),
        _ => escape_number(keysym),
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
///
/// `keysym` is a key [`main_key`] has already read.
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
        None => Some(text_code(unshifted, made_char(key, keysym)?)),
        _ => None,
    }
}

/// A release the program is owed under flags without flag 1 or flag 8, in
/// the legacy form: the key's code from [`legacy_code`], or `CSI code u`
/// for Escape and for a key that makes text, the space among them. Both
/// carry the legacy encoding's modifiers, which leave out the locks. A
/// keypad key is its equivalent non-keypad key ([`main_key`]). Enter, Tab
/// and Backspace never reach here: [`owes_release`](super::owes_release)
/// owes them no release without flag 8.
pub(in crate::terminal) fn legacy_release(
    key: &Key,
    keysym: u32,
    unshifted: u32,
) -> Option<Vec<u8>> {
    let keysym = main_key(keysym);
    let mods = key.mods.kitty() & !LOCKS;
    if let Some((code, _)) = legacy_code(keysym, mods) {
        return Some(functional(code, mods, Event::Release));
    }
    let code = match kind(keysym) {
        None => text_code(unshifted, made_char(key, keysym)?),
        _ => escape_number(keysym)?,
    };
    Some(csi(code, mods, Event::Release, b'u'))
}

impl State {
    /// A key's press or repeat under flags that are not 0 and hold neither
    /// flag 1 nor flag 8, where the legacy section decides the key.
    /// `keysym` is a key [`main_key`] has already read, and `sym` is the
    /// ASCII layout's keysym, which Ctrl reads, or for a keypad key the
    /// keysym of its equivalent key.
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
    ///   first; Ctrl maps the key by the kitty table of [`ctrl_byte`] and
    ///   leaves a key not in that table untouched; else the key sends its
    ///   text, Shift's included.
    ///
    /// Under flag 2 a repeat carries its event type wherever the key is
    /// an escape code. `None` leaves the press of a cursor, editing or
    /// function key to xterm's encoding, which DECCKM changes as the
    /// section says. modifyOtherKeys, DECKPAM and modes 66, 1034, 1035 and
    /// 1036 are not consulted.
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
            _ => match ctrl_byte(sym, CtrlTable::Kitty).filter(|_| ctrl) {
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
