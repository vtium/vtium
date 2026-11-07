//! The kitty keyboard protocol (ADR-1005): the flag stacks a program
//! pushes, pops, sets and queries, one for each screen, what resets them,
//! and the bytes a key sends under flag 1, disambiguate escape codes.
//!
//! Each expected sequence follows the specification, "Comprehensive
//! keyboard handling in terminals"
//! (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>), in the section
//! each test names. Keys are X11 keysyms, as the host reports them.

mod common;

use common::term;
use vtium_term::{Key, KeyEvent, Mods, Terminal};

const BACKSPACE: u32 = 0xff08;
const TAB: u32 = 0xff09;
const RETURN: u32 = 0xff0d;
const ESCAPE: u32 = 0xff1b;
const UP: u32 = 0xff52;
const HOME: u32 = 0xff50;
const INSERT: u32 = 0xff63;
const DELETE: u32 = 0xffff;
const MENU: u32 = 0xff67;
const ISO_LEFT_TAB: u32 = 0xfe20;
const KP_ENTER: u32 = 0xff8d;
const KP_END: u32 = 0xff9c;
const KP_BEGIN: u32 = 0xff9d;
const KP_ADD: u32 = 0xffab;
const KP_1: u32 = 0xffb1;
const F1: u32 = 0xffbe;
const F3: u32 = 0xffc0;
const F5: u32 = 0xffc2;
const F13: u32 = 0xffca;
const SHIFT_L: u32 = 0xffe1;
const CAPS_LOCK: u32 = 0xffe5;
const CYRILLIC_ES: u32 = 0x6d3;
const AUDIO_PLAY: u32 = 0x1008_ff14;

fn send(t: &mut Terminal, bytes: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(bytes);
    t.take_replies()
}

/// The flags in force on the screen shown, as `CSI ? u` reports them.
fn flags(t: &mut Terminal) -> String {
    String::from_utf8_lossy(&send(t, b"\x1b[?u")).into_owned()
}

fn press(t: &mut Terminal, key: Key) -> String {
    t.take_replies();
    t.key(&key);
    String::from_utf8_lossy(&t.take_replies()).into_owned()
}

fn sym(t: &mut Terminal, keysym: u32, mods: Mods) -> String {
    press(t, Key::press(keysym, 0, mods, None))
}

/// A printable key: its keysym is its character.
fn char_key(t: &mut Terminal, c: char, mods: Mods) -> String {
    let s = u32::from(c);
    press(t, Key::press(s, s, mods, Some(c)))
}

fn mods(list: &[Mods]) -> Mods {
    list.iter().fold(Mods::empty(), |a, &m| a.union(m))
}

// "Disambiguate escape codes": Escape, and Alt, Ctrl, Ctrl+Alt and
// Shift+Alt with a text key, as `CSI code ; modifiers u`, the code the
// unshifted key ("Key codes"), the modifiers 1 plus their bits
// ("Modifiers"); a text key with nothing or Shift alone sends its text,
// and the lock modifiers turn no text key into a code.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_1_sends_escape_alt_and_ctrl_keys_as_codes() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>1u");
    let none = Mods::empty();
    let lock = Mods::LOCK;
    assert_eq!(sym(&mut t, ESCAPE, none), "\x1b[27u");
    assert_eq!(sym(&mut t, ESCAPE, Mods::SHIFT), "\x1b[27;2u");
    assert_eq!(sym(&mut t, ESCAPE, lock), "\x1b[27;65u");
    assert_eq!(char_key(&mut t, 'a', none), "a");
    assert_eq!(char_key(&mut t, 'A', Mods::SHIFT), "A");
    assert_eq!(char_key(&mut t, 'A', lock), "A");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), "\x1b[97;3u");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), "\x1b[97;5u");
    assert_eq!(
        char_key(&mut t, 'a', mods(&[Mods::CTRL, Mods::ALT])),
        "\x1b[97;7u"
    );
    assert_eq!(
        char_key(&mut t, 'A', mods(&[Mods::SHIFT, Mods::ALT])),
        "\x1b[97;4u"
    );
    assert_eq!(
        char_key(&mut t, 'A', mods(&[Mods::SHIFT, Mods::CTRL])),
        "\x1b[97;6u"
    );
    assert_eq!(
        char_key(&mut t, 'a', mods(&[Mods::CTRL, lock])),
        "\x1b[97;69u"
    );
    assert_eq!(char_key(&mut t, 'a', Mods::LOGO), "\x1b[97;9u");
    assert_eq!(char_key(&mut t, 'a', Mods::HYPER), "\x1b[97;17u");
    assert_eq!(char_key(&mut t, 'a', Mods::META), "\x1b[97;33u");
    // the keys legacy Ctrl folds together: Ctrl+I is not Tab, Ctrl+[ is
    // not Escape, Ctrl+space is not NUL, Alt+[ does not start a CSI
    assert_eq!(char_key(&mut t, 'i', Mods::CTRL), "\x1b[105;5u");
    assert_eq!(char_key(&mut t, 'm', Mods::CTRL), "\x1b[109;5u");
    assert_eq!(char_key(&mut t, '[', Mods::CTRL), "\x1b[91;5u");
    assert_eq!(char_key(&mut t, '[', Mods::ALT), "\x1b[91;3u");
    assert_eq!(char_key(&mut t, ' ', Mods::CTRL), "\x1b[32;5u");
    assert_eq!(char_key(&mut t, '1', Mods::CTRL), "\x1b[49;5u");
    // a key of a non-Latin layout is sent by its own character
    let es = Key::press(CYRILLIC_ES, 0x63, Mods::CTRL, Some('\u{441}'));
    assert_eq!(press(&mut t, es), "\x1b[1089;5u");
    let es = Key::press(CYRILLIC_ES, 0x63, Mods::empty(), Some('\u{441}'));
    assert_eq!(press(&mut t, es), "\u{441}");
    // the host's unshifted key, where it reports one, is the code
    let bang = Key {
        unshifted: u32::from(b'1'),
        ..Key::press(
            u32::from(b'!'),
            0,
            mods(&[Mods::SHIFT, Mods::CTRL]),
            Some('!'),
        )
    };
    assert_eq!(press(&mut t, bang), "\x1b[49;6u");
}

// "Disambiguate escape codes": Enter, Tab and Backspace keep their legacy
// bytes, so a user can still type reset; with a modifier held they are
// `CSI 13 u`, `CSI 9 u` and `CSI 127 u` ("Functional key definitions").
// covers: input.kitty-keyboard
#[test]
fn a_push_of_1_keeps_enter_tab_and_backspace_unmodified() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>1u");
    let none = Mods::empty();
    assert_eq!(sym(&mut t, RETURN, none), "\r");
    assert_eq!(sym(&mut t, RETURN, Mods::LOCK), "\r");
    assert_eq!(sym(&mut t, TAB, none), "\t");
    assert_eq!(sym(&mut t, BACKSPACE, none), "\x08");
    t.set_backspace_sends_delete(true);
    assert_eq!(sym(&mut t, BACKSPACE, none), "\x7f");
    assert_eq!(sym(&mut t, RETURN, Mods::SHIFT), "\x1b[13;2u");
    assert_eq!(sym(&mut t, RETURN, Mods::ALT), "\x1b[13;3u");
    assert_eq!(sym(&mut t, ISO_LEFT_TAB, Mods::SHIFT), "\x1b[9;2u");
    assert_eq!(sym(&mut t, TAB, Mods::CTRL), "\x1b[9;5u");
    assert_eq!(sym(&mut t, BACKSPACE, Mods::CTRL), "\x1b[127;5u");
    // LNM still makes Enter CR LF, as it does in legacy mode
    t.input(b"\x1b[20h");
    assert_eq!(sym(&mut t, RETURN, none), "\r\n");
}

// "Disambiguate escape codes": the keypad's keys that make no text are
// keys of their own, `CSI number u` with the numbers of "Functional key
// definitions"; one that makes text sends it until a modifier other than
// Shift is held. DECKPAM changes nothing.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_1_sends_the_keypad_as_keys_of_its_own() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>1u");
    let none = Mods::empty();
    assert_eq!(
        press(&mut t, Key::press(KP_1, 0, Mods::NUM, Some('1'))),
        "1"
    );
    assert_eq!(sym(&mut t, KP_ADD, none), "+");
    assert_eq!(sym(&mut t, KP_END, none), "\x1b[57424u");
    assert_eq!(sym(&mut t, KP_ENTER, none), "\x1b[57414u");
    assert_eq!(sym(&mut t, KP_ENTER, Mods::NUM), "\x1b[57414;129u");
    assert_eq!(sym(&mut t, KP_BEGIN, none), "\x1b[57427u");
    assert_eq!(sym(&mut t, KP_1, Mods::CTRL), "\x1b[57400;5u");
    assert_eq!(sym(&mut t, KP_ADD, Mods::ALT), "\x1b[57413;3u");
    t.input(b"\x1b=");
    assert_eq!(
        press(&mut t, Key::press(KP_1, 0, Mods::NUM, Some('1'))),
        "1"
    );
    assert_eq!(sym(&mut t, KP_END, none), "\x1b[57424u");
}

// "Disambiguate escape codes": every key that makes no text is `CSI
// number ; modifiers u` or `CSI 1 ; modifiers [~ABCDEFHPQS]`, in the forms
// "Functional key definitions" gives; DECCKM changes nothing; modifier
// keys are reported only under flag 8; a release sends nothing without
// flag 2, and a repeat is sent as a press ("Report event types").
// covers: input.kitty-keyboard
#[test]
fn a_push_of_1_sends_the_functional_keys_in_the_specifications_forms() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>1u");
    let none = Mods::empty();
    assert_eq!(sym(&mut t, UP, none), "\x1b[A");
    assert_eq!(sym(&mut t, UP, Mods::CTRL), "\x1b[1;5A");
    assert_eq!(sym(&mut t, HOME, Mods::SHIFT), "\x1b[1;2H");
    t.input(b"\x1b[?1h");
    assert_eq!(sym(&mut t, UP, none), "\x1b[A");
    assert_eq!(sym(&mut t, F1, none), "\x1b[P");
    assert_eq!(sym(&mut t, F1, Mods::SHIFT), "\x1b[1;2P");
    assert_eq!(sym(&mut t, F3, none), "\x1b[13~");
    assert_eq!(sym(&mut t, F5, Mods::SHIFT), "\x1b[15;2~");
    assert_eq!(sym(&mut t, F13, none), "\x1b[57376u");
    assert_eq!(sym(&mut t, INSERT, none), "\x1b[2~");
    assert_eq!(sym(&mut t, DELETE, Mods::CTRL), "\x1b[3;5~");
    assert_eq!(sym(&mut t, MENU, none), "\x1b[57363u");
    assert_eq!(sym(&mut t, AUDIO_PLAY, none), "\x1b[57428u");
    assert_eq!(sym(&mut t, SHIFT_L, Mods::SHIFT), "");
    assert_eq!(sym(&mut t, CAPS_LOCK, Mods::LOCK), "");
    let release = Key {
        event: KeyEvent::Release,
        ..Key::press(ESCAPE, 0, none, None)
    };
    assert_eq!(press(&mut t, release), "");
    let repeat = Key {
        event: KeyEvent::Repeat,
        ..Key::press(ESCAPE, 0, none, None)
    };
    assert_eq!(press(&mut t, repeat), "\x1b[27u");
}

// "Progressive enhancement" and "Detection of support for this protocol":
// `CSI ? u` reports the flags in force, and vtium keeps only the flags it
// implements, so after a push of 31 it reports 1 (ADR-1005, D3 and D5).
// `CSI = flags ; mode u` sets exactly, sets bits or clears bits.
// covers: input.kitty-keyboard
#[test]
fn the_query_reports_only_the_flags_vtium_implements() {
    let mut t = term(80, 4, b"");
    assert_eq!(flags(&mut t), "\x1b[?0u");
    t.input(b"\x1b[>31u");
    assert_eq!(flags(&mut t), "\x1b[?1u");
    t.input(b"\x1b[=0u");
    assert_eq!(flags(&mut t), "\x1b[?0u");
    t.input(b"\x1b[=31;2u");
    assert_eq!(flags(&mut t), "\x1b[?1u");
    t.input(b"\x1b[=1;3u");
    assert_eq!(flags(&mut t), "\x1b[?0u");
    t.input(b"\x1b[=1;1u");
    assert_eq!(flags(&mut t), "\x1b[?1u");
    t.input(b"\x1b[=0;4u");
    assert_eq!(flags(&mut t), "\x1b[?1u", "mode 4 changes nothing");
    // the query comes before DA1's reply, as the specification's detection
    // reads them
    assert_eq!(
        String::from_utf8_lossy(&send(&mut t, b"\x1b[?u\x1b[c")),
        "\x1b[?1u\x1b[?62;22c"
    );
}

// "Progressive enhancement": the main and alternate screens keep their own
// stacks, so a switch by 47, 1047 or 1049 keeps each screen's flags
// (ADR-1005, D4), and a key reads the shown screen's flags.
// covers: input.kitty-keyboard
#[test]
fn each_screen_keeps_its_own_flags_across_a_switch() {
    for mode in ["47", "1047", "1049"] {
        let mut t = term(80, 4, b"");
        let enter = format!("\x1b[?{mode}h");
        let leave = format!("\x1b[?{mode}l");
        t.input(b"\x1b[>1u");
        t.input(enter.as_bytes());
        assert_eq!(
            flags(&mut t),
            "\x1b[?0u",
            "{mode}: the alternate screen's own"
        );
        assert_eq!(sym(&mut t, ESCAPE, Mods::empty()), "\x1b", "{mode}");
        t.input(b"\x1b[>1u\x1b[>0u");
        t.input(leave.as_bytes());
        assert_eq!(flags(&mut t), "\x1b[?1u", "{mode}: the main screen's kept");
        assert_eq!(sym(&mut t, ESCAPE, Mods::empty()), "\x1b[27u", "{mode}");
        t.input(b"\x1b[<u");
        assert_eq!(flags(&mut t), "\x1b[?0u", "{mode}: the main screen's pop");
        t.input(enter.as_bytes());
        assert_eq!(
            flags(&mut t),
            "\x1b[?0u",
            "{mode}: the alternate screen's kept"
        );
        t.input(b"\x1b[<u");
        assert_eq!(
            flags(&mut t),
            "\x1b[?1u",
            "{mode}: the alternate screen's stack"
        );
    }
}

// ADR-1005, D4: RIS and DECSTR empty both stacks and put 0 in force on
// both screens.
// covers: input.kitty-keyboard
#[test]
fn ris_and_decstr_clear_both_stacks() {
    for reset in [&b"\x1b[!p"[..], b"\x1bc"] {
        let mut t = term(80, 4, b"");
        t.input(b"\x1b[>1u\x1b[>1u\x1b[?1049h\x1b[>1u\x1b[>1u");
        t.input(reset);
        t.input(b"\x1b[?1049h");
        assert_eq!(flags(&mut t), "\x1b[?0u", "{reset:?}: the alternate screen");
        t.input(b"\x1b[>0u\x1b[<u");
        assert_eq!(flags(&mut t), "\x1b[?0u", "{reset:?}: nothing saved there");
        t.input(b"\x1b[?1049l");
        assert_eq!(flags(&mut t), "\x1b[?0u", "{reset:?}: the main screen");
        t.input(b"\x1b[>0u\x1b[<u");
        assert_eq!(flags(&mut t), "\x1b[?0u", "{reset:?}: nothing saved there");
    }
}

// "Progressive enhancement": a push onto a full stack evicts the oldest
// entry. A stack keeps eight sets, the set in force and seven saved
// (ADR-1005, D3), and a pop that empties it puts 0 in force; a pop of 0
// pops nothing.
// covers: input.kitty-keyboard
#[test]
fn a_ninth_push_drops_the_oldest_level() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[=1u");
    t.input(&b"\x1b[>0u".repeat(7));
    t.input(b"\x1b[<7u");
    assert_eq!(flags(&mut t), "\x1b[?1u", "eight levels hold the first");
    t.input(&b"\x1b[>0u".repeat(8));
    t.input(b"\x1b[<7u");
    assert_eq!(
        flags(&mut t),
        "\x1b[?0u",
        "the ninth push dropped the first"
    );
    t.input(b"\x1b[=1u\x1b[>1u\x1b[<0u");
    assert_eq!(flags(&mut t), "\x1b[?1u", "a pop of 0");
    t.input(b"\x1b[<9u");
    assert_eq!(flags(&mut t), "\x1b[?0u", "a pop past the bottom");
}

// ADR-1005, D6: while the flags are 0 a key sends xterm's bytes, and a
// release nothing; a pop back to 0 brings them back.
// covers: input.kitty-keyboard
#[test]
fn flags_at_0_send_xterms_bytes() {
    let mut t = term(80, 4, b"");
    let release = Key {
        event: KeyEvent::Release,
        ..Key::press(u32::from(b'a'), 0x61, Mods::empty(), Some('a'))
    };
    assert_eq!(press(&mut t, release), "");
    t.input(b"\x1b[>1u");
    assert_eq!(sym(&mut t, ESCAPE, Mods::empty()), "\x1b[27u");
    t.input(b"\x1b[<u");
    assert_eq!(sym(&mut t, ESCAPE, Mods::empty()), "\x1b");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), "\x01");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), "\x1ba");
}

// ADR-1005, D6: modifyOtherKeys keeps its level while the flags are not
// 0, which XTQMODKEYS reports, and is consulted again once they are 0.
// covers: input.kitty-keyboard
#[test]
fn modify_other_keys_waits_while_the_flags_are_not_0() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>4;2m");
    assert_eq!(char_key(&mut t, '1', Mods::CTRL), "\x1b[27;5;49~");
    t.input(b"\x1b[>1u");
    assert_eq!(char_key(&mut t, '1', Mods::CTRL), "\x1b[49;5u");
    assert_eq!(
        String::from_utf8_lossy(&send(&mut t, b"\x1b[?4m")),
        "\x1b[>4;2m"
    );
    t.input(b"\x1b[>4;1m");
    t.input(b"\x1b[<u");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), "\x1b[27;3;97~");
}
