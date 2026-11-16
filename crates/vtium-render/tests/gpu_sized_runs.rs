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
// display, decorated runs of scale 2 and 3 under an underline cursor, and
// a run between two underlined cells that change
// covers: text.kitty-sizing
#[test]
fn every_gpu_frame_of_sized_runs_matches_a_whole_draw() {
    let steps: [(&str, &[u8]); 11] = [
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
        (
            "decorated runs",
            b"\x1b[1;1H\x1b[4:3;9m\x1b]66;s=2;A\x07\x1b[m\x1b[4:4;53m\x1b]66;s=3;g\x07\x1b[m\x1b[4:5;58:5:1m\x1b]66;s=2:w=2;Hi\x07\x1b[m",
        ),
        ("an underline cursor on a run", b"\x1b[4 q\x1b[2;2H\x1b[?25h"),
        (
            "a run between two underlined cells",
            b"\x1b[?25l\x1b[5;1H\x1b[4mx\x1b[m\x1b]66;w=2;Ab\x07\x1b[4mx\x1b[m",
        ),
        (
            "both underlined cells change",
            b"\x1b[5;1H\x1b[4my\x1b[5;4Hy\x1b[m",
        ),
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

// Runs on double-size lines draw on the GPU path as on the CPU path: a
// run on a double-width line, a run too wide for its cells there, a run
// on each line of a double-height pair, and a run with a curly underline
// across three cells of a double-width line, at every turn of the display
// covers: text.kitty-sizing
#[test]
fn every_gpu_frame_of_runs_on_double_size_lines_matches_a_whole_draw() {
    let steps: [(&str, &[u8]); 4] = [
        ("a run on a double-width line", b"\x1b#6\x1b]66;w=2;A\x07"),
        (
            "a run too wide for its cells",
            b"\x1b[2;1H\x1b#6\x1b]66;w=1;ABCD\x07",
        ),
        (
            "a run on each line of a pair",
            b"\x1b[3;1H\x1b#3\x1b]66;w=2;A\x07\r\n\x1b#4\x1b]66;w=2;A\x07",
        ),
        (
            "a decorated run on a double-width line",
            b"\x1b[5;1H\x1b#6\x1b[4:3;9;53m\x1b]66;w=3;abc\x07\x1b[m",
        ),
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
            let differ = (0..cpu.height())
                .flat_map(|y| (0..cpu.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| cpu.pixel(x, y) != Some(gpu(x, y)))
                .count();
            assert_eq!(differ, 0, "{what}, {o:?}");
        }
    }
}

// A run on a line of the normal size draws its decorations from the
// normal sprites, so the frame that first draws it makes no texture of
// the doubled sprites; the frame that first draws a double-width line
// does
// covers: text.kitty-sizing
#[test]
fn a_run_on_a_normal_line_uploads_no_doubled_sprites() {
    let size = display(Orientation::Normal);
    let mut t = Terminal::new(COLS, ROWS, 0);
    t.input(b"\x1b[?25lA");
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
    let buffer: DrawTarget = gl.import_buffer(&size).unwrap();
    let mut r = Renderer::new(unifont(), size.width, size.height, Config::default());
    let mut since = Age::NEVER;
    let mut made = Vec::new();
    for bytes in [
        &b""[..],
        b"\x1b[2;1H\x1b[4m\x1b]66;s=2;A\x07",
        b"\x1b[5;1H\x1b#6\x1b[4mA",
    ] {
        t.input(bytes);
        let before = gl.calls().len();
        let frame = t.screen().frame(t.palette(), since);
        r.draw_gpu(&mut gl, &mut pipeline, &frame, &buffer, 1, &state(&t), &[])
            .unwrap();
        since = t.screen_mut().checkpoint();
        let textures = gl.calls()[before..]
            .iter()
            .filter(|c| matches!(c, vtium_render::gpu::Call::CreateTexture(..)))
            .count();
        made.push(textures);
    }
    assert_eq!(made[1], 0, "the run's frame makes no texture");
    assert_ne!(
        made[2], 0,
        "the double-width line's frame makes the doubled sprites"
    );
}

/// A face whose E is a color glyph, an opaque gradient, and whose every
/// other character is a mask that fills its cell.
struct Mixed;

impl vtium_font::Face for Mixed {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: CW,
            height: CH,
        }
    }
    fn size_step(&self) -> u32 {
        CH
    }
    fn has_glyph(&self, c: char) -> bool {
        c != ' '
    }
    fn draw(&self, text: &[char], _: vtium_font::Style) -> Option<vtium_font::Glyph> {
        use vtium_font::{Format, Glyph};
        if text == ['E'] {
            let px = (0..CH)
                .flat_map(|y| (0..CW).map(move |x| [(x * 32) as u8, (y * 16) as u8, 0x40, 0xff]))
                .flatten()
                .collect();
            Glyph::from_pixels(CW, CH, Format::Rgba, px)
        } else {
            Glyph::from_pixels(CW, CH, Format::Mask, vec![0xff; (CW * CH) as usize])
        }
    }
}

// A run that mixes text with a color glyph draws on the GPU path as on
// the CPU path, its text's mask and its color layer over it, plain, under
// an underline and selected-looking in reverse video, at every turn of the
// display
// covers: text.kitty-sizing
#[test]
fn a_run_of_text_and_a_color_glyph_draws_on_the_gpu_as_on_the_cpu() {
    let bytes = b"\x1b]66;s=2:w=2;AE\x07\x1b[3;1H\x1b[4;31m\x1b]66;s=2:w=3;EAE\x07\x1b[m\x1b[1;9H\x1b[7m\x1b]66;s=2:w=2;EA\x07\x1b[m";
    for o in [
        Orientation::Normal,
        Orientation::UpsideDown,
        Orientation::Right,
        Orientation::Left,
    ] {
        let size = display(o);
        let mut t = Terminal::new(COLS, ROWS, 0);
        t.input(b"\x1b[?25l");
        t.input(bytes);
        let frame = t.screen().frame(t.palette(), Age::NEVER);
        let mut cpu = Renderer::new(
            vtium_font::Font::new(Mixed),
            size.width,
            size.height,
            config(o),
        );
        let mut canvas = Canvas::new(size.width, size.height, 0);
        cpu.draw(&frame, &mut canvas, 0, &state(&t));
        let mut gl = StandIn::new();
        let mut pipeline =
            Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer: DrawTarget = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(
            vtium_font::Font::new(Mixed),
            size.width,
            size.height,
            config(o),
        );
        r.draw_gpu(&mut gl, &mut pipeline, &frame, &buffer, 1, &state(&t), &[])
            .unwrap();
        let gpu = gpu_pixel(&gl, o, size);
        let differ: Vec<(u32, u32)> = (0..canvas.height())
            .flat_map(|y| (0..canvas.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| canvas.pixel(x, y) != Some(gpu(x, y)))
            .collect();
        assert!(
            differ.is_empty(),
            "{o:?}: {} pixels differ, the first {:?}",
            differ.len(),
            differ.first()
        );
    }
}
