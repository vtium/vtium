//! The in-band reports a program opts into, so it hears of a change
//! without polling: the text area's size (mode 2048), the console's
//! visibility (mode 2033 and `CSI ? 998 n`) and its color scheme (mode
//! 2031 and `CSI ? 996 n`). Each report is sent while its mode is set, and
//! only on a change; the queries answer whatever the modes hold.
//!
//! The C reference had none of them: it told a program of a resize only by
//! the SIGWINCH that TIOCSWINSZ sends (kmscon `src/terminal.c:1149-1163`,
//! `src/misc/pty.c:955`).

mod common;

use common::term;
use vtium_term::{Palette, Terminal};

/// DECRQM's answer for a DEC private mode.
fn rqm(t: &mut Terminal, n: u32) -> u8 {
    t.take_replies();
    t.input(format!("\x1b[?{n}$p").as_bytes());
    let reply = String::from_utf8(t.take_replies()).unwrap();
    let want = format!("\x1b[?{n};");
    assert!(
        reply.starts_with(&want) && reply.ends_with("$y"),
        "DECRQM ?{n} replied {reply:?}"
    );
    reply[want.len()..reply.len() - 2].parse().unwrap()
}

fn sent(t: &mut Terminal) -> String {
    String::from_utf8(t.take_replies()).unwrap()
}

// rockorager's in-band resize notifications (foot(7), mode 2048): set, the
// mode reports the text area at once, `CSI 48 ; rows ; cols ; height px ;
// width px t`, and again after each resize that changes it; a set while
// set reports again; pixels are 0 while the cell size is unknown.
// covers: DEC-2048
#[test]
fn mode_2048_reports_the_text_area_on_set_and_each_resize() {
    let mut t = term(10, 5, b"");
    assert_eq!(rqm(&mut t, 2048), 2);
    t.input(b"\x1b[?2048h");
    assert_eq!(sent(&mut t), "\x1b[48;5;10;0;0t");
    t.set_cell_size(8, 16);
    assert_eq!(sent(&mut t), "", "the cell size waits for the resize");
    t.resize(12, 6);
    assert_eq!(sent(&mut t), "\x1b[48;6;12;96;96t");
    t.resize(12, 6);
    assert_eq!(sent(&mut t), "", "an unchanged size reports nothing");
    t.set_cell_size(10, 20);
    t.resize(12, 6);
    assert_eq!(sent(&mut t), "\x1b[48;6;12;120;120t", "a new font");
    t.input(b"\x1b[?2048h");
    assert_eq!(sent(&mut t), "\x1b[48;6;12;120;120t", "set again");
    assert_eq!(rqm(&mut t, 2048), 1);
    t.input(b"\x1b[?2048l");
    assert_eq!(rqm(&mut t, 2048), 2);
    t.resize(20, 10);
    assert_eq!(sent(&mut t), "", "reset, a resize reports nothing");
    t.input(b"\x1b[?2048h");
    assert_eq!(sent(&mut t), "\x1b[48;10;20;200;200t");
}

// foot(7), mode 2033 and `CSI ? 998 n` (rockorager): set, the mode reports
// the visibility at once, `CSI ? 999 ; 1 n` potentially visible and `2`
// not, and again on each change; the query answers whatever the mode.
// covers: DEC-2033
#[test]
fn mode_2033_reports_visibility_on_set_and_each_change() {
    let mut t = term(10, 5, b"");
    assert_eq!(rqm(&mut t, 2033), 2);
    t.input(b"\x1b[?998n");
    assert_eq!(sent(&mut t), "\x1b[?999;1n");
    t.set_visible(false);
    assert_eq!(sent(&mut t), "", "reset, a change reports nothing");
    t.input(b"\x1b[?998n");
    assert_eq!(sent(&mut t), "\x1b[?999;2n");
    t.input(b"\x1b[?2033h");
    assert_eq!(sent(&mut t), "\x1b[?999;2n");
    assert_eq!(rqm(&mut t, 2033), 1);
    t.set_visible(true);
    assert_eq!(sent(&mut t), "\x1b[?999;1n");
    t.set_visible(true);
    assert_eq!(sent(&mut t), "", "no change, no report");
    t.set_visible(false);
    assert_eq!(sent(&mut t), "\x1b[?999;2n");
    t.input(b"\x1b[?2033l");
    assert_eq!(rqm(&mut t, 2033), 2);
    t.set_visible(true);
    assert_eq!(sent(&mut t), "");
}

// contour's color palette update notifications (mode 2031): set, the
// mode sends nothing until the default background changes between dark
// and light, by OSC 11, OSC 111, OSC 21 or the host's palette; then
// `CSI ? 997 ; 1 n` dark or `2` light. A change that keeps the scheme, and
// a palette color, report nothing.
// covers: DEC-2031
#[test]
fn mode_2031_reports_a_change_of_scheme() {
    let mut t = term(10, 5, b"");
    assert_eq!(rqm(&mut t, 2031), 2);
    t.input(b"\x1b]11;#ffffff\x07");
    assert_eq!(sent(&mut t), "", "reset, a change reports nothing");
    t.input(b"\x1b]111\x07\x1b[?2031h");
    assert_eq!(sent(&mut t), "", "set, nothing until a change");
    assert_eq!(rqm(&mut t, 2031), 1);
    t.input(b"\x1b]11;#ffffff\x07");
    assert_eq!(sent(&mut t), "\x1b[?997;2n");
    t.input(b"\x1b]11;#eeeeee\x07\x1b]4;0;#ffffff\x07\x1b]10;#000000\x07");
    assert_eq!(sent(&mut t), "", "still light");
    t.input(b"\x1b]111\x1b\\");
    assert_eq!(sent(&mut t), "\x1b[?997;1n");
    t.input(b"\x1b]21;background=#f0f0f0\x1b\\");
    assert_eq!(sent(&mut t), "\x1b[?997;2n");
    t.input(b"\x1b]21;background\x07");
    assert_eq!(sent(&mut t), "\x1b[?997;1n");
    t.set_palette(Palette::named("base16-light").unwrap());
    assert_eq!(sent(&mut t), "\x1b[?997;2n", "the configured palette");
    t.set_palette(Palette::named("base16-light").unwrap());
    assert_eq!(sent(&mut t), "");
    t.input(b"\x1b[?2031l");
    assert_eq!(rqm(&mut t, 2031), 2);
    t.input(b"\x1b]11;#000000\x07");
    assert_eq!(sent(&mut t), "");
}

// A pop of the color stack, XTPOPCOLORS or kitty's OSC 30101, that
// restores a background of the other scheme is a change of scheme: one
// report a sequence, however many entries it pops.
// covers: DEC-2031
#[test]
fn a_color_stack_pop_reports_a_change_of_scheme() {
    let mut t = term(10, 5, b"\x1b[?2031h\x1b]11;#ffffff\x07");
    assert_eq!(sent(&mut t), "\x1b[?997;2n");
    t.input(b"\x1b[#P\x1b]11;#000000\x07");
    assert_eq!(sent(&mut t), "\x1b[?997;1n");
    t.input(b"\x1b[#Q");
    assert_eq!(sent(&mut t), "\x1b[?997;2n", "XTPOPCOLORS");
    t.input(b"\x1b[#P\x1b]11;#eeeeee\x07\x1b[#Q");
    assert_eq!(sent(&mut t), "", "a pop within the scheme");
    t.input(b"\x1b]30001\x1b\\\x1b]11;#000000\x07");
    assert_eq!(sent(&mut t), "\x1b[?997;1n");
    t.input(b"\x1b]30101\x1b\\");
    assert_eq!(sent(&mut t), "\x1b[?997;2n", "OSC 30101");
    t.input(b"\x1b]11;#000000\x07\x1b[1#P\x1b]11;#ffffff\x07\x1b[2#P");
    assert_eq!(sent(&mut t), "\x1b[?997;1n\x1b[?997;2n");
    t.input(b"\x1b[1;2#Q");
    assert_eq!(
        sent(&mut t),
        "",
        "dark, then light again: no change at the end"
    );
    t.input(b"\x1b[2;1#Q");
    assert_eq!(sent(&mut t), "\x1b[?997;1n", "one report for two pops");
}

// contour, `CSI ? 996 n`: `CSI ? 997 ; 1 n` while the default background
// is dark and `2` while it is light, whatever mode 2031 holds. Dark is a
// luma below half, Y = 0.299 R + 0.587 G + 0.114 B on the stored channels
// (ITU-R BT.601), the rule neovim applies to OSC 11's answer.
// covers: color.scheme-report
#[test]
fn the_scheme_query_answers_by_the_backgrounds_luma() {
    let mut t = term(10, 5, b"");
    for (bg, want) in [
        ("#000000", 1),
        ("#ffffff", 2),
        ("#7f7f7f", 1),
        ("#808080", 2),
        ("#ff0000", 1),
        ("#00ff00", 2),
        ("#0000ff", 1),
        ("#ffff00", 2),
        ("#909000", 2),
        ("#8f8f00", 1),
        // Y = 127499 / 255000 and 127500 / 255000: either side of half.
        ("#02d125", 1),
        ("#00cc44", 2),
    ] {
        t.input(format!("\x1b]11;{bg}\x07\x1b[?996n").as_bytes());
        assert_eq!(sent(&mut t), format!("\x1b[?997;{want}n"), "{bg}");
    }
}

// DECSTR and RIS reset the three modes, as they reset mode 1004.
// covers: DEC-2031, DEC-2033, DEC-2048
#[test]
fn decstr_and_ris_reset_the_report_modes() {
    for reset in ["\x1b[!p", "\x1bc"] {
        let mut t = term(10, 5, b"\x1b[?2048h\x1b[?2033h\x1b[?2031h");
        t.take_replies();
        t.input(reset.as_bytes());
        for n in [2031, 2033, 2048] {
            assert_eq!(rqm(&mut t, n), 2, "{reset:?} {n}");
        }
        t.resize(12, 6);
        t.set_visible(false);
        t.input(b"\x1b]11;#ffffff\x07");
        assert_eq!(sent(&mut t), "", "{reset:?}");
    }
}

// docs/architecture.md, "Determinism": the reports a stream provokes do not
// depend on how the stream was cut.
#[test]
fn the_reports_are_the_same_however_the_stream_is_cut() {
    let stream: &[u8] = b"\x1b[?2048h\x1b[?2033h\x1b[?2031h\x1b]11;#ffffff\x07\x1b[?996n\
        \x1b[?998n\x1b]21;background=#101010\x1b\\\x1b[?2048h\x1b]111\x1b\\\x1b[?2031l\
        \x1b]11;#ffffff\x07\x1b[?996n";
    let mut whole = term(10, 5, b"");
    whole.set_cell_size(8, 16);
    whole.input(stream);
    let mut each = term(10, 5, b"");
    each.set_cell_size(8, 16);
    for b in stream {
        each.input(&[*b]);
    }
    let want = "\x1b[48;5;10;80;80t\x1b[?999;1n\x1b[?997;2n\x1b[?997;2n\x1b[?999;1n\
                \x1b[?997;1n\x1b[48;5;10;80;80t\x1b[?997;2n";
    assert_eq!(sent(&mut whole), want);
    assert_eq!(sent(&mut each), want);
}
