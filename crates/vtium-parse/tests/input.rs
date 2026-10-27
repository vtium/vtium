//! Input decoding: the UTF-8 decoder in front of the state machine, the
//! 7-bit and 8-bit modes, and pausing (tsm-vte.c:3646-3687).

mod common;

use common::{Act, Recorder, St, csi, esc, parse, parse_in, prints};
use vtium_parse::{InputMode, Parser};

// tsm-vte.c:3667-3681: a character cut across inputs decodes whole.
#[test]
fn utf8_split_across_inputs() {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    for piece in [&b"\xf0\x9f"[..], b"\x98", b"\x80\xe2", b"\x82\xac"] {
        rec.feed(&mut parser, piece);
    }
    assert_eq!(rec.acts, prints("😀€"));
}

// The decoder's contract, point 3: the byte that ends an ill-formed
// sequence is processed again, so an ESC after a truncated sequence still
// starts a sequence.
#[test]
fn malformed_utf8_keeps_the_breaking_byte() {
    assert_eq!(parse(b"\xe2\x82A"), [Act::Print(0xfffd), Act::Print(0x41)]);
    assert_eq!(
        parse(b"\xe2\x1b[1mB"),
        [Act::Print(0xfffd), csi(&[1], "", b'm'), Act::Print(0x42)]
    );
}

// The decoder's contract, point 2: a byte that can neither start nor
// continue a sequence prints one U+FFFD.
#[test]
fn stray_and_invalid_bytes() {
    assert_eq!(
        parse(b"a\x80b\xffc"),
        acts![
            prints("a"),
            Act::Print(0xfffd),
            prints("b"),
            Act::Print(0xfffd),
            prints("c")
        ]
    );
}

// The decoder's contract, point 4: an overlong form of ESC is ill-formed and
// never starts a sequence, whatever its length.
#[test]
fn overlong_esc_never_starts_a_sequence() {
    let r = Act::Print(0xfffd);
    assert_eq!(
        parse(b"\xc0\x9b[1m"),
        acts![vec![r.clone(); 2], prints("[1m")]
    );
    assert_eq!(
        parse(b"\xe0\x80\x9b[1m"),
        acts![vec![r.clone(); 3], prints("[1m")]
    );
    assert_eq!(
        parse(b"\xf0\x80\x80\x9b[1m"),
        acts![vec![r; 4], prints("[1m")]
    );
}

// The decoder's contract, point 4: a surrogate and a value past U+10FFFF
// are ill-formed, one U+FFFD for each maximal subpart.
#[test]
fn surrogates_and_values_past_unicode_are_replaced() {
    assert_eq!(
        parse(b"\xed\xa0\x80\xf7\xbf\xbf\xbf"),
        vec![Act::Print(0xfffd); 7]
    );
}

// The decoder's contract, point 3, inside a string: the ESC after a
// truncated sequence still ends the APC.
#[test]
fn malformed_utf8_inside_a_string_keeps_the_esc() {
    assert_eq!(
        parse(b"\x1b_G\xe2\x1b\\"),
        [
            Act::ApcStart,
            Act::ApcByte(b'G'),
            Act::ApcChar(0xfffd),
            Act::ApcEnd(St),
            esc("", b'\\'),
        ]
    );
}

// tsm-vte.c:3665-3666: in 8-bit mode a byte is the character of its value.
// covers: text.c1
#[test]
fn eight_bit_mode() {
    assert_eq!(
        parse_in(InputMode::EightBit, b"\xe9\x85\x9d0;x\x9c"),
        [
            Act::Print(0xe9),
            Act::Execute(0x85),
            common::osc(b"0;x", St),
            Act::Execute(0x9c),
        ]
    );
}

// tsm-vte.c:3660-3664: in 7-bit mode a byte loses its top bit.
// covers: text.c1
#[test]
fn seven_bit_mode() {
    assert_eq!(
        parse_in(InputMode::SevenBit, b"\xc1\x9b[1m\x9b7\xe9"),
        [
            Act::Print(0x41),
            csi(&[1], "", b'm'),
            esc("", b'7'),
            Act::Print(0x69),
        ]
    );
}

// tsm-vte.c:1784-1823 with 3660-3681: a mode the consumer sets in a dispatch
// applies from the next byte, inside the same input.
#[test]
fn mode_change_applies_from_the_next_byte() {
    let mut parser = Parser::new();
    let mut rec = Recorder {
        follow_decscl: true,
        ..Recorder::new()
    };
    rec.feed(&mut parser, b"\x1b[62\"p\x9b1m\x1b[!p\xc3\xa9");
    assert_eq!(
        rec.acts,
        [
            csi(&[62], "\"", b'p'),
            csi(&[1], "", b'm'),
            csi(&[-1], "!", b'p'),
            Act::Print(0xe9),
        ]
    );
}

// tsm-vte-image.c:376-398, 414-434: the run fast path applies in every mode,
// and gives what the character path gives.
#[test]
fn string_runs_in_each_mode() {
    for mode in [InputMode::Utf8, InputMode::EightBit, InputMode::SevenBit] {
        let input = b"\x1bPqab\r\x1b\\\x1b_Gcd\x1b\\";
        let whole = parse_in(mode, input);
        let mut parser = Parser::new();
        let mut rec = Recorder {
            mode,
            ..Recorder::new()
        };
        for b in input.chunks(1) {
            rec.feed(&mut parser, b);
        }
        assert_eq!(rec.acts, whole, "{mode:?}");
        assert!(whole.contains(&Act::DcsByte(b'\r')), "{mode:?}");
        assert!(whole.contains(&Act::ApcByte(b'd')), "{mode:?}");
    }
}

// tsm-vte.c:3682-3684: advance stops after the byte at which the consumer
// pauses, and returns how many it took; the rest parses later, in order.
#[test]
fn pause_returns_early_and_resumes() {
    let input = b"\x1b_Gi=1\x1b\\after";
    let mut parser = Parser::new();
    let mut rec = Recorder {
        pause_on: Some(|a| matches!(a, Act::ApcEnd(_))),
        ..Recorder::new()
    };
    let n = parser.advance(&mut rec, input);
    assert_eq!(n, 8, "through the backslash of the ST that ended the APC");
    rec.feed(&mut parser, &input[n..]);
    assert_eq!(rec.acts, parse(input));
    assert_eq!(rec.acts[rec.acts.len() - 6], esc("", b'\\'));
}

// Without a pause, advance takes the whole input.
#[test]
fn advance_takes_everything_unpaused() {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    assert_eq!(parser.advance(&mut rec, b"\x1b[1mabc\x1b]x"), 10);
    assert_eq!(parser.advance(&mut rec, b""), 0);
}

// Through the parser, every pair of bytes from 0x80 up prints and nothing
// else, so no ill-formed input executes a control or starts a sequence
// (the decoder's contract, point 4), and the well-formed C1 controls C2 80
// to C2 9F do nothing at all (a departure, decided by the operator).
#[test]
fn no_pair_from_0x80_up_reaches_the_state_machine_as_a_control() {
    for a in 0x80..=0xffu8 {
        for b in 0x80..=0xffu8 {
            let acts = parse(&[a, b, b'A']);
            assert!(
                acts.iter().all(|act| matches!(act, Act::Print(_))),
                "[{a:#x}, {b:#x}] gave {acts:?}"
            );
            assert_eq!(acts.last(), Some(&Act::Print(0x41)), "[{a:#x}, {b:#x}]");
            if a == 0xc2 && b <= 0x9f {
                assert_eq!(acts, [Act::Print(0x41)], "[{a:#x}, {b:#x}]");
            }
        }
    }
}

// The decoder's contract, point 3, inside a DCS or APC string: the byte
// that breaks a sequence comes after its U+FFFD. The run fast path takes
// plain bytes only while the decoder is between characters, so the `a`
// after a truncated E2 goes through the decoder, not straight into a run.
#[test]
fn run_byte_after_a_truncated_sequence_follows_its_replacement() {
    assert_eq!(
        parse(b"\x1bPq\xe2a\x1b\\"),
        acts![
            common::hook(&[-1], "", u32::from(b'q')),
            Act::DcsChar(0xfffd),
            Act::DcsByte(b'a'),
            Act::DcsUnhook(St),
            esc("", b'\\'),
        ]
    );
    assert_eq!(
        parse(b"\x1b_\xe2a\x1b\\"),
        acts![
            Act::ApcStart,
            Act::ApcChar(0xfffd),
            Act::ApcByte(b'a'),
            Act::ApcEnd(St),
            esc("", b'\\'),
        ]
    );
}

// The decoder's contract, point 2, at the fourth byte of a sequence.
#[test]
fn a_bad_fourth_byte_is_its_own_replacement() {
    assert_eq!(
        parse(b"\xf0\x90\x80\xc0A"),
        [Act::Print(0xfffd), Act::Print(0xfffd), Act::Print(0x41)]
    );
}
