//! A vitrified set built in memory for the color glyph screen
//! (vtium/vtium#23): unifont's printable ASCII as a 1-bit strike, and
//! three synthetic color glyphs as a premultiplied RGBA strike at the same
//! 8x16 cell, so no font file of the system's reaches tier 2.
//!
//! Each color glyph is two cells wide, 16x16, painted from integer shapes
//! sampled 4 by 4 in each pixel: opaque inside, with alpha falling off
//! across the edges, and the rocket's flame translucent throughout, so a
//! wrong blend shows on every background.

use std::sync::Arc;

use vtium_font::set::{PixelFormat, Set, SetBuilder, StrikeBuilder};
use vtium_font::{Cell, Face, Style, Unifont, Width};

/// The set's one cell, unifont's.
pub const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// A straight (not premultiplied) color and its alpha.
type Rgba = [u8; 4];

/// Subsamples a pixel's side; a pixel is 8 units across, samples at the
/// odd units.
const SAMPLES: u32 = 4;
/// Units a pixel's side.
const UNIT: i64 = 8;

/// A wide glyph's side in pixels.
const SIDE: u32 = 16;

fn disc(u: i64, v: i64, cu: i64, cv: i64, r: i64) -> bool {
    (u - cu).pow(2) + (v - cv).pow(2) <= r * r
}

/// The grinning face, U+1F600: a yellow disc with two eyes and a smile.
fn grin(u: i64, v: i64) -> Option<Rgba> {
    let d2 = (u - 64).pow(2) + (v - 64).pow(2);
    if d2 > 60 * 60 {
        return None;
    }
    let dark = [70, 40, 0, 255];
    if disc(u, v, 44, 46, 9) || disc(u, v, 84, 46, 9) {
        return Some(dark);
    }
    if v >= 72 && (30 * 30..=40 * 40).contains(&d2) {
        return Some(dark);
    }
    Some([255, 204, 0, 255])
}

/// The rocket, U+1F680: a silver body rising to the right with a red nose
/// and a blue window, over a translucent orange flame.
fn rocket(u: i64, v: i64) -> Option<Rgba> {
    if disc(u, v, 74, 54, 8) {
        return Some([40, 110, 220, 255]);
    }
    // the body: within 16 units of the segment from (44,84) to (92,36)
    let (du, dv) = (48i64, -48i64);
    let (pu, pv) = (u - 44, v - 84);
    let num = pu * du + pv * dv;
    let den = du * du + dv * dv;
    let dist2_den = if num <= 0 {
        (pu * pu + pv * pv) * den
    } else if num >= den {
        ((u - 92).pow(2) + (v - 36).pow(2)) * den
    } else {
        (pu * pu + pv * pv) * den - num * num
    };
    if dist2_den <= 16 * 16 * den {
        return Some(if num * 100 > den * 72 {
            [220, 30, 30, 255]
        } else {
            [190, 195, 205, 255]
        });
    }
    disc(u, v, 26, 102, 30).then_some([255, 120, 0, 160])
}

/// The heart, U+2764: `(x² + y² - 1)³ - x²y³ <= 0`, 52 units to 1,
/// red darkening downwards, with a pink highlight.
fn heart(u: i64, v: i64) -> Option<Rgba> {
    const S: i64 = 52;
    let (x, y) = (u - 64, 70 - v);
    let f = (x * x + y * y - S * S).pow(3) - x * x * y.pow(3) * S;
    if f > 0 {
        return None;
    }
    if disc(u, v, 40, 40, 8) {
        return Some([255, 150, 170, 255]);
    }
    let shade = u8::try_from(235 - v / 2).unwrap_or(0);
    Some([shade, 20, 50, 255])
}

/// A 16x16 premultiplied RGBA glyph sampled from `shape`.
fn paint(shape: fn(i64, i64) -> Option<Rgba>) -> Vec<u8> {
    let n = SAMPLES * SAMPLES;
    let mut px = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut sum = [0u32; 4];
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let u = i64::from(x) * UNIT + i64::from(2 * i + 1);
                    let v = i64::from(y) * UNIT + i64::from(2 * j + 1);
                    let Some([r, g, b, a]) = shape(u, v) else {
                        continue;
                    };
                    let a32 = u32::from(a);
                    let pre = |c: u8| (u32::from(c) * a32 + 127) / 255;
                    sum[0] += pre(r);
                    sum[1] += pre(g);
                    sum[2] += pre(b);
                    sum[3] += a32;
                }
            }
            // each sample's color is at most its alpha, so the means are too
            px.extend(sum.map(|s| u8::try_from((s + n / 2) / n).unwrap_or(u8::MAX)));
        }
    }
    px
}

/// The color glyphs and the codepoints that draw them.
pub fn color_glyphs() -> [(char, Vec<u8>); 3] {
    [
        ('\u{1f600}', paint(grin)),
        ('\u{1f680}', paint(rocket)),
        ('\u{2764}', paint(heart)),
    ]
}

/// A mask glyph's rows as 1-bit rows, the first pixel the high bit.
fn bits(glyph: &vtium_font::Glyph) -> Vec<u8> {
    glyph
        .rows()
        .flat_map(|row| {
            row.chunks(8).map(|byte| {
                byte.iter()
                    .enumerate()
                    .fold(0u8, |b, (i, &p)| b | (u8::from(p >= 0x80) << (7 - i)))
            })
        })
        .collect()
}

/// The set's bytes: unifont's printable ASCII in a 1-bit strike and the
/// color glyphs in an RGBA strike, both at [`CELL`].
pub fn bytes() -> Result<Vec<u8>, String> {
    let unifont = Unifont::new(CELL.height);
    let mut text = StrikeBuilder::new(PixelFormat::Bits, CELL, Style::default());
    for c in ' '..='~' {
        let g = unifont
            .draw(&[c], Style::default())
            .ok_or_else(|| format!("unifont lacks {c:?}"))?;
        let n = text.glyph(Width::Narrow, bits(&g));
        text.map(c, n);
    }
    let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default());
    for (c, px) in color_glyphs() {
        let n = color.glyph(Width::Wide, px);
        color.map(c, n);
    }
    SetBuilder::new("tier2-color")
        .strike(text)
        .strike(color)
        .write()
        .map_err(|e| e.to_string())
}

/// The set, read as vtium reads a set from the store.
pub fn set() -> Result<Arc<Set>, String> {
    Set::from_bytes(bytes()?)
        .map(Arc::new)
        .map_err(|e| format!("the color set: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_font::set::SetFace;
    use vtium_font::{Format, Glyph};

    // Every color glyph is premultiplied, has opaque pixels, and has
    // pixels of partial alpha, so its edges blend.
    #[test]
    fn color_glyphs_are_premultiplied_with_partial_edges() {
        for (c, px) in color_glyphs() {
            assert_eq!(px.len(), (SIDE * SIDE * 4) as usize, "{c:?}");
            let alphas: Vec<u8> = px.chunks(4).map(|p| p[3]).collect();
            for p in px.chunks(4) {
                assert!(p[..3].iter().all(|&ch| ch <= p[3]), "{c:?}: {p:?}");
            }
            assert!(alphas.contains(&255), "{c:?} has no opaque pixel");
            assert!(alphas.contains(&0), "{c:?} has no clear pixel");
            assert!(
                alphas.iter().any(|&a| a > 0 && a < 255),
                "{c:?} has no partial alpha"
            );
        }
    }

    // The set reads back at 8x16 with both strikes: text as masks, the
    // color glyphs as RGBA two cells wide.
    #[test]
    fn the_set_draws_text_and_color_at_one_cell() {
        let face = SetFace::new(set().unwrap(), 16).unwrap();
        assert_eq!(face.cell(), CELL);
        let a = face.draw(&['A'], Style::default()).unwrap();
        let want: Glyph = Unifont::new(16).draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.format(), a.pixels()), (Format::Mask, want.pixels()));
        for (c, px) in color_glyphs() {
            let g = face.draw(&[c], Style::default()).unwrap();
            assert_eq!((g.format(), g.cells()), (Format::Rgba, Width::Wide));
            assert_eq!(g.pixels(), px);
        }
    }
}
