//! A 1-bit glyph from a later bitmap source, raised onto the first
//! source's baseline, draws the ink it then has above its cell into the
//! row above (ADR-1200 D17, ADR-1306 D1, the maintainer's notes of
//! 2026-11-06). The glyph comes from a set as the glyph helper writes it:
//! its 1-bit pixels cut to its cell, and an ink box of 1-bit rows that
//! reaches 2 rows above the cell. The set reader hands the renderer both
//! as coverage, so the CPU path draws the box's rows above the cell over
//! the row above, and the GPU path draws the same pixels.

mod common;

use std::sync::Arc;

use common::{Screen, rgb, style};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::{Cell, Font, Style, Width};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, LineSize};

const CW: u32 = 8;
const CH: u32 = 16;
const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// unifont's ẳ (U+1EB3), its 16 rows of 8 pixels: the hook on rows 0 to 2,
/// the breve on rows 3 and 4, the letter on rows 6 to 13, which stand on
/// unifont's baseline, row 14.
const UNIFONT_A_HOOK_BREVE: [u8; 16] = [
    0x08, 0x08, 0x10, 0x44, 0x38, 0x00, 0x3c, 0x42, 0x02, 0x3e, 0x42, 0x42, 0x46, 0x3a, 0x00, 0x00,
];

/// A Terminus-like a on the first source's baseline, row 12.
const TERMINUS_A: [u8; 16] = [
    0, 0, 0, 0, 0x3c, 0x42, 0x02, 0x3e, 0x42, 0x42, 0x46, 0x3a, 0, 0, 0, 0,
];

/// The set the helper writes for `a` and ẳ: ẳ raised 2 rows, its pixels
/// cut to its cell and, when `boxed`, the 18 rows from 2 above its cell in
/// its ink box.
fn set(boxed: bool) -> SetFace {
    let cell = Cell {
        width: CW,
        height: CH,
    };
    let mut s = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default()).baseline(12);
    let a = s.glyph(Width::Narrow, TERMINUS_A.to_vec());
    s.map('a', a);
    let mut own: Vec<u8> = UNIFONT_A_HOOK_BREVE[2..].to_vec();
    own.extend([0, 0]);
    let g = s.glyph(Width::Narrow, own);
    s.map('ẳ', g);
    if boxed {
        let mut rows = UNIFONT_A_HOOK_BREVE.to_vec();
        rows.extend([0, 0]);
        s.ink(g, (0, -2), (CW, CH + 2), rows);
    }
    let bytes = SetBuilder::new("t").strike(s).write().expect("written");
    SetFace::new(Arc::new(Set::from_bytes(bytes).expect("read")), CH).expect("a face")
}

/// Three rows of three cells, `a` and ẳ on the middle row.
fn screen() -> Screen {
    let mut s = Screen::new(3, 3, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    s.text(0, 1, "aẳ", style(rgb(FG), rgb(BG)));
    s
}

fn size(s: &Screen) -> (u32, u32) {
    (s.cols as u32 * CW + 4, s.rows as u32 * CH + 2)
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: false,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn whole on the CPU path.
fn cpu(s: &Screen, face: SetFace) -> Canvas {
    let (w, h) = size(s);
    let mut r = Renderer::new(Font::new(face), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    c
}

/// The rows of the cell `col`, `row` drawn in the foreground, from the
/// top of the grid: the display's border is 2 pixels across and 1 down.
fn inked_rows(c: &Canvas, col: u32, rows: std::ops::Range<u32>) -> Vec<u32> {
    let want = u32::from_str_radix(FG, 16).unwrap();
    rows.filter(|&y| {
        (0..CW).any(|x| {
            c.pixel(2 + col * CW + x, 1 + y)
                .is_some_and(|p| p & 0xff_ffff == want)
        })
    })
    .collect()
}

// ADR-1306 D1's note of 2026-11-06: ẳ, raised onto the first source's
// baseline, ends on a's row and draws the top of its hook, whole, in the
// 2 rows above its cell, over the row above; without its box the hook's
// top is cut at the cell's top
// covers: text.overhang
#[test]
fn a_raised_bitmap_glyph_draws_its_hook_in_the_row_above_on_the_cpu_path() {
    let s = screen();
    let c = cpu(&s, set(true));
    let (a, hook) = (inked_rows(&c, 0, 16..32), inked_rows(&c, 1, 0..32));
    assert_eq!(
        a.last(),
        Some(&27),
        "a ends on the row above the baseline, 12"
    );
    assert_eq!(hook.last(), a.last(), "ẳ ends on a's row");
    assert_eq!(
        hook.first(),
        Some(&14),
        "ẳ's hook starts 2 rows above its cell"
    );
    // the hook's top: column 4 of the two rows above the cell
    let want = u32::from_str_radix(FG, 16).unwrap();
    for y in [14, 15] {
        assert_eq!(
            c.pixel(2 + CW + 4, 1 + y).map(|p| p & 0xff_ffff),
            Some(want),
            "row {y}"
        );
    }
    let cut = cpu(&s, set(false));
    assert_eq!(
        inked_rows(&cut, 1, 0..32).first(),
        Some(&16),
        "no box: cut at the cell"
    );
}

// The GPU path draws the box of a 1-bit glyph from a set as the CPU path
// does: the stand-in GL draws the cells through its model of the cell
// shader, and its frame is the CPU path's pixels
// covers: text.overhang
#[cfg(feature = "gpu")]
#[test]
fn a_raised_bitmap_glyph_draws_its_hook_in_the_row_above_on_the_gpu_path() {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    let s = screen();
    let want = cpu(&s, set(true));
    let (w, h) = size(&s);
    let size = Size::new(w, h);
    let mut gl = StandIn::new();
    let mut pipeline =
        Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).expect("a pipeline");
    let buffer = gl.import_buffer(&size).expect("a buffer");
    let mut r = Renderer::new(Font::new(set(true)), w, h, Config::default());
    r.draw_gpu(
        &mut gl,
        &mut pipeline,
        &s.frame(Age::NEVER),
        &buffer,
        1,
        &state(&s),
        &[],
    )
    .expect("drawn");
    let name = *gl.frame_names().last().expect("a frame");
    let got = gl.frame_pixels(name).expect("its pixels").1.to_vec();
    let differ = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| want.pixel(x, y) != Some(got[(y * w + x) as usize]))
        .count();
    assert_eq!(differ, 0, "pixels that differ from the CPU path's");
    assert!(
        inked_rows(&want, 1, 0..16).contains(&14),
        "the hook drawn in the row above"
    );
}

/// unifont's 😀 (U+1F600), its 16 rows of 16 pixels: its ink on rows 1 to
/// 14, which stand on unifont's baseline, row 14.
const UNIFONT_GRIN: [u16; 16] = [
    0x0000, 0x03e0, 0x0c18, 0x1004, 0x2002, 0x2632, 0x4631, 0x4001, 0x4001, 0x4ff9, 0x2aaa, 0x26b2,
    0x13e4, 0x0c18, 0x03e0, 0x0000,
];

/// `rows` of a 16-pixel glyph as the set's 1-bit bytes, two a row.
fn wide_bits(rows: &[u16]) -> Vec<u8> {
    rows.iter().flat_map(|r| r.to_be_bytes()).collect()
}

/// The set the helper writes for 😀 behind a first source whose baseline
/// is row 12, as Terminus's 8x16 has it: 😀 raised 2 rows onto that
/// baseline, so its top row lies 1 row above its cell. Its pixels are cut
/// to its two cells, and its ink box holds the 17 rows from that top row.
fn grin_set() -> SetFace {
    let cell = Cell {
        width: CW,
        height: CH,
    };
    let mut s = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default()).baseline(12);
    let mut own = UNIFONT_GRIN[2..].to_vec();
    own.extend([0, 0]);
    let g = s.glyph(Width::Wide, wide_bits(&own));
    s.map('😀', g);
    let mut boxed = UNIFONT_GRIN[1..].to_vec();
    boxed.extend([0, 0]);
    s.ink(g, (0, -1), (2 * CW, CH + 1), wide_bits(&boxed));
    let bytes = SetBuilder::new("t").strike(s).write().expect("written");
    SetFace::new(Arc::new(Set::from_bytes(bytes).expect("read")), CH).expect("a face")
}

/// Four rows of four cells, 😀 at the left of row 1 at `top`'s size, and
/// when `bottom` is given, of row 2 at its size.
fn grin_screen(top: LineSize, bottom: Option<LineSize>) -> Screen {
    let mut s = Screen::new(4, 4, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    for (row, size) in [(1, Some(top)), (2, bottom)] {
        let Some(size) = size else { continue };
        s.put(0, row, '😀', true, style(rgb(FG), rgb(BG)));
        s.lines[row].flags_mut().set_size(size);
        s.damage.mark_cells(row, 0..s.cols);
    }
    s
}

/// The foreground's pixels in the grid's first `cols` columns of pixels,
/// one row of them for each display row from the top of the grid to
/// `rows`, with the rows that hold none left out.
fn inked_lines(c: &Canvas, cols: u32, rows: std::ops::Range<u32>) -> Vec<Vec<bool>> {
    let want = u32::from_str_radix(FG, 16).unwrap();
    rows.map(|y| {
        (0..cols)
            .map(|x| c.pixel(2 + x, 1 + y).is_some_and(|p| p & 0xff_ffff == want))
            .collect::<Vec<bool>>()
    })
    .filter(|line| line.contains(&true))
    .collect()
}

/// 😀's inked rows, each pixel `across` pixels wide and `down` rows tall.
fn grin_lines(across: usize, down: usize) -> Vec<Vec<bool>> {
    UNIFONT_GRIN
        .iter()
        .filter(|&&r| r != 0)
        .flat_map(|&r| {
            let line: Vec<bool> = (0..16)
                .flat_map(|x| std::iter::repeat_n(r & (0x8000 >> x) != 0, across))
                .collect();
            std::iter::repeat_n(line, down)
        })
        .collect()
}

// The maintainer's note of 2026-11-15, from the acceptance page of
// double-size scripts in the Terminus-then-unifont font set: unifont's
// 😀, raised onto Terminus's baseline, lost its top row on a double-width
// line and on a double-height pair. Every one of its 14 inked rows
// reaches the screen at the normal size, on a double-width line and on a
// double-height pair, whole and in order. A line of double size keeps its
// glyphs' ink in their cells, and each piece of a doubled glyph is cut from
// the glyph's own pixels, which stop at its cell, so the row above the cell
// is lost on both. The test waits for the fix, which draws that ink past
// the line as the normal size draws it.
#[test]
fn a_raised_bitmap_glyph_keeps_every_row_on_lines_of_every_size() {
    use LineSize::*;
    let cases = [
        ("normal", (Single, None), 1, 1),
        ("double width", (DoubleWidth, None), 2, 1),
        (
            "double height",
            (DoubleHeightTop, Some(DoubleHeightBottom)),
            2,
            2,
        ),
    ];
    let cut: Vec<&str> = cases
        .into_iter()
        .filter(|&(_, (top, bottom), across, down)| {
            let c = cpu(&grin_screen(top, bottom), grin_set());
            // rows 0 to 2 of the grid, in the columns of 😀's cells
            let wide = 2 * CW * across as u32;
            inked_lines(&c, wide, 0..3 * CH) != grin_lines(across, down)
        })
        .map(|(what, ..)| what)
        .collect();
    assert_eq!(cut, [] as [&str; 0], "the sizes that cut 😀's ink");
}

/// Three rows of three cells, `a` and ẳ on the middle row, the top row at
/// `top`'s size with `aa` on it.
fn hook_under(top: LineSize, hook: bool) -> Screen {
    let mut s = Screen::new(3, 3, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    s.text(0, 0, "aa", style(rgb(FG), rgb(BG)));
    s.text(
        0,
        1,
        if hook { "aẳ" } else { "aa" },
        style(rgb(FG), rgb(BG)),
    );
    s.lines[0].flags_mut().set_size(top);
    s
}

/// The pixels of grid row `row` of `c`, `cols` cells across.
fn row_pixels(c: &Canvas, cols: u32, row: u32) -> Vec<u32> {
    (row * CH..(row + 1) * CH)
        .flat_map(|y| (0..cols * CW).map(move |x| c.pixel(2 + x, 1 + y).unwrap_or(0)))
        .collect()
}

// A line of double size receives no ink: the hook of ẳ, raised into the
// row above, draws into that row at the normal size and into none of the
// double sizes, whichever line of a pair the row is. The row above is
// drawn again on a second frame, once the hook's reach is known, so a
// line of double size that read its neighbours' reach would draw it.
// covers: text.line-size
#[test]
fn ink_from_a_normal_row_never_reaches_into_a_line_of_double_size() {
    use LineSize::*;
    let row0 = |top: LineSize, hook: bool| {
        let mut s = hook_under(top, hook);
        let (w, h) = size(&s);
        let mut r = Renderer::new(Font::new(set(true)), w, h, Config::default());
        let mut c = Canvas::new(w, h, 0);
        r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(&s));
        let since = s.damage.checkpoint();
        s.damage.mark_cells(0, 0..s.cols);
        r.draw(&s.frame(since), &mut c, 1, &state(&s));
        row_pixels(&c, 3, 0)
    };
    assert_ne!(
        row0(Single, true),
        row0(Single, false),
        "the hook draws into a row of the normal size"
    );
    for top in [DoubleWidth, DoubleHeightTop, DoubleHeightBottom] {
        assert_eq!(row0(top, true), row0(top, false), "{top:?}");
    }
}

/// The steps of a screen whose 😀 on row 1 sends ink past a line of double
/// size: each changes what the rows around the line receive.
type Step = (&'static str, fn(&mut Screen));

fn grin_steps() -> [Step; 9] {
    fn sized(s: &mut Screen, row: usize, size: LineSize) {
        s.lines[row].flags_mut().set_size(size);
        s.damage.mark_cells(row, 0..s.cols);
    }
    fn grin(s: &mut Screen, c: char) {
        s.put(0, 1, c, c != ' ', style(rgb(FG), rgb(BG)));
        if c == ' ' {
            s.put(1, 1, ' ', false, style(rgb(FG), rgb(BG)));
        }
    }
    [
        ("the screen", |_| {}),
        ("the screen again", |_| {}),
        ("its line doubles in width", |s| {
            sized(s, 1, LineSize::DoubleWidth)
        }),
        ("the grin goes", |s| grin(s, ' ')),
        ("the grin comes back", |s| grin(s, '😀')),
        ("the cursor stands on it", |s| {
            s.cursor.visible = true;
            (s.cursor.col, s.cursor.row) = (0, 1);
            s.damage.mark_cell(1, 0);
        }),
        ("a double-height pair", |s| {
            s.cursor.visible = false;
            s.damage.mark_cell(1, 0);
            s.put(0, 2, '😀', true, style(rgb(FG), rgb(BG)));
            sized(s, 1, LineSize::DoubleHeightTop);
            sized(s, 2, LineSize::DoubleHeightBottom);
        }),
        ("the pair's top half goes", |s| grin(s, ' ')),
        ("every line normal", |s| {
            sized(s, 1, LineSize::Single);
            sized(s, 2, LineSize::Single);
        }),
    ]
}

/// A screen of four rows of four cells with 😀 at the left of row 1.
fn grin_start() -> Screen {
    grin_screen(LineSize::Single, None)
}

// Each frame of a doubled glyph whose ink passes the line, drawn into one
// of `n` buffers in turn, matches a whole draw: a change to the glyph, its
// line's size or the cursor on it draws again the cells its old and new
// ink reach, in the rows above and below
// covers: text.line-size
#[test]
fn every_buffer_gets_a_doubled_glyphs_ink_as_it_changes() {
    for n in [2usize, 3] {
        let mut s = grin_start();
        let (w, h) = size(&s);
        let mut r = Renderer::new(Font::new(grin_set()), w, h, Config::default());
        let mut buffers: Vec<Canvas> = (0..n).map(|_| Canvas::new(w, h, 0)).collect();
        let mut since = Age::NEVER;
        for (i, (what, step)) in grin_steps().into_iter().enumerate() {
            step(&mut s);
            let age = if i < n { 0 } else { n as u32 };
            let buffer = &mut buffers[i % n];
            r.draw(&s.frame(since), buffer, age, &state(&s));
            since = s.damage.checkpoint();
            let want = cpu(&s, grin_set());
            assert!(
                *buffer == want,
                "{what}, {n} buffers: differs from a whole draw"
            );
        }
    }
}

// The GPU path draws a doubled glyph's ink past the line as the CPU path
// does: the stand-in GL draws the cells through its model of the cell
// shader, whose ink grid names each piece of the doubled glyph by its
// display cell, and each frame's pixels are a whole CPU draw's
// covers: text.line-size
#[cfg(feature = "gpu")]
#[test]
fn every_gpu_frame_of_a_doubled_glyphs_ink_matches_a_whole_draw() {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    let mut s = grin_start();
    let (w, h) = size(&s);
    let size = Size::new(w, h);
    let mut gl = StandIn::new();
    let mut pipeline =
        Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).expect("a pipeline");
    let buffer = gl.import_buffer(&size).expect("a buffer");
    let mut r = Renderer::new(Font::new(grin_set()), w, h, Config::default());
    let mut since = Age::NEVER;
    for (what, step) in grin_steps() {
        step(&mut s);
        let frame = s.frame(since);
        r.draw_gpu(&mut gl, &mut pipeline, &frame, &buffer, 1, &state(&s), &[])
            .expect("drawn");
        since = s.damage.checkpoint();
        let want = cpu(&s, grin_set());
        let name = *gl.frame_names().last().expect("a frame");
        let got = gl.frame_pixels(name).expect("its pixels").1.to_vec();
        let differ = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| want.pixel(x, y) != Some(got[(y * w + x) as usize]))
            .count();
        assert_eq!(differ, 0, "{what}: pixels that differ from the CPU path's");
        if what == "its line doubles in width" {
            // the grin's top row, in the row above its line
            let fg = u32::from_str_radix(FG, 16).unwrap();
            let above = (1 + CH - 1) * w;
            let lit = (0..w).any(|x| got[(above + x) as usize] & 0xff_ffff == fg);
            assert!(lit, "{what}: the row above shows the grin's top row");
        }
    }
}

/// A face in 8x16 cells whose `A` carries an accent in its ink box, 4
/// rows above its cell, and which holds `A` drawn at each double size with
/// the accent 4 rows above the doubled cell, as the font helper renders an
/// outline font's glyph at the line's size.
struct Accented;

impl Accented {
    /// `A` in a box `w` by `h` with the accent's 4 rows above it: a frame
    /// of ink in the cells, and a bar of accent above.
    fn glyph(w: u32, h: u32) -> Option<vtium_font::Glyph> {
        use vtium_font::{Format, Glyph};
        let own = (0..h)
            .flat_map(|y| (0..w).map(move |x| if x == 1 || y == h - 1 { 255 } else { 0 }))
            .collect();
        let boxed = (0..h + 4)
            .flat_map(|y| {
                (0..w).map(move |x| match y {
                    0..4 => 255 * u8::from((2..w - 2).contains(&x)),
                    _ if x == 1 || y == h + 3 => 255,
                    _ => 0,
                })
            })
            .collect();
        Glyph::from_pixels(w, h, Format::Mask, own)?.with_ink((0, -4), (w, h + 4), boxed)
    }
}

impl vtium_font::Face for Accented {
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
        c == 'A'
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        (cluster == ['A'])
            .then(|| Accented::glyph(CW, CH))
            .flatten()
    }
    fn doubled(
        &self,
        cluster: &[char],
        _: vtium_font::Variant,
        doubled: vtium_font::Doubled,
    ) -> Option<vtium_font::Glyph> {
        (cluster == ['A'])
            .then(|| Accented::glyph(2 * CW, CH * doubled.rows()))
            .flatten()
    }
}

// A glyph that the font helper renders at the line's size keeps its ink
// past the line as a doubled bitmap glyph does: the accent of `A`, drawn
// above the doubled cell, lies in the row above a double-width line and
// above a pair's top line, on the CPU path and the GPU path alike
// covers: text.line-size
#[test]
fn a_glyph_rendered_at_double_size_keeps_its_accent_above_the_line() {
    use LineSize::*;
    for (top, bottom) in [
        (DoubleWidth, None),
        (DoubleHeightTop, Some(DoubleHeightBottom)),
    ] {
        let mut s = Screen::new(4, 3, rgb(FG), rgb(BG));
        s.cursor.visible = false;
        for (row, size) in [(1, Some(top)), (2, bottom)] {
            let Some(size) = size else { continue };
            s.put(0, row, 'A', false, style(rgb(FG), rgb(BG)));
            s.lines[row].flags_mut().set_size(size);
        }
        let (w, h) = size(&s);
        let mut r = Renderer::new(Font::new(Accented), w, h, Config::default());
        let mut c = Canvas::new(w, h, 0);
        r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(&s));
        // the accent's 4 rows, at the bottom of the row above
        let accent: Vec<u32> = (12..16).collect();
        assert_eq!(inked_rows(&c, 1, 0..16), accent, "{top:?}");
        #[cfg(feature = "gpu")]
        {
            use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
            let size = Size::new(w, h);
            let mut gl = StandIn::new();
            let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default())
                .expect("a pipeline");
            let buffer = gl.import_buffer(&size).expect("a buffer");
            let mut r = Renderer::new(Font::new(Accented), w, h, Config::default());
            r.draw_gpu(
                &mut gl,
                &mut pipeline,
                &s.frame(Age::NEVER),
                &buffer,
                1,
                &state(&s),
                &[],
            )
            .expect("drawn");
            let name = *gl.frame_names().last().expect("a frame");
            let got = gl.frame_pixels(name).expect("its pixels").1.to_vec();
            let differ = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| c.pixel(x, y) != Some(got[(y * w + x) as usize]))
                .count();
            assert_eq!(differ, 0, "{top:?}: pixels that differ from the CPU path's");
        }
    }
}
