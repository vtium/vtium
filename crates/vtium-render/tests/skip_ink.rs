//! Underlines skip the ink of descenders: vtium draws a cell's underline
//! over its glyph, and leaves the line off every pixel within one pixel of
//! the glyph's ink along the cell's row. The line stops just short of each
//! descender, leaves a gap of one pixel, and goes on after it. Every
//! underline style and a colored underline do this, on the CPU path and on
//! the GPU path, at every turn of the display. The underline cursor is not
//! text decoration, so its line draws whole under the glyph.
//!
//! Each test judges the drawn pixels against two other drawings of the same
//! screen: the letters with no underline, and the underlines under blank
//! cells. A pixel within one pixel of a letter's ink, across the row, takes
//! the letter's pixel, and every other pixel takes the underline's.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, Color, CursorShape, Style, UnderlineStyle};

const FG: &str = "c0c0c0";
const BG: &str = "101010";
/// The underline's own color, SGR 58.
const LINE: &str = "ff2040";
const TEXT: &str = "gjpqy";

const STYLES: [UnderlineStyle; 5] = [
    UnderlineStyle::Single,
    UnderlineStyle::Double,
    UnderlineStyle::Curly,
    UnderlineStyle::Dotted,
    UnderlineStyle::Dashed,
];

const COLS: usize = TEXT.len();
const ROWS: usize = STYLES.len();
/// The display holds the grid of 8×16 cells exactly, with no margin.
const W: u32 = COLS as u32 * 8;
const H: u32 = ROWS as u32 * 16;

fn plain() -> Style {
    style(rgb(FG), rgb(BG))
}

fn underlined(u: UnderlineStyle) -> Style {
    let mut st = plain();
    st.attrs.set_underline(u);
    st.underline_color = Color::Rgb(rgb(LINE));
    st
}

/// A row for each underline style, each the letters of [`TEXT`], whose
/// descenders cross the underline's rows; with `text` false the cells are
/// blank, and with `lines` false they have no underline.
fn screen(text: bool, lines: bool) -> Screen {
    let mut s = Screen::new(COLS, ROWS, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    for (row, u) in STYLES.into_iter().enumerate() {
        let st = if lines { underlined(u) } else { plain() };
        let chars: String = if text { TEXT.into() } else { " ".repeat(COLS) };
        s.text(0, row, &chars, st);
    }
    s
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn on the CPU path on a display turned by `o`.
fn draw(s: &Screen, o: Orientation) -> Canvas {
    let (w, h) = if o == Orientation::Right || o == Orientation::Left {
        (H, W)
    } else {
        (W, H)
    };
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    assert_eq!(r.grid_size(), (s.cols, s.rows));
    let mut c = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    c
}

/// The pixels of `c`, upright and row by row, from a display turned by `o`.
fn upright(c: &Canvas, o: Orientation) -> Vec<u32> {
    (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (tx, ty) = match o {
                Orientation::Normal => (x, y),
                Orientation::UpsideDown => (W - 1 - x, H - 1 - y),
                Orientation::Right => (H - 1 - y, x),
                Orientation::Left => (y, W - 1 - x),
            };
            c.pixel(tx, ty).unwrap()
        })
        .collect()
}

/// What the screen of underlined letters should draw, upright, from the
/// letters alone and the lines alone: the letter's pixel within one pixel
/// of its ink across its cell's row, and the line's pixel elsewhere. Also
/// gives, for each row, how many pixels of its line are skipped and how
/// many are drawn, so a test can tell that both happen.
fn expected(letters: &[u32], lines: &[u32]) -> (Vec<u32>, Vec<(usize, usize)>) {
    let bg = pixel(BG);
    let at = |x: u32, y: u32| (y * W + x) as usize;
    let mut want = Vec::with_capacity(letters.len());
    let mut counts = vec![(0, 0); ROWS];
    for y in 0..H {
        for x in 0..W {
            let cell = x / 8;
            let near = (x.saturating_sub(1)..=x + 1)
                .filter(|&n| n / 8 == cell && n < W)
                .any(|n| letters[at(n, y)] != bg);
            let line = lines[at(x, y)] != bg;
            let row = &mut counts[(y / 16) as usize];
            match (near, line) {
                (true, true) => row.0 += 1,
                (false, true) => row.1 += 1,
                _ => {}
            }
            want.push(if near {
                letters[at(x, y)]
            } else {
                lines[at(x, y)]
            });
        }
    }
    (want, counts)
}

fn pixel(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

/// The first pixel where `got` differs from `want`, and how many do.
fn differ(got: &[u32], want: &[u32]) -> Option<(usize, (u32, u32), u32, u32)> {
    let n = got.iter().zip(want).filter(|(a, b)| a != b).count();
    let i = got.iter().zip(want).position(|(a, b)| a != b)?;
    Some((n, (i as u32 % W, i as u32 / W), got[i], want[i]))
}

/// The upright pixels and the expectation for the CPU path at turn `o`.
fn cpu(o: Orientation) -> (Vec<u32>, Vec<u32>, Vec<(usize, usize)>) {
    let got = upright(&draw(&screen(true, true), o), o);
    let letters = upright(&draw(&screen(true, false), o), o);
    let lines = upright(&draw(&screen(false, true), o), o);
    let (want, counts) = expected(&letters, &lines);
    (got, want, counts)
}

// covers: sgr.underline-style
#[test]
fn every_underline_style_skips_the_ink_of_descenders() {
    let (got, want, counts) = cpu(Orientation::Normal);
    // each style's line meets a descender and is drawn beside it, so the
    // comparison below tests both the gap and the line
    for (u, (skipped, drawn)) in STYLES.iter().zip(&counts) {
        assert!(
            *skipped > 0 && *drawn > 0,
            "{u:?}: {skipped} skipped, {drawn} drawn"
        );
    }
    let d = differ(&got, &want);
    assert!(
        d.is_none(),
        "(pixels that differ, the first (x, y), drawn, expected): {d:x?}"
    );
}

// A display turned a quarter runs the cell's row down a display column, so
// the gap lies along that column.
// covers: sgr.underline-style
#[test]
fn an_underline_skips_the_ink_at_every_turn_of_the_display() {
    for o in [
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ] {
        let (got, want, _) = cpu(o);
        let d = differ(&got, &want);
        assert!(d.is_none(), "{o:?}: {d:x?}");
    }
}

/// One row of [`TEXT`] in the plain style with the cursor at `col`, as an
/// underline when `shape` is one.
fn cursor_screen(shape: Option<CursorShape>, col: usize) -> Screen {
    let mut s = Screen::new(COLS, 1, rgb(FG), rgb(BG));
    s.text(0, 0, TEXT, plain());
    s.cursor.visible = shape.is_some();
    s.cursor.col = col;
    s.cursor.row = 0;
    if let Some(shape) = shape {
        s.cursor.shape = shape;
    }
    s
}

// The underline cursor shows where typing goes, so it keeps its whole line
// and the glyph draws over it: under the g every pixel of the line's rows
// that the g leaves clear is the line's.
// covers: cursor.style
#[test]
fn the_underline_cursor_draws_its_whole_line_under_a_descender() {
    let render = |s: &Screen| {
        let mut r = Renderer::new(unifont(), W, 16, Config::default());
        let mut c = Canvas::new(W, 16, 0);
        r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
        c
    };
    let letters = render(&cursor_screen(None, 0));
    let with = render(&cursor_screen(Some(CursorShape::Underline), 0));
    let (bg, fg) = (pixel(BG), pixel(FG));
    // the rows the cursor's line takes: where a blank cell would draw it
    let mut blank = Screen::new(COLS, 1, rgb(FG), rgb(BG));
    blank.text(0, 0, &" ".repeat(COLS), plain());
    blank.cursor.visible = true;
    blank.cursor.shape = CursorShape::Underline;
    let blank = render(&blank);
    let mut beside = 0;
    for y in 0..16 {
        if blank.pixel(0, y) == Some(bg) {
            continue;
        }
        for x in 0..8 {
            let ink = letters.pixel(x, y) != Some(bg);
            let near = (x.saturating_sub(1)..=x + 1)
                .filter(|&n| n < 8)
                .any(|n| letters.pixel(n, y) != Some(bg));
            let want = if ink { letters.pixel(x, y) } else { Some(fg) };
            assert_eq!(with.pixel(x, y), want, "{x},{y}");
            beside += usize::from(near && !ink);
        }
    }
    assert!(beside > 0, "the g's ink lies beside the cursor's line");
}

/// The GPU path's drawing of `s`, upright, on the stand-in GL, whose model
/// of the cell shader draws the cells.
#[cfg(feature = "gpu")]
fn gpu(s: &Screen) -> Vec<u32> {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    let size = Size::new(W, H);
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
    let buffer = gl.import_buffer(&size).unwrap();
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    r.draw_gpu(
        &mut gl,
        &mut pipeline,
        &s.frame(Age::NEVER),
        &buffer,
        0,
        &state(s),
        &[],
    )
    .unwrap();
    gl.frame_names()
        .iter()
        .rev()
        .find_map(|&n| gl.frame_pixels(n).filter(|(f, _)| *f == size))
        .unwrap()
        .1
        .to_vec()
}

// The cell shader reads the glyph's ink beside each pixel of the underline,
// within the cell, and leaves the line off where the CPU path does.
// covers: sgr.underline-style
#[cfg(feature = "gpu")]
#[test]
fn every_underline_style_skips_the_ink_of_descenders_on_the_gpu_path() {
    let got = gpu(&screen(true, true));
    let (cpu, want, _) = cpu(Orientation::Normal);
    let d = differ(&got, &want);
    assert!(d.is_none(), "against the expectation: {d:x?}");
    let d = differ(&got, &cpu);
    assert!(d.is_none(), "against the CPU path: {d:x?}");
}

// covers: cursor.style
#[cfg(feature = "gpu")]
#[test]
fn the_underline_cursor_draws_on_the_gpu_path_as_on_the_cpu_path() {
    let s = {
        let mut s = screen(true, true);
        s.cursor.visible = true;
        s.cursor.shape = CursorShape::Underline;
        s.cursor.col = 0;
        s.cursor.row = 0;
        // the cursor's own line on a cell with no underline of its own
        s.text(0, 0, "g", plain());
        s
    };
    let got = gpu(&s);
    let want = upright(&draw(&s, Orientation::Normal), Orientation::Normal);
    let d = differ(&got, &want);
    assert!(d.is_none(), "against the CPU path: {d:x?}");
}
