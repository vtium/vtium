//! The selection on a pair of double-height lines, drawn on the CPU path
//! and on the GPU path from a real terminal's frame.
//!
//! A top line (ESC # 3) and its bottom line (ESC # 4) draw one line of
//! tall characters: the top line shows each character's upper half and
//! the bottom line its lower half. A drag across the doubled b on the top
//! line selects the b, and both halves of the b draw in the selection's
//! colors. The expected drawing is the same terminal drawn with column 1
//! selected on both rows, set by hand.
//!
//! References:
//! - ADR-1308 D1, "Selection on a double-size line follows the cursor's
//!   rule": a selection that covers a character on either line of a pair
//!   highlights both halves of that doubled character.

mod common;

use std::ops::Range;

use common::unifont;
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Frame, Terminal};

const COLS: usize = 10;
const ROWS: usize = 2;
const W: u32 = COLS as u32 * 8;
const H: u32 = ROWS as u32 * 16;

/// A pair of double-height lines that reads abcd, with the selection made
/// as the pointer makes it: a drag from screen column 2 to screen column 3
/// of the line `on`, the two display cells of the doubled b.
fn pair(on: usize) -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 0);
    t.input(b"\x1b#3abcd\r\n\x1b#4abcd\x1b[?25l");
    t.screen_mut().select_start(2, on);
    t.screen_mut().select_to(3, on);
    t
}

fn state(t: &Terminal) -> DrawState {
    DrawState {
        background: t.palette().bg(),
        ..DrawState::default()
    }
}

/// `t` drawn whole on the CPU path with the selection its screen made, or
/// with the selected columns `sel` in its place when they are given.
fn cpu(t: &Terminal, sel: Option<Vec<Range<usize>>>) -> Canvas {
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let mut c = Canvas::new(W, H, 0);
    let frame: Frame<'_> = t.screen().frame(t.palette(), Age::NEVER);
    let frame = match sel {
        Some(sel) => frame.with_selection(sel),
        None => frame,
    };
    r.draw(&frame, &mut c, 0, &state(t));
    c
}

fn pixels(c: &Canvas) -> Vec<u32> {
    (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

// A drag over the doubled b on either line of the pair draws both halves
// of the b selected on the CPU path.
// covers: text.line-size, clipboard.selection
#[test]
fn a_selection_on_either_line_of_a_pair_draws_both_halves_selected() {
    let both = vec![1..2, 1..2];
    for on in [0, 1] {
        let t = pair(on);
        let want = pixels(&cpu(&t, Some(both.clone())));
        let top_only = pixels(&cpu(&t, Some(std::iter::once(1..2).collect())));
        assert_ne!(
            want, top_only,
            "the bottom half of the b shows the selection"
        );
        assert_eq!(
            pixels(&cpu(&t, None)),
            want,
            "the drag on line {on} draws both halves of the b selected"
        );
    }
}

// The GPU path draws the same selection as the CPU path draws with both
// rows selected by hand, pixel for pixel.
// covers: text.line-size, clipboard.selection
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_path_draws_both_halves_of_a_selected_pair_character() {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    for on in [0, 1] {
        let t = pair(on);
        let size = Size::new(W, H);
        let mut gl = StandIn::new();
        let mut pipeline =
            Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(unifont(), W, H, Config::default());
        let frame = t.screen().frame(t.palette(), Age::NEVER);
        r.draw_gpu(&mut gl, &mut pipeline, &frame, &buffer, 1, &state(&t), &[])
            .unwrap();
        let gpu = gl
            .frame_names()
            .iter()
            .rev()
            .find_map(|&n| gl.frame_pixels(n).filter(|(s, _)| *s == size))
            .unwrap()
            .1
            .to_vec();
        let want = pixels(&cpu(&t, Some(vec![1..2, 1..2])));
        assert_eq!(
            gpu, want,
            "the GPU path draws the drag on line {on} as both halves of the b selected"
        );
    }
}
