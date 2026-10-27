//! The character sets, driven through the byte stream: SCS designating G0
//! to G3, SO and SI, the locking shifts into GL and GR, the single shifts,
//! DEC special graphics and DEC supplemental graphics, GR in 8-bit input,
//! and DECSC, DECRC, DECSTR and RIS around them.
//!
//! In UTF-8 input a decoded character prints as itself whatever the sets
//! hold, as in xterm's UTF-8 mode. Each expected screen is the C
//! reference's, or xterm's and the VT220's where vtium departs from the C
//! reference (the crate's README lists each departure).

mod common;

use common::{rows, term};

/// The first row after `bytes`, on a terminal 120 columns wide.
fn shows(bytes: &[u8]) -> String {
    rows(term(120, 1, bytes).screen()).swap_remove(0)
}

/// DECSCL 62: 8-bit input, where GR is reachable.
const EIGHT_BIT: &[u8] = b"\x1b[62\"p";

// VT220 table 2-4, DEC special graphics, from 5/15 to 7/14, each to the
// character xterm draws; 5/15 is a blank. Below 5/15 the set is
// ASCII. ESC ( B designates ASCII again (the conformance case
// dec-graphics).
// covers: text.charsets
#[test]
fn dec_special_graphics_draws_lines() {
    let t = term(12, 3, b"\x1b(0lqqqk\r\nx   x\r\nmqqqj\x1b(B ascii");
    assert_eq!(
        rows(t.screen()),
        [
            "\u{250C}\u{2500}\u{2500}\u{2500}\u{2510}",
            "\u{2502}   \u{2502}",
            "\u{2514}\u{2500}\u{2500}\u{2500}\u{2518} ascii",
        ]
    );
    assert_eq!(
        shows(b"\x1b(0AZ[^_`abcdefghijklmnopqrstuvwxyz{|}~"),
        "AZ[^ \u{25C6}\u{2592}\u{2409}\u{240C}\u{240D}\u{240A}\u{B0}\u{B1}\u{2424}\u{240B}\
         \u{2518}\u{2510}\u{250C}\u{2514}\u{253C}\u{23BA}\u{23BB}\u{2500}\u{23BC}\u{23BD}\
         \u{251C}\u{2524}\u{2534}\u{252C}\u{2502}\u{2264}\u{2265}\u{3C0}\u{2260}\u{A3}\u{B7}"
    );
    assert_eq!(shows(b"a\x1b(0_\x1b(Bb"), "a b");
}

// VT220 4.4.3, SO (LS1) invokes G1 into GL and SI (LS0) G0, the shift
// TERM=linux's enacs, GNU screen and elinks rely on. G1 holds ASCII until
// something is designated there.
// covers: text.charsets
#[test]
fn shift_out_invokes_g1_into_gl() {
    assert_eq!(shows(b"\x1b)0\x0eq\x0fq"), "\u{2500}q");
    assert_eq!(shows(b"\x0eq\x0f"), "q");
    // A designation into G1 while it is in GL takes effect at once.
    assert_eq!(shows(b"\x0e\x1b)0q\x1b)Bq"), "\u{2500}q");
}

// vtium/vtium#13: a character decoded from UTF-8 prints as itself, whatever
// G1 to G3 hold and whichever of them GR holds. The C reference ran U+00A1
// to U+00FE through GR, so after ESC ) 0 an ñ printed as a line.
// covers: text.charsets
#[test]
fn a_utf8_character_prints_as_itself_whatever_g1_to_g3_hold() {
    assert_eq!(shows("\x1b)0\u{F1}".as_bytes()), "\u{F1}");
    let latin1: String = ('\u{A0}'..='\u{FF}').collect();
    for gr in ["\x1b)0\x1b~", "\x1b*0\x1b}", "\x1b+0\x1b|", "\x1b)<\x1b~"] {
        assert_eq!(shows(format!("{gr}{latin1}").as_bytes()), latin1, "{gr:?}");
    }
    // DEC special graphics in GL maps ASCII only.
    assert_eq!(
        shows("\x1b(0\u{F1}q\x1b)0\x0e\u{E9}q".as_bytes()),
        "\u{F1}\u{2500}\u{E9}\u{2500}"
    );
}

// VT220 4.4.4: SS2 and SS3 invoke G2 or G3 into GL for the next graphic
// character only, whatever that character is; a control between them
// leaves the shift waiting, and DEL, which prints nothing, does too.
// covers: text.charsets
#[test]
fn single_shifts_take_the_next_character_only() {
    assert_eq!(shows(b"\x1b*0\x1bNqq"), "\u{2500}q");
    assert_eq!(shows(b"\x1b+0\x1bOqq"), "\u{2500}q");
    assert_eq!(shows(b"\x1b*0\x1bN\rq"), "\u{2500}");
    assert_eq!(shows(b"\x1b*0\x1bN\x7fq"), "\u{2500}");
    assert_eq!(shows("\x1b*0\x1bN\u{F1}q".as_bytes()), "\u{F1}q");
    assert_eq!(shows(b"\x1b*0\x1bN q"), " q");
    // With G2 and G3 holding ASCII, as they start, a single shift changes
    // nothing.
    assert_eq!(shows(b"\x1bNq\x1bOq"), "qq");
    // The C1 forms act in 8-bit input.
    let eight = [EIGHT_BIT, b"\x1b*0\x1b+0\x8eqq\x8fqq"].concat();
    assert_eq!(shows(&eight), "\u{2500}q\u{2500}q");
}

// VT220 4.4.3: LS2 and LS3 invoke G2 or G3 into GL until another locking
// shift.
// covers: text.charsets
#[test]
fn locking_shifts_invoke_g2_and_g3_into_gl() {
    assert_eq!(shows(b"\x1b*0\x1bnqq\x0fq"), "\u{2500}\u{2500}q");
    assert_eq!(shows(b"\x1b+0\x1boqq\x0fq"), "\u{2500}\u{2500}q");
    assert_eq!(shows(b"\x1b*0\x1bn\x0eq\x1bnq"), "q\u{2500}");
}

// VT220 4.4: in 8-bit input the bytes A1 to FE are GR, and map through the
// set GR holds as the byte 80 below them would through GL. GR holds G2,
// which DECSCL 62 to 64 designates DEC supplemental graphics, the VT220's
// default mapping; LS1R, LS2R and LS3R invoke G1, G2 or G3 into GR. A0 and
// FF print as themselves.
// covers: text.charsets
#[test]
fn gr_maps_the_upper_half_in_eight_bit_input() {
    let eight = |bytes: &[u8]| shows(&[EIGHT_BIT, bytes].concat());
    // DEC supplemental: F1 ñ, A8 ¤, D7 Œ, A4 reserved and printing nothing.
    assert_eq!(eight(b"\xf1\xa8\xd7\xa4q"), "\u{F1}\u{A4}\u{152}q");
    assert_eq!(eight(b"\xa0\xff"), "\u{A0}\u{FF}");
    // G1 is not in GR until LS1R.
    assert_eq!(eight(b"\x1b)0\xf1\x1b~\xf1"), "\u{F1}\u{2500}");
    assert_eq!(eight(b"\x1b*0\xf1"), "\u{2500}");
    assert_eq!(eight(b"\x1b+0\x1b|\xf1\x1b}\xf1"), "\u{2500}\u{F1}");
    // GL keeps G0 meanwhile.
    assert_eq!(eight(b"\x1b*0\xf1q"), "\u{2500}q");
    // ASCII in GR.
    assert_eq!(eight(b"\x1b*B\xf1\xc1"), "qA");
    // A single shift is taken by a GR byte too, which still maps through GR.
    assert_eq!(eight(b"\x1b+0\x1bO\xf1q"), "\u{F1}q");
}

// VT220 4.2: 7-bit input clears each byte's eighth bit, so GR is never
// reached; DECSCL 61 designates nothing past what VT100 mode has.
// covers: text.charsets
#[test]
fn seven_bit_input_reaches_only_gl() {
    assert_eq!(shows(b"\x1b[61\"p\x1b(0\xf1\x1b(Bq\x0eq\x0f"), "\u{2500}qq");
}

// DEC supplemental graphics designated into GL maps ASCII to the upper
// half in 8-bit input, where it is the VT220's; in UTF-8 and 7-bit input
// `<` designates ASCII, as in xterm, Konsole, Ghostty and the Linux
// console.
// covers: text.charsets
#[test]
fn dec_supplemental_graphics_only_in_eight_bit_input() {
    let eight = [EIGHT_BIT, b"\x1b(<q1($\x1b(Bq"].concat();
    assert_eq!(shows(&eight), "\u{F1}\u{B1}\u{A4}q");
    assert_eq!(shows(b"\x1b(0\x1b(<q1($"), "q1($");
    assert_eq!(shows(b"\x1b[61\"p\x1b(<q"), "q");
}

// ADR-1000 refuses DECNRCM (DEC-42), so a national replacement set's final
// designates ASCII, as xterm takes it with DECNRCM reset.
// covers: text.charsets
#[test]
fn national_replacement_sets_designate_ascii() {
    assert_eq!(
        shows(b"\x1b(0\x1b(A#a\x1b(K[{~\x1b)Z\x0e#\x0f\x1b(=q"),
        "#a[{~#q"
    );
}

// VT510, DECSC: it saves the sets GL and GR hold and a pending single
// shift, and xterm saves what G0 to G3 hold with them; DECRC with nothing
// saved returns the sets to the default mapping, as xterm does.
// covers: ctl.decsc, text.charsets
#[test]
fn decsc_saves_the_character_sets() {
    assert_eq!(shows(b"\x1b(0\x1b7\x1b(B\x1b8q"), "\u{2500}");
    assert_eq!(shows(b"\x1b)0\x0e\x1b7\x0f\x1b)B\x1b8q"), "\u{2500}");
    assert_eq!(shows(b"\x1b*0\x1b7\x1bN\x1b8q"), "q");
    assert_eq!(shows(b"\x1b*0\x1bN\x1b7q\x1b8q"), "\u{2500}");
    assert_eq!(shows(b"\x1b(0\x1b8q"), "q");
    let eight = [EIGHT_BIT, b"\x1b)0\x1b~\x1b7\x1b}\x1b8\xf1"].concat();
    assert_eq!(shows(&eight), "\u{2500}");
}

// VT220 4.4: DECSTR selects the default mapping, and RIS returns every set
// to it.
// covers: ctl.decstr, text.charsets
#[test]
fn decstr_and_ris_reset_the_character_sets() {
    assert_eq!(shows(b"\x1b(0\x1b)0\x0e\x1b[!pq"), "q");
    assert_eq!(shows(b"\x1b*0\x1bn\x1bcq"), "q");
    assert_eq!(shows(b"\x1b+0\x1bO\x1b[!pq"), "q");
}
