//! Block elements and box lines drawn from an outline font fill their
//! character cell exactly, whatever the font's vertical metrics. Each test
//! font copies the metrics and the outlines of one real font's block and
//! box glyphs, as measured from the font files:
//!
//! - Fira Code spans its full block exactly from its descent to its
//!   ascent. Sized so that the ascent and descent span the cell less half
//!   a pixel, the block fell half a pixel short of the cell's top.
//! - JetBrains Mono does the same, and its box lines run a tenth of an em
//!   past the block, as Fira Code's do.
//! - Cascadia Code draws its block up to its Windows ascent, 2226 units,
//!   while its line's ascent is 1900, so the block rose past the cell.
//! - DejaVu Sans Mono draws its block 20 units above its ascent and 29
//!   below its descent, and 20 units past both sides of its advance.
//!
//! The checks run at cells 12, 16 and 24 pixels high. A full block covers
//! every pixel of its cell and has no ink past it. An upper half block
//! covers exactly the top half. A vertical line (U+2502) holds the same
//! ink on every row from the top to the bottom, and a horizontal line
//! (U+2500) the same ink in every column from the left edge to the right.
//! Each line keeps the font's stroke width, light and heavy.

use vtium_config::frit::Hinting;
use vtium_config::outline::{Outline, OutlineRequest, Placed, rasterise};
use vtium_testfont::ttf;

const FULL: u32 = 0x2588;
const UPPER_HALF: u32 = 0x2580;
const LIGHT_VERTICAL: u32 = 0x2502;
const HEAVY_VERTICAL: u32 = 0x2503;
const LIGHT_HORIZONTAL: u32 = 0x2500;
const HEAVY_HORIZONTAL: u32 = 0x2501;

const HEIGHTS: [u32; 3] = [12, 16, 24];

/// One font's block and box glyphs, in font units with y up: each as the
/// rectangle `x0, y0` to `x1, y1`.
struct Shape {
    name: &'static str,
    upem: u16,
    ascent: i16,
    descent: i16,
    advance: u16,
    full: (i16, i16, i16, i16),
    upper_half: (i16, i16, i16, i16),
    light_vertical: (i16, i16, i16, i16),
    heavy_vertical: (i16, i16, i16, i16),
    light_horizontal: (i16, i16, i16, i16),
    heavy_horizontal: (i16, i16, i16, i16),
}

const FIRA_CODE: Shape = Shape {
    name: "Fira Code",
    upem: 1950,
    ascent: 1800,
    descent: -600,
    advance: 1200,
    full: (0, -600, 1200, 1800),
    upper_half: (0, 600, 1200, 1800),
    light_vertical: (520, -1000, 680, 2000),
    heavy_vertical: (360, -1000, 840, 2000),
    light_horizontal: (-10, 620, 1210, 780),
    heavy_horizontal: (-10, 460, 1210, 940),
};

const JETBRAINS_MONO: Shape = Shape {
    name: "JetBrains Mono",
    upem: 1000,
    ascent: 1020,
    descent: -300,
    advance: 600,
    full: (0, -300, 600, 1020),
    upper_half: (0, 360, 600, 1020),
    light_vertical: (250, -400, 350, 1120),
    heavy_vertical: (200, -400, 400, 1120),
    light_horizontal: (-20, 320, 620, 420),
    heavy_horizontal: (-20, 270, 620, 470),
};

const CASCADIA_CODE: Shape = Shape {
    name: "Cascadia Code",
    upem: 2048,
    ascent: 1900,
    descent: -480,
    advance: 1200,
    full: (0, -480, 1200, 2226),
    upper_half: (0, 873, 1200, 2226),
    light_vertical: (496, -530, 704, 2226),
    heavy_vertical: (392, -530, 808, 2226),
    light_horizontal: (-104, 603, 1304, 811),
    heavy_horizontal: (-104, 499, 1304, 915),
};

const DEJAVU_SANS_MONO: Shape = Shape {
    name: "DejaVu Sans Mono",
    upem: 2048,
    ascent: 1901,
    descent: -483,
    advance: 1233,
    full: (-20, -512, 1253, 1921),
    upper_half: (-20, 704, 1253, 1920),
    light_vertical: (536, -512, 696, 1921),
    heavy_vertical: (456, -512, 776, 1921),
    light_horizontal: (-20, 618, 1253, 790),
    heavy_horizontal: (-20, 532, 1253, 876),
};

const SHAPES: [&Shape; 4] = [
    &FIRA_CODE,
    &JETBRAINS_MONO,
    &CASCADIA_CODE,
    &DEJAVU_SANS_MONO,
];

/// The test font for `shape`: its W fills its advance up to the ascent,
/// so the W rule gives the advance as the cell's width.
fn font(shape: &Shape) -> Vec<u8> {
    let rect = |(x0, y0, x1, y1)| ttf::Glyph::rect(shape.advance, x0, y0, x1, y1);
    let w = shape.advance as i16;
    let glyphs = vec![
        ttf::Glyph::empty(shape.advance),
        rect((w / 10, 0, w - w / 10, shape.ascent * 2 / 3)),
        rect(shape.full),
        rect(shape.upper_half),
        rect(shape.light_vertical),
        rect(shape.heavy_vertical),
        rect(shape.light_horizontal),
        rect(shape.heavy_horizontal),
    ];
    ttf::Font {
        family: "Block Test".to_owned(),
        upem: shape.upem,
        ascent: shape.ascent,
        descent: shape.descent,
        glyphs,
        cmap: vec![
            (u32::from('W'), 1),
            (FULL, 2),
            (UPPER_HALF, 3),
            (LIGHT_VERTICAL, 4),
            (HEAVY_VERTICAL, 5),
            (LIGHT_HORIZONTAL, 6),
            (HEAVY_HORIZONTAL, 7),
        ],
        colr: Vec::new(),
        palette: Vec::new(),
        cbdt: None,
        gsub: Vec::new(),
        gpos: Vec::new(),
        underline: None,
        os2: None,
        empty_gpos: false,
    }
    .build()
}

/// A request for `shape`'s font at each of [`HEIGHTS`], as the frit's
/// first source draws it.
fn request(shape: &Shape) -> OutlineRequest {
    OutlineRequest {
        double_width: false,
        cluster_width: Default::default(),
        fit: false,
        synth: Default::default(),
        script: Default::default(),
        font: font(shape).into(),
        family: "Block Test".to_owned(),
        cells: HEIGHTS.iter().map(|&h| (0, h)).collect(),
        cover: Vec::new(),
        clusters: Vec::new(),
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::Light,
    }
}

/// `shape`'s font drawn at each of [`HEIGHTS`].
fn drawn(shape: &Shape) -> Outline {
    rasterise(&request(shape)).expect("rasterised")
}

/// The glyph for `c` in strike `strike` of `o`, and its cell's width and
/// height.
fn glyph(o: &Outline, strike: usize, c: u32) -> (&Placed, usize, usize) {
    let s = &o.strikes[strike];
    let (_, g) = s.map.iter().find(|(cp, _)| *cp == c).expect("mapped");
    (&s.glyphs[*g as usize], s.width as usize, s.height as usize)
}

/// Each row of `p`, `w` pixels wide.
fn rows(p: &Placed, w: usize) -> Vec<&[u8]> {
    p.pixels.chunks(w).collect()
}

/// The ink of a row or column, in pixels of full coverage.
fn ink(pixels: impl Iterator<Item = u8>) -> f32 {
    pixels.map(f32::from).sum::<f32>() / 255.0
}

/// A pixel a block covers wholly, allowing for rounding in the
/// rasteriser.
const SOLID: u8 = 0xf8;
/// A pixel a block leaves empty, allowing for an edge that a font draws
/// a hair past a half block's middle.
const EMPTY: u8 = 0x08;

#[test]
fn a_full_block_covers_its_cell_and_nothing_past_it() {
    for shape in SHAPES {
        let o = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let name = shape.name;
            let (block, w, ch) = glyph(&o, i, FULL);
            assert_eq!(ch, h as usize);
            for (y, row) in rows(block, w).iter().enumerate() {
                assert!(
                    row.iter().all(|&p| p >= SOLID),
                    "{name} at {h} pixels: row {y} of the full block is {row:?}"
                );
            }
            assert!(
                block.ink.is_none(),
                "{name} at {h} pixels: the full block has ink past its cell: {:?}",
                block.ink
            );
        }
    }
}

#[test]
fn an_upper_half_block_covers_the_top_half_exactly() {
    for shape in SHAPES {
        let o = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let name = shape.name;
            let (half, w, _) = glyph(&o, i, UPPER_HALF);
            for (y, row) in rows(half, w).iter().enumerate() {
                if y < h as usize / 2 {
                    assert!(
                        row.iter().all(|&p| p >= SOLID),
                        "{name} at {h} pixels: row {y} of the upper half block is {row:?}"
                    );
                } else {
                    assert!(
                        row.iter().all(|&p| p <= EMPTY),
                        "{name} at {h} pixels: row {y} of the upper half block is {row:?}"
                    );
                }
            }
            assert!(half.ink.is_none(), "{name} at {h} pixels: {:?}", half.ink);
        }
    }
}

/// The stroke width in pixels of a line `units` thick in `shape`, drawn
/// at the size where the font's full block spans a cell `h` pixels high.
fn stroke(shape: &Shape, units: i16, h: u32) -> f32 {
    let (_, y0, _, y1) = shape.full;
    f32::from(units) * h as f32 / f32::from(y1 - y0)
}

#[test]
fn a_vertical_line_runs_from_the_cells_top_to_its_bottom_at_the_fonts_stroke() {
    for shape in SHAPES {
        let o = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let name = shape.name;
            for (c, (x0, _, x1, _)) in [
                (LIGHT_VERTICAL, shape.light_vertical),
                (HEAVY_VERTICAL, shape.heavy_vertical),
            ] {
                let (line, w, _) = glyph(&o, i, c);
                let (_, _, bw, _) = shape.full;
                let (bx, _, _, _) = shape.full;
                // across, the block's width maps onto the cell's
                let want = f32::from(x1 - x0) * w as f32 / f32::from(bw - bx);
                for (y, row) in rows(line, w).iter().enumerate() {
                    let got = ink(row.iter().copied());
                    assert!(
                        (got - want).abs() < 0.1,
                        "{name} at {h} pixels: row {y} of U+{c:04X} holds {got} pixels of ink, not {want}: {row:?}"
                    );
                }
                assert!(line.ink.is_none(), "{name} at {h} pixels: {:?}", line.ink);
            }
        }
    }
}

#[test]
fn a_horizontal_line_meets_both_edges_at_the_fonts_stroke() {
    for shape in SHAPES {
        let o = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let name = shape.name;
            let mut widths = Vec::new();
            for (c, (_, y0, _, y1)) in [
                (LIGHT_HORIZONTAL, shape.light_horizontal),
                (HEAVY_HORIZONTAL, shape.heavy_horizontal),
            ] {
                let (line, w, ch) = glyph(&o, i, c);
                let want = stroke(shape, y1 - y0, h);
                for x in 0..w {
                    let got = ink((0..ch).map(|y| line.pixels[y * w + x]));
                    assert!(
                        (got - want).abs() < 0.1,
                        "{name} at {h} pixels: column {x} of U+{c:04X} holds {got} pixels of ink, not {want}"
                    );
                }
                assert!(line.ink.is_none(), "{name} at {h} pixels: {:?}", line.ink);
                widths.push(want);
            }
            assert!(widths[1] > widths[0], "{name}: heavy is wider than light");
        }
    }
}

// On a line of double width (DECDWL), the request names a cell twice as
// wide, and the helper stretches each outline sideways after mapping it.
// The full block still covers that whole cell, and a horizontal line meets
// both of its edges, and a vertical line keeps its place and its share of
// the cell's width, twice its normal stroke. The light hinter can move a
// stem sideways by part of a pixel, which the stretch doubles, so the
// line's centre may lie up to half a pixel from the outline's.
#[test]
fn on_a_line_of_double_width_a_block_fills_the_wider_cell() {
    for shape in SHAPES {
        let name = shape.name;
        let normal = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let w = normal.strikes[i].width;
            let request = OutlineRequest {
                double_width: true,
                cells: vec![(2 * w, h)],
                font: font(shape).into(),
                ..request(shape)
            };
            let o = rasterise(&request).expect("rasterised");
            let (block, bw, _) = glyph(&o, 0, FULL);
            assert_eq!(bw, 2 * w as usize);
            assert!(
                block.pixels.iter().all(|&p| p >= SOLID),
                "{name} at {h} pixels: {:?}",
                block.pixels
            );
            assert!(block.ink.is_none(), "{name} at {h} pixels: {:?}", block.ink);
            let (line, _, ch) = glyph(&o, 0, LIGHT_HORIZONTAL);
            let column = |x: usize| ink((0..ch).map(|y| line.pixels[y * bw + x]));
            assert!(
                (column(0) - column(bw - 1)).abs() < 0.1 && column(0) > 0.5,
                "{name} at {h} pixels: the line's edge columns hold {} and {}",
                column(0),
                column(bw - 1)
            );
            let (line, _, _) = glyph(&o, 0, LIGHT_VERTICAL);
            let (x0, _, x1, _) = shape.light_vertical;
            let (bx0, _, bx1, _) = shape.full;
            let want = f32::from(x1 - x0) * bw as f32 / f32::from(bx1 - bx0);
            let middle = f32::from(x0 + x1 - 2 * bx0) / 2.0 * bw as f32 / f32::from(bx1 - bx0);
            for (y, row) in rows(line, bw).iter().enumerate() {
                let got = ink(row.iter().copied());
                let centre = row
                    .iter()
                    .enumerate()
                    .map(|(x, &p)| (x as f32 + 0.5) * f32::from(p))
                    .sum::<f32>()
                    / (got * 255.0);
                assert!(
                    (got - want).abs() < 0.1 && (centre - middle).abs() < 0.5,
                    "{name} at {h} pixels: row {y} of U+2502 holds {got} pixels centred at {centre}, not {want} at {middle}: {row:?}"
                );
            }
        }
    }
}
