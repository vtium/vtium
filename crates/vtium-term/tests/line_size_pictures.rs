//! Pictures and kitty placeholder runs on double-size lines (ADR-1004 D8).
//! A double-width or double-height line shows one cell for every two
//! screen columns, and the cursor's column on it counts the line's cells.
//! A picture starts at the left edge of the screen columns where the cell
//! it was placed in is drawn, and a run of placeholder cells covers the
//! screen columns where its cells are drawn, twice as wide as on a line of
//! single size.
#![cfg(feature = "images")]

use vtium_term::images::{Kitty, Switches};
use vtium_term::{Age, ImageString, Palette, Terminal};

const P: char = '\u{10EEEE}';
const D: [char; 2] = ['\u{0305}', '\u{030D}'];

/// A terminal `cols` by `rows` that has read `start`, holding image 1,
/// 16×16 pixels, which is two cells of 8×16 pixels wide and one high.
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
    let mut s = b"\x1b_Ga=t,f=24,s=16,v=16,i=1,q=2;".to_vec();
    s.extend(b"////".repeat(2 * 128));
    s.extend(b"\x1b\\");
    t.input(&s);
    t
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

/// The column each entry of the screen's placement list starts at.
fn listed(t: &Terminal) -> Vec<u32> {
    t.screen().placements().iter().map(|p| p.col).collect()
}

const PLACE: &str = "\x1b_Ga=p,i=1,c=2,r=1,C=1,q=2\x1b\\";

// ADR-1004 D8: on a double-width line the cursor's column counts the
// line's cells, and the cell in column n is drawn at screen columns 2n and
// 2n + 1. A picture placed at the line's first, second and third cell
// starts at screen column 0, 2 and 4, keeping its own width.
// covers: image.kitty, text.line-size
#[test]
fn a_picture_on_a_double_width_line_starts_where_its_cell_is_drawn() {
    for (at, col) in [("\r", 0), ("\x1b[2G", 2), ("\x1b[3G", 4)] {
        let mut t = term(10, 2, &format!("\x1b#6ab{at}"));
        t.input(PLACE.as_bytes());
        assert_eq!(boxes(&t), [(col, 2, 16)], "placed after {at:?}");
        assert_eq!(listed(&t), [col], "placed after {at:?}");
    }
}

// ADR-1004 D8: each line of a double-height pair shows half the screen's
// columns too, so a picture placed on the pair's top line starts where its
// cell is drawn there.
// covers: image.kitty, text.line-size
#[test]
fn a_picture_on_a_double_height_pair_starts_where_its_cell_is_drawn() {
    for (at, col) in [("\x1b[H", 0), ("\x1b[1;2H", 2), ("\x1b[1;3H", 4)] {
        let mut t = term(10, 3, &format!("\x1b#3ab\r\n\x1b#4ab{at}"));
        t.input(PLACE.as_bytes());
        assert_eq!(boxes(&t), [(col, 2, 16)], "placed after {at:?}");
    }
}

// ADR-1004 D8: a run of placeholder cells on a double-width line covers
// the screen columns where its cells are drawn. Its two cells are drawn
// four screen columns wide, so the run's entry is four columns and 32
// pixels wide, the picture stretched to twice its width.
// covers: image.kitty-placeholders, text.line-size
#[test]
fn a_placeholder_run_on_a_double_width_line_covers_its_cells() {
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    for (at, col) in [("", 0), ("\x1b[2G", 2)] {
        let mut t = term(12, 2, &format!("\x1b#6{at}"));
        t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
        t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
        assert_eq!(boxes(&t), [(col, 4, 32)], "written after {at:?}");
    }
}

// ADR-1004 D8: a line made double after a picture was placed on it shows
// only the first half of its cells. A picture placed in a cell the line no
// longer shows is not drawn, even on an odd width that leaves the last
// screen column free.
// covers: image.kitty, text.line-size
#[test]
fn a_picture_in_a_cell_a_double_width_line_hides_is_not_drawn() {
    let mut t = term(11, 2, "\x1b[6G");
    t.input(PLACE.as_bytes());
    assert_eq!(boxes(&t), [(5, 2, 16)]);
    t.input(b"\x1b#6");
    assert_eq!(boxes(&t), []);
    assert_eq!(listed(&t), []);
}
