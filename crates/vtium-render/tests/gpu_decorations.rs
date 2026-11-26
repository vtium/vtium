//! The decorations on the GPU path (ADR-1300 D6): every underline style,
//! the strike, the overline and the two lines over the glyph together draw
//! as the CPU path draws them, pixel for pixel.
//!
//! The cell shader reads each decoration from the sprites' texture, which
//! holds a cell for each kind in a grid. The stand-in GL draws the cells
//! through its model of the cell shader, so the test reads every kind from
//! its place in the grid, the second row of it too. Tier 2 judges the
//! shader itself on a real GL.
#![cfg(feature = "gpu")]

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs, UnderlineStyle};

const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// One cell of each decoration, a cell with an underline and both lines
/// over the glyph, which reads two places in the grid, and a bare letter.
fn screen() -> Screen {
    let mut s = Screen::new(10, 1, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    let plain = style(rgb(FG), rgb(BG));
    let underlines = [
        UnderlineStyle::Single,
        UnderlineStyle::Double,
        UnderlineStyle::Curly,
        UnderlineStyle::Dotted,
        UnderlineStyle::Dashed,
    ];
    for (col, u) in underlines.into_iter().enumerate() {
        let mut st = plain;
        st.attrs.set_underline(u);
        s.text(col, 0, "x", st);
    }
    for (col, lines) in [
        (5, &[Attrs::STRIKE][..]),
        (6, &[Attrs::OVERLINE]),
        (7, &[Attrs::STRIKE, Attrs::OVERLINE]),
    ] {
        let mut st = plain;
        for &a in lines {
            st.attrs.insert(a);
        }
        s.text(col, 0, "x", st);
    }
    let mut st = plain;
    st.attrs.set_underline(UnderlineStyle::Dashed);
    st.attrs.insert(Attrs::STRIKE);
    st.attrs.insert(Attrs::OVERLINE);
    s.text(8, 0, "x", st);
    s.text(9, 0, "x", plain);
    s
}

// covers: sgr.underline-style
#[test]
fn every_decoration_draws_on_the_gpu_path_as_on_the_cpu_path() {
    let mut s = screen();
    // the grid inside a border of 2 pixels across and 1 down
    let size = Size::new(10 * 8 + 4, 16 + 2);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let mut cpu = Renderer::new(unifont(), size.width, size.height, Config::default());
    let mut canvas = Canvas::new(size.width, size.height, 0);
    cpu.draw(&s.frame(Age::NEVER), &mut canvas, 0, &st);

    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
    let buffer = gl.import_buffer(&size).unwrap();
    let mut gpu = Renderer::new(unifont(), size.width, size.height, Config::default());
    gpu.draw_gpu(
        &mut gl,
        &mut pipeline,
        &s.frame(Age::NEVER),
        &buffer,
        0,
        &st,
        &[],
    )
    .unwrap();
    s.damage.checkpoint();
    let upright = gl
        .frame_names()
        .iter()
        .rev()
        .find_map(|&n| gl.frame_pixels(n).filter(|(f, _)| *f == size))
        .unwrap()
        .1;

    let differ: Vec<(u32, u32, u32, u32)> = (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let c = canvas.pixel(x, y)?;
            let p = upright[(y * size.width + x) as usize];
            (c != p).then_some((x, y, c, p))
        })
        .collect();
    assert!(
        differ.is_empty(),
        "{} pixels differ from the CPU path's, the first (x, y, cpu, gpu) {:x?}",
        differ.len(),
        differ.first()
    );
    // the test is not vacuous: every cell draws differently, the bare
    // letter at the end too, so each decoration drew
    let cells: Vec<Vec<Option<u32>>> = (0..10)
        .map(|col| {
            (0..16)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .map(|(x, y)| canvas.pixel(2 + col * 8 + x, 1 + y))
                .collect()
        })
        .collect();
    for (i, a) in cells.iter().enumerate() {
        for (j, b) in cells.iter().enumerate().skip(i + 1) {
            assert_ne!(a, b, "cells {i} and {j} draw the same pixels");
        }
    }
}
