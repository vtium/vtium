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
// covers those four columns. Its 16×16 picture keeps its aspect in the
// 32×16 pixels the two cells cover, so it is 16 pixels wide.
// covers: text.bidi
#[test]
fn a_placeholder_run_on_a_double_width_right_to_left_row_covers_its_cells() {
    let mut t = term(12, 2, "\x1b[2 k\x1b#6");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
    assert_eq!(boxes(&t), [(8, 4, 16)]);
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

/// The view row and the column of each entry of the frame, in draw order.
fn places(t: &Terminal) -> Vec<(i64, u32)> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    frame.placements().iter().map(|p| (p.row, p.col)).collect()
}

// ADR-1004 D8: a picture placed in the third cell of a double-width
// right-to-left row of six cells is drawn in the row's fourth place, at
// screen columns 6 and 7, and keeps that place once its top row has
// scrolled above the view: the line's size and order are read from the
// scrollback.
// covers: text.bidi
#[test]
fn a_picture_whose_top_row_scrolled_above_the_view_keeps_its_right_to_left_double_place() {
    let mut t = term(12, 3, "\x1b[2;1H\x1b[2 k\x1b#6\x1b[2;3H");
    t.screen_mut().set_scrollback_max(100);
    t.input(b"\x1b_Ga=p,i=1,c=2,r=3,C=1,q=2\x1b\\");
    assert_eq!(places(&t), [(1, 6)]);
    t.input(b"\x1b[3;1H\n\n");
    assert_eq!(places(&t), [(-1, 6)]);
}

// ADR-1004 D8: each line of a right-to-left double-height pair draws two
// placeholder cells in its fourth and fifth places, at screen columns 6 to
// 9. A grid of two by two cells is 32×32 pixels there, which the 16×16
// picture fills: the top line shows source rows 0 to 7 and the bottom
// line rows 8 to 15.
// covers: text.bidi
#[test]
fn a_placeholder_grid_on_a_right_to_left_double_height_pair_covers_its_cells() {
    let top: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    let bottom: String = [P, D[1], D[0], P, D[1], D[1]].iter().collect();
    let mut t = term(12, 3, "\x1b[2 k\x1b#3\x1b[2G");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=2,q=2\x1b\\");
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{top}\r\n\x1b#4\x1b[2G{bottom}\x1b[m").as_bytes());
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    let parts: Vec<_> = frame
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.width, p.height, p.src_y))
        .collect();
    assert_eq!(parts, [(0, 6, 4, 32, 16, 0), (1, 6, 4, 32, 16, 8)]);
}

/// Sends image 2, then places image 1 with placement id 1 at the cursor
/// and image 2 as its child, `h` columns right of it and one row below.
fn parent_and_child(t: &mut Terminal, h: i32) {
    let mut s = b"\x1b_Ga=t,f=24,s=16,v=16,i=2,q=2;".to_vec();
    s.extend(b"////".repeat(2 * 128));
    s.extend(b"\x1b\\");
    t.input(&s);
    t.input(b"\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\");
    t.input(format!("\x1b_Ga=p,i=2,p=1,P=1,Q=1,H={h},V=1,q=2\x1b\\").as_bytes());
}

// ADR-1004 D8: a relative placement starts from where its parent is drawn
// on the screen, offset by H screen columns to the right and V rows down,
// so the two pictures keep their places side by side as pictures are
// never flipped. On an empty right-to-left row of twelve cells the fourth
// cell is drawn at screen column 8, so a parent placed there ends there
// and starts at column 7, and its child with H=1 starts at column 8. On a
// double-width row of six cells the fourth cell is drawn in place 2, at
// screen columns 4 and 5, so the parent starts at column 4 and the child
// at 5.
// covers: text.bidi
#[test]
fn a_child_of_a_picture_in_a_right_to_left_row_is_offset_from_where_its_parent_is_drawn() {
    for (start, parent) in [("\x1b[2 k\x1b[1;4H", 7), ("\x1b[2 k\x1b#6\x1b[1;4H", 4)] {
        let mut t = term(12, 3, start);
        parent_and_child(&mut t, 1);
        assert_eq!(places(&t), [(0, parent), (1, parent + 1)], "{start:?}");
    }
}
