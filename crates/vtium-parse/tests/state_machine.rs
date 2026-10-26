//! Each state's transitions, transcribed from `parse_data` in the C
//! reference (tsm-vte.c:3280-3642) and from libtsm's test_vte*.c.

mod common;

use common::{Act, apc_bytes, csi, dcs_bytes, esc, osc, parse, parse_bytewise, parse_in, prints};
use vtium_parse::{InputMode, Intermediates as I};

// tsm-vte.c:3323-3338: in ground, C0 controls execute and 0x20 to 0x7E print.
#[test]
fn ground_prints_and_executes() {
    assert_eq!(
        parse(b"a\tb\r\n\x07\x00"),
        acts![
            Act::Print(0x61),
            Act::Execute(0x09),
            Act::Print(0x62),
            Act::Execute(0x0d),
            Act::Execute(0x0a),
            Act::Execute(0x07),
            Act::Execute(0x00),
        ]
    );
}

// tsm-vte.c:3333: DEL prints in ground, where vt100.net ignores it.
#[test]
fn ground_prints_del() {
    assert_eq!(parse(b"\x7f"), [Act::Print(0x7f)]);
}

// tsm-vte.c:3337: a character from U+00A0 up prints.
#[test]
fn ground_prints_past_ascii() {
    assert_eq!(parse("é€😀".as_bytes()), prints("é€😀"));
}

// tsm-vte.c:3291-3298, 3328-3330: C1 controls arriving as UTF-8 execute.
#[test]
fn c1_as_utf8_executes() {
    assert_eq!(
        parse(b"\xc2\x84\xc2\x85\xc2\x88\xc2\x8d\xc2\x9a\xc2\x9c"),
        [
            Act::Execute(0x84),
            Act::Execute(0x85),
            Act::Execute(0x88),
            Act::Execute(0x8d),
            Act::Execute(0x9a),
            Act::Execute(0x9c),
        ]
    );
}

// tsm-vte.c:3291-3298: CAN and SUB execute in every state and return to
// ground, so the sequence they interrupt is dropped.
#[test]
fn can_and_sub_cancel_a_sequence() {
    assert_eq!(
        parse(b"\x1b[1\x18m"),
        [Act::Execute(0x18), Act::Print(0x6d)]
    );
    assert_eq!(parse(b"\x1b(\x1a0"), [Act::Execute(0x1a), Act::Print(0x30)]);
}

// tsm-vte.c:3349-3350, 3358: ESC and its final byte.
#[test]
fn esc_dispatches() {
    assert_eq!(
        parse(b"\x1b7\x1bc\x1bM\x1b\\"),
        [
            esc(I::NONE, b'7'),
            esc(I::NONE, b'c'),
            esc(I::NONE, b'M'),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3349-3350, 3386-3393, 1232-1275: intermediates are collected as
// flags, and the final byte dispatches.
#[test]
fn esc_collects_intermediates() {
    assert_eq!(
        parse(b"\x1b(0\x1b)B\x1b G\x1b#8"),
        [
            esc(I::LPAREN, b'0'),
            esc(I::RPAREN, b'B'),
            esc(I::SPACE, b'G'),
            esc(I::HASH, b'8'),
        ]
    );
}

// libtsm's test_vte_esc_96_set_not_hts_ri: '-', '.' and '/' have no flag of
// their own and set CSI_OTHER, so ESC - H is not HTS.
#[test]
fn esc_unknown_intermediate_is_other() {
    assert_eq!(
        parse(b"\x1b-H\x1b.A\x1b/A"),
        [
            esc(I::OTHER, b'H'),
            esc(I::OTHER, b'A'),
            esc(I::OTHER, b'A')
        ]
    );
}

// tsm-vte.c:3377, 3396: in an ESC sequence a character from U+00A0 up is
// collected as an intermediate.
#[test]
fn esc_collects_past_ascii_as_other() {
    assert_eq!(parse(b"\x1b\xc3\xa9A"), [esc(I::OTHER, b'A')]);
}

// tsm-vte.c:3344, 3346-3347, 3384, 3390: C0 controls execute inside an ESC
// sequence and DEL is ignored.
#[test]
fn esc_executes_c0_and_ignores_del() {
    assert_eq!(
        parse(b"\x1b\n\x7f(\r\x7f0"),
        [Act::Execute(0x0a), Act::Execute(0x0d), esc(I::LPAREN, b'0')]
    );
}

// tsm-vte.c:3300-3301, 3243: ESC restarts a sequence, clearing what it had
// collected.
#[test]
fn esc_restarts_and_clears() {
    assert_eq!(parse(b"\x1b(\x1b0"), [esc(I::NONE, b'0')]);
    assert_eq!(parse(b"\x1b[1;2\x1b[m"), [csi(&[-1], I::NONE, b'm')]);
}

// tsm-vte.c:3369-3371, 3305, 3608-3621: SOS and PM are consumed to their end
// with nothing dispatched, controls inside included.
#[test]
fn sos_and_pm_are_ignored() {
    assert_eq!(
        parse(b"\x1bXab\x07\ncd\x1b\\x"),
        [esc(I::NONE, b'\\'), Act::Print(0x78)]
    );
    assert_eq!(
        parse(b"\x1b^ab\xc2\x9cx"),
        [Act::Execute(0x9c), Act::Print(0x78)]
    );
    assert_eq!(
        parse(b"\xc2\x98ab\xc2\x9cx"),
        [Act::Execute(0x9c), Act::Print(0x78)]
    );
}

// tsm-vte.c:3411-3418, 1277-1326, 2542-2543: parameters and the final byte.
#[test]
fn csi_dispatches_with_params() {
    assert_eq!(
        parse(b"\x1b[m\x1b[1m\x1b[3;5H\x1b[007d"),
        [
            csi(&[-1], I::NONE, b'm'),
            csi(&[1], I::NONE, b'm'),
            csi(&[3, 5], I::NONE, b'H'),
            csi(&[7], I::NONE, b'd'),
        ]
    );
}

// Departure (ECMA-48 5.4.2 e and g): every empty parameter is -1, wherever
// it falls. The C reference made one before a ';' 0 (tsm-vte.c:1284-1286).
#[test]
fn csi_omitted_params() {
    assert_eq!(
        parse(b"\x1b[;5H\x1b[5;H\x1b[;;r\x1b[0;5H"),
        [
            csi(&[-1, 5], I::NONE, b'H'),
            csi(&[5, -1], I::NONE, b'H'),
            csi(&[-1, -1, -1], I::NONE, b'r'),
            csi(&[0, 5], I::NONE, b'H'),
        ]
    );
}

// tsm-vte.c:3414-3415: a marker right after CSI is collected.
#[test]
fn csi_collects_markers() {
    assert_eq!(
        parse(b"\x1b[?25h\x1b[>4;1m\x1b[=c\x1b[<1p"),
        [
            csi(&[25], I::QUESTION, b'h'),
            csi(&[4, 1], I::GREATER, b'm'),
            csi(&[-1], I::OTHER, b'c'),
            csi(&[1], I::OTHER, b'p'),
        ]
    );
}

// libtsm's test_vte_csi_other_intermediates: intermediates do_collect has no
// name for set CSI_OTHER.
#[test]
fn csi_unknown_intermediates_are_other() {
    for seq in [
        &b"\x1b[1;2,p"[..],
        b"\x1b[1%p",
        b"\x1b[1&p",
        b"\x1b[1-p",
        b"\x1b[1.p",
        b"\x1b[1/p",
    ] {
        let acts = parse(seq);
        assert_eq!(acts.len(), 1, "{seq:?}");
        let Act::Csi(_, im, b'p') = &acts[0] else {
            panic!("{seq:?}: {acts:?}");
        };
        assert_eq!(*im, I::OTHER, "{seq:?}");
    }
}

// tsm-vte.c:3439-3440, 3455-3456: intermediates after the parameters, as
// flags that keep neither order nor count.
#[test]
fn csi_collects_intermediates() {
    assert_eq!(
        parse(b"\x1b[2 q\x1b[!p\x1b[1$p\x1b[61\"p\x1b[!!p\x1b[$!p"),
        [
            csi(&[2], I::SPACE, b'q'),
            csi(&[-1], I::BANG, b'p'),
            csi(&[1], I::DOLLAR, b'p'),
            csi(&[61], I::DQUOTE, b'p'),
            csi(&[-1], I::BANG, b'p'),
            csi(&[-1], I::BANG | I::DOLLAR, b'p'),
        ]
    );
}

// tsm-vte.c:3436-3437: a marker after a parameter ignores the sequence.
#[test]
fn csi_marker_after_param_ignores() {
    assert_eq!(parse(b"\x1b[1?hx"), [Act::Print(0x78)]);
    assert_eq!(parse(b"\x1b[??hx"), [Act::Print(0x78)]);
}

// tsm-vte.c:3461-3462: a parameter after an intermediate ignores the
// sequence.
#[test]
fn csi_param_after_intermediate_ignores() {
    assert_eq!(parse(b"\x1b[ 1qx"), [Act::Print(0x78)]);
}

// tsm-vte.c:3421, 3446, 3468: a character from U+00A0 up ignores the
// sequence.
#[test]
fn csi_past_ascii_ignores() {
    assert_eq!(parse(b"\x1b[\xc3\xa9mx"), [Act::Print(0x78)]);
    assert_eq!(parse(b"\x1b[1\xc3\xa9mx"), [Act::Print(0x78)]);
    assert_eq!(parse(b"\x1b[ \xc3\xa9mx"), [Act::Print(0x78)]);
}

// tsm-vte.c:3470-3486: the ignored sequence ends at its final byte with no
// dispatch; C0 controls still execute and DEL is ignored.
#[test]
fn csi_ignore_executes_c0_and_ends_at_final() {
    assert_eq!(
        parse(b"\x1b[1?2\n\x7f;3mx"),
        [Act::Execute(0x0a), Act::Print(0x78)]
    );
}

// tsm-vte.c:3403, 3405-3406, 3428, 3433-3434, 3453, 3458-3459: C0 controls
// execute inside CSI, before it dispatches, and DEL is ignored.
#[test]
fn csi_executes_c0_and_ignores_del() {
    assert_eq!(
        parse(b"\x1b[\r1\x7f;\x082 \tq"),
        [
            Act::Execute(0x0d),
            Act::Execute(0x08),
            Act::Execute(0x09),
            csi(&[1, 2], I::SPACE, b'q'),
        ]
    );
}

// tsm-vte.c:1297-1312: colon sub-parameters, each -1 when omitted.
#[test]
fn csi_subparams() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(
        &mut parser,
        b"\x1b[4:3m\x1b[38:2::255:0:0;1m\x1b[4:m\x1b[:5m",
    );
    assert_eq!(
        rec.acts,
        [
            csi(&[4], I::NONE, b'm'),
            csi(&[38, 1], I::NONE, b'm'),
            csi(&[4], I::NONE, b'm'),
            csi(&[-1], I::NONE, b'm'),
        ]
    );
    assert_eq!(
        rec.subs,
        [
            vec![vec![3]],
            vec![vec![2, -1, 255, 0, 0], vec![]],
            vec![vec![-1]],
            vec![vec![5]],
        ]
    );
}

// tsm-vte.c:3316-3317, 3360-3361: CSI as C1 (UTF-8 U+009B) and as ESC [.
#[test]
fn csi_c1_introducer() {
    assert_eq!(parse(b"\xc2\x9b1m"), [csi(&[1], I::NONE, b'm')]);
}

// libtsm's test_vte_8bit_c1: in 8-bit mode 0x9B introduces a CSI.
#[test]
fn csi_c1_in_8bit_mode() {
    assert_eq!(
        parse_in(InputMode::EightBit, b"\x9b1mB"),
        [csi(&[1], I::NONE, b'm'), Act::Print(0x42)]
    );
}

// tsm-vte.c:3363-3364, 3598-3603, 3004-3015: an OSC string ended by BEL.
#[test]
fn osc_ended_by_bel() {
    assert_eq!(parse(b"\x1b]0;title\x07"), [osc(b"0;title", 0x07)]);
}

// tsm-vte.c:3291-3298, 3250-3255: ST as C1 ends it, then executes.
#[test]
fn osc_ended_by_c1_st() {
    assert_eq!(
        parse(b"\xc2\x9d10;?\xc2\x9c"),
        [osc(b"10;?", 0x9c), Act::Execute(0x9c)]
    );
}

// tsm-vte.c:3300-3301: ESC ends it, and ESC \ then dispatches as ST.
#[test]
fn osc_ended_by_esc_backslash() {
    assert_eq!(
        parse(b"\x1b]2;x\x1b\\"),
        [osc(b"2;x", 0x1b), esc(I::NONE, b'\\')]
    );
}

// tsm-vte.c:3300-3301: an ESC ends the string whatever follows it, so the
// OSC dispatches and the ESC sequence runs.
#[test]
fn osc_ended_by_esc_alone() {
    assert_eq!(
        parse(b"\x1b]2;x\x1b[1m"),
        [osc(b"2;x", 0x1b), csi(&[1], I::NONE, b'm')]
    );
}

// tsm-vte.c:3291-3298: CAN and SUB end the string by dispatching it, where
// vt100.net cancels it, and then execute.
#[test]
fn osc_dispatched_by_can_and_sub() {
    assert_eq!(
        parse(b"\x1b]2;x\x18"),
        [osc(b"2;x", 0x18), Act::Execute(0x18)]
    );
    assert_eq!(
        parse(b"\x1b]2;x\x1a"),
        [osc(b"2;x", 0x1a), Act::Execute(0x1a)]
    );
}

// tsm-vte.c:3592-3600: C0 controls other than BEL are ignored inside, and DEL
// is collected.
#[test]
fn osc_ignores_c0_and_collects_del() {
    assert_eq!(parse(b"\x1b]a\nb\x7fc\x07"), [osc(b"ab\x7fc", 0x07)]);
}

// tsm-vte.c:3606, 2912-2921: characters past ASCII are collected as UTF-8,
// ill-formed input as U+FFFD, and the noncharacters tsm_ucs4_to_utf8 cannot
// encode are dropped.
#[test]
fn osc_collects_utf8() {
    assert_eq!(
        parse("\x1b]2;é€\x07".as_bytes()),
        [osc("2;é€".as_bytes(), 0x07)]
    );
    assert_eq!(
        parse(b"\x1b]a\xed\xa0\x80b\xef\xbf\xbfc\xef\xb7\x90d\x07"),
        [osc("a\u{fffd}\u{fffd}\u{fffd}bcd".as_bytes(), 0x07)]
    );
}

// tsm-vte.c:3313-3314, 3244: OSC as C1 restarts with an empty buffer.
#[test]
fn osc_restart_clears() {
    assert_eq!(
        parse(b"\x1b]abc\xc2\x9ddef\x07"),
        [osc(b"abc", 0x9d), osc(b"def", 0x07)]
    );
}

// tsm-vte.c:3366-3367, 3501-3509, 3128-3140, 3559-3566: DCS hooks with its
// parameters and final, passes its data and ends at ST.
#[test]
fn dcs_hook_put_unhook() {
    assert_eq!(
        parse(b"\x1bP0;1q#0;2;0~-\x1b\\"),
        acts![
            Act::DcsHook(vec![0, 1], I::NONE, u32::from(b'q')),
            dcs_bytes(b"#0;2;0~-"),
            Act::DcsUnhook(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3498-3499, 3505-3506, 3530-3531: markers and intermediates.
#[test]
fn dcs_collects_markers_and_intermediates() {
    assert_eq!(
        parse(b"\x1bP>|x\xc2\x9c\x1bP$qm\xc2\x9c"),
        acts![
            Act::DcsHook(vec![-1], I::GREATER, u32::from(b'|')),
            Act::DcsByte(b'x'),
            Act::DcsUnhook(0x9c),
            Act::Execute(0x9c),
            Act::DcsHook(vec![-1], I::DOLLAR, u32::from(b'q')),
            Act::DcsByte(b'm'),
            Act::DcsUnhook(0x9c),
            Act::Execute(0x9c),
        ]
    );
}

// tsm-vte.c:3489-3494, 3516-3521, 3541-3546: C0 controls and DEL are ignored
// before the data starts; they do not execute.
#[test]
fn dcs_ignores_c0_before_data() {
    assert_eq!(
        parse(b"\x1bP\n1\r;\x7f2\t$\x08qz\x1b\\"),
        acts![
            Act::DcsHook(vec![1, 2], I::DOLLAR, u32::from(b'q')),
            Act::DcsByte(b'z'),
            Act::DcsUnhook(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3561-3568: the data takes C0 controls and drops DEL.
#[test]
fn dcs_data_takes_c0_and_drops_del() {
    assert_eq!(
        parse(b"\x1bPqa\r\n\x7fb\x1b\\"),
        acts![
            Act::DcsHook(vec![-1], I::NONE, u32::from(b'q')),
            dcs_bytes(b"a\r\nb"),
            Act::DcsUnhook(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3574, 3142-3154: a character past ASCII in the data.
#[test]
fn dcs_data_past_ascii() {
    assert_eq!(
        parse("\x1bPqé\x1b\\".as_bytes()),
        acts![
            Act::DcsHook(vec![-1], I::NONE, u32::from(b'q')),
            Act::DcsChar(0xe9),
            Act::DcsUnhook(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3495-3496, 3526-3528, 3550-3551, 3576-3589: a ':', or a marker or
// parameter where neither may be, ignores the string to its ST.
#[test]
fn dcs_malformed_is_ignored() {
    for seq in [&b"\x1bP:q"[..], b"\x1bP1:q", b"\x1bP1?q", b"\x1bP$1q"] {
        let mut input = seq.to_vec();
        input.extend_from_slice(b"data\n\x1b\\x");
        assert_eq!(
            parse(&input),
            [esc(I::NONE, b'\\'), Act::Print(0x78)],
            "{seq:?}"
        );
    }
}

// tsm-vte.c:3512, 3537, 3557: a character from U+00A0 up ends the
// parameters, as a final byte would.
#[test]
fn dcs_final_past_ascii() {
    assert_eq!(
        parse("\x1bP1é\x1b\\".as_bytes()),
        acts![
            Act::DcsHook(vec![1], I::NONE, 0xe9),
            Act::DcsUnhook(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3291-3298, 3250-3251: CAN ends the data, then executes.
#[test]
fn dcs_ended_by_can() {
    assert_eq!(
        parse(b"\x1bPqab\x18c"),
        acts![
            Act::DcsHook(vec![-1], I::NONE, u32::from(b'q')),
            dcs_bytes(b"ab"),
            Act::DcsUnhook(0x18),
            Act::Execute(0x18),
            Act::Print(0x63),
        ]
    );
}

// tsm-vte.c:3310-3311, 3316-3317: a C1 introducer ends the data and starts
// its own sequence.
#[test]
fn dcs_ended_by_c1_csi() {
    assert_eq!(
        parse(b"\xc2\x90qa\xc2\x9b2J"),
        acts![
            Act::DcsHook(vec![-1], I::NONE, u32::from(b'q')),
            Act::DcsByte(b'a'),
            Act::DcsUnhook(0x9b),
            csi(&[2], I::NONE, b'J'),
        ]
    );
}

// tsm-vte.c:3373-3374, 3245, 3253, 3630-3631: an APC string, as the kitty
// graphics protocol sends it.
#[test]
fn apc_start_put_end() {
    assert_eq!(
        parse(b"\x1b_Ga=q;AAAA\x1b\\"),
        acts![
            Act::ApcStart,
            apc_bytes(b"Ga=q;AAAA"),
            Act::ApcEnd(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// libtsm's test_vte_kitty C1 framing: APC and ST as UTF-8, and as bytes in
// 8-bit mode.
#[test]
fn apc_c1_framing() {
    let want = acts![
        Act::ApcStart,
        apc_bytes(b"Gi=1"),
        Act::ApcEnd(0x9c),
        Act::Execute(0x9c),
    ];
    assert_eq!(parse(b"\xc2\x9fGi=1\xc2\x9c"), want);
    assert_eq!(parse_in(InputMode::EightBit, b"\x9fGi=1\x9c"), want);
}

// tsm-vte.c:3624-3629: C0 controls and DEL are dropped inside.
#[test]
fn apc_drops_c0_and_del() {
    assert_eq!(
        parse(b"\x1b_a\r\n\x7fb\x1b\\"),
        acts![
            Act::ApcStart,
            apc_bytes(b"ab"),
            Act::ApcEnd(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3637: a character past ASCII is collected as itself.
#[test]
fn apc_past_ascii() {
    assert_eq!(
        parse("\x1b_aé\x1b\\".as_bytes()),
        acts![
            Act::ApcStart,
            Act::ApcByte(b'a'),
            Act::ApcChar(0xe9),
            Act::ApcEnd(0x1b),
            esc(I::NONE, b'\\'),
        ]
    );
}

// tsm-vte.c:3307-3308, 3258-3269: APC inside APC ends the first and starts
// another, since a transition to the same state runs both actions.
#[test]
fn apc_restarts() {
    assert_eq!(
        parse(b"\xc2\x9fa\xc2\x9fb\xc2\x9c"),
        acts![
            Act::ApcStart,
            Act::ApcByte(b'a'),
            Act::ApcEnd(0x9f),
            Act::ApcStart,
            Act::ApcByte(b'b'),
            Act::ApcEnd(0x9c),
            Act::Execute(0x9c),
        ]
    );
}

// tsm-vte.c:3291-3298: SUB ends an APC, then executes.
#[test]
fn apc_ended_by_sub() {
    assert_eq!(
        parse(b"\x1b_Gx\x1a"),
        acts![
            Act::ApcStart,
            apc_bytes(b"Gx"),
            Act::ApcEnd(0x1a),
            Act::Execute(0x1a),
        ]
    );
}

// The run fast path and the character path agree: a string fed a byte at a
// time gives what it gives whole.
#[test]
fn bytewise_matches_whole() {
    let input: &[u8] = b"\x1bPqa\r\x7fb\xc3\xa9\x1b\\\x1b_Gq\nr\xc3\xa9\x1b\\\x1b]x\x07";
    let whole = parse(input);
    assert_eq!(parse_bytewise(input), whole);
    assert_eq!(whole.len(), 15);
    assert_eq!(whole[2], Act::DcsByte(b'\r'));
    assert_eq!(whole[11], Act::ApcChar(0xe9));
}

// tsm-vte.c:1043-1047: reset drops a string in progress with no end call.
#[test]
fn reset_drops_the_string_in_progress() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b]2;abc");
    parser.reset();
    rec.feed(&mut parser, b"x\x07");
    assert_eq!(rec.acts, [Act::Print(0x78), Act::Execute(0x07)]);
}

// tsm-vte.c:1043: reset leaves the UTF-8 decoder between characters.
#[test]
fn reset_drops_a_partial_character() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\xe2\x82");
    parser.reset();
    rec.feed(&mut parser, b"x");
    assert_eq!(rec.acts, [Act::Print(0x78)]);
}

// aaronsb/libtsm#53, the one deliberate departure from the C reference: a
// host reset between the ESC that ended an APC string and the `\` of its ST,
// as when the host cancels a kitty request it was handed at that ESC, drops
// the `\`. The C reference printed it in ground state.
#[test]
fn reset_after_apc_esc_drops_the_st_backslash() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b_Ga=T;AAAA\x1b");
    parser.reset();
    rec.feed(&mut parser, b"\\text");
    assert_eq!(
        rec.acts,
        acts![
            Act::ApcStart,
            apc_bytes(b"Ga=T;AAAA"),
            Act::ApcEnd(0x1b),
            prints("text"),
        ]
    );
}

// aaronsb/libtsm#53: only a `\` right after the reset is dropped; anything
// else parses as it would after any reset.
#[test]
fn reset_after_apc_esc_keeps_other_characters() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b_Gq\x1b");
    parser.reset();
    parser.reset();
    rec.feed(&mut parser, b"x\\");
    assert_eq!(
        rec.acts,
        acts![
            Act::ApcStart,
            apc_bytes(b"Gq"),
            Act::ApcEnd(0x1b),
            prints("x\\"),
        ]
    );
}

// aaronsb/libtsm#53: without a reset, ESC \ ends the APC as ST, as before.
#[test]
fn apc_esc_backslash_without_reset_is_st() {
    assert_eq!(
        parse(b"\x1b_Gq\x1b\\text"),
        acts![
            Act::ApcStart,
            apc_bytes(b"Gq"),
            Act::ApcEnd(0x1b),
            esc(I::NONE, b'\\'),
            prints("text"),
        ]
    );
}
