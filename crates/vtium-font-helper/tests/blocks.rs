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
//!
//! The same holds under synthetic italic, which leaves these glyphs
//! upright, and synthetic bold grows their strokes as much as the text's.
//! A font with no full block has its descent and ascent mapped onto the
//! cell. A font whose full block lies far outside its line has these
//! glyphs placed as text is placed.

use vtium_font::Style;
use vtium_font_helper::frit::Hinting;
use vtium_font_helper::outline::{Outline, OutlineRequest, Placed, rasterise};
use vtium_testfont::ttf;

const FULL: u32 = 0x2588;
const UPPER_HALF: u32 = 0x2580;
const LIGHT_VERTICAL: u32 = 0x2502;
const HEAVY_VERTICAL: u32 = 0x2503;
const LIGHT_HORIZONTAL: u32 = 0x2500;
const HEAVY_HORIZONTAL: u32 = 0x2501;

const HEIGHTS: [u32; 3] = [12, 16, 24];

/// A rectangle in font units with y up: `x0, y0` to `x1, y1`.
type Rect = (i16, i16, i16, i16);

/// One font's block and box glyphs, each as a [`Rect`].
struct Shape {
    name: &'static str,
    upem: u16,
    ascent: i16,
    descent: i16,
    advance: u16,
    full: Rect,
    upper_half: Rect,
    light_vertical: Rect,
    heavy_vertical: Rect,
    light_horizontal: Rect,
    heavy_horizontal: Rect,
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

/// A font whose full block starts a quarter of its advance left of its
/// pen, so its box lines sit as far left of where the advance would put
/// them, and only the full block's left edge places them in the cell.
const OVERHANG: Shape = Shape {
    name: "a full block left of its pen",
    upem: 1000,
    ascent: 800,
    descent: -200,
    advance: 600,
    full: (-150, -200, 450, 800),
    upper_half: (-150, 300, 450, 800),
    light_vertical: (100, -200, 200, 800),
    heavy_vertical: (50, -200, 250, 800),
    light_horizontal: (-150, 250, 450, 350),
    heavy_horizontal: (-150, 200, 450, 400),
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
    font_of(
        shape,
        &[
            (FULL, shape.full),
            (UPPER_HALF, shape.upper_half),
            (LIGHT_VERTICAL, shape.light_vertical),
            (HEAVY_VERTICAL, shape.heavy_vertical),
            (LIGHT_HORIZONTAL, shape.light_horizontal),
            (HEAVY_HORIZONTAL, shape.heavy_horizontal),
        ],
    )
}

/// A test font with `shape`'s metrics and a W, which maps each character
/// of `glyphs` to its rectangle, and nothing else.
fn font_of(shape: &Shape, glyphs: &[(u32, Rect)]) -> Vec<u8> {
    let rect = |(x0, y0, x1, y1)| ttf::Glyph::rect(shape.advance, x0, y0, x1, y1);
    let w = shape.advance as i16;
    let mut drawn = vec![
        ttf::Glyph::empty(shape.advance),
        rect((w / 10, 0, w - w / 10, shape.ascent * 2 / 3)),
    ];
    let mut cmap = vec![(u32::from('W'), 1)];
    for (i, &(c, r)) in glyphs.iter().enumerate() {
        drawn.push(rect(r));
        cmap.push((c, i as u16 + 2));
    }
    ttf::Font {
        family: "Block Test".to_owned(),
        upem: shape.upem,
        ascent: shape.ascent,
        descent: shape.descent,
        glyphs: drawn,
        cmap,
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

/// Where the ink of `row` is centred, in pixels from its left edge.
fn centre(row: &[u8]) -> f32 {
    let weight = row
        .iter()
        .enumerate()
        .map(|(x, &p)| (x as f32 + 0.5) * f32::from(p))
        .sum::<f32>();
    weight / (ink(row.iter().copied()) * 255.0)
}

/// How far a line's centre may lie from where the font's outline puts it:
/// the light hinter can move a stem sideways by part of a pixel to line
/// its edges up with the pixels, by up to 0.27 pixels in these fonts.
const CENTRE: f32 = 0.35;

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
    for shape in SHAPES.into_iter().chain([&OVERHANG]) {
        let o = drawn(shape);
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let name = shape.name;
            for (c, (x0, _, x1, _)) in [
                (LIGHT_VERTICAL, shape.light_vertical),
                (HEAVY_VERTICAL, shape.heavy_vertical),
            ] {
                let (line, w, _) = glyph(&o, i, c);
                let (bx0, _, bx1, _) = shape.full;
                // across, the block's width maps onto the cell's, its left
                // edge onto the cell's left edge
                let across = w as f32 / f32::from(bx1 - bx0);
                let want = f32::from(x1 - x0) * across;
                let middle = f32::from(x0 + x1 - 2 * bx0) / 2.0 * across;
                for (y, row) in rows(line, w).iter().enumerate() {
                    let got = ink(row.iter().copied());
                    let at = centre(row);
                    assert!(
                        (got - want).abs() < 0.1 && (at - middle).abs() < CENTRE,
                        "{name} at {h} pixels: row {y} of U+{c:04X} holds {got} pixels of ink centred at {at}, not {want} at {middle}: {row:?}"
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

// Synthetic italic slants text, but box lines and block elements stay
// upright, as vtium's own sprites do, so that they still meet the cells
// around them. A full block still covers every pixel of its cell, and a
// vertical line sits in the same columns on every row, so it meets the
// line in the cell above.
#[test]
fn under_synthetic_italic_blocks_and_lines_stay_upright() {
    for shape in SHAPES {
        let name = shape.name;
        let italic = Style {
            bold: false,
            italic: true,
        };
        let o = rasterise(&OutlineRequest {
            synth: italic,
            ..request(shape)
        })
        .expect("rasterised");
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let (block, w, _) = glyph(&o, i, FULL);
            for (y, row) in rows(block, w).iter().enumerate() {
                assert!(
                    row.iter().all(|&p| p >= SOLID),
                    "{name} at {h} pixels in italic: row {y} of the full block is {row:?}"
                );
            }
            let (line, w, _) = glyph(&o, i, LIGHT_VERTICAL);
            let (x0, _, x1, _) = shape.light_vertical;
            let (bx0, _, bx1, _) = shape.full;
            let middle = f32::from(x0 + x1 - 2 * bx0) / 2.0 * w as f32 / f32::from(bx1 - bx0);
            for (y, row) in rows(line, w).iter().enumerate() {
                let at = centre(row);
                assert!(
                    (at - middle).abs() < CENTRE,
                    "{name} at {h} pixels in italic: row {y} of U+2502 is centred at {at}, not {middle}: {row:?}"
                );
            }
        }
    }
}

/// A font whose full block is a quarter of its em high, so its box lines
/// and block elements are drawn at four times the text's size.
const SHORT_BLOCK: Shape = Shape {
    name: "a short full block",
    upem: 1000,
    ascent: 800,
    descent: -200,
    advance: 600,
    full: (0, 0, 600, 250),
    upper_half: (0, 125, 600, 250),
    light_vertical: (250, 0, 350, 250),
    heavy_vertical: (200, 0, 400, 250),
    light_horizontal: (0, 100, 600, 150),
    heavy_horizontal: (0, 75, 600, 175),
};

// Synthetic bold grows a box line's strokes as much as it grows the
// text's: by a 48th of the text's size on each side, and at least half a
// pixel. Every height here is under 24 pixels to the em, so a light
// vertical line grows by one pixel in all. The font draws its box lines at
// four times the text's size, which must not grow them four times as much.
#[test]
fn synthetic_bold_grows_a_box_line_as_much_as_the_text() {
    let name = SHORT_BLOCK.name;
    let regular = drawn(&SHORT_BLOCK);
    let bold = rasterise(&OutlineRequest {
        synth: Style {
            bold: true,
            italic: false,
        },
        ..request(&SHORT_BLOCK)
    })
    .expect("rasterised");
    for (i, h) in HEIGHTS.into_iter().enumerate() {
        let (thin, w, _) = glyph(&regular, i, LIGHT_VERTICAL);
        let (thick, _, _) = glyph(&bold, i, LIGHT_VERTICAL);
        for (y, (a, b)) in rows(thin, w).iter().zip(rows(thick, w)).enumerate() {
            let grown = ink(b.iter().copied()) - ink(a.iter().copied());
            assert!(
                (grown - 1.0).abs() < 0.15,
                "{name} at {h} pixels: bold grows row {y} of U+2502 by {grown} pixels, not 1: {a:?} to {b:?}"
            );
        }
    }
}

/// A private-use character that is neither a box line, a block element
/// nor a Powerline separator, so the helper places it as it places text.
const PLAIN: u32 = 0xe000;

// A full block far from its font's line would have the helper draw the
// box lines and block elements at an extreme size: a full block one unit
// high at 1000 units to the em would draw them at about 1000 times the
// text's size. The helper maps the box onto the cell only while that size
// is between an eighth of the text's size and eight times it. Outside it,
// these glyphs are placed as text is, so each is drawn exactly as the same
// outline mapped to a private-use character.
#[test]
fn a_full_block_far_outside_its_line_is_placed_as_text() {
    for (what, full) in [
        ("a full block one unit high", (0, 0, 600, 1)),
        ("a full block nine ems high", (0, -4000, 600, 5000)),
    ] {
        let bytes = font_of(
            &SHORT_BLOCK,
            &[
                (FULL, full),
                (PLAIN, full),
                (LIGHT_VERTICAL, (250, 0, 350, 1)),
            ],
        );
        let o = rasterise(&OutlineRequest {
            font: bytes.into(),
            ..request(&SHORT_BLOCK)
        })
        .expect("rasterised");
        for (i, h) in HEIGHTS.into_iter().enumerate() {
            let (block, _, _) = glyph(&o, i, FULL);
            let (text, _, _) = glyph(&o, i, PLAIN);
            assert!(
                block.pixels == text.pixels && block.ink == text.ink,
                "{what} at {h} pixels: the full block is drawn as {:?} with ink {:?}, not as text, {:?} with ink {:?}",
                block.pixels,
                block.ink,
                text.pixels,
                text.ink
            );
        }
    }
}

/// A font with no full block, whose upper half block and vertical line
/// are drawn from its descent to its ascent.
const NO_FULL_BLOCK: Shape = Shape {
    name: "a font with no full block",
    upem: 1000,
    ascent: 800,
    descent: -200,
    advance: 600,
    full: (0, -200, 600, 800),
    upper_half: (0, 300, 600, 800),
    light_vertical: (250, -200, 350, 800),
    heavy_vertical: (200, -200, 400, 800),
    light_horizontal: (0, 250, 600, 350),
    heavy_horizontal: (0, 200, 600, 400),
};

// A font with no full block has its descent and ascent mapped onto the
// cell's bottom and top, and its advance across. An upper half block drawn
// from the middle of that span to the ascent covers exactly the top half,
// and a vertical line holds the same ink on every row, in the middle of
// the cell.
#[test]
fn with_no_full_block_the_ascent_and_descent_span_the_cell() {
    let shape = &NO_FULL_BLOCK;
    let name = shape.name;
    let bytes = font_of(
        shape,
        &[
            (UPPER_HALF, shape.upper_half),
            (LIGHT_VERTICAL, shape.light_vertical),
        ],
    );
    let o = rasterise(&OutlineRequest {
        font: bytes.into(),
        ..request(shape)
    })
    .expect("rasterised");
    for (i, h) in HEIGHTS.into_iter().enumerate() {
        let (half, w, _) = glyph(&o, i, UPPER_HALF);
        for (y, row) in rows(half, w).iter().enumerate() {
            let (top, want) = (y < h as usize / 2, row.iter().all(|&p| p >= SOLID));
            let empty = row.iter().all(|&p| p <= EMPTY);
            assert!(
                if top { want } else { empty },
                "{name} at {h} pixels: row {y} of the upper half block is {row:?}"
            );
        }
        assert!(half.ink.is_none(), "{name} at {h} pixels: {:?}", half.ink);
        let (line, w, _) = glyph(&o, i, LIGHT_VERTICAL);
        let want = 100.0 * w as f32 / 600.0;
        let middle = w as f32 / 2.0;
        for (y, row) in rows(line, w).iter().enumerate() {
            let (got, at) = (ink(row.iter().copied()), centre(row));
            assert!(
                (got - want).abs() < 0.1 && (at - middle).abs() < CENTRE,
                "{name} at {h} pixels: row {y} of U+2502 holds {got} pixels centred at {at}, not {want} at {middle}: {row:?}"
            );
        }
    }
}
