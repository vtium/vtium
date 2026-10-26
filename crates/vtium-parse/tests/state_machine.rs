//! Each state's transitions, transcribed from `parse_data` in the C
//! reference (tsm-vte.c:3280-3642) and from libtsm's test_vte*.c.

mod common;

use common::{
    Act, Bel, Cancelled, St, apc_bytes, csi, dcs_bytes, esc, hook, osc, parse, parse_bytewise,
    parse_in, parse8, prints,
};
use vtium_parse::InputMode;

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

// tsm-vte.c:3291-3298, 3328-3330: C1 controls execute (8-bit input).
#[test]
fn c1_executes() {
    assert_eq!(
        parse8(b"\x84\x85\x88\x8d\x9a\x9c"),
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
        [esc("", b'7'), esc("", b'c'), esc("", b'M'), esc("", b'\\'),]
    );
}

// tsm-vte.c:3349-3350, 3386-3393, 1232-1275: intermediates are collected as
// flags, and the final byte dispatches.
#[test]
fn esc_collects_intermediates() {
    assert_eq!(
        parse(b"\x1b(0\x1b)B\x1b G\x1b#8"),
        [
            esc("(", b'0'),
            esc(")", b'B'),
            esc(" ", b'G'),
            esc("#", b'8'),
        ]
    );
}

// libtsm's test_vte_esc_96_set_not_hts_ri: ESC - H is not HTS. Each
// intermediate keeps its own byte (the C reference set CSI_OTHER).
#[test]
fn esc_keeps_every_intermediate() {
    assert_eq!(
        parse(b"\x1b-H\x1b.A\x1b/A\x1b%G"),
        [
            esc("-", b'H'),
            esc(".", b'A'),
            esc("/", b'A'),
            esc("%", b'G')
        ]
    );
}

// Departure, decided by the operator: in UTF-8 mode a character from
// U+00A0 up drops the ESC sequence it interrupts. The C reference collected
// it as an intermediate (tsm-vte.c:3377, 3396).
#[test]
fn esc_dropped_by_a_character_past_ascii() {
    assert_eq!(parse(b"\x1b\xc3\xa9A"), [Act::Print(0x41)]);
    assert_eq!(parse(b"\x1b(\xc3\xa90"), [Act::Print(0x30)]);
}

// tsm-vte.c:3344, 3346-3347, 3384, 3390: C0 controls execute inside an ESC
// sequence and DEL is ignored.
#[test]
fn esc_executes_c0_and_ignores_del() {
    assert_eq!(
        parse(b"\x1b\n\x7f(\r\x7f0"),
        [Act::Execute(0x0a), Act::Execute(0x0d), esc("(", b'0')]
    );
}

// tsm-vte.c:3300-3301, 3243: ESC restarts a sequence, clearing what it had
// collected.
#[test]
fn esc_restarts_and_clears() {
    assert_eq!(parse(b"\x1b(\x1b0"), [esc("", b'0')]);
    assert_eq!(parse(b"\x1b[1;2\x1b[m"), [csi(&[-1], "", b'm')]);
}

// tsm-vte.c:3369-3371, 3305, 3608-3621: SOS and PM are consumed to their end
// with nothing dispatched, controls inside included.
#[test]
fn sos_and_pm_are_ignored() {
    assert_eq!(
        parse(b"\x1bXab\x07\ncd\x1b\\x"),
        [esc("", b'\\'), Act::Print(0x78)]
    );
    assert_eq!(
        parse8(b"\x1b^ab\x9cx"),
        [Act::Execute(0x9c), Act::Print(0x78)]
    );
    assert_eq!(
        parse8(b"\x98ab\x9cx"),
        [Act::Execute(0x9c), Act::Print(0x78)]
    );
}

// tsm-vte.c:3411-3418, 1277-1326, 2542-2543: parameters and the final byte.
#[test]
fn csi_dispatches_with_params() {
    assert_eq!(
        parse(b"\x1b[m\x1b[1m\x1b[3;5H\x1b[007d"),
        [
            csi(&[-1], "", b'm'),
            csi(&[1], "", b'm'),
            csi(&[3, 5], "", b'H'),
            csi(&[7], "", b'd'),
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
            csi(&[-1, 5], "", b'H'),
            csi(&[5, -1], "", b'H'),
            csi(&[-1, -1, -1], "", b'r'),
            csi(&[0, 5], "", b'H'),
        ]
    );
}

// tsm-vte.c:3414-3415: a marker right after CSI is collected, apart from
// the intermediates (ECMA-48 5.4.2 b).
#[test]
fn csi_collects_markers() {
    assert_eq!(
        parse(b"\x1b[?25h\x1b[>4;1m\x1b[=c\x1b[<1p"),
        [
            csi(&[25], "?", b'h'),
            csi(&[4, 1], ">", b'm'),
            csi(&[-1], "=", b'c'),
            csi(&[1], "<", b'p'),
        ]
    );
}

// libtsm's test_vte_csi_other_intermediates: intermediates do_collect has no
// name for keep their own byte, so none runs as the plain sequence.
#[test]
fn csi_keeps_every_intermediate() {
    for (seq, im) in [
        (&b"\x1b[1;2,p"[..], ","),
        (b"\x1b[1%p", "%"),
        (b"\x1b[1&p", "&"),
        (b"\x1b[1-p", "-"),
        (b"\x1b[1.p", "."),
        (b"\x1b[1/p", "/"),
    ] {
        let acts = parse(seq);
        assert_eq!(acts.len(), 1, "{seq:?}");
        let Act::Csi(_, got, b'p') = &acts[0] else {
            panic!("{seq:?}: {acts:?}");
        };
        assert_eq!(got, im, "{seq:?}");
    }
}

// tsm-vte.c:3439-3440, 3455-3456: intermediates after the parameters, in
// order and with their count (the C reference kept neither).
#[test]
fn csi_collects_intermediates() {
    assert_eq!(
        parse(b"\x1b[2 q\x1b[!p\x1b[1$p\x1b[61\"p\x1b[!!p\x1b[$!p"),
        [
            csi(&[2], " ", b'q'),
            csi(&[-1], "!", b'p'),
            csi(&[1], "$", b'p'),
            csi(&[61], "\"", b'p'),
            csi(&[-1], "!!", b'p'),
            csi(&[-1], "$!", b'p'),
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
            csi(&[1, 2], " ", b'q'),
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
            csi(&[4], "", b'm'),
            csi(&[38, 1], "", b'm'),
            csi(&[4], "", b'm'),
            csi(&[-1], "", b'm'),
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

// Departure, decided by the operator: in UTF-8 mode U+0080 to U+009F do
// nothing, so C2 9B is not CSI and C2 85 does not cancel a sequence. That
// closes the way past filters that strip ESC. The C reference took them as
// C1 controls (tsm-vte.c:3289-3319).
#[test]
fn c1_as_utf8_does_nothing() {
    assert_eq!(parse(b"a\xc2\x9b1mb"), prints("a1mb"));
    assert_eq!(parse(b"\x1b[1\xc2\x85m"), [csi(&[1], "", b'm')]);
    assert_eq!(parse(b"\x1b]0;x\xc2\x9cy\x07"), [osc(b"0;xy", Bel)]);
}

// libtsm's test_vte_8bit_c1: in 8-bit mode 0x9B introduces a CSI.
#[test]
fn csi_c1_in_8bit_mode() {
    assert_eq!(
        parse_in(InputMode::EightBit, b"\x9b1mB"),
        [csi(&[1], "", b'm'), Act::Print(0x42)]
    );
}

// tsm-vte.c:3363-3364, 3598-3603, 3004-3015: an OSC string ended by BEL.
#[test]
fn osc_ended_by_bel() {
    assert_eq!(parse(b"\x1b]0;title\x07"), [osc(b"0;title", Bel)]);
}

// tsm-vte.c:3291-3298, 3250-3255: ST as C1 ends it, then executes.
#[test]
fn osc_ended_by_c1_st() {
    assert_eq!(
        parse8(b"\x9d10;?\x9c"),
        [osc(b"10;?", St), Act::Execute(0x9c)]
    );
}

// tsm-vte.c:3300-3301: ESC ends it, and ESC \ then dispatches as ST.
#[test]
fn osc_ended_by_esc_backslash() {
    assert_eq!(parse(b"\x1b]2;x\x1b\\"), [osc(b"2;x", St), esc("", b'\\')]);
}

// tsm-vte.c:3300-3301: an ESC ends the string whatever follows it, so the
// Departure (vt100.net: ESC "will "cancel" the control string"; ECMA-48
// 5.6: a control string ends with ST): an ESC not followed by '\\' cancels
// the string, and the ESC sequence it starts runs. The C reference acted on
// the string at the ESC (tsm-vte.c:3300-3301).
#[test]
fn osc_cancelled_by_esc_alone() {
    assert_eq!(
        parse(b"\x1b]2;x\x1b[1m"),
        [osc(b"2;x", Cancelled(0x1b)), csi(&[1], "", b'm')]
    );
}

// The same for DCS and APC: an ESC not followed by '\\' cancels the string.
#[test]
fn dcs_and_apc_cancelled_by_esc_alone() {
    assert_eq!(
        parse(b"\x1bPqa\x1b7"),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            Act::DcsByte(b'a'),
            Act::DcsUnhook(Cancelled(0x1b)),
            esc("", b'7'),
        ]
    );
    assert_eq!(
        parse(b"\x1b_Ga\x1b7"),
        acts![
            Act::ApcStart,
            apc_bytes(b"Ga"),
            Act::ApcEnd(Cancelled(0x1b)),
            esc("", b'7'),
        ]
    );
}

// Departure (vt100.net: "the C0 controls CAN and SUB cancel any ... control
// string in progress"; ECMA-48 8.3.6): CAN and SUB end the string as
// cancelled, then execute. The C reference acted on the string
// (tsm-vte.c:3291-3298, 3004-3015).
#[test]
fn osc_dispatched_by_can_and_sub() {
    assert_eq!(
        parse(b"\x1b]2;x\x18"),
        [osc(b"2;x", Cancelled(0x18)), Act::Execute(0x18)]
    );
    assert_eq!(
        parse(b"\x1b]2;x\x1a"),
        [osc(b"2;x", Cancelled(0x1a)), Act::Execute(0x1a)]
    );
}

// tsm-vte.c:3592-3600: C0 controls other than BEL are ignored inside, and DEL
// is collected.
#[test]
fn osc_ignores_c0_and_collects_del() {
    assert_eq!(parse(b"\x1b]a\nb\x7fc\x07"), [osc(b"ab\x7fc", Bel)]);
}

// tsm-vte.c:3606, 2912-2921: characters past ASCII are collected as UTF-8,
// ill-formed input as U+FFFD, and the noncharacters tsm_ucs4_to_utf8 cannot
// encode are dropped.
#[test]
fn osc_collects_utf8() {
    assert_eq!(
        parse("\x1b]2;é€\x07".as_bytes()),
        [osc("2;é€".as_bytes(), Bel)]
    );
    assert_eq!(
        parse(b"\x1b]a\xed\xa0\x80b\xef\xbf\xbfc\xef\xb7\x90d\x07"),
        [osc("a\u{fffd}\u{fffd}\u{fffd}bcd".as_bytes(), Bel)]
    );
}

// tsm-vte.c:3313-3314, 3244: OSC as C1 restarts with an empty buffer.
#[test]
fn osc_restart_clears() {
    assert_eq!(
        parse8(b"\x1b]abc\x9ddef\x07"),
        [osc(b"abc", Cancelled(0x9d)), osc(b"def", Bel)]
    );
}

// tsm-vte.c:3366-3367, 3501-3509, 3128-3140, 3559-3566: DCS hooks with its
// parameters and final, passes its data and ends at ST.
#[test]
fn dcs_hook_put_unhook() {
    assert_eq!(
        parse(b"\x1bP0;1q#0;2;0~-\x1b\\"),
        acts![
            hook(&[0, 1], "", u32::from(b'q')),
            dcs_bytes(b"#0;2;0~-"),
            Act::DcsUnhook(St),
            esc("", b'\\'),
        ]
    );
}

// tsm-vte.c:3498-3499, 3505-3506, 3530-3531: markers and intermediates.
#[test]
fn dcs_collects_markers_and_intermediates() {
    assert_eq!(
        parse8(b"\x1bP>|x\x9c\x1bP$qm\x9c"),
        acts![
            hook(&[-1], ">", u32::from(b'|')),
            Act::DcsByte(b'x'),
            Act::DcsUnhook(St),
            Act::Execute(0x9c),
            hook(&[-1], "$", u32::from(b'q')),
            Act::DcsByte(b'm'),
            Act::DcsUnhook(St),
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
            hook(&[1, 2], "$", u32::from(b'q')),
            Act::DcsByte(b'z'),
            Act::DcsUnhook(St),
            esc("", b'\\'),
        ]
    );
}

// tsm-vte.c:3561-3568: the data takes C0 controls and drops DEL.
#[test]
fn dcs_data_takes_c0_and_drops_del() {
    assert_eq!(
        parse(b"\x1bPqa\r\n\x7fb\x1b\\"),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            dcs_bytes(b"a\r\nb"),
            Act::DcsUnhook(St),
            esc("", b'\\'),
        ]
    );
}

// tsm-vte.c:3574, 3142-3154: a character past ASCII in the data.
#[test]
fn dcs_data_past_ascii() {
    assert_eq!(
        parse("\x1bPqé\x1b\\".as_bytes()),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            Act::DcsChar(0xe9),
            Act::DcsUnhook(St),
            esc("", b'\\'),
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
        assert_eq!(parse(&input), [esc("", b'\\'), Act::Print(0x78)], "{seq:?}");
    }
}

// Departure, decided by the operator: in UTF-8 mode a character from
// U+00A0 up in a DCS header ignores the string to its end, as the C
// reference's own rule for CSI does (tsm-vte.c:3421). The C reference took
// it as the final (tsm-vte.c:3512, 3537, 3557).
#[test]
fn dcs_header_with_a_character_past_ascii_is_ignored() {
    for seq in ["\x1bPé", "\x1bP1é", "\x1bP$é"] {
        let input = format!("{seq}data\x1b\\x");
        assert_eq!(
            parse(input.as_bytes()),
            [esc("", b'\\'), Act::Print(0x78)],
            "{seq:?}"
        );
    }
}

// Departure (vt100.net: "In all states, these codes are treated
// identically to GL codes 20-7F"): in 8-bit mode a GR byte inside a
// sequence acts as its GL byte. The C reference ignored CSI sequences,
// collected ESC intermediates and took DCS finals instead.
#[test]
fn gr_acts_as_gl_in_8bit_sequences() {
    // CSI 1 m with the final as 0xED
    assert_eq!(parse8(b"\x9b1\xed"), [csi(&[1], "", b'm')]);
    // CSI 3 ; 5 H with every byte after CSI in GR
    assert_eq!(parse8(b"\x9b\xb3\xbb\xb5\xc8"), [csi(&[3, 5], "", b'H')]);
    // ESC ( 0
    assert_eq!(parse8(b"\x1b\xa8\xb0"), [esc("(", b'0')]);
    // DCS q, data, ST
    assert_eq!(
        parse8(b"\x90\xf1#\x9c"),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            Act::DcsByte(b'#'),
            Act::DcsUnhook(St),
            Act::Execute(0x9c),
        ]
    );
    // GR in ground and in strings stays itself
    assert_eq!(
        parse8(b"\xe9\x9d0;\xe9\x07"),
        [Act::Print(0xe9), osc("0;é".as_bytes(), Bel)]
    );
}

// tsm-vte.c:3291-3298, 3250-3251: CAN ends the data, then executes.
#[test]
fn dcs_ended_by_can() {
    assert_eq!(
        parse(b"\x1bPqab\x18c"),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            dcs_bytes(b"ab"),
            Act::DcsUnhook(Cancelled(0x18)),
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
        parse8(b"\x90qa\x9b2J"),
        acts![
            hook(&[-1], "", u32::from(b'q')),
            Act::DcsByte(b'a'),
            Act::DcsUnhook(Cancelled(0x9b)),
            csi(&[2], "", b'J'),
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
            Act::ApcEnd(St),
            esc("", b'\\'),
        ]
    );
}

// libtsm's test_vte_kitty C1 framing: APC and ST as bytes in 8-bit mode.
// As UTF-8 they do nothing (a departure, see c1_as_utf8_does_nothing).
#[test]
fn apc_c1_framing() {
    let want = acts![
        Act::ApcStart,
        apc_bytes(b"Gi=1"),
        Act::ApcEnd(St),
        Act::Execute(0x9c),
    ];
    assert_eq!(parse(b"\xc2\x9fGi=1\xc2\x9c"), prints("Gi=1"));
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
            Act::ApcEnd(St),
            esc("", b'\\'),
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
            Act::ApcEnd(St),
            esc("", b'\\'),
        ]
    );
}

// tsm-vte.c:3307-3308, 3258-3269: APC inside APC ends the first and starts
// another, since a transition to the same state runs both actions.
#[test]
fn apc_restarts() {
    assert_eq!(
        parse8(b"\x9fa\x9fb\x9c"),
        acts![
            Act::ApcStart,
            Act::ApcByte(b'a'),
            Act::ApcEnd(Cancelled(0x9f)),
            Act::ApcStart,
            Act::ApcByte(b'b'),
            Act::ApcEnd(St),
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
            Act::ApcEnd(Cancelled(0x1a)),
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

// Departure 6 inside an ignored CSI: in 8-bit mode the GR byte 0xED acts
// as `m` and ends the ignored sequence, so the `x` after it prints.
#[test]
fn gr_ends_an_ignored_csi_in_8bit_mode() {
    assert_eq!(parse8(b"\x9b1?\xedx"), [Act::Print(0x78)]);
}

// aaronsb/libtsm#53 covers APC strings only: after a reset at the ESC of an
// OSC string's ST, the `\` prints in ground state, as in the C reference.
#[test]
fn reset_after_osc_esc_prints_the_backslash() {
    let mut parser = vtium_parse::Parser::new();
    let mut rec = common::Recorder::new();
    rec.feed(&mut parser, b"\x1b]0;t\x1b");
    parser.reset();
    rec.feed(&mut parser, b"\\");
    assert_eq!(rec.acts, [Act::Print(0x5c)]);
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
        acts![Act::ApcStart, apc_bytes(b"Ga=T;AAAA"), prints("text")]
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
        acts![Act::ApcStart, apc_bytes(b"Gq"), prints("x\\")]
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
            Act::ApcEnd(St),
            esc("", b'\\'),
            prints("text"),
        ]
    );
}

// Departure (ECMA-48 5.4 c, d: intermediate bytes "Together with the Final
// Byte F ... identify the control function"): intermediates keep their
// order and count, and each keeps its own byte. The C reference collapsed
// them to a set of flags, with every unnamed one as OTHER
// (tsm-vte.c:1232-1275), so `CSI ! ! p` ran as DECSTR.
#[test]
fn intermediates_keep_order_count_and_identity() {
    assert_ne!(parse(b"\x1b[!!p"), parse(b"\x1b[!p"));
    assert_ne!(parse(b"\x1b[$!p"), parse(b"\x1b[!$p"));
    assert_ne!(parse(b"\x1b%G"), parse(b"\x1b-G"));
    assert_ne!(parse(b"\x1b[1,p"), parse(b"\x1b[1.p"));
    assert_ne!(parse(b"\x1bP<q\x1b\\"), parse(b"\x1bP=q\x1b\\"));
}

// Departure (vt100.net: "If more than two intermediate characters arrive,
// the parser can just flag this so that the dispatch can be turned into a
// null operation"): a third intermediate makes the sequence do nothing, in
// ESC, CSI and DCS alike. The C reference had no count bound.
#[test]
fn a_third_intermediate_makes_a_null_operation() {
    assert_eq!(parse(b"\x1b[!\"$p"), []);
    assert_eq!(parse(b"\x1b(((B"), []);
    let dcs = parse(b"\x1bP!\"$qdata\x1b\\x");
    assert!(
        !dcs.iter()
            .any(|a| matches!(a, Act::DcsHook(..) | Act::DcsByte(_) | Act::DcsUnhook(_))),
        "{dcs:?}"
    );
    assert_eq!(dcs.last(), Some(&Act::Print(u32::from(b'x'))));
    // two still dispatch
    assert_eq!(parse(b"\x1b$(B").len(), 1);
}
