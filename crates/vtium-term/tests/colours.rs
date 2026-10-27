//! The colours a program sets, queries and resets: the palette (OSC 4 and
//! 104), the defaults and the cursor, pointer and selection colours (OSC 10
//! to 19 and 110 to 119), and kitty's colour protocol (OSC 21), driven
//! through the byte stream.
//!
//! Each expected reply is xterm's (ctlseqs, "Operating System Commands")
//! or kitty's (`color-stack`), as the crate's README lists. The reply
//! rule's test, with hostile text in every field, is the unit test in
//! `src/terminal/reply.rs`.

mod common;

use common::term;
use vtium_term::{Age, Palette, Rgb, Special, Terminal};

fn reply(t: &mut Terminal, query: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(query);
    t.take_replies()
}

/// The colour OSC 4 sets index 1 to for `spec`, `None` when it sets
/// nothing.
fn parsed(spec: &str) -> Option<Rgb> {
    let mut t = term(4, 1, b"");
    let before = t.palette().color(1);
    t.input(format!("\x1b]4;1;{spec}\x1b\\").as_bytes());
    let after = t.palette().color(1);
    (after != before).then_some(after)
}

// ctlseqs, OSC 4: `index ; spec` pairs set palette colours, each spec an
// X11 colour specification; a query after a set answers the new colour,
// and text already written in that colour recolours.
// covers: colour.palette.set
#[test]
fn osc_4_sets_palette_colours() {
    let mut t = term(4, 1, b"\x1b[31mx");
    t.input(b"\x1b]4;1;rgb:12/34/56;196;#abcdef\x07");
    assert_eq!(t.palette().color(1), Rgb::new(0x12, 0x34, 0x56));
    assert_eq!(t.palette().color(196), Rgb::new(0xab, 0xcd, 0xef));
    assert_eq!(
        reply(&mut t, b"\x1b]4;1;?;196;?\x07"),
        b"\x1b]4;1;rgb:1212/3434/5656\x07\x1b]4;196;rgb:abab/cdcd/efef\x07"
    );
    let palette = t.palette().clone();
    let frame = t.screen().frame(&palette, Age::NEVER);
    assert_eq!(frame.cell(0, 0).unwrap().fg, Rgb::new(0x12, 0x34, 0x56));
    // a set and a query in one list, an index past 255 and a spec vtium
    // does not read are skipped, and the list goes on
    assert_eq!(
        reply(&mut t, b"\x1b]4;2;rgb:00/00/01;300;#ffffff;3;red;2;?\x1b\\"),
        b"\x1b]4;2;rgb:0000/0000/0101\x1b\\"
    );
    assert_eq!(t.palette().color(3), Palette::default().color(3));
}

// X11's XParseColor, which ctlseqs names: `rgb:` with one to four hex
// digits a channel, scaled to the channel's range, and `#` with one to
// four a channel, taken as its most significant bits, either case.
#[test]
fn colour_specs_are_read_as_x11_reads_them() {
    let rgb = |r, g, b| Some(Rgb::new(r, g, b));
    assert_eq!(parsed("rgb:12/34/56"), rgb(0x12, 0x34, 0x56));
    assert_eq!(parsed("rgb:f/8/0"), rgb(0xff, 0x88, 0x00));
    assert_eq!(parsed("rgb:abc/123/fff"), rgb(0xab, 0x12, 0xff));
    assert_eq!(parsed("rgb:1234/5678/9abc"), rgb(0x12, 0x56, 0x9a));
    assert_eq!(parsed("RGB:AB/cD/eF"), rgb(0xab, 0xcd, 0xef));
    assert_eq!(parsed("rgb:12/3/456"), rgb(0x12, 0x33, 0x45));
    assert_eq!(parsed("#123"), rgb(0x10, 0x20, 0x30));
    assert_eq!(parsed("#123456"), rgb(0x12, 0x34, 0x56));
    assert_eq!(parsed("#123456789"), rgb(0x12, 0x45, 0x78));
    assert_eq!(parsed("#123456789ABC"), rgb(0x12, 0x56, 0x9a));
    for bad in [
        "",
        "rgb:",
        "rgb:12/34",
        "rgb:12/34/56/78",
        "rgb:12345/0/0",
        "rgb:/0/0",
        "rgb:1g/0/0",
        "rgb: 1/2/3",
        "rgb:+1/2/3",
        "#",
        "#12",
        "#1234",
        "#1234567",
        "#12345g",
        "red",
        "rgbi:1/0/0",
        "oklch(0.6 0.2 30)",
    ] {
        assert_eq!(parsed(bad), None, "{bad:?}");
    }
}

// ctlseqs, OSC 104: each index listed returns to the configured palette's
// colour, and no index returns them all.
// covers: colour.palette.set
#[test]
fn osc_104_resets_palette_colours_to_the_configured_palette() {
    let nord = Palette::named("nord").unwrap();
    let mut t = term(4, 1, b"");
    t.set_palette(nord.clone());
    t.input(b"\x1b]4;1;#010101;2;#020202;3;#030303\x07");
    t.input(b"\x1b]104;1;3\x07");
    assert_eq!(t.palette().color(1), nord.color(1));
    assert_eq!(t.palette().color(2), Rgb::new(2, 2, 2));
    assert_eq!(t.palette().color(3), nord.color(3));
    t.input(b"\x1b]104;x;2;999\x07");
    assert_eq!(t.palette().color(2), nord.color(2));
    t.input(b"\x1b]4;1;#010101;255;#020202\x07\x1b]104\x1b\\");
    assert_eq!(t.palette(), &nord);
    t.input(b"\x1b]4;7;#010101\x07\x1b]104;\x07");
    assert_eq!(t.palette(), &nord);
}

// ctlseqs, OSC 10, 11 and 12: the default foreground and background and
// the cursor colour, each set and queried; a list goes on to the next
// number. The cursor colour unset answers the foreground, as xterm's
// cursorColor defaults to it.
// covers: colour.dynamic.set
#[test]
fn osc_10_11_and_12_set_and_query() {
    let mut t = term(4, 1, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]12;?\x1b\\"),
        b"\x1b]12;rgb:e5e5/e5e5/e5e5\x1b\\"
    );
    t.input(b"\x1b]10;#102030\x07\x1b]11;rgb:40/50/60\x07\x1b]12;#708090\x07");
    assert_eq!(t.palette().fg(), Rgb::new(0x10, 0x20, 0x30));
    assert_eq!(t.palette().bg(), Rgb::new(0x40, 0x50, 0x60));
    assert_eq!(t.palette().cursor(), Some(Rgb::new(0x70, 0x80, 0x90)));
    let palette = t.palette().clone();
    assert_eq!(
        t.screen().frame(&palette, Age::NEVER).cursor().color,
        Some(Rgb::new(0x70, 0x80, 0x90))
    );
    assert_eq!(
        reply(&mut t, b"\x1b]10;?;?;?\x07"),
        [
            &b"\x1b]10;rgb:1010/2020/3030\x07"[..],
            b"\x1b]11;rgb:4040/5050/6060\x07",
            b"\x1b]12;rgb:7070/8080/9090\x07",
        ]
        .concat()
    );
    // a list from 11 sets 11 and 12, and a field vtium does not read
    // leaves its colour
    t.input(b"\x1b]11;#000001;#000002\x07\x1b]10;red;#000003\x07");
    assert_eq!(t.palette().fg(), Rgb::new(0x10, 0x20, 0x30));
    assert_eq!(t.palette().bg(), Rgb::new(0, 0, 3));
    assert_eq!(t.palette().cursor(), Some(Rgb::new(0, 0, 2)));
}

// ctlseqs, OSC 110, 111 and 112: each returns its colour to the
// configured one; the cursor's is unset, drawing inverted.
// covers: colour.dynamic.set
#[test]
fn osc_110_111_and_112_reset_to_the_configured_palette() {
    let vga = Palette::named("vga").unwrap();
    let mut t = term(4, 1, b"");
    t.set_palette(vga.clone());
    t.input(b"\x1b]10;#010101;#020202;#030303\x07");
    t.input(b"\x1b]110\x07");
    assert_eq!(t.palette().fg(), vga.fg());
    assert_eq!(t.palette().bg(), Rgb::new(2, 2, 2));
    t.input(b"\x1b]111\x1b\\\x1b]112;\x07");
    assert_eq!(t.palette(), &vga);
    assert_eq!(t.palette().cursor(), None);
}

// ctlseqs, OSC 13 and 14: the pointer's foreground and background, unset
// following the defaults; OSC 113 and 114 reset them. The host draws the
// pointer in `Palette::pointer`.
// covers: colour.pointer
#[test]
fn osc_13_and_14_set_query_and_reset_the_pointer_colours() {
    let mut t = term(4, 1, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]13;?;?\x07"),
        b"\x1b]13;rgb:e5e5/e5e5/e5e5\x07\x1b]14;rgb:0000/0000/0000\x07"
    );
    t.input(b"\x1b]13;#ff0000;#00ff00\x07");
    assert_eq!(
        t.palette().pointer(),
        (Rgb::new(0xff, 0, 0), Rgb::new(0, 0xff, 0))
    );
    assert_eq!(
        reply(&mut t, b"\x1b]14;?\x1b\\"),
        b"\x1b]14;rgb:0000/ffff/0000\x1b\\"
    );
    t.input(b"\x1b]113\x07");
    assert_eq!(t.palette().special(Special::PointerFg), None);
    assert_eq!(
        t.palette().special(Special::PointerBg),
        Some(Rgb::new(0, 0xff, 0))
    );
    t.input(b"\x1b]114\x07");
    assert_eq!(t.palette(), &Palette::default());
}

/// The colours of the selected cell at column 0 and the plain one at
/// column 1, of a terminal whose first cell is selected.
fn selection_colours(t: &mut Terminal) -> ((Rgb, Rgb), (Rgb, Rgb)) {
    t.screen_mut().select_start(0, 0);
    t.screen_mut().select_to(0, 0);
    let palette = t.palette().clone();
    let frame = t.screen().frame(&palette, Age::NEVER);
    let selected = frame.cell(0, 0).unwrap();
    let plain = frame.cell(0, 1).unwrap();
    assert!(selected.selected && !plain.selected);
    ((selected.fg, selected.bg), (plain.fg, plain.bg))
}

// ctlseqs, OSC 17 and 19: the selection's background and foreground.
// Unset, a selected cell draws inverted; with a background set, its own
// foreground over it; with both set, both. OSC 117 and 119 reset them.
// covers: colour.selection
#[test]
fn osc_17_and_19_colour_the_selection() {
    let red = Rgb::new(0xcd, 0, 0);
    let bg = Rgb::new(0, 0, 0);
    let mut t = term(4, 1, b"\x1b[31mab");
    assert_eq!(selection_colours(&mut t), ((bg, red), (red, bg)));
    assert_eq!(
        reply(&mut t, b"\x1b]17;?\x07\x1b]19;?\x07"),
        b"\x1b]17;rgb:e5e5/e5e5/e5e5\x07\x1b]19;rgb:0000/0000/0000\x07"
    );
    let blue = Rgb::new(0, 0, 0xff);
    t.input(b"\x1b]17;#0000ff\x07");
    assert_eq!(selection_colours(&mut t), ((red, blue), (red, bg)));
    let green = Rgb::new(0, 0xff, 0);
    t.input(b"\x1b]19;#00ff00\x07");
    assert_eq!(selection_colours(&mut t), ((green, blue), (red, bg)));
    assert_eq!(
        reply(&mut t, b"\x1b]17;?;?;?\x07"),
        b"\x1b]17;rgb:0000/0000/ffff\x07\x1b]19;rgb:0000/ffff/0000\x07"
    );
    t.input(b"\x1b]117\x07");
    assert_eq!(selection_colours(&mut t), ((green, red), (red, bg)));
    t.input(b"\x1b]119\x07");
    assert_eq!(selection_colours(&mut t), ((bg, red), (red, bg)));
}

// ctlseqs: OSC 15, 16 and 18 are Tektronix's colours, which vtium has no
// mode for; a list through them skips their fields.
#[test]
fn a_list_skips_the_tektronix_colours() {
    let mut t = term(4, 1, b"");
    assert_eq!(
        reply(&mut t, b"\x1b]14;?;?;?;?;?;?\x07"),
        [
            &b"\x1b]14;rgb:0000/0000/0000\x07"[..],
            b"\x1b]17;rgb:e5e5/e5e5/e5e5\x07",
            b"\x1b]19;rgb:0000/0000/0000\x07",
        ]
        .concat()
    );
    t.input(b"\x1b]15;#010101;#020202;#030303\x07");
    assert_eq!(
        t.palette().special(Special::SelectionBg),
        Some(Rgb::new(3, 3, 3))
    );
    t.input(b"\x1b]117\x07");
    assert_eq!(t.palette(), &Palette::default());
}

// kitty's color-stack, OSC 21: `key=spec` sets, `key=?` queries in one
// reply, `key=` makes a dynamic colour dynamic (unset), and a bare key
// resets it; spaces are ignored. A key vtium does not know is left out of
// the reply, a departure from kitty, which names it in base64 under
// `unknown` (the reply rule).
// covers: colour.kitty-protocol
#[test]
fn osc_21_sets_queries_and_resets_by_key() {
    let mut t = term(4, 1, b"");
    t.input(
        b"\x1b]21;foreground=#010203; background = rgb:04/05/06;cursor=#070809;\
          selection_background=#0a0b0c;selection_foreground=#0d0e0f;7=#101112\x1b\\",
    );
    let p = t.palette();
    assert_eq!(p.fg(), Rgb::new(1, 2, 3));
    assert_eq!(p.bg(), Rgb::new(4, 5, 6));
    assert_eq!(p.cursor(), Some(Rgb::new(7, 8, 9)));
    assert_eq!(p.special(Special::SelectionBg), Some(Rgb::new(10, 11, 12)));
    assert_eq!(p.special(Special::SelectionFg), Some(Rgb::new(13, 14, 15)));
    assert_eq!(p.color(7), Rgb::new(16, 17, 18));
    assert_eq!(
        reply(
            &mut t,
            b"\x1b]21;foreground=?;cursor=?;visual_bell=?;007=?;selection_foreground=?\x07"
        ),
        [
            &b"\x1b]21;foreground=rgb:0101/0202/0303;cursor=rgb:0707/0808/0909;"[..],
            b"7=rgb:1010/1111/1212;selection_foreground=rgb:0d0d/0e0e/0f0f\x07",
        ]
        .concat()
    );
    // dynamic: the cursor and the selection unset; the defaults and the
    // palette cannot be
    t.input(b"\x1b]21;cursor=;selection_background=;foreground=;7=\x1b\\");
    assert_eq!(t.palette().cursor(), None);
    assert_eq!(t.palette().special(Special::SelectionBg), None);
    assert_eq!(t.palette().fg(), Rgb::new(1, 2, 3));
    assert_eq!(t.palette().color(7), Rgb::new(16, 17, 18));
    assert_eq!(
        reply(&mut t, b"\x1b]21;cursor=?;selection_background=?\x1b\\"),
        b"\x1b]21;cursor=;selection_background=\x1b\\"
    );
    // a bare key resets to the configured palette's
    t.input(b"\x1b]21;foreground;background;7;selection_foreground\x1b\\");
    assert_eq!(t.palette(), &Palette::default());
    // nothing known asked, no reply
    assert_eq!(reply(&mut t, b"\x1b]21;visual_bell=?;256=?;x\x07"), b"");
}

// A colour a program sets recolours every cell drawn in it, so every cell
// is marked changed for a display that drew before it; a set that changes
// nothing marks nothing.
#[test]
fn a_set_marks_the_screen_changed() {
    let mut t = term(4, 2, b"\x1b[31mx");
    let since = t.screen_mut().checkpoint();
    let changed = |t: &Terminal, since| {
        let palette = t.palette().clone();
        let frame = t.screen().frame(&palette, since);
        (0..2).any(|row| frame.row_changed(row))
    };
    for set in [
        &b"\x1b]4;1;#cd0000\x07"[..],
        b"\x1b]10;#e5e5e5\x07",
        b"\x1b]12;?\x07",
        b"\x1b]110\x07",
        b"\x1b]104\x07",
    ] {
        t.input(set);
        assert!(!changed(&t, since), "{set:?}");
    }
    for set in [
        &b"\x1b]4;1;#010101\x07"[..],
        b"\x1b]10;#010101\x07",
        b"\x1b]11;#010101\x07",
        b"\x1b]17;#010101\x07",
        b"\x1b]19;#010101\x07",
        b"\x1b]21;3=#010101\x07",
        b"\x1b]104\x07",
    ] {
        let since = t.screen_mut().checkpoint();
        t.input(set);
        assert!(changed(&t, since), "{set:?}");
    }
}

// xterm's ReallyReset (charproc.c, xterm 412) resets the OSC 4 palette on
// DECSTR and RIS alike, and neither resets the colours OSC 10 to 19 set:
// vtium's DECSTR follows it. vtium's RIS returns every colour to the
// configured palette, a departure the crate's README lists: the C
// reference's RIS also ended on the configured colours, and RIS returns
// the terminal to its initial state.
#[test]
fn decstr_resets_the_palette_and_ris_every_colour() {
    let nord = Palette::named("nord").unwrap();
    let mut t = term(4, 1, b"");
    t.set_palette(nord.clone());
    t.input(b"\x1b]4;1;#010101\x07\x1b]10;#020202;#030303;#040404\x07\x1b]17;#050505\x07");
    t.input(b"\x1b[!p");
    assert_eq!(t.palette().color(1), nord.color(1));
    assert_eq!(t.palette().fg(), Rgb::new(2, 2, 2));
    assert_eq!(t.palette().bg(), Rgb::new(3, 3, 3));
    assert_eq!(t.palette().cursor(), Some(Rgb::new(4, 4, 4)));
    assert_eq!(
        t.palette().special(Special::SelectionBg),
        Some(Rgb::new(5, 5, 5))
    );
    t.input(b"\x1b]4;1;#010101\x07\x1bc");
    assert_eq!(t.palette(), &nord);
}

// docs/architecture.md, "Determinism": what a stream sets does not depend
// on when the host took the replies. A list whose queries reach the
// replies' bound skips the queries past it and still applies its sets, so
// a terminal whose replies wait at the bound ends with the palette of one
// whose replies were taken.
#[test]
fn a_list_at_the_replies_bound_still_sets() {
    let query = b"\x1b]4;1;?\x07";
    let one = reply(&mut term(4, 1, b""), query).len();
    let k = (vtium_term::REPLIES_PENDING_MAX - 1) / one;
    let lists: [&[u8]; 2] = [
        b"\x1b]4;1;?;2;?;3;#ff0000\x07",
        b"\x1b]21;1=?;2=?;3=#ff0000;foreground=#00ff00;4\x07",
    ];
    for list in lists {
        let mut waiting = term(4, 1, b"\x1b]4;4;#010101\x07");
        waiting.input(&query.repeat(k));
        assert!(waiting.replies().len() + one >= vtium_term::REPLIES_PENDING_MAX);
        assert_eq!(waiting.input(list), list.len());
        let mut taken = term(4, 1, b"\x1b]4;4;#010101\x07");
        taken.input(&query.repeat(k));
        taken.take_replies();
        assert_eq!(taken.input(list), list.len());
        assert_eq!(taken.palette().color(3), Rgb::new(0xff, 0, 0));
        assert_eq!(waiting.palette(), taken.palette(), "{list:?}");
    }
}

// The host's palette replaces the one a program changed.
#[test]
fn the_hosts_palette_replaces_the_programs_changes() {
    let mut t = term(4, 1, b"\x1b]4;1;#010101\x07\x1b]12;#020202\x07");
    let solarized = Palette::named("solarized").unwrap();
    t.set_palette(solarized.clone());
    assert_eq!(t.palette(), &solarized);
    t.input(b"\x1b]4;1;#010101\x07\x1b]104\x07");
    assert_eq!(t.palette(), &solarized);
}

// docs/architecture.md, "Determinism", bounds: an OSC 21 at the OSC bound
// holds about 200,000 queries, so its one reply skips the queries past the
// field that brings the replies waiting for the host to the bound.
#[test]
fn an_osc_21_reply_stops_at_the_replies_bound() {
    let mut seq = b"\x1b]21".to_vec();
    while seq.len() < vtium_parse::OSC_MAX - 8 {
        seq.extend_from_slice(b";1=?");
    }
    seq.push(0x07);
    let mut t = term(4, 1, b"");
    t.input(&seq);
    let replies = t.take_replies();
    let field = b";1=rgb:cdcd/0000/0000";
    assert!(replies.len() >= vtium_term::REPLIES_PENDING_MAX);
    assert!(replies.len() < vtium_term::REPLIES_PENDING_MAX + 2 * field.len());
    assert!(replies.starts_with(b"\x1b]21;1=rgb:cdcd/0000/0000;1=rgb"));
    assert!(replies.ends_with(b";1=rgb:cdcd/0000/0000\x07"));
}
