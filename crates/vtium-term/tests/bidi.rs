//! The paragraph state of right-to-left text (ADR-1004 D2, D5, D12, D13),
//! driven through the byte stream: BDSM, DCSM and modes 1243, 2500 and
//! 2501, SCP and SPD, the paragraph properties each row holds and how a
//! wrap, a scroll and a rewrap carry them, DECSTR and RIS, and
//! `text-direction`'s default. A paragraph is the rows a wrap joins.
//!
//! A build without the `bidi` feature has none of it: DECRQM answers 0 for
//! each bidi mode, SM and RM do nothing, and SCP and SPD are ignored.

mod common;

use common::term;
use vtium_term::{LineFlags, Terminal};

/// DECRQM's answer for a mode: 0 not recognised, 1 set, 2 reset, 3
/// permanently set, 4 permanently reset.
fn rqm(t: &mut Terminal, dec: bool, n: u32) -> u8 {
    t.take_replies();
    let q = if dec { "?" } else { "" };
    t.input(format!("\x1b[{q}{n}$p").as_bytes());
    let reply = String::from_utf8(t.take_replies()).unwrap();
    let want = format!("\x1b[{q}{n};");
    assert!(
        reply.starts_with(&want) && reply.ends_with("$y"),
        "DECRQM {q}{n} replied {reply:?}"
    );
    reply[want.len()..reply.len() - 2].parse().unwrap()
}

/// The paragraph properties each row of the screen holds, top to bottom.
#[cfg(feature = "bidi")]
fn paragraphs(t: &Terminal) -> Vec<LineFlags> {
    t.screen()
        .lines()
        .iter()
        .map(|l| l.flags().intersection(LineFlags::PARAGRAPH))
        .collect()
}

#[cfg(feature = "bidi")]
mod with_the_feature {
    use super::*;
    use common::rows;
    use vtium_term::TextDirection;

    const NONE: LineFlags = LineFlags::empty();
    const RTL: LineFlags = LineFlags::RTL;
    const EXPLICIT: LineFlags = LineFlags::EXPLICIT;

    // ADR-1004 D2, D12: BDSM is set by default, implicit mode. RM sets
    // explicit mode on every row of the cursor's paragraph, the rows a wrap
    // joins, and on each new row; SM returns them to implicit mode.
    // covers: ANSI-8
    #[test]
    fn bdsm_sets_explicit_mode_on_the_cursors_paragraph_and_each_new_row() {
        // Row 0 ends with a newline; rows 1 and 2 are one paragraph.
        let mut t = term(4, 5, b"ab\r\nabcdefg");
        assert_eq!(rqm(&mut t, false, 8), 1);
        t.input(b"\x1b[8l");
        assert_eq!(rqm(&mut t, false, 8), 2);
        assert_eq!(paragraphs(&t), [NONE, EXPLICIT, EXPLICIT, NONE, NONE]);
        // A line feed at the bottom brings a new row, which takes the
        // current values.
        t.input(b"\x1b[5H\n");
        assert_eq!(paragraphs(&t), [EXPLICIT, EXPLICIT, NONE, NONE, EXPLICIT]);
        t.input(b"\x1b[1;1H\x1b[8h");
        assert_eq!(rqm(&mut t, false, 8), 1);
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE, NONE, EXPLICIT]);
    }

    // ADR-1004 D12: DCSM is permanently set, since editing always acts on
    // the data, and SM and RM change nothing.
    // covers: ANSI-9
    #[test]
    fn dcsm_answers_permanently_set_and_changes_nothing() {
        let mut t = term(4, 3, b"abcdef");
        let before = format!("{t:?}");
        for seq in [&b"\x1b[9l"[..], b"\x1b[9h"] {
            t.input(seq);
            assert_eq!(rqm(&mut t, false, 9), 3);
            assert_eq!(format!("{t:?}"), before);
        }
    }

    // ADR-1004 D11, D12: mode 1243 is set by default, and SM and RM set
    // and reset it. The arrow keys it swaps are the frame's work.
    // covers: DEC-1243
    #[test]
    fn mode_1243_is_set_by_default_and_toggles() {
        let mut t = term(4, 3, b"");
        assert_eq!(rqm(&mut t, true, 1243), 1);
        t.input(b"\x1b[?1243l");
        assert_eq!(rqm(&mut t, true, 1243), 2);
        t.input(b"\x1b[?1243h");
        assert_eq!(rqm(&mut t, true, 1243), 1);
    }

    // ADR-1004 D2, D7, D12: mode 2500 is reset by default; SM marks the
    // cursor's paragraph and each new row as mirroring box drawing.
    // covers: DEC-2500
    #[test]
    fn mode_2500_marks_box_drawing_mirrored_on_the_cursors_paragraph() {
        let mut t = term(4, 3, b"abcdef");
        assert_eq!(rqm(&mut t, true, 2500), 2);
        t.input(b"\x1b[?2500h");
        assert_eq!(rqm(&mut t, true, 2500), 1);
        let mirror = LineFlags::BOX_MIRROR;
        assert_eq!(paragraphs(&t), [mirror, mirror, NONE]);
        t.input(b"\x1b[?2500l");
        assert_eq!(rqm(&mut t, true, 2500), 2);
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);
    }

    // ADR-1004 D2, D5, D12: mode 2501, direction detected, follows
    // `text-direction`: reset by default, set under `auto`, and DECSTR
    // returns it there. SM and RM mark the cursor's paragraph.
    // covers: DEC-2501
    #[test]
    fn mode_2501_marks_detection_and_starts_as_text_direction_says() {
        let detect = LineFlags::DETECT;
        let mut t = term(4, 3, b"ab");
        assert_eq!(rqm(&mut t, true, 2501), 2);
        t.input(b"\x1b[?2501h");
        assert_eq!(rqm(&mut t, true, 2501), 1);
        assert_eq!(paragraphs(&t), [detect, NONE, NONE]);
        t.input(b"\x1b[?2501l");
        assert_eq!(rqm(&mut t, true, 2501), 2);
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);

        let mut t = Terminal::new(4, 3, 100);
        t.set_text_direction(TextDirection::Auto);
        assert_eq!(rqm(&mut t, true, 2501), 1);
        assert_eq!(paragraphs(&t), [detect, detect, detect]);
        t.input(b"\x1b[?2501l");
        assert_eq!(rqm(&mut t, true, 2501), 2);
        t.input(b"\x1b[!p");
        assert_eq!(rqm(&mut t, true, 2501), 1);
    }

    // ADR-1004 D2, D5: SCP 1 and 2 set the cursor's paragraph left to right
    // and right to left; SCP 0 and SCP with no parameter return it to the
    // direction `text-direction` gives. Any other first parameter is
    // ignored, and so is the second parameter.
    // covers: ctl.scp
    #[test]
    fn scp_sets_the_paragraph_direction_and_0_restores_the_default() {
        let mut t = term(4, 3, b"abcdef");
        t.input(b"\x1b[2 k");
        assert_eq!(t.screen().paragraph(), RTL);
        assert_eq!(paragraphs(&t), [RTL, RTL, NONE]);
        t.input(b"\x1b[1 k");
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);
        t.input(b"\x1b[2;1 k");
        assert_eq!(paragraphs(&t), [RTL, RTL, NONE]);
        for ignored in [&b"\x1b[3 k"[..], b"\x1b[9 k", b"\x1b[1:2 k"] {
            t.input(ignored);
            assert_eq!(paragraphs(&t), [RTL, RTL, NONE], "{ignored:?}");
        }
        t.input(b"\x1b[0 k");
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);
        t.input(b"\x1b[2 k\x1b[ k");
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);

        // Under `text-direction = rtl` the default is right to left.
        let mut t = Terminal::new(4, 3, 100);
        t.set_text_direction(TextDirection::Rtl);
        assert_eq!(paragraphs(&t), [RTL, RTL, RTL]);
        t.input(b"\x1b[1 k");
        assert_eq!(paragraphs(&t), [NONE, RTL, RTL]);
        t.input(b"\x1b[ k");
        assert_eq!(paragraphs(&t), [RTL, RTL, RTL]);
    }

    // ADR-1004 D2: SPD 0 and 3 are aliases of SCP 1 and 2, as the
    // recommendation allows and mintty documents; SPD with no parameter is
    // SPD 0, and the other values are ignored.
    // covers: ctl.spd
    #[test]
    fn spd_0_and_3_are_scp_1_and_2_and_the_rest_are_ignored() {
        let mut t = Terminal::new(4, 2, 100);
        t.set_text_direction(TextDirection::Rtl);
        t.input(b"\x1b[0 S");
        assert_eq!(paragraphs(&t), [NONE, RTL]);
        t.input(b"\x1b[3 S");
        assert_eq!(paragraphs(&t), [RTL, RTL]);
        t.input(b"\x1b[ S");
        assert_eq!(paragraphs(&t), [NONE, RTL]);
        t.input(b"\x1b[3 S");
        for ignored in [&b"\x1b[1 S"[..], b"\x1b[2 S", b"\x1b[4 S", b"\x1b[7 S"] {
            t.input(ignored);
            assert_eq!(paragraphs(&t), [RTL, RTL], "{ignored:?}");
        }
    }

    // ADR-1004 D2: a wrap carries the paragraph's values to the row it
    // starts, both into a row already on the screen and into the row a
    // scroll brings, where a new row would otherwise take the current
    // values.
    // covers: text.bidi
    #[test]
    fn a_wrap_carries_the_paragraphs_values_to_the_row_it_starts() {
        // Row 0 is set right to left alone; printing past its end wraps
        // into row 1, which was made left to right.
        let mut t = term(4, 3, b"ab\x1b[2 k");
        assert_eq!(paragraphs(&t), [RTL, NONE, NONE]);
        t.input(b"cdef");
        assert_eq!(paragraphs(&t), [RTL, RTL, NONE]);

        // The current values are right to left, and the last row's
        // paragraph left to right: the row its wrap scrolls in is left to
        // right.
        let mut t = term(4, 3, b"\x1b[2 k\x1b[3Habcdef");
        assert_eq!(rows(t.screen()), ["", "abcd", "ef"]);
        assert_eq!(paragraphs(&t), [NONE, NONE, NONE]);
        // A line feed's new row takes the current values.
        t.input(b"\n");
        assert_eq!(paragraphs(&t), [NONE, NONE, RTL]);
    }

    // ADR-1004 D2: rows a scroll pushes into the scrollback keep their
    // values, and a change of the cursor's paragraph reaches its rows in
    // the scrollback, and no row of another paragraph there.
    // covers: text.bidi
    #[test]
    fn a_paragraph_in_the_scrollback_keeps_and_takes_its_values() {
        let mut t = term(4, 2, b"");
        t.screen_mut().set_scrollback_max(10);
        t.input(b"xy\r\n\x1b[2 kab\r\n\x1b[1 kabcdefghij");
        let scrolled: Vec<LineFlags> = t
            .screen()
            .scrollback()
            .map(|l| l.flags().intersection(LineFlags::PARAGRAPH))
            .collect();
        assert_eq!(scrolled, [NONE, RTL, NONE]);
        t.input(b"\x1b[2 k");
        let scrolled: Vec<LineFlags> = t
            .screen()
            .scrollback()
            .map(|l| l.flags().intersection(LineFlags::PARAGRAPH))
            .collect();
        assert_eq!(scrolled, [NONE, RTL, RTL]);
        assert_eq!(paragraphs(&t), [RTL, RTL]);
    }

    // ADR-1004 D2, ADR-1003: a rewrap gives every row it makes the values
    // of the paragraph's first row, narrower and wider.
    // covers: text.bidi
    #[test]
    fn a_rewrap_gives_every_row_the_paragraphs_values() {
        let mut t = term(8, 6, b"ab\r\n\x1b[2 kabcdefghijkl\r\n\x1b[1 kxy");
        assert_eq!(paragraphs(&t), [NONE, RTL, RTL, NONE, NONE, NONE]);
        t.resize(4, 6);
        assert_eq!(rows(t.screen()), ["ab", "abcd", "efgh", "ijkl", "xy", ""]);
        assert_eq!(paragraphs(&t), [NONE, RTL, RTL, RTL, NONE, NONE]);
        t.resize(12, 6);
        assert_eq!(rows(t.screen())[..3], ["ab", "abcdefghijkl", "xy"]);
        assert_eq!(paragraphs(&t)[..3], [NONE, RTL, NONE]);
    }

    // ADR-1003: a rewrap removes the blank rows it pushes below the screen
    // whatever paragraph properties they hold, which every new row takes,
    // so growing the screen afterwards brings new rows, holding the current
    // values, and not the old blank rows.
    #[test]
    fn a_rewrap_removes_blank_rows_whatever_their_paragraph() {
        // Every row is a new right-to-left row; then the text's rows, and
        // the current values, are set left to right.
        let mut t = term(
            6,
            4,
            b"\x1b[2 k\x1b[4H\n\n\n\n\x1b[Habcdefghij\x1b[2H\x1b[1 k",
        );
        assert_eq!(paragraphs(&t), [NONE, NONE, RTL, RTL]);
        t.resize(3, 4);
        assert_eq!(rows(t.screen()), ["abc", "def", "ghi", "j"]);
        t.resize(3, 6);
        assert_eq!(rows(t.screen())[4..], ["", ""]);
        assert_eq!(paragraphs(&t)[4..], [NONE, NONE]);
    }

    // ADR-1004 D2: DECSTR and RIS return the current values to their
    // defaults and apply them to the cursor's paragraph, as SM and RM
    // would; the defaults follow `text-direction`. RIS applies them again
    // once it has moved the cursor home, so the row the program writes next
    // holds them too.
    // covers: ctl.decstr, ANSI-8, DEC-1243, DEC-2500, DEC-2501, ctl.scp
    #[test]
    fn decstr_and_ris_return_the_paragraph_to_its_defaults() {
        let changes = b"\x1b[8l\x1b[?1243l\x1b[?2500h\x1b[?2501h\x1b[2 k";
        for (reset, what) in [(&b"\x1b[!p"[..], "DECSTR"), (b"\x1bc", "RIS")] {
            for (direction, default) in [(TextDirection::Ltr, NONE), (TextDirection::Rtl, RTL)] {
                let mut t = Terminal::new(4, 3, 100);
                t.set_text_direction(direction);
                // Row 0 in explicit mode alone, then rows 1 and 2, one
                // paragraph, changed every way, the cursor on row 2.
                t.input(b"\x1b[8l\x1b[2Habcdef");
                t.input(changes);
                assert!(paragraphs(&t).iter().all(|&p| p != default), "{what}");
                t.input(reset);
                assert_eq!(rqm(&mut t, false, 8), 1, "{what}");
                assert_eq!(rqm(&mut t, true, 1243), 1, "{what}");
                assert_eq!(rqm(&mut t, true, 2500), 2, "{what}");
                assert_eq!(rqm(&mut t, true, 2501), 2, "{what}");
                assert_eq!(t.screen().paragraph(), default, "{what}");
                let after = paragraphs(&t);
                if what == "DECSTR" {
                    assert_ne!(after[0], default, "{what} {direction:?}");
                    assert_eq!(after[1..], [default, default], "{what} {direction:?}");
                } else {
                    assert_eq!(after, [default; 3], "{what} {direction:?}");
                }
            }
        }
    }

    // ADR-1004 D5: `text-direction` sets the default and the rows nothing
    // has been written in; a row already written keeps its values.
    // covers: text.bidi
    #[test]
    fn text_direction_changes_no_row_already_written() {
        let mut t = term(4, 3, b"ab");
        t.set_text_direction(TextDirection::Rtl);
        assert_eq!(paragraphs(&t), [NONE, RTL, RTL]);
        assert_eq!(t.screen().paragraph(), RTL);
        t.set_text_direction(TextDirection::Auto);
        assert_eq!(paragraphs(&t), [NONE, LineFlags::DETECT, LineFlags::DETECT]);
    }
}

// ADR-1004 D13: a build without the bidi feature has no bidi mode: DECRQM
// answers 0 for BDSM, DCSM, 1243, 2500 and 2501, and SM, RM, SCP and SPD
// change nothing.
// covers: ANSI-8, ANSI-9, DEC-1243, DEC-2500, DEC-2501, ctl.scp, ctl.spd
#[cfg(not(feature = "bidi"))]
#[test]
fn a_build_without_bidi_answers_0_and_changes_nothing() {
    let mut t = term(4, 3, b"abcdef");
    let before = format!("{t:?}");
    for (dec, n) in [
        (false, 8),
        (false, 9),
        (true, 1243),
        (true, 2500),
        (true, 2501),
    ] {
        let q = if dec { "?" } else { "" };
        for seq in ["h", "l", "h"] {
            t.input(format!("\x1b[{q}{n}{seq}").as_bytes());
            assert_eq!(rqm(&mut t, dec, n), 0, "{q}{n}{seq}");
        }
    }
    for seq in ["\x1b[2 k", "\x1b[1 k", "\x1b[ k", "\x1b[3 S", "\x1b[0 S"] {
        t.input(seq.as_bytes());
    }
    assert_eq!(format!("{t:?}"), before);
    let flags = t
        .screen()
        .lines()
        .iter()
        .map(|l| l.flags().intersection(LineFlags::PARAGRAPH));
    assert!(flags.into_iter().all(|f| f == LineFlags::empty()));
}
