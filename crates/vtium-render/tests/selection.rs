//! The selection draws inverted, cell by cell: a selection that starts and
//! ends inside a run of one style inverts exactly its cells, and dim mixes
//! the colours it left (`crates/vtium-term/src/frame.rs`). Every expected
//! colour is a literal.

mod common;

use std::collections::BTreeSet;
use std::ops::Range;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs};

const FG: u32 = 0xff_80_40;
const BG: u32 = 0x21_a0_c3;
/// Dim's mix of `FG` and `BG`, as `tests/dim.rs` works it out: each
/// channel halves both, so the selection's swap leaves the mix as it was.
const DIM: u32 = 0x8f_90_81;

fn draw(s: &Screen, selection: Range<usize>) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let frame = s.frame(Age::NEVER).with_selection(vec![selection]);
    r.draw(&frame, &mut c, 0, &st);
    c
}

/// The colours of the cell at `col`, the most used first: its background,
/// then its glyph's colour.
fn colours(c: &Canvas, col: u32) -> Vec<u32> {
    let px: Vec<u32> = (0..16)
        .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect();
    let set: BTreeSet<u32> = px.iter().copied().collect();
    let mut by_use: Vec<u32> = set.into_iter().collect();
    by_use.sort_by_key(|&p| core::cmp::Reverse(px.iter().filter(|&&q| q == p).count()));
    by_use
}

#[test]
fn a_selection_inside_a_run_inverts_exactly_its_cells() {
    let plain = style(rgb("ff8040"), rgb("21a0c3"));
    let mut s = Screen::new(8, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "xxxxxx", plain);
    let c = draw(&s, 2..5);
    for col in 0..6 {
        let want = if (2..5).contains(&col) {
            [FG, BG]
        } else {
            [BG, FG]
        };
        assert_eq!(
            colours(&c, col),
            want,
            "cell {col}: {:x?}",
            colours(&c, col)
        );
    }
}

// dim mixes the colours the selection left, so the old foreground is the
// selected cell's background
#[test]
fn a_selected_dim_cell_mixes_the_swapped_colours() {
    let mut dim = style(rgb("ff8040"), rgb("21a0c3"));
    dim.attrs.insert(Attrs::DIM);
    let mut s = Screen::new(4, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "xxx", dim);
    let c = draw(&s, 1..2);
    assert_eq!(colours(&c, 0), [BG, DIM], "unselected");
    assert_eq!(colours(&c, 1), [FG, DIM], "selected");
    assert_eq!(colours(&c, 2), [BG, DIM], "unselected after");
}
