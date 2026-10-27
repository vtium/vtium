//! The rest of Symbols for Legacy Computing (U+1FB3C-U+1FBAF,
//! U+1FBBD-U+1FBBF and U+1FBCE-U+1FBEF) and the one quarter blocks of its
//! supplement (U+1CEA0-U+1CEAF): smooth mosaics, triangles, eighth, third
//! and quarter blocks, shades, fills, light diagonals and circles. The
//! block's pictographs, U+1FBB0-U+1FBBC, U+1FBC0-U+1FBCD and
//! U+1FBF0-U+1FBFA, are left to the face.
//!
//! Adapted from Ghostty's sprite font, src/font/sprite/draw/ at 9ee02ae6:
//!
//! Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors
//!
//! Permission is hereby granted, free of charge, to any person obtaining a
//! copy of this software and associated documentation files (the
//! "Software"), to deal in the Software without restriction, including
//! without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to
//! permit persons to whom the Software is furnished to do so, subject to
//! the following conditions:
//!
//! The above copyright notice and this permission notice shall be included
//! in all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
//! OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//! IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
//! TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
//! SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//!
//! From symbols_for_legacy_computing.zig:
//! - smooth mosaics: draw1FB3C_1FB67(), a polygon through the cell's
//!   corners, the thirds down its sides and the centres of its top and
//!   bottom, found from a 3x4 picture of each character;
//! - triangles: edgeTriangle() and geometric_shapes.zig
//!   cornerTriangleShade(), the three quarter blocks drawn as the inverse
//!   of a one quarter triangle;
//! - eighth, quarter and third blocks and the shades: draw1FB70_1FB75(),
//!   draw1FB76_1FB7B(), draw1FB7C_1FB97(), draw1FBCE() and draw1FBCF(), with
//!   common.zig fill() and Fraction rounding;
//! - the checkerboards: checkerboardFill(), four squares across;
//! - the diagonal fills: draw1FB98() and draw1FB99(), light lines parallel
//!   to the cell's diagonal;
//! - the light diagonals: cornerDiagonalLines() and cellDiagonal(), and
//!   the negative ones inverted;
//! - the circles: circle(), as wide as the cell's shorter side and centred
//!   on an edge or a corner, white ones a light line's outline.
//!
//! From symbols_for_legacy_computing_supplement.zig: the one quarter
//! blocks, draw1CE90_1CEAF(), on a 4x4 grid.
//!
//! Where vtium draws otherwise:
//! - a diagonal's ends on the cell's edge, and the centre it meets, are
//!   the light lines' centre, so the diagonals join box drawing's lines,
//!   and a line through an edge runs on, cut off by the edge, rather than
//!   ending in a butt cap;
//! - the checkerboard's rows are an even number, at least two, so the board
//!   tiles down a column;
//! - the diagonal fills space their lines a whole share of the cell's width
//!   apart, so they run on into the cells beside and below;
//! - U+1FBAF's vertical stroke is heavy and spans the middle half of the
//!   cell, where Ghostty drew U+2542's full-height line;
//! - U+1FBE4-U+1FBE7 sit on the 4x4 grid the one quarter blocks use;
//! - slanted and curved edges are supersampled, as in `sprite.rs`, where
//!   Ghostty filled paths with z2d.

use super::{Pt, SS, centre, frac_max, frac_min};
use crate::glyph::Glyph;

/// Whether `cp` is one of this module's characters.
pub(super) fn has(cp: u32) -> bool {
    matches!(
        cp,
        0x1cea0..=0x1ceaf
            | 0x1fb3c..=0x1fb92
            | 0x1fb94..=0x1fbaf
            | 0x1fbbd..=0x1fbbf
            | 0x1fbce..=0x1fbef
    )
}

/// Draws `cp` into the empty `buf` with light lines `t` wide.
pub(super) fn draw(buf: &mut Glyph, cp: u32, t: u32) {
    let (w, h) = (buf.width(), buf.height());
    let index = |first: u32| usize::try_from(cp.saturating_sub(first)).unwrap_or(usize::MAX);
    let n = |first: u32| cp.saturating_sub(first);
    match cp {
        0x1cea0..=0x1ceaf => {
            if let Some(&[x0, x1, y0, y1]) = QUARTER_BLOCKS.get(index(0x1cea0)) {
                rect(buf, [x0, x1, 4], [y0, y1, 4], 0xff);
            }
        }
        0x1fb3c..=0x1fb67 => {
            if let Some(picture) = SMOOTH_MOSAICS.get(index(0x1fb3c)) {
                let (corners, len) = smooth_mosaic(picture, w, h);
                let corners = corners.get(..len).unwrap_or(&[]);
                paint(buf, &[poly(corners)], false, 0xff);
            }
        }
        // the three quarter blocks, each the inverse of a one quarter
        // triangle: left, upper, right, lower
        0x1fb68..=0x1fb6b => edge_triangle(buf, n(0x1fb68), true),
        0x1fb6c..=0x1fb6f => edge_triangle(buf, n(0x1fb6c), false),
        // vertical and horizontal one eighth blocks 2 to 7
        0x1fb70..=0x1fb75 => rect(buf, [n(0x1fb6f), n(0x1fb6e), 8], [0, 1, 1], 0xff),
        0x1fb76..=0x1fb7b => rect(buf, [0, 1, 1], [n(0x1fb75), n(0x1fb74), 8], 0xff),
        // one eighth blocks on two sides
        0x1fb7c => {
            rect(buf, [0, 1, 8], [0, 1, 1], 0xff);
            rect(buf, [0, 1, 1], [7, 8, 8], 0xff);
        }
        0x1fb7d => {
            rect(buf, [0, 1, 8], [0, 1, 1], 0xff);
            rect(buf, [0, 1, 1], [0, 1, 8], 0xff);
        }
        0x1fb7e => {
            rect(buf, [7, 8, 8], [0, 1, 1], 0xff);
            rect(buf, [0, 1, 1], [0, 1, 8], 0xff);
        }
        0x1fb7f => {
            rect(buf, [7, 8, 8], [0, 1, 1], 0xff);
            rect(buf, [0, 1, 1], [7, 8, 8], 0xff);
        }
        0x1fb80 => {
            rect(buf, [0, 1, 1], [0, 1, 8], 0xff);
            rect(buf, [0, 1, 1], [7, 8, 8], 0xff);
        }
        // horizontal one eighth blocks 1, 3, 5 and 8
        0x1fb81 => {
            for row in [0, 2, 4, 7] {
                rect(buf, [0, 1, 1], [row, row.saturating_add(1), 8], 0xff);
            }
        }
        // upper one quarter, three, five and seven eighths blocks
        0x1fb82..=0x1fb86 => {
            let eighths = [2, 3, 5, 6, 7];
            if let Some(&e) = eighths.get(index(0x1fb82)) {
                rect(buf, [0, 1, 1], [0, e, 8], 0xff);
            }
        }
        // right one quarter, three, five and seven eighths blocks
        0x1fb87..=0x1fb8b => {
            let eighths = [2, 3, 5, 6, 7];
            if let Some(&e) = eighths.get(index(0x1fb87)) {
                rect(buf, [8u32.saturating_sub(e), 8, 8], [0, 1, 1], 0xff);
            }
        }
        // left, right, upper and lower half medium shade
        0x1fb8c => rect(buf, [0, 1, 2], [0, 1, 1], 0x80),
        0x1fb8d => rect(buf, [1, 2, 2], [0, 1, 1], 0x80),
        0x1fb8e => rect(buf, [0, 1, 1], [0, 1, 2], 0x80),
        0x1fb8f => rect(buf, [0, 1, 1], [1, 2, 2], 0x80),
        // the inverse of half coverage is half coverage
        0x1fb90 => buf.fill(0, 0, w, h, 0x80),
        0x1fb91 => {
            buf.fill(0, 0, w, h, 0x80);
            rect(buf, [0, 1, 1], [0, 1, 2], 0xff);
        }
        0x1fb92 => {
            buf.fill(0, 0, w, h, 0x80);
            rect(buf, [0, 1, 1], [1, 2, 2], 0xff);
        }
        0x1fb94 => {
            buf.fill(0, 0, w, h, 0x80);
            rect(buf, [1, 2, 2], [0, 1, 1], 0xff);
        }
        0x1fb95 => checkerboard(buf, 0),
        0x1fb96 => checkerboard(buf, 1),
        // heavy horizontal fill: the second and fourth quarters down
        0x1fb97 => {
            rect(buf, [0, 1, 1], [1, 2, 4], 0xff);
            rect(buf, [0, 1, 1], [3, 4, 4], 0xff);
        }
        0x1fb98 => paint(buf, &[hatch(w, h, t, false)], false, 0xff),
        0x1fb99 => paint(buf, &[hatch(w, h, t, true)], false, 0xff),
        // upper and lower, and left and right, triangular half blocks
        0x1fb9a => edge_triangles(buf, &[1, 3]),
        0x1fb9b => edge_triangles(buf, &[0, 2]),
        // upper left, upper right, lower right and lower left triangular
        // medium shade: half the cell, cut by the diagonal
        0x1fb9c..=0x1fb9f => {
            let (wf, hf) = (f64::from(w), f64::from(h));
            let corners = match cp {
                0x1fb9c => [pt(0.0, 0.0), pt(0.0, hf), pt(wf, 0.0)],
                0x1fb9d => [pt(0.0, 0.0), pt(wf, hf), pt(wf, 0.0)],
                0x1fb9e => [pt(0.0, hf), pt(wf, hf), pt(wf, 0.0)],
                _ => [pt(0.0, 0.0), pt(0.0, hf), pt(wf, hf)],
            };
            paint(buf, &[poly(&corners)], false, 0x80);
        }
        0x1fba0..=0x1fbae => {
            if let Some(&quads) = CORNER_DIAGONALS.get(index(0x1fba0)) {
                corner_diagonals(buf, quads, t, false);
            }
        }
        // light horizontal with a heavy vertical stroke down its middle half
        0x1fbaf => {
            let y = h.saturating_sub(t) / 2;
            buf.fill(0, y, w, y.saturating_add(t), 0xff);
            let ht = t.saturating_mul(2);
            let x = w.saturating_sub(ht) / 2;
            buf.fill(
                x,
                frac_min(h, 1, 4),
                x.saturating_add(ht),
                frac_max(h, 3, 4),
                0xff,
            );
        }
        // negative diagonal cross, middle right to lower centre, and diamond
        0x1fbbd => {
            let p = Points::new(buf, t);
            let (wf, hf) = (f64::from(w), f64::from(h));
            let cross = [
                p.line(pt(0.0, 0.0), pt(wf, hf), true, true),
                p.line(pt(wf, 0.0), pt(0.0, hf), true, true),
            ];
            paint(buf, &cross, true, 0xff);
        }
        0x1fbbe => corner_diagonals(buf, BR, t, true),
        0x1fbbf => corner_diagonals(buf, TL | TR | BL | BR, t, true),
        // left two thirds and one third blocks
        0x1fbce => rect(buf, [0, 2, 3], [0, 1, 1], 0xff),
        0x1fbcf => rect(buf, [0, 1, 3], [0, 1, 1], 0xff),
        0x1fbd0..=0x1fbdf => {
            if let Some(path) = CELL_DIAGONALS.get(index(0x1fbd0)) {
                cell_diagonals(buf, path, t);
            }
        }
        // upper and lower centre, middle left and right one quarter blocks
        0x1fbe4 => rect(buf, [1, 3, 4], [0, 1, 2], 0xff),
        0x1fbe5 => rect(buf, [1, 3, 4], [1, 2, 2], 0xff),
        0x1fbe6 => rect(buf, [0, 1, 2], [1, 3, 4], 0xff),
        0x1fbe7 => rect(buf, [1, 2, 2], [1, 3, 4], 0xff),
        0x1fbe0..=0x1fbef => circle(buf, cp, t),
        _ => {}
    }
}

/// common.zig fill(): the block from `x[0]`/`x[2]` to `x[1]`/`x[2]` of the
/// width and likewise down, its near edges rounded as Fraction.min() and
/// its far edges as Fraction.max(), set to `alpha`.
fn rect(buf: &mut Glyph, x: [u32; 3], y: [u32; 3], alpha: u8) {
    let (w, h) = (buf.width(), buf.height());
    let [x0, x1, xd] = x;
    let [y0, y1, yd] = y;
    buf.fill(
        frac_min(w, x0, xd),
        frac_min(h, y0, yd),
        frac_max(w, x1, xd),
        frac_max(h, y1, yd),
        alpha,
    );
}

/// The one quarter blocks U+1CEA0-U+1CEAF on the 4x4 grid: left, right,
/// top and bottom, in quarters.
#[rustfmt::skip]
const QUARTER_BLOCKS: [[u32; 4]; 16] = [
    [2, 4, 3, 4], [1, 4, 3, 4], [0, 3, 3, 4], [0, 2, 3, 4], // lower, from the right
    [0, 1, 2, 4], [0, 1, 1, 4], [0, 1, 0, 3], [0, 1, 0, 2], // left, from the bottom
    [0, 2, 0, 1], [0, 3, 0, 1], [1, 4, 0, 1], [2, 4, 0, 1], // upper, from the left
    [3, 4, 0, 2], [3, 4, 0, 3], [3, 4, 1, 4], [3, 4, 2, 4], // right, from the top
];

/// The smooth mosaics U+1FB3C-U+1FB67 as Ghostty's pictures: three columns
/// by four rows, `#` filled, the slashes marking the diagonal.
#[rustfmt::skip]
const SMOOTH_MOSAICS: [&[u8; 12]; 44] = [
    b"......#..##.", b"......#\\.###", b"...#..#\\.##.", b"...#..##.###", // 1fb3c
    b"#..#..##.##.", b"/###########", b"./##########", b".##.########", // 1fb40
    b"..#.########", b".##.##.#####", b"..../#######", b"........#.##", // 1fb44
    b"......./####", b".....#./#.##", b".....#.#####", b"..#..#.##.##", // 1fb48
    b"##\\#########", b"#\\.#########", b"##.##.######", b"#..##.######", // 1fb4c
    b"##.##.##.###", b"...#\\.######", b"#########\\##", b"#########.\\#", // 1fb50
    b"######.##.##", b"######.##..#", b"###.##.##.##", b"##.#........", // 1fb54
    b"####/.......", b"##.#/.#.....", b"#####.#.....", b"##.##.#..#..", // 1fb58
    b"#######/....", b"###########/", b"##########/.", b"########.##.", // 1fb5c
    b"########.#..", b"#####.##.##.", b".##..#......", b"###.\\#......", // 1fb60
    b".##.\\#..#...", b"###.##..#...", b".##.##..#..#", b"######.\\#...", // 1fb64
];

/// The corners of the smooth mosaic `picture` as Ghostty's SmoothMosaic
/// reads them, in order down the left side, along the bottom, up the right
/// and back along the top, and how many there are.
fn smooth_mosaic(picture: &[u8; 12], w: u32, h: u32) -> ([Pt; 10], usize) {
    let on = |row: usize, col: usize| {
        picture.get(row.saturating_mul(3).saturating_add(col)) == Some(&b'#')
    };
    let (wf, hf) = (f64::from(w), f64::from(h));
    let (upper, lower) = (hf / 3.0, hf * 2.0 / 3.0);
    // a point between two corners counts only where it bends the outline
    let candidates = [
        (on(0, 0), pt(0.0, 0.0)),
        (on(1, 0) && !(on(0, 0) && on(2, 0)), pt(0.0, upper)),
        (on(2, 0) && !(on(1, 0) && on(3, 0)), pt(0.0, lower)),
        (on(3, 0), pt(0.0, hf)),
        (on(3, 1) && !(on(3, 0) && on(3, 2)), pt(wf / 2.0, hf)),
        (on(3, 2), pt(wf, hf)),
        (on(2, 2) && !(on(3, 2) && on(1, 2)), pt(wf, lower)),
        (on(1, 2) && !(on(2, 2) && on(0, 2)), pt(wf, upper)),
        (on(0, 2), pt(wf, 0.0)),
        (on(0, 1) && !(on(0, 2) && on(0, 0)), pt(wf / 2.0, 0.0)),
    ];
    let mut corners = [pt(0.0, 0.0); 10];
    let mut len = 0;
    for (keep, p) in candidates {
        if keep {
            if let Some(c) = corners.get_mut(len) {
                *c = p;
            }
            len = len.saturating_add(1);
        }
    }
    (corners, len)
}

fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

/// edgeTriangle(): the triangle on edge `edge` (left, upper, right,
/// lower) with its apex at the cell's centre, or, `inverse`, the rest of
/// the cell.
fn edge_triangle(buf: &mut Glyph, edge: u32, inverse: bool) {
    let corners = triangle(buf, edge);
    paint(buf, &[poly(&corners)], inverse, 0xff);
}

fn edge_triangles(buf: &mut Glyph, edges: &[u32; 2]) {
    let [a, b] = edges.map(|e| triangle(buf, e));
    paint(buf, &[poly(&a), poly(&b)], false, 0xff);
}

fn triangle(buf: &Glyph, edge: u32) -> [Pt; 3] {
    let (w, h) = (f64::from(buf.width()), f64::from(buf.height()));
    let c = pt(w / 2.0, h / 2.0);
    match edge {
        0 => [pt(0.0, 0.0), pt(0.0, h), c],
        1 => [pt(w, 0.0), pt(0.0, 0.0), c],
        2 => [pt(w, h), pt(w, 0.0), c],
        _ => [pt(0.0, h), pt(w, h), c],
    }
}

/// checkerboardFill(): four squares across, and down the even number of
/// rows nearest to square ones, at least two; `odd` picks the squares
/// whose column and row add up odd.
fn checkerboard(buf: &mut Glyph, odd: u32) {
    let (w, h) = (buf.width(), buf.height());
    // round(2h / w), at least 1
    let half_rows = h
        .saturating_mul(4)
        .saturating_add(w)
        .checked_div(w.saturating_mul(2))
        .unwrap_or(0)
        .max(1);
    let rows = half_rows.saturating_mul(2);
    for row in 0..rows {
        for col in 0..4u32 {
            if col.saturating_add(row) % 2 != odd {
                continue;
            }
            buf.fill(
                frac_max(w, col, 4),
                frac_max(h, row, rows),
                frac_max(w, col.saturating_add(1), 4),
                frac_max(h, row.saturating_add(1), rows),
                0xff,
            );
        }
    }
}

/// draw1FB98() and draw1FB99(): light lines parallel to the cell's
/// diagonal from the upper left (or, `rising`, from the upper right), a
/// whole share of the width apart, as many across as leaves a line's width
/// clear between them, and at least one.
fn hatch(w: u32, h: u32, t: u32, rising: bool) -> Region {
    // across the lines they are w h / (n sqrt(w^2 + h^2)) apart; at least
    // 2t when (w h)^2 >= (2 t n)^2 (w^2 + h^2)
    let (w64, h64, t2) = (u64::from(w), u64::from(h), u64::from(t).saturating_mul(2));
    let wh2 = w64.saturating_mul(h64).saturating_pow(2);
    let diag2 = w64.saturating_pow(2).saturating_add(h64.saturating_pow(2));
    let mut n = w.checked_div(t.saturating_mul(2)).unwrap_or(0);
    while n > 1
        && t2
            .saturating_mul(u64::from(n))
            .saturating_pow(2)
            .saturating_mul(diag2)
            > wh2
    {
        n = n.saturating_sub(1);
    }
    let n = n.max(1);
    let (wf, hf) = (f64::from(w), f64::from(h));
    Region::Hatch {
        space: wf / f64::from(n),
        slope: wf / hf * if rising { 1.0 } else { -1.0 },
        h2: hf * hf,
        n2: wf * wf + hf * hf,
        r: f64::from(t) / 2.0,
    }
}

const TL: u8 = 1;
const TR: u8 = 2;
const BL: u8 = 4;
const BR: u8 = 8;

/// U+1FBA0-U+1FBAE: the light diagonals between the centres of the edges,
/// upper to left, upper to right, lower to left and lower to right.
#[rustfmt::skip]
const CORNER_DIAGONALS: [u8; 15] = [
    TL, TR, BL, BR,
    TL | BL, TR | BR, BL | BR, TL | TR,
    TL | BR, TR | BL,
    TR | BL | BR, TL | BL | BR, TL | TR | BR, TL | TR | BL,
    TL | TR | BL | BR,
];

/// cornerDiagonalLines(): the diagonals `quads` names, or, `negative`,
/// the rest of the cell.
fn corner_diagonals(buf: &mut Glyph, quads: u8, t: u32, negative: bool) {
    let p = Points::new(buf, t);
    let lines: Vec<Region> = [
        (TL, p.uc, p.ml),
        (TR, p.uc, p.mr),
        (BL, p.lc, p.ml),
        (BR, p.lc, p.mr),
    ]
    .into_iter()
    .filter(|&(q, _, _)| quads & q != 0)
    .map(|(_, a, b)| p.line(a, b, true, true))
    .collect();
    paint(buf, &lines, negative, 0xff);
}

// The points of the light diagonals' names.
const UL: u8 = 0;
const UC: u8 = 1;
const UR: u8 = 2;
const ML: u8 = 3;
const MC: u8 = 4;
const MR: u8 = 5;
const LL: u8 = 6;
const LC: u8 = 7;
const LR: u8 = 8;

/// U+1FBD0-U+1FBDF: the light diagonals through the cell, each a path of
/// two or three points.
#[rustfmt::skip]
const CELL_DIAGONALS: [&[u8]; 16] = [
    &[MR, LL], &[UR, ML], &[UL, MR], &[ML, LR],
    &[UL, LC], &[UC, LR], &[UR, LC], &[UC, LL],
    &[UL, MC, UR], &[UR, MC, LR], &[LL, MC, LR], &[UL, MC, LL],
    &[UL, LC, UR], &[UR, ML, LR], &[LL, UC, LR], &[UL, MR, LL],
];

/// cellDiagonal(): the light diagonals along `path`.
fn cell_diagonals(buf: &mut Glyph, path: &[u8], t: u32) {
    let p = Points::new(buf, t);
    let lines: Vec<Region> = path
        .windows(2)
        .filter_map(|pair| match *pair {
            [a, b] => Some(p.line(p.named(a), p.named(b), a != MC, b != MC)),
            _ => None,
        })
        .collect();
    paint(buf, &lines, false, 0xff);
}

/// The points the light diagonals' names give, the centres where box
/// drawing's light lines run.
struct Points {
    w: f64,
    h: f64,
    mx: f64,
    my: f64,
    t: f64,
    uc: Pt,
    ml: Pt,
    mr: Pt,
    lc: Pt,
}

impl Points {
    fn new(buf: &Glyph, t: u32) -> Points {
        let (w, h) = (f64::from(buf.width()), f64::from(buf.height()));
        let (mx, my) = (centre(buf.width(), t), centre(buf.height(), t));
        Points {
            w,
            h,
            mx,
            my,
            t: f64::from(t),
            uc: pt(mx, 0.0),
            ml: pt(0.0, my),
            mr: pt(w, my),
            lc: pt(mx, h),
        }
    }

    fn named(&self, name: u8) -> Pt {
        let x = match name % 3 {
            0 => 0.0,
            1 => self.mx,
            _ => self.w,
        };
        let y = match name / 3 {
            0 => 0.0,
            1 => self.my,
            _ => self.h,
        };
        pt(x, y)
    }

    /// The light line from `a` to `b`, run on far past the ends on the
    /// cell's edge (`a_out`, `b_out`), so the edge cuts it.
    fn line(&self, a: Pt, b: Pt, a_out: bool, b_out: bool) -> Region {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let long = dx.abs().max(dy.abs());
        // each moved end goes at least the cell's width and height out
        let k = if long > 0.0 {
            (self.w + self.h + self.t) / long
        } else {
            0.0
        };
        let a = if a_out {
            pt(a.x - dx * k, a.y - dy * k)
        } else {
            a
        };
        let b = if b_out {
            pt(b.x + dx * k, b.y + dy * k)
        } else {
            b
        };
        Region::Line(a, b, self.t / 2.0)
    }
}

/// circle(): U+1FBE0-U+1FBE3 white and U+1FBE8-U+1FBEB black half circles
/// centred on the top, right, bottom and left edges, U+1FBEC-U+1FBEF black
/// quarter circles centred on the top right, bottom left, bottom right and
/// top left corners, all as wide as the cell's shorter side.
fn circle(buf: &mut Glyph, cp: u32, t: u32) {
    let (w, h) = (f64::from(buf.width()), f64::from(buf.height()));
    let at = match cp & 0xf {
        0x0 | 0x8 => pt(w / 2.0, 0.0),
        0x1 | 0x9 => pt(w, h / 2.0),
        0x2 | 0xa => pt(w / 2.0, h),
        0x3 | 0xb => pt(0.0, h / 2.0),
        0xc => pt(w, 0.0),
        0xd => pt(0.0, h),
        0xe => pt(w, h),
        _ => pt(0.0, 0.0),
    };
    let r = w.min(h) / 2.0;
    let inner = if cp < 0x1fbe8 {
        (r - f64::from(t)).max(0.0)
    } else {
        0.0
    };
    paint(buf, &[Region::Ring(at, inner, r)], false, 0xff);
}

/// A shape drawn by sampling: convex, but for the ring and the hatching.
enum Region {
    /// A convex polygon: the points no edge has outside it.
    Poly(Vec<Edge>),
    /// Within the radius of the segment.
    Line(Pt, Pt, f64),
    /// From the inner radius to the outer one round the centre.
    Ring(Pt, f64, f64),
    /// Within `r` of the lines x + slope y = k space for every whole k. A
    /// point's distance across the lines is its offset from the nearest
    /// along x, times sqrt(h2 / n2); the squares are compared, unscaled.
    Hatch {
        space: f64,
        slope: f64,
        h2: f64,
        n2: f64,
        r: f64,
    },
}

/// A polygon's edge from `a` as the linear function dx (y - a.y) -
/// dy (x - a.x), positive outside the polygon.
#[derive(Clone, Copy)]
struct Edge {
    a: Pt,
    dx: f64,
    dy: f64,
}

impl Edge {
    fn at(&self, p: Pt) -> f64 {
        self.dx * (p.y - self.a.y) - self.dy * (p.x - self.a.x)
    }

    /// The least and greatest values over the pixel with its top left at
    /// `p`, found at two of its corners.
    fn span(&self, p: Pt) -> (f64, f64) {
        let f = self.at(p);
        (
            f + self.dx.min(0.0) - self.dy.max(0.0),
            f + self.dx.max(0.0) - self.dy.min(0.0),
        )
    }
}

/// The convex polygon with these corners, in either order round.
fn poly(corners: &[Pt]) -> Region {
    let pairs = || corners.iter().zip(corners.iter().cycle().skip(1));
    let area: f64 = pairs().map(|(a, b)| a.x * b.y - b.x * a.y).sum();
    let sign = if area > 0.0 { -1.0 } else { 1.0 };
    Region::Poly(
        pairs()
            .map(|(&a, &b)| Edge {
                a,
                dx: (b.x - a.x) * sign,
                dy: (b.y - a.y) * sign,
            })
            .collect(),
    )
}

/// Past half a pixel's diagonal: a pixel whose centre is this much farther
/// than a region's reach lies wholly outside it.
const HALF_DIAGONAL: f64 = 0.7072;

#[derive(Clone, Copy, PartialEq)]
enum Class {
    /// The region covers the pixel whole.
    In,
    /// The region misses the pixel.
    Out,
    /// The region's edge crosses the pixel.
    Edge,
}

fn point_dist2(p: Pt, q: Pt) -> f64 {
    (p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y)
}

impl Region {
    fn contains(&self, p: Pt) -> bool {
        match *self {
            Region::Poly(ref edges) => edges.iter().all(|e| e.at(p) <= 0.0),
            Region::Line(a, b, r) => super::dist2(p, a, b) <= r * r,
            Region::Ring(o, r0, r1) => {
                let d = point_dist2(p, o);
                d >= r0 * r0 && d <= r1 * r1
            }
            Region::Hatch {
                space,
                slope,
                h2,
                n2,
                r,
            } => {
                let u = p.x + slope * p.y;
                let off = u - nearest_line(u, space);
                off * off * h2 <= r * r * n2
            }
        }
    }

    /// Whether the region covers the pixel with its top left at `p` whole,
    /// misses it, or holds only some of it. A region that is unsure says
    /// the pixel is an edge, and the pixel is sampled.
    fn class(&self, p: Pt) -> Class {
        let corners = [
            p,
            pt(p.x + 1.0, p.y),
            pt(p.x, p.y + 1.0),
            pt(p.x + 1.0, p.y + 1.0),
        ];
        let mid = pt(p.x + 0.5, p.y + 0.5);
        let far = |reach: f64| (reach + HALF_DIAGONAL) * (reach + HALF_DIAGONAL);
        let (out, whole) = match *self {
            // wholly past one edge, or within them all; a convex region
            // holds every point between its corners
            Region::Poly(ref edges) => {
                let mut whole = true;
                let mut out = false;
                for e in edges {
                    let (lo, hi) = e.span(p);
                    out |= lo > 0.0;
                    whole &= hi <= 0.0;
                }
                (out, whole)
            }
            Region::Line(a, b, r) => (
                super::dist2(mid, a, b) > far(r),
                corners.iter().all(|&c| super::dist2(c, a, b) <= r * r),
            ),
            Region::Ring(o, r0, r1) => {
                let d = point_dist2(mid, o);
                (
                    d > far(r1) || corners.iter().all(|&c| point_dist2(c, o) < r0 * r0),
                    d >= far(r0) && corners.iter().all(|&c| point_dist2(c, o) <= r1 * r1),
                )
            }
            // against the line nearest the pixel's middle
            Region::Hatch {
                space,
                slope,
                h2,
                n2,
                r,
            } => {
                let u = mid.x + slope * mid.y;
                let k = nearest_line(u, space);
                (
                    (u - k) * (u - k) * h2 > far(r) * n2,
                    corners.iter().all(|c| {
                        let off = c.x + slope * c.y - k;
                        off * off * h2 <= r * r * n2
                    }),
                )
            }
        };
        if out {
            Class::Out
        } else if whole {
            Class::In
        } else {
            Class::Edge
        }
    }
}

/// The hatching line nearest `u` along x: the multiple of `space` nearest
/// it. Rounding down is deterministic in IEEE 754, as the rest of the
/// arithmetic is.
fn nearest_line(u: f64, space: f64) -> f64 {
    (u / space + 0.5).floor() * space
}

/// Sets each pixel to the larger of what it holds and `value` scaled by the
/// share of its samples inside any of `regions` (outside all of them,
/// `inverse`). A pixel the regions cover whole or all miss is not sampled.
fn paint(buf: &mut Glyph, regions: &[Region], inverse: bool, value: u8) {
    let (w, h) = (buf.width(), buf.height());
    let ss = f64::from(SS);
    let all = SS.saturating_mul(SS);
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (f64::from(x), f64::from(y));
            let mut class = Class::Out;
            for r in regions {
                match r.class(pt(xf, yf)) {
                    Class::In => {
                        class = Class::In;
                        break;
                    }
                    Class::Edge => class = Class::Edge,
                    Class::Out => {}
                }
            }
            let hits = match class {
                Class::In => all,
                Class::Out => 0,
                Class::Edge => {
                    let mut hits = 0u32;
                    for sy in 0..SS {
                        for sx in 0..SS {
                            let s = pt(
                                xf + (f64::from(sx) + 0.5) / ss,
                                yf + (f64::from(sy) + 0.5) / ss,
                            );
                            if regions.iter().any(|r| r.contains(s)) {
                                hits = hits.saturating_add(1);
                            }
                        }
                    }
                    hits
                }
            };
            let hits = if inverse {
                all.saturating_sub(hits)
            } else {
                hits
            };
            let v = hits
                .saturating_mul(u32::from(value))
                .checked_div(all)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(u8::MAX);
            if let Some(px) = buf.coverage_mut(x, y) {
                *px = (*px).max(v);
            }
        }
    }
}
