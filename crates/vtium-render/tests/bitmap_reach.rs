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
use vtium_term::Age;

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
