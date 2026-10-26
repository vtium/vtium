//! XTGETTCAP collection, transcribed from tsm-vte.c:3128-3170 and
//! 3282-3287 and from the test_vte_xtgettcap_* cases of libtsm's
//! test_vte.c.

mod common;

use common::{Act, Recorder, csi, esc, parse, parse8, tcap};
use vtium_parse::{Intermediates as I, Parser, XTGETTCAP_MAX};

// test_vte_xtgettcap_found: ESC \ ends the request, which is answered at the
// '\', before the ESC \ itself dispatches.
#[test]
fn ended_by_esc_backslash() {
    assert_eq!(
        parse(b"\x1bP+q544e\x1b\\"),
        [tcap(b"544e"), esc(I::NONE, b'\\')]
    );
}

// test_vte_xtgettcap_end: ST as U+009C answers it.
#[test]
fn ended_by_c1_st() {
    assert_eq!(
        parse8(b"\x1bP+q544e\x9c"),
        [tcap(b"544e"), Act::Execute(0x9c)]
    );
}

// test_vte_xtgettcap_end: CAN and SUB drop it.
#[test]
fn dropped_by_can_and_sub() {
    assert_eq!(parse(b"\x1bP+q544e\x18"), [Act::Execute(0x18)]);
    assert_eq!(parse(b"\x1bP+q544e\x1a"), [Act::Execute(0x1a)]);
}

// test_vte_xtgettcap_end: an ESC not followed by '\' drops it, and the
// sequence after the ESC runs.
#[test]
fn dropped_by_esc_without_backslash() {
    assert_eq!(parse(b"\x1bP+q544e\x1b[2C"), [csi(&[2], I::NONE, b'C')]);
}

// test_vte_xtgettcap_end: the ESC of a dropped request is not left waiting
// for a '\'.
#[test]
fn esc_esc_backslash_drops() {
    assert_eq!(parse(b"\x1bP+q544e\x1b\x1b\\"), [esc(I::NONE, b'\\')]);
}

// tsm-vte.c:3291-3298, 3164-3168: another C1 control drops it.
#[test]
fn dropped_by_other_c1() {
    assert_eq!(parse8(b"\x1bP+q544e\x85"), [Act::Execute(0x85)]);
}

// test_vte_xtgettcap_end: DCS + q with a parameter, a marker or another
// intermediate is not XTGETTCAP, and goes to the consumer.
#[test]
fn not_xtgettcap_with_param_or_other_flags() {
    let q = u32::from(b'q');
    let hooked = [
        (
            &b"\x1bP1+q544e\x1b\\"[..],
            Act::DcsHook(vec![1], I::PLUS, q),
        ),
        (
            b"\x1bP>+q544e\x1b\\",
            Act::DcsHook(vec![-1], I::GREATER | I::PLUS, q),
        ),
        (
            b"\x1bP+$q544e\x1b\\",
            Act::DcsHook(vec![-1], I::PLUS | I::DOLLAR, q),
        ),
        (
            b"\x1bP;+q544e\x1b\\",
            Act::DcsHook(vec![-1, -1], I::PLUS, q),
        ),
    ];
    for (seq, hook) in hooked {
        let acts = parse(seq);
        assert_eq!(acts[0], hook, "{seq:?}");
        assert!(
            !acts.iter().any(|a| matches!(a, Act::Xtgettcap(..))),
            "{seq:?}"
        );
    }
}

// test_vte_xtgettcap_invalid: the parser keeps the payload as sent, controls
// included, and a character past ASCII as 0 (tsm-vte.c:3149-3150).
#[test]
fn payload_as_sent() {
    assert_eq!(parse(b"\x1bP+q\x1b\\")[0], tcap(b""));
    assert_eq!(parse(b"\x1bP+q544e\r\x1b\\")[0], tcap(b"544e\r"));
    assert_eq!(parse(b"\x1bP+q54\xc3\xa94e\x1b\\")[0], tcap(b"54\x004e"));
}

// tsm-vte.c:3567-3568: DEL is dropped from the payload.
#[test]
fn payload_drops_del() {
    assert_eq!(parse(b"\x1bP+q54\x7f4e\x1b\\")[0], tcap(b"544e"));
}

fn request(payload_len: usize) -> Vec<u8> {
    let mut seq = b"\x1bP+q".to_vec();
    seq.extend((0..payload_len).map(|i| b"544e;"[i % 5]));
    seq.extend_from_slice(b"\x1b\\");
    seq
}

// test_vte_xtgettcap_bounds: a 1039-byte payload is kept whole.
#[test]
fn payload_of_1039_kept() {
    let seq = request(XTGETTCAP_MAX);
    let acts = parse(&seq);
    assert_eq!(
        acts[0],
        Act::Xtgettcap(seq[4..4 + XTGETTCAP_MAX].to_vec(), false)
    );
    assert_eq!(XTGETTCAP_MAX, 1039);
}

// test_vte_xtgettcap_bounds: one byte more is flagged, and the payload holds
// its first 1039 bytes (tsm-vte.c:3149-3152).
#[test]
fn payload_past_1039_flagged() {
    let seq = request(XTGETTCAP_MAX + 1);
    let acts = parse(&seq);
    assert_eq!(
        acts[0],
        Act::Xtgettcap(seq[4..4 + XTGETTCAP_MAX].to_vec(), true)
    );
}

// A megabyte of payload takes no more than the bound.
#[test]
fn payload_flood_bounded() {
    let seq = request(1 << 20);
    let acts = parse(&seq);
    assert_eq!(
        acts[0],
        Act::Xtgettcap(seq[4..4 + XTGETTCAP_MAX].to_vec(), true)
    );
}

// A request after a flagged one starts empty and unflagged
// (tsm-vte.c:3137-3138).
#[test]
fn next_request_starts_clean() {
    let mut seq = request(XTGETTCAP_MAX + 1);
    seq.extend_from_slice(b"\x1bP+q436f\x1b\\");
    let acts = parse(&seq);
    assert_eq!(acts[2], tcap(b"436f"));
}

// test_vte_xtgettcap_split: a request split at every byte.
#[test]
fn split_at_every_byte() {
    let query = b"\x1bP+q544e;436f\x1b\\";
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    for b in query.chunks(1) {
        rec.feed(&mut parser, b);
    }
    assert_eq!(rec.acts, [tcap(b"544e;436f"), esc(I::NONE, b'\\')]);
}

// test_vte_xtgettcap_esc_reset: a reset between the ESC and the '\' drops
// the request.
#[test]
fn reset_between_esc_and_backslash_drops() {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    rec.feed(&mut parser, b"\x1bP+q544e\x1b");
    parser.reset();
    rec.feed(&mut parser, b"\\");
    assert_eq!(rec.acts, [Act::Print(u32::from(b'\\'))]);
    rec.feed(&mut parser, b"\x1bP+q544e\x1b\\");
    assert_eq!(rec.acts[1], tcap(b"544e"));
}

// tsm-vte.c:3282-3287: the check for '\' takes the next character, and a
// stray UTF-8 continuation byte is one: U+FFFD, under the decoder's
// contract. It drops the request, and the ESC sequence it interrupts with
// it, so the '\' prints.
#[test]
fn stray_byte_between_esc_and_backslash_drops() {
    assert_eq!(
        parse(b"\x1bP+q544e\x1b\x80\\"),
        [Act::Print(u32::from(b'\\'))]
    );
}
