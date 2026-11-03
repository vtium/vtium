//! Kitty's virtual placements drawn through the placeholder cells a
//! program writes (ADR-1100, D13): vtium-term hands the image store the
//! runs it reads from the cells for a frame, for the decisions that take a
//! virtual parent's children by the cells they draw on, and marks the
//! cells of a virtual placement the store reports changed.

#![cfg(feature = "images")]

use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

/// The placeholder.
const P: char = '\u{10EEEE}';

/// The first row and column diacritics, by value.
const D: [char; 4] = ['\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}'];

/// A terminal of `cols` by `rows` cells of 8 by 16 pixels that offers
/// kitty graphics.
fn term(cols: usize, rows: usize) -> Terminal {
    let mut t = Terminal::new(cols, rows, 100);
    t.screen_mut().set_image_limit(1 << 30);
    t.screen_mut().set_image_cell(8, 16).expect("a cell");
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    t
}

/// Transmits kitty image `id`, `w` by `h` cells of 8 by 16 opaque white
/// RGB pixels, with replies off.
fn transmit(t: &mut Terminal, id: u32, w: usize, h: usize) {
    let mut s = format!("\x1b_Ga=t,f=24,s={},v={},i={id},q=2;", w * 8, h * 16).into_bytes();
    s.extend(b"////".repeat(w * h * 128));
    s.extend(b"\x1b\\");
    t.input(&s);
}

/// A placeholder cell with the row and column marks `row`, `col`.
fn cell(row: usize, col: usize) -> String {
    [P, D[row], D[col]].iter().collect()
}

/// A terminal 12 by 6 with image 42, 2 by 2 cells, placed virtually with
/// placement id 5, and its 2 by 2 grid of placeholder cells at row 1,
/// column 3.
fn grid() -> Terminal {
    let mut t = term(12, 6);
    transmit(&mut t, 42, 2, 2);
    t.input(b"\x1b_Ga=p,U=1,i=42,p=5,c=2,r=2,q=2\x1b\\");
    let s = format!(
        "\x1b[38;5;42m\x1b[58;5;5m\x1b[2;4H{}{}\x1b[3;4H{}{}\x1b[m",
        cell(0, 0),
        cell(0, 1),
        cell(1, 0),
        cell(1, 1)
    );
    t.input(s.as_bytes());
    t
}

/// The frame's entries as (view row, column, columns, rows, source y).
fn entries(t: &Terminal) -> Vec<(i64, u32, u32, u32, u32)> {
    t.screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows, p.src_y))
        .collect()
}

// ADR-1100, D13: the frame hands the store the runs on the rows it shows,
// and the store draws each run's slice of the virtual placement's picture.
// covers: image.kitty-placeholders
#[test]
fn a_virtual_placement_draws_its_slices_where_its_cells_are() {
    let t = grid();
    assert_eq!(entries(&t), vec![(1, 3, 2, 1, 0), (2, 3, 2, 1, 16)]);
}

// D13: the cells move as text, so inserting two characters before the
// first row's cells moves that row's slice and leaves the other.
// covers: image.kitty-placeholders
#[test]
fn moving_the_cells_as_text_moves_the_picture() {
    let mut t = grid();
    t.input(b"\x1b[2;1H\x1b[2@");
    assert_eq!(entries(&t), vec![(1, 5, 2, 1, 0), (2, 3, 2, 1, 16)]);
}

// D13, "Damage": deleting the virtual placement marks its image's
// placeholder cells changed, and no other cell.
// covers: image.kitty-placeholders
#[test]
fn deleting_a_virtual_placement_marks_its_cells() {
    let mut t = grid();
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b_Ga=d,d=i,i=42,q=2\x1b\\");
    assert_eq!(entries(&t), vec![]);
    let damage = t.screen().damage();
    let marked: Vec<(usize, usize)> = (0..6)
        .flat_map(|y| (0..12).map(move |x| (y, x)))
        .filter(|&(y, x)| damage.changed(y, x, seen))
        .collect();
    assert_eq!(marked, vec![(1, 3), (1, 4), (2, 3), (2, 4)]);
}

/// [`grid`] with image 43, one cell, placed as a child of the virtual
/// placement three columns right of its origin, at row 1, column 6.
fn with_child() -> Terminal {
    let mut t = grid();
    transmit(&mut t, 43, 1, 1);
    t.input(b"\x1b_Ga=p,i=43,p=1,P=42,Q=5,H=3,q=2\x1b\\");
    assert!(entries(&t).contains(&(1, 6, 1, 1, 0)), "{:?}", entries(&t));
    t
}

// D13's dated note: ED 2 takes a child of a virtual parent by the cells it
// draws on, read before ED 2 erases the placeholder cells; the virtual
// placement, on no line, stays.
// covers: image.kitty-relative
#[test]
fn ed_2_removes_a_child_of_a_virtual_parent() {
    let mut t = with_child();
    let before = t.screen().images().placement_count();
    t.input(b"\x1b[2J");
    assert_eq!(t.screen().images().placement_count(), before - 1);
}

// D13: a kitty delete of every placement on the screen (d=a) takes a child
// of a virtual parent by the cells it draws on.
// covers: image.kitty-relative
#[test]
fn a_delete_of_every_placement_removes_a_child_of_a_virtual_parent() {
    let mut t = with_child();
    let before = t.screen().images().placement_count();
    t.input(b"\x1b_Ga=d,d=a,q=2\x1b\\");
    assert_eq!(t.screen().images().placement_count(), before - 1);
    assert_eq!(entries(&t), vec![(1, 3, 2, 1, 0), (2, 3, 2, 1, 16)]);
}

/// The image work one kitty delete by cell does on `t`: an input call
/// counts its image work afresh.
fn delete_work(t: &mut Terminal) -> u64 {
    t.input(b"\x1b_Ga=d,d=p,x=1,y=1,q=2\x1b\\");
    t.screen().images().work()
}

// D13: vtium-term reads the screen's placeholder runs for a decision only
// while a buffer holds a child of a virtual placement, so a delete by cell
// does the same work with placeholder cells on the screen as without them,
// and more once such a child exists.
// covers: image.kitty-placeholders
#[test]
fn nothing_reads_runs_while_no_virtual_placement_has_a_child() {
    let mut bare = term(12, 6);
    transmit(&mut bare, 42, 2, 2);
    bare.input(b"\x1b_Ga=p,U=1,i=42,p=5,c=2,r=2,q=2\x1b\\");
    let mut cells = grid();
    assert_eq!(delete_work(&mut cells), delete_work(&mut bare));
    let mut child = with_child();
    assert!(delete_work(&mut child) > delete_work(&mut bare));
}
