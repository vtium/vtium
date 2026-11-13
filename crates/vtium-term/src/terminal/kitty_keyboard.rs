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
use crate::bounds::{KEYBOARD_FLAGS_MAX, KEYS_HELD_MAX};

/// Flag 1, disambiguate escape codes.
const DISAMBIGUATE: u8 = 1;
/// Flag 2, report event types.
const EVENT_TYPES: u8 = 2;
/// Flag 4, report alternate keys.
const ALTERNATES: u8 = 4;
/// Flag 8, report all keys as escape codes.
const ALL_KEYS: u8 = 8;
/// Flag 16, report associated text.
const TEXT: u8 = 16;

/// The five flags the specification documents, 1 to 16. A push or a set
/// drops every other bit as it arrives, so the query reports only these
/// (ADR-1005, D3 and D5).
const IMPLEMENTED: u32 = (DISAMBIGUATE | EVENT_TYPES | ALTERNATES | ALL_KEYS | TEXT) as u32;

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

/// A key held whose release the program is owed: the keysym its press
/// reported, and the flags in force at the press, which choose the
/// release's form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Held {
    keysym: u32,
    flags: u8,
}

/// The flags of the main screen and of the alternate screen (ADR-1005,
/// D3), and the keys held whose release the program is owed (D7), newest
/// last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Keyboard {
    stacks: [Stack; 2],
    held: [Held; KEYS_HELD_MAX],
    held_len: usize,
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

    /// The press of the key that reports `keysym`: its release is owed,
    /// with the flags `owed` gives, or not owed at all. A press with the
    /// record full drops the oldest key held.
    pub(super) fn pressed(&mut self, keysym: u32, owed: Option<u8>) {
        self.take(keysym);
        let Some(flags) = owed else {
            return;
        };
        if self.held_len == KEYS_HELD_MAX {
            self.held.copy_within(1.., 0);
            self.held_len = KEYS_HELD_MAX - 1;
        }
        if let Some(slot) = self.held.get_mut(self.held_len) {
            *slot = Held { keysym, flags };
            self.held_len = self.held_len.saturating_add(1);
        }
    }

    /// The release of the key that reports `keysym`: the flags in force at
    /// its press when its release is owed, and the key is no longer held.
    pub(super) fn take(&mut self, keysym: u32) -> Option<u8> {
        let held = self.held.get(..self.held_len)?;
        let i = held.iter().position(|h| h.keysym == keysym)?;
        let flags = held.get(i)?.flags;
        self.held.copy_within(i.saturating_add(1)..self.held_len, i);
        self.held_len = self.held_len.saturating_sub(1);
        Some(flags)
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
    /// A modifier or lock key, which only flag 8 reports, with its number
    /// from the table, or `None` for Shift_Lock, which the table does not
    /// name.
    Modifier(Option<u32>),
}

/// The kitty number of F13 to F35, and of the keypad's keys from KP_0.
const F13: u32 = 57376;
const KP_0: u32 = 57399;
/// The first number of the Unicode Private Use Area, where the table
/// numbers every key that has no legacy encoding.
const PRIVATE_USE: u32 = 57344;

/// The numbers of Shift_L to Hyper_R, in keysym order: Shift_L, Shift_R,
/// Control_L, Control_R, Caps_Lock, Shift_Lock, Meta_L, Meta_R, Alt_L,
/// Alt_R, Super_L, Super_R, Hyper_L and Hyper_R. The table names no
/// Shift_Lock.
const MODIFIER_KEYS: [Option<u32>; 14] = [
    Some(57441),
    Some(57447),
    Some(57442),
    Some(57448),
    Some(57358),
    None,
    Some(57446),
    Some(57452),
    Some(57443),
    Some(57449),
    Some(57444),
    Some(57450),
    Some(57445),
    Some(57451),
];

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
        sym::NUM_LOCK => Some(Kind::Modifier(Some(57360))),
        sym::ISO_LEVEL3_SHIFT => Some(Kind::Modifier(Some(57453))),
        sym::ISO_LEVEL5_SHIFT => Some(Kind::Modifier(Some(57454))),
        sym::SHIFT_L..=sym::HYPER_R => {
            let i = usize::try_from(keysym.saturating_sub(sym::SHIFT_L)).ok()?;
            Some(Kind::Modifier(*MODIFIER_KEYS.get(i)?))
        }
        _ => None,
    }
}

/// The modifiers that make a text key an escape code: all but Shift and
/// the locks, whose bits the specification leaves out of a key that sends
/// text.
const LOCKS: u8 = 64 | 128;
const SHIFT: u8 = 1;

/// The event type a key's escape code carries, as "Event types" numbers
/// them: a press 1, a repeat 2 and a release 3. A press is the default,
/// which the code leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Press,
    Repeat,
    Release,
}

impl Event {
    /// What a key's event is sent as under `flags`: under flag 2 each event
    /// is its own; without it a repeat is a press, and a release `None`,
    /// since nothing is sent for it.
    fn of(key: &Key, flags: u8) -> Option<Event> {
        let types = flags & EVENT_TYPES != 0;
        match key.event {
            KeyEvent::Press => Some(Event::Press),
            KeyEvent::Repeat if types => Some(Event::Repeat),
            KeyEvent::Repeat => Some(Event::Press),
            KeyEvent::Release if types => Some(Event::Release),
            KeyEvent::Release => None,
        }
    }

    fn number(self) -> u8 {
        match self {
            Event::Press => 1,
            Event::Repeat => 2,
            Event::Release => 3,
        }
    }
}

/// One escape code in the specification's central form, `CSI key ;
/// modifiers ; text final` ("An overview"). The key field is the key's
/// number, then the shifted key and the base layout key after colons, the
/// shifted key's sub-field left empty when only the base layout key is
/// sent ("Key codes"). The modifiers field is 1 plus the modifier bits,
/// then the event type after a colon for a repeat or a release ("Event
/// types"). The text field is the text's code point ("Text as code
/// points"). A field that holds nothing is left out, and the modifiers
/// field is left empty where only the text follows it.
#[derive(Clone, Copy, Debug)]
struct Escape {
    number: u32,
    shifted: Option<u32>,
    base: Option<u32>,
    mods: u8,
    event: Event,
    text: Option<char>,
    final_byte: u8,
}

impl Escape {
    fn new(number: u32, mods: u8, event: Event, final_byte: u8) -> Self {
        Self {
            number,
            shifted: None,
            base: None,
            mods,
            event,
            text: None,
            final_byte,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut out = format!("\x1b[{}", self.number);
        if self.shifted.is_some() || self.base.is_some() {
            out.push(':');
            if let Some(shifted) = self.shifted {
                let _ = write!(out, "{shifted}");
            }
            if let Some(base) = self.base {
                let _ = write!(out, ":{base}");
            }
        }
        let modifiers = self.mods != 0 || self.event != Event::Press;
        if modifiers || self.text.is_some() {
            out.push(';');
        }
        if modifiers {
            let _ = write!(out, "{}", u16::from(self.mods) + 1);
            if self.event != Event::Press {
                let _ = write!(out, ":{}", self.event.number());
            }
        }
        if let Some(text) = self.text {
            let _ = write!(out, ";{}", u32::from(text));
        }
        out.push(char::from(self.final_byte));
        out.into_bytes()
    }
}

/// `CSI number ; modifiers final`, the modifiers left out when there are
/// none and the event is a press, and `CSI number ; modifiers : event
/// final` for a repeat or a release, the modifiers 1 when there are none
/// ("Event types").
fn csi(number: u32, mods: u8, event: Event, final_byte: u8) -> Vec<u8> {
    Escape::new(number, mods, event, final_byte).bytes()
}

/// A functional key's code: `CSI number ; modifiers u` or `~`, or a
/// letter, written `CSI letter` only for a press with no modifier.
fn functional(code: Code, mods: u8, event: Event) -> Vec<u8> {
    match code {
        Code::U(n) => csi(n, mods, event, b'u'),
        Code::Tilde(n) => csi(n, mods, event, b'~'),
        Code::Letter(l) if mods == 0 && event == Event::Press => vec![0x1b, b'[', l],
        Code::Letter(l) => csi(1, mods, event, l),
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

/// The character a key that makes text made, after Shift, or `None` for a
/// key that makes none.
fn made_char(key: &Key, keysym: u32) -> Option<char> {
    key_char(keysym, key.text)
        .and_then(char::from_u32)
        .or_else(|| keysym_char(keysym))
}

/// Whether a press under `flags` owes the program its release, decided as
/// the press goes out (ADR-1005, D7). Only flag 2 reports releases.
///
/// - With flag 1 or flag 8, a key whose press is an escape code gets its
///   release: every functional key, and a key held with a modifier other
///   than Shift and the locks. A key that sends its text gets none, and
///   neither do Enter, Tab and Backspace, unless flag 8 is in force, as
///   "Report event types" says. Under flag 8 every key with a number gets
///   one.
/// - Without them, a key gets its release where "Legacy key event
///   encoding" gives it an escape code with a modifiers field, and a key
///   that makes text gets one too, as the maintainer decided on
///   2026-11-13.
pub(super) fn owes_release(key: &Key, keysym: u32, flags: u8) -> bool {
    if flags & EVENT_TYPES == 0 {
        return false;
    }
    let all = flags & ALL_KEYS != 0;
    let plain = key.mods.kitty() & !LOCKS & !SHIFT == 0;
    let makes_text = || made_char(key, keysym).is_some();
    if disambiguates(flags) {
        return match kind(keysym) {
            Some(Kind::Modifier(number)) => all && number.is_some(),
            Some(Kind::Legacy(_)) => all,
            Some(Kind::Functional(_)) => true,
            Some(Kind::KeypadText(..)) => all || !plain,
            None => makes_text() && (all || !plain),
        };
    }
    if legacy_code(keysym).is_some() {
        return true;
    }
    match kind(keysym) {
        Some(Kind::KeypadText(..)) => plain,
        None => plain && makes_text(),
        _ => false,
    }
}

/// A release the program is owed under flags without flag 1 or flag 8, in
/// the legacy form: the key's code from [`legacy_code`], or for a key that
/// makes text `CSI code u`, both with the legacy encoding's modifiers,
/// which leave out the locks. A keypad key that makes text is its
/// equivalent non-keypad key, as "Legacy key event encoding" says.
pub(super) fn legacy_release(key: &Key, keysym: u32, unshifted: u32) -> Option<Vec<u8>> {
    let mods = key.mods.kitty() & !LOCKS;
    if let Some((code, _)) = legacy_code(keysym) {
        return Some(functional(code, mods, Event::Release));
    }
    let code = match kind(keysym) {
        Some(Kind::KeypadText(ch, _)) => u32::from(ch),
        None => text_code(unshifted, made_char(key, keysym)?),
        _ => return None,
    };
    Some(csi(code, mods, Event::Release, b'u'))
}

impl State {
    /// A key's bytes while the flags in force, `flags`, include flag 1,
    /// disambiguate escape codes, or flag 8, report all keys as escape
    /// codes, which implies flag 1; `None` for a key that sends nothing.
    /// `keysym` and `unshifted` are the key's, with Left and Right swapped
    /// as the host's direction asks.
    ///
    /// - Under flag 2, report event types, a repeat and a release are sent
    ///   with their event types wherever the key is sent as an escape code
    ///   ("Event types"). Without it, a repeat is sent as a press and a
    ///   release sends nothing.
    /// - Without flag 8, Enter, Tab and Backspace send their legacy bytes
    ///   with no modifier but the locks held, so a user can still type
    ///   `reset`, and send no release; with a modifier they are `CSI 13
    ///   u`, `CSI 9 u` and `CSI 127 u`. Under flag 8 they are always those
    ///   codes, with their releases.
    /// - Every other key that sends no text is written as the
    ///   specification's table gives it, with every modifier held, the
    ///   locks included; the keypad's keys have numbers of their own.
    ///   Modifier keys send their own numbers under flag 8, and nothing
    ///   without it.
    /// - Without flag 8, a key that sends text sends it, in the input's
    ///   encoding, on a press and a repeat while only Shift and the locks
    ///   are held; with any other modifier it is `CSI code ; modifiers u`.
    ///   A release reaches this function only where its press owed it
    ///   ([`owes_release`]), so a release is always an escape code. Under flag 8 it is always `CSI code ;
    ///   modifiers u`, the locks among the modifiers.
    /// - Under flag 4, report alternate keys, a key that makes text and is
    ///   sent as `CSI code u` carries the shifted key while Shift is held,
    ///   the host's where it reports one and else the character the key
    ///   made, and the base layout key the host reports, each only where
    ///   it differs from the code ("Key codes").
    /// - Under flags 8 and 16, report associated text, a key that makes
    ///   text carries it in a third field, on a press and a repeat. A key
    ///   makes text while only Shift and the locks are held, and the text
    ///   never holds a control character ("Text as code points").
    ///
    /// DECCKM, the keypad modes, modifyOtherKeys and modes 1034, 1035
    /// and 1036 are not consulted (ADR-1005, D6).
    pub(super) fn kitty_key_bytes(
        &self,
        key: &Key,
        keysym: u32,
        unshifted: u32,
        flags: u8,
    ) -> Option<Vec<u8>> {
        let event = Event::of(key, flags)?;
        let all = flags & ALL_KEYS != 0;
        let mods = key.mods.kitty();
        let pressed = mods & !LOCKS;
        // Only Shift and the locks are held, so the key makes its text.
        let plain = pressed & !SHIFT == 0;
        let text = |ch: char| {
            if event == Event::Release {
                return None;
            }
            let mut out = Vec::with_capacity(4);
            self.push_char(ch, &mut out);
            Some(out)
        };
        // The text flag 16 puts inside the code, under flag 8.
        let associated = |ch: char| {
            (all && flags & TEXT != 0 && plain && event != Event::Release && !ch.is_control())
                .then_some(ch)
        };
        match kind(keysym) {
            Some(Kind::Modifier(number)) if all => Some(csi(number?, mods, event, b'u')),
            Some(Kind::Modifier(_)) => None,
            Some(Kind::Legacy(code)) if all => Some(csi(code, mods, event, b'u')),
            Some(Kind::Legacy(_)) if event == Event::Release => None,
            Some(Kind::Legacy(code)) if pressed == 0 => Some(match code {
                13 if self.modes.newline() => b"\r\n".to_vec(),
                13 => vec![0x0d],
                9 => vec![0x09],
                _ if self.host.backspace_delete => vec![0x7f],
                _ => vec![0x08],
            }),
            Some(Kind::Legacy(code)) => Some(csi(code, mods, event, b'u')),
            Some(Kind::Functional(code)) => Some(functional(code, mods, event)),
            Some(Kind::KeypadText(ch, n)) => {
                let ch = key.text.filter(|c| !c.is_control()).unwrap_or(ch);
                if !all && plain && event != Event::Release {
                    return text(ch);
                }
                let escape = Escape {
                    text: associated(ch),
                    ..Escape::new(n, mods, event, b'u')
                };
                Some(escape.bytes())
            }
            None => {
                let made = made_char(key, keysym)?;
                let ch = key.text.filter(|c| !c.is_control()).unwrap_or(made);
                if !all && plain && event != Event::Release {
                    return text(ch);
                }
                let code = text_code(unshifted, made);
                let mut escape = Escape::new(code, mods, event, b'u');
                if flags & ALTERNATES != 0 {
                    let shifted = keysym_char(key.shifted).unwrap_or(made);
                    escape.shifted =
                        Some(u32::from(shifted)).filter(|&s| mods & SHIFT != 0 && s != code);
                    escape.base = key.base.map(u32::from).filter(|&b| b != code);
                }
                escape.text = associated(ch);
                Some(escape.bytes())
            }
        }
    }
}

/// Whether the flags in force, not 0, include flag 1, or flag 8, which
/// the specification says implies it ("Report all keys as escape codes").
/// Without either, a key keeps the specification's legacy encoding.
pub(super) fn disambiguates(flags: u8) -> bool {
    flags & (DISAMBIGUATE | ALL_KEYS) != 0
}

/// A key's form in "Legacy key event encoding", where that form is an
/// escape code with a modifiers field, and whether the form is the key's
/// press as well. The cursor, editing and function keys keep the press
/// xterm's encoding gives them, so only their repeat and release take the
/// form here, in the forms of "Functional key definitions". The keypad's
/// cursor and editing keys are reported as their equivalent non-keypad
/// keys, as the section says of every keypad key. Menu is the section's
/// `CSI 29 ~`. A key the section does not describe and the table numbers
/// from 57344, such as F13 to F35, is sent in the standard `CSI number u`
/// encoding, its press included.
fn legacy_code(keysym: u32) -> Option<(Code, bool)> {
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

/// A press's or a repeat's escape code under flags that are not 0 and
/// include neither flag 1 nor flag 8, where the legacy encoding decides
/// the key: the forms [`legacy_code`] gives, with the legacy encoding's
/// modifiers, which leave out the locks, and under flag 2 a repeat's event
/// type. `None` leaves the key to xterm's encoding: a press of a key whose
/// press is xterm's, and every key the legacy section sends as text or as
/// legacy bytes, whose repeat is a press. [`legacy_release`] writes a
/// release.
pub(super) fn legacy_event(key: &Key, keysym: u32, flags: u8) -> Option<Vec<u8>> {
    let event = Event::of(key, flags)?;
    let (code, own_press) = legacy_code(keysym)?;
    if event == Event::Press && !own_press {
        return None;
    }
    Some(functional(code, key.mods.kitty() & !LOCKS, event))
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
                Some(
                    Kind::Functional(Code::U(n)) | Kind::KeypadText(_, n) | Kind::Modifier(Some(n)),
                ) => n,
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
