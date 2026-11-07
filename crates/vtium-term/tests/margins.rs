//! Left and right margins (ADR-1000's `DEC-69`, `ctl.decslrm` and
//! `ctl.decic`): mode 69 (DECLRMM), DECSLRM and its `CSI s` shared with
//! SCOSC, DECRQSS's report, and each printing, cursor, editing, erase,
//! scroll and origin-mode function inside the margins, with DECIC, DECDC,
//! DECBI and DECFI and the rectangle operations in origin mode.
//!
//! Each case states the specification its expectation follows: xterm's
//! ctlseqs (<https://invisible-island.net/xterm/ctlseqs/ctlseqs.html>), the
//! VT510 manual, and where both leave a case open, xterm 411 run as a black
//! box, which printed the same screens for every stream here.

mod common;

use common::{rows, scrollback, term};
use vtium_term::{LineFlags, Terminal};

/// Ten columns by five rows, each row its own characters, written with
/// CUP so no row wraps.
const FILL: &[u8] =
    b"\x1b[1;1H0123456789\x1b[2;1Habcdefghij\x1b[3;1HABCDEFGHIJ\x1b[4;1Hklmnopqrst\x1b[5;1HKLMNOPQRST";

/// FILL with the left and right margins at columns 3 and 7, the third to
/// the seventh: 2 to 6 counted from 0.
fn filled() -> Terminal {
    let mut t = term(10, 5, FILL);
    t.input(b"\x1b[?69h\x1b[3;7s");
    t
}

/// FILL with the margins of `filled` and the top and bottom margins at
/// rows 2 and 4.
fn region() -> Terminal {
    let mut t = filled();
    t.input(b"\x1b[2;4r");
    t
}

fn reply(t: &mut Terminal, query: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(query);
    t.take_replies()
}

fn screen(t: &Terminal) -> Vec<String> {
    rows(t.screen())
}

// ctlseqs, DECLRMM and DECSLRM: while mode 69 is reset, `CSI s` with no
// parameter is SCOSC; while it is set, `CSI Pl ; Pr s` sets the margins and
// homes the cursor, `CSI s` sets them to the whole width, and margins with
// fewer than two columns are ignored, as xterm ignores them. Resetting mode
// 69 returns the margins to the whole width and leaves the cursor; DECSTR
// resets the mode (VT510 manual, DECSTR) and DECRQM reports it.
// covers: DEC-69, ctl.decslrm
#[test]
fn mode_69_makes_csi_s_set_margins_and_resetting_it_clears_them() {
    let mut t = term(10, 5, b"\x1b[2;3H\x1b[s\x1b[H");
    assert_eq!(t.screen().lr_margins(), (0, 9));
    assert_eq!(reply(&mut t, b"\x1b[?69$p"), b"\x1b[?69;2$y");
    t.input(b"\x1b[?69h\x1b[3;3H\x1b[3;7s");
    assert_eq!(reply(&mut t, b"\x1b[?69$p"), b"\x1b[?69;1$y");
    assert_eq!(t.screen().lr_margins(), (2, 6));
    assert_eq!(t.screen().cursor(), (0, 0));
    // An inverted pair, and a pair one column wide, change nothing.
    t.input(b"\x1b[4;4H\x1b[7;3s\x1b[5;5s");
    assert_eq!(t.screen().lr_margins(), (2, 6));
    assert_eq!(t.screen().cursor(), (3, 3));
    // A right margin past the screen is its last column; 0 or omitted is
    // the last column, and an omitted left margin the first.
    t.input(b"\x1b[2;99s");
    assert_eq!(t.screen().lr_margins(), (1, 9));
    t.input(b"\x1b[;4s");
    assert_eq!(t.screen().lr_margins(), (0, 3));
    // `CSI s` with no parameter is DECSLRM now, and saves nothing: SCORC
    // still finds the cursor SCOSC saved before mode 69 was set.
    t.input(b"\x1b[4;4H\x1b[s");
    assert_eq!(t.screen().lr_margins(), (0, 9));
    assert_eq!(t.screen().cursor(), (0, 0));
    t.input(b"\x1b[u");
    assert_eq!(t.screen().cursor(), (2, 1));
    // Resetting mode 69 clears the margins, and the cursor stays.
    t.input(b"\x1b[3;7s\x1b[4;5H\x1b[?69l");
    assert_eq!(t.screen().lr_margins(), (0, 9));
    assert_eq!(t.screen().cursor(), (4, 3));
    // With mode 69 reset, a parameter list makes `CSI s` do nothing.
    t.input(b"\x1b[3;7s");
    assert_eq!(t.screen().lr_margins(), (0, 9));
    assert_eq!(t.screen().cursor(), (4, 3));
    // DECSTR resets mode 69 and the margins.
    t.input(b"\x1b[?69h\x1b[3;7s\x1b[!p");
    assert!(!t.screen().lr_margin_mode());
    assert_eq!(t.screen().lr_margins(), (0, 9));
}

// ctlseqs, DECRQSS: `s` reports DECSLRM's margins, the whole width while
// DECLRMM is reset, as xterm reports them.
// covers: query.decrqss, ctl.decslrm
#[test]
fn decrqss_reports_the_left_and_right_margins() {
    let mut t = term(10, 5, b"");
    assert_eq!(reply(&mut t, b"\x1bP$qs\x1b\\"), b"\x1bP1$r1;10s\x1b\\");
    t.input(b"\x1b[?69h\x1b[3;7s");
    assert_eq!(reply(&mut t, b"\x1bP$qs\x1b\\"), b"\x1bP1$r3;7s\x1b\\");
}

// VT510 manual, DECAWM, and xterm's `dotext`: printed text wraps at the
// right margin to the left margin of the next row; from left of the left
// margin it still wraps at the right margin; from right of the right margin
// it wraps at the screen's last column. A wrap at the bottom margin scrolls
// only the cells between the margins, and only from inside them. No row is
// marked as continuing, since none wraps from the last column to the next
// row's first.
// covers: ctl.decslrm
#[test]
fn printing_wraps_at_the_right_margin_to_the_left_margin() {
    let mut t = term(10, 5, b"\x1b[?69h\x1b[3;7sabcdefghijk");
    assert_eq!(screen(&t)[..2], ["abcdefg", "  hijk"]);
    assert_eq!(t.screen().cursor(), (6, 1));
    // Right of the right margin, the wrap is at the screen's edge.
    t.input(b"\x1b[3;9Hxyz");
    assert_eq!(screen(&t)[2..4], ["        xy", "  z"]);
    // Text that fills the margins leaves a pending wrap on the margin's
    // column.
    t.input(b"\x1b[4;3HABCDE");
    assert_eq!(t.screen().cursor(), (6, 3));
    assert!(t.screen().margin_wrap());
    t.input(b"F");
    assert_eq!(screen(&t)[3..5], ["  ABCDE", "  F"]);
    for row in 0..5 {
        let line = t.screen().line(row).expect("a row");
        assert!(!line.flags().contains(LineFlags::WRAPPED), "row {row}");
    }
    // At the bottom margin the wrap scrolls the margins' cells.
    let mut t = term(10, 3, b"0123456789\x1b[2;1Habcdefghij\x1b[3;1HABCDEFGHIJ");
    t.input(b"\x1b[?69h\x1b[3;7s\x1b[3;3Hvwxyz12");
    assert_eq!(screen(&t), ["01cdefg789", "abvwxyzhij", "AB12   HIJ"]);
    assert_eq!(t.screen().scrollback().len(), 0);
}

// ctlseqs and xterm's `CursorForward`, `CursorBack`, `CarriageReturn`,
// `TabToNextStop` and `TabToPrevStop`; VT510 manual, CUF, CUB, CR and HT:
// CUF stops at the right margin, or at the last column from right of it; HT
// stops at the right margin, from right of it too, as xterm moves; CUB, BS
// and CR stop at the left margin, or at the first column from left of it;
// CBT stops at the left margin only in origin mode.
// covers: ctl.decslrm
#[test]
fn cursor_movement_stops_at_the_margins() {
    let mut t = filled();
    let mut at = |seq: &[u8]| {
        t.input(seq);
        t.screen().cursor()
    };
    assert_eq!(at(b"\x1b[1;5H\x1b[9C"), (6, 0));
    assert_eq!(at(b"\x1b[1;9H\x1b[9C"), (9, 0));
    assert_eq!(at(b"\x1b[1;5H\x1b[9D"), (2, 0));
    assert_eq!(at(b"\x1b[1;2H\x1b[9D"), (0, 0));
    assert_eq!(at(b"\x1b[1;3H\x08"), (2, 0));
    assert_eq!(at(b"\x1b[1;5H\r"), (2, 0));
    assert_eq!(at(b"\x1b[1;2H\r"), (0, 0));
    assert_eq!(at(b"\x1b[1;1H\t"), (6, 0));
    assert_eq!(at(b"\x1b[1;8H\t"), (6, 0));
    assert_eq!(at(b"\x1b[1;9H\t"), (6, 0));
    assert_eq!(at(b"\x1b[1;6H\x1b[Z"), (0, 0));
    assert_eq!(at(b"\x1b[?6h\x1b[1;4H\x1b[Z"), (2, 0));
}

// ctlseqs, DECOM, and xterm's `CursorSet`, `CursorCol` and the CPR it
// replies: in origin mode CUP, CHA, HPA and VPA count columns from the left
// margin and stay inside the margins; DECSTBM and DECSLRM home the cursor
// to the region's top left corner; CPR counts from it.
// covers: ctl.decslrm
#[test]
fn origin_mode_counts_columns_from_the_left_margin() {
    let mut t = region();
    t.input(b"\x1b[?6h");
    assert_eq!(t.screen().cursor(), (2, 1));
    t.input(b"\x1b[9;99H");
    assert_eq!(t.screen().cursor(), (6, 3));
    t.input(b"\x1b[2;3H");
    assert_eq!(t.screen().cursor(), (4, 2));
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[2;3R");
    t.input(b"\x1b[2G");
    assert_eq!(t.screen().cursor(), (3, 2));
    t.input(b"\x1b[9`");
    assert_eq!(t.screen().cursor(), (6, 2));
    t.input(b"\x1b[1d");
    assert_eq!(t.screen().cursor(), (6, 1));
    t.input(b"\x1b[2;3r");
    assert_eq!(t.screen().cursor(), (2, 1));
    t.input(b"\x1b[2;3H\x1b[4;8s");
    assert_eq!(t.screen().cursor(), (3, 1));
    // Outside origin mode, CPR counts from the screen's first column.
    t.input(b"\x1b[?6l\x1b[2;5H");
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[2;5R");
}

// VT510 manual, IND, RI, NEL and LF, and xterm's `xtermIndex` and
// `RevIndex`: at the bottom margin, a line feed between the left and right
// margins scrolls the cells between them; outside the margins it does not
// scroll, and the cursor stays. RI at the top margin scrolls down the same
// way. Nothing enters the scrollback.
// covers: ctl.decslrm
#[test]
fn line_feeds_scroll_only_between_the_margins() {
    let mut t = region();
    t.screen_mut().set_scrollback_max(10);
    t.input(b"\x1b[4;4H\n");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abCDEFGhij",
            "ABmnopqHIJ",
            "kl     rst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (3, 3));
    assert_eq!(scrollback(t.screen()).len(), 0);
    let mut t = region();
    t.input(b"\x1b[4;9H\n\x1bD");
    assert_eq!(screen(&t), rows(term(10, 5, FILL).screen()));
    assert_eq!(t.screen().cursor(), (8, 3));
    t.input(b"\x1b[2;4H\x1bM");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "ab     hij",
            "ABcdefgHIJ",
            "klCDEFGrst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (3, 1));
    let mut t = region();
    t.input(b"\x1b[4;5H\x1bE");
    assert_eq!(screen(&t)[3], "kl     rst");
    assert_eq!(t.screen().cursor(), (2, 3));
}

// ctlseqs, SU and SD, and xterm's `xtermScroll` and `RevScroll`: the region
// between all four margins scrolls, wherever the cursor is.
// covers: ctl.decslrm
#[test]
fn su_and_sd_scroll_the_region_between_the_margins() {
    let mut t = region();
    t.input(b"\x1b[5;10H\x1b[S");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abCDEFGhij",
            "ABmnopqHIJ",
            "kl     rst",
            "KLMNOPQRST"
        ]
    );
    let mut t = region();
    t.input(b"\x1b[2T");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "ab     hij",
            "AB     HIJ",
            "klcdefgrst",
            "KLMNOPQRST"
        ]
    );
}

// VT510 manual, IL and DL, and xterm's `InsertLine` and `DeleteLine`: the
// rows from the cursor's to the bottom margin move, between the left and
// right margins only, and the cursor goes to the left margin; outside the
// margins nothing happens.
// covers: ctl.decslrm
#[test]
fn il_and_dl_move_the_cells_between_the_margins() {
    let mut t = region();
    t.input(b"\x1b[3;4H\x1b[L");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abcdefghij",
            "AB     HIJ",
            "klCDEFGrst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (2, 2));
    let mut t = region();
    t.input(b"\x1b[3;4H\x1b[M");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abcdefghij",
            "ABmnopqHIJ",
            "kl     rst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (2, 2));
    let fill = rows(term(10, 5, FILL).screen());
    for outside in [&b"\x1b[3;1H"[..], b"\x1b[3;9H", b"\x1b[5;4H"] {
        let mut t = region();
        t.input(outside);
        let at = t.screen().cursor();
        t.input(b"\x1b[L\x1b[M");
        assert_eq!(screen(&t), fill, "{outside:?}");
        assert_eq!(t.screen().cursor(), at, "{outside:?}");
    }
}

// VT510 manual, ICH and DCH, and xterm's `InsertChar` and `DeleteChar`: the
// cells from the cursor to the right margin move; outside the margins
// nothing happens.
// covers: ctl.decslrm
#[test]
fn ich_and_dch_move_the_cells_up_to_the_right_margin() {
    let mut t = filled();
    t.input(b"\x1b[2;4H\x1b[2@");
    assert_eq!(screen(&t)[1], "abc  dehij");
    t.input(b"\x1b[3;4H\x1b[2P");
    assert_eq!(screen(&t)[2], "ABCFG  HIJ");
    t.input(b"\x1b[4;4H\x1b[9P");
    assert_eq!(screen(&t)[3], "klm    rst");
    let fill = rows(term(10, 5, FILL).screen());
    for outside in [&b"\x1b[5;2H"[..], b"\x1b[5;9H"] {
        let mut t = filled();
        t.input(outside);
        t.input(b"\x1b[2@\x1b[P");
        assert_eq!(screen(&t), fill, "{outside:?}");
    }
}

// ctlseqs and xterm's `InsertChar`: in insert mode, text printed between
// the margins moves the cells up to the right margin, and text printed
// outside them moves nothing.
// covers: ctl.decslrm
#[test]
fn insert_mode_moves_the_cells_up_to_the_right_margin() {
    let mut t = filled();
    t.input(b"\x1b[4h\x1b[2;4HXY\x1b[3;9HZ");
    assert_eq!(screen(&t)[1..3], ["abcXYdehij", "ABCDEFGHZJ"]);
}

// VT510 manual, ED, EL and ECH: the erase functions clear whole rows' cells,
// whatever the margins.
// covers: ctl.decslrm
#[test]
fn erases_ignore_the_margins() {
    let mut t = region();
    t.input(b"\x1b[2;2H\x1b[8X\x1b[3;4H\x1b[K\x1b[4;4H\x1b[1K");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "a        j",
            "ABC",
            "    opqrst",
            "KLMNOPQRST"
        ]
    );
    t.input(b"\x1b[1;5H\x1b[J");
    assert_eq!(screen(&t), ["0123", "", "", "", ""]);
}

// VT510 manual, DECIC and DECDC, and xterm's `xtermColScroll`: columns go
// in and out at the cursor's column in each row from the top margin to the
// bottom, the columns up to the right margin moving; the cursor stays; and
// outside the region nothing happens. Without margins, DECIC and DECDC work
// on the whole screen.
// covers: ctl.decic
#[test]
fn decic_and_decdc_insert_and_delete_columns() {
    let mut t = region();
    t.input(b"\x1b[3;5H\x1b['}");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abcd efhij",
            "ABCD EFHIJ",
            "klmn oprst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (4, 2));
    let mut t = region();
    t.input(b"\x1b[3;5H\x1b[2'~");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abcdg  hij",
            "ABCDG  HIJ",
            "klmnq  rst",
            "KLMNOPQRST"
        ]
    );
    let fill = rows(term(10, 5, FILL).screen());
    for outside in [&b"\x1b[3;9H"[..], b"\x1b[1;5H"] {
        let mut t = region();
        t.input(outside);
        t.input(b"\x1b['}\x1b['~");
        assert_eq!(screen(&t), fill, "{outside:?}");
    }
    let mut t = term(10, 5, FILL);
    t.input(b"\x1b[1;9H\x1b['}");
    assert_eq!(
        screen(&t),
        [
            "01234567 8",
            "abcdefgh i",
            "ABCDEFGH I",
            "klmnopqr s",
            "KLMNOPQR S"
        ]
    );
}

// VT510 manual, DECBI and DECFI, and xterm's `xtermColIndex`: away from a
// margin the cursor moves one column; at the left margin DECBI moves the
// region's columns right, and at the right margin DECFI moves them left,
// the cursor staying.
// covers: ctl.decic
#[test]
fn decbi_and_decfi_move_the_cursor_or_the_columns() {
    let mut t = region();
    t.input(b"\x1b[3;5H\x1b6");
    assert_eq!(t.screen().cursor(), (3, 2));
    t.input(b"\x1b9\x1b9");
    assert_eq!(t.screen().cursor(), (5, 2));
    assert_eq!(screen(&t), rows(term(10, 5, FILL).screen()));
    t.input(b"\x1b[3;3H\x1b6");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "ab cdefhij",
            "AB CDEFHIJ",
            "kl mnoprst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (2, 2));
    let mut t = region();
    t.input(b"\x1b[3;7H\x1b9");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abdefg hij",
            "ABDEFG HIJ",
            "klnopq rst",
            "KLMNOPQRST"
        ]
    );
    assert_eq!(t.screen().cursor(), (6, 2));
    // Right of the right margin DECFI moves the cursor, up to the last
    // column.
    t.input(b"\x1b[3;9H\x1b9\x1b9");
    assert_eq!(t.screen().cursor(), (9, 2));
}

// ctlseqs and DEC STD 070, the rectangle operations in origin mode, as
// xterm's `limitedParseCol` reads their corners: columns count from the
// left margin and stay inside the margins, DECCRA's target too.
// covers: ctl.decslrm
#[test]
fn rectangles_in_origin_mode_count_from_the_left_margin() {
    let mut t = region();
    t.input(b"\x1b[?6h\x1b[120;1;1;2;2$x\x1b[42;3;4;3;99$x");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "abxxefghij",
            "ABxxEFGHIJ",
            "klmno**rst",
            "KLMNOPQRST"
        ]
    );
    t.input(b"\x1b[1;1;1;2;1;1;4$v");
    assert_eq!(screen(&t)[1], "abxxexxhij");
    t.input(b"\x1b[$z");
    assert_eq!(
        screen(&t),
        [
            "0123456789",
            "ab     hij",
            "AB     HIJ",
            "kl     rst",
            "KLMNOPQRST"
        ]
    );
}

// xterm's `scrollInMargins`: a wide character across a margin goes whole
// before the cells between the margins move, at the left margin and at the
// right.
// covers: ctl.decslrm
#[test]
fn a_wide_character_across_a_margin_goes_whole_when_the_region_scrolls() {
    let mut t = term(10, 3, "\x1b[2;2H\u{4E2D}x".as_bytes());
    t.input(b"\x1b[?69h\x1b[3;7s\x1b[S");
    assert_eq!(screen(&t), ["   x", "", ""]);
    let mut t = term(10, 3, "\x1b[1;7H\u{4E2D}x\x1b[2;1Habcdefghij".as_bytes());
    t.input(b"\x1b[?69h\x1b[3;7s\x1b[S");
    assert_eq!(screen(&t), ["  cdefg x", "ab     hij", ""]);
}

// ctlseqs, DECALN, as xterm's `CASE_DECALN` resets the margins, and a
// resize, which returns them to the whole screen: both leave mode 69 as it
// is.
// covers: ctl.decslrm
#[test]
fn decaln_and_a_resize_return_the_margins_to_the_whole_screen() {
    let mut t = filled();
    t.input(b"\x1b#8");
    assert_eq!(t.screen().lr_margins(), (0, 9));
    assert!(t.screen().lr_margin_mode());
    let mut t = filled();
    t.resize(12, 5);
    assert_eq!(t.screen().lr_margins(), (0, 11));
    assert!(t.screen().lr_margin_mode());
}

// VT510 manual, DECSC, which saves the wrap flag: a pending wrap at the
// right margin survives DECSC and DECRC, as xterm's `CursorSave` keeps it.
// covers: ctl.decslrm
#[test]
fn decsc_keeps_a_pending_wrap_at_the_right_margin() {
    let mut t = filled();
    t.input(b"\x1b[2;3HVWXYZ\x1b7\x1b[H\x1b8Q");
    assert_eq!(screen(&t)[1..3], ["abVWXYZhij", "ABQDEFGHIJ"]);
}
