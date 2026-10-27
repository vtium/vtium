//! The replies and the control functions around them, driven through the
//! byte stream: the identification and status reports, XTGETTCAP at its
//! bounds, DECRQSS, the colour queries, XTWINOPS, DECSCUSR, DECSC and
//! DECRC, the keypad and conformance levels, and the sequences vtium
//! ignores or refuses, which change nothing and send nothing.
//!
//! Each expected reply is the C reference's, or the specification's where
//! vtium departs from it (the crate's README lists each departure). The
//! reply rule's test, with hostile text in every field, is the unit test
//! in `src/terminal/reply.rs`.

mod common;

use common::{rows, term};
use vtium_term::{
    Attrs, CursorStyle, Mods, Termcap, TermcapError, Terminal, XTGETTCAP_NAMES, XTGETTCAP_REPLY_MAX,
};

fn reply(t: &mut Terminal, query: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(query);
    t.take_replies()
}

/// A terminal in a state far from its initial one.
fn busy() -> Terminal {
    let mut t = term(
        10,
        4,
        b"ab\x1b[1;31m\x1b[2;3r\x1b[2;2Hx\x1b[?1000h\x1b[4 q\x1b7",
    );
    t.take_replies();
    t
}

/// Sends each sequence to a busy terminal and checks it changes nothing
/// and sends nothing.
fn inert(seqs: &[&[u8]]) {
    for seq in seqs {
        let mut t = busy();
        let before = format!("{t:?}");
        t.input(seq);
        assert_eq!(t.take_replies(), b"", "{seq:?} replied");
        assert_eq!(format!("{t:?}"), before, "{seq:?} changed the state");
    }
}

// ctlseqs, DA1: a VT220 (62), one of xterm's defined levels, with ANSI
// colour (22); the C reference sent level 60 (aaronsb/libtsm#31). DECID
// (ESC Z) answers the same; a parameter other than 0 is another request.
// covers: query.da1, query.decid
#[test]
fn da1_reports_a_vt220_with_colour() {
    let mut t = term(8, 2, b"");
    assert_eq!(reply(&mut t, b"\x1b[c"), b"\x1b[?62;22c");
    assert_eq!(reply(&mut t, b"\x1b[0c"), b"\x1b[?62;22c");
    assert_eq!(reply(&mut t, b"\x1bZ"), b"\x1b[?62;22c");
    assert_eq!(reply(&mut t, b"\x1b[1c"), b"");
}

// ctlseqs, DA2: a VT220, firmware 1, as the C reference answered.
// covers: query.da2
#[test]
fn da2_reports_a_vt220() {
    let mut t = term(8, 2, b"");
    assert_eq!(reply(&mut t, b"\x1b[>c"), b"\x1b[>1;1;0c");
    assert_eq!(reply(&mut t, b"\x1b[>0c"), b"\x1b[>1;1;0c");
}

// VT510, DA3 and DECRPTUI: a unit id of zeros, as xterm answers; the C
// reference answered `CSI = c` as DA1 (aaronsb/libtsm#31).
// covers: query.da3
#[test]
fn da3_reports_a_unit_id() {
    let mut t = term(8, 2, b"");
    assert_eq!(reply(&mut t, b"\x1b[=c"), b"\x1bP!|00000000\x1b\\");
}

// ECMA-48 8.3.35, DSR: 5 reports no malfunction, 6 the cursor (CPR);
// DECXCPR (`CSI ? 6 n`) adds the page. In origin mode the row counts from
// the region's top (VT100 User Guide, CPR: "The numbering of lines depends
// on the state of the Origin Mode (DECOM)"), and a pending wrap reports the
// last column, where the cursor stands.
// covers: query.dsr
#[test]
fn dsr_reports_the_status_and_the_cursor() {
    let mut t = term(10, 6, b"\x1b[3;5H");
    assert_eq!(reply(&mut t, b"\x1b[5n"), b"\x1b[0n");
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[3;5R");
    assert_eq!(reply(&mut t, b"\x1b[?6n"), b"\x1b[?3;5;1R");
    t.input(b"\x1b[2;5r\x1b[?6h\x1b[2;3H");
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[2;3R");
    t.input(b"\x1b[?6l\x1b[r\x1b[1;7Habcd");
    assert_eq!(t.screen().cursor(), (10, 0));
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[1;10R");
}

// ctlseqs: the DEC printer, key, locator, macro, checksum and session
// reports, `CSI ? Ps n`, get no reply (ADR-1000, query.dsr-other).
// covers: query.dsr-other
#[test]
fn the_other_device_status_reports_are_ignored() {
    inert(&[
        b"\x1b[?15n",
        b"\x1b[?25n",
        b"\x1b[?26n",
        b"\x1b[?53n",
        b"\x1b[?55n",
        b"\x1b[?56n",
        b"\x1b[?62n",
        b"\x1b[?63n",
        b"\x1b[?75n",
        b"\x1b[?85n",
        b"\x1b[?5n",
    ]);
}

// ctlseqs, XTVERSION: `DCS > | text ST`, the host's text; a parameter
// other than 0 is another request. The text is printable ASCII of at most
// 64 bytes.
// covers: query.xtversion
#[test]
fn xtversion_reports_the_hosts_text() {
    let mut t = term(8, 2, b"");
    let text = reply(&mut t, b"\x1b[>q");
    assert!(text.starts_with(b"\x1bP>|vtium(") && text.ends_with(b")\x1b\\"));
    t.set_version("console 1.0").unwrap();
    assert_eq!(reply(&mut t, b"\x1b[>0q"), b"\x1bP>|console 1.0\x1b\\");
    assert_eq!(reply(&mut t, b"\x1b[>1q"), b"");
    assert!(t.set_version("a\x1b").is_err());
    assert!(t.set_version(&"v".repeat(65)).is_err());
    assert_eq!(reply(&mut t, b"\x1b[>q"), b"\x1bP>|console 1.0\x1b\\");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// A table of names `N00` to `N{n-1}`, each with `value`.
fn table(n: usize, value: &[u8]) -> (Termcap, Vec<String>) {
    let names: Vec<String> = (0..n).map(|i| format!("N{i:02}")).collect();
    let caps = Termcap::new(names.iter().map(|n| (n.as_bytes(), Some(value)))).unwrap();
    (caps, names)
}

fn xtgettcap(names: &[String]) -> Vec<u8> {
    let list: Vec<String> = names.iter().map(|n| hex(n.as_bytes())).collect();
    format!("\x1bP+q{}\x1b\\", list.join(";")).into_bytes()
}

// ctlseqs, XTGETTCAP: names printed in uppercase hex from the table, never
// in the request's spelling (the reply rule); a boolean is its name alone;
// an unknown name ends the list after the `;` before it, and as the first
// name gets `DCS 0 + r ST`.
// covers: query.xtgettcap
#[test]
fn xtgettcap_answers_from_the_table_in_uppercase() {
    let caps: [(&[u8], Option<&[u8]>); 2] = [(b"TN", Some(b"vtium")), (b"am", None)];
    let mut t = term(8, 2, b"");
    t.set_termcap(Termcap::new(caps).unwrap());
    assert_eq!(
        reply(&mut t, b"\x1bP+q544e;616D\x1b\\"),
        b"\x1bP1+r544E=767469756D;616D\x1b\\"
    );
    assert_eq!(
        reply(&mut t, b"\x1bP+q616d;5858;544E\x1b\\"),
        b"\x1bP1+r616D;\x1b\\"
    );
    assert_eq!(reply(&mut t, b"\x1bP+q5858\x1b\\"), b"\x1bP0+r\x1b\\");
    assert_eq!(reply(&mut t, b"\x1bP+q54\x1b\\"), b"\x1bP0+r\x1b\\");
    assert_eq!(reply(&mut t, b"\x1bP+q54G\x1b\\"), b"\x1bP0+r\x1b\\");
    // Ended by anything but ST, it is dropped.
    assert_eq!(reply(&mut t, b"\x1bP+q544e\x07"), b"");
}

// The bound of names: sixteen are answered, and the seventeenth ends the
// list after the `;` before it.
// covers: query.xtgettcap
#[test]
fn xtgettcap_answers_sixteen_names() {
    let (caps, names) = table(XTGETTCAP_NAMES + 1, b"v");
    let mut t = term(8, 2, b"");
    t.set_termcap(caps);
    let pair = |n: &String| format!("{}=76", hex(n.as_bytes()));
    let sixteen = &names[..XTGETTCAP_NAMES];
    let want: Vec<String> = sixteen.iter().map(pair).collect();
    assert_eq!(
        String::from_utf8(reply(&mut t, &xtgettcap(sixteen))).unwrap(),
        format!("\x1bP1+r{}\x1b\\", want.join(";"))
    );
    assert_eq!(
        String::from_utf8(reply(&mut t, &xtgettcap(&names))).unwrap(),
        format!("\x1bP1+r{};\x1b\\", want.join(";"))
    );
}

// The bound of the reply: a reply of exactly XTGETTCAP_REPLY_MAX bytes is
// sent whole, and one byte longer drops its last pair. Each pair here is
// the name (6 hex digits), `=`, and the value; the C reference refused a
// reply of exactly the bound.
// covers: query.xtgettcap
#[test]
fn xtgettcap_replies_up_to_its_bound() {
    // Seven names of 128-byte values: each pair is 6 + 1 + 256 = 263
    // bytes, and a reply is 5 + 7 * 263 + 6 + 2 = 1854 bytes. The last
    // value's length decides how close it comes to the bound.
    let fill = |last: usize| -> (Termcap, Vec<String>) {
        let names: Vec<String> = (0..8).map(|i| format!("N{i:02}")).collect();
        let values: Vec<Vec<u8>> = (0..8)
            .map(|i| vec![b'v'; if i == 7 { last } else { 255 }])
            .collect();
        let caps = Termcap::new(
            names
                .iter()
                .zip(&values)
                .map(|(n, v)| (n.as_bytes(), Some(v.as_slice()))),
        )
        .unwrap();
        (caps, names)
    };
    // Seven pairs of 6 + 1 + 510 bytes and their `;`, the DCS and the ST
    // take 5 + 7 * 518 + 2 = 3633 bytes; the eighth pair's 6 + 1 bytes and
    // its value's hex bring it to the bound at 456 hex digits.
    let (caps, names) = fill(228);
    let mut t = term(8, 2, b"");
    t.set_termcap(caps);
    let whole = reply(&mut t, &xtgettcap(&names));
    assert_eq!(whole.len(), XTGETTCAP_REPLY_MAX);
    assert!(whole.ends_with(b"6\x1b\\"));
    let (caps, names) = fill(229);
    t.set_termcap(caps);
    let cut = reply(&mut t, &xtgettcap(&names));
    assert_eq!(cut.len(), 5 + 7 * 518 + 2);
    assert!(cut.ends_with(b";\x1b\\"));
}

// libtsm.h's bounds on the host's table: names of 1 to 32 printable bytes,
// values of at most 256 bytes and no NUL, at most 1024 entries, each name
// once.
// covers: query.xtgettcap
#[test]
fn a_table_past_its_bounds_is_refused() {
    let one = |name: &[u8], value: &[u8]| Termcap::new([(name, Some(value))]);
    assert!(one(b"ok", b"v").is_ok());
    assert!(matches!(one(b"", b"v"), Err(TermcapError::BadName(_))));
    assert!(matches!(
        one(&[b'n'; 33], b"v"),
        Err(TermcapError::BadName(_))
    ));
    assert!(matches!(one(b"a b", b"v"), Err(TermcapError::BadName(_))));
    assert!(matches!(
        one(b"v", &[b'v'; 257]),
        Err(TermcapError::BadValue(_))
    ));
    assert!(matches!(one(b"v", b"a\0"), Err(TermcapError::BadValue(_))));
    let twice: [(&[u8], Option<&[u8]>); 2] = [(b"x", None), (b"x", None)];
    assert!(matches!(
        Termcap::new(twice),
        Err(TermcapError::Duplicate(_))
    ));
    let names: Vec<String> = (0..1025).map(|i| format!("n{i}")).collect();
    let many = Termcap::new(names.iter().map(|n| (n.as_bytes(), None)));
    assert_eq!(many, Err(TermcapError::TooMany));
}

// ctlseqs and VT510, DECRQSS: SGR, DECSTBM, DECSCUSR and DECSCL are
// reported; anything else gets the error form with no echo (ADR-1000).
// covers: query.decrqss
#[test]
fn decrqss_reports_settings_and_refuses_with_no_echo() {
    let mut t = term(
        10,
        6,
        b"\x1b[1;4:3;7;38;5;100;48;2;1;2;3;58;5;9m\x1b[2;5r\x1b[5 q",
    );
    assert_eq!(
        reply(&mut t, b"\x1bP$qm\x1b\\"),
        b"\x1bP1$r0;1;4:3;7;38:5:100;48:2::1:2:3;58:5:9m\x1b\\"
    );
    assert_eq!(reply(&mut t, b"\x1bP$qr\x1b\\"), b"\x1bP1$r2;5r\x1b\\");
    assert_eq!(reply(&mut t, b"\x1bP$q q\x1b\\"), b"\x1bP1$r5 q\x1b\\");
    assert_eq!(reply(&mut t, b"\x1bP$q\"p\x1b\\"), b"\x1bP1$r62;1\"p\x1b\\");
    assert_eq!(reply(&mut t, b"\x1b[m\x1bP$qm\x1b\\"), b"\x1bP1$r0m\x1b\\");
    for unknown in [
        &b"\x1bP$qs\x1b\\"[..],
        b"\x1bP$q31m\x1b\\",
        b"\x1bP$qmm\x1b\\",
    ] {
        assert_eq!(reply(&mut t, unknown), b"\x1bP0$r\x1b\\", "{unknown:?}");
    }
    assert_eq!(reply(&mut t, b"\x1bP$qm\x18"), b"");
}

// ctlseqs, SGR 58: the underline colour has no short form, so DECRQSS
// reports every index, the first sixteen among them, as `58:5:Ps`.
// covers: query.decrqss
#[test]
fn decrqss_reports_every_underline_colour_index_in_the_colon_form() {
    let mut t = term(8, 2, b"");
    for i in [0, 1, 3, 7, 8, 9, 15] {
        let set = format!("\x1b[m\x1b[58:5:{i}m");
        t.input(set.as_bytes());
        assert_eq!(
            reply(&mut t, b"\x1bP$qm\x1b\\"),
            format!("\x1bP1$r0;58:5:{i}m\x1b\\").as_bytes(),
            "index {i}"
        );
    }
}

// ctlseqs, DECRQSS: a direct underline colour reads back in the colon form.
#[test]
fn decrqss_reports_a_direct_underline_colour_in_the_colon_form() {
    let mut t = term(8, 2, b"");
    t.input(b"\x1b[m\x1b[58:2::18:52:86m");
    assert_eq!(
        reply(&mut t, b"\x1bP$qm\x1b\\"),
        b"\x1bP1$r0;58:2::18:52:86m\x1b\\"
    );
}

// ctlseqs, OSC 4: each `index ; ?` pair answers with the palette's colour,
// ended as the query was; an index past 255 is left out.
// covers: colour.palette.query
#[test]
fn osc_4_reports_palette_colours() {
    let mut t = term(8, 2, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]4;1;?;300;?;196;?\x07"),
        b"\x1b]4;1;rgb:cdcd/0000/0000\x07\x1b]4;196;rgb:ffff/0000/0000\x07"
    );
    assert_eq!(
        reply(&mut t, b"\x1b]4;232;?\x1b\\"),
        b"\x1b]4;232;rgb:0808/0808/0808\x1b\\"
    );
    assert_eq!(reply(&mut t, b"\x1b]4;1;#123456\x07"), b"");
}

// ctlseqs, OSC 10 and 11: the default foreground and background; a list
// after 10 goes on to 11.
// covers: colour.dynamic.query
#[test]
fn osc_10_and_11_report_the_default_colours() {
    let mut t = term(8, 2, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]10;?\x07"),
        b"\x1b]10;rgb:e5e5/e5e5/e5e5\x07"
    );
    assert_eq!(
        reply(&mut t, b"\x1b]11;?\x1b\\"),
        b"\x1b]11;rgb:0000/0000/0000\x1b\\"
    );
    assert_eq!(
        reply(&mut t, b"\x1b]10;?;?\x07"),
        b"\x1b]10;rgb:e5e5/e5e5/e5e5\x07\x1b]11;rgb:0000/0000/0000\x07"
    );
}

// ADR-1000, "Replies and policy": XTWINOPS reports a size only where vtium
// has what it measures: the text area in pixels (14) and cells (18), the
// cell (16), the screen in cells (19) and the display in pixels (15).
// covers: window.size-report, window.screen-pixels
#[test]
fn xtwinops_reports_the_sizes_vtium_has() {
    let mut t = term(10, 4, b"");
    assert_eq!(reply(&mut t, b"\x1b[14t"), b"");
    assert_eq!(reply(&mut t, b"\x1b[15t"), b"");
    t.set_cell_size(8, 16);
    t.set_display_size(1366, 768);
    assert_eq!(reply(&mut t, b"\x1b[14t"), b"\x1b[4;64;80t");
    assert_eq!(reply(&mut t, b"\x1b[15t"), b"\x1b[5;768;1366t");
    assert_eq!(reply(&mut t, b"\x1b[16t"), b"\x1b[6;16;8t");
    assert_eq!(reply(&mut t, b"\x1b[18t"), b"\x1b[8;4;10t");
    assert_eq!(reply(&mut t, b"\x1b[19t"), b"\x1b[9;4;10t");
}

// ADR-1000: the title report is a fixed empty title, whatever OSC 0 and 2
// sent, as no title is kept.
// covers: window.title-report
#[test]
fn the_title_report_is_empty() {
    let mut t = term(8, 2, b"\x1b]2;\x1b[6n\x07\x1b]0;rm -rf ~\x1b\\");
    assert_eq!(reply(&mut t, b"\x1b[21t"), b"\x1b]l\x1b\\");
}

// ADR-1000: the window's state, position and pixels (11, 13, 14;2), the
// icon's label (20) and the title stack (22, 23) get no reply and change
// nothing; moving, resizing, raising, lowering and iconifying the window
// (1 to 7, 9, 10, 24 and above) are refused.
// covers: window.state-report, window.icon-report, window.title-stack, window.manipulate
#[test]
fn window_reports_vtium_has_no_window_for_are_ignored() {
    inert(&[
        b"\x1b[11t",
        b"\x1b[13t",
        b"\x1b[14;2t",
        b"\x1b[20t",
        b"\x1b[22;0t",
        b"\x1b[23;0t",
        b"\x1b[1t",
        b"\x1b[2t",
        b"\x1b[3;10;10t",
        b"\x1b[4;400;600t",
        b"\x1b[5t",
        b"\x1b[6t",
        b"\x1b[7t",
        b"\x1b[9;1t",
        b"\x1b[10;1t",
        b"\x1b[24t",
        b"\x1b[48t",
    ]);
}

// ctlseqs: XTSMTITLE, XTRMTITLE and XTTITLEPOS, XTSMPOINTER, and kitty's
// multiple cursors are ignored.
// covers: window.xtsmtitle, window.xtsmpointer, cursor.kitty-multiple
#[test]
fn title_modes_the_pointer_and_extra_cursors_are_ignored() {
    inert(&[
        b"\x1b[>3t",
        b"\x1b[>3T",
        b"\x1b[#S",
        b"\x1b[>2p",
        b"\x1b[>29;2:1:1 q",
        b"\x1b[>2;1:3:4 q",
    ]);
}

// ctlseqs and VT510: XTSETTCAP and XTGETXRES, XTREPORTSGR, XTCHECKSUM and
// DECRQCRA, DECRQPSR and DECRSPS, DECREQTPARM, DECRQDE, DECRQUPSS and
// DECRQTSR, and the DEC locator get no reply and change nothing.
// covers: query.xtsettcap, query.xtreportsgr, query.decrqpsr, query.decreqtparm, query.locator
#[test]
fn the_reports_vtium_does_not_give_are_ignored() {
    inert(&[
        b"\x1bP+p544e=7674\x1b\\",
        b"\x1bP+Q666f6e74\x1b\\",
        b"\x1b[1;1;2;2#|",
        b"\x1b[0#y",
        b"\x1b[1;1;1;1;2;2*y",
        b"\x1b[1$w",
        b"\x1b[2$w",
        b"\x1bP1$t1;1;1;@;@;@;0;2;@;BBBB\x1b\\",
        b"\x1b[x",
        b"\x1b[1x",
        b"\x1b[\"v",
        b"\x1b[&u",
        b"\x1b[1$u",
        b"\x1b[1;1'z",
        b"\x1b[1'{",
        b"\x1b[1;1;2;2'w",
        b"\x1b[1'|",
    ]);
}

// ADR-1000: ENQ answers an empty answerback; the C reference answered ACK.
// covers: query.enq
#[test]
fn enq_answers_nothing() {
    let mut t = term(8, 2, b"");
    assert_eq!(reply(&mut t, b"\x05"), b"");
}

// ctlseqs, XTMODKEYS and XTQMODKEYS (aaronsb/libtsm#25): modifyOtherKeys's
// levels 0 to 2 are taken, level 3 is not, an omitted value is 0, and
// `CSI > m` and `CSI > 4 n` return it to 0. The query answers the
// XTMODKEYS that restores the level.
// covers: input.modify-other-keys
#[test]
fn xtmodkeys_sets_and_reports_the_level() {
    let mut t = term(8, 2, b"");
    assert_eq!(reply(&mut t, b"\x1b[?4m"), b"\x1b[>4;0m");
    for (set, level) in [
        (&b"\x1b[>4;2m"[..], &b"2"[..]),
        (b"\x1b[>4;3m", b"2"),
        (b"\x1b[>4;1m", b"1"),
        (b"\x1b[>4m", b"0"),
        (b"\x1b[>4;2m\x1b[>m", b"0"),
        (b"\x1b[>4;2m\x1b[>4n", b"0"),
        (b"\x1b[>4;1m\x1b[>1;2m", b"1"),
        (b"\x1b[>4:1;2m", b"1"),
    ] {
        t.input(set);
        let want = [&b"\x1b[>4;"[..], level, b"m"].concat();
        assert_eq!(reply(&mut t, b"\x1b[?4m"), want, "{set:?}");
    }
    assert_eq!(reply(&mut t, b"\x1b[?1m"), b"");
}

// ctlseqs and VT510, DECSCUSR: 0 the host's style, 1 to 6 a shape that
// blinks or stays steady; a value past 6 is ignored, as xterm ignores it
// (aaronsb/libtsm#78, the operator's decision). DECSC does not save the
// style.
// covers: cursor.style
#[test]
fn decscusr_sets_the_cursor_style() {
    let mut t = term(8, 2, b"");
    for (ps, style) in [
        (1, CursorStyle::BlockBlink),
        (2, CursorStyle::BlockSteady),
        (3, CursorStyle::UnderlineBlink),
        (4, CursorStyle::UnderlineSteady),
        (5, CursorStyle::BarBlink),
        (6, CursorStyle::BarSteady),
        (0, CursorStyle::Default),
    ] {
        t.input(format!("\x1b[{ps} q").as_bytes());
        assert_eq!(t.cursor_style(), style, "DECSCUSR {ps}");
    }
    t.input(b"\x1b[4 q\x1b[7 q");
    assert_eq!(t.cursor_style(), CursorStyle::UnderlineSteady);
    t.input(b"\x1b[ q");
    assert_eq!(t.cursor_style(), CursorStyle::Default);
    t.input(b"\x1b[6 q\x1b7\x1b[2 q\x1b8");
    assert_eq!(t.cursor_style(), CursorStyle::BlockSteady);
}

// VT510, DECSC and DECRC: the cursor, the wrap pending at it, the pen, and
// the autowrap and origin modes, one slot for each screen as xterm keeps
// them; DECRC with nothing saved goes home with the default pen.
// covers: ctl.decsc
#[test]
fn decsc_saves_and_decrc_restores_the_cursor_state() {
    let mut t = term(6, 4, b"\x1b8");
    assert_eq!(t.screen().cursor(), (0, 0));
    t.input(b"\x1b[2;3H\x1b[1;32m\x1b[?7l\x1b7\x1b[H\x1b[m\x1b[?7h\x1b8");
    assert_eq!(t.screen().cursor(), (2, 1));
    assert!(t.screen().pen().attrs.contains(Attrs::BOLD));
    assert!(!t.screen().auto_wrap());
    // The wrap pending at the cursor comes back with it.
    t.input(b"\x1b[?7h\x1b[1;1Habcdef\x1b7\x1b[H\x1b8g");
    assert_eq!(rows(t.screen())[..2], ["abcdef", "g"]);
    // Origin mode comes back, and the cursor to where it was on the screen.
    t.input(b"\x1b[2;3r\x1b[?6h\x1b[2;1H\x1b7\x1b[?6l\x1b[H\x1b8");
    assert!(t.screen().origin_mode());
    assert_eq!(t.screen().cursor(), (0, 2));
    // The alternate screen keeps a slot of its own.
    t.input(b"\x1b[?6l\x1b[r\x1b[4;5H\x1b7\x1b[?47h\x1b[1;1H\x1b7\x1b[?47l\x1b8");
    assert_eq!(t.screen().cursor(), (4, 3));
}

// ctlseqs, DECKPAM and DECKPNM: the keypad sends application sequences, or
// its characters.
// covers: ctl.deckpam
#[test]
fn deckpam_switches_the_keypad() {
    let mut t = term(8, 2, b"\x1b=");
    for (sym, app) in [
        (0xffb5, &b"\x1bOu"[..]),
        (0xffab, b"\x1bOk"),
        (0xff8d, b"\x1bOM"),
    ] {
        t.key(sym, 0, Mods::empty(), None);
        assert_eq!(t.take_replies(), app);
    }
    t.input(b"\x1b>");
    for (sym, plain) in [(0xffb5, &b"5"[..]), (0xffab, b"+"), (0xff8d, b"\r")] {
        t.key(sym, 0, Mods::empty(), None);
        assert_eq!(t.take_replies(), plain);
    }
}

// VT510, DECSCL: a soft reset, then 7-bit input for a VT100 level and
// 8-bit input for VT200 to VT400, as the C reference took it; a key then
// sends what the input can carry.
// covers: ctl.decscl
#[test]
fn decscl_resets_and_sets_the_input() {
    let mut t = term(8, 2, b"\x1b[4h\x1b[61\"p");
    assert!(!t.screen().insert_mode());
    // 7-bit input drops each byte's top bit, so UTF-8's é is "C)".
    t.input(b"\xc3\xa9");
    assert_eq!(rows(t.screen())[0], "C)");
    t.key(0xe9, 0, Mods::empty(), Some('\u{e9}'));
    assert_eq!(t.take_replies(), b"?");
    t.input(b"\x1b[62;1\"p\r\x1b[K\xe9");
    assert_eq!(rows(t.screen())[0], "\u{e9}");
    t.key(0xe9, 0, Mods::empty(), Some('\u{e9}'));
    assert_eq!(t.take_replies(), b"\xe9");
    t.input(b"\x9b2J\x9bH\x9b!p\xc3\xa9");
    assert_eq!(rows(t.screen())[0], "\u{e9}");
}

// DECRQSS for DECSCL reports the level DECSCL set, 62 until one is set and
// after DECSTR, and Pc 1: the replies stay 7-bit whatever DECSCL asked.
// covers: query.decrqss
#[test]
fn decrqss_reports_the_conformance_level() {
    let mut t = term(8, 2, b"");
    let ask = b"\x1bP$q\"p\x1b\\";
    assert_eq!(reply(&mut t, ask), b"\x1bP1$r62;1\"p\x1b\\");
    for (set, level) in [
        (&b"\x1b[61\"p"[..], &b"61"[..]),
        (b"\x1b[63;0\"p", b"63"),
        (b"\x1b[64;2\"p", b"64"),
        (b"\x1b[62;1\"p", b"62"),
        (b"\x1b[64\"p\x1b[!p", b"62"),
    ] {
        t.input(set);
        let want = [&b"\x1bP1$r"[..], level, b";1\"p\x1b\\"].concat();
        assert_eq!(reply(&mut t, ask), want, "{set:?}");
    }
}

// ctlseqs, S7C1T and S8C1T: taken, and the replies stay 7-bit, as the C
// reference sent them; an 8-bit C1 control acts in 8-bit input.
// covers: text.c1
#[test]
fn c1_controls_and_their_announcers() {
    let mut t = term(8, 2, b"\x1b G");
    assert_eq!(reply(&mut t, b"\x1b[c"), b"\x1b[?62;22c");
    t.input(b"\x1b F\x1b[62\"p");
    assert_eq!(reply(&mut t, b"\x9b6n"), b"\x1b[1;1R");
}

// ctlseqs: XTFMTKEYS and XTQFMTKEYS, DECUDK's user-defined keys (refused:
// program output would program a key that later types into the shell),
// and XTHIMOUSE (refused with mode 1001) change nothing; XTHIMOUSE's five
// parameters are not SD's one.
// covers: input.format-other-keys, input.decudk, input.xthimouse
#[test]
fn key_formats_user_defined_keys_and_hilite_tracking_are_not_taken() {
    inert(&[
        b"\x1b[>4;1f",
        b"\x1b[?4f",
        b"\x1bP0;1|17/6c73\x1b\\",
        b"\x1b[1;2;3;4;5T",
    ]);
    let mut t = busy();
    t.input(b"\x1bP0;1|17/6c73\x1b\\");
    t.key(0xffc4, 0, Mods::SHIFT, None);
    assert_eq!(t.take_replies(), b"\x1b[31~");
}

// ADR-1000: kitty's remote control (refused: program output would drive
// the terminal), tmux's control mode, ReGIS and DECDLD soft fonts change
// nothing.
// covers: dcs.kitty-cmd, dcs.tmux, image.regis, image.decdld
#[test]
fn control_strings_vtium_does_not_take_change_nothing() {
    inert(&[
        b"\x1bP@kitty-cmd{\"cmd\":\"close-window\"}\x1b\\",
        b"\x1bP1000p%begin\x1b\\",
        b"\x1bPpP[100,100]V[200,200]\x1b\\",
        b"\x1bP1;1;1;10;0;2;20;0{ @???~~~~/??~~~~\x1b\\",
    ]);
}

// ADR-1000: the OSC strings vtium ignores or refuses change nothing and get
// no reply, their queries included: the title and icon name, X properties
// and Sun's title strings, the special and Tektronix colours, a colour in a
// wide-gamut space, XTQALLOWED and its kin, the working directory,
// notifications and progress, iTerm2's and the applications' own strings,
// the log file, and kitty's clipboard, file transfer and drag and drop.
// covers: window.title, window.xprop, colour.special, colour.tektronix
// covers: colour.wide-gamut, query.xtqallowed, text.contour-fonts, osc.7
// covers: osc.notify, osc.9.4, osc.1337, osc.app-specific, osc.46
// covers: osc.5522, osc.5113, osc.72
#[test]
fn the_osc_strings_vtium_does_not_take_change_nothing() {
    inert(&[
        b"\x1b]0;title\x07",
        b"\x1b]1;icon\x1b\\",
        b"\x1b]2;title\x1b\\",
        b"\x1b]3;WM_NAME=x\x07",
        b"\x1b]ltitle\x1b\\",
        b"\x1b]Llabel\x1b\\",
        b"\x1b]Ifile.xpm\x1b\\",
        b"\x1b]5;0;rgb:ff/00/00\x07",
        b"\x1b]5;0;?\x07",
        b"\x1b]6;1;1\x07",
        b"\x1b]105;0\x07",
        b"\x1b]106;1\x07",
        b"\x1b]15;?\x07",
        b"\x1b]16;rgb:00/ff/00\x07",
        b"\x1b]18;?\x1b\\",
        b"\x1b]115\x07",
        b"\x1b]116\x07",
        b"\x1b]118\x07",
        b"\x1b]4;1;oklch(0.6 0.2 30)\x07",
        b"\x1b]11;lab(50% 40 59)\x07",
        b"\x1b]60\x07",
        b"\x1b]61\x07",
        b"\x1b]62\x07",
        b"\x1b]60;font=Mono;size=12\x1b\\",
        b"\x1b]7;file://host/tmp\x1b\\",
        b"\x1b]9;done\x07",
        b"\x1b]777;notify;title;body\x07",
        b"\x1b]99;i=1:p=?;\x1b\\",
        b"\x1b]99;;hello\x1b\\",
        b"\x1b]9;4;1;50\x07",
        b"\x1b]1337;SetUserVar=a=Yg==\x07",
        b"\x1b]1337;CurrentDir=/tmp\x07",
        b"\x1b]51;[\"call\"]\x07",
        b"\x1b]176;app\x07",
        b"\x1b]555\x07",
        b"\x1b]3008;start=1;type=shell\x1b\\",
        b"\x1b]7501;x\x07",
        b"\x1b]46;/tmp/log\x07",
        b"\x1b]5522;type=read:mime=dGV4dC9wbGFpbg==\x1b\\",
        b"\x1b]5522;type=write\x1b\\",
        b"\x1b]5113;ac=send;id=x\x1b\\",
        b"\x1b]72;t=o\x1b\\",
    ]);
}

// ADR-1000: DECST8C, kitty's screen-to-scrollback (`CSI 22 J`) and
// unscroll (`CSI + T`), the status line, the page size, the LEDs and the
// bell volumes are ignored.
// covers: ctl.decst8c, ctl.kitty-clear, ctl.kitty-unscroll, ctl.status-line
// covers: ctl.page-size, ctl.decll, ctl.bell-volume
#[test]
fn the_ignored_control_functions_change_nothing() {
    inert(&[
        b"\x1b[?5W",
        b"\x1b[22J",
        b"\x1b[3+T",
        b"\x1b[2$~",
        b"\x1b[1$}",
        b"\x1b[132$|",
        b"\x1b[36t",
        b"\x1b[48*|",
        b"\x1b[1q",
        b"\x1b[0q",
        b"\x1b[5 t",
        b"\x1b[5 u",
    ]);
}

// The Linux console's `CSI 16 ; Pn ]`, the cursor's blink interval, as the
// C reference took it.
#[test]
fn the_cursor_blink_interval_is_kept_for_the_host() {
    let t = term(8, 2, b"\x1b[16;250]");
    assert_eq!(t.cursor_blink_ms(), Some(250));
}

// docs/architecture.md, "Determinism", bounds: the replies the host has not
// taken are bounded. The input call returns after the byte whose reply
// reached the bound, and goes on once the host takes them, so a program
// that queries without reading cannot grow them.
#[test]
fn replies_waiting_for_the_host_are_bounded() {
    let caps: [(&[u8], Option<&[u8]>); 1] = [(b"TN", Some(&[b'v'; 256]))];
    let mut t = term(8, 2, b"");
    t.set_termcap(Termcap::new(caps).unwrap());
    let query = b"\x1bP+q544e\x1b\\";
    let stream = query.repeat(1000);
    let mut fed = 0;
    let mut rounds = 0;
    while fed < stream.len() {
        fed += t.input(&stream[fed..]);
        assert!(t.replies().len() < vtium_term::REPLIES_PENDING_MAX + 524);
        let replies = t.take_replies();
        assert_eq!(replies.len() % 524, 0);
        rounds += 1;
    }
    assert!(rounds > 1, "the bound never paused the input");
}

// vtium-parse's README, "Notes for vtium-term": an OSC 4 the parser
// truncated is refused whole, though each query it kept is complete, so a
// list past the OSC bound gets no reply.
#[test]
fn a_truncated_osc_4_list_gets_no_reply() {
    let mut seq = b"\x1b]4".to_vec();
    while seq.len() <= vtium_parse::OSC_MAX + 8 {
        seq.extend_from_slice(b";1;?");
    }
    seq.push(0x07);
    let mut t = term(8, 2, b"");
    t.input(&seq);
    assert_eq!(t.take_replies(), b"");
}

// The same bound inside one string: an OSC 4 at the OSC bound holds about
// 260,000 queries, whose replies would be 7 MiB, so the list's queries
// stop at the reply that reaches the bound, as a stream of queries does.
#[test]
fn an_osc_4_list_stops_at_the_replies_bound() {
    let mut seq = b"\x1b]4".to_vec();
    while seq.len() < vtium_parse::OSC_MAX - 8 {
        seq.extend_from_slice(b";1;?");
    }
    seq.push(0x07);
    let mut t = term(8, 2, b"");
    t.input(&seq);
    let replies = t.take_replies();
    let one = b"\x1b]4;1;rgb:cdcd/0000/0000\x07";
    assert!(replies.len() >= vtium_term::REPLIES_PENDING_MAX);
    assert!(replies.len() < vtium_term::REPLIES_PENDING_MAX + one.len());
    assert!(replies.chunks(one.len()).all(|r| r == one));
}

// The same bound, for a host that calls the input again without taking the
// replies: at the bound the call takes nothing, so the replies stop at the
// one that reached it.
#[test]
fn input_at_the_bound_takes_nothing_until_the_replies_are_taken() {
    let caps: [(&[u8], Option<&[u8]>); 1] = [(b"TN", Some(&[b'v'; 256]))];
    let mut t = term(8, 2, b"");
    t.set_termcap(Termcap::new(caps).unwrap());
    let stream = b"\x1bP+q544e\x1b\\".repeat(1000);
    let mut fed = 0;
    for _ in 0..1000 {
        fed += t.input(&stream[fed..]);
        assert!(t.replies().len() < vtium_term::REPLIES_PENDING_MAX + 524);
    }
    let held = t.replies().len();
    assert!(held >= vtium_term::REPLIES_PENDING_MAX);
    assert_eq!(t.input(&stream[fed..]), 0);
    assert_eq!(t.replies().len(), held);
    t.take_replies();
    assert!(t.input(&stream[fed..]) > 0);
}
