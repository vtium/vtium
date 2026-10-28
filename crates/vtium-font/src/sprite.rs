//! Box Drawing (U+2500-U+257F), Block Elements (U+2580-U+259F), Braille
//! Patterns (U+2800-U+28FF), the sextants (U+1FB00-U+1FB3B) and the octants
//! (U+1CD00-U+1CDE5) drawn as sprites exactly the size of the cell, in place
//! of a font's glyphs; the rest of Symbols for Legacy Computing's block
//! shapes are in `legacy`. A font draws these between its own ascent and
//! descent, so where the cell is taller (line spacing), stacked blocks and
//! vertical lines leave a gap between rows, and Braille dots and mosaic
//! blocks miss the cell's grid.
//!
//! Ported from the C reference's `font_box.c` (aaronsb/kmscon#46, #76):
//!
//! Copyright (c) 2026 the kmscon fork authors
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
//! The geometry is adapted from Ghostty's sprite font (MIT, Copyright (c)
//! 2024 Mitchell Hashimoto, Ghostty contributors), src/font/sprite/draw/:
//! - lines and junctions: box.zig linesChar(), each edge's line meeting the
//!   others at the centre, light lines the font's underline thickness and
//!   heavy ones twice that, double lines two light strokes a light stroke
//!   apart;
//! - dashes: box.zig dashHorizontal() and dashVertical(), which divide the
//!   cell so the dashes tile across cells;
//! - rounded corners: box.zig arc(), a cubic curve between the two half
//!   lines;
//! - diagonals: box.zig lightDiagonal*(), corner to corner, overshooting the
//!   corners slightly so neighbours join;
//! - blocks: block.zig blockShade() and quadrant(), with common.zig
//!   Fraction.min()/max() rounding, so two halves of an odd cell overlap by
//!   a pixel rather than leave one empty;
//! - shades: block.zig fullBlockShade(), the whole cell at 25%, 50% and 75%
//!   coverage (common.zig Shade), which the renderer blends like any glyph;
//! - Braille: braille.zig draw2800_28FF(), square dots on a 2x4 grid whose
//!   dot size, spacing and margins are shared out from the cell's size;
//! - sextants: symbols_for_legacy_computing.zig draw1FB00_1FB3B(), a 2x3
//!   grid of blocks with common.zig fill(), the pattern counted from the
//!   codepoint;
//! - octants: symbols_for_legacy_computing_supplement.zig draw1CD00_1CDE5(),
//!   a 2x4 grid of blocks, the pattern looked up as octants.txt does.
//!
//! Curves and diagonals are drawn by supersampling instead of Ghostty's
//! path rasteriser. They use only IEEE 754 addition, subtraction,
//! multiplication, division and comparison on `f64`, which every host
//! rounds the same way and Rust never fuses, so a sprite is the same bytes
//! on every host.

mod legacy;
pub(crate) mod spans;

use core::fmt;

use crate::bounds::CELL_SIDE_MAX;
use crate::glyph::Glyph;

/// A range of characters vtium draws as sprites, each of which a font may
/// draw instead (vtium/vtium#85).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Range {
    /// U+2500-U+257F.
    BoxDrawing,
    /// U+2580-U+259F.
    BlockElements,
    /// U+2800-U+28FF.
    Braille,
    /// U+1FB00-U+1FB3B.
    Sextants,
    /// U+1CD00-U+1CDE5.
    Octants,
    /// The rest of Symbols for Legacy Computing's block shapes and its
    /// supplement's one quarter blocks: U+1CEA0-U+1CEAF, U+1FB3C-U+1FB92,
    /// U+1FB94-U+1FBAF, U+1FBBD-U+1FBBF and U+1FBCE-U+1FBEF.
    LegacyComputing,
}

impl Range {
    pub const ALL: [Range; 6] = [
        Range::BoxDrawing,
        Range::BlockElements,
        Range::Braille,
        Range::Sextants,
        Range::Octants,
        Range::LegacyComputing,
    ];

    /// The range `c` belongs to, or `None` for a character vtium does not
    /// draw as a sprite.
    pub fn of(c: char) -> Option<Range> {
        let cp = u32::from(c);
        Range::ALL.into_iter().find(|r| {
            r.spans()
                .iter()
                .any(|&(first, last)| (first..=last).contains(&cp))
        })
    }

    /// Every character of the range, in order.
    pub fn chars(self) -> impl Iterator<Item = char> {
        self.spans()
            .iter()
            .flat_map(|&(first, last)| first..=last)
            .filter_map(char::from_u32)
    }

    /// The first and last codepoints of each run of the range.
    fn spans(self) -> &'static [(u32, u32)] {
        match self {
            Range::BoxDrawing => spans::BOX_DRAWING,
            Range::BlockElements => spans::BLOCK_ELEMENTS,
            Range::Braille => spans::BRAILLE,
            Range::Sextants => spans::SEXTANTS,
            Range::Octants => spans::OCTANTS,
            Range::LegacyComputing => spans::LEGACY_COMPUTING,
        }
    }
}

/// The range's name, as a warning about it reads.
impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Range::BoxDrawing => "box drawing",
            Range::BlockElements => "block elements",
            Range::Braille => "Braille",
            Range::Sextants => "sextants",
            Range::Octants => "octants",
            Range::LegacyComputing => "Legacy Computing",
        })
    }
}

/// Whether vtium can draw `c` as a sprite; a [`Font`](crate::Font)'s
/// policy decides whether it does.
pub fn has(c: char) -> bool {
    Range::of(c).is_some()
}

/// A shade, U+2591 to U+2593, as a `width` by `height` sprite for a 1-bit
/// display (vtium/vtium#135): an ordered 2x2 pattern a quarter, half or
/// three quarters lit, its first pixel in each 2x2 the top left, so cells
/// whose sides are even join without a seam. [`draw`]'s shades are even
/// coverage, which a 1-bit display cuts at half to empty or solid. `None`
/// for any other character or a size [`Glyph::mask`] refuses.
pub fn shade_one_bit(c: char, width: u32, height: u32) -> Option<Glyph> {
    // the 2x2 ordered dither's thresholds, top left, top right, bottom
    // left, bottom right
    const ORDER: [u32; 4] = [0, 2, 3, 1];
    let lit = match c {
        '\u{2591}' => 1,
        '\u{2592}' => 2,
        '\u{2593}' => 3,
        _ => return None,
    };
    let mut buf = Glyph::mask(width, height)?;
    for y in 0..height {
        for x in 0..width {
            let i = usize::try_from((y & 1) << 1 | x & 1).unwrap_or(0);
            if ORDER.get(i).is_some_and(|&t| t < lit) {
                buf.fill(x, y, x.saturating_add(1), y.saturating_add(1), 0xff);
            }
        }
    }
    Some(buf)
}

/// `c` as a `width` by `height` sprite with light lines `thickness` pixels
/// wide (heavy lines twice that; 0 draws 1), or `None` for a character it
/// does not draw, a side of 0, a size past [`Glyph::mask`]'s bound, or a
/// thickness past [`CELL_SIDE_MAX`].
pub fn draw(c: char, width: u32, height: u32, thickness: u32) -> Option<Glyph> {
    let range = Range::of(c)?;
    if thickness > CELL_SIDE_MAX {
        return None;
    }
    let mut buf = Glyph::mask(width, height)?;
    let t = thickness.max(1);
    let cp = u32::from(c);
    match range {
        Range::LegacyComputing => legacy::draw(&mut buf, cp, t),
        Range::Sextants => {
            // the 64 patterns of a 2x3 grid counted in binary, less empty,
            // full and the two halves (21 and 42)
            let n = cp.saturating_sub(0x1fb00);
            mosaic(&mut buf, n.saturating_add(n / 20).saturating_add(1), 3);
        }
        Range::Octants => {
            let i = usize::try_from(cp.saturating_sub(0x1cd00)).ok()?;
            mosaic(&mut buf, u32::from(*OCTANTS.get(i)?), 4);
        }
        Range::Braille => braille(&mut buf, cp),
        Range::BlockElements => block_element(&mut buf, cp),
        Range::BoxDrawing => box_drawing(&mut buf, cp, t),
    }
    Some(buf)
}

// Line styles, two bits per edge.
const N: u8 = 0; // none
const L: u8 = 1; // light
const H: u8 = 2; // heavy
const D: u8 = 3; // double

const fn lines(up: u8, right: u8, down: u8, left: u8) -> u8 {
    up | right << 2 | down << 4 | left << 6
}

/// U+2500-U+257F made of straight lines from the edges to the centre; 0 for
/// the dashes, curves and diagonals, which are drawn otherwise.
#[rustfmt::skip]
const BOX_LINES: [u8; 0x80] = [
    lines(N, L, N, L), lines(N, H, N, H), lines(L, N, L, N), lines(H, N, H, N), // 2500
    0, 0, 0, 0, 0, 0, 0, 0,                                                      // 2504
    lines(N, L, L, N), lines(N, H, L, N), lines(N, L, H, N), lines(N, H, H, N), // 250c
    lines(N, N, L, L), lines(N, N, L, H), lines(N, N, H, L), lines(N, N, H, H), // 2510
    lines(L, L, N, N), lines(L, H, N, N), lines(H, L, N, N), lines(H, H, N, N), // 2514
    lines(L, N, N, L), lines(L, N, N, H), lines(H, N, N, L), lines(H, N, N, H), // 2518
    lines(L, L, L, N), lines(L, H, L, N), lines(H, L, L, N), lines(L, L, H, N), // 251c
    lines(H, L, H, N), lines(H, H, L, N), lines(L, H, H, N), lines(H, H, H, N), // 2520
    lines(L, N, L, L), lines(L, N, L, H), lines(H, N, L, L), lines(L, N, H, L), // 2524
    lines(H, N, H, L), lines(H, N, L, H), lines(L, N, H, H), lines(H, N, H, H), // 2528
    lines(N, L, L, L), lines(N, L, L, H), lines(N, H, L, L), lines(N, H, L, H), // 252c
    lines(N, L, H, L), lines(N, L, H, H), lines(N, H, H, L), lines(N, H, H, H), // 2530
    lines(L, L, N, L), lines(L, L, N, H), lines(L, H, N, L), lines(L, H, N, H), // 2534
    lines(H, L, N, L), lines(H, L, N, H), lines(H, H, N, L), lines(H, H, N, H), // 2538
    lines(L, L, L, L), lines(L, L, L, H), lines(L, H, L, L), lines(L, H, L, H), // 253c
    lines(H, L, L, L), lines(L, L, H, L), lines(H, L, H, L), lines(H, L, L, H), // 2540
    lines(H, H, L, L), lines(L, L, H, H), lines(L, H, H, L), lines(H, H, L, H), // 2544
    lines(L, H, H, H), lines(H, L, H, H), lines(H, H, H, L), lines(H, H, H, H), // 2548
    0, 0, 0, 0,                                                                  // 254c
    lines(N, D, N, D), lines(D, N, D, N), lines(N, D, L, N), lines(N, L, D, N), // 2550
    lines(N, D, D, N), lines(N, N, L, D), lines(N, N, D, L), lines(N, N, D, D), // 2554
    lines(L, D, N, N), lines(D, L, N, N), lines(D, D, N, N), lines(L, N, N, D), // 2558
    lines(D, N, N, L), lines(D, N, N, D), lines(L, D, L, N), lines(D, L, D, N), // 255c
    lines(D, D, D, N), lines(L, N, L, D), lines(D, N, D, L), lines(D, N, D, D), // 2560
    lines(N, D, L, D), lines(N, L, D, L), lines(N, D, D, D), lines(L, D, N, D), // 2564
    lines(D, L, N, L), lines(D, D, N, D), lines(L, D, L, D), lines(D, L, D, L), // 2568
    lines(D, D, D, D), 0, 0, 0,                                                  // 256c
    0, 0, 0, 0,                                                                  // 2570
    lines(N, N, N, L), lines(L, N, N, N), lines(N, L, N, N), lines(N, N, L, N), // 2574
    lines(N, N, N, H), lines(H, N, N, N), lines(N, H, N, N), lines(N, N, H, N), // 2578
    lines(N, H, N, L), lines(L, N, H, N), lines(N, L, N, H), lines(H, N, L, N), // 257c
];

/// `num`/`den` of `size`, rounded, as the far edge of a block
/// (Fraction.max).
fn frac_max(size: u32, num: u32, den: u32) -> u32 {
    num.saturating_mul(size)
        .saturating_mul(2)
        .saturating_add(den)
        .checked_div(den.saturating_mul(2))
        .unwrap_or(0)
}

/// The same as a near edge (Fraction.min): measured from the far side, so a
/// block from 0 to a fraction and one from it to the end together cover the
/// cell.
fn frac_min(size: u32, num: u32, den: u32) -> u32 {
    size.saturating_sub(frac_max(size, den.saturating_sub(num), den))
}

/// linesChar()
fn draw_lines(buf: &mut Glyph, spec: u8, t: u32) {
    let (w, h) = (buf.width(), buf.height());
    let (up, right, down, left) = (spec & 3, spec >> 2 & 3, spec >> 4 & 3, spec >> 6);
    let ht = t.saturating_mul(2);

    let h_light_top = h.saturating_sub(t) / 2;
    let h_light_bottom = h_light_top.saturating_add(t);
    let h_heavy_top = h.saturating_sub(ht) / 2;
    let h_heavy_bottom = h_heavy_top.saturating_add(ht);
    let h_double_top = h_light_top.saturating_sub(t);
    let h_double_bottom = h_light_bottom.saturating_add(t);
    let v_light_left = w.saturating_sub(t) / 2;
    let v_light_right = v_light_left.saturating_add(t);
    let v_heavy_left = w.saturating_sub(ht) / 2;
    let v_heavy_right = v_heavy_left.saturating_add(ht);
    let v_double_left = v_light_left.saturating_sub(t);
    let v_double_right = v_light_right.saturating_add(t);

    // how far each line reaches into the centre, so that it meets the
    // crossing lines without a notch or a stub
    let up_bottom = if left == H || right == H {
        h_heavy_bottom
    } else if left != right || down == up {
        if left == D || right == D {
            h_double_bottom
        } else {
            h_light_bottom
        }
    } else if left == N && right == N {
        h_light_bottom
    } else {
        h_light_top
    };
    let down_top = if left == H || right == H {
        h_heavy_top
    } else if left != right || up == down {
        if left == D || right == D {
            h_double_top
        } else {
            h_light_top
        }
    } else if left == N && right == N {
        h_light_top
    } else {
        h_light_bottom
    };
    let left_right = if up == H || down == H {
        v_heavy_right
    } else if up != down || left == right {
        if up == D || down == D {
            v_double_right
        } else {
            v_light_right
        }
    } else if up == N && down == N {
        v_light_right
    } else {
        v_light_left
    };
    let right_left = if up == H || down == H {
        v_heavy_left
    } else if up != down || right == left {
        if up == D || down == D {
            v_double_left
        } else {
            v_light_left
        }
    } else if up == N && down == N {
        v_light_left
    } else {
        v_light_right
    };

    match up {
        L => buf.fill(v_light_left, 0, v_light_right, up_bottom, 0xff),
        H => buf.fill(v_heavy_left, 0, v_heavy_right, up_bottom, 0xff),
        D => {
            let a = if left == D { h_light_top } else { up_bottom };
            let b = if right == D { h_light_top } else { up_bottom };
            buf.fill(v_double_left, 0, v_light_left, a, 0xff);
            buf.fill(v_light_right, 0, v_double_right, b, 0xff);
        }
        _ => {}
    }
    match right {
        L => buf.fill(right_left, h_light_top, w, h_light_bottom, 0xff),
        H => buf.fill(right_left, h_heavy_top, w, h_heavy_bottom, 0xff),
        D => {
            let a = if up == D { v_light_right } else { right_left };
            let b = if down == D { v_light_right } else { right_left };
            buf.fill(a, h_double_top, w, h_light_top, 0xff);
            buf.fill(b, h_light_bottom, w, h_double_bottom, 0xff);
        }
        _ => {}
    }
    match down {
        L => buf.fill(v_light_left, down_top, v_light_right, h, 0xff),
        H => buf.fill(v_heavy_left, down_top, v_heavy_right, h, 0xff),
        D => {
            let a = if left == D { h_light_bottom } else { down_top };
            let b = if right == D { h_light_bottom } else { down_top };
            buf.fill(v_double_left, a, v_light_left, h, 0xff);
            buf.fill(v_light_right, b, v_double_right, h, 0xff);
        }
        _ => {}
    }
    match left {
        L => buf.fill(0, h_light_top, left_right, h_light_bottom, 0xff),
        H => buf.fill(0, h_heavy_top, left_right, h_heavy_bottom, 0xff),
        D => {
            let a = if up == D { v_light_left } else { left_right };
            let b = if down == D { v_light_left } else { left_right };
            buf.fill(0, h_double_top, a, h_light_top, 0xff);
            buf.fill(0, h_light_bottom, b, h_double_bottom, 0xff);
        }
        _ => {}
    }
}

/// dashHorizontal(): `count` dashes `thick` high with gaps of up to `gap`,
/// half a gap at each end so the dashes tile; leftover pixels widen the
/// dashes.
fn dash_h(buf: &mut Glyph, count: u32, thick: u32, gap: u32, t: u32) {
    let (w, h) = (buf.width(), buf.height());
    if w < count.saturating_mul(2) {
        let y = h.saturating_sub(t) / 2;
        buf.fill(0, y, w, y.saturating_add(t), 0xff);
        return;
    }
    let gap = gap.min(w.checked_div(count.saturating_mul(2)).unwrap_or(0));
    let left = w.saturating_sub(count.saturating_mul(gap));
    let dash = left.checked_div(count).unwrap_or(0);
    let mut extra = left.checked_rem(count).unwrap_or(0);
    let y = h.saturating_sub(thick) / 2;
    let mut x = gap / 2;
    for _ in 0..count {
        let x1 = x.saturating_add(dash).saturating_add(u32::from(extra > 0));
        extra = extra.saturating_sub(1);
        buf.fill(x, y, x1, y.saturating_add(thick), 0xff);
        x = x1.saturating_add(gap);
    }
}

/// dashVertical(): the same down the cell, with the whole gap at the bottom.
fn dash_v(buf: &mut Glyph, count: u32, thick: u32, gap: u32, t: u32) {
    let (w, h) = (buf.width(), buf.height());
    if h < count.saturating_mul(2) {
        let x = w.saturating_sub(t) / 2;
        buf.fill(x, 0, x.saturating_add(t), h, 0xff);
        return;
    }
    let gap = gap.min(h.checked_div(count.saturating_mul(2)).unwrap_or(0));
    let left = h.saturating_sub(count.saturating_mul(gap));
    let dash = left.checked_div(count).unwrap_or(0);
    let mut extra = left.checked_rem(count).unwrap_or(0);
    let x = w.saturating_sub(thick) / 2;
    let mut y = 0u32;
    for _ in 0..count {
        let y1 = y.saturating_add(dash).saturating_add(u32::from(extra > 0));
        extra = extra.saturating_sub(1);
        buf.fill(x, y, x.saturating_add(thick), y1, 0xff);
        y = y1.saturating_add(gap);
    }
}

#[derive(Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
}

/// The squared distance from `p` to the segment `a`-`b`.
fn dist2(p: Pt, a: Pt, b: Pt) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    let u = if len2 > 0.0 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2
    } else {
        0.0
    };
    let u = u.clamp(0.0, 1.0);
    let ex = a.x + u * dx - p.x;
    let ey = a.y + u * dy - p.y;
    ex * ex + ey * ey
}

/// Samples per pixel side for curves and diagonals.
const SS: u32 = 8;

/// `v` rounded down and held to `0..=max`; NaN is 0.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "v is held inside 0..=max, a u32, before the cast"
)]
fn clamp_floor(v: f64, max: u32) -> u32 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= f64::from(max) {
        max
    } else {
        v as u32
    }
}

/// Strokes the polyline `p` `thick` wide, keeping the larger of what the
/// bitmap holds and each pixel's coverage. The ends lie outside the cell,
/// so their caps are cut off. Only pixels within two pixels of the line's
/// reach are sampled; the rest would get no samples.
fn stroke(buf: &mut Glyph, p: &[Pt], thick: f64) {
    let r2 = thick * thick / 4.0;
    let r = thick / 2.0 + 2.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in p {
        x0 = x0.min(q.x);
        y0 = y0.min(q.y);
        x1 = x1.max(q.x);
        y1 = y1.max(q.y);
    }
    let (w, h) = (buf.width(), buf.height());
    let xs = clamp_floor(x0 - r, w)..clamp_floor(x1 + r + 1.0, w);
    let ys = clamp_floor(y0 - r, h)..clamp_floor(y1 + r + 1.0, h);
    let ss = f64::from(SS);
    for y in ys {
        for x in xs.clone() {
            let mut hits = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let s = Pt {
                        x: f64::from(x) + (f64::from(sx) + 0.5) / ss,
                        y: f64::from(y) + (f64::from(sy) + 0.5) / ss,
                    };
                    if p.windows(2).any(|seg| match seg {
                        [a, b] => dist2(s, *a, *b) <= r2,
                        _ => false,
                    }) {
                        hits = hits.saturating_add(1);
                    }
                }
            }
            let v = u8::try_from(hits.saturating_mul(255) / (SS * SS)).unwrap_or(u8::MAX);
            if let Some(px) = buf.coverage_mut(x, y) {
                *px = (*px).max(v);
            }
        }
    }
}

/// The light lines' centre, where curves and diagonals meet them.
fn centre(size: u32, t: u32) -> f64 {
    f64::from(size.saturating_sub(t) / 2) + f64::from(t) / 2.0
}

const ARC_STEPS: u32 = 16;

/// arc(): a light line from the vertical edge `dy` (1 bottom, -1 top) up to
/// a radius from the centre, a cubic curve with its control points a
/// quarter radius from the centre, and on to the horizontal edge `dx` (1
/// right, -1 left).
fn arc(buf: &mut Glyph, dx: f64, dy: f64, t: u32) {
    let (w, h) = (buf.width(), buf.height());
    let (cx, cy) = (centre(w, t), centre(h, t));
    let r = f64::from(w.min(h)) / 2.0;
    let s = 0.25;
    let c = [
        Pt {
            x: cx,
            y: cy + dy * r,
        },
        Pt {
            x: cx,
            y: cy + dy * s * r,
        },
        Pt {
            x: cx + dx * s * r,
            y: cy,
        },
        Pt {
            x: cx + dx * r,
            y: cy,
        },
    ];
    let tf = f64::from(t);
    let mut p = Vec::with_capacity(19);
    p.push(Pt {
        x: cx,
        y: if dy > 0.0 { f64::from(h) + tf } else { -tf },
    });
    for i in 0..=ARC_STEPS {
        let u = f64::from(i) / f64::from(ARC_STEPS);
        let v = 1.0 - u;
        let (k0, k1, k2, k3) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
        p.push(Pt {
            x: k0 * c[0].x + k1 * c[1].x + k2 * c[2].x + k3 * c[3].x,
            y: k0 * c[0].y + k1 * c[1].y + k2 * c[2].y + k3 * c[3].y,
        });
    }
    p.push(Pt {
        x: if dx > 0.0 { f64::from(w) + tf } else { -tf },
        y: cy,
    });
    stroke(buf, &p, tf);
}

/// lightDiagonal*(): from the top corner `right` names to the opposite
/// bottom corner, overshooting each by half a pixel along the slope, then
/// on by a line width so the cut-off caps leave the corners as Ghostty's
/// butt caps would.
fn diagonal(buf: &mut Glyph, right: bool, t: u32) {
    let (w, h) = (f64::from(buf.width()), f64::from(buf.height()));
    let ox = 0.5 * if w < h { w / h } else { 1.0 };
    let oy = 0.5 * if h < w { h / w } else { 1.0 };
    // the segment is at least max(w, h) long, so this moves each end out by
    // at least a line width
    let k = f64::from(t) / if w > h { w } else { h };
    let mut a = Pt {
        x: if right { w + ox } else { -ox },
        y: -oy,
    };
    let mut b = Pt {
        x: if right { -ox } else { w + ox },
        y: h + oy,
    };
    let ux = (b.x - a.x) * k;
    let uy = (b.y - a.y) * k;
    a.x -= ux;
    a.y -= uy;
    b.x += ux;
    b.y += uy;
    stroke(buf, &[a, b], f64::from(t));
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Top,
    Bottom,
}

/// block.zig blockShade(): a block `wn`/8 wide and `hn`/8 high.
fn block(buf: &mut Glyph, align: Align, wn: u32, hn: u32) {
    let (w, h) = (buf.width(), buf.height());
    let bw = frac_max(w, wn, 8);
    let bh = frac_max(h, hn, 8);
    let x = if align == Align::Right {
        w.saturating_sub(bw)
    } else {
        0
    };
    let y = if align == Align::Bottom {
        h.saturating_sub(bh)
    } else {
        0
    };
    buf.fill(x, y, x.saturating_add(bw), y.saturating_add(bh), 0xff);
}

const Q_TL: u32 = 1;
const Q_TR: u32 = 2;
const Q_BL: u32 = 4;
const Q_BR: u32 = 8;

/// quadrant(): `q` holds top left, top right, bottom left, bottom right as
/// bits 0 to 3.
fn quadrants(buf: &mut Glyph, q: u32) {
    let (w, h) = (buf.width(), buf.height());
    let (xl, xr) = (frac_max(w, 1, 2), frac_min(w, 1, 2));
    let (yt, yb) = (frac_max(h, 1, 2), frac_min(h, 1, 2));
    if q & Q_TL != 0 {
        buf.fill(0, 0, xl, yt, 0xff);
    }
    if q & Q_TR != 0 {
        buf.fill(xr, 0, w, yt, 0xff);
    }
    if q & Q_BL != 0 {
        buf.fill(0, yb, xl, h, 0xff);
    }
    if q & Q_BR != 0 {
        buf.fill(xr, yb, w, h, 0xff);
    }
}

fn block_element(buf: &mut Glyph, cp: u32) {
    let (w, h) = (buf.width(), buf.height());
    match cp {
        0x2580 => block(buf, Align::Top, 8, 4),
        0x2581..=0x2587 => block(buf, Align::Bottom, 8, cp.saturating_sub(0x2580)),
        0x2588 => buf.fill(0, 0, w, h, 0xff),
        0x2589..=0x258f => block(buf, Align::Left, 0x2590u32.saturating_sub(cp), 8),
        0x2590 => block(buf, Align::Right, 4, 8),
        // light, medium and dark shade
        0x2591 => buf.fill(0, 0, w, h, 0x40),
        0x2592 => buf.fill(0, 0, w, h, 0x80),
        0x2593 => buf.fill(0, 0, w, h, 0xc0),
        0x2594 => block(buf, Align::Top, 8, 1),
        0x2595 => block(buf, Align::Right, 1, 8),
        0x2596 => quadrants(buf, Q_BL),
        0x2597 => quadrants(buf, Q_BR),
        0x2598 => quadrants(buf, Q_TL),
        0x2599 => quadrants(buf, Q_TL | Q_BL | Q_BR),
        0x259a => quadrants(buf, Q_TL | Q_BR),
        0x259b => quadrants(buf, Q_TL | Q_TR | Q_BL),
        0x259c => quadrants(buf, Q_TL | Q_TR | Q_BR),
        0x259d => quadrants(buf, Q_TR),
        0x259e => quadrants(buf, Q_TR | Q_BL),
        0x259f => quadrants(buf, Q_TR | Q_BL | Q_BR),
        _ => {}
    }
}

fn box_drawing(buf: &mut Glyph, cp: u32, t: u32) {
    let gap = t.max(4);
    let t2 = t.saturating_mul(2);
    match cp {
        // triple dash horizontal, light and heavy
        0x2504 => dash_h(buf, 3, t, gap, t),
        0x2505 => dash_h(buf, 3, t2, gap, t),
        // triple dash vertical
        0x2506 => dash_v(buf, 3, t, gap, t),
        0x2507 => dash_v(buf, 3, t2, gap, t),
        // quadruple dash horizontal
        0x2508 => dash_h(buf, 4, t, gap, t),
        0x2509 => dash_h(buf, 4, t2, gap, t),
        // quadruple dash vertical
        0x250a => dash_v(buf, 4, t, gap, t),
        0x250b => dash_v(buf, 4, t2, gap, t),
        // double dash horizontal
        0x254c => dash_h(buf, 2, t, t, t),
        0x254d => dash_h(buf, 2, t2, t2, t),
        // double dash vertical
        0x254e => dash_v(buf, 2, t, t2, t),
        0x254f => dash_v(buf, 2, t2, t2, t),
        // arcs down and right, down and left, up and left, up and right
        0x256d => arc(buf, 1.0, 1.0, t),
        0x256e => arc(buf, -1.0, 1.0, t),
        0x256f => arc(buf, -1.0, -1.0, t),
        0x2570 => arc(buf, 1.0, -1.0, t),
        // upper right to lower left, upper left to lower right, both
        0x2571 => diagonal(buf, true, t),
        0x2572 => diagonal(buf, false, t),
        0x2573 => {
            diagonal(buf, true, t);
            diagonal(buf, false, t);
        }
        _ => {
            let i = usize::try_from(cp.saturating_sub(0x2500)).unwrap_or(usize::MAX);
            if let Some(&spec) = BOX_LINES.get(i) {
                draw_lines(buf, spec, t);
            }
        }
    }
}

/// braille.zig draw2800_28FF(): eight square dots on a 2x4 grid. The dot is
/// min(w/4, h/8) wide, the gaps between dots and the margins start at
/// about that, and the pixels left over go, in Ghostty's order, to a dot of
/// at least one pixel, a margin of at least one, the gaps, the margins and
/// the dot. Bits 0 to 7 of the codepoint are dots 1 to 8: 1, 2, 3 and 7
/// down the left, 4, 5, 6 and 8 down the right.
fn braille(buf: &mut Glyph, cp: u32) {
    const COL: [u32; 8] = [0, 0, 0, 1, 1, 1, 0, 1];
    const ROW: [u32; 8] = [0, 1, 2, 0, 1, 2, 3, 3];
    let (w, h) = (i64::from(buf.width()), i64::from(buf.height()));
    let mut dot = (w / 4).min(h / 8);
    let mut x_gap = w / 4;
    let mut y_gap = h / 8;
    let mut x_margin = x_gap / 2;
    let mut y_margin = y_gap / 2;
    // every term is at most a few times the bitmap's side, a u32
    let mut x_left = w
        .saturating_sub(x_margin.saturating_mul(2))
        .saturating_sub(x_gap)
        .saturating_sub(dot.saturating_mul(2));
    let mut y_left = h
        .saturating_sub(y_margin.saturating_mul(2))
        .saturating_sub(y_gap.saturating_mul(3))
        .saturating_sub(dot.saturating_mul(4));

    if x_left >= 2 && y_left >= 4 && dot == 0 {
        dot = 1;
        x_left = x_left.saturating_sub(2);
        y_left = y_left.saturating_sub(4);
    }
    if x_left >= 2 && x_margin == 0 {
        x_margin = 1;
        x_left = x_left.saturating_sub(2);
    }
    if y_left >= 2 && y_margin == 0 {
        y_margin = 1;
        y_left = y_left.saturating_sub(2);
    }
    if x_left >= 1 {
        x_gap = x_gap.saturating_add(1);
        x_left = x_left.saturating_sub(1);
    }
    if y_left >= 3 {
        y_gap = y_gap.saturating_add(1);
        y_left = y_left.saturating_sub(3);
    }
    if x_left >= 2 {
        x_margin = x_margin.saturating_add(1);
        x_left = x_left.saturating_sub(2);
    }
    if y_left >= 2 {
        y_margin = y_margin.saturating_add(1);
        y_left = y_left.saturating_sub(2);
    }
    if x_left >= 2 && y_left >= 4 {
        dot = dot.saturating_add(1);
    }

    let to_u32 = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    let (dot, x_gap, y_gap) = (to_u32(dot), to_u32(x_gap), to_u32(y_gap));
    let (x_margin, y_margin) = (to_u32(x_margin), to_u32(y_margin));
    for (i, (&col, &row)) in COL.iter().zip(ROW.iter()).enumerate() {
        if cp >> i & 1 == 0 {
            continue;
        }
        let x = x_margin.saturating_add(col.saturating_mul(dot.saturating_add(x_gap)));
        let y = y_margin.saturating_add(row.saturating_mul(dot.saturating_add(y_gap)));
        buf.fill(x, y, x.saturating_add(dot), y.saturating_add(dot), 0xff);
    }
}

/// common.zig fill() on a grid two blocks wide and `rows` high: bit 2r + c
/// of `mask` fills the block in column c of row r, its near edges rounded
/// as Fraction.min() and its far edges as Fraction.max(), so the blocks of
/// a cell and of its neighbours meet with nothing between them.
fn mosaic(buf: &mut Glyph, mask: u32, rows: u32) {
    let (w, h) = (buf.width(), buf.height());
    for i in 0..rows.saturating_mul(2) {
        if mask >> i & 1 == 0 {
            continue;
        }
        let (c, r) = (i % 2, i / 2);
        buf.fill(
            frac_min(w, c, 2),
            frac_min(h, r, rows),
            frac_max(w, c.saturating_add(1), 2),
            frac_max(h, r.saturating_add(1), rows),
            0xff,
        );
    }
}

/// The octants U+1CD00-U+1CDE5 (Unicode 16), as their names BLOCK OCTANT-n
/// list them: bit n-1 is octant n, numbered 1 2 / 3 4 / 5 6 / 7 8 from the
/// top. The 26 patterns that equal an earlier character (empty, full, the
/// halves, the quadrants, the quarter blocks and the like) are not in the
/// range, so the order follows no rule and is spelled out.
#[rustfmt::skip]
const OCTANTS: [u8; 0xe6] = [
    0x04, 0x06, 0x07, 0x08, 0x09, 0x0b, 0x0c, 0x0d,
    0x0e, 0x10, 0x11, 0x12, 0x13, 0x15, 0x16, 0x17,
    0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
    0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x41, 0x42,
    0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a,
    0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x51, 0x52, 0x53,
    0x54, 0x56, 0x57, 0x58, 0x59, 0x5b, 0x5c, 0x5d,
    0x5e, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
    0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e,
    0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76,
    0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e,
    0x7f, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
    0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
    0xa1, 0xa2, 0xa3, 0xa4, 0xa6, 0xa7, 0xa8, 0xa9,
    0xab, 0xac, 0xad, 0xae, 0xb0, 0xb1, 0xb2, 0xb3,
    0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb,
    0xbc, 0xbd, 0xbe, 0xbf, 0xc1, 0xc2, 0xc3, 0xc4,
    0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc,
    0xcd, 0xce, 0xcf, 0xd0, 0xd1, 0xd2, 0xd3, 0xd4,
    0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xdb, 0xdc,
    0xdd, 0xde, 0xdf, 0xe0, 0xe1, 0xe2, 0xe3, 0xe4,
    0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xeb, 0xec,
    0xed, 0xee, 0xef, 0xf1, 0xf2, 0xf3, 0xf4, 0xf6,
    0xf7, 0xf8, 0xf9, 0xfb, 0xfd, 0xfe,
];
