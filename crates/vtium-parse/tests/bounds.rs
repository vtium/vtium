//! Every bound the parser enforces, at its edge, with the C reference's
//! behaviour past it.

mod common;

use common::{Act, Bel, St, VALUE_MAX, csi, osc, parse};
use vtium_parse::{Intermediates as I, OSC_MAX, PARAMS_MAX, Parser, SUBPARAMS_MAX};

fn csi_with(params: &str, final_byte: char) -> Vec<u8> {
    format!("\x1b[{params}{final_byte}").into_bytes()
}

// tsm-vte.c:1283, 2542-2543: 16 parameters are kept.
#[test]
fn sixteen_params_kept() {
    let list: Vec<String> = (1..=PARAMS_MAX).map(|i| i.to_string()).collect();
    let want: Vec<i32> = (1..=16).collect();
    assert_eq!(
        parse(&csi_with(&list.join(";"), 'm')),
        [csi(&want, I::NONE, b'm')]
    );
}

// tsm-vte.c:1283, 1293-1294, 2542: the 17th and later are dropped, and the
// sequence still dispatches with 16.
#[test]
fn params_past_sixteen_dropped() {
    let list: Vec<String> = (1..=40).map(|i| i.to_string()).collect();
    let want: Vec<i32> = (1..=16).collect();
    assert_eq!(
        parse(&csi_with(&list.join(";"), 'm')),
        [csi(&want, I::NONE, b'm')]
    );
}

// ECMA-48 5.4.2 g: sixteen empty parameters before separators are all
// omitted, and the ones past them are dropped.
#[test]
fn sixteenth_param_omitted_before_separator() {
    let list = ";".repeat(16);
    assert_eq!(
        parse(&csi_with(&list, 'm')),
        [csi(&[-1; 16], I::NONE, b'm')]
    );
}

// tsm-vte.c:1314-1316: a value takes digits until it is past 0xffff.
#[test]
fn value_stops_past_0xffff() {
    assert_eq!(parse(b"\x1b[65535m"), [csi(&[65535], I::NONE, b'm')]);
    assert_eq!(parse(b"\x1b[655359m"), [csi(&[VALUE_MAX], I::NONE, b'm')]);
    assert_eq!(
        parse(b"\x1b[6553599999m"),
        [csi(&[VALUE_MAX], I::NONE, b'm')]
    );
    assert_eq!(parse(b"\x1b[65536m"), [csi(&[65536], I::NONE, b'm')]);
    assert_eq!(parse(b"\x1b[655369m"), [csi(&[65536], I::NONE, b'm')]);
    assert_eq!(parse(b"\x1b[99999999m"), [csi(&[99999], I::NONE, b'm')]);
}

// tsm-vte.c:1314-1316 applies to sub-parameters too.
#[test]
fn subparam_value_stops_past_0xffff() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[4:99999999m");
    assert_eq!(rec.subs, [vec![vec![99999]]]);
}

// tsm-vte.c:1300-1301: 8 sub-parameters are kept.
#[test]
fn eight_subparams_kept() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[1:1:2:3:4:5:6:7:8m");
    assert_eq!(rec.subs, [vec![vec![1, 2, 3, 4, 5, 6, 7, 8]]]);
    assert_eq!(SUBPARAMS_MAX, 8);
}

// tsm-vte.c:1302-1303, 1307-1308, 1289: past 8, the colon and every digit up
// to the next ';' are dropped, and the sequence still dispatches.
#[test]
fn subparams_past_eight_dropped_until_separator() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[1:1:2:3:4:5:6:7:8:9:10;5:6m");
    assert_eq!(rec.acts, [csi(&[1, 5], I::NONE, b'm')]);
    assert_eq!(rec.subs, [vec![vec![1, 2, 3, 4, 5, 6, 7, 8], vec![6]]]);
}

// tsm-vte.c:1293-1294: the 17th parameter's sub-parameters are dropped with
// it.
#[test]
fn subparams_of_dropped_param_dropped() {
    let mut list = "0;".repeat(16);
    list.push_str("1:2");
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, &csi_with(&list, 'm'));
    assert_eq!(rec.acts, [csi(&[0; 16], I::NONE, b'm')]);
    assert!(rec.subs[0].iter().all(Vec::is_empty));
}

// tsm-vte.c:1232-1275: intermediates are flags, so any number of them takes
// no room.
#[test]
fn many_intermediates_collapse() {
    let mut seq = b"\x1b[".to_vec();
    seq.extend_from_slice(&b"!".repeat(10_000));
    seq.extend_from_slice(&b"%".repeat(10_000));
    seq.push(b'p');
    assert_eq!(parse(&seq), [csi(&[-1], I::BANG | I::OTHER, b'p')]);
}

// tsm-vte-int.h:31, tsm-vte.c:2915: an OSC string keeps 127 bytes.
#[test]
fn osc_keeps_127_bytes() {
    let body = vec![b'a'; OSC_MAX];
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&body);
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc(&body, Bel)]);
    assert_eq!(OSC_MAX, 127);
}

// tsm-vte.c:2915-2917: the bytes past 127 are dropped, and the string still
// dispatches.
#[test]
fn osc_past_127_dropped() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&[b'a'; OSC_MAX]);
    seq.extend_from_slice(b"bcdef");
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc(&[b'a'; OSC_MAX], Bel)]);
}

// Departure, decided by the operator: the kept bytes are always a prefix.
// Once a character fails to fit, nothing after it is kept, where the C
// reference kept a shorter character after it (tsm-vte.c:2915-2917).
#[test]
fn osc_keeps_a_prefix_once_a_character_does_not_fit() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&[b'a'; OSC_MAX - 2]);
    seq.extend_from_slice("€".as_bytes());
    seq.extend_from_slice(b"bc");
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc(&[b'a'; OSC_MAX - 2], Bel)]);
}

// A megabyte of OSC takes no more than the bound.
#[test]
fn osc_flood_bounded() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend(std::iter::repeat_n(b'x', 1 << 20));
    seq.extend_from_slice(b"\x1b\\");
    let acts = parse(&seq);
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[0], osc(&[b'x'; OSC_MAX], St));
}

// tsm-vte.c:3142-3145, tsm-vte-image.c:117-131: DCS data is streamed, not
// kept, so a megabyte of it all reaches the consumer.
#[test]
fn dcs_streams_unbounded_data() {
    let mut seq = b"\x1bPq".to_vec();
    seq.extend(std::iter::repeat_n(b'~', 1 << 20));
    seq.extend_from_slice(b"\x1b\\");
    let acts = parse(&seq);
    let n = acts.iter().filter(|a| **a == Act::DcsByte(b'~')).count();
    assert_eq!(n, 1 << 20);
}

// tsm-vte.c:3622-3638: APC data is streamed, so a megabyte of a kitty
// payload all reaches the consumer.
#[test]
fn apc_streams_unbounded_data() {
    let mut seq = b"\x1b_Ga=T;".to_vec();
    seq.extend(std::iter::repeat_n(b'A', 1 << 20));
    seq.extend_from_slice(b"\x1b\\");
    let acts = parse(&seq);
    let n = acts.iter().filter(|a| **a == Act::ApcByte(b'A')).count();
    assert_eq!(n, 1 << 20);
}

// The parser's buffers are inline and fixed; this catches one that grows.
#[test]
fn parser_size_is_fixed() {
    let size = std::mem::size_of::<Parser>();
    assert!(size <= 2048, "Parser is {size} bytes");
}
