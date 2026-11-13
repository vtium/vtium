//! Pictures inside left and right margins, through the terminal (ADR-1100,
//! D6): a region scroll inside the margins moves a rectangle, the part of a
//! sixel inside it with it and a kitty placement only when wholly inside
//! it; sixel scrolling scrolls the rectangle only from a column between
//! the margins; and the cursor after a picture stops or wraps where CUF
//! stops and CR goes.

#![cfg(feature = "images")]

mod common;

use common::rows;
use vtium_term::images::{Kitty, Sixel, Switches};
use vtium_term::{ImageString, Terminal};

/// A terminal of 12 by 6 cells of 8 by 16 pixels that offers sixel and
/// kitty graphics, with a budget of 1 GiB, every row filled with its own
/// letter, and the scroll region rows 2 to 5 and columns 3 to 8, counted
/// from 1: rows 1 to 4 and columns 2 to 7, counted from 0.
fn term() -> Terminal {
    let mut t = Terminal::new(12, 6, 100);
    t.screen_mut().set_image_limit(1 << 30);
    t.screen_mut().set_image_cell(8, 16).expect("a cell");
    t.set_image_switches(Switches {
        sixel: true,
        kitty: true,
        iterm2: false,
    });
    t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    for (row, letter) in ('a'..='f').enumerate() {
        t.input(format!("\x1b[{};1H{}", row + 1, letter.to_string().repeat(12)).as_bytes());
    }
    t.input(b"\x1b[?69h\x1b[2;5r\x1b[3;8s");
    t
}

/// A solid sixel picture of `w` by `h` dots, sized by raster attributes.
fn sixel(w: u32, h: u32) -> Vec<u8> {
    let mut data = format!("\"1;1;{w};{h}#1;2;0;0;100#1");
    for band in 0..h.div_ceil(6) {
        let bits = (h - band * 6).min(6);
        let six = char::from(0x3f + ((1u8 << bits) - 1));
        data += &format!("!{w}{six}-");
    }
    format!("\x1bP0;1;0q{data}\x1b\\").into_bytes()
}

/// A kitty transmission and placement of image `id`, `w` by `h` opaque
/// RGB pixels, with replies off and the cursor movement `c` asks.
fn kitty(id: u32, w: usize, h: usize, c: u8) -> Vec<u8> {
    let mut s = format!("\x1b_Ga=T,f=24,s={w},v={h},i={id},q=2,C={c};").into_bytes();
    s.extend(b"////".repeat(w * h));
    s.extend(b"\x1b\\");
    s
}

/// The placements as (row, col, cols, rows, src_x, src_y), in order.
fn shown(t: &Terminal) -> Vec<(i64, u32, u32, u32, u32, u32)> {
    let mut p: Vec<_> = t
        .screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows, p.src_x, p.src_y))
        .collect();
    p.sort_unstable();
    p
}

fn text(t: &Terminal) -> Vec<String> {
    rows(t.screen())
}

// A line feed at the bottom margin, between the left and right margins,
// scrolls the rectangle: a sixel inside it moves up with the text, and a
// sixel across the right margin splits there, its part inside moving and
// its part outside staying.
// covers: image.sixel
#[test]
fn a_sixel_inside_the_margins_scrolls_with_the_rectangle() {
    let mut t = term();
    t.input(b"\x1b[4;4H");
    t.input(&sixel(16, 32));
    t.input(b"\x1b[4;8H");
    t.input(&sixel(16, 32));
    assert_eq!(shown(&t), [(3, 3, 2, 2, 0, 0), (3, 7, 2, 2, 0, 0)]);
    assert_eq!(t.screen().cursor(), (7, 4));
    t.input(b"\n");
    assert_eq!(
        shown(&t),
        [
            // the first moves whole; of the second, the column inside the
            // margins moves and the column past the right margin stays
            (2, 3, 2, 2, 0, 0),
            (2, 7, 1, 2, 0, 0),
            (3, 8, 1, 2, 8, 0),
        ]
    );
    assert_eq!(text(&t)[1], "bbccccccbbbb");
    // two more: the band's top clips what leaves it
    t.input(b"\n\n");
    assert_eq!(
        shown(&t),
        [(1, 3, 2, 1, 0, 16), (1, 7, 1, 1, 0, 16), (3, 8, 1, 2, 8, 0)]
    );
}

// Sixel scrolling scrolls the rectangle only while the picture's left
// column is between the left and right margins, as a line feed does; from
// a column outside them nothing scrolls, and the picture runs past the
// bottom margin.
// covers: image.sixel
#[test]
fn a_sixel_outside_the_margins_scrolls_nothing() {
    let mut t = term();
    let before = text(&t);
    t.input(b"\x1b[4;11H");
    t.input(&sixel(8, 48));
    assert_eq!(text(&t), before);
    assert_eq!(shown(&t), [(3, 10, 1, 3, 0, 0)]);
    assert_eq!(t.screen().cursor(), (10, 4));
    // from inside the margins, the rectangle scrolls first so the picture
    // fits it whole
    t.input(b"\x1b[4;4H");
    t.input(&sixel(8, 48));
    assert_eq!(text(&t)[1], "bbccccccbbbb");
    assert!(shown(&t).contains(&(2, 3, 1, 3, 0, 0)));
}

// With mode 8452 the cursor after a sixel stops at the column CUF stops
// at: the right margin from inside the margins, the screen's last column
// from past them.
// covers: DEC-8452
#[test]
fn mode_8452_stops_the_cursor_at_the_right_margin() {
    let mut t = term();
    t.input(b"\x1b[?8452h\x1b[3;7H");
    t.input(&sixel(24, 16));
    assert_eq!(t.screen().cursor(), (7, 2));
    t.input(b"\x1b[2;11H");
    t.input(&sixel(16, 16));
    assert_eq!(t.screen().cursor(), (11, 1));
}

// A region scroll inside the margins moves a kitty placement wholly inside
// the rectangle, and leaves one that crosses its right edge where it is.
// covers: image.kitty
#[test]
fn a_kitty_placement_moves_only_when_wholly_inside_the_rectangle() {
    let mut t = term();
    t.input(b"\x1b[4;4H");
    t.input(&kitty(1, 8, 32, 1));
    t.input(b"\x1b[3;8H");
    t.input(&kitty(2, 16, 16, 1));
    t.input(b"\x1b[5;4H\n");
    assert_eq!(shown(&t), [(2, 3, 1, 2, 0, 0), (2, 7, 2, 1, 0, 0)]);
    // SD moves it back down, and the band's bottom clips it
    t.input(b"\x1b[2T");
    assert_eq!(shown(&t), [(2, 7, 2, 1, 0, 0), (4, 3, 1, 1, 0, 0)]);
}

// The cursor after a kitty placement that reaches past the column CUF
// stops at goes down one row more, to the column CR goes to: the left
// margin, from inside the margins or right of them.
// covers: image.kitty
#[test]
fn the_cursor_after_a_kitty_placement_wraps_at_the_right_margin() {
    let mut t = term();
    t.input(b"\x1b[2;7H");
    t.input(&kitty(1, 24, 16, 0));
    assert_eq!(t.screen().cursor(), (2, 2));
    t.input(b"\x1b[2;4H");
    t.input(&kitty(2, 16, 16, 0));
    assert_eq!(t.screen().cursor(), (5, 1));
    t.input(b"\x1b[2;11H");
    t.input(&kitty(3, 16, 16, 0));
    assert_eq!(t.screen().cursor(), (2, 2));
}
