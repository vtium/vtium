//! The kitty keyboard protocol (ADR-1005): the flags a program pushes,
//! pops, sets and queries, one stack for each screen, and the encoding a
//! key gets while the flags in force are not 0. The behaviour follows the
//! specification, "Comprehensive keyboard handling in terminals"
//! (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>), and vtium-term's
//! README records where vtium settled what the specification leaves open.

use vtium_parse::Params;

use super::State;
use super::input::{Key, KeyEvent, key_char, sym};
use super::reply::Query;
use crate::bounds::KEYBOARD_FLAGS_MAX;

/// The flags vtium implements: 1, disambiguate escape codes. A push or a
/// set drops every other bit as it arrives, so the query reports only
/// these (ADR-1005, D3 and D5).
const IMPLEMENTED: u32 = 1;

/// The flags saved under the set in force: one level fewer than the
/// bound, which counts the set in force.
const SAVED_MAX: usize = KEYBOARD_FLAGS_MAX - 1;

/// One screen's stack of flags: the set in force and the sets saved under
/// it, the newest last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stack {
    flags: u8,
    saved: [u8; SAVED_MAX],
    len: usize,
}

impl Stack {
    /// `CSI > flags u`: saves the set in force and puts `flags` in force.
    /// When seven sets are saved already, the oldest is dropped, as the
    /// specification says a push onto a full stack evicts the oldest
    /// entry.
    fn push(&mut self, flags: u8) {
        if self.len == SAVED_MAX {
            self.saved.copy_within(1.., 0);
            self.len = SAVED_MAX - 1;
        }
        if let Some(slot) = self.saved.get_mut(self.len) {
            *slot = self.flags;
            self.len = self.len.saturating_add(1);
        }
        self.flags = flags;
    }

    /// `CSI < n u`: pops `n` sets. The newest saved set that remains comes
    /// back into force; a pop of more sets than are saved empties the
    /// stack and puts 0 in force, as the specification says a pop that
    /// empties the stack resets all flags.
    fn pop(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        let n = usize::try_from(n).unwrap_or(usize::MAX);
        if n > self.len {
            *self = Self::default();
            return;
        }
        self.len = self.len.saturating_sub(n);
        self.flags = self.saved.get(self.len).copied().unwrap_or(0);
    }
}

/// The flags of the main screen and of the alternate screen (ADR-1005,
/// D3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Keyboard {
    stacks: [Stack; 2],
}

impl Keyboard {
    /// The flags in force on the main screen, or on the alternate screen
    /// when `alternate`.
    pub(super) fn flags(&self, alternate: bool) -> u8 {
        self.stacks
            .get(usize::from(alternate))
            .map_or(0, |s| s.flags)
    }

    fn stack(&mut self, alternate: bool) -> Option<&mut Stack> {
        self.stacks.get_mut(usize::from(alternate))
    }
}

/// The flags a push or a set asks for, with the bits vtium does not
/// implement dropped.
fn implemented(flags: Option<u32>) -> u8 {
    u8::try_from(flags.unwrap_or(0) & IMPLEMENTED).unwrap_or(0)
}

impl State {
    /// `CSI > flags u`, a push onto the shown screen's stack.
    pub(super) fn push_keyboard_flags(&mut self, params: &Params) {
        let flags = implemented(params.get(0));
        if let Some(s) = self.keyboard.stack(self.screen.alternate()) {
            s.push(flags);
        }
    }

    /// `CSI < n u`, a pop of `n` sets from the shown screen's stack, 1
    /// when the number is left out.
    pub(super) fn pop_keyboard_flags(&mut self, params: &Params) {
        let n = params.get(0).unwrap_or(1);
        if let Some(s) = self.keyboard.stack(self.screen.alternate()) {
            s.pop(n);
        }
    }

    /// `CSI = flags ; mode u`: changes the flags in force on the shown
    /// screen and saves nothing. Mode 1, or none, sets exactly `flags`;
    /// mode 2 sets its bits; mode 3 clears its bits; any other mode
    /// changes nothing.
    pub(super) fn set_keyboard_flags(&mut self, params: &Params) {
        let flags = implemented(params.get(0));
        let Some(s) = self.keyboard.stack(self.screen.alternate()) else {
            return;
        };
        match params.get(1).unwrap_or(1) {
            1 => s.flags = flags,
            2 => s.flags |= flags,
            3 => s.flags &= !flags,
            _ => {}
        }
    }

    /// RIS and DECSTR: both stacks empty, and both screens' flags 0
    /// (ADR-1005, D4).
    pub(super) fn reset_keyboard_flags(&mut self) {
        self.keyboard = Keyboard::default();
    }

    /// `CSI ? u`, answered `CSI ? flags u` with the flags in force on the
    /// shown screen, a number vtium writes from its own state.
    pub(super) fn reply_keyboard_flags(&mut self) {
        let flags = self.keyboard.flags(self.screen.alternate());
        let reply = format!("\x1b[?{flags}u");
        self.reply(Query::KeyboardFlags, reply.as_bytes());
    }
}

/// How the specification writes a key that does not send text: the second
/// column of its "Functional key definitions" table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Code {
    /// `CSI number ; modifiers u`.
    U(u32),
    /// `CSI number ; modifiers ~`.
    Tilde(u32),
    /// `CSI 1 ; modifiers letter`, and `CSI letter` with no modifier.
    Letter(u8),
}

/// What the specification makes of a key, by its keysym.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Enter, Tab or Backspace, which keep their legacy bytes with no
    /// modifier held, and are otherwise `CSI number u`.
    Legacy(u32),
    /// A key that never sends text.
    Functional(Code),
    /// A keypad key that types `char` with its own number for the escape
    /// code.
    KeypadText(char, u32),
    /// A modifier or lock key, which only flag 8 reports.
    Modifier,
}

/// The kitty number of F13 to F35, and of the keypad's keys from KP_0.
const F13: u32 = 57376;
const KP_0: u32 = 57399;

/// The specification's reading of a keysym, or `None` for a key that is
/// either text or a key the specification gives no number.
fn kind(keysym: u32) -> Option<Kind> {
    use Code::{Letter, Tilde, U};
    let kp_text = |c: char, n: u32| Some(Kind::KeypadText(c, n));
    let f = |code| Some(Kind::Functional(code));
    match keysym {
        sym::RETURN => Some(Kind::Legacy(13)),
        sym::TAB | sym::ISO_LEFT_TAB | sym::KP_TAB => Some(Kind::Legacy(9)),
        sym::BACKSPACE => Some(Kind::Legacy(127)),
        sym::ESCAPE => f(U(27)),
        sym::INSERT => f(Tilde(2)),
        sym::DELETE => f(Tilde(3)),
        sym::PAGE_UP => f(Tilde(5)),
        sym::PAGE_DOWN => f(Tilde(6)),
        sym::LEFT => f(Letter(b'D')),
        sym::RIGHT => f(Letter(b'C')),
        sym::UP => f(Letter(b'A')),
        sym::DOWN => f(Letter(b'B')),
        sym::HOME => f(Letter(b'H')),
        sym::END => f(Letter(b'F')),
        sym::SCROLL_LOCK => f(U(57359)),
        sym::PRINT => f(U(57361)),
        sym::PAUSE => f(U(57362)),
        sym::MENU => f(U(57363)),
        // F1, F2 and F4 are letters; F3's `CSI R` would read as a cursor
        // position report, so it is `CSI 13 ~`. PF1 to PF4 are F1 to F4.
        sym::F1 | sym::KP_F1 => f(Letter(b'P')),
        0xffbf | 0xff92 => f(Letter(b'Q')),
        0xffc0 | 0xff93 => f(Tilde(13)),
        0xffc1 | sym::KP_F4 => f(Letter(b'S')),
        0xffc2..=0xffc9 => {
            const CODES: [u32; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            let i = usize::try_from(keysym.saturating_sub(0xffc2)).ok()?;
            f(Tilde(*CODES.get(i)?))
        }
        0xffca..=sym::F35 => f(U(F13.saturating_add(keysym.saturating_sub(0xffca)))),
        sym::KP_0..=sym::KP_9 => {
            let n = keysym.saturating_sub(sym::KP_0);
            kp_text(char::from_digit(n, 10)?, KP_0.saturating_add(n))
        }
        sym::KP_DECIMAL => kp_text('.', 57409),
        sym::KP_DIVIDE => kp_text('/', 57410),
        sym::KP_MULTIPLY => kp_text('*', 57411),
        sym::KP_SUBTRACT => kp_text('-', 57412),
        sym::KP_ADD => kp_text('+', 57413),
        sym::KP_EQUAL => kp_text('=', 57415),
        sym::KP_SEPARATOR => kp_text(',', 57416),
        sym::KP_SPACE => kp_text(' ', 32),
        sym::KP_ENTER => f(U(57414)),
        sym::KP_LEFT => f(U(57417)),
        sym::KP_RIGHT => f(U(57418)),
        sym::KP_UP => f(U(57419)),
        sym::KP_DOWN => f(U(57420)),
        sym::KP_PAGE_UP => f(U(57421)),
        sym::KP_PAGE_DOWN => f(U(57422)),
        sym::KP_HOME => f(U(57423)),
        sym::KP_END => f(U(57424)),
        sym::KP_INSERT => f(U(57425)),
        sym::KP_DELETE => f(U(57426)),
        sym::KP_BEGIN => f(U(57427)),
        // The media keys xkbcommon names (`xkbcommon-keysyms.h`).
        0x1008_ff14 => f(U(57428)),
        0x1008_ff31 => f(U(57429)),
        0x1008_ff15 => f(U(57432)),
        0x1008_ff97 => f(U(57433)),
        0x1008_ff3e => f(U(57434)),
        0x1008_ff17 => f(U(57435)),
        0x1008_ff16 => f(U(57436)),
        0x1008_ff1c => f(U(57437)),
        0x1008_ff11 => f(U(57438)),
        0x1008_ff13 => f(U(57439)),
        0x1008_ff12 => f(U(57440)),
        sym::NUM_LOCK
        | sym::SHIFT_L..=sym::HYPER_R
        | sym::ISO_LEVEL3_SHIFT
        | sym::ISO_LEVEL5_SHIFT => Some(Kind::Modifier),
        _ => None,
    }
}

/// The modifiers that make a text key an escape code: all but Shift and
/// the locks, whose bits the specification leaves out of a key that sends
/// text.
const LOCKS: u8 = 64 | 128;
const SHIFT: u8 = 1;

/// `CSI number ; modifiers final`, the modifiers left out when there are
/// none.
fn csi(number: u32, mods: u8, final_byte: u8) -> Vec<u8> {
    let f = char::from(final_byte);
    if mods == 0 {
        format!("\x1b[{number}{f}").into_bytes()
    } else {
        format!("\x1b[{number};{}{f}", u16::from(mods) + 1).into_bytes()
    }
}

/// The character a keysym stands for, where the keysym says it alone:
/// Latin-1, and the Unicode keysyms from 0x1000100.
fn keysym_char(keysym: u32) -> Option<char> {
    match keysym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(keysym),
        0x0100_0100..=0x0110_ffff => char::from_u32(keysym.saturating_sub(0x0100_0000)),
        _ => None,
    }
}

/// The key's code for `CSI code u`, the unshifted key: the host's
/// unshifted key where it reports one, else the character the key made,
/// a letter in its lower case, so Ctrl+Shift+A is 97 (the specification's
/// "Key codes").
fn text_code(unshifted: u32, ch: char) -> u32 {
    if let Some(c) = keysym_char(unshifted) {
        return u32::from(c);
    }
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => u32::from(l),
        _ => u32::from(ch),
    }
}

impl State {
    /// A key's bytes while the flags in force are not 0, for the flags
    /// vtium implements, which are 1, disambiguate escape codes; `None` for
    /// a key that sends nothing. `keysym` and `unshifted` are the key's,
    /// with Left and Right swapped as the host's direction asks.
    ///
    /// - A release sends nothing, since flag 2 is not implemented, and a
    ///   repeat is sent as a press.
    /// - Enter, Tab and Backspace send their legacy bytes with no modifier
    ///   but the locks held, so a user can still type `reset`, and are
    ///   `CSI 13 u`, `CSI 9 u` and `CSI 127 u` with one.
    /// - Every other key that sends no text is written as the
    ///   specification's table gives it, with every modifier held, the
    ///   locks included; the keypad's keys have numbers of their own, and
    ///   modifier keys send nothing, since flag 8 is not implemented.
    /// - A key that sends text sends it, in the input's encoding, while
    ///   only Shift and the locks are held; with any other modifier it is
    ///   `CSI code ; modifiers u`.
    ///
    /// DECCKM, the keypad modes, modifyOtherKeys and modes 1034, 1035
    /// and 1036 are not consulted (ADR-1005, D6).
    pub(super) fn kitty_key_bytes(
        &self,
        key: &Key,
        keysym: u32,
        unshifted: u32,
    ) -> Option<Vec<u8>> {
        if key.event == KeyEvent::Release {
            return None;
        }
        let mods = key.mods.kitty();
        let pressed = mods & !LOCKS;
        let text = |ch: char| {
            let mut out = Vec::with_capacity(4);
            self.push_char(ch, &mut out);
            out
        };
        match kind(keysym) {
            Some(Kind::Modifier) => None,
            Some(Kind::Legacy(code)) if pressed == 0 => Some(match code {
                13 if self.modes.newline() => b"\r\n".to_vec(),
                13 => vec![0x0d],
                9 => vec![0x09],
                _ if self.host.backspace_delete => vec![0x7f],
                _ => vec![0x08],
            }),
            Some(Kind::Legacy(code)) => Some(csi(code, mods, b'u')),
            Some(Kind::Functional(Code::U(n))) => Some(csi(n, mods, b'u')),
            Some(Kind::Functional(Code::Tilde(n))) => Some(csi(n, mods, b'~')),
            Some(Kind::Functional(Code::Letter(l))) if mods == 0 => Some(vec![0x1b, b'[', l]),
            Some(Kind::Functional(Code::Letter(l))) => Some(csi(1, mods, l)),
            Some(Kind::KeypadText(ch, n)) => {
                if pressed & !SHIFT == 0 {
                    let ch = key.text.filter(|c| !c.is_control()).unwrap_or(ch);
                    Some(text(ch))
                } else {
                    Some(csi(n, mods, b'u'))
                }
            }
            None => {
                let ch = key_char(keysym, key.text)
                    .and_then(char::from_u32)
                    .or_else(|| keysym_char(keysym))?;
                if pressed & !SHIFT == 0 {
                    let ch = key.text.filter(|c| !c.is_control()).unwrap_or(ch);
                    Some(text(ch))
                } else {
                    Some(csi(text_code(unshifted, ch), mods, b'u'))
                }
            }
        }
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn a_push_onto_a_full_stack_drops_the_oldest_set() {
        let mut s = Stack::default();
        for n in 1..=12 {
            s.push(n);
        }
        let mut seen = vec![s.flags];
        for _ in 0..8 {
            s.pop(1);
            seen.push(s.flags);
        }
        assert_eq!(seen, [12, 11, 10, 9, 8, 7, 6, 5, 0]);
    }

    #[test]
    fn a_pop_past_the_bottom_puts_0_in_force() {
        let mut s = Stack {
            flags: 3,
            ..Stack::default()
        };
        s.push(1);
        s.pop(1);
        assert_eq!(s.flags, 3, "the set saved under the push");
        s.pop(2);
        assert_eq!(s, Stack::default());
        s.push(1);
        s.pop(0);
        assert_eq!(s.flags, 1, "a pop of 0 pops nothing");
        s.pop(u32::MAX);
        assert_eq!(s, Stack::default());
    }

    #[test]
    fn every_numbered_key_has_one_number() {
        let mut seen = std::collections::BTreeMap::new();
        for keysym in (0xfe00..=0xffff).chain(0x1008_ff00..=0x1008_ffff) {
            let code = match kind(keysym) {
                Some(Kind::Functional(Code::U(n)) | Kind::KeypadText(_, n)) => n,
                _ => continue,
            };
            if let Some(other) = seen.insert(code, keysym) {
                let tab = [sym::KP_TAB, sym::TAB, sym::ISO_LEFT_TAB];
                if !(tab.contains(&other) && tab.contains(&keysym)) {
                    panic!("{other:#x} and {keysym:#x} share {code}");
                }
            }
        }
    }
}
