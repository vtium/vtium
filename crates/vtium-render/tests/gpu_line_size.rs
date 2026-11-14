//! Double-width and double-height lines (DECDWL, DECDHL) on the GPU path:
//! each frame the GPU path composes matches the CPU path's whole drawing
//! of the same screen, pixel for pixel, at every turn of the display.
//!
//! The stand-in GL draws the cells through its model of the cell shader.
//! A line of double size draws each piece of its doubled glyphs and
//! decorations as a one-cell job, so the cell shader draws it as it draws
//! any cell, and tier 2 judges the shader itself on a real GL.
#![cfg(feature = "gpu")]

mod common;

use common::{Screen, rgb, style};
use vtium_font::{
    Cell, Doubled, Face, Font, Format as FontFormat, Glyph, Script, Style, Variant,
    Width as FontWidth,
};
use vtium_render::gpu::{DrawTarget, Gl, Pipeline, Size, StandIn};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, Attrs, Color, CursorShape, LineSize, UnderlineStyle};

const CW: u32 = 8;
const CH: u32 = 16;
const FG: &str = "e0e0e0";
const BG: &str = "102030";

/// An outline face in 8x16 cells: each glyph a pattern of partly covered
/// pixels that differs at every pixel of its cell, `W` two cells wide,
/// and `A` with an accent 4 rows into the row above.
struct Shades;

impl Face for Shades {
    fn cell(&self) -> Cell {
        Cell {
            width: CW,
            height: CH,
        }
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, c: char) -> bool {
        c.is_ascii_graphic() || c == ' '
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<Glyph> {
        let c = *cluster.first()?;
        if c == ' ' {
            return Glyph::mask(CW, CH);
        }
        let (w, width) = if c == 'W' {
            (2 * CW, FontWidth::Wide)
        } else {
            (CW, FontWidth::Narrow)
        };
        let k = u32::from(c);
        let px = move |x: u32, y: u32| ((x * 37 + y * 11 + k * 5) % 256) as u8;
        let own = (0..CH)
            .flat_map(|y| (0..w).map(move |x| px(x, y)))
            .collect();
        let g = Glyph::from_pixels(w, CH, FontFormat::Mask, own)?.with_cells(width);
        if c != 'A' {
            return Some(g);
        }
        let boxed = (0..CH + 4)
            .flat_map(|y| (0..w).map(move |x| if y < 4 { (x * 30) as u8 } else { px(x, y - 4) }))
            .collect();
        g.with_ink((0, -4), (w, CH + 4), boxed)
    }
}

/// [`Shades`], whose glyphs the face also holds rendered at each double
/// size, as the font helper renders an outline font's: a pattern of its
/// own over the doubled cell, which no doubling of the normal glyph's
/// pixels gives. As the console's face does, it holds none for a cluster
/// or for a superscript or subscript.
struct Rendered;

impl Face for Rendered {
    fn cell(&self) -> Cell {
        Shades.cell()
    }
    fn size_step(&self) -> u32 {
        Shades.size_step()
    }
    fn has_glyph(&self, c: char) -> bool {
        Shades.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        Shades.draw(cluster, style)
    }
    fn doubled(&self, cluster: &[char], variant: Variant, doubled: Doubled) -> Option<Glyph> {
        let &[c] = cluster else {
            return None;
        };
        if variant.script != Script::Baseline || !self.has_glyph(c) {
            return None;
        }
        let width = Shades.draw(cluster, variant.style)?.cells();
        let (w, h) = (2 * CW * width.cells(), CH * doubled.rows());
        let k = u32::from(c);
        let px = move |x: u32, y: u32| ((x * 13 + y * 7 + k * 3) % 251) as u8;
        let own = (0..h).flat_map(|y| (0..w).map(move |x| px(x, y))).collect();
        Some(Glyph::from_pixels(w, h, FontFormat::Mask, own)?.with_cells(width))
    }
}

fn plain() -> vtium_term::Style {
    style(rgb(FG), rgb(BG))
}

/// Sets the size of `row`, and marks its cells changed.
fn size(s: &mut Screen, row: usize, size: LineSize) {
    s.lines[row].flags_mut().set_size(size);
    s.damage.mark_cells(row, 0..s.cols);
}

/// A row of every decoration, a wide character, colors and reverse video.
fn decorated(s: &mut Screen, row: usize) {
    let mut st = plain();
    s.text(0, row, "ab", st);
    st.attrs.set_underline(UnderlineStyle::Single);
    s.put(2, row, 'c', false, st);
    st.attrs.set_underline(UnderlineStyle::Curly);
    s.put(3, row, 'd', false, st);
    st.attrs.set_underline(UnderlineStyle::Dashed);
    st.attrs.insert(Attrs::STRIKE);
    s.put(4, row, 'e', false, st);
    st.attrs.insert(Attrs::OVERLINE);
    s.put(5, row, 'W', true, st);
    let mut colored = plain();
    colored.fg = Color::Rgb(rgb("ff4000"));
    colored.bg = Color::Rgb(rgb("004080"));
    s.put(7, row, 'g', false, colored);
    colored.attrs.insert(Attrs::INVERSE);
    s.put(8, row, 'h', false, colored);
}

/// The display for `s` turned as `o` says: its grid inside a border of 2
/// pixels across and 1 down, upright.
fn display(s: &Screen, o: Orientation) -> Size {
    let (w, h) = (s.cols as u32 * CW + 4, s.rows as u32 * CH + 2);
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

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn whole on the CPU path in `face`, turned as `o` says, with the
/// columns `sel` of each row selected.
fn whole<F: Face>(face: F, s: &Screen, o: Orientation, sel: &[std::ops::Range<usize>]) -> Canvas {
    let size = display(s, o);
    let mut r = Renderer::new(Font::new(face), size.width, size.height, config(o));
    let mut c = Canvas::new(size.width, size.height, 0);
    let frame = s.frame(Age::NEVER).with_selection(sel.to_vec());
    r.draw(&frame, &mut c, 0, &state(s));
    c
}

/// A display on the stand-in GL, whose upright frame keeps every frame
/// the GPU path composed.
struct Gpu<F: Face> {
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    r: Renderer<F>,
    since: Age,
    orientation: Orientation,
}

impl<F: Face> Gpu<F> {
    fn new(face: F, s: &Screen, o: Orientation) -> Gpu<F> {
        let size = display(s, o);
        let mut gl = StandIn::new();
        let pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        Gpu {
            gl,
            pipeline,
            buffer,
            r: Renderer::new(Font::new(face), size.width, size.height, config(o)),
            since: Age::NEVER,
            orientation: o,
        }
    }

    /// Draws `s` into the buffer one frame old.
    fn frame(&mut self, s: &mut Screen, sel: &[std::ops::Range<usize>]) {
        let frame = s.frame(self.since).with_selection(sel.to_vec());
        self.r
            .draw_gpu(
                &mut self.gl,
                &mut self.pipeline,
                &frame,
                &self.buffer,
                1,
                &state(s),
                &[],
            )
            .unwrap();
        self.since = s.damage.checkpoint();
    }

    /// The upright frame's pixels.
    fn upright(&self) -> Vec<u32> {
        let size = self.pipeline.size();
        let up = match self.orientation {
            Orientation::Right | Orientation::Left => Size::new(size.height, size.width),
            Orientation::Normal | Orientation::UpsideDown => size,
        };
        let pixels = self
            .gl
            .frame_names()
            .iter()
            .rev()
            .find_map(|&n| self.gl.frame_pixels(n).filter(|(s, _)| *s == up));
        pixels.unwrap().1.to_vec()
    }
}

/// Fails, saying where, unless the GPU path's upright frame is `cpu`'s
/// pixels.
fn assert_same<F: Face>(g: &Gpu<F>, cpu: &Canvas, what: &str) {
    let gpu = g.upright();
    let (w, h) = (cpu.width(), cpu.height());
    let up = |x: u32, y: u32| match g.orientation {
        Orientation::Normal => (x, y, w),
        Orientation::UpsideDown => (w - 1 - x, h - 1 - y, w),
        Orientation::Right => (y, w - 1 - x, h),
        Orientation::Left => (h - 1 - y, x, h),
    };
    let differ: Vec<(u32, u32, u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let c = cpu.pixel(x, y)?;
            let (ux, uy, stride) = up(x, y);
            let p = gpu[(uy * stride + ux) as usize];
            (c != p).then_some((x, y, c, p))
        })
        .collect();
    assert!(
        differ.is_empty(),
        "{what}: {} pixels differ from the CPU path's, the first (x, y, cpu, gpu) {:x?}",
        differ.len(),
        differ.first()
    );
}

const TURNS: [Orientation; 4] = [
    Orientation::Normal,
    Orientation::UpsideDown,
    Orientation::Right,
    Orientation::Left,
];

// Each frame of a screen whose lines change size draws on the GPU path as
// the CPU path draws it whole: a double-width line, a double-height pair,
// every decoration, a wide character, colors, reverse video, the cursor
// in each shape and the selection, with an accent below a line of double
// size, at every turn of the display.
// covers: text.line-size
#[test]
fn every_gpu_frame_of_double_size_lines_matches_a_whole_draw() {
    frames_match_whole_draws(|| Shades);
}

// The same frames match where the face holds its glyphs rendered at each
// double size, as the font helper renders an outline font's: the GPU path
// draws the pieces of those glyphs as the CPU path does, and the glyph
// cache keeps them under their piece keys apart from the doubled pixels
#[test]
fn every_gpu_frame_of_rendered_double_size_glyphs_matches_a_whole_draw() {
    frames_match_whole_draws(|| Rendered);
}

/// Draws each step of a screen whose lines change size on the GPU path
/// in the face `face` makes, and fails unless each frame matches the CPU
/// path's whole drawing of the same screen, at every turn of the display.
fn frames_match_whole_draws<F: Face>(face: impl Fn() -> F) {
    for o in TURNS {
        let mut s = Screen::new(10, 4, rgb(FG), rgb(BG));
        s.cursor.visible = false;
        for row in 0..3 {
            decorated(&mut s, row);
        }
        s.text(0, 3, "A A A", plain());
        let mut g = Gpu::new(face(), &s, o);
        type Step = fn(&mut Screen) -> Vec<std::ops::Range<usize>>;
        let steps: [(&str, Step); 10] = [
            ("the screen", |_| Vec::new()),
            ("a double-width line", |s| {
                size(s, 0, LineSize::DoubleWidth);
                Vec::new()
            }),
            ("a double-height pair above the accents", |s| {
                size(s, 1, LineSize::DoubleHeightTop);
                size(s, 2, LineSize::DoubleHeightBottom);
                Vec::new()
            }),
            ("a block cursor on the double-width line", |s| {
                s.cursor.visible = true;
                (s.cursor.col, s.cursor.row) = (1, 0);
                s.damage.mark_cell(0, 1);
                Vec::new()
            }),
            ("an underline cursor on the top line", |s| {
                s.cursor.shape = CursorShape::Underline;
                (s.cursor.col, s.cursor.row) = (2, 1);
                s.damage.mark_cell(0, 1);
                s.damage.mark_cell(1, 2);
                Vec::new()
            }),
            ("a bar cursor on the wide character", |s| {
                s.cursor.shape = CursorShape::Bar;
                (s.cursor.col, s.cursor.row) = (5, 2);
                s.damage.mark_cell(1, 2);
                s.damage.mark_cell(2, 5);
                Vec::new()
            }),
            ("a selection across the lines", |s| {
                s.damage.mark_rows(0..s.rows);
                vec![1..4, 0..2, 3..6, 0..3]
            }),
            ("a letter typed on the bottom line", |s| {
                // the selection ends, which changes the cells it held
                s.damage.mark_rows(0..s.rows);
                s.put(1, 2, 'z', false, plain());
                Vec::new()
            }),
            ("a letter typed on the double-width line", |s| {
                s.put(3, 0, 'y', false, plain());
                Vec::new()
            }),
            ("every line normal again", |s| {
                for row in 0..3 {
                    size(s, row, LineSize::Single);
                }
                Vec::new()
            }),
        ];
        for (what, step) in steps {
            let sel = step(&mut s);
            g.frame(&mut s, &sel);
            assert_same(&g, &whole(face(), &s, o, &sel), &format!("{what}, {o:?}"));
        }
    }
}
