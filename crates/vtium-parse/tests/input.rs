//! Input decoding: the UTF-8 decoder in front of the state machine, the
//! 7-bit and 8-bit modes, and pausing (tsm-vte.c:3646-3680).

mod common;

use common::{Act, Recorder, csi, esc, parse, parse_in, prints};
use vtium_parse::{InputMode, Intermediates as I, Parser};

// tsm-vte.c:3668-3673: a character cut across inputs decodes whole.
#[test]
fn utf8_split_across_inputs() {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    for piece in [&b"\xf0\x9f"[..], b"\x98", b"\x80\xe2", b"\x82\xac"] {
        rec.feed(&mut parser, piece);
    }
    assert_eq!(rec.acts, prints("😀€"));
}

// tsm-unicode.c:775-795: a truncated sequence prints U+FFFD and takes the
// byte that broke it with it, even an ESC.
#[test]
fn malformed_utf8_replaces_and_consumes() {
    assert_eq!(parse(b"\xe2\x82A"), [Act::Print(0xfffd)]);
    assert_eq!(
        parse(b"\xe2\x1b[1mB"),
        acts![Act::Print(0xfffd), prints("[1mB")]
    );
}

// tsm-unicode.c:755-757, 769-772: a stray continuation byte vanishes, and
// 0xF8 to 0xFF print U+FFFD.
#[test]
fn stray_and_invalid_bytes() {
    assert_eq!(
        parse(b"a\x80b\xffc"),
        acts![prints("ab"), Act::Print(0xfffd), prints("c")]
    );
}

// tsm-unicode.c:742,748: an overlong encoding of ESC is ESC to the state
// machine, so it starts a sequence.
#[test]
fn overlong_esc_starts_a_sequence() {
    assert_eq!(parse(b"\xe0\x80\x9b[1m"), [csi(&[1], I::NONE, b'm')]);
    assert_eq!(parse(b"\xc0\x9b[1m"), [csi(&[1], I::NONE, b'm')]);
}

// tsm-unicode.c has no range check: a surrogate and a value past U+10FFFF
// print as they decode.
#[test]
fn surrogates_and_values_past_unicode_print() {
    assert_eq!(
        parse(b"\xed\xa0\x80\xf7\xbf\xbf\xbf"),
        [Act::Print(0xd800), Act::Print(0x1f_ffff)]
    );
}

// tsm-vte.c:3665-3666: in 8-bit mode a byte is the character of its value.
#[test]
fn eight_bit_mode() {
    assert_eq!(
        parse_in(InputMode::EightBit, b"\xe9\x85\x9d0;x\x9c"),
        [
            Act::Print(0xe9),
            Act::Execute(0x85),
            common::osc(b"0;x", 0x9c),
            Act::Execute(0x9c),
        ]
    );
}

// tsm-vte.c:3660-3664: in 7-bit mode a byte loses its top bit.
#[test]
fn seven_bit_mode() {
    assert_eq!(
        parse_in(InputMode::SevenBit, b"\xc1\x9b[1m\x9b7\xe9"),
        [
            Act::Print(0x41),
            csi(&[1], I::NONE, b'm'),
            esc(I::NONE, b'7'),
            Act::Print(0x69),
        ]
    );
}

// tsm-vte.c:1784-1823 with 3660-3674: a mode the consumer sets in a dispatch
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
            csi(&[62], I::DQUOTE, b'p'),
            csi(&[1], I::NONE, b'm'),
            csi(&[-1], I::BANG, b'p'),
            Act::Print(0xe9),
        ]
    );
}

// tsm-vte-image.c:376-397, 413-432: the run fast path applies in every mode,
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

// tsm-vte.c:3675-3677: advance stops after the byte at which the consumer
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
    assert_eq!(n, 7, "through the ESC that ended the APC");
    rec.feed(&mut parser, &input[n..]);
    assert_eq!(rec.acts, parse(input));
    assert_eq!(rec.acts[rec.acts.len() - 6], esc(I::NONE, b'\\'));
}

// Without a pause, advance takes the whole input.
#[test]
fn advance_takes_everything_unpaused() {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    assert_eq!(parser.advance(&mut rec, b"\x1b[1mabc\x1b]x"), 10);
    assert_eq!(parser.advance(&mut rec, b""), 0);
}
