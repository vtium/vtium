//! Underlines and strike-through as one-cell coverage sprites, tinted when
//! drawn and tiled across a wide cell, never part of a glyph (ADR-1300,
//! "Carried forward"). The shapes are the C reference's `deco.c`, whose
//! notice, under the MIT licence whose text is in COPYING, is:
//!
//! Copyright (c) 2026 the kmscon fork authors
//!
//! - single: the font's underline, full width;
//! - double: the single line and a second one a thickness below it;
//! - curly: one sine period a cell, from its centre line and down first,
//!   in the space from the single line's top downwards;
//! - dotted: square dots a thickness wide, as many a cell as fit with a gap
//!   of the same size, centred in their slots;
//! - dashed: one dash a cell over its middle two thirds;
//! - strike: the font's strike-through, full width.
//!
//! A line that would leave the cell is moved up into it.

use vtium_font::Decorations;
use vtium_term::UnderlineStyle;

use crate::stored::Stored;
use crate::target::Orientation;
use crate::trig;

/// Samples a pixel side for the curly wave's coverage.
const CURLY_SS: u32 = 8;

/// A cell's coverage, `w` by `h`, row by row.
struct Sprite {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Sprite {
    fn new(w: u32, h: u32) -> Sprite {
        let n = usize::try_from(u64::from(w).saturating_mul(u64::from(h))).unwrap_or(0);
        Sprite {
            w,
            h,
            px: vec![0; n],
        }
    }

    fn at(&mut self, x: u32, y: u32) -> Option<&mut u8> {
        let i = u64::from(y)
            .checked_mul(u64::from(self.w))?
            .checked_add(u64::from(x))?;
        self.px.get_mut(usize::try_from(i).ok()?)
    }

    /// Fills columns `[x0, x1)` of `rows` rows from `y0`, clipped.
    fn fill_rows(&mut self, x0: u32, x1: u32, y0: u32, rows: u32) {
        let x1 = x1.min(self.w);
        for y in y0..y0.saturating_add(rows).min(self.h) {
            for x in x0..x1 {
                if let Some(p) = self.at(x, y) {
                    *p = 0xff;
                }
            }
        }
    }
}

/// The top of a band `rows` high starting at `top`, moved up to end inside
/// `h`.
fn fit(top: u32, rows: u32, h: u32) -> u32 {
    if top.saturating_add(rows) <= h {
        top
    } else if rows >= h {
        0
    } else {
        h.saturating_sub(rows)
    }
}

// The arithmetic is the C reference's, in `f64`, which every host rounds the
// same way; no integer here passes the cell's size, so none wraps.
fn curly(s: &mut Sprite, m: &Decorations) {
    let (w, h, t) = (s.w, s.h, m.underline_thickness);
    let amp = if w / 4 > 2 { w / 4 } else { 2 };
    let band = amp.saturating_add(t);
    let top = fit(m.underline, band, h);
    let mid = f64::from(top) + f64::from(band) / 2.0;
    let k = 2.0 * core::f64::consts::PI / f64::from(w);
    let half_amp = f64::from(amp) / 2.0;
    for y in top..top.saturating_add(band).min(h) {
        for x in 0..w {
            let mut hits = 0u32;
            for sx in 0..CURLY_SS {
                let fx = f64::from(x) + (f64::from(sx) + 0.5) / f64::from(CURLY_SS);
                let yc = mid + half_amp * trig::sin(k * fx);
                let slope = half_amp * k * trig::cos(k * fx);
                // a stroke of thickness t, measured across the curve
                let half = f64::from(t) / 2.0 * (1.0 + slope * slope).sqrt();
                for sy in 0..CURLY_SS {
                    let fy = f64::from(y) + (f64::from(sy) + 0.5) / f64::from(CURLY_SS);
                    if (fy - yc).abs() <= half {
                        hits = hits.saturating_add(1);
                    }
                }
            }
            if let Some(p) = s.at(x, y) {
                *p = u8::try_from(hits.saturating_mul(255) / (CURLY_SS * CURLY_SS)).unwrap_or(0xff);
            }
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the dots lie inside the sprite's width"
)]
fn dotted(s: &mut Sprite, m: &Decorations) {
    let t = m.underline_thickness;
    let n =
        s.w.saturating_add(t)
            .checked_div(t.saturating_mul(2))
            .unwrap_or(0)
            .max(1);
    let slot = f64::from(s.w) / f64::from(n);
    for i in 0..n {
        let i = f64::from(i);
        let x0 = (i * slot + slot / 4.0 + 0.5).floor() as u32;
        let x1 = (i * slot + 3.0 * slot / 4.0 + 0.5).floor() as u32;
        let x1 = if x1 <= x0 { x0.saturating_add(1) } else { x1 };
        s.fill_rows(x0, x1, m.underline, t);
    }
}

/// The decorations a cell draws: `None` for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
    Strike,
}

impl Kind {
    pub(crate) fn underline(u: UnderlineStyle) -> Option<Kind> {
        match u {
            UnderlineStyle::None => None,
            UnderlineStyle::Single => Some(Kind::Single),
            UnderlineStyle::Double => Some(Kind::Double),
            UnderlineStyle::Curly => Some(Kind::Curly),
            UnderlineStyle::Dotted => Some(Kind::Dotted),
            UnderlineStyle::Dashed => Some(Kind::Dashed),
        }
    }

    const ALL: [Kind; 6] = [
        Kind::Single,
        Kind::Double,
        Kind::Curly,
        Kind::Dotted,
        Kind::Dashed,
        Kind::Strike,
    ];
}

/// The `w` by `h` coverage of `kind`, from the font's lines `m`, row by row.
pub(crate) fn draw(kind: Kind, w: u32, h: u32, m: &Decorations) -> Vec<u8> {
    let mut s = Sprite::new(w, h);
    if w == 0 || h == 0 {
        return s.px;
    }
    let t = m.underline_thickness;
    match kind {
        Kind::Single => s.fill_rows(0, w, m.underline, t),
        Kind::Double => {
            let top = fit(m.underline, t.saturating_mul(3), h);
            s.fill_rows(0, w, top, t);
            s.fill_rows(0, w, top.saturating_add(t.saturating_mul(2)), t);
        }
        Kind::Curly => curly(&mut s, m),
        Kind::Dotted => dotted(&mut s, m),
        Kind::Dashed => s.fill_rows(
            w.saturating_add(3) / 6,
            w.saturating_mul(5).saturating_add(3) / 6,
            m.underline,
            t,
        ),
        Kind::Strike => s.fill_rows(0, w, m.strike, m.strike_thickness),
    }
    s.px
}

/// Every decoration for one cell, turned to the display's orientation.
#[derive(Debug)]
pub(crate) struct Sprites {
    sprites: Vec<Stored>,
}

impl Sprites {
    pub(crate) fn new(w: u32, h: u32, m: &Decorations, o: Orientation, bits: bool) -> Sprites {
        let sprites = Kind::ALL
            .iter()
            .map(|&k| Stored::mask(&draw(k, w, h, m), w, h, o, bits))
            .collect();
        Sprites { sprites }
    }

    pub(crate) fn get(&self, kind: Kind) -> Option<&Stored> {
        let i = Kind::ALL.iter().position(|&k| k == kind)?;
        self.sprites.get(i)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
        h
    }

    // Every sprite at every cell size from 1x1 to 24x48 against the C
    // reference's `kmscon_deco_render` (tests/reference/README.md); the
    // curly wave's sine is vtium's own, so this is also the check that it
    // draws the C reference's bytes.
    #[test]
    fn sprites_match_the_c_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/cells.tsv");
        let tsv = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        let mut bad = Vec::new();
        for line in tsv.lines().filter(|l| l.starts_with("deco\t")) {
            let f: Vec<&str> = line.split('\t').collect();
            let (kind, w, h): (usize, u32, u32) = (
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
            );
            let want = u64::from_str_radix(f[4], 16).unwrap();
            let px = draw(Kind::ALL[kind - 1], w, h, &Decorations::for_height(h));
            let d = fnv(
                fnv(
                    fnv(0xcbf2_9ce4_8422_2325, &w.to_le_bytes()),
                    &h.to_le_bytes(),
                ),
                &px,
            );
            checked += 1;
            if d != want {
                bad.push(line.to_owned());
            }
        }
        assert_eq!(checked, 6 * 24 * 48);
        assert!(
            bad.is_empty(),
            "{} of {checked} differ, first: {:?}",
            bad.len(),
            &bad[..bad.len().min(10)]
        );
    }
}
