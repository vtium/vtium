//! Kitty's Unicode placeholders through the terminal (ADR-1100, D13):
//! vtium-term reads the cells a program writes U+10EEEE into, fills in the
//! marks a cell leaves out, and groups the cells into the runs it hands
//! the image store, each time from the cells as they are.

#![cfg(feature = "images")]

use vtium_term::images::LineKey;
use vtium_term::{PlaceholderRun, Terminal};

/// The placeholder.
const P: char = '\u{10EEEE}';

/// The first row and column diacritics, by value.
const D: [char; 10] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}',
    '\u{0346}', '\u{034A}',
];

/// A placeholder cell carrying the marks of `values`, in order.
fn cell(values: &[usize]) -> String {
    std::iter::once(P)
        .chain(values.iter().map(|&v| D[v]))
        .collect()
}

/// The key of the line at screen row `y`.
fn key(t: &Terminal, y: usize) -> LineKey {
    LineKey::new(t.screen().line(y).expect("a row").id().get())
}

/// A run on screen row `y` from column `col`, `cells` cells of image
/// `image` with no placement id, its first cell showing picture row and
/// column `at`.
fn run(t: &Terminal, y: usize, col: u32, cells: u32, image: u32, at: (u32, u32)) -> PlaceholderRun {
    PlaceholderRun {
        line: key(t, y),
        col,
        cells,
        image,
        placement: None,
        row: at.0,
        column: at.1,
    }
}

/// A 2 by 2 grid of image 42's placeholders at row 2, column 4, its cells
/// written as `cells` gives them, row by row.
fn grid(cells: [[&[usize]; 2]; 2]) -> Terminal {
    let mut t = Terminal::new(10, 4, 0);
    let mut s = String::from("\x1b[38;5;42m");
    for (y, row) in cells.iter().enumerate() {
        s += &format!("\x1b[{};4H", y + 2);
        for c in row {
            s += &cell(c);
        }
    }
    t.input(s.as_bytes());
    t
}

// ADR-1100, D13: a 2 by 2 grid of placeholder cells, each with its row and
// column, is two runs, one for each row of the picture, which together
// show its four quarters.
// covers: image.kitty-placeholders
#[test]
fn a_two_by_two_grid_reads_as_the_pictures_four_quarters() {
    let t = grid([[&[0, 0], &[0, 1]], [&[1, 0], &[1, 1]]]);
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![run(&t, 1, 3, 2, 42, (0, 0)), run(&t, 2, 3, 2, 42, (1, 0)),]
    );
}

// ADR-1100, D13, "Diacritics left out": a cell with no mark takes its left
// neighbor's row and the next column, one with only a row takes the next
// column, and one with a row and a column takes the neighbor's top byte,
// so cells that leave their marks out read as cells that carry them.
// covers: image.kitty-placeholders
#[test]
fn cells_that_leave_their_marks_out_read_as_cells_that_carry_them() {
    let full = grid([[&[0, 0], &[0, 1]], [&[1, 0], &[1, 1]]]);
    let sparse = grid([[&[0, 0], &[]], [&[1], &[]]]);
    let runs = |t: &Terminal| -> Vec<(u32, u32, u32, u32, u32)> {
        t.screen()
            .placeholder_runs()
            .iter()
            .map(|r| (r.col, r.cells, r.image, r.row, r.column))
            .collect()
    };
    assert_eq!(runs(&sparse), runs(&full));
    assert_eq!(runs(&sparse), vec![(3, 2, 42, 0, 0), (3, 2, 42, 1, 0)]);
    // the third mark gives the image id's top byte, and a cell with a row
    // and a column, in the column after its neighbor's, takes it too
    let mut t = Terminal::new(10, 4, 0);
    let s = format!(
        "\x1b[38;2;0;1;2m\x1b[58;5;7m{}{}{}",
        cell(&[1, 0, 3]),
        cell(&[1, 1]),
        cell(&[]),
    );
    t.input(s.as_bytes());
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![PlaceholderRun {
            placement: Some(7),
            ..run(&t, 0, 0, 3, (3 << 24) | 0x0102, (1, 0))
        }]
    );
    // a neighbor in other colors, or not in the column before, gives
    // nothing: the cell reads as row 0, column 0, top byte 0
    let mut t = Terminal::new(10, 4, 0);
    let s = format!(
        "\x1b[38;5;42m{}\x1b[38;5;43m{}\x1b[38;5;42m{} {}",
        cell(&[2, 2, 1]),
        cell(&[]),
        cell(&[2, 5]),
        cell(&[2]),
    );
    t.input(s.as_bytes());
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![
            run(&t, 0, 0, 1, (1 << 24) | 42, (2, 2)),
            run(&t, 0, 1, 1, 43, (0, 0)),
            run(&t, 0, 2, 1, 42, (2, 5)),
            run(&t, 0, 4, 1, 42, (2, 0)),
        ]
    );
}

// ADR-1100, D13: the default foreground, or an image id of 0, names no
// image, so such a cell belongs to no run, and the default underline color
// names no placement.
// covers: image.kitty-placeholders
#[test]
fn a_cell_that_names_no_image_belongs_to_no_run() {
    let mut t = Terminal::new(10, 4, 0);
    let s = format!(
        "{}\x1b[38;5;0m{}\x1b[38;5;9m{}",
        cell(&[0, 0]),
        cell(&[0, 1]),
        cell(&[0, 2]),
    );
    t.input(s.as_bytes());
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![run(&t, 0, 2, 1, 9, (0, 2))]
    );
}

// ADR-1100, D13, "When vtium-term reads them": the runs come from the
// cells each time, so a rewrap that moves the cells moves the runs, and a
// picture row the rewrap splits over two rows is two runs, the second
// starting at the picture column after the first's last.
// covers: image.kitty-placeholders
#[test]
fn a_rewrap_moves_the_runs_with_the_cells() {
    let mut t = Terminal::new(8, 4, 0);
    let s = format!(
        "\x1b[1;5H\x1b[38;5;42m{}{}{}{}",
        cell(&[0, 0]),
        cell(&[0, 1]),
        cell(&[0, 2]),
        cell(&[0, 3]),
    );
    t.input(s.as_bytes());
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![run(&t, 0, 4, 4, 42, (0, 0))]
    );
    t.screen_mut().resize(6, 4);
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![run(&t, 0, 4, 2, 42, (0, 0)), run(&t, 1, 0, 2, 42, (0, 2))]
    );
    t.screen_mut().resize(10, 4);
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![run(&t, 0, 4, 4, 42, (0, 0))]
    );
}

// ADR-1100, D13 and ADR-1001, "Placeholder cells": a picture whose rows
// start late in long lines, after accented text that spends the line's
// cluster budget and after another picture's placeholders, keeps every
// cell's marks, so each of its rows reads where it belongs.
// covers: image.kitty-placeholders
#[test]
fn a_picture_late_in_a_long_line_keeps_its_marks() {
    let mut t = Terminal::new(40, 4, 0);
    let accented: String = std::iter::once('e')
        .chain(std::iter::repeat_n('\u{301}', 31))
        .collect();
    let mut s = String::new();
    for y in 0..2 {
        s += &format!("\x1b[{};1H\x1b[39m", y + 1);
        s += &accented.repeat(6);
        s += "\x1b[38;5;7m";
        for x in 0..10 {
            s += &cell(&[y, x]);
        }
        s += "\x1b[30G\x1b[38;5;42m";
        for x in 0..4 {
            s += &cell(&[y, x]);
        }
    }
    t.input(s.as_bytes());
    assert_eq!(
        t.screen().placeholder_runs(),
        vec![
            run(&t, 0, 6, 10, 7, (0, 0)),
            run(&t, 0, 29, 4, 42, (0, 0)),
            run(&t, 1, 6, 10, 7, (1, 0)),
            run(&t, 1, 29, 4, 42, (1, 0)),
        ]
    );
}

// ADR-1100, D13, "Damage": when the store reports a change to a virtual
// placement of an image, vtium-term marks every placeholder cell of that
// image on the screen's own lines, and no other cell.
// covers: image.kitty-placeholders
#[test]
fn a_change_to_a_virtual_placement_marks_its_images_placeholder_cells() {
    let mut t = grid([[&[0, 0], &[0, 1]], [&[1, 0], &[1, 1]]]);
    t.input(format!("\x1b[1;1H\x1b[38;5;9m{}", cell(&[0, 0])).as_bytes());
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().mark_placeholders(false, 42);
    let damage = t.screen().damage();
    let marked: Vec<(usize, usize)> = (0..4)
        .flat_map(|y| (0..10).map(move |x| (y, x)))
        .filter(|&(y, x)| damage.changed(y, x, seen))
        .collect();
    assert_eq!(marked, vec![(1, 3), (1, 4), (2, 3), (2, 4)]);
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().mark_placeholders(true, 42);
    assert!(
        !t.screen().damage().changed(1, 3, seen),
        "the alternate buffer is not shown"
    );
}
