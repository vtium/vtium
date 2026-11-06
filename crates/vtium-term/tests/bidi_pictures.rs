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
