//! What the terminal sends the program for keys and pastes: xterm's legacy
//! encoding with its Ctrl control bytes (aaronsb/libtsm#72), modifyOtherKeys
//! (aaronsb/libtsm#25), and pastes filtered as xterm filters them. The
//! pointer and focus reports are in `modes.rs`, beside the modes that ask
//! for them.
//!
//! Each expected sequence is the C reference's, or the specification's
//! where vtium departs from it (the crate's README lists each departure).
//! Keys are X11 keysyms, as the host reports them.

mod common;

use common::term;
use vtium_term::{Mods, MouseButton, MouseEvent, Terminal};

const BACKSPACE: u32 = 0xff08;
const TAB: u32 = 0xff09;
const RETURN: u32 = 0xff0d;
const ESCAPE: u32 = 0xff1b;
const UP: u32 = 0xff52;
const PAGE_UP: u32 = 0xff55;
const INSERT: u32 = 0xff63;
const DELETE: u32 = 0xffff;
const ISO_LEFT_TAB: u32 = 0xfe20;
const F1: u32 = 0xffbe;
const CYRILLIC_ES: u32 = 0x6d3;

fn key(t: &mut Terminal, sym: u32, ascii: u32, mods: Mods, c: Option<char>) -> Vec<u8> {
    t.take_replies();
    t.key(sym, ascii, mods, c);
    t.take_replies()
}

/// A printable key: its keysym is its character.
fn char_key(t: &mut Terminal, c: char, mods: Mods) -> Vec<u8> {
    let sym = u32::from(c);
    key(t, sym, sym, mods, Some(c))
}

fn paste(t: &mut Terminal, data: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.paste(data);
    t.take_replies()
}

/// `text` bracketed as a paste is.
fn bracketed(text: &[u8]) -> Vec<u8> {
    [b"\x1b[200~", text, b"\x1b[201~"].concat()
}

fn ctrl_alt() -> Mods {
    Mods::CTRL.union(Mods::ALT)
}

// ctlseqs, "PC-Style Function Keys" and xterm's us-pc105 table: characters
// as UTF-8, Alt putting ESC before them, Ctrl making the control bytes
// (from the ASCII layout's keysym, so Ctrl+C works on any layout), the
// editing, cursor and function keys with their modifiers.
// covers: input.legacy
#[test]
fn keys_send_xterms_legacy_encoding() {
    let mut t = term(8, 2, b"");
    let none = Mods::empty();
    assert_eq!(char_key(&mut t, 'a', none), b"a");
    assert_eq!(
        key(&mut t, 0xe9, 0, none, Some('\u{e9}')),
        "\u{e9}".as_bytes()
    );
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), b"\x1ba");
    assert_eq!(char_key(&mut t, 'c', Mods::CTRL), b"\x03");
    assert_eq!(char_key(&mut t, 'c', ctrl_alt()), b"\x1b\x03");
    assert_eq!(
        key(&mut t, CYRILLIC_ES, 0x63, Mods::CTRL, Some('\u{441}')),
        b"\x03"
    );
    for (c, byte) in [
        ('2', 0x00),
        (' ', 0x00),
        ('3', 0x1b),
        ('[', 0x1b),
        ('4', 0x1c),
        ('5', 0x1d),
    ] {
        assert_eq!(char_key(&mut t, c, Mods::CTRL), [byte], "Ctrl+{c}");
    }
    for (c, byte) in [
        ('6', 0x1e),
        ('~', 0x1e),
        ('7', 0x1f),
        ('_', 0x1f),
        ('8', 0x7f),
        ('?', 0x7f),
    ] {
        assert_eq!(char_key(&mut t, c, Mods::CTRL), [byte], "Ctrl+{c}");
    }
    assert_eq!(char_key(&mut t, '1', Mods::CTRL), b"1");
    assert_eq!(key(&mut t, BACKSPACE, 0, none, None), b"\x08");
    t.set_backspace_sends_delete(true);
    assert_eq!(key(&mut t, BACKSPACE, 0, none, None), b"\x7f");
    assert_eq!(key(&mut t, TAB, 0, none, None), b"\t");
    assert_eq!(key(&mut t, ISO_LEFT_TAB, 0, Mods::SHIFT, None), b"\x1b[Z");
    assert_eq!(key(&mut t, ESCAPE, 0, none, None), b"\x1b");
    assert_eq!(key(&mut t, RETURN, 0, Mods::ALT, None), b"\x1b\r");
    assert_eq!(key(&mut t, INSERT, 0, none, None), b"\x1b[2~");
    assert_eq!(key(&mut t, DELETE, 0, none, None), b"\x1b[3~");
    assert_eq!(key(&mut t, PAGE_UP, 0, none, None), b"\x1b[5~");
    assert_eq!(key(&mut t, PAGE_UP, 0, Mods::SHIFT, None), b"\x1b[5;2~");
    assert_eq!(key(&mut t, PAGE_UP, 0, Mods::CTRL, None), b"\x1b[5;5~");
    assert_eq!(
        key(&mut t, PAGE_UP, 0, Mods::CTRL.union(Mods::SHIFT), None),
        b"\x1b[5;6~"
    );
    assert_eq!(key(&mut t, PAGE_UP, 0, Mods::ALT, None), b"\x1b\x1b[5~");
    assert_eq!(key(&mut t, UP, 0, Mods::SHIFT, None), b"\x1b[1;2A");
    assert_eq!(key(&mut t, UP, 0, Mods::ALT, None), b"\x1b[1;3A");
    assert_eq!(key(&mut t, UP, 0, ctrl_alt(), None), b"\x1b[1;7A");
    assert_eq!(key(&mut t, F1, 0, none, None), b"\x1bOP");
    assert_eq!(key(&mut t, F1, 0, Mods::SHIFT, None), b"\x1b[23~");
    assert_eq!(key(&mut t, F1 + 4, 0, none, None), b"\x1b[15~");
    assert_eq!(key(&mut t, F1 + 4, 0, Mods::SHIFT, None), b"\x1b[28~");
    assert_eq!(key(&mut t, F1 + 11, 0, none, None), b"\x1b[24~");
    assert_eq!(key(&mut t, F1 + 11, 0, Mods::SHIFT, None), b"\x1b[24;2~");
    assert!(
        !t.key(0xffe1, 0, Mods::SHIFT, None),
        "Shift alone sends nothing"
    );
}

// ctlseqs, "Alt and Meta Keys", and xterm's "Modified Other Keys (us-pc105)"
// table: at level 1 a printable key with Alt, or with Ctrl where Ctrl makes
// no control byte, and Ctrl+?, are sent as `CSI 27 ; mod ; code ~`; Tab and
// Return with any modifier; BackSpace never; Shift+Tab stays CSI Z. Level 2
// encodes them all, but Shift alone only on space and `@` to `~`, and
// BackSpace not with Ctrl alone.
// covers: input.modify-other-keys
#[test]
fn modify_other_keys_encodes_modified_keys() {
    let mut t = term(8, 2, b"\x1b[>4;1m");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), b"\x1b[27;3;97~");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), b"\x01");
    assert_eq!(char_key(&mut t, '1', Mods::CTRL), b"\x1b[27;5;49~");
    assert_eq!(char_key(&mut t, '?', Mods::CTRL), b"\x1b[27;5;63~");
    assert_eq!(char_key(&mut t, 'A', Mods::SHIFT), b"A");
    assert_eq!(key(&mut t, RETURN, 0, Mods::CTRL, None), b"\x1b[27;5;13~");
    assert_eq!(key(&mut t, TAB, 0, Mods::ALT, None), b"\x1b[27;3;9~");
    assert_eq!(key(&mut t, ISO_LEFT_TAB, 0, Mods::SHIFT, None), b"\x1b[Z");
    assert_eq!(key(&mut t, ESCAPE, 0, Mods::CTRL, None), b"\x1b");
    assert_eq!(key(&mut t, ESCAPE, 0, Mods::ALT, None), b"\x1b[27;3;27~");
    assert_eq!(key(&mut t, BACKSPACE, 0, Mods::ALT, None), b"\x1b\x08");
    assert_eq!(key(&mut t, UP, 0, Mods::CTRL, None), b"\x1b[1;5A");
    t.input(b"\x1b[>4;2m");
    assert_eq!(char_key(&mut t, 'A', Mods::SHIFT), b"\x1b[27;2;65~");
    assert_eq!(char_key(&mut t, '!', Mods::SHIFT), b"!");
    assert_eq!(char_key(&mut t, ' ', Mods::SHIFT), b"\x1b[27;2;32~");
    assert_eq!(char_key(&mut t, 'a', Mods::CTRL), b"\x1b[27;5;97~");
    assert_eq!(
        key(&mut t, ISO_LEFT_TAB, 0, Mods::SHIFT, None),
        b"\x1b[27;2;9~"
    );
    assert_eq!(key(&mut t, ESCAPE, 0, Mods::CTRL, None), b"\x1b[27;5;27~");
    assert_eq!(key(&mut t, BACKSPACE, 0, Mods::CTRL, None), b"\x08");
    assert_eq!(
        key(&mut t, BACKSPACE, 0, Mods::SHIFT, None),
        b"\x1b[27;2;8~"
    );
    t.input(b"\x1b[>4;0m");
    assert_eq!(char_key(&mut t, 'a', Mods::ALT), b"\x1ba");
}

// ADR-1000: fixterms' `CSI u` encoding is superseded by the kitty keyboard
// protocol, so no key sends it, at any modifyOtherKeys level.
// covers: input.fixterms
#[test]
fn no_key_sends_fixterms_csi_u() {
    let mut t = term(8, 2, b"");
    for level in [&b"\x1b[>4;0m"[..], b"\x1b[>4;1m", b"\x1b[>4;2m"] {
        t.input(level);
        for mods in [Mods::SHIFT, Mods::CTRL, Mods::ALT, ctrl_alt()] {
            for c in ['a', 'A', '1', ' ', '?', '\u{e9}'] {
                let sent = char_key(&mut t, c, mods);
                assert!(!sent.ends_with(b"u"), "{c:?} with {mods:?} sent {sent:?}");
            }
        }
    }
}

// A typed character is sent in the input's encoding: its byte in 8-bit
// input to U+00FF and in 7-bit input to U+007F, and `?` past that.
// covers: input.legacy
#[test]
fn a_typed_character_is_sent_in_the_inputs_encoding() {
    for (decscl, e_acute, euro) in [
        (&b""[..], "\u{e9}".as_bytes(), "\u{20ac}".as_bytes()),
        (b"\x1b[62\"p", b"\xe9", b"?"),
        (b"\x1b[61\"p", b"?", b"?"),
    ] {
        let mut t = term(8, 2, decscl);
        let none = Mods::empty();
        assert_eq!(char_key(&mut t, '\u{e9}', none), e_acute, "{decscl:?}");
        assert_eq!(char_key(&mut t, '\u{20ac}', none), euro, "{decscl:?}");
    }
}

// xterm's paste filtering, with allowPasteControls false, turns ESC and the
// C1 controls in every paste into a space, so no form of the end marker
// reaches the program inside a bracketed paste (ADR-1000).
// covers: input.paste-end-marker
#[test]
fn a_bracketed_paste_never_carries_its_own_end() {
    let mut t = term(8, 2, b"\x1b[?2004h");
    for (text, sent) in [
        (&b"a\x1b[201~b"[..], &b"a [201~b"[..]),
        (b"\x1b[0201~", b" [0201~"),
        (b"\x1b[201;5~", b" [201;5~"),
        (b"\x1b[20\x001~", b" [20 1~"),
        (b"x\x1b", b"x "),
        (b"a\xc2\x9b201~", b"a 201~"),
    ] {
        assert_eq!(paste(&mut t, text), bracketed(sent), "{text:?}");
    }
}

// xterm's paste filtering, for a bracketed paste or not: of the C0
// controls only HT, LF and CR are pasted, LF as CR, and each other one,
// DEL and each C1 control, a code point in UTF-8, becomes one space.
// xterm sends each LF of a paste as CR, as the Return key sends, unless
// `pasteLiteralNl` is set (`button.c`, `_WriteSelectionData`); in 8-bit
// input too.
#[test]
fn a_paste_sends_each_line_end_as_cr() {
    let mut t = term(8, 2, b"");
    assert_eq!(paste(&mut t, b"one\ntwo\r\n"), b"one\rtwo\r\r");
    t.input(b"\x1b[62\"p");
    assert_eq!(paste(&mut t, b"a\nb"), b"a\rb");
}

#[test]
fn a_paste_keeps_only_the_formatting_controls() {
    let c0: Vec<u8> = (0..0x20).chain([0x7f]).collect();
    let c0_sent = [&[b' '; 9][..], b"\t\r", &[b' '; 2], b"\r", &[b' '; 19]].concat();
    let c1: String = ('\u{80}'..='\u{a0}').collect();
    let c1_sent = [&[b' '; 32][..], "\u{a0}".as_bytes()].concat();
    let mut t = term(8, 2, b"");
    assert_eq!(paste(&mut t, &c0), c0_sent);
    assert_eq!(paste(&mut t, c1.as_bytes()), c1_sent);
    t.input(b"\x1b[?2004h");
    assert_eq!(paste(&mut t, &c0), bracketed(&c0_sent));
    assert_eq!(paste(&mut t, c1.as_bytes()), bracketed(&c1_sent));
}

// A C1 control in UTF-8 is a code point: the bytes 0x80 to 0x9F inside a
// character are its own, and the character arrives whole.
#[test]
fn a_character_whose_bytes_look_like_c1_arrives_whole() {
    let mut t = term(8, 2, b"\x1b[?2004h");
    for text in ["\u{41b}201~ ok", "\u{11b}201~"] {
        assert_eq!(
            paste(&mut t, text.as_bytes()),
            bracketed(text.as_bytes()),
            "{text}"
        );
    }
}

// In UTF-8 ill-formed bytes are pasted as U+FFFD, so the program reads
// well-formed text and no stray byte joins what follows it.
#[test]
fn ill_formed_utf8_in_a_paste_becomes_the_replacement_character() {
    let mut t = term(8, 2, b"");
    assert_eq!(
        paste(&mut t, b"\xc2\xc2\x9b\x85\x9b201~"),
        "\u{fffd} \u{fffd}\u{fffd}201~".as_bytes()
    );
}

// A paste is UTF-8 text, filtered by code point, and sent in the input's
// encoding as a typed key is: in 8-bit input each character to U+00FF is
// its byte, in 7-bit input each one to U+007F, and any other character is
// `?`. A C1 control is filtered whatever the input, so neither its two
// UTF-8 bytes nor a byte 0x80 to 0x9F, which 8-bit input reads as a C1
// control, reaches the program.
// covers: input.paste-end-marker
#[test]
fn a_paste_is_sent_in_the_inputs_encoding() {
    let text = "\u{20ac} \u{e9}\u{a0}\u{9b}201~\u{85}\n".as_bytes();
    let stray = b"\x9b201~\xe9";
    for (decscl, sent, stray_sent) in [
        (
            &b""[..],
            "\u{20ac} \u{e9}\u{a0} 201~ \r".as_bytes(),
            "\u{fffd}201~\u{fffd}".as_bytes(),
        ),
        (b"\x1b[62\"p", b"? \xe9\xa0 201~ \r", b"?201~?"),
        (b"\x1b[61\"p", b"? ?? 201~ \r", b"?201~?"),
    ] {
        let mut t = term(8, 2, decscl);
        assert_eq!(paste(&mut t, text), sent, "{decscl:?}");
        assert_eq!(paste(&mut t, stray), stray_sent, "{decscl:?}");
        t.input(b"\x1b[?2004h");
        assert_eq!(paste(&mut t, text), bracketed(sent), "{decscl:?}");
    }
}

// An ordinary paste, tabs, line ends and text past ASCII, arrives as it
// was, bracketed or not, but each LF as CR.
#[test]
fn an_ordinary_paste_arrives_unchanged() {
    let text = "ls -l\t~/src\r\necho 'b\u{e5}r \u{65e5}\u{672c} \u{1f600}'\n".as_bytes();
    let sent = "ls -l\t~/src\r\recho 'b\u{e5}r \u{65e5}\u{672c} \u{1f600}'\r".as_bytes();
    let mut t = term(8, 2, b"");
    assert_eq!(paste(&mut t, text), sent);
    t.input(b"\x1b[?2004h");
    assert_eq!(paste(&mut t, text), bracketed(sent));
}

// ctlseqs, the X10 encoding: a cell counts from 1, plus 32, in one byte,
// and stops at 223.
#[test]
fn x10_mouse_positions_stop_at_223() {
    let mut t = term(300, 2, b"\x1b[?1000h");
    t.mouse(
        221,
        0,
        0,
        0,
        MouseButton::Left,
        MouseEvent::Press,
        Mods::empty(),
    );
    t.mouse(
        250,
        1,
        0,
        0,
        MouseButton::Left,
        MouseEvent::Press,
        Mods::empty(),
    );
    assert_eq!(t.take_replies(), b"\x1b[M\x20\xfe\x21\x1b[M\x20\xff\x22");
}
