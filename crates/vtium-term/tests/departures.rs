//! The screen's departures from the C reference, each with the case that
//! tells the two apart. The crate's README lists each with its reason and
//! the specification it follows.

mod common;

use common::{rows, term};
use vtium_term::{Age, Color, Palette, Rgb, Style, Terminal};

fn palette() -> Palette {
    Palette::new(
        [Rgb::new(9, 9, 9); 256],
        Rgb::new(0xe5, 0xe5, 0xe5),
        Rgb::new(0, 0, 0),
    )
}

/// The cells a display that drew at `since` sees changed, as (row, col).
fn changed(t: &Terminal, since: Age) -> Vec<(usize, usize)> {
    let palette = palette();
    let frame = t.screen().frame(&palette, since);
    (0..frame.rows())
        .flat_map(|r| (0..frame.cols()).map(move |c| (r, c)))
        .filter(|&(r, c)| frame.cell(r, c).is_some_and(|cell| cell.changed))
        .collect()
}

// aaronsb/libtsm#34: a scroll inside margins marked every cell of the
// screen. It marks the region's rows, and a status line outside it stays
// unchanged for the renderer.
#[test]
fn a_region_scroll_marks_only_the_region() {
    let mut t = term(4, 5, b"1\r\n2\r\n3\r\n4\r\n5\x1b[2;4r\x1b[4;1H");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\n");
    let rows_changed: Vec<usize> = {
        let mut r: Vec<usize> = changed(&t, seen).iter().map(|&(r, _)| r).collect();
        r.dedup();
        r
    };
    assert_eq!(rows_changed, [1, 2, 3]);
    // IL and DL are region scrolls too.
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b[3;1H\x1b[L");
    let mut r: Vec<usize> = changed(&t, seen).iter().map(|&(r, _)| r).collect();
    r.dedup();
    assert_eq!(r, [2, 3]);
}

// aaronsb/libtsm#47: cells past a scrollback line's end were drawn with the
// pen, so an SGR under background colour erase recoloured them with no
// change marked. They read in the default style whatever the pen, and a pen
// change alone marks nothing.
#[test]
fn a_pen_change_while_scrolled_back_leaves_the_padding_alone() {
    let mut t = Terminal::new(6, 2, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"ab\r\n\r\n\r\n");
    t.screen_mut().scroll_view_up(1);
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().set_pen(Style {
        bg: Color::Indexed(4),
        ..Style::DEFAULT
    });
    assert!(changed(&t, seen).is_empty());
    let palette = palette();
    let frame = t.screen().frame(&palette, seen);
    let pad = frame.cell(0, 4).unwrap();
    assert_eq!(pad.bg, Rgb::new(0, 0, 0));
    assert!(pad.default_bg);
    assert!(!pad.changed);
}

// The C reference added the region's top to the current row for CHA and
// VPR in origin mode (it passed an absolute row where a relative one was
// due), so each moved the cursor down. ECMA-48 8.3.9: CHA moves within
// the active line; VPR moves relative to the active line.
#[test]
fn cha_and_vpr_in_origin_mode_keep_to_the_cursors_row() {
    let mut t = term(8, 8, b"\x1b[3;6r");
    t.screen_mut().set_origin_mode(true);
    t.input(b"\x1b[2;1H\x1b[5G");
    assert_eq!(t.screen().cursor(), (4, 3));
    t.input(b"\x1b[e");
    assert_eq!(t.screen().cursor(), (4, 4));
}

// The C reference ran CUD, CUF, CUB, CNL, CPL, CHA, VPA, VPR, ED, EL, ECH,
// IL, DL, CBT, CHT and REP whatever private marker or intermediate the
// sequence carried. ECMA-48 5.4: the intermediates and final byte together
// identify the function, and a private marker makes the sequence private
// (vtium-parse's README, "Notes for vtium-term").
#[test]
fn a_marker_or_an_intermediate_makes_another_function() {
    let mut t = term(8, 4, b"\x1b[2;2H");
    for seq in [
        &b"\x1b[?B"[..],
        b"\x1b[>C",
        b"\x1b[ D",
        b"\x1b[?E",
        b"\x1b[!F",
        b"\x1b[?5G",
        b"\x1b[>3d",
        b"\x1b[ e",
        b"\x1b[?I",
        b"\x1b[ Z",
    ] {
        t.input(seq);
        assert_eq!(t.screen().cursor(), (1, 1), "{seq:?}");
    }
    let mut t = term(4, 2, b"abcd\r\nefgh\x1b[1;2H");
    for seq in [
        &b"\x1b[>J"[..],
        b"\x1b[ K",
        b"\x1b[?X",
        b"\x1b[>L",
        b"\x1b[ M",
        b"\x1b[?b",
    ] {
        t.input(seq);
        assert_eq!(rows(t.screen()), ["abcd", "efgh"], "{seq:?}");
    }
}

// HPA and HPR (ECMA-48 8.3.57, 8.3.59), which the C reference did not
// carry out.
#[test]
fn hpa_and_hpr_move_the_cursor() {
    let mut t = term(8, 2, b"\x1b[2;1H\x1b[5`");
    assert_eq!(t.screen().cursor(), (4, 1));
    t.input(b"\x1b[2a");
    assert_eq!(t.screen().cursor(), (6, 1));
    t.input(b"\x1b[9a");
    assert_eq!(t.screen().cursor(), (7, 1));
}

// The C reference wrote a printed DEL (0x7F) to the screen, where it joined
// the cell before as a character of no width. vt100.net: "terminals only
// claiming VT220 compatibility can always ignore 7F"; only a 96-character
// set in GL draws it (vtium-parse's README), and those come with
// vtium/vtium#13.
#[test]
fn a_printed_del_does_nothing() {
    let t = term(4, 1, b"a\x7fb");
    assert_eq!(rows(t.screen()), ["ab"]);
    assert_eq!(t.screen().cursor(), (2, 0));
}

// The C reference copied a space for the second column of every wide
// character in a selection, so copying "中x" gave "中 x".
#[test]
fn a_copied_wide_character_brings_no_space() {
    let mut t = term(6, 1, "\u{4E2D}x".as_bytes());
    let s = t.screen_mut();
    s.select_start(0, 0);
    s.select_to(2, 0);
    assert_eq!(s.selection_text().as_deref(), Some("\u{4E2D}x"));
}

// ADR-1001's cluster budget: a line keeps 6 bytes of clusters a cell. On a
// screen one column wide, a heart and VS16 (7 bytes with its length byte)
// keeps the heart; the C reference kept both (the conformance case
// insert-wide-one-column). A wider line keeps it whole.
#[test]
fn the_cluster_budget_cuts_a_cluster_on_a_one_column_line() {
    let t = term(1, 2, "\u{2764}\u{FE0F}".as_bytes());
    assert_eq!(rows(t.screen()), ["\u{2764}", ""]);
    let t = term(2, 1, "\u{2764}\u{FE0F}".as_bytes());
    assert_eq!(rows(t.screen()), ["\u{2764}\u{FE0F}"]);
}
