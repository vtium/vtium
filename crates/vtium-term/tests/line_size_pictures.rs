//! Pictures and kitty placeholder runs on double-size lines (ADR-1004 D8).
//! A double-width or double-height line shows one cell for every two
//! screen columns, and the cursor's column on it counts the line's cells.
//! A picture starts at the left edge of the screen columns where the cell
//! it was placed in is drawn, and a run of placeholder cells covers the
//! screen columns where its cells are drawn, twice as wide as on a line of
//! single size, its picture keeping its aspect. A relative placement starts
//! from where its parent is drawn, offset in screen columns.
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
// four screen columns wide, so the run's entry is four columns wide. The
// 16×16 picture keeps its aspect in those 32×16 pixels, 16 pixels wide.
// covers: image.kitty-placeholders, text.line-size
#[test]
fn a_placeholder_run_on_a_double_width_line_covers_its_cells() {
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    for (at, col) in [("", 0), ("\x1b[2G", 2)] {
        let mut t = term(12, 2, &format!("\x1b#6{at}"));
        t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
        t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
        assert_eq!(boxes(&t), [(col, 4, 16)], "written after {at:?}");
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

/// The view row, the column, the width in columns and the width in pixels
/// of each entry of the frame, in draw order.
fn entries(t: &Terminal) -> Vec<(i64, u32, u32, u32)> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    frame
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.width))
        .collect()
}

/// The view row and the column of each entry of the screen's placement
/// list.
fn listed_at(t: &Terminal) -> Vec<(i64, u32)> {
    t.screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col))
        .collect()
}

/// Sends image 2, a copy of image 1, for a relative placement to show.
fn second(t: &mut Terminal) {
    let mut s = b"\x1b_Ga=t,f=24,s=16,v=16,i=2,q=2;".to_vec();
    s.extend(b"////".repeat(2 * 128));
    s.extend(b"\x1b\\");
    t.input(&s);
}

/// Places image 1 with placement id 1 at the cursor, then image 2 as its
/// child, `h` columns right of it and one row below.
fn parent_and_child(t: &mut Terminal, h: i32) {
    second(t);
    t.input(b"\x1b_Ga=p,i=1,p=1,C=1,q=2\x1b\\");
    t.input(format!("\x1b_Ga=p,i=2,p=1,P=1,Q=1,H={h},V=1,q=2\x1b\\").as_bytes());
}

// ADR-1004 D8: a relative placement starts from where its parent is drawn
// on the screen, offset by H screen columns and V rows, whatever the size
// of either line. The parent sits in the fourth cell of a double-width
// line, drawn at screen columns 6 and 7, so a child one row below on a
// line of single size starts at screen column 6 + H.
// covers: image.kitty-relative, text.line-size
#[test]
fn a_child_of_a_picture_on_a_double_width_line_is_offset_in_screen_columns() {
    for h in [0, 1] {
        let mut t = term(12, 3, "\x1b#6\x1b[1;4H");
        parent_and_child(&mut t, h);
        let col = 6 + u32::try_from(h).expect("a column");
        assert_eq!(entries(&t), [(0, 6, 2, 16), (1, col, 2, 16)], "H={h}");
        assert_eq!(listed_at(&t), [(0, 6), (1, col)], "H={h}");
    }
}

// ADR-1004 D8: a parent on a line of single size, at screen column 3,
// places its child one row below on a double-width line at screen column
// 3 + H. H counts screen columns, not the double line's cells, so H=4
// puts the child at screen column 7, inside the line's fourth cell.
// covers: image.kitty-relative, text.line-size
#[test]
fn a_child_on_a_double_width_line_is_offset_in_screen_columns() {
    for (h, col) in [(0, 3), (4, 7)] {
        let mut t = term(12, 3, "\x1b[2;1H\x1b#6\x1b[1;4H");
        parent_and_child(&mut t, h);
        assert_eq!(entries(&t), [(0, 3, 2, 16), (1, col, 2, 16)], "H={h}");
        assert_eq!(listed_at(&t), [(0, 3), (1, col)], "H={h}");
    }
}

// ADR-1004 D8: a virtual parent is drawn by its placeholder cells. Two
// cells in the second and third cell of a double-width line are drawn at
// screen columns 2 to 5, so the parent's top left is screen column 2. Its
// child starts H screen columns right of that, one row below with V=1 or
// on the same row with V=0. On a line of single size the cells are drawn
// at screen columns 1 and 2, and the child starts at screen column 1 + H.
// covers: image.kitty-relative, image.kitty-placeholders, text.line-size
#[test]
fn a_child_of_a_virtual_parent_on_a_double_width_line_is_offset_in_screen_columns() {
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    for (start, run, left) in [
        ("\x1b#6\x1b[2G", (0, 2, 4, 16), 2),
        ("\x1b[2G", (0, 1, 2, 16), 1),
    ] {
        for (h, v) in [(0, 1), (0, 0), (1, 0)] {
            let mut t = term(12, 3, start);
            second(&mut t);
            t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
            t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
            t.input(format!("\x1b_Ga=p,i=2,p=1,P=1,Q=5,H={h},V={v},q=2\x1b\\").as_bytes());
            let child = (i64::from(v), left + h, 2, 16);
            assert_eq!(entries(&t), [run, child], "{start:?} H={h} V={v}");
        }
    }
}

// ADR-1004 D8: a picture placed in the third cell of a double-width line
// starts at screen column 4, and keeps that column once its top row has
// scrolled above the view: the line's size is read from the scrollback.
// covers: image.kitty, text.line-size
#[test]
fn a_picture_whose_top_row_scrolled_above_the_view_keeps_its_double_width_column() {
    let mut t = term(12, 3, "\x1b[2;1H\x1b#6\x1b[2;3H");
    t.screen_mut().set_scrollback_max(100);
    t.input(b"\x1b_Ga=p,i=1,c=2,r=3,C=1,q=2\x1b\\");
    assert_eq!(listed_at(&t), [(1, 4)]);
    t.input(b"\x1b[3;1H\n\n");
    assert_eq!(listed_at(&t), [(-1, 4)]);
}

// ADR-1004 D8: DECSWL makes the line single again, so it shows every cell
// again, and a picture that the double line hid is drawn again where it
// was placed.
// covers: image.kitty, text.line-size
#[test]
fn a_picture_a_double_width_line_hid_comes_back_after_decswl() {
    let mut t = term(11, 2, "\x1b[6G");
    t.input(PLACE.as_bytes());
    t.input(b"\x1b#6");
    assert_eq!(boxes(&t), []);
    t.input(b"\x1b#5");
    assert_eq!(boxes(&t), [(5, 2, 16)]);
    assert_eq!(listed(&t), [5]);
}

/// Each entry of the frame as (view row, column, columns, the visible
/// part's x, y, width and height, and the source rows it shows).
fn parts(t: &Terminal) -> Vec<(i64, u32, u32, [u32; 6])> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    frame
        .placements()
        .iter()
        .map(|p| {
            let part = [p.x, p.y, p.width, p.height, p.src_y, p.src_height];
            (p.row, p.col, p.cols, part)
        })
        .collect()
}

// ADR-1004 D8: on a double-width line a virtual placement's grid of two by
// one cells is drawn 32×16 pixels, and its 16×16 picture keeps its aspect:
// it fills the height, 16 pixels wide, centered with 8 pixels free on each
// side.
// covers: image.kitty-placeholders, text.line-size
#[test]
fn a_placeholder_picture_on_a_double_width_line_keeps_its_aspect() {
    let cells: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    let mut t = term(12, 2, "\x1b#6\x1b[2G");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=1,q=2\x1b\\");
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{cells}\x1b[m").as_bytes());
    assert_eq!(parts(&t), [(0, 2, 4, [8, 0, 16, 16, 0, 16])]);
}

// ADR-1004 D8: each row of a virtual placement's grid shows its own row of
// the picture fitted into the grid with that row's cells. A grid of two by
// two cells is 16×32 pixels on single-size rows, where the 16×16 picture
// fills the width and is centered, 8 pixels from the top: the first row
// shows source rows 0 to 7 in its lower 8 pixels. With 16-pixel-wide
// cells the grid is 32×32, where the picture fills it: the second row,
// on a double-width line, shows source rows 8 to 15, 32×16 pixels.
// covers: image.kitty-placeholders, text.line-size
#[test]
fn a_placeholder_grid_on_a_single_and_a_double_width_line_fits_each_row() {
    let top: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    let bottom: String = [P, D[1], D[0], P, D[1], D[1]].iter().collect();
    let mut t = term(12, 3, "\x1b[2G");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=2,q=2\x1b\\");
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{top}\r\n\x1b#6\x1b[2G{bottom}\x1b[m").as_bytes());
    assert_eq!(
        parts(&t),
        [
            (0, 1, 2, [0, 8, 16, 8, 0, 8]),
            (1, 2, 4, [0, 0, 32, 16, 8, 8]),
        ]
    );
}

// ADR-1004 D8: each line of a double-height pair draws its cells two
// screen columns wide. A grid of two by two cells across the pair is
// 32×32 pixels there, which the 16×16 picture fills: the top line shows
// source rows 0 to 7 and the bottom line rows 8 to 15, each 32×16 pixels
// at screen columns 2 to 5.
// covers: image.kitty-placeholders, text.line-size
#[test]
fn a_placeholder_grid_on_a_double_height_pair_covers_its_cells() {
    let top: String = [P, D[0], D[0], P, D[0], D[1]].iter().collect();
    let bottom: String = [P, D[1], D[0], P, D[1], D[1]].iter().collect();
    let mut t = term(12, 3, "\x1b#3\x1b[2G");
    t.input(b"\x1b_Ga=p,U=1,i=1,p=5,c=2,r=2,q=2\x1b\\");
    t.input(format!("\x1b[38;5;1m\x1b[58;5;5m{top}\r\n\x1b#4\x1b[2G{bottom}\x1b[m").as_bytes());
    assert_eq!(
        parts(&t),
        [
            (0, 2, 4, [0, 0, 32, 16, 0, 8]),
            (1, 2, 4, [0, 0, 32, 16, 8, 8]),
        ]
    );
}
