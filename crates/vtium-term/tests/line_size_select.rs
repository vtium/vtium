//! Selection, copy and the pointer on lines of double size, in the cases
//! tier 1's `line-size-select` case does not reach: a double-height pair
//! cut by the top of the view, a pair the alternate screen would split,
//! and a link under the pointer on a double-width line.
//!
//! The rule each case follows is ADR-1308 D1, "Selection on a double-size
//! line follows the cursor's rule": the pointer maps a click on a line of
//! double size to the line's own columns, a selection on either line of a
//! pair highlights both halves of each character, and a copy takes the
//! pair's text once, from its top line.

mod common;

use common::term;
use vtium_term::{Age, Terminal};

/// The columns the frame draws selected in each view row.
fn selected(t: &Terminal) -> Vec<std::ops::Range<usize>> {
    t.screen()
        .frame(t.palette(), Age::NEVER)
        .selection()
        .to_vec()
}

// A drag from the line above a pair to the line below it copies the
// pair's text once, from its top line. A drag that starts on the top line
// and ends on the bottom line selects the characters between the two
// columns, as on one line, and both lines of the pair draw those columns
// selected.
// covers: text.line-size, clipboard.selection
#[test]
fn a_pair_copies_its_text_once_and_selects_as_one_line() {
    let mut t = term(10, 4, b"above\r\n\x1b#3abcd\r\n\x1b#4abcd\r\nbelow");
    let s = t.screen_mut();
    s.select_start(2, 0);
    s.select_to(1, 3);
    assert_eq!(
        t.screen().selection_text().as_deref(),
        Some("ove\nabcd\nbe")
    );
    // screen column 6 of the top line is the doubled d, and screen column
    // 2 of the bottom line is the doubled b
    let s = t.screen_mut();
    s.select_start(6, 1);
    s.select_to(2, 2);
    assert_eq!(t.screen().selection_text().as_deref(), Some("bcd"));
    assert_eq!(selected(&t)[1..3], [1..4, 1..4]);
}

// A pair whose top line scrolled into the scrollback shows only its bottom
// line, at the top of the view. A drag there selects the doubled
// characters under the pointer. The visible bottom line draws them
// selected, and the copy takes them from the top line, which holds the
// pair's text.
// covers: text.line-size, clipboard.selection
#[test]
fn a_pair_cut_by_the_top_of_the_view_selects_and_copies_its_text() {
    let mut t = term(10, 2, b"\x1b#3abcd\r\n\x1b#4abcd\r\nnext");
    let s = t.screen_mut();
    // the view shows the bottom line at row 0; screen columns 2 to 5 show
    // the doubled b and c
    s.select_start(2, 0);
    s.select_to(5, 0);
    assert_eq!(t.screen().selection_text().as_deref(), Some("bc"));
    assert_eq!(
        selected(&t)[0],
        1..3,
        "the bottom line draws b and c selected"
    );
}

// On the alternate screen, the scrollback's last line is a line of the
// main screen, so it does not pair with the alternate screen's first row.
// A selection on that row takes that row's own text.
// covers: text.line-size, clipboard.selection
#[test]
fn the_alternate_screen_does_not_pair_its_first_row_with_the_scrollback() {
    let mut t = term(10, 2, b"\x1b#3main\r\n\r\n");
    t.input(b"\x1b[?1049h\x1b[H\x1b#4alt!");
    let s = t.screen_mut();
    s.select_word(2, 0);
    assert_eq!(t.screen().selection_text().as_deref(), Some("alt!"));
    assert_eq!(selected(&t)[0], 0..4);
}

// A link on a double-width line is under the pointer wherever its doubled
// characters draw: screen columns 2 to 5 show the link's two characters,
// and the columns past them show none.
// covers: text.line-size
#[test]
fn the_link_under_the_pointer_on_a_double_width_line_is_the_one_drawn_there() {
    let t = term(
        10,
        1,
        b"\x1b#6a\x1b]8;;https://example.com\x1b\\bc\x1b]8;;\x1b\\d",
    );
    let s = t.screen();
    let linked: Vec<bool> = (0..10).map(|x| s.link_at(x, 0).is_some()).collect();
    assert_eq!(
        linked,
        [
            false, false, true, true, true, true, false, false, false, false
        ]
    );
}
