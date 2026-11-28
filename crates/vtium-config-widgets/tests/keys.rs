//! The rebuilt TUI's decoder: each sequence ADR-1206 D2's notes of
//! 2026-11-27 add, and the forms the old decoder reads that must not
//! change, the lone ESC, `ESC [` sequences and replies split across reads.
//!
//! References:
//! - ADR-1206 D2, the notes of 2026-11-27: Alt and a key, F10, Ctrl+S,
//!   F1 in three forms, Ctrl+Z, Ctrl+Y, Ctrl+F and Delete.
//! - ADR-1206 D2, "Esc stays the key that goes back", and the notes of
//!   2026-11-19: the lone ESC and the ESC that cancels a sequence.
//! - ADR-1208 D10: the key type.

use vtium_config_widgets::keys::{Button, Code, Decoder, Event, Key, Mods, Mouse, Reply};

fn keys(bytes: &[u8]) -> Vec<Event> {
    Decoder::new().feed(bytes)
}

fn key(code: Code) -> Event {
    Event::Key(Key::plain(code))
}

fn ch(c: char) -> Event {
    Event::Key(Key::char(c))
}

fn ctrl(c: char) -> Event {
    Event::Key(Key::ctrl(c))
}

fn alt(c: char) -> Event {
    Event::Key(Key::alt(c))
}

fn with(mods: Mods, code: Code) -> Event {
    Event::Key(Key { mods, code })
}

fn f(n: u8) -> Event {
    key(Code::F(n))
}

const ESC: Event = Event::Key(Key::ESCAPE);
const DA1: Event = Event::Reply(Reply::Attributes);

fn font(name: &str) -> Event {
    Event::Reply(Reply::Font(name.into()))
}

/// Everything the decoder holds whole, with `awaiting` fixed.
fn drain(d: &mut Decoder, awaiting: bool) -> Vec<Event> {
    std::iter::from_fn(|| d.next(awaiting)).collect()
}

// The sequences the notes of 2026-11-27 add.

#[test]
fn esc_and_a_key_in_one_read_is_alt_and_the_key() {
    assert_eq!(keys(b"\x1bo"), [alt('o')]);
    assert_eq!(keys(b"\x1bf\x1bs"), [alt('f'), alt('s')]);
    assert_eq!(keys("\x1bé".as_bytes()), [alt('é')]);
    // Alt and a control: Alt+Enter, Alt+Backspace, Alt+Ctrl+S
    assert_eq!(
        keys(b"\x1b\r\x1b\x7f\x1b\x13"),
        [
            with(Mods::ALT, Code::Enter),
            with(Mods::ALT, Code::Backspace),
            Event::Key(Key::ctrl('s').with(Mods::ALT)),
        ]
    );
    // a UTF-8 character cut by a read waits with its ESC
    let mut d = Decoder::new();
    assert_eq!(d.feed(&"\x1bé".as_bytes()[..2]), []);
    assert_eq!(d.feed(&"é".as_bytes()[1..]), [alt('é')]);
}

#[test]
fn esc_and_a_byte_that_begins_a_sequence_ending_a_read_is_alt_and_the_key() {
    // Alt+Shift+O, Alt+Shift+P, Alt+] and Alt+Shift+_, with no reply due
    for (bytes, c) in [
        (&b"\x1bO"[..], 'O'),
        (b"\x1bP", 'P'),
        (b"\x1b]", ']'),
        (b"\x1b_", '_'),
    ] {
        let mut d = Decoder::new();
        d.push(bytes);
        assert_eq!(drain(&mut d, false), [alt(c)], "{bytes:?}");
        // the next key is its own, never swallowed or read as an arrow
        d.push(b"A");
        assert_eq!(drain(&mut d, false), [ch('A')], "{bytes:?}");
    }
    // while a reply is due, each waits for what follows
    for bytes in [&b"\x1bO"[..], b"\x1bP", b"\x1b]", b"\x1b_"] {
        let mut d = Decoder::new();
        d.push(bytes);
        assert_eq!(drain(&mut d, true), [], "{bytes:?}");
    }
    // a reply's ESC after it shows it was Alt and the key
    for (bytes, c) in [
        (&b"\x1bO"[..], 'O'),
        (b"\x1bP", 'P'),
        (b"\x1b]", ']'),
        (b"\x1b_", '_'),
    ] {
        let mut d = Decoder::new();
        d.push(bytes);
        assert_eq!(drain(&mut d, true), []);
        d.push(b"\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [alt(c), DA1], "{bytes:?}");
    }
    // the queries settle with nothing after it
    let mut d = Decoder::new();
    d.push(b"\x1bO");
    assert_eq!(drain(&mut d, true), []);
    assert_eq!(drain(&mut d, false), [alt('O')]);
}

#[test]
fn an_alt_letter_matches_regardless_of_case() {
    let [Event::Key(lower), Event::Key(upper)] = keys(b"\x1bo\x1bO")[..] else {
        panic!("two keys");
    };
    for k in [lower, upper] {
        assert!(k.is_alt('o') && k.is_alt('O'), "{k:?}");
        assert_eq!(k.alt_letter(), Some('o'));
        assert!(!k.is_alt('f'));
    }
    assert_eq!(Key::char('o').alt_letter(), None);
    assert_eq!(Key::ctrl('o').with(Mods::ALT).alt_letter(), None);
}

#[test]
fn f10_and_the_function_keys_decode() {
    assert_eq!(keys(b"\x1b[21~"), [f(10)]);
    assert_eq!(
        keys(b"\x1b[12~\x1b[13~\x1b[14~\x1b[15~\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[23~\x1b[24~"),
        [2, 3, 4, 5, 6, 7, 8, 9, 11, 12].map(f)
    );
    assert_eq!(keys(b"\x1bOQ\x1bOR\x1bOS"), [f(2), f(3), f(4)]);
    // with a modifier, as xterm and vtium send them
    assert_eq!(
        keys(b"\x1b[21;2~\x1b[1;5P\x1b[15;3~"),
        [
            with(Mods::SHIFT, Code::F(10)),
            with(Mods::CTRL, Code::F(1)),
            with(Mods::ALT, Code::F(5)),
        ]
    );
}

#[test]
fn f1_decodes_in_each_form_terminals_send_it() {
    // vtium and xterm, some terminals, and the Linux console
    assert_eq!(keys(b"\x1bOP"), [f(1)]);
    assert_eq!(keys(b"\x1b[11~"), [f(1)]);
    assert_eq!(keys(b"\x1b[[A"), [f(1)]);
    // the Linux console's F2 to F5
    assert_eq!(
        keys(b"\x1b[[B\x1b[[C\x1b[[D\x1b[[E"),
        [f(2), f(3), f(4), f(5)]
    );
    // cut by a read, the console's form waits for its last byte
    let mut d = Decoder::new();
    assert_eq!(d.feed(b"\x1b[["), []);
    assert_eq!(d.feed(b"A"), [f(1)]);
    // `ESC [ [` and any other byte: `ESC [ [` is a sequence the TUI
    // drops, as ECMA-48 ends it, and the byte after it is a key
    assert_eq!(keys(b"\x1b[[Zq"), [ch('Z'), ch('q')]);
}

#[test]
fn the_control_letters_decode_as_ctrl_and_the_letter() {
    assert_eq!(
        keys(b"\x13\x1a\x19\x06"),
        [ctrl('s'), ctrl('z'), ctrl('y'), ctrl('f')]
    );
    assert_eq!(
        keys(b"\x03\x04\x15\x01"),
        [ctrl('c'), ctrl('d'), ctrl('u'), ctrl('a')]
    );
    // Tab, Enter and Backspace keep their names
    assert_eq!(
        keys(b"\t\r\n\x08\x7f"),
        [
            Code::Tab,
            Code::Enter,
            Code::Enter,
            Code::Backspace,
            Code::Backspace
        ]
        .map(key)
    );
}

#[test]
fn delete_insert_and_the_modified_arrows_decode() {
    assert_eq!(
        keys(b"\x1b[3~\x1b[2~"),
        [key(Code::Delete), key(Code::Insert)]
    );
    assert_eq!(
        keys(b"\x1b[1;5A\x1b[1;2B\x1b[1;3H\x1b[3;5~\x1b[Z"),
        [
            with(Mods::CTRL, Code::Up),
            with(Mods::SHIFT, Code::Down),
            with(Mods::ALT, Code::Home),
            with(Mods::CTRL, Code::Delete),
            with(Mods::SHIFT, Code::Tab),
        ]
    );
}

#[test]
fn a_key_named_as_the_key_table_writes_it_reads_back_in_the_bar_s_form() {
    for (name, bar, key) in [
        ("Ctrl-S", "Ctrl+S", Key::ctrl('s')),
        ("Ctrl-Z", "Ctrl+Z", Key::ctrl('z')),
        ("Alt-O", "Alt+O", Key::alt('o')),
        ("F10", "F10", Key::plain(Code::F(10))),
        ("F1", "F1", Key::plain(Code::F(1))),
        ("Delete", "Del", Key::plain(Code::Delete)),
        ("PgUp", "PgUp", Key::plain(Code::PageUp)),
        (
            "Shift-Tab",
            "Shift+Tab",
            Key::plain(Code::Tab).with(Mods::SHIFT),
        ),
        ("Space", "Space", Key::char(' ')),
        ("Esc", "Esc", Key::ESCAPE),
        ("q", "q", Key::char('q')),
        ("?", "?", Key::char('?')),
    ] {
        assert_eq!(Key::named(name), Some(key), "{name}");
        assert_eq!(key.bar_form(), bar, "{name}");
        assert_eq!(
            vtium_config_widgets::contract::bar_form(name),
            bar,
            "{name}: the contract's bar form"
        );
    }
    for not_a_key in [
        "any letter",
        "Click on a choice",
        "Up, Down",
        "Hyper-X",
        "F13",
    ] {
        assert_eq!(Key::named(not_a_key), None, "{not_a_key}");
    }
}

// The forms that must not change.

#[test]
fn keys_decode_as_a_vt100_line_terminal_sends_them() {
    assert_eq!(
        keys(b"\x1b[A\x1bOB\x1b[C\x1b[D\r\t\x7fq\x1b[5~\x1b[6~\x1b[H\x1b[4~\x1b[1~\x1b[F"),
        [
            key(Code::Up),
            key(Code::Down),
            key(Code::Right),
            key(Code::Left),
            key(Code::Enter),
            key(Code::Tab),
            key(Code::Backspace),
            ch('q'),
            key(Code::PageUp),
            key(Code::PageDown),
            key(Code::Home),
            key(Code::End),
            key(Code::Home),
            key(Code::End),
        ]
    );
    assert_eq!(keys("é".as_bytes()), [ch('é')]);
}

#[test]
fn replies_decode_with_either_ending() {
    assert_eq!(
        keys(b"\x1bP>|vtium(0.1.0)\x1b\\\x1b]50;tiny5\x07\x1b]50;unifont\x1b\\\x1b_Gi=31;OK\x1b\\\x1b_Gi=31;ENOENT:no\x1b\\\x1b[?62;4c\x1b[4;4320;7680t"),
        [
            Reply::Version("vtium(0.1.0)".into()),
            Reply::Font("tiny5".into()),
            Reply::Font("unifont".into()),
            Reply::Graphics(true),
            Reply::Graphics(false),
            Reply::Attributes,
            Reply::TextArea((7680, 4320)),
        ]
        .map(Event::Reply)
    );
}

#[test]
fn a_sequence_cut_by_a_read_waits_for_its_end() {
    let mut d = Decoder::new();
    d.push(b"\x1b]");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"50;ti");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"ny\x1b");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"\\\x1b[");
    assert_eq!(drain(&mut d, true), [font("tiny")]);
    d.push(b"B");
    assert_eq!(drain(&mut d, true), [key(Code::Down)]);
    assert_eq!(d.feed(&"é".as_bytes()[..1]), []);
    assert_eq!(d.feed(&"é".as_bytes()[1..]), [ch('é')]);
    // a key's sequence too, with no reply due
    assert_eq!(d.feed(b"\x1b[2"), []);
    assert_eq!(d.feed(b"1~"), [f(10)]);
}

#[test]
fn an_esc_that_ends_a_read_is_the_escape_key() {
    assert_eq!(keys(b"\x1b"), [ESC]);
    assert_eq!(keys(b"q\x1b"), [ch('q'), ESC]);
    // the Esc key in a read of its own, then `[` and `B` in the next
    let mut d = Decoder::new();
    assert_eq!(d.feed(b"\x1b"), [ESC]);
    assert_eq!(d.feed(b"[B"), [ch('['), ch('B')]);
}

#[test]
fn a_lone_esc_waits_for_its_next_byte_while_a_reply_is_awaited() {
    let mut d = Decoder::new();
    // the ESC begins DA1's reply, which the read cut
    d.push(b"\x1b]50;tiny5\x1b\\\x1b");
    assert_eq!(d.next(true), Some(font("tiny5")));
    assert_eq!(d.next(true), None);
    d.push(b"[?62;4c");
    assert_eq!(d.next(true), Some(DA1));
    assert_eq!(d.next(false), None);
    // the Esc key, then a reply: the ESC after it makes it the key
    d.push(b"\x1b");
    assert_eq!(d.next(true), None);
    d.push(b"\x1b]50;tiny5\x07");
    assert_eq!(drain(&mut d, true), [ESC, font("tiny5")]);
    // the Esc key, then a letter in the next read: the Esc key and the
    // letter, never Alt and the letter
    d.push(b"\x1b");
    assert_eq!(d.next(true), None);
    d.push(b"q");
    assert_eq!(drain(&mut d, true), [ESC, ch('q')]);
    // the Esc key, then `O` in the next read: no reply begins with SS3
    d.push(b"\x1b");
    assert_eq!(d.next(true), None);
    d.push(b"O");
    assert_eq!(drain(&mut d, true), [ESC, ch('O')]);
    // the queries are settled and nothing more came: the Esc key
    d.push(b"\x1b");
    assert_eq!(d.next(true), None);
    assert_eq!(drain(&mut d, false), [ESC]);
    // with no reply awaited, the Esc key at once
    d.push(b"\x1b");
    assert_eq!(d.next(false), Some(ESC));
}

#[test]
fn in_esc_esc_the_first_is_the_escape_key_and_the_second_begins_a_reply() {
    let mut d = Decoder::new();
    d.push(b"\x1b\x1b]50;tiny5\x1b\\\x1b[?62;4c");
    assert_eq!(drain(&mut d, true), [ESC, font("tiny5"), DA1]);
    d.push(b"\x1b]50;tiny5\x1b\\\x1b\x1b[?62;4c");
    assert_eq!(drain(&mut d, true), [font("tiny5"), ESC, DA1]);
    // and Esc then an arrow
    assert_eq!(keys(b"\x1b\x1b[A"), [ESC, key(Code::Up)]);
}

#[test]
fn a_lone_esc_then_a_reply_in_the_next_read_is_the_escape_key() {
    let replies: [(&[u8], Event); 3] = [
        (b"\x1b[?62;4c", DA1),
        (
            b"\x1bP>|vtium(0.1.0)\x1b\\",
            Event::Reply(Reply::Version("vtium(0.1.0)".into())),
        ),
        (b"\x1b_Gi=31;OK\x1b\\", Event::Reply(Reply::Graphics(true))),
    ];
    for (reply, heard) in replies {
        let mut d = Decoder::new();
        d.push(b"\x1b");
        assert_eq!(drain(&mut d, true), []);
        d.push(reply);
        assert_eq!(drain(&mut d, true), [ESC, heard], "{reply:?}");
    }
}

/// The Esc key, then a key whose byte could continue a reply, each in a
/// read of its own, then a reply: the Esc key and the key, as typed.
#[test]
fn a_reply_after_the_esc_key_and_a_bracket_shows_the_esc_was_the_key() {
    for typed in *b"[]_P" {
        for (reply, heard) in [
            (&b"\x1b[?62;4c"[..], vec![DA1]),
            (b"\x1b]50;tiny5\x1b\\\x1b[?62;4c", vec![font("tiny5"), DA1]),
        ] {
            let mut d = Decoder::new();
            d.push(b"\x1b");
            assert_eq!(drain(&mut d, true), []);
            d.push(&[typed]);
            assert_eq!(drain(&mut d, true), [], "{}", char::from(typed));
            d.push(reply);
            let mut want = vec![ESC, ch(char::from(typed))];
            want.extend(heard);
            assert_eq!(drain(&mut d, true), want, "{}", char::from(typed));
        }
    }
}

#[test]
fn an_esc_and_a_bracket_are_keys_once_no_reply_is_due() {
    let mut d = Decoder::new();
    d.push(b"\x1b");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"]");
    assert_eq!(drain(&mut d, true), []);
    assert_eq!(drain(&mut d, false), [ESC, ch(']')]);
}

#[test]
fn an_esc_inside_a_sequence_cancels_it() {
    assert_eq!(keys(b"\x1b]50;tin\x1b[?62;4c"), [DA1]);
    assert_eq!(keys(b"\x1bP>|vt\x1b]50;tiny5\x07"), [font("tiny5")]);
    assert_eq!(keys(b"\x1b_Gi=31\x1b[A"), [key(Code::Up)]);
    assert_eq!(keys(b"\x1b[1;\x1b[?62;4c"), [DA1]);
    // SS3 holds no ESC, so `ESC O` before an ESC is Alt+O
    assert_eq!(keys(b"\x1bO\x1b[B"), [alt('O'), key(Code::Down)]);
    let mut d = Decoder::new();
    d.push(b"\x1b]50;tiny5\x1b");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"\\");
    assert_eq!(drain(&mut d, true), [font("tiny5")]);
}

#[test]
fn mouse_reports_decode_in_both_encodings() {
    let at = |button, col, row| Event::Mouse(Mouse { button, col, row });
    assert_eq!(
        keys(b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<2;1;1M\x1b[<64;3;4M\x1b[<65;3;4M"),
        [
            at(Button::Press(0), 9, 4),
            at(Button::Release, 9, 4),
            at(Button::Press(2), 0, 0),
            at(Button::WheelUp, 2, 3),
            at(Button::WheelDown, 2, 3),
        ]
    );
    assert_eq!(keys(b"\x1b[<32;2;2M"), []);
    assert_eq!(
        keys(b"\x1b[M #%\x1b[M##%"),
        [at(Button::Press(0), 2, 4), at(Button::Release, 2, 4)]
    );
    let mut d = Decoder::new();
    assert_eq!(d.feed(b"\x1b[M "), []);
    assert_eq!(d.feed(b"#%"), [at(Button::Press(0), 2, 4)]);
    assert_eq!(keys(b"\x1b[<0;0;5M\x1b[<0;1;2;3M"), []);
    // an ESC among a mode 1000 report's three bytes cancels the report
    let mut d = Decoder::new();
    d.push(b"\x1b[M \x1b[?62;4c");
    assert_eq!(drain(&mut d, true), [DA1]);
}

#[test]
fn with_no_reply_due_an_unfinished_string_is_alt_and_a_key_and_the_rest_are_keys() {
    for (alt_bytes, c) in [(&b"\x1bP"[..], 'P'), (b"\x1b]", ']'), (b"\x1b_", '_')] {
        let mut d = Decoder::new();
        d.push(&[alt_bytes, b"q\x03\x1b"].concat());
        assert_eq!(
            drain(&mut d, false),
            [alt(c), ch('q'), ctrl('c'), ESC],
            "{alt_bytes:?}"
        );
    }
    // while a reply is due, the string waits for the rest of the reply
    let mut d = Decoder::new();
    d.push(b"\x1b]50;tin");
    assert_eq!(drain(&mut d, true), []);
    d.push(b"y5\x1b\\");
    assert_eq!(drain(&mut d, true), [font("tiny5")]);
}

#[test]
fn a_c0_control_inside_a_control_sequence_acts_at_once() {
    for awaiting in [false, true] {
        let mut d = Decoder::new();
        d.push(b"\x1b[");
        assert_eq!(drain(&mut d, awaiting), []);
        d.push(b"\x03");
        assert_eq!(drain(&mut d, awaiting), [ctrl('c')], "{awaiting}");
        d.push(b"A");
        assert_eq!(drain(&mut d, awaiting), [key(Code::Up)], "{awaiting}");
        let mut d = Decoder::new();
        d.push(b"\x1b[2\x131~");
        assert_eq!(drain(&mut d, awaiting), [ctrl('s'), f(10)], "{awaiting}");
    }
    assert_eq!(keys(b"\x1b[?62\x03;4c"), [ctrl('c'), DA1]);
    // inside SS3 too
    assert_eq!(keys(b"\x1bO\x06P"), [ctrl('f'), f(1)]);
}

#[test]
fn a_c0_control_after_a_guessed_esc_shows_the_esc_was_the_key() {
    let mut d = Decoder::new();
    for typed in [&b"\x1b"[..], b"["] {
        d.push(typed);
        assert_eq!(drain(&mut d, true), []);
    }
    d.push(b"\x03");
    assert_eq!(drain(&mut d, true), [ESC, ch('['), ctrl('c')]);
    d.push(b"\x1b[?62;4c");
    assert_eq!(drain(&mut d, true), [DA1]);
}

#[test]
fn an_endless_string_is_dropped_at_its_bound() {
    let mut d = Decoder::new();
    let mut long = b"\x1b]50;".to_vec();
    long.extend(std::iter::repeat_n(b'x', 4096 + 10));
    d.push(&long);
    assert_eq!(drain(&mut d, true), []);
    d.push(b"q");
    assert_eq!(drain(&mut d, true), [ch('q')]);
}

#[test]
fn other_sequences_and_controls_are_dropped() {
    assert_eq!(keys(b"\x1b[200~\x1b[?1;2$y\x00\x1c\x1b[99~\x1bOx"), []);
}

/// The terminal's bytes are untrusted input: bytes of every value, in
/// reads of every length, with and without a reply due, never panic the
/// decoder, and a lone ESC still comes out as the Esc key after them.
#[test]
fn any_bytes_in_any_reads_decode_without_a_panic() {
    // a linear congruential generator, so the test is the same each run
    let mut state: u64 = 0x5eed;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as u32
    };
    let mut d = Decoder::new();
    let mut events = 0usize;
    for _ in 0..20_000 {
        let len = next() % 16;
        // ESC, `[`, `]`, `P`, `O` and controls come often, so sequences
        // begin, nest and break
        let read: Vec<u8> = (0..len)
            .map(|_| match next() % 8 {
                0 => 0x1b,
                1 | 2 => b"[]PO_\\~;M<?0123456789"[(next() % 21) as usize],
                3 => (next() % 0x20) as u8,
                _ => (next() % 256) as u8,
            })
            .collect();
        d.push(&read);
        events += drain(&mut d, next() % 2 == 0).len();
    }
    events += drain(&mut d, false).len();
    assert!(events > 0);
    // whatever is held, a string's end and a lone ESC clear the decoder
    d.push(b"\x1b\\\x1b[A");
    drain(&mut d, false);
    assert_eq!(d.feed(b"\x1b"), [ESC]);
}
