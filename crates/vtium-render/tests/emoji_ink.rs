//! An underline under a color glyph, such as an emoji, lies behind the
//! glyph's picture: the picture's drawn pixels cover the line. A color
//! glyph's ink is the pixels whose alpha is at least 16 of 255, not its
//! whole square, so a transparent margin and the faint ringing that
//! scaling leaves in it do not break the line.
//!
//! In a cell at least 32 pixels tall, the line also keeps a gap of one
//! pixel on each side of the picture's ink, as it does around a letter's
//! descender. In a shorter cell a gap of one pixel would cut too much of
//! a short line, so the line runs whole behind the picture, right up to its
//! ink. The strike-through draws over the picture, and a letter beside it
//! still breaks the line around its descender. The GPU path draws the same
//! pixels as the CPU path.
//!
//! Each test computes the pixels it expects from two other drawings of the
//! same screen: the glyphs with no lines, and the lines under blank cells.

mod common;

use std::sync::Arc;

use common::{Screen, rgb, style};
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::{Cell, Decorations, Font, Width};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs, Color, Style, UnderlineStyle};

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};
const SMILE: char = '\u{1f600}';
const UNDERLINE_ROW: u32 = 14;
const STRIKE_ROW: u32 = 8;

const FG: &str = "c0c0c0";
const BG: &str = "101010";
/// The underline's own color, SGR 58.
const LINE: &str = "ff2040";

/// The glyph's alpha in each of its 16 columns, the same on every row: a
/// transparent margin with faint ringing (5 and 3, below the threshold),
/// a partly transparent edge (128), an opaque body, and a faint pixel at
/// 20 that still counts as ink.
const ALPHA: [u8; 16] = [
    0, 5, 0, 128, 255, 255, 255, 255, 255, 255, 255, 255, 20, 0, 3, 0,
];
/// The first and last columns whose alpha is at least the threshold.
const INK: (u32, u32) = (3, 12);

/// The glyph's pixel at column `x`, premultiplied RGBA.
fn glyph_px(x: u32) -> [u8; 4] {
    let a = ALPHA[x as usize];
    [a, a / 2, a / 4, a]
}

/// The letter g's mask, 8×16: a stem down column 3 from row 6 to row 15,
/// so its descender crosses the underline.
fn g_px(x: u32, y: u32) -> u8 {
    if x == 3 && y >= 6 { 255 } else { 0 }
}

fn decorations() -> Decorations {
    Decorations {
        underline: UNDERLINE_ROW,
        underline_thickness: 1,
        strike: STRIKE_ROW,
        strike_thickness: 1,
        overline: 0,
    }
}

/// A set with the smile in color, and the space and the letter g as masks.
fn set() -> Arc<Set> {
    let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, vtium_font::Style::default())
        .decorations(decorations());
    let px = (0..16).flat_map(|_| (0..16).flat_map(glyph_px)).collect();
    let g = color.glyph(Width::Wide, px);
    color.map(SMILE, g);
    let mut text = StrikeBuilder::new(PixelFormat::R8, CELL, vtium_font::Style::default())
        .decorations(decorations());
    let blank = text.glyph(Width::Narrow, vec![0; 8 * 16]);
    text.map(' ', blank);
    let px = (0..16)
        .flat_map(|y| (0..8).map(move |x| g_px(x, y)))
        .collect();
    let g = text.glyph(Width::Narrow, px);
    text.map('g', g);
    let bytes = SetBuilder::new("emoji-ink-test")
        .strike(text)
        .strike(color)
        .write()
        .expect("the set writes");
    Arc::new(Set::from_bytes(bytes).expect("the set reads"))
}

fn font(height: u32) -> Font<SetFace> {
    Font::new(SetFace::new(set(), height).expect("a face"))
}

/// The cells across: the smile's two, the g, and a blank cell.
const COLS: usize = 4;

/// The smile and a g in `st`, then a blank cell in `st`; with `glyphs`
/// false, all four cells are blank in `st`.
fn screen(st: Style, glyphs: bool) -> Screen {
    let mut s = Screen::new(COLS, 1, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    if glyphs {
        s.put(0, 0, SMILE, true, st);
        s.put(2, 0, 'g', false, st);
    } else {
        s.text(0, 0, "  ", st);
        s.put(2, 0, ' ', false, st);
    }
    s.put(3, 0, ' ', false, st);
    s
}

fn plain() -> Style {
    style(rgb(FG), rgb(BG))
}

fn underlined() -> Style {
    let mut st = plain();
    st.attrs.set_underline(UnderlineStyle::Single);
    st.underline_color = Color::Rgb(rgb(LINE));
    st
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// The display's size for a cell `height` pixels high.
fn size(height: u32) -> (u32, u32) {
    let s = height / CELL.height;
    (COLS as u32 * CELL.width * s, height)
}

/// `s` drawn on the CPU path at a cell `height` pixels high.
fn draw(s: &Screen, height: u32) -> Vec<u32> {
    let (w, h) = size(height);
    let mut r = Renderer::new(font(height), w, h, Config::default());
    assert_eq!(r.grid_size(), (COLS, 1));
    let mut c = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s));
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

/// `s` drawn on the GPU path at a cell `height` pixels high, on the
/// stand-in GL, whose model of the cell shader draws the cells.
#[cfg(feature = "gpu")]
fn gpu(s: &Screen, height: u32) -> Vec<u32> {
    use vtium_render::gpu::{Gl, Pipeline, Size, StandIn};
    let (w, h) = size(height);
    let size = Size::new(w, h);
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
    let buffer = gl.import_buffer(&size).unwrap();
    let mut r = Renderer::new(font(height), w, h, Config::default());
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

fn pixel(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

/// The premultiplied `src` over the opaque pixel `under`, each channel
/// `src + under (255 - a) / 255` rounded to the nearest.
fn over(src: [u8; 4], under: u32) -> u32 {
    let a = u32::from(src[3]);
    let ch = |s: u8, shift: u32| {
        let d = (under >> shift) & 0xff;
        (u32::from(s) + (d * (255 - a) * 2 + 255) / 510) << shift
    };
    ch(src[0], 16) | ch(src[1], 8) | ch(src[2], 0)
}

/// What the underlined screen should draw at a cell `height` pixels high,
/// from `glyphs`, the screen with no underline, and `lines`, the underline
/// under blank cells. Where the line is drawn and the pixel lies outside
/// the gaps, the smile's pixel goes over the line's pixel, and the g's
/// cell shows the line. Elsewhere the pixel is the glyph's. Also gives how
/// many of the line's pixels in the smile's cells are skipped and how many
/// are drawn.
fn expected(glyphs: &[u32], lines: &[u32], height: u32) -> (Vec<u32>, (usize, usize)) {
    let s = height / CELL.height;
    let (w, h) = size(height);
    let bg = pixel(BG);
    let gap = height >= 32;
    let mut want = Vec::new();
    let mut counts = (0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let line = lines[i] != bg;
            let (gx, gy) = (x / s, y / s);
            let px = if gx < 16 {
                // the smile: its ink dilated by one display pixel when the
                // cell is tall enough
                let ink = (INK.0 * s, (INK.1 + 1) * s - 1);
                let skipped = gap && x + 1 >= ink.0 && x <= ink.1 + 1;
                if line && skipped {
                    counts.0 += 1;
                } else if line {
                    counts.1 += 1;
                }
                if line && !skipped {
                    over(glyph_px(gx), lines[i])
                } else {
                    glyphs[i]
                }
            } else if gx < 24 {
                // the g: its ink, one display pixel each side, skipped
                let near = (x.saturating_sub(1)..=x + 1)
                    .filter(|&n| n / s >= 16 && n / s < 24)
                    .any(|n| g_px(n / s - 16, gy) != 0);
                if line && !near { lines[i] } else { glyphs[i] }
            } else if line {
                lines[i]
            } else {
                glyphs[i]
            };
            want.push(px);
        }
    }
    (want, counts)
}

fn differ(got: &[u32], want: &[u32], w: u32) -> Option<((u32, u32), u32, u32)> {
    got.iter()
        .zip(want)
        .position(|(g, w)| g != w)
        .map(|i| ((i as u32 % w, i as u32 / w), got[i], want[i]))
}

/// The underlined screen on the CPU path at `height`, what it should
/// draw, and the counts [`expected`] gives.
fn cpu(height: u32) -> (Vec<u32>, Vec<u32>, (usize, usize)) {
    let got = draw(&screen(underlined(), true), height);
    let glyphs = draw(&screen(plain(), true), height);
    let lines = draw(&screen(underlined(), false), height);
    let (want, counts) = expected(&glyphs, &lines, height);
    (got, want, counts)
}

// At 16 pixels a cell, the underline runs whole behind the smile: the
// smile's transparent margin shows the line, its faint ringing and its
// partly transparent edge blend over the line, and its opaque body hides
// the line. No pixel of the line is skipped.
// covers: text.emoji
#[test]
fn a_short_cells_underline_runs_whole_behind_a_color_glyph() {
    let (got, want, (skipped, drawn)) = cpu(16);
    let d = differ(&got, &want, size(16).0);
    assert!(d.is_none(), "{d:x?}");
    assert_eq!(skipped, 0);
    assert_eq!(drawn, 16, "the line crosses the smile's two cells");
    // the line shows in the margin, beside the ink on both sides
    let row = (UNDERLINE_ROW * size(16).0) as usize;
    assert_eq!(got[row + 2], pixel(LINE));
    assert_eq!(got[row + 13], pixel(LINE));
}

// At 32 pixels a cell, the underline stops one display pixel short of the
// smile's ink on each side and shows in the rest of the margin. The
// opaque body is the glyph's own color, never the line's.
// covers: text.emoji
#[test]
fn a_tall_cells_underline_keeps_a_gap_around_a_color_glyphs_ink() {
    let (got, want, (skipped, drawn)) = cpu(32);
    let w = size(32).0;
    let d = differ(&got, &want, w);
    assert!(d.is_none(), "{d:x?}");
    assert!(skipped > 0 && drawn > 0, "{skipped} skipped, {drawn} drawn");
    let lines = draw(&screen(underlined(), false), 32);
    let rows: Vec<u32> = (0..32)
        .filter(|&y| lines[(y * w) as usize] != pixel(BG))
        .collect();
    assert!(!rows.is_empty());
    for y in rows {
        let at = |x: u32| got[(y * w + x) as usize];
        // the margin's first pixels show the line, as does the last
        assert_eq!(at(0), pixel(LINE), "row {y}");
        assert_eq!(at(31), pixel(LINE), "row {y}");
        // the gap before the edge at columns 6 and 7, and after the faint
        // pixel at 24 and 25
        assert_eq!(at(5), pixel(BG), "row {y}");
        assert_eq!(at(26), pixel(BG), "row {y}");
        // the opaque body is the glyph's color
        for x in 8..24 {
            assert_eq!(at(x), over(glyph_px(x / 2), 0), "row {y}, {x}");
        }
    }
}

// The strike-through draws over the smile's opaque body, at every height,
// and the underline still lies behind it.
// covers: text.emoji
#[test]
fn the_strike_draws_over_a_color_glyph_that_lies_over_its_underline() {
    for height in [16, 32] {
        let mut st = underlined();
        st.attrs.insert(Attrs::STRIKE);
        let (w, _) = size(height);
        let got = draw(&screen(st, true), height);
        let strike = draw(&screen(plain_struck(), false), height);
        let mut crossed = 0;
        for (i, (&g, &s)) in got.iter().zip(&strike).enumerate() {
            let x = i as u32 % w;
            if s != pixel(BG) && x < 16 * (height / 16) {
                assert_eq!(g, s, "{height}: {x},{}", i as u32 / w);
                crossed += 1;
            }
        }
        assert!(crossed > 0);
    }
}

fn plain_struck() -> Style {
    let mut st = plain();
    st.attrs.insert(Attrs::STRIKE);
    st
}

// A letter beside the smile still breaks the line around its descender,
// at both heights, as it did before color glyphs had ink of their own. The
// expectations above check the g's cell too; here the gap around its stem
// is seen directly.
// covers: sgr.underline-style
#[test]
fn a_letter_beside_a_color_glyph_still_breaks_the_line_around_its_descender() {
    for height in [16u32, 32] {
        let s = height / 16;
        let (got, _, _) = cpu(height);
        let w = size(height).0;
        let y = (UNDERLINE_ROW * s + s / 2) * w;
        let at = |x: u32| got[(y + x) as usize];
        // the stem at column 3 of the g's cell, and a pixel each side
        let stem = (16 + 3) * s;
        assert_eq!(at(stem - 1), pixel(BG), "{height}");
        assert_eq!(at(stem), pixel(FG), "{height}");
        assert_eq!(at(stem + s), pixel(BG), "{height}");
        assert_eq!(at(stem - 2), pixel(LINE), "{height}");
        assert_eq!(at(stem + s + 1), pixel(LINE), "{height}");
    }
}

// The cell shader draws what the CPU path draws, at both heights, with
// the underline, with the strike and with both.
// covers: text.emoji
#[cfg(feature = "gpu")]
#[test]
fn a_color_glyphs_underline_draws_the_same_on_the_gpu_path() {
    let mut both = underlined();
    both.attrs.insert(Attrs::STRIKE);
    for height in [16, 32] {
        for st in [underlined(), plain_struck(), both] {
            let s = screen(st, true);
            let (w, _) = size(height);
            let d = differ(&gpu(&s, height), &draw(&s, height), w);
            assert!(d.is_none(), "{height}, {st:?}: {d:x?}");
        }
        let (_, want, _) = cpu(height);
        let d = differ(
            &gpu(&screen(underlined(), true), height),
            &want,
            size(height).0,
        );
        assert!(d.is_none(), "{height}: against the expectation: {d:x?}");
    }
}
