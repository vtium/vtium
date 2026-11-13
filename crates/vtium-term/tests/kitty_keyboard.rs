//! The kitty keyboard protocol (ADR-1005): the flag stacks a program
//! pushes, pops, sets and queries, one for each screen, what resets them,
//! and the bytes a key sends under each of the five flags: 1, disambiguate
//! escape codes; 2, report event types; 4, report alternate keys; 8,
//! report all keys as escape codes; and 16, report associated text.
//!
//! Each expected sequence follows the specification, "Comprehensive
//! keyboard handling in terminals"
//! (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>), in the section
//! each test names. Keys are X11 keysyms, as the host reports them.

mod common;

use common::{rows, scrollback, term};
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
// `CSI ? u` reports the flags in force, and vtium keeps only the five
// flags the specification documents, 1 to 16, so after a push of 63 it
// reports 31 (ADR-1005, D3 and D5). `CSI = flags ; mode u` sets exactly,
// sets bits or clears bits.
// covers: input.kitty-keyboard
#[test]
fn the_query_reports_only_the_flags_vtium_implements() {
    let mut t = term(80, 4, b"");
    assert_eq!(flags(&mut t), "\x1b[?0u");
    t.input(b"\x1b[>63u");
    assert_eq!(flags(&mut t), "\x1b[?31u");
    t.input(b"\x1b[=0u");
    assert_eq!(flags(&mut t), "\x1b[?0u");
    t.input(b"\x1b[=255;2u");
    assert_eq!(flags(&mut t), "\x1b[?31u");
    t.input(b"\x1b[=1;3u");
    assert_eq!(flags(&mut t), "\x1b[?30u");
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

// ADR-1005, D10: the host's reset, which vtium's reset key calls, leaves
// the terminal as RIS does, the flag stacks of both screens emptied among
// it. The replies a program's queries left queued are dropped, since the
// program that asked is being reset, and a sequence the program had half
// written is dropped, as the ESC of RIS would drop it.
// covers: input.kitty-keyboard
#[test]
fn a_host_reset_leaves_the_terminal_as_ris_does() {
    let setup: &[u8] = b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n\x1b[>1u\x1b[>1u\x1b[?1h\x1b[>4;2m\x1b[1;31m\x1b[?1049h\x1b[>1u\x1b[2;5Hx";
    // DSR's reply waits for the host to take it, and `CSI 3` is half a
    // sequence.
    let last: &[u8] = b"\x1b[6n\x1b[3";
    let mut by_ris = term(80, 4, b"");
    by_ris.input(setup);
    by_ris.input(last);
    by_ris.input(b"\x1bc");
    by_ris.take_replies();
    let mut by_host = term(80, 4, b"");
    by_host.input(setup);
    by_host.input(last);
    assert!(!by_host.replies().is_empty());
    by_host.reset();
    assert_eq!(by_host.replies(), b"", "the queued reply is dropped");
    for t in [&mut by_ris, &mut by_host] {
        t.input(b"1mA");
    }
    for query in [
        &b"\x1b[?u"[..],
        b"\x1b[6n",
        b"\x1b[?1$p",
        b"\x1b[?1049$p",
        b"\x1b[?4m",
        b"\x1bP$qm\x1b\\",
        b"\x1b[?1049h\x1b[?u\x1b[?1049l",
    ] {
        assert_eq!(
            String::from_utf8_lossy(&send(&mut by_host, query)),
            String::from_utf8_lossy(&send(&mut by_ris, query)),
            "{query:?}"
        );
    }
    assert_eq!(flags(&mut by_host), "\x1b[?0u", "the main screen");
    by_host.input(b"\x1b[?1049h");
    assert_eq!(flags(&mut by_host), "\x1b[?0u", "the alternate screen");
    by_host.input(b"\x1b[?1049l");
    assert_eq!(rows(by_host.screen()), rows(by_ris.screen()));
    assert_eq!(rows(by_host.screen())[0].trim_end(), "1mA");
    assert_eq!(scrollback(by_host.screen()), Vec::<String>::new());
    assert_eq!(scrollback(by_ris.screen()), Vec::<String>::new());
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

/// `key` with the event `event`.
fn event(key: Key, event: KeyEvent) -> Key {
    Key { event, ..key }
}

/// A key's press, repeat and release, each as it is sent.
fn three(t: &mut Terminal, key: Key) -> [String; 3] {
    [KeyEvent::Press, KeyEvent::Repeat, KeyEvent::Release].map(|e| press(t, event(key, e)))
}

// "Report event types" and "Event types": under flags 3 a key that is
// sent as an escape code carries its event type after its modifiers and
// a colon, 2 for a repeat and 3 for a release, and a press none; with no
// modifier held the modifiers field is 1. The functional keys keep the
// forms of "Functional key definitions", with `CSI 1` before a letter
// that carries a modifiers field. Once the flags are 0, a release sends
// nothing and a repeat is a press again.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_3_reports_repeats_and_releases() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>3u");
    assert_eq!(flags(&mut t), "\x1b[?3u");
    let none = Mods::empty();
    let esc = Key::press(ESCAPE, 0, none, None);
    assert_eq!(
        three(&mut t, esc),
        ["\x1b[27u", "\x1b[27;1:2u", "\x1b[27;1:3u"]
    );
    let ctrl_a = Key::press(0x61, 0x61, Mods::CTRL, Some('a'));
    assert_eq!(
        three(&mut t, ctrl_a),
        ["\x1b[97;5u", "\x1b[97;5:2u", "\x1b[97;5:3u"]
    );
    let up = Key::press(UP, 0, none, None);
    assert_eq!(three(&mut t, up), ["\x1b[A", "\x1b[1;1:2A", "\x1b[1;1:3A"]);
    let shift_up = Key::press(UP, 0, Mods::SHIFT, None);
    assert_eq!(
        three(&mut t, shift_up),
        ["\x1b[1;2A", "\x1b[1;2:2A", "\x1b[1;2:3A"]
    );
    let delete = Key::press(DELETE, 0, none, None);
    assert_eq!(
        three(&mut t, delete),
        ["\x1b[3~", "\x1b[3;1:2~", "\x1b[3;1:3~"]
    );
    let kp_end = Key::press(KP_END, 0, none, None);
    assert_eq!(
        three(&mut t, kp_end),
        ["\x1b[57424u", "\x1b[57424;1:2u", "\x1b[57424;1:3u"]
    );
    let ctrl_kp_1 = Key::press(KP_1, 0, mods(&[Mods::CTRL, Mods::NUM]), Some('1'));
    assert_eq!(
        three(&mut t, ctrl_kp_1),
        ["\x1b[57400;133u", "\x1b[57400;133:2u", "\x1b[57400;133:3u"]
    );
    t.input(b"\x1b[<u");
    assert_eq!(three(&mut t, esc), ["\x1b", "\x1b", ""]);
    assert_eq!(three(&mut t, up), ["\x1b[A", "\x1b[A", ""]);
}

// "Report event types": Enter, Tab and Backspace keep their legacy bytes
// and have no release events unless flag 8 is set, so a user can still
// type reset; with a modifier held, a repeat carries its event type. A key
// that makes text sends its text on a press and a repeat, and nothing on
// its release ("Key events that result in text are reported as plain
// UTF-8 text, so events are not supported for them").
// covers: input.kitty-keyboard
#[test]
fn flag_2_sends_no_release_for_enter_tab_backspace_or_text() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>3u");
    let none = Mods::empty();
    let key = |s: u32, m: Mods| Key::press(s, 0, m, None);
    assert_eq!(three(&mut t, key(RETURN, none)), ["\r", "\r", ""]);
    assert_eq!(three(&mut t, key(TAB, none)), ["\t", "\t", ""]);
    assert_eq!(three(&mut t, key(BACKSPACE, none)), ["\x08", "\x08", ""]);
    assert_eq!(
        three(&mut t, key(RETURN, Mods::CTRL)),
        ["\x1b[13;5u", "\x1b[13;5:2u", ""]
    );
    let a = Key::press(0x61, 0x61, none, Some('a'));
    assert_eq!(three(&mut t, a), ["a", "a", ""]);
    let shift_a = Key::press(0x41, 0x61, Mods::SHIFT, Some('A'));
    assert_eq!(three(&mut t, shift_a), ["A", "A", ""]);
    let kp_1 = Key::press(KP_1, 0, Mods::NUM, Some('1'));
    assert_eq!(three(&mut t, kp_1), ["1", "1", ""]);
    // a modifier key sends nothing until flag 8
    assert_eq!(three(&mut t, key(SHIFT_L, Mods::SHIFT)), ["", "", ""]);
}

// "Progressive enhancement": each flag acts alone. Under flag 2 without
// flag 1, keys keep the specification's legacy encoding ("Legacy key event
// encoding"), and only a key whose legacy form is an escape code with a
// modifiers field, the cursor, editing and function keys, carries a
// repeat's or a release's event type; Escape, a Ctrl key and a text key
// send their legacy bytes on a press and a repeat, and nothing on a
// release. modifyOtherKeys is not consulted while the flags are not 0
// (ADR-1005, D6).
// covers: input.kitty-keyboard
#[test]
fn flag_2_alone_reports_events_for_the_legacy_escape_codes() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>4;2m\x1b[>2u");
    assert_eq!(flags(&mut t), "\x1b[?2u");
    let none = Mods::empty();
    let key = |s: u32, m: Mods| Key::press(s, 0, m, None);
    assert_eq!(
        three(&mut t, key(UP, none)),
        ["\x1b[A", "\x1b[1;1:2A", "\x1b[1;1:3A"]
    );
    assert_eq!(
        three(&mut t, key(F3, Mods::SHIFT)),
        ["\x1b[1;2R", "\x1b[13;2:2~", "\x1b[13;2:3~"]
    );
    assert_eq!(
        three(&mut t, key(DELETE, none)),
        ["\x1b[3~", "\x1b[3;1:2~", "\x1b[3;1:3~"]
    );
    assert_eq!(three(&mut t, key(ESCAPE, none)), ["\x1b", "\x1b", ""]);
    let ctrl_a = Key::press(0x61, 0x61, Mods::CTRL, Some('a'));
    assert_eq!(three(&mut t, ctrl_a), ["\x01", "\x01", ""]);
    // a key that makes text sends its text, and its release as an escape
    // code (ADR-1005, the maintainer's decision of 2026-11-13)
    let a = Key::press(0x61, 0x61, none, Some('a'));
    assert_eq!(three(&mut t, a), ["a", "a", "\x1b[97;1:3u"]);
    // the cursor key mode still chooses SS3 for a press
    t.input(b"\x1b[?1h");
    assert_eq!(
        three(&mut t, key(UP, none)),
        ["\x1bOA", "\x1b[1;1:2A", "\x1b[1;1:3A"]
    );
}

// "Report alternate keys" and "Key codes": under flags 5 a key sent as an
// escape code carries the shifted key after its code and a colon, only
// while Shift is held, and the base layout key after a second colon, only
// where it differs from the code; with a base layout key and no shifted
// key the shifted key's sub-field is empty. A key that sends text, and a
// key the table of functional keys numbers, carries neither. Under flag 4
// alone a key that would send text or its legacy bytes still does.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_5_reports_the_shifted_and_base_layout_keys() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>5u");
    assert_eq!(flags(&mut t), "\x1b[?5u");
    let shift_ctrl = mods(&[Mods::SHIFT, Mods::CTRL]);
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), "\x1b[97;5u");
    assert_eq!(char_key(&mut t, 'A', shift_ctrl), "\x1b[97:65;6u");
    assert_eq!(
        char_key(&mut t, 'A', mods(&[Mods::SHIFT, Mods::ALT])),
        "\x1b[97:65;4u"
    );
    // a shifted key equal to the code is left out
    assert_eq!(char_key(&mut t, ' ', shift_ctrl), "\x1b[32;6u");
    // with Caps Lock on, Shift makes a, and the host's shifted key, the
    // key's keysym at its Shift level, is A
    let caps_shift_a = Key {
        unshifted: 0x61,
        shifted: 0x41,
        ..Key::press(
            0x61,
            0x61,
            mods(&[Mods::LOCK, Mods::SHIFT, Mods::CTRL]),
            Some('a'),
        )
    };
    assert_eq!(press(&mut t, caps_shift_a), "\x1b[97:65;70u");
    // Caps Lock without Shift reports no shifted key
    assert_eq!(
        char_key(&mut t, 'A', mods(&[Mods::LOCK, Mods::CTRL])),
        "\x1b[97;69u"
    );
    // Ctrl+Shift+1 on a US layout: the unshifted key 1 is the code, and !
    // the shifted key
    let bang = Key {
        unshifted: u32::from(b'1'),
        base: Some('1'),
        ..Key::press(u32::from(b'!'), 0x21, shift_ctrl, Some('!'))
    };
    assert_eq!(press(&mut t, bang), "\x1b[49:33;6u");
    // Ctrl+С and Ctrl+Shift+С on a Russian layout: the base layout key c
    let es = Key {
        unshifted: CYRILLIC_ES,
        base: Some('c'),
        ..Key::press(CYRILLIC_ES, 0x63, Mods::CTRL, Some('\u{441}'))
    };
    assert_eq!(press(&mut t, es), "\x1b[1089::99;5u");
    let shift_es = Key {
        unshifted: CYRILLIC_ES,
        base: Some('c'),
        ..Key::press(0x6f3, 0x43, shift_ctrl, Some('\u{421}'))
    };
    assert_eq!(press(&mut t, shift_es), "\x1b[1089:1057:99;6u");
    // a base layout key equal to the code is left out
    let ctrl_a = Key {
        unshifted: 0x61,
        base: Some('a'),
        ..Key::press(0x61, 0x61, Mods::CTRL, Some('a'))
    };
    assert_eq!(press(&mut t, ctrl_a), "\x1b[97;5u");
    // keys that send text, and the functional keys, carry no alternates
    let plain_es = Key {
        unshifted: CYRILLIC_ES,
        base: Some('c'),
        ..Key::press(CYRILLIC_ES, 0x63, Mods::empty(), Some('\u{441}'))
    };
    assert_eq!(press(&mut t, plain_es), "\u{441}");
    assert_eq!(sym(&mut t, UP, Mods::SHIFT), "\x1b[1;2A");
    assert_eq!(sym(&mut t, ESCAPE, Mods::SHIFT), "\x1b[27;2u");
    // flag 4 alone changes no key: a pure enhancement to escape codes that
    // other flags make
    t.input(b"\x1b[=4u");
    assert_eq!(flags(&mut t), "\x1b[?4u");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), "\x01");
    assert_eq!(sym(&mut t, ESCAPE, Mods::empty()), "\x1b");
    assert_eq!(sym(&mut t, UP, Mods::SHIFT), "\x1b[1;2A");
}

// "Report all keys as escape codes": under flags 9 every key is an escape
// code and no text is sent. A text key is `CSI code u`, with the locks
// among its modifiers; Enter, Tab and Backspace are `CSI 13 u`, `CSI 9 u`
// and `CSI 127 u`; a modifier key reports its own number from the table
// of functional keys, with its own bit set as its press leaves it
// ("Modifiers"); the keypad's text keys are their own numbers. Flag 8
// implies disambiguation, so under flag 8 alone Escape is `CSI 27 u`.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_9_reports_every_key_as_an_escape_code() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>9u");
    assert_eq!(flags(&mut t), "\x1b[?9u");
    let none = Mods::empty();
    assert_eq!(char_key(&mut t, 'a', none), "\x1b[97u");
    assert_eq!(char_key(&mut t, 'A', Mods::SHIFT), "\x1b[97;2u");
    assert_eq!(char_key(&mut t, 'A', Mods::LOCK), "\x1b[97;65u");
    assert_eq!(char_key(&mut t, 'a', Mods::NUM), "\x1b[97;129u");
    assert_eq!(char_key(&mut t, ' ', none), "\x1b[32u");
    assert_eq!(sym(&mut t, RETURN, none), "\x1b[13u");
    assert_eq!(sym(&mut t, TAB, none), "\x1b[9u");
    assert_eq!(sym(&mut t, ISO_LEFT_TAB, Mods::SHIFT), "\x1b[9;2u");
    assert_eq!(sym(&mut t, BACKSPACE, none), "\x1b[127u");
    assert_eq!(sym(&mut t, ESCAPE, none), "\x1b[27u");
    assert_eq!(sym(&mut t, UP, none), "\x1b[A");
    assert_eq!(sym(&mut t, SHIFT_L, Mods::SHIFT), "\x1b[57441;2u");
    assert_eq!(sym(&mut t, 0xffe4, Mods::CTRL), "\x1b[57448;5u");
    assert_eq!(sym(&mut t, 0xffe9, Mods::ALT), "\x1b[57443;3u");
    assert_eq!(sym(&mut t, 0xffeb, Mods::LOGO), "\x1b[57444;9u");
    assert_eq!(sym(&mut t, CAPS_LOCK, Mods::LOCK), "\x1b[57358;65u");
    assert_eq!(sym(&mut t, 0xff7f, Mods::NUM), "\x1b[57360;129u");
    assert_eq!(sym(&mut t, 0xfe03, none), "\x1b[57453u");
    assert_eq!(sym(&mut t, 0xffe7, Mods::META), "\x1b[57446;33u");
    assert_eq!(sym(&mut t, 0xffe8, Mods::META), "\x1b[57452;33u");
    assert_eq!(sym(&mut t, 0xffed, Mods::HYPER), "\x1b[57445;17u");
    assert_eq!(sym(&mut t, 0xffee, Mods::HYPER), "\x1b[57451;17u");
    let kp_1 = Key::press(KP_1, 0, Mods::NUM, Some('1'));
    assert_eq!(press(&mut t, kp_1), "\x1b[57400;129u");
    let es = Key::press(CYRILLIC_ES, 0x63, none, Some('\u{441}'));
    assert_eq!(press(&mut t, es), "\x1b[1089u");
    // flag 8 alone disambiguates too
    t.input(b"\x1b[=8u");
    assert_eq!(flags(&mut t), "\x1b[?8u");
    assert_eq!(sym(&mut t, ESCAPE, none), "\x1b[27u");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), "\x1b[97;5u");
    assert_eq!(char_key(&mut t, 'a', none), "\x1b[97u");
    // RIS ends it, as the reset key does
    t.input(b"\x1bc");
    assert_eq!(sym(&mut t, RETURN, none), "\r");
    assert_eq!(char_key(&mut t, 'a', none), "a");
}

// "Report all keys as escape codes" and "Report event types": under
// flags 11 every key reports its repeat and its release, a text key,
// Enter, Tab, Backspace and a modifier key among them. A modifier key's
// release carries the modifiers after it, so Shift's release has the
// modifiers field 1.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_11_reports_the_release_of_every_key() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>11u");
    let none = Mods::empty();
    let key = |s: u32, m: Mods| Key::press(s, 0, m, None);
    let a = Key::press(0x61, 0x61, none, Some('a'));
    assert_eq!(
        three(&mut t, a),
        ["\x1b[97u", "\x1b[97;1:2u", "\x1b[97;1:3u"]
    );
    assert_eq!(
        three(&mut t, key(RETURN, none)),
        ["\x1b[13u", "\x1b[13;1:2u", "\x1b[13;1:3u"]
    );
    assert_eq!(
        three(&mut t, key(TAB, none)),
        ["\x1b[9u", "\x1b[9;1:2u", "\x1b[9;1:3u"]
    );
    assert_eq!(
        three(&mut t, key(BACKSPACE, none)),
        ["\x1b[127u", "\x1b[127;1:2u", "\x1b[127;1:3u"]
    );
    assert_eq!(sym(&mut t, SHIFT_L, Mods::SHIFT), "\x1b[57441;2u");
    assert_eq!(
        press(&mut t, event(key(SHIFT_L, none), KeyEvent::Release)),
        "\x1b[57441;1:3u"
    );
    let kp_1 = Key::press(KP_1, 0, Mods::NUM, Some('1'));
    assert_eq!(press(&mut t, kp_1), "\x1b[57400;129u");
    assert_eq!(
        press(&mut t, event(kp_1, KeyEvent::Release)),
        "\x1b[57400;129:3u"
    );
}

// "Report associated text" and "Text as code points": under flags 25 a
// key that makes text carries the text's code points in a third field,
// after an empty modifiers field where no modifier is held. A key held
// with Ctrl, Alt or another modifier but Shift and the locks makes no
// text, and the text never holds a control character. A release carries
// no text, and a repeat carries it as a press does. Flag 16 without flag
// 8 adds no text, since the specification leaves it undefined there.
// covers: input.kitty-keyboard
#[test]
fn a_push_of_25_reports_the_text_inside_the_code() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>25u");
    assert_eq!(flags(&mut t), "\x1b[?25u");
    let none = Mods::empty();
    assert_eq!(char_key(&mut t, 'a', none), "\x1b[97;;97u");
    assert_eq!(char_key(&mut t, 'A', Mods::SHIFT), "\x1b[97;2;65u");
    assert_eq!(char_key(&mut t, 'A', Mods::LOCK), "\x1b[97;65;65u");
    let ctrl_a = Key::press(0x61, 0x61, Mods::CTRL, Some('\x01'));
    assert_eq!(press(&mut t, ctrl_a), "\x1b[97;5u");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), "\x1b[97;3u");
    let es = Key::press(CYRILLIC_ES, 0x63, none, Some('\u{441}'));
    assert_eq!(press(&mut t, es), "\x1b[1089;;1089u");
    let kp_1 = Key::press(KP_1, 0, Mods::NUM, Some('1'));
    assert_eq!(press(&mut t, kp_1), "\x1b[57400;129;49u");
    assert_eq!(sym(&mut t, RETURN, none), "\x1b[13u");
    assert_eq!(sym(&mut t, ESCAPE, none), "\x1b[27u");
    t.input(b"\x1b[=27u");
    let a = Key::press(0x61, 0x61, none, Some('a'));
    assert_eq!(
        three(&mut t, a),
        ["\x1b[97;;97u", "\x1b[97;1:2;97u", "\x1b[97;1:3u"]
    );
    // with flag 4 the alternates come before the text
    t.input(b"\x1b[=29u");
    let shift_a = Key {
        unshifted: 0x61,
        base: Some('a'),
        ..Key::press(0x41, 0x61, Mods::SHIFT, Some('A'))
    };
    assert_eq!(press(&mut t, shift_a), "\x1b[97:65;2;65u");
    // flag 16 without flag 8 adds nothing
    t.input(b"\x1b[=17u");
    assert_eq!(char_key(&mut t, 'a', none), "a");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), "\x1b[97;3u");
}

// "Legacy key event encoding": under flag 2 without flag 1, a key the
// legacy section does not describe, F13 to F35 and the other keys the
// table of functional keys numbers from 57344, is sent in the standard
// `CSI number u` encoding, with its repeat and release; Menu is the
// legacy table's `CSI 29 ~`. A keypad key is reported as its equivalent
// non-keypad key, so the keypad's End sends End's release.
// covers: input.kitty-keyboard
#[test]
fn flag_2_alone_reports_f13_and_the_keypad_keys_events() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>2u");
    let none = Mods::empty();
    let key = |s: u32, m: Mods| Key::press(s, 0, m, None);
    assert_eq!(
        three(&mut t, key(F13, none)),
        ["\x1b[57376u", "\x1b[57376;1:2u", "\x1b[57376;1:3u"]
    );
    assert_eq!(
        three(&mut t, key(0xffe0, Mods::SHIFT)),
        ["\x1b[57398;2u", "\x1b[57398;2:2u", "\x1b[57398;2:3u"]
    );
    assert_eq!(
        three(&mut t, key(AUDIO_PLAY, none)),
        ["\x1b[57428u", "\x1b[57428;1:2u", "\x1b[57428;1:3u"]
    );
    assert_eq!(
        three(&mut t, key(MENU, none)),
        ["\x1b[29~", "\x1b[29;1:2~", "\x1b[29;1:3~"]
    );
    assert_eq!(
        three(&mut t, key(KP_END, none)),
        ["\x1b[F", "\x1b[1;1:2F", "\x1b[1;1:3F"]
    );
    assert_eq!(
        three(&mut t, key(KP_BEGIN, none)),
        ["\x1b[E", "\x1b[1;1:2E", "\x1b[1;1:3E"]
    );
    assert_eq!(
        three(&mut t, key(0xff9f, Mods::CTRL)),
        ["\x1b[3;5~", "\x1b[3;5:2~", "\x1b[3;5:3~"]
    );
    // the keypad's Enter is Enter, with no release, and its 1 is the text
    // key 1, whose release leaves out the locks as the legacy encoding does
    assert_eq!(three(&mut t, key(KP_ENTER, none)), ["\r", "\r", ""]);
    let kp_1 = Key::press(KP_1, 0, Mods::NUM, Some('1'));
    assert_eq!(three(&mut t, kp_1), ["1", "1", "\x1b[49;1:3u"]);
    // without flag 2 the press alone, and at flags 0 F13 is xterm's again
    t.input(b"\x1b[=4u");
    assert_eq!(
        three(&mut t, key(F13, none)),
        ["\x1b[57376u", "\x1b[57376u", ""]
    );
    t.input(b"\x1b[=0u");
    assert_eq!(three(&mut t, key(F13, none)), ["\x1b[25~", "\x1b[25~", ""]);
}

// ADR-1005 D7, and the maintainer's decision of 2026-11-13: whether a key
// gets a release follows from its press. A key pressed while flag 2 was in
// force, whose press was an escape code, gets a release as an escape code,
// with the code its press used and the modifiers held at the release,
// whatever the flags are then. A key pressed while flag 2 was not in force
// gets none. "Report event types": a key whose press sent its text under
// flag 1 gets no release, nor do Enter, Tab and Backspace without flag 8.
// covers: input.kitty-keyboard
#[test]
fn a_release_follows_from_its_press() {
    let none = Mods::empty();
    let a = Key::press(0x61, 0x61, none, Some('a'));
    let release = |k: Key| event(k, KeyEvent::Release);
    // Ctrl+a, with Ctrl let go first
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>3u");
    let ctrl_a = Key::press(0x61, 0x61, Mods::CTRL, Some('a'));
    assert_eq!(press(&mut t, ctrl_a), "\x1b[97;5u");
    assert_eq!(press(&mut t, release(a)), "\x1b[97;1:3u");
    // a press that sent its text, and a release with no press
    assert_eq!(press(&mut t, a), "a");
    assert_eq!(press(&mut t, release(a)), "");
    assert_eq!(press(&mut t, release(ctrl_a)), "", "released once");
    // pressed under flags 3, released at flags 0
    let up = Key::press(UP, 0, none, None);
    assert_eq!(press(&mut t, up), "\x1b[A");
    t.input(b"\x1b[<u");
    assert_eq!(press(&mut t, release(up)), "\x1b[1;1:3A");
    // pressed at flags 0, released under flags 3
    assert_eq!(press(&mut t, ctrl_a), "\x01");
    t.input(b"\x1b[>3u");
    assert_eq!(press(&mut t, release(ctrl_a)), "");
    // F13 pressed at flags 0, released under flag 2 alone
    let f13 = Key::press(F13, 0, none, None);
    t.input(b"\x1b[=0u");
    assert_eq!(press(&mut t, f13), "\x1b[25~");
    t.input(b"\x1b[=2u");
    assert_eq!(press(&mut t, release(f13)), "");
    // a pressed under flags 11, released under flags 3
    t.input(b"\x1b[=11u");
    assert_eq!(press(&mut t, a), "\x1b[97u");
    t.input(b"\x1b[=3u");
    assert_eq!(press(&mut t, release(a)), "\x1b[97;1:3u");
    // Ctrl+Enter under flags 3 sends no release
    let ctrl_enter = Key::press(RETURN, 0, Mods::CTRL, None);
    assert_eq!(press(&mut t, ctrl_enter), "\x1b[13;5u");
    assert_eq!(press(&mut t, release(ctrl_enter)), "");
    // RIS forgets the keys held
    assert_eq!(press(&mut t, ctrl_a), "\x1b[97;5u");
    t.input(b"\x1bc\x1b[>3u");
    assert_eq!(press(&mut t, release(ctrl_a)), "");
}

// ADR-1005, the maintainer's decision of 2026-11-13: under flag 2 without
// flag 1 or flag 8, a key that makes text sends its text on a press and a
// repeat, and its release as `CSI code u` with the release's event type.
// The legacy encoding leaves out the locks. A key that makes no text, such
// as Escape or Ctrl+a, keeps its legacy bytes and gets no release.
// covers: input.kitty-keyboard
#[test]
fn flag_2_alone_reports_the_release_of_a_text_key() {
    let mut t = term(80, 4, b"");
    t.input(b"\x1b[>2u");
    let shift_a = Key {
        unshifted: 0x61,
        ..Key::press(0x41, 0x61, Mods::SHIFT, Some('A'))
    };
    assert_eq!(three(&mut t, shift_a), ["A", "A", "\x1b[97;2:3u"]);
    let caps_a = Key::press(0x41, 0x61, Mods::LOCK, Some('A'));
    assert_eq!(three(&mut t, caps_a), ["A", "A", "\x1b[97;1:3u"]);
    let es = Key::press(CYRILLIC_ES, 0x63, Mods::empty(), Some('\u{441}'));
    assert_eq!(three(&mut t, es), ["\u{441}", "\u{441}", "\x1b[1089;1:3u"]);
    let ctrl_a = Key::press(0x61, 0x61, Mods::CTRL, Some('a'));
    assert_eq!(three(&mut t, ctrl_a), ["\x01", "\x01", ""]);
    let esc = Key::press(ESCAPE, 0, Mods::empty(), None);
    assert_eq!(three(&mut t, esc), ["\x1b", "\x1b", ""]);
    // Shift let go before the key: the release carries the modifiers then
    assert_eq!(press(&mut t, shift_a), "A");
    let released = Key {
        event: KeyEvent::Release,
        mods: Mods::empty(),
        ..shift_a
    };
    assert_eq!(press(&mut t, released), "\x1b[97;1:3u");
}
