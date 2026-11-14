//! Double-width and double-height lines (ADR-1000's `text.line-size`):
//! `ESC # 3` and `ESC # 4` (DECDHL), `ESC # 5` (DECSWL) and `ESC # 6`
//! (DECDWL) driven through the byte stream, and what a line of double
//! size does to printing, the cursor, the edits, the erases, the scrolls,
//! DECALN, DECLRMM and a resize.
//!
//! Each case states the statement its expectation follows: the VT100 User
//! Guide's chapter 3 (<https://vt100.net/docs/vt100-ug/chapter3.html>) and
//! the VT510 manual (<https://vt100.net/docs/vt510-rm/>). Where both leave
//! a case open, the case says which reading vtium takes, and the crate's
//! README lists it.

mod common;

use common::{rows, scrollback, term};
use vtium_term::{LineFlags, LineSize, Terminal};

/// The size of row `y` of the screen in use.
fn size(t: &Terminal, y: usize) -> LineSize {
    t.screen()
        .line(y)
        .map_or(LineSize::Single, |l| l.flags().size())
}

fn reply(t: &mut Terminal, query: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(query);
    t.take_replies()
}

// VT100 User Guide, DECDHL, DECSWL and DECDWL; VT510 manual, the same
// pages: each sequence sets the size of the line that holds the cursor,
// and of no other line. `ESC # 7` and `ESC # 9` name nothing and change
// nothing.
// covers: text.line-size
#[test]
fn esc_hash_3_to_6_set_the_size_of_the_cursors_line() {
    let mut t = term(10, 4, b"\x1b#3\n\x1b#4\n\x1b#6\n\x1b#6\x1b#5");
    assert_eq!(size(&t, 0), LineSize::DoubleHeightTop);
    assert_eq!(size(&t, 1), LineSize::DoubleHeightBottom);
    assert_eq!(size(&t, 2), LineSize::DoubleWidth);
    assert_eq!(size(&t, 3), LineSize::Single);
    t.input(b"\x1b[3;1H\x1b#7\x1b#9");
    assert_eq!(size(&t, 2), LineSize::DoubleWidth);
}

// VT100 User Guide, DECDWL: "If the line was single-width single-height,
// all characters to the right of the screen are lost", and DECDHL: "all
// characters to the right of the center of the screen are lost". DECSWL
// keeps the line's characters and brings none back. The line loses its
// wrap, since its last column is gone.
// covers: text.line-size
#[test]
fn a_line_made_double_size_loses_the_characters_right_of_the_center() {
    let mut t = term(10, 3, b"0123456789abc\x1b[1;1H\x1b#6");
    assert_eq!(rows(t.screen())[0], "01234");
    assert!(
        !t.screen()
            .line(0)
            .unwrap()
            .flags()
            .contains(LineFlags::WRAPPED)
    );
    t.input(b"\x1b#5");
    assert_eq!(rows(t.screen())[0], "01234");
    let t = term(10, 3, b"\x1b[2;1H0123456789\x1b[2;1H\x1b#3");
    assert_eq!(rows(t.screen())[1], "01234");
}

// VT100 User Guide, DECDWL and DECDHL: "The cursor remains over the same
// character position unless it would be to the right of the right margin,
// in which case, it is moved to the right margin"; DECSWL: "The cursor
// remains on the same character position".
// covers: text.line-size
#[test]
fn the_cursor_keeps_its_column_or_moves_to_the_lines_last_one() {
    let mut t = term(10, 3, b"\x1b[1;3H\x1b#6");
    assert_eq!(t.screen().cursor(), (2, 0));
    t.input(b"\x1b#5\x1b[1;9H\x1b#6");
    assert_eq!(t.screen().cursor(), (4, 0));
    t.input(b"\x1b#5");
    assert_eq!(t.screen().cursor(), (4, 0));
    // a pending wrap at the screen's width moves to the last column too
    let mut t = term(10, 3, b"0123456789");
    assert_eq!(t.screen().cursor(), (10, 0));
    t.input(b"\x1b#4");
    assert_eq!(t.screen().cursor(), (4, 0));
}

// VT100 User Guide, DECDWL: "The use of double-width characters reduces
// the number of characters per line by half". Printing wraps after the
// line's last column, half the screen's width, and the pending wrap stands
// on that column, so CPR reports it as column 5 of 10. The line is marked
// as continuing on the next row, as a line of the normal size is, so
// reverse wraparound goes back across the wrap. A resize still does not
// rewrap it, because reflow rewraps only lines of the normal size.
// covers: text.line-size
#[test]
fn printing_on_a_double_size_line_wraps_at_half_the_width() {
    let mut t = term(10, 3, b"\x1b#6abcde");
    assert_eq!(t.screen().cursor(), (4, 0));
    assert!(t.screen().margin_wrap());
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[1;5R");
    t.input(b"fg");
    assert_eq!(rows(t.screen())[..2], ["abcde", "fg"]);
    assert_eq!(t.screen().cursor(), (2, 1));
    assert!(
        t.screen()
            .line(0)
            .unwrap()
            .flags()
            .contains(LineFlags::WRAPPED)
    );
    // a wide character that does not fit before the last column wraps whole
    let t = term(10, 3, b"\x1b#6abcd\xe4\xb8\x80");
    assert_eq!(rows(t.screen())[..2], ["abcd", "\u{4e00}"]);
    assert_eq!(t.screen().cursor(), (2, 1));
}

// ctlseqs, DECSET 45, reverse wraparound, as ADR-1000's DEC-45 row states
// it: while DECAWM is set, BS and CUB go on from the first column to the
// last column of the row above when printing wrapped from that row. On a
// line of double size that column is the line's own last one, column 5 of
// 10 (VT100 User Guide, DECDWL and DECDHL: half as many characters).
// covers: text.line-size, DEC-45
#[test]
fn reverse_wraparound_goes_back_to_a_double_size_lines_last_column() {
    let mut t = term(10, 4, b"\x1b[?45h\x1b#6abcdefg\x08\x08\x08");
    assert_eq!(t.screen().cursor(), (4, 0));
    t.input(b"\x1b[3;1H\x1b#3hijklmn\x1b[4;1H\x1b[3D");
    assert_eq!(t.screen().cursor(), (2, 2));
}

// VT510 manual, DECAWM: with autowrap off, "characters received when the
// cursor is at the right margin replace previously displayed characters";
// on a double-size line that margin is its last column.
// covers: text.line-size
#[test]
fn without_autowrap_printing_overwrites_the_lines_last_column() {
    let t = term(10, 3, b"\x1b[?7l\x1b#6abcdefg");
    assert_eq!(rows(t.screen())[..2], ["abcdg", ""]);
    assert_eq!(t.screen().cursor(), (4, 0));
}

// VT100 User Guide, DECDWL: the cursor is not to the right of the right
// margin, which is the line's last column. CUF, CUP, CHA, HT and CUD from
// another row stop there, CPR counts the line's own columns, and the
// cursor keeps the column it stopped at when it leaves the line, as it
// does after DECDWL itself. Tab stops count the line's columns.
// covers: text.line-size
#[test]
fn cursor_movement_stops_at_the_lines_last_column() {
    let mut t = term(10, 4, b"\x1b[2;1H\x1b#6\x1b[99C");
    assert_eq!(t.screen().cursor(), (4, 1));
    t.input(b"\x1b[2;9H");
    assert_eq!(t.screen().cursor(), (4, 1));
    t.input(b"\x1b[9G");
    assert_eq!(t.screen().cursor(), (4, 1));
    t.input(b"\x1b[1G\t");
    assert_eq!(t.screen().cursor(), (4, 1));
    assert_eq!(reply(&mut t, b"\x1b[6n"), b"\x1b[2;5R");
    t.input(b"\x1b[1;8H\x1b[B");
    assert_eq!(t.screen().cursor(), (4, 1));
    t.input(b"\x1b[B");
    assert_eq!(t.screen().cursor(), (4, 2));
    // a tab stop inside the line's columns still stops the cursor
    t.input(b"\x1b[3g\x1b[2;3H\x1bH\x1b[2;1H\t");
    assert_eq!(t.screen().cursor(), (2, 1));
}

// VT510 manual, DECSC: the cursor's position and the pending wrap are
// saved, and DECRC restores them. A wrap pending at a double-size line's
// last column comes back pending.
// covers: text.line-size
#[test]
fn decrc_restores_a_wrap_pending_at_the_lines_last_column() {
    let mut t = term(10, 3, b"\x1b#6abcde\x1b7\x1b[3;1H\x1b8");
    assert_eq!(t.screen().cursor(), (4, 0));
    assert!(t.screen().margin_wrap());
    t.input(b"f");
    assert_eq!(rows(t.screen())[..2], ["abcde", "f"]);
}

// VT510 manual, ED: "When you erase complete lines, they become
// single-height, single-width lines"; VT100 User Guide, ED: "Any complete
// line erased by this sequence will return that line to single width
// mode". A line erased in part keeps its size. EL changes no line's size,
// since neither manual's EL says it does.
// covers: text.line-size
#[test]
fn ed_returns_each_line_it_erases_whole_to_single_size() {
    let all = b"\x1b[1;1H\x1b#6ab\x1b[2;1H\x1b#3ab\x1b[3;1H\x1b#4ab";
    let mut t = term(10, 4, all);
    t.input(b"\x1b[2J");
    assert_eq!(
        (size(&t, 0), size(&t, 1), size(&t, 2)),
        (LineSize::Single, LineSize::Single, LineSize::Single)
    );
    t.input(all);
    t.input(b"\x1b[2;2H\x1b[J");
    assert_eq!(size(&t, 0), LineSize::DoubleWidth);
    assert_eq!(size(&t, 1), LineSize::DoubleHeightTop);
    assert_eq!(size(&t, 2), LineSize::Single);
    assert_eq!(rows(t.screen())[1], "a");
    t.input(all);
    // ED 1 through the last column of a double-size line erases it whole
    t.input(b"\x1b[2;5H\x1b[1J");
    assert_eq!(
        (size(&t, 0), size(&t, 1), size(&t, 2)),
        (
            LineSize::Single,
            LineSize::Single,
            LineSize::DoubleHeightBottom
        )
    );
    t.input(all);
    t.input(b"\x1b[1;1H\x1b[2K\x1b[2;1H\x1b[K\x1b[3;5H\x1b[1K");
    assert_eq!(size(&t, 0), LineSize::DoubleWidth);
    assert_eq!(size(&t, 1), LineSize::DoubleHeightTop);
    assert_eq!(size(&t, 2), LineSize::DoubleHeightBottom);
    assert_eq!(rows(t.screen())[..3], ["", "", ""]);
}

// VT510 manual, ICH: "Text between the cursor and right margin moves to
// the right. Characters scrolled past the right margin are lost"; DCH:
// "The terminal adds blank spaces with no visual character attributes at
// the right margin". On a double-size line that margin is its last
// column.
// covers: text.line-size
#[test]
fn ich_and_dch_move_the_cells_up_to_the_lines_last_column() {
    let mut t = term(10, 3, b"\x1b#6abcde\x1b[1;2H\x1b[@");
    assert_eq!(rows(t.screen())[0], "a bcd");
    t.input(b"\x1b[3P");
    assert_eq!(rows(t.screen())[0], "ad");
    // insert mode moves the cells up to the same column
    t.input(b"\x1b[1;1H\x1b[2Kabcde\x1b[1;1H\x1b[4hX\x1b[4l");
    assert_eq!(rows(t.screen())[0], "Xabcd");
}

// VT510 manual, IL: "lines below the cursor and in the scrolling region
// move down"; DL: "The terminal adds blank lines with no visual character
// attributes at the bottom of the scrolling region". A line moves with
// its size, and the lines brought in are single size; a line scrolled
// off the top keeps its size in the scrollback.
// covers: text.line-size
#[test]
fn lines_move_with_their_size_and_blank_lines_come_in_single_size() {
    let mut t = term(10, 3, b"\x1b#6ab\x1b[1;1H\x1b[L");
    assert_eq!(
        (size(&t, 0), size(&t, 1)),
        (LineSize::Single, LineSize::DoubleWidth)
    );
    t.input(b"\x1b[M");
    assert_eq!(
        (size(&t, 0), size(&t, 2)),
        (LineSize::DoubleWidth, LineSize::Single)
    );
    t.input(b"\x1b[S\x1b[T");
    assert_eq!(size(&t, 0), LineSize::Single);
    t.screen_mut().set_scrollback_max(10);
    t.input(b"\x1b[1;1H\x1b#3top\x1b[3;1H\n");
    assert_eq!(scrollback(t.screen()), ["top"]);
    let line = t.screen().scrollback().next().unwrap();
    assert_eq!(line.flags().size(), LineSize::DoubleHeightTop);
}

// ADR-1000's `ctl.decaln` fills the screen with E as xterm fills it, and
// xterm's DECALN returns every line to single size first.
// covers: text.line-size
#[test]
fn decaln_returns_every_line_to_single_size() {
    let t = term(10, 3, b"\x1b#6\x1b[3;1H\x1b#3\x1b#8");
    assert_eq!(
        (size(&t, 0), size(&t, 2)),
        (LineSize::Single, LineSize::Single)
    );
    assert_eq!(rows(t.screen())[2], "EEEEEEEEEE");
}

// VT510 manual, DECLRMM: "All line attributes currently in page memory for
// the session are set to single width, single height. The terminal
// ignores any sequences to change the line attributes to double width or
// double height (DECDWL or DECDHL)". Once the mode is reset, the
// sequences work again.
// covers: text.line-size
#[test]
fn declrmm_returns_lines_to_single_size_and_keeps_them_there() {
    let mut t = term(10, 3, b"\x1b#6abc\x1b[?47h\x1b#3\x1b[?47l\x1b[?69h");
    assert_eq!(size(&t, 0), LineSize::Single);
    assert_eq!(rows(t.screen())[0], "abc");
    t.input(b"\x1b[?47h");
    assert_eq!(size(&t, 0), LineSize::Single);
    t.input(b"\x1b[?47l\x1b#6\x1b#3\x1b#4");
    assert_eq!(size(&t, 0), LineSize::Single);
    t.input(b"\x1b#5\x1b[?69l\x1b#6");
    assert_eq!(size(&t, 0), LineSize::DoubleWidth);
}

// A resize the VT510 does not make: a line of double size holds half the
// new width's columns, so a narrower screen cuts it at the new half, and
// the cursor on it moves to its last column. vtium-term's README lists
// this as its own choice.
// covers: text.line-size
#[test]
fn a_resize_cuts_a_double_size_line_at_half_the_new_width() {
    let mut t = term(10, 3, b"\x1b#6abcde");
    t.resize(6, 3);
    assert_eq!(rows(t.screen())[0], "abc");
    assert_eq!(size(&t, 0), LineSize::DoubleWidth);
    assert_eq!(t.screen().cursor(), (2, 0));
    t.resize(12, 3);
    assert_eq!(rows(t.screen())[0], "abc");
    t.input(b"\x1b[99C");
    assert_eq!(t.screen().cursor(), (5, 0));
    // the alternate screen's lines are cut the same way
    let mut t = term(10, 3, b"\x1b[?1049h\x1b#6abcde");
    t.resize(6, 3);
    assert_eq!(rows(t.screen())[0], "abc");
    assert_eq!(t.screen().cursor(), (2, 0));
}
