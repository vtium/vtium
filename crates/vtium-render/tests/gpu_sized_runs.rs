//! Sized runs, the blocks of cells that kitty's text sizing protocol
//! (OSC 66) writes, on the GPU path: each frame the GPU path composes
//! matches the CPU path's whole drawing of the same screen, pixel for
//! pixel, at every turn of the display.
//!
//! Each cell of a run draws its piece of the run's block as a one-cell
//! job, so the cell shader draws it as it draws any narrow glyph. The
//! stand-in GL draws through its model of the cell shader, and tier 2
//! judges the shader itself on a real GL.
#![cfg(feature = "gpu")]

mod common;

use common::unifont;
use vtium_render::gpu::{DrawTarget, Gl, Pipeline, Size, StandIn};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, Terminal};

const CW: u32 = 8;
const CH: u32 = 16;
const COLS: usize = 12;
const ROWS: usize = 6;

fn display(o: Orientation) -> Size {
    let (w, h) = (COLS as u32 * CW + 4, ROWS as u32 * CH + 2);
    match o {
        Orientation::Right | Orientation::Left => Size::new(h, w),
        Orientation::Normal | Orientation::UpsideDown => Size::new(w, h),
    }
}

fn config(o: Orientation) -> Config {
    Config {
        orientation: o,
        ..Config::default()
    }
}

fn state(t: &Terminal) -> DrawState {
    DrawState {
        cursor_on: true,
        background: t.palette().bg(),
        ..DrawState::default()
    }
}

/// `t`'s screen drawn whole on the CPU path, turned as `o` says.
fn whole(t: &Terminal, o: Orientation) -> Canvas {
    let size = display(o);
    let mut r = Renderer::new(unifont(), size.width, size.height, config(o));
    let mut c = Canvas::new(size.width, size.height, 0);
    r.draw(
        &t.screen().frame(t.palette(), Age::NEVER),
        &mut c,
        0,
        &state(t),
    );
    c
}

/// The upright frame the stand-in GL composed last, as `o` turns it back
/// to the display's pixels: `(x, y)` of the display at its index.
fn gpu_pixel(gl: &StandIn, o: Orientation, size: Size) -> impl Fn(u32, u32) -> u32 + '_ {
    let up = match o {
        Orientation::Right | Orientation::Left => Size::new(size.height, size.width),
        Orientation::Normal | Orientation::UpsideDown => size,
    };
    let pixels = gl
        .frame_names()
        .iter()
        .rev()
        .find_map(|&n| gl.frame_pixels(n).filter(|(s, _)| *s == up))
        .unwrap()
        .1;
    let (w, h) = (size.width, size.height);
    move |x, y| {
        let (ux, uy, stride) = match o {
            Orientation::Normal => (x, y, w),
            Orientation::UpsideDown => (w - 1 - x, h - 1 - y, w),
            Orientation::Right => (y, w - 1 - x, h),
            Orientation::Left => (h - 1 - y, x, h),
        };
        pixels[(uy * stride + ux) as usize]
    }
}

// Each frame of a screen whose sized runs come and go draws on the GPU
// path as the CPU path draws it whole: runs of scale 2 and 3, text in one
// block with w, a fraction at each alignment, a block cursor on a run, an
// overwrite that erases a run, and a run scrolled up, at every turn of the
// display
// covers: text.kitty-sizing
#[test]
fn every_gpu_frame_of_sized_runs_matches_a_whole_draw() {
    let steps: [(&str, &[u8]); 7] = [
        ("runs of scale 2 and 3", b"\x1b]66;s=2;A\x07\x1b]66;s=3;g\x07"),
        ("text in one block", b"\x1b[4;1H\x1b]66;s=2:w=2;AB\x07"),
        (
            "a fraction at each alignment",
            b"\x1b[1;9H\x1b]66;s=2:n=1:d=2:v=1:h=1;x\x07\x1b]66;n=1:d=2:w=1;Ha\x07\x1b]66;s=2:n=2:d=3:v=2:h=2;M\x07",
        ),
        ("a cursor on a run", b"\x1b[?25h\x1b[2;2H"),
        ("an overwrite that erases a run", b"\x1b[1;3Hz\x1b[?25l"),
        ("colors on a run", b"\x1b[4;5H\x1b[31;44m\x1b]66;s=2;Q\x07\x1b[m"),
        ("a scroll", b"\x1b[6;1H\n\n"),
    ];
    for o in [
        Orientation::Normal,
        Orientation::UpsideDown,
        Orientation::Right,
        Orientation::Left,
    ] {
        let size = display(o);
        let mut t = Terminal::new(COLS, ROWS, 0);
        t.input(b"\x1b[?25l");
        let mut gl = StandIn::new();
        let mut pipeline =
            Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer: DrawTarget = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(unifont(), size.width, size.height, config(o));
        let mut since = Age::NEVER;
        for (what, bytes) in steps {
            t.input(bytes);
            let frame = t.screen().frame(t.palette(), since);
            r.draw_gpu(&mut gl, &mut pipeline, &frame, &buffer, 1, &state(&t), &[])
                .unwrap();
            since = t.screen_mut().checkpoint();
            let cpu = whole(&t, o);
            let gpu = gpu_pixel(&gl, o, size);
            let differ: Vec<(u32, u32)> = (0..cpu.height())
                .flat_map(|y| (0..cpu.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| cpu.pixel(x, y) != Some(gpu(x, y)))
                .collect();
            assert!(
                differ.is_empty(),
                "{what}, {o:?}: {} pixels differ, the first {:?}",
                differ.len(),
                differ.first()
            );
        }
    }
}
