//! Every bound the parser enforces, at its edge, with the C reference's
//! behaviour past it.

mod common;

use common::{Act, Bel, St, VALUE_MAX, csi, csi_subs, osc, osc_truncated, parse};
use vtium_parse::{OSC_MAX, PARAMS_MAX, Parser, SUBPARAMS_MAX};

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
        [csi(&want, "", b'm')]
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
        [csi(&want, "", b'm')]
    );
}

// ECMA-48 5.4.2 g: sixteen empty parameters before separators are all
// omitted, and the ones past them are dropped.
#[test]
fn sixteenth_param_omitted_before_separator() {
    let list = ";".repeat(16);
    assert_eq!(parse(&csi_with(&list, 'm')), [csi(&[-1; 16], "", b'm')]);
}

// tsm-vte.c:1314-1316: a value takes digits until it is past 0xffff.
#[test]
fn value_stops_past_0xffff() {
    assert_eq!(parse(b"\x1b[65535m"), [csi(&[65535], "", b'm')]);
    assert_eq!(parse(b"\x1b[655359m"), [csi(&[VALUE_MAX], "", b'm')]);
    assert_eq!(parse(b"\x1b[6553599999m"), [csi(&[VALUE_MAX], "", b'm')]);
    assert_eq!(parse(b"\x1b[65536m"), [csi(&[65536], "", b'm')]);
    assert_eq!(parse(b"\x1b[655369m"), [csi(&[65536], "", b'm')]);
    assert_eq!(parse(b"\x1b[99999999m"), [csi(&[99999], "", b'm')]);
}

// tsm-vte.c:1314-1316 applies to sub-parameters too.
#[test]
fn subparam_value_stops_past_0xffff() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[4:99999999m");
    assert_eq!(rec.acts, [csi_subs(&[4], "", b'm', &[(0, &[99999])])]);
}

// tsm-vte.c:1300-1301: 8 sub-parameters are kept.
#[test]
fn eight_subparams_kept() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[1:1:2:3:4:5:6:7:8m");
    assert_eq!(
        rec.acts,
        [csi_subs(&[1], "", b'm', &[(0, &[1, 2, 3, 4, 5, 6, 7, 8])])]
    );
    assert_eq!(SUBPARAMS_MAX, 8);
}

// tsm-vte.c:1302-1303, 1307-1308, 1289: past 8, the colon and every digit up
// to the next ';' are dropped, and the sequence still dispatches.
#[test]
fn subparams_past_eight_dropped_until_separator() {
    let mut parser = Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b[1:1:2:3:4:5:6:7:8:9:10;5:6m");
    assert_eq!(
        rec.acts,
        [csi_subs(
            &[1, 5],
            "",
            b'm',
            &[(0, &[1, 2, 3, 4, 5, 6, 7, 8]), (1, &[6])]
        )]
    );
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
    assert_eq!(rec.acts, [csi(&[0; 16], "", b'm')]);
}

// vt100.net: past two intermediates the sequence does nothing, however
// many follow, and they take no room.
#[test]
fn many_intermediates_take_no_room() {
    let mut seq = b"\x1b[".to_vec();
    seq.extend_from_slice(&b"!".repeat(10_000));
    seq.extend_from_slice(&b"%".repeat(10_000));
    seq.push(b'p');
    seq.extend_from_slice(b"\x1b[!%p");
    assert_eq!(parse(&seq), [csi(&[-1], "!%", b'p')]);
}

// The OSC bound (src/bounds.rs, OSC_MAX): a string of exactly OSC_MAX
// bytes is kept whole and not truncated.
#[test]
fn osc_keeps_its_bound() {
    let body = vec![b'a'; OSC_MAX];
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&body);
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc(&body, Bel)]);
}

// One byte past the bound is dropped, and the string still dispatches,
// marked truncated.
#[test]
fn osc_one_past_its_bound_is_truncated() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&vec![b'a'; OSC_MAX]);
    seq.extend_from_slice(b"b");
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc_truncated(&vec![b'a'; OSC_MAX], Bel)]);
}

// The bound is 1 MiB, tmux's `input-buffer-size` (src/bounds.rs).
#[test]
fn osc_bound_is_a_mebibyte() {
    assert_eq!(OSC_MAX, 1 << 20);
}

// Departure, decided by the operator: the kept bytes are always a prefix.
// Once a character fails to fit, nothing after it is kept, where the C
// reference kept a shorter character after it (tsm-vte.c:2915-2917).
#[test]
fn osc_keeps_a_prefix_once_a_character_does_not_fit() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend_from_slice(&vec![b'a'; OSC_MAX - 2]);
    seq.extend_from_slice("€".as_bytes());
    seq.extend_from_slice(b"bc");
    seq.push(0x07);
    assert_eq!(parse(&seq), [osc_truncated(&vec![b'a'; OSC_MAX - 2], Bel)]);
}

// An OSC string after a truncated one starts empty and unflagged.
#[test]
fn next_osc_starts_clean() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend(std::iter::repeat_n(b'a', OSC_MAX + 100));
    seq.push(0x07);
    seq.extend_from_slice(b"\x1b]0;x\x07");
    let acts = parse(&seq);
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[1], osc(b"0;x", Bel));
}

// Four times the bound of OSC takes no more than the bound.
#[test]
fn osc_flood_bounded() {
    let mut seq = b"\x1b]".to_vec();
    seq.extend(std::iter::repeat_n(b'x', 4 * OSC_MAX));
    seq.extend_from_slice(b"\x1b\\");
    let acts = parse(&seq);
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[0], osc_truncated(&vec![b'x'; OSC_MAX], St));
}

/// The OSC strings at the bound: exactly `OSC_MAX` bytes, one byte past
/// it, and a three-byte character across it, each with what it gives.
fn strings_at_the_bound() -> Vec<(Vec<u8>, Act)> {
    let mut v = Vec::new();
    for (body, tail, kept, truncated) in [
        (OSC_MAX, &b""[..], OSC_MAX, false),
        (OSC_MAX, b"b", OSC_MAX, true),
        (OSC_MAX - 1, "€".as_bytes(), OSC_MAX - 1, true),
    ] {
        let mut seq = b"\x1b]".to_vec();
        seq.extend(std::iter::repeat_n(b'a', body));
        seq.extend_from_slice(tail);
        seq.extend_from_slice(b"\x1b\\");
        let data = vec![b'a'; kept];
        let act = if truncated {
            osc_truncated(&data, St)
        } else {
            osc(&data, St)
        };
        v.push((seq, act));
    }
    v
}

// docs/architecture.md, "Determinism": a string at the bound, cut into two
// input calls at every offset where the parser's state changes (its
// opening, each byte around the bound, and its ST), gives the actions it
// gives whole. The bytes between are one state, `a` collected.
#[test]
fn osc_at_its_bound_split_at_every_offset() {
    for (seq, want) in strings_at_the_bound() {
        let n = seq.len();
        let around = OSC_MAX + 2;
        let cuts = (0..=8).chain(around - 8..=around + 8).chain(n - 8..=n);
        for cut in cuts.filter(|&c| c <= n) {
            let (a, b) = seq.split_at(cut);
            let mut parser = Parser::new();
            let mut rec = common::Recorder::new();
            rec.feed(&mut parser, a);
            rec.feed(&mut parser, b);
            assert_eq!(rec.acts[0], want, "cut at {cut} of {n}");
            assert_eq!(rec.acts.len(), 2, "cut at {cut} of {n}");
        }
    }
}

// The bound holds however the input is cut: in pieces of 1, 3 and 4096
// bytes the strings give the actions they give whole.
#[test]
fn osc_at_its_bound_in_pieces() {
    for (seq, want) in strings_at_the_bound() {
        for size in [1, 3, 4096] {
            let mut parser = Parser::new();
            let mut rec = common::Recorder::new();
            for piece in seq.chunks(size) {
                rec.feed(&mut parser, piece);
            }
            assert_eq!(rec.acts[0], want, "pieces of {size}");
        }
    }
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

// The parser's buffers but the OSC string's are inline and fixed; this
// catches one that grows.
#[test]
fn parser_size_is_fixed() {
    let size = std::mem::size_of::<Parser>();
    assert!(size <= 2048, "Parser is {size} bytes");
    #[cfg(target_pointer_width = "64")]
    assert_eq!(size, 1704, "Parser is {size} bytes");
}
