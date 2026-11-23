//! Pictures in right-to-left rows (ADR-1004 D8): a picture whose box's top
//! row belongs to a right-to-left paragraph is mirrored as a whole, and a
//! run of kitty placeholder cells is placed where its cells are drawn.
#![cfg(all(feature = "images", feature = "bidi"))]

use vtium_term::images::{Kitty, Switches};
use vtium_term::{Age, ImageString, Palette, Terminal};

const P: char = '\u{10EEEE}';
const D: [char; 2] = ['\u{0305}', '\u{030D}'];

fn term(cols: usize, rows: usize, start: &str) -> Terminal {
    let mut t = Terminal::new(cols, rows, 100);
    t.screen_mut().set_image_limit(1 << 30);
    t.screen_mut().set_image_cell(8, 16).expect("a cell");
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    t.input(start.as_bytes());
    // image 1, two cells wide and one high
    let mut s = b"\x1b_Ga=t,f=24,s=16,v=16,i=1,q=2;".to_vec();
    s.extend(b"////".repeat(2 * 128));
    s.extend(b"\x1b\\");
    t.input(&s);
    t
}

/// The column each placement of the frame starts at, in draw order.
fn columns(t: &Terminal) -> Vec<u32> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    frame.placements().iter().map(|p| p.col).collect()
}

// ADR-1004 D8: a picture placed at the start of a right-to-left row ends
// at the right edge of the cell its first column is drawn in, the row's
// last; in a left-to-right row it stays where it was placed.
// covers: text.bidi
#[test]
fn a_picture_in_a_right_to_left_row_is_mirrored_as_a_whole() {
    let place = "\x1b_Ga=p,i=1,c=2,r=1,C=1,q=2\x1b\\";
    let mut t = term(12, 2, "");
    t.input(place.as_bytes());
    assert_eq!(columns(&t), [0]);
    let mut t = term(12, 2, "\x1b[2 k");
    t.input(place.as_bytes());
    assert_eq!(columns(&t), [10]);
}

// ADR-1004 D8: a run of placeholder cells in a right-to-left row keeps its
// order and is placed where its cells are drawn, at the row's right end.
// covers: text.bidi
#[test]
fn a_placeholder_run_is_placed_where_its_cells_are_drawn() {
    let mut t = term(12, 2, "\x1b[2 k");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
    assert_eq!(columns(&t), [10]);
}

/// The column, the width in columns and the width in pixels of each entry
/// of the frame, in draw order.
fn boxes(t: &Terminal) -> Vec<(u32, u32, u32)> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    frame
        .placements()
        .iter()
        .map(|p| (p.col, p.cols, p.width))
        .collect()
}

// ADR-1004 D8: a double-width row shows five of the ten columns, each
// twice as wide, and its right-to-left paragraph draws "ab" against the
// half's right edge: 'a' at screen columns 6 and 7, 'b' at 8 and 9. The
// three erased cells after "ab" go to the paragraph's end side, the left:
// the line's third cell is drawn at screen columns 4 and 5. The cursor's
// column counts the line's cells, and a picture placed in a cell ends at
// the right edge of the screen columns where that cell is drawn: on 'a'
// it covers columns 6 and 7, on 'b' 8 and 9, and on the third cell 4 and
// 5.
// covers: text.bidi
#[test]
fn a_picture_on_a_double_width_right_to_left_row_ends_where_its_cell_is_drawn() {
    let place = "\x1b_Ga=p,i=1,c=2,r=1,C=1,q=2\x1b\\";
    for (at, col) in [("\r", 6), ("\x1b[2G", 8), ("\x1b[3G", 4)] {
        let mut t = term(10, 2, &format!("\x1b[2 k\x1b#6ab{at}"));
        t.input(place.as_bytes());
        assert_eq!(columns(&t), [col], "placed after {at:?}");
    }
}

// ADR-1004 D8: each row of a double-height pair shows half the screen's
// columns too, so a picture placed in a cell of the pair's top row ends at
// the right edge of the screen columns where that cell is drawn there.
// covers: text.bidi
#[test]
fn a_picture_on_a_double_height_right_to_left_pair_ends_where_its_cell_is_drawn() {
    for (at, col) in [("\x1b[H", 6), ("\x1b[1;2H", 8), ("\x1b[1;3H", 4)] {
        let mut t = term(10, 3, &format!("\x1b[2 k\x1b#3ab\r\n\x1b#4ab{at}"));
        t.input(b"\x1b_Ga=p,i=1,c=2,r=1,C=1,q=2\x1b\\");
        assert_eq!(columns(&t), [col], "placed after {at:?}");
    }
}

// ADR-1004 D8: a double-width row of a 12-column screen shows six cells.
// Two placeholder cells at the start of a right-to-left row keep their
// order and are drawn against the half's right edge, in the line's fifth
// and sixth places, which are screen columns 8 to 11. The run's entry
// covers those four columns, its picture stretched to 32 pixels, twice
// its width on a row of single size.
// covers: text.bidi
#[test]
fn a_placeholder_run_on_a_double_width_right_to_left_row_covers_its_cells() {
    let mut t = term(12, 2, "\x1b[2 k\x1b#6");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
    assert_eq!(boxes(&t), [(8, 4, 32)]);
}

// ADR-1004 D8: a line made double after a picture was placed on it shows
// only the first half of its cells. On the empty right-to-left row of
// single size, the sixth of eleven cells is drawn in the middle, so a
// picture placed there ends at screen column 6 and starts at 4. Once the
// row is double, it no longer shows that cell, and the picture is not
// drawn.
// covers: text.bidi
#[test]
fn a_picture_in_a_cell_a_double_width_right_to_left_row_hides_is_not_drawn() {
    let mut t = term(11, 2, "\x1b[2 k\x1b[6G");
    t.input(b"\x1b_Ga=p,i=1,c=2,r=1,C=1,q=2\x1b\\");
    assert_eq!(columns(&t), [4]);
    t.input(b"\x1b#6");
    assert_eq!(columns(&t), []);
}
