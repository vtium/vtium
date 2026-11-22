//! The dotted underline (SGR 4:4) keeps one dot period along the whole
//! line. Each dot is as wide as the underline is thick, and so is each gap,
//! in every cell. A cell whose width is not a whole number of periods does
//! not restart the pattern, so the last dot of one cell never touches the
//! first dot of the next. The cells here are blank, so the underline skips
//! no ink, and every lit pixel of the line is one of its dots.
//!
//! Each test draws a line of dotted cells at cell widths that hold no whole
//! number of periods, on the CPU path at every turn of the display and on
//! the GPU path, and reads the underline's rows upright.

mod common;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Decorations, Face, Font, Format, Glyph};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, LineSize, UnderlineStyle};

const FG: &str = "c0c0c0";
const BG: &str = "101010";
const COLS: usize = 11;

/// A face of blank cells `w` by `h`, with the lines `for_height` gives.
#[derive(Clone, Copy)]
struct Blank {
    w: u32,
    h: u32,
}

impl Face for Blank {
    fn cell(&self) -> Cell {
        Cell {
            width: self.w,
            height: self.h,
        }
    }
    fn size_step(&self) -> u32 {
        self.h
    }
    fn has_glyph(&self, _: char) -> bool {
        true
    }
    fn draw(&self, _: &[char], _: vtium_font::Style) -> Option<Glyph> {
        let n = (self.w * self.h) as usize;
        Glyph::from_pixels(self.w, self.h, Format::Mask, vec![0; n])
    }
}

/// A line of dotted blanks, at `size`.
fn screen(size: LineSize) -> Screen {
    let mut s = Screen::new(COLS, 1, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    let mut st = style(rgb(FG), rgb(BG));
    st.attrs.set_underline(UnderlineStyle::Dotted);
    s.text(0, 0, &" ".repeat(COLS), st);
    s.lines[0].flags_mut().set_size(size);
    s.damage.mark_cells(0, 0..COLS);
    s
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn on the CPU path in `face` on a display turned by `o`, upright
/// and row by row.
fn cpu(s: &Screen, face: Blank, o: Orientation) -> Vec<u32> {
    let (w, h) = (face.w * COLS as u32, face.h);
    let (dw, dh) = if o == Orientation::Right || o == Orientation::Left {
        (h, w)
    } else {
        (w, h)
    };
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(face), dw, dh, config);
    assert_eq!(r.grid_size(), (COLS, 1));
    let mut c = Canvas::new(dw, dh, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (tx, ty) = match o {
                Orientation::Normal => (x, y),
                Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                Orientation::Right => (h - 1 - y, x),
                Orientation::Left => (y, w - 1 - x),
            };
            c.pixel(tx, ty).unwrap()
        })
        .collect()
}

/// `s` drawn on the GPU path in `face`, through the stand-in GL's model
/// of the cell shader, upright and row by row.
#[cfg(feature = "gpu")]
fn gpu(s: &Screen, face: Blank) -> Vec<u32> {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    let size = Size::new(face.w * COLS as u32, face.h);
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
    let buffer = gl.import_buffer(&size).unwrap();
    let mut r = Renderer::new(Font::new(face), size.width, size.height, Config::default());
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

/// Checks that the underline's rows in `px`, a line drawn in `face` with
/// each of its pixels `mag` display pixels wide, are dots and gaps each
/// `thickness × mag` pixels wide along the whole line. Only the first and
/// last run may be cut short by the line's ends.
fn assert_one_period(px: &[u32], face: Blank, mag: u32, what: &str) {
    let w = face.w * COLS as u32;
    let m = Decorations::for_height(face.h);
    let d = (m.underline_thickness * mag) as usize;
    let bg = px[0];
    let rows: Vec<u32> = (0..face.h)
        .filter(|&y| (0..w).any(|x| px[(y * w + x) as usize] != bg))
        .collect();
    assert!(!rows.is_empty(), "{what}: the underline draws");
    for y in rows {
        let lit: Vec<bool> = (0..w).map(|x| px[(y * w + x) as usize] != bg).collect();
        // the lengths of the runs of lit and unlit pixels along the row
        let mut runs: Vec<(bool, usize)> = Vec::new();
        for &l in &lit {
            match runs.last_mut() {
                Some((on, n)) if *on == l => *n += 1,
                _ => runs.push((l, 1)),
            }
        }
        let row: String = lit.iter().map(|&l| if l { '█' } else { '·' }).collect();
        assert!(runs.len() > 2, "{what}, row {y}: no dots: {row}");
        let inner = &runs[1..runs.len() - 1];
        assert!(
            inner.iter().all(|&(_, n)| n == d),
            "{what}, row {y}: a dot or gap is not {d} pixels wide: {row}"
        );
        assert!(
            runs[0].1 <= d && runs[runs.len() - 1].1 <= d,
            "{what}, row {y}: {row}"
        );
    }
}

/// Cell sizes whose widths hold no whole number of dot periods: odd
/// widths with a line 1 pixel thick, and widths that are no multiple of 4
/// with a line 2 pixels thick.
const FACES: [Blank; 4] = [
    Blank { w: 9, h: 18 },
    Blank { w: 7, h: 16 },
    Blank { w: 11, h: 24 },
    Blank { w: 13, h: 32 },
];

// covers: sgr.underline-style
#[test]
fn a_dotted_underline_keeps_one_period_across_cells_on_the_cpu_path() {
    for face in FACES {
        let s = screen(LineSize::Single);
        for o in [
            Orientation::Normal,
            Orientation::UpsideDown,
            Orientation::Right,
            Orientation::Left,
        ] {
            let what = format!("{}x{} cells, {o:?}", face.w, face.h);
            assert_one_period(&cpu(&s, face, o), face, 1, &what);
        }
    }
}

// On a line of double width each pixel of the line doubles, so the dots
// and gaps are twice as wide, and still one period along the whole line.
#[test]
fn a_dotted_underline_keeps_one_period_on_a_double_width_line() {
    for face in FACES {
        let s = screen(LineSize::DoubleWidth);
        let what = format!("{}x{} cells, double width", face.w, face.h);
        assert_one_period(&cpu(&s, face, Orientation::Normal), face, 2, &what);
    }
}

#[cfg(feature = "gpu")]
#[test]
fn a_dotted_underline_keeps_one_period_on_the_gpu_path_as_on_the_cpu_path() {
    for face in FACES {
        for size in [LineSize::Single, LineSize::DoubleWidth] {
            let s = screen(size);
            let what = format!("{}x{} cells, {size:?}, GPU path", face.w, face.h);
            let px = gpu(&s, face);
            let mag = if size == LineSize::Single { 1 } else { 2 };
            assert_one_period(&px, face, mag, &what);
            assert_eq!(px, cpu(&s, face, Orientation::Normal), "{what}");
        }
    }
}
