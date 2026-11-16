//! Sized runs, the blocks of cells that kitty's text sizing protocol
//! (OSC 66) writes, on the CPU path.
//!
//! Each test writes a run through vtium-term and checks the run's block of
//! pixels against the same character drawn at the normal size by the same
//! renderer: at a whole scale each pixel of the normal glyph fills a
//! square of the scale's side, and at a fraction the text sits where the
//! run's alignment puts it.

mod common;

use common::unifont;
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Terminal};

const CW: u32 = 8;
const CH: u32 = 16;
const COLS: usize = 12;
const ROWS: usize = 8;
const W: u32 = COLS as u32 * CW;
const H: u32 = ROWS as u32 * CH;

/// A terminal that has read `bytes`, with the cursor hidden.
fn term(bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 0);
    t.input(b"\x1b[?25l");
    t.input(bytes);
    t
}

fn state(t: &Terminal) -> DrawState {
    DrawState {
        cursor_on: true,
        background: t.palette().bg(),
        ..DrawState::default()
    }
}

/// `t`'s screen drawn whole into a new canvas.
fn draw(t: &Terminal) -> Canvas {
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let mut c = Canvas::new(W, H, 0);
    r.draw(
        &t.screen().frame(t.palette(), Age::NEVER),
        &mut c,
        0,
        &state(t),
    );
    c
}

/// The pixels of `c` in the rectangle `w` by `h` at `x`, `y`, row by row.
fn area(c: &Canvas, (x, y): (u32, u32), (w, h): (u32, u32)) -> Vec<Vec<u32>> {
    (y..y + h)
        .map(|py| (x..x + w).map(|px| c.pixel(px, py).unwrap()).collect())
        .collect()
}

/// The glyph of `text` printed at the normal size at the top left: its
/// pixels in `cells` cells.
fn normal(text: &str, cells: u32) -> Vec<Vec<u32>> {
    let t = term(text.as_bytes());
    area(&draw(&t), (0, 0), (cells * CW, CH))
}

/// `glyph` drawn `k` times as large: each pixel a square `k` pixels on a
/// side.
fn magnified(glyph: &[Vec<u32>], k: usize) -> Vec<Vec<u32>> {
    glyph
        .iter()
        .flat_map(|row| {
            let wide: Vec<u32> = row
                .iter()
                .flat_map(|&p| std::iter::repeat_n(p, k))
                .collect();
            std::iter::repeat_n(wide, k)
        })
        .collect()
}

/// The background of an empty screen.
fn background() -> u32 {
    area(&draw(&term(b"")), (0, 0), (1, 1))[0][0]
}

// A run of scale 2 draws its letter at twice its size across its block of
// 2 by 2 cells: each pixel of the letter at the normal size fills a square
// of 2 by 2 pixels, as a bitmap font's pixels multiply by whole factors
// covers: text.kitty-sizing
#[test]
fn a_run_of_scale_2_draws_each_pixel_of_a_bitmap_letter_twice_as_large() {
    let t = term(b"\x1b]66;s=2;A\x07");
    let got = area(&draw(&t), (0, 0), (2 * CW, 2 * CH));
    assert_eq!(got, magnified(&normal("A", 1), 2));
}

// A run of scale 3 multiplies each pixel by 3, across 3 by 3 cells, and a
// wide character's run across 6 by 3
// covers: text.kitty-sizing
#[test]
fn a_run_of_scale_3_multiplies_each_pixel_by_3() {
    let t = term(b"\x1b]66;s=3;g\x07");
    let got = area(&draw(&t), (0, 0), (3 * CW, 3 * CH));
    assert_eq!(got, magnified(&normal("g", 1), 3));
    let t = term("\x1b]66;s=3;\u{4e00}\x07".as_bytes());
    let got = area(&draw(&t), (0, 0), (6 * CW, 3 * CH));
    assert_eq!(got, magnified(&normal("\u{4e00}", 2), 3));
}

// A run with w draws all of its text in one block: s=2 with w=2 holds
// "AB" in 4 by 2 cells, each letter at twice its size
// covers: text.kitty-sizing
#[test]
fn a_run_with_a_width_draws_its_whole_text_in_one_block() {
    let t = term(b"\x1b]66;s=2:w=2;AB\x07");
    let got = area(&draw(&t), (0, 0), (4 * CW, 2 * CH));
    assert_eq!(got, magnified(&normal("AB", 2), 2));
}

/// The block of `w` by `h` pixels holding `glyph` at `(x, y)` over the
/// background.
fn placed(glyph: &[Vec<u32>], (w, h): (u32, u32), (x, y): (u32, u32)) -> Vec<Vec<u32>> {
    let bg = background();
    (0..h)
        .map(|py| {
            (0..w)
                .map(|px| {
                    let (gx, gy) = (px.wrapping_sub(x) as usize, py.wrapping_sub(y) as usize);
                    glyph
                        .get(gy)
                        .and_then(|row| row.get(gx))
                        .copied()
                        .unwrap_or(bg)
                })
                .collect()
        })
        .collect()
}

// s=2 with n=1 and d=2 draws the letter at its normal size inside a block
// of 2 by 2 cells, placed by v and h: at the top left, the bottom right,
// and the middle
// covers: text.kitty-sizing
#[test]
fn a_fraction_draws_the_text_smaller_where_v_and_h_place_it() {
    let a = normal("A", 1);
    let block = (2 * CW, 2 * CH);
    for (keys, at) in [
        ("v=0:h=0", (0, 0)),
        ("v=1:h=1", (CW, CH)),
        ("v=2:h=2", (CW / 2, CH / 2)),
        ("v=1:h=0", (0, CH)),
        ("v=0:h=2", (CW / 2, 0)),
    ] {
        let t = term(format!("\x1b]66;s=2:n=1:d=2:{keys};A\x07").as_bytes());
        let got = area(&draw(&t), (0, 0), block);
        assert_eq!(got, placed(&a, block, at), "{keys}");
    }
}

// n=1 and d=2 with w=1 fits two letters in one cell at half their size,
// at the cell's top left: each pixel of the half-size text is the pixel
// nearest its centre in the text at the normal size
// covers: text.kitty-sizing
#[test]
fn half_size_text_fits_two_letters_in_one_cell() {
    let t = term(b"\x1b]66;n=1:d=2:w=1;Ha\x07");
    let got = area(&draw(&t), (0, 0), (CW, CH));
    let ha = normal("Ha", 2);
    let half: Vec<Vec<u32>> = (0..CH / 2)
        .map(|y| {
            (0..CW)
                .map(|x| ha[(2 * y + 1) as usize][(2 * x + 1) as usize])
                .collect()
        })
        .collect();
    assert_eq!(got, placed(&half, (CW, CH), (0, 0)));
}

// Text wider than its block draws smaller until it fits: "ABCD" at s=1
// in one cell draws a quarter of its size, the width of the cell, and
// centred across the block's width
// covers: text.kitty-sizing
#[test]
fn text_wider_than_its_block_draws_smaller_until_it_fits() {
    let t = term(b"\x1b]66;w=1;ABCD\x07");
    let c = draw(&t);
    let bg = background();
    let block = area(&c, (0, 0), (CW, CH));
    let inked: Vec<(usize, usize)> = block
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter(|(_, p)| **p != bg)
                .map(move |(x, _)| (x, y))
        })
        .collect();
    let (xs, ys): (Vec<usize>, Vec<usize>) = inked.iter().copied().unzip();
    assert!(!inked.is_empty(), "the text draws");
    assert!(
        ys.iter().max().unwrap() - ys.iter().min().unwrap() < 4,
        "a quarter of 16 rows tall: {ys:?}"
    );
    assert!(
        xs.iter().min().unwrap() <= &1 && xs.iter().max().unwrap() >= &6,
        "across the cell: {xs:?}"
    );
    // the cell after the run stays empty
    assert!(
        area(&c, (CW, 0), (CW, CH))
            .iter()
            .flatten()
            .all(|&p| p == bg)
    );
}

// A frame drawn into a buffer that lacks only some of a run's cells draws
// the same pixels as a whole drawing: each cell draws its own piece
// covers: text.kitty-sizing
#[test]
fn a_later_frame_draws_each_cell_of_a_run_as_a_whole_drawing_does() {
    let mut t = term(b"x\x1b]66;s=2;M\x07");
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let mut c = Canvas::new(W, H, 0);
    r.draw(
        &t.screen().frame(t.palette(), Age::NEVER),
        &mut c,
        0,
        &state(&t),
    );
    let since = t.screen_mut().checkpoint();
    t.input(b"\x1b[1;1Hy");
    r.draw(&t.screen().frame(t.palette(), since), &mut c, 1, &state(&t));
    assert_eq!(area(&c, (0, 0), (W, H)), area(&draw(&t), (0, 0), (W, H)));
}

/// `t` with its cursor shown, steady in `shape` (DECSCUSR), at row 2,
/// column 2: the lower right cell of a run of scale 2 at the top left.
fn with_cursor(bytes: &[u8], shape: u8) -> Canvas {
    let mut t = term(bytes);
    t.input(format!("\x1b[{shape} q\x1b[2;2H\x1b[?25h").as_bytes());
    draw(&t)
}

// A block cursor on any cell of a run covers the whole block: every cell
// of it draws in reverse
// covers: text.kitty-sizing
#[test]
fn a_block_cursor_on_a_run_covers_the_whole_block() {
    let run = b"\x1b]66;s=2;A\x07";
    let plain = area(&draw(&term(run)), (0, 0), (2 * CW, 2 * CH));
    let (bg, fg) = (
        background(),
        plain
            .iter()
            .flatten()
            .copied()
            .find(|&p| p != background())
            .unwrap(),
    );
    let reversed: Vec<Vec<u32>> = plain
        .iter()
        .map(|row| row.iter().map(|&p| if p == bg { fg } else { bg }).collect())
        .collect();
    let got = area(&with_cursor(run, 2), (0, 0), (2 * CW, 2 * CH));
    assert_eq!(got, reversed);
}

// A bar cursor on a run stands along the block's left edge on both its
// rows, twice as thick at scale 2, and an underline cursor lies under the
// block's bottom row alone
// covers: text.kitty-sizing
#[test]
fn a_line_cursor_on_a_run_marks_the_blocks_edge() {
    let run = b"\x1b]66;s=2;A\x07";
    let plain = area(&draw(&term(run)), (0, 0), (2 * CW, 2 * CH));
    let bar = area(&with_cursor(run, 6), (0, 0), (2 * CW, 2 * CH));
    let fg = 0xe5e5e5;
    for (y, (got, was)) in bar.iter().zip(&plain).enumerate() {
        assert_eq!(&got[..2], &[fg, fg], "the bar's 2 columns on row {y}");
        assert_eq!(&got[2..], &was[2..], "the rest of row {y}");
    }
    let under = area(&with_cursor(run, 4), (0, 0), (2 * CW, 2 * CH));
    assert_eq!(
        &under[..CH as usize],
        &plain[..CH as usize],
        "the top row keeps its look"
    );
    assert_ne!(
        &under[CH as usize..],
        &plain[CH as usize..],
        "the bottom row draws the underline"
    );
}

// Two runs of the same letter at different scales each draw at their own
// scale, whichever the renderer drew first
// covers: text.kitty-sizing
#[test]
fn runs_of_one_letter_at_two_scales_each_draw_at_their_own() {
    let t = term(b"\x1b]66;s=2;A\x07\x1b]66;s=3;A\x07");
    let c = draw(&t);
    let a = normal("A", 1);
    assert_eq!(area(&c, (0, 0), (2 * CW, 2 * CH)), magnified(&a, 2));
    assert_eq!(area(&c, (2 * CW, 0), (3 * CW, 3 * CH)), magnified(&a, 3));
}
