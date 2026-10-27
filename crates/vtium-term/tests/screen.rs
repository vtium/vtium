//! The screen driven through the terminal: printing, clusters and widths,
//! cursor movement, editing, erasing, scrolling, tab stops, margins, the
//! scrollback, resizing, the alternate screen and the selection.
//!
//! Each expected screen is the C reference's behaviour, or the
//! specification's where vtium departs from it (the crate's README lists
//! each departure, and `departures.rs` tests them).

mod common;

use common::{rows, scrollback, term};
use vtium_term::{Attrs, Color, Glyph, Rgb, Screen, Style, Terminal, Width, WidthMethod};

fn cursor(t: &Terminal) -> (usize, usize) {
    t.screen().cursor()
}

fn text(t: &Terminal) -> Vec<String> {
    rows(t.screen())
}

fn glyph(screen: &Screen, row: usize, col: usize) -> (String, Width) {
    let line = screen.line(row).unwrap();
    let cell = line.cell(col);
    let g = match line.glyph(cell) {
        Glyph::Empty => String::new(),
        Glyph::Char(c) => c.into(),
        Glyph::Cluster(s) => s.into(),
    };
    (g, cell.width())
}

// ECMA-48 8.3.22, 8.3.19, 8.3.20, 8.3.18, 8.3.12, 8.3.13, 8.3.9, 8.3.57,
// 8.3.59, 8.3.158, 8.3.160, 8.3.21, 8.3.63: each movement and where it
// stops.
// covers: ctl.edit
#[test]
fn cursor_movement_stops_at_the_screen_and_the_margins() {
    let mut t = term(10, 6, b"\x1b[3;4H");
    assert_eq!(cursor(&t), (3, 2));
    t.input(b"\x1b[A");
    assert_eq!(cursor(&t), (3, 1));
    t.input(b"\x1b[99A");
    assert_eq!(cursor(&t), (3, 0));
    t.input(b"\x1b[2B\x1b[3C");
    assert_eq!(cursor(&t), (6, 2));
    t.input(b"\x1b[99C\x1b[99B");
    assert_eq!(cursor(&t), (9, 5));
    t.input(b"\x1b[4D\x1b[F");
    assert_eq!(cursor(&t), (0, 4));
    t.input(b"\x1b[5G\x1b[2E");
    assert_eq!(cursor(&t), (0, 5));
    t.input(b"\x1b[7`\x1b[2a");
    assert_eq!(cursor(&t), (8, 5));
    t.input(b"\x1b[2d\x1b[3e");
    assert_eq!(cursor(&t), (8, 4));
    t.input(b"\x1b[f");
    assert_eq!(cursor(&t), (0, 0));
    t.input(b"\x1b[0;0H\x1b[99;99f");
    assert_eq!(cursor(&t), (9, 5));
    // Inside a region CUU and CUD stop at its margins; outside, at the
    // screen's edge.
    t.input(b"\x1b[2;4r\x1b[3;1H\x1b[9A");
    assert_eq!(cursor(&t), (0, 1));
    t.input(b"\x1b[9B");
    assert_eq!(cursor(&t), (0, 3));
    t.input(b"\x1b[6;1H\x1b[9A");
    assert_eq!(cursor(&t), (0, 1));
}

// ECMA-48 8.3.39 (ED), 8.3.41 (EL), 8.3.38 (ECH).
// covers: ctl.edit
#[test]
fn erase_in_display_line_and_characters() {
    let fill = b"abcdefgh\r\nijklmnop\r\nqrstuvwx";
    let mut t = term(8, 3, fill);
    t.input(b"\x1b[2;4H\x1b[K");
    assert_eq!(text(&t), ["abcdefgh", "ijk", "qrstuvwx"]);
    t.input(b"\x1b[1K");
    assert_eq!(text(&t), ["abcdefgh", "", "qrstuvwx"]);
    let mut t = term(8, 3, fill);
    t.input(b"\x1b[2;4H\x1b[J");
    assert_eq!(text(&t), ["abcdefgh", "ijk", ""]);
    let mut t = term(8, 3, fill);
    t.input(b"\x1b[2;4H\x1b[1J");
    assert_eq!(text(&t), ["", "    mnop", "qrstuvwx"]);
    t.input(b"\x1b[2J");
    assert_eq!(text(&t), ["", "", ""]);
    assert_eq!(cursor(&t), (3, 1));
    let mut t = term(8, 3, fill);
    t.input(b"\x1b[3;2H\x1b[3X");
    assert_eq!(text(&t), ["abcdefgh", "ijklmnop", "q   uvwx"]);
    t.input(b"\x1b[99X\x1b[2K");
    assert_eq!(text(&t), ["abcdefgh", "ijklmnop", ""]);
}

// ECMA-48 8.3.64 (ICH), 8.3.26 (DCH): the rest of the line moves, and
// what passes the margin goes.
// covers: ctl.edit
#[test]
fn insert_and_delete_characters_move_the_rest_of_the_line() {
    let mut t = term(8, 1, b"abcdefgh\x1b[3G\x1b[2@");
    assert_eq!(text(&t), ["ab  cdef"]);
    assert_eq!(cursor(&t), (2, 0));
    t.input(b"\x1b[3P");
    assert_eq!(text(&t), ["abdef"]);
    t.input(b"\x1b[99P");
    assert_eq!(text(&t), ["ab"]);
    // A pending wrap inserts at the last column.
    let mut t = term(4, 1, b"abcd\x1b[@");
    assert_eq!(text(&t), ["abc"]);
    assert_eq!(cursor(&t), (3, 0));
    // A wide character the insertion cuts goes whole.
    t.input("\r\u{4E2D}\u{6587}".as_bytes());
    t.input(b"\x1b[2G\x1b[@");
    assert_eq!(text(&t), ["      ".trim_end()]);
}

// ECMA-48 8.3.67 (IL), 8.3.32 (DL): inside the region only, and the
// cursor goes to the first column.
// covers: ctl.edit
#[test]
fn insert_and_delete_lines_move_the_region_only() {
    let fill = b"1\r\n2\r\n3\r\n4\r\n5";
    let mut t = term(4, 5, fill);
    t.input(b"\x1b[2;4r\x1b[3;2H\x1b[L");
    assert_eq!(text(&t), ["1", "2", "", "3", "5"]);
    assert_eq!(cursor(&t), (0, 2));
    t.input(b"\x1b[2M");
    assert_eq!(text(&t), ["1", "2", "", "", "5"]);
    // Outside the region, nothing happens.
    t.input(b"\x1b[5;3H\x1b[L\x1b[M");
    assert_eq!(text(&t), ["1", "2", "", "", "5"]);
    assert_eq!(cursor(&t), (2, 4));
    let mut t = term(4, 5, fill);
    t.input(b"\x1b[2;1H\x1b[99M");
    assert_eq!(text(&t), ["1", "", "", "", ""]);
}

// ECMA-48 8.3.147 (SU), 8.3.113 (SD), 8.3.Index (IND), 8.3.104 (RI),
// 8.3.86 (NEL); a line feed at the bottom margin scrolls the region.
// covers: ctl.edit
#[test]
fn scrolling_moves_the_region_and_the_scrollback_takes_the_top() {
    let mut t = Terminal::new(4, 3, 100);
    t.screen_mut().set_scrollback_max(100);
    t.input(b"1\r\n2\r\n3\x1b[S");
    assert_eq!(text(&t), ["2", "3", ""]);
    assert_eq!(scrollback(t.screen()), ["1"]);
    t.input(b"\x1b[2T");
    assert_eq!(text(&t), ["", "", "2"]);
    t.input(b"\x1b[3;1H\x1bD");
    assert_eq!(text(&t), ["", "2", ""]);
    t.input(b"\x1b[1;3H\x1bM");
    assert_eq!(text(&t), ["", "", "2"]);
    t.input(b"\x1b[3;3H\x1bE");
    assert_eq!(text(&t), ["", "2", ""]);
    assert_eq!(cursor(&t), (0, 2));
    // RI at the top margin scrolls only the region.
    let mut t = term(4, 4, b"1\r\n2\r\n3\r\n4\x1b[2;3r\x1b[2;1H\x1bM");
    assert_eq!(text(&t), ["1", "", "2", "4"]);
    t.input(b"\x1b[3;1H\n");
    assert_eq!(text(&t), ["1", "2", "", "4"]);
}

// The C reference's flow (tsm-screen.c:388-399): a line leaving the top of
// the main screen's region enters the scrollback wherever the region
// starts; on the alternate screen it goes.
#[test]
fn a_line_leaving_any_region_enters_the_scrollback() {
    let mut t = Terminal::new(4, 4, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"1\r\n2\r\n3\r\n4\x1b[2;3r\x1b[3;1H\n");
    assert_eq!(text(&t), ["1", "3", "", "4"]);
    assert_eq!(scrollback(t.screen()), ["2"]);
    t.screen_mut().set_alternate(true);
    t.input(b"\x1b[2;3rx\x1b[3;1H\n");
    assert_eq!(scrollback(t.screen()), ["2"]);
}

// xterm's ED 3: the scrollback goes, and the screen stays.
// covers: ctl.ed3
#[test]
fn ed_3_empties_the_scrollback() {
    let mut t = Terminal::new(4, 2, 100);
    t.screen_mut().set_scrollback_max(100);
    t.input(b"1\r\n2\r\n3\r\n4");
    assert_eq!(scrollback(t.screen()), ["1", "2"]);
    t.screen_mut().scroll_view_up(1);
    t.input(b"\x1b[3J");
    assert_eq!(t.screen().scrollback().len(), 0);
    assert_eq!(t.screen().scrolled_back(), 0);
    assert_eq!(text(&t), ["3", "4"]);
}

// ECMA-48 8.3.60 (HTS), 8.3.154 (TBC), 8.3.10 (CHT), 8.3.7 (CBT): a stop
// every eighth column until changed; a tab never leaves a pending wrap.
// covers: ctl.tabs
#[test]
fn tab_stops_are_set_cleared_and_followed() {
    let mut t = term(30, 1, b"\t");
    assert_eq!(cursor(&t), (8, 0));
    t.input(b"\x1b[2I");
    assert_eq!(cursor(&t), (24, 0));
    t.input(b"\t\t");
    assert_eq!(cursor(&t), (29, 0));
    t.input(b"\x1b[Z");
    assert_eq!(cursor(&t), (24, 0));
    t.input(b"\x1b[3Z");
    assert_eq!(cursor(&t), (0, 0));
    t.input(b"\x1b[3G\x1bH\x1b[G\t");
    assert_eq!(cursor(&t), (2, 0));
    t.input(b"\x1b[g\x1b[G\t");
    assert_eq!(cursor(&t), (8, 0));
    t.input(b"\x1b[3g\x1b[G\t");
    assert_eq!(cursor(&t), (29, 0));
    assert!(t.screen().tab_stops().iter().all(|&s| !s));
}

// DECSTBM (xterm's ctlseqs): the region the line feed scrolls; an invalid
// one is the whole screen. The cursor stays, as in the C reference.
// covers: ctl.decstbm
#[test]
fn decstbm_sets_the_scroll_region() {
    let mut t = term(4, 5, b"1\r\n2\r\n3\r\n4\r\n5\x1b[2;4r");
    assert_eq!(t.screen().margins(), (1, 3));
    assert_eq!(cursor(&t), (1, 4));
    t.input(b"\x1b[4;1H\nx");
    assert_eq!(text(&t), ["1", "3", "4", "x", "5"]);
    // At the screen's last row, below the region, a line feed scrolls the
    // region, as in the C reference.
    t.input(b"\x1b[5;1H\ny");
    assert_eq!(text(&t), ["1", "4", "x", "", "y"]);
    for invalid in [&b"\x1b[r"[..], b"\x1b[3;3r", b"\x1b[4;2r", b"\x1b[2;9r"] {
        t.input(b"\x1b[2;4r");
        t.input(invalid);
        assert_eq!(t.screen().margins(), (0, 4), "{invalid:?}");
    }
    // A private marker is not DECSTBM.
    t.input(b"\x1b[?2;4r");
    assert_eq!(t.screen().margins(), (0, 4));
}

// Background colour erase: an erased, inserted or scrolled-in cell takes
// the pen's background and nothing else of it, as Ghostty's and foot's
// do (the C reference's probe erase-colours-only).
// covers: ctl.bce
#[test]
fn erasing_takes_only_the_pens_background() {
    let mut t = term(6, 3, b"abcdef");
    let mut pen = Style {
        fg: Color::Indexed(1),
        bg: Color::Rgb(Rgb::new(1, 2, 3)),
        ..Style::DEFAULT
    };
    pen.attrs.insert(Attrs::BOLD);
    pen.attrs.insert(Attrs::INVERSE);
    t.screen_mut().set_pen(pen);
    let blank = Style {
        bg: Color::Rgb(Rgb::new(1, 2, 3)),
        ..Style::DEFAULT
    };
    t.input(b"\x1b[3G\x1b[K\x1b[G\x1b[@\x1b[S");
    let s = t.screen();
    let line = s.line(2).unwrap();
    assert!((0..6).all(|c| line.style(line.cell(c)) == blank));
    t.input(b"\x1b[2;1H\x1b[2X");
    let line = t.screen().line(1).unwrap();
    assert_eq!(line.style(line.cell(1)), blank);
    assert_eq!(line.style(line.cell(2)), Style::DEFAULT);
}

// UAX #29: a cluster is one cell, two when its base is wide.
// covers: text.grapheme
#[test]
fn a_grapheme_cluster_takes_one_cell() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let input = format!("e\u{301}x{family}\u{1F1FA}\u{1F1F8}\u{1100}\u{1161}\u{11A8}.");
    let t = term(20, 1, input.as_bytes());
    let s = t.screen();
    assert_eq!(glyph(s, 0, 0), ("e\u{301}".into(), Width::Narrow));
    assert_eq!(glyph(s, 0, 1), ("x".into(), Width::Narrow));
    assert_eq!(glyph(s, 0, 2), (family.into(), Width::Wide));
    assert_eq!(glyph(s, 0, 3), (String::new(), Width::WideTail));
    assert_eq!(glyph(s, 0, 4), ("\u{1F1FA}\u{1F1F8}".into(), Width::Wide));
    assert_eq!(
        glyph(s, 0, 6),
        ("\u{1100}\u{1161}\u{11A8}".into(), Width::Wide)
    );
    assert_eq!(glyph(s, 0, 8), (".".into(), Width::Narrow));
    assert_eq!(cursor(&t), (9, 0));
    // A combining mark after the cursor moved joins the cell before it,
    // as foot does; at the first column it is dropped.
    let t = term(4, 2, "ab\x1b[D\x1b[C\u{301}\r\n\u{301}".as_bytes());
    assert_eq!(glyph(t.screen(), 0, 1).0, "b\u{301}");
    assert_eq!(rows(t.screen()), ["ab\u{301}", ""]);
}

// VS16 makes an emoji-style base two cells wide; VS15 leaves a width as
// it is, as in the C reference. A wide character with only the last column
// left wraps whole, the cell it leaves cleared; without autowrap it is not
// printed (the C reference's probes vs16-width and wide-at-margin).
// covers: text.vs-width
#[test]
fn vs16_widens_and_a_wide_character_wraps_whole() {
    let t = term(6, 2, "\u{2764}\u{FE0F}x\u{1F600}\u{FE0E}".as_bytes());
    let s = t.screen();
    assert_eq!(glyph(s, 0, 0), ("\u{2764}\u{FE0F}".into(), Width::Wide));
    assert_eq!(glyph(s, 0, 2), ("x".into(), Width::Narrow));
    assert_eq!(glyph(s, 0, 3), ("\u{1F600}\u{FE0E}".into(), Width::Wide));
    assert_eq!(cursor(&t), (5, 0));
    // In the last column a VS16 moves its cluster to the next line.
    let t = term(4, 2, "abc\u{2764}\u{FE0F}".as_bytes());
    assert_eq!(rows(t.screen()), ["abc", "\u{2764}\u{FE0F}"]);
    assert_eq!(cursor(&t), (2, 1));
    // A wide character at the last column wraps whole.
    let t = term(4, 2, "abc\u{4E2D}".as_bytes());
    assert_eq!(rows(t.screen()), ["abc", "\u{4E2D}"]);
    assert!(t.screen().line(0).unwrap().cell(3).is_blank());
    // Without autowrap it is not printed, and the cursor stays.
    let mut t = Terminal::new(4, 2, 0);
    t.screen_mut().set_auto_wrap(false);
    t.input("abc\u{4E2D}".as_bytes());
    assert_eq!(rows(t.screen()), ["abc", ""]);
    assert_eq!(cursor(&t), (3, 0));
}

// REP (ECMA-48 8.3.103): the base of the last printed cell, each copy a
// cell of its own, wrapping as printed text does; nothing before a print.
// covers: text.rep
#[test]
fn rep_repeats_the_last_printed_base() {
    let t = term(5, 2, "\x1b[3be\u{301}\x1b[5b".as_bytes());
    assert_eq!(rows(t.screen()), ["e\u{301}eeee", "e"]);
    assert_eq!(cursor(&t), (1, 1));
    let t = term(6, 1, "\u{4E2D}\x1b[2b".as_bytes());
    assert_eq!(rows(t.screen()), ["\u{4E2D}\u{4E2D}\u{4E2D}"]);
}

// Mode 2027's two ways to cut text into cells: by UAX #29, or by codepoint
// width, where only a codepoint of no width joins the cell before.
// covers: DEC-2027
#[test]
fn the_width_method_cuts_text_into_cells() {
    let input = "\u{1F1FA}\u{1F1F8}\u{2764}\u{FE0F}e\u{301}".as_bytes();
    let mut t = Terminal::new(10, 1, 0);
    t.input(input);
    assert_eq!(
        rows(t.screen()),
        ["\u{1F1FA}\u{1F1F8}\u{2764}\u{FE0F}e\u{301}"]
    );
    assert_eq!(cursor(&t), (5, 0));
    let mut t = Terminal::new(10, 1, 0);
    t.screen_mut().set_width_method(WidthMethod::Wcwidth);
    t.input(input);
    let s = t.screen();
    assert_eq!(glyph(s, 0, 0), ("\u{1F1FA}".into(), Width::Wide));
    assert_eq!(glyph(s, 0, 2), ("\u{1F1F8}".into(), Width::Wide));
    assert_eq!(glyph(s, 0, 4), ("\u{2764}\u{FE0F}".into(), Width::Narrow));
    assert_eq!(glyph(s, 0, 5).0, "e\u{301}");
    assert_eq!(cursor(&t), (6, 0));
}

// The C reference's tsm-selection.c: a selection from where the pointer
// started to where it is, across the scrollback and the screen; a word
// selection stops at spaces.
// covers: clipboard.selection
#[test]
fn a_selection_copies_its_text() {
    let mut t = Terminal::new(8, 2, 10);
    t.screen_mut().set_scrollback_max(10);
    t.input("one two\r\n\u{4E2D}x e\u{301}\r\nthree".as_bytes());
    let s = t.screen_mut();
    assert_eq!(s.selection_text(), None);
    s.scroll_view_up(1);
    s.select_start(4, 0);
    s.select_to(2, 1);
    assert_eq!(s.selection_text().as_deref(), Some("two\n\u{4E2D}x"));
    // Backwards from where it started.
    s.select_to(1, 0);
    assert_eq!(s.selection_text().as_deref(), Some("ne t"));
    s.select_word(5, 1);
    assert_eq!(s.selection_text().as_deref(), Some("e\u{301}"));
    s.select_word(3, 1);
    assert_eq!(s.selection_text(), None);
    // The selection follows its text as the screen scrolls.
    s.reset_view();
    s.select_start(0, 1);
    s.select_to(4, 1);
    let frame_selected = |s: &Screen| {
        let palette =
            vtium_term::Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
        let frame = s.frame(&palette, vtium_term::Age::NEVER);
        (0..s.rows())
            .flat_map(|r| (0..s.cols()).map(move |c| (r, c)))
            .filter(|&(r, c)| frame.cell(r, c).unwrap().selected)
            .collect::<Vec<_>>()
    };
    assert_eq!(frame_selected(s), [(1, 0), (1, 1), (1, 2), (1, 3), (1, 4)]);
    t.input(b"\r\nfour");
    let s = t.screen();
    assert_eq!(s.selection_text().as_deref(), Some("three"));
    assert_eq!(frame_selected(s), [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]);
}

// A shorter screen pushes its top lines into the scrollback; a taller one
// pulls them back; the cursor stays on its text.
#[test]
fn resizing_moves_lines_through_the_scrollback() {
    let mut t = Terminal::new(6, 4, 100);
    t.screen_mut().set_scrollback_max(100);
    t.input(b"1\r\n2\r\n3\r\n4");
    t.screen_mut().resize(6, 2);
    assert_eq!(text(&t), ["3", "4"]);
    assert_eq!(scrollback(t.screen()), ["1", "2"]);
    assert_eq!(cursor(&t), (1, 1));
    t.screen_mut().resize(6, 3);
    assert_eq!(text(&t), ["2", "3", "4"]);
    assert_eq!(cursor(&t), (1, 2));
    // Narrower and wider again: the cells past the edge come back.
    t.input(b"\x1b[1;1Habcdef");
    t.screen_mut().resize(3, 3);
    assert_eq!(text(&t), ["abc", "3", "4"]);
    t.screen_mut().resize(6, 3);
    assert_eq!(text(&t), ["abcdef", "3", "4"]);
    // With nothing in the scrollback, it scrolls only as far as the
    // cursor needs, and the rows below come back when it grows.
    let mut t = term(4, 4, b"1\r\n2\x1b[H");
    t.screen_mut().resize(4, 2);
    assert_eq!(text(&t), ["1", "2"]);
    t.input(b"\x1b[3;1H");
    t.screen_mut().resize(4, 4);
    assert_eq!(text(&t), ["1", "2", "", ""]);
}

// The alternate screen keeps its own lines and no scrollback.
#[test]
fn the_alternate_screen_has_its_own_lines_and_no_scrollback() {
    let mut t = Terminal::new(4, 2, 100);
    t.screen_mut().set_scrollback_max(100);
    t.input(b"main");
    t.screen_mut().set_alternate(true);
    assert_eq!(text(&t), ["", ""]);
    t.input(b"\r\nalt\r\nmore");
    assert_eq!(text(&t), ["alt", "more"]);
    assert_eq!(t.screen().scrollback().len(), 0);
    t.screen_mut().set_alternate(false);
    assert_eq!(text(&t), ["main", ""]);
}

// The scrollback keeps at most `sb-size` lines, and `sb-size` at most the
// cap the screen was made with: `sb_size_cap` for the machine.
#[test]
fn the_scrollback_keeps_at_most_its_size_and_its_cap() {
    let cap = vtium_term::sb_size_cap(512 << 20, Some(3));
    let mut t = Terminal::new(4, 1, cap);
    t.screen_mut().set_scrollback_max(1000);
    t.input(b"1\n\r2\n\r3\n\r4\n\r5");
    assert_eq!(scrollback(t.screen()), ["2", "3", "4"]);
    t.screen_mut().set_scrollback_max(2);
    assert_eq!(scrollback(t.screen()), ["3", "4"]);
    t.screen_mut().set_scrollback_max(0);
    t.input(b"\n");
    assert_eq!(t.screen().scrollback().len(), 0);
}

// The view moves through the scrollback and back; the frame shows it.
#[test]
fn the_view_scrolls_through_the_scrollback() {
    let mut t = Terminal::new(4, 2, 100);
    t.screen_mut().set_scrollback_max(100);
    t.input(b"1\r\n2\r\n3\r\n4");
    let palette = vtium_term::Palette::new([Rgb::default(); 256], Rgb::default(), Rgb::default());
    let view = |s: &Screen| {
        let frame = s.frame(&palette, vtium_term::Age::NEVER);
        (0..frame.rows())
            .map(|r| common::line_text(frame.line(r).unwrap()))
            .collect::<Vec<_>>()
    };
    let s = t.screen_mut();
    s.scroll_view_up(1);
    assert_eq!(view(s), ["2", "3"]);
    s.scroll_view_up(9);
    assert_eq!(view(s), ["1", "2"]);
    assert_eq!(s.scrolled_back(), 2);
    s.scroll_view_down(1);
    assert_eq!(view(s), ["2", "3"]);
    s.scroll_view_down(9);
    assert_eq!(view(s), ["3", "4"]);
    s.scroll_view_up(1);
    s.reset_view();
    assert_eq!(s.scrolled_back(), 0);
    // While scrolled back the cursor's row is lower in the view.
    s.scroll_view_up(1);
    let frame = s.frame(&palette, vtium_term::Age::NEVER);
    assert!(!frame.cursor().visible);
}

// DECOM addresses rows from the region's top and keeps the cursor in it;
// DECAWM off overwrites the last column; IRM moves the line right. The
// mode table (vtium/vtium#12) sets these through the screen's accessors.
#[test]
fn origin_autowrap_and_insert_change_the_screen() {
    let mut t = term(6, 5, b"\x1b[2;4r");
    t.screen_mut().set_origin_mode(true);
    t.input(b"\x1b[1;1Ho\x1b[9;1Hp");
    assert_eq!(text(&t), ["", "o", "", "p", ""]);
    let mut t = Terminal::new(4, 1, 0);
    t.screen_mut().set_auto_wrap(false);
    t.input(b"abcdef");
    assert_eq!(text(&t), ["abcf"]);
    let mut t = term(6, 1, b"abcdef\r");
    t.screen_mut().set_insert_mode(true);
    t.input("x\u{4E2D}".as_bytes());
    assert_eq!(text(&t), ["x\u{4E2D}abc"]);
}

// A program writing past the line's end wraps it: the line carries the
// flag reflow reads.
#[test]
fn a_wrapped_line_is_flagged() {
    let t = term(4, 3, b"abcdef\r\nxy");
    let flags = |row| t.screen().line(row).unwrap().flags();
    assert!(flags(0).contains(vtium_term::LineFlags::WRAPPED));
    assert!(!flags(1).contains(vtium_term::LineFlags::WRAPPED));
}
