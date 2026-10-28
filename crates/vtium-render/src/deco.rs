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
//! A line that would leave the cell is moved up into it. An underline takes
//! at most a quarter of the cell's rows, where the C reference moved a
//! double or a curl as far up as it needed (vtium/vtium#119): a curl lowers
//! its wave to fit, down to 2 rows peak to peak, and a double, or a curl
//! that still does not fit, draws as the single line.

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

/// The rows an underline may take in a cell `h` tall: a quarter, the most
/// the C reference let a line's thickness take (font.c:370-408).
fn room(h: u32) -> u32 {
    h / 4
}

/// The curl's height peak to peak in a cell `w` by `h` with lines `t`
/// thick: the C reference's quarter of the width, at least 2, lowered to
/// fit [`room`]; `None` when 2 does not fit.
fn curl_height(w: u32, h: u32, t: u32) -> Option<u32> {
    let amp = (w / 4).max(2).min(room(h).checked_sub(t)?);
    (amp >= 2).then_some(amp)
}

// The arithmetic is the C reference's, in `f64`, which every host rounds the
// same way; no integer here passes the cell's size, so none wraps.
fn curly(s: &mut Sprite, m: &Decorations, amp: u32) {
    let (w, h, t) = (s.w, s.h, m.underline_thickness);
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
    match (kind, curl_height(w, h, t)) {
        (Kind::Double, _) if t.saturating_mul(3) <= room(h) => {
            let top = fit(m.underline, t.saturating_mul(3), h);
            s.fill_rows(0, w, top, t);
            s.fill_rows(0, w, top.saturating_add(t.saturating_mul(2)), t);
        }
        (Kind::Curly, Some(amp)) => curly(&mut s, m, amp),
        (Kind::Single | Kind::Double | Kind::Curly, _) => s.fill_rows(0, w, m.underline, t),
        (Kind::Dotted, _) => dotted(&mut s, m),
        (Kind::Dashed, _) => s.fill_rows(
            w.saturating_add(3) / 6,
            w.saturating_mul(5).saturating_add(3) / 6,
            m.underline,
            t,
        ),
        (Kind::Strike, _) => s.fill_rows(0, w, m.strike, m.strike_thickness),
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
    clippy::arithmetic_side_effects,
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

    /// Whether the C reference's `kind` at `w` by `h` took more than a
    /// quarter of the cell's rows: a double's 3 lines in a cell taller than
    /// one line, or a curl's height, a quarter of the width and at least 2,
    /// and a line.
    fn past_a_quarter(kind: Kind, w: u32, h: u32) -> bool {
        let t = Decorations::for_height(h).underline_thickness;
        match kind {
            Kind::Double => 3 * t > h / 4 && h > t,
            Kind::Curly => (w / 4).max(2) + t > h / 4,
            _ => false,
        }
    }

    /// Whether `px`, the `kind` vtium draws at `w` by `h` where the C
    /// reference's took more than a quarter of the cell, is the fallback:
    /// the single line, or a curl lowered to the quarter's rows when 2
    /// rows peak to peak and the line fit in it.
    fn falls_back(kind: Kind, w: u32, h: u32, px: &[u8]) -> bool {
        let m = Decorations::for_height(h);
        let single = draw(Kind::Single, w, h, &m);
        if kind == Kind::Curly && h / 4 >= m.underline_thickness + 2 {
            px != single && rows(px, w) == Some((h - h / 4, h))
        } else {
            px == single
        }
    }

    // Every sprite at every cell size from 1x1 to 24x48 against the C
    // reference's `kmscon_deco_render` (tests/reference/README.md); the
    // curly wave's sine is vtium's own, so this is also the check that it
    // draws the C reference's bytes. A double or curl past a quarter of the
    // cell is the render crate's departure: it must differ from the C
    // reference's and draw the fallback.
    // covers: platform.compositor
    #[test]
    fn sprites_match_the_c_reference() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/cells.tsv");
        let tsv = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        let mut departed = 0;
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
            if past_a_quarter(Kind::ALL[kind - 1], w, h) {
                departed += 1;
                if d == want {
                    bad.push(format!("{line} (a departure, drawn as the C reference)"));
                } else if !falls_back(Kind::ALL[kind - 1], w, h, &px) {
                    bad.push(format!("{line} (a departure, not the fallback)"));
                }
            } else if d != want {
                bad.push(line.to_owned());
            }
        }
        assert_eq!(checked, 6 * 24 * 48);
        // every width's double at heights 2 to 11; every width's curl at
        // heights 1 to 11, and widths 12 to 24 at 12 to 15, 16 to 24 at
        // 16 to 19, 20 to 24 at 20 to 23 and 24 at 24 to 27
        assert_eq!(departed, 10 * 24 + (11 * 24 + 4 * 13 + 4 * 9 + 4 * 5 + 4));
        assert!(
            bad.is_empty(),
            "{} of {checked} differ, first: {:?}",
            bad.len(),
            &bad[..bad.len().min(10)]
        );
    }

    /// The rows `px` covers, `w` wide: the first and one past the last.
    fn rows(px: &[u8], w: u32) -> Option<(u32, u32)> {
        let lit: Vec<u32> = (0..)
            .zip(px.chunks(w as usize))
            .filter(|(_, r)| r.iter().any(|&p| p != 0))
            .map(|(y, _)| y)
            .collect();
        Some((*lit.first()?, lit.last()? + 1))
    }

    const UNDERLINES: [Kind; 5] = [
        Kind::Single,
        Kind::Double,
        Kind::Curly,
        Kind::Dotted,
        Kind::Dashed,
    ];

    // An 8x8 cell's letters stand on row 6 and its underline is row 7
    // (`tools/testfont`), where the C reference's double and curl reached
    // rows 5 to 7 (vtium/vtium#119).
    // covers: platform.compositor
    #[test]
    fn every_underline_at_8_pixels_lies_in_the_bottom_row() {
        let m = Decorations::for_height(8);
        for kind in UNDERLINES {
            assert_eq!(rows(&draw(kind, 8, 8, &m), 8), Some((7, 8)), "{kind:?}");
        }
    }

    // covers: platform.compositor
    #[test]
    fn a_double_or_curl_with_no_room_draws_as_the_single_line() {
        for (w, h) in [(8, 8), (16, 8), (6, 10), (8, 11), (5, 4), (4, 1)] {
            let m = Decorations::for_height(h);
            let single = draw(Kind::Single, w, h, &m);
            assert_eq!(draw(Kind::Double, w, h, &m), single, "double {w}x{h}");
            assert_eq!(draw(Kind::Curly, w, h, &m), single, "curly {w}x{h}");
        }
    }

    // A band of 3t rows for the double, and the curl's height plus t, are
    // what the C reference drew; a quarter of the cell is the most it let a
    // line's thickness take (font.c:370-408).
    // covers: platform.compositor
    #[test]
    fn no_underline_takes_more_than_a_quarter_of_the_cell() {
        for h in 1..=48u32 {
            let m = Decorations::for_height(h);
            let most = (h / 4).max(m.underline_thickness);
            for w in 1..=24u32 {
                for kind in UNDERLINES {
                    let (top, end) = rows(&draw(kind, w, h, &m), w).unwrap();
                    assert!(end - top <= most, "{kind:?} {w}x{h}: rows {top}..{end}");
                }
            }
        }
    }

    // covers: platform.compositor
    #[test]
    fn a_curl_lowers_its_wave_to_fit_before_it_flattens() {
        // 16x16: a curl 4 rows peak to peak would take 5 of the 4 rows a
        // quarter gives; 3 fits, and it is still a wave
        let m = Decorations::for_height(16);
        let px = draw(Kind::Curly, 16, 16, &m);
        assert_eq!(rows(&px, 16), Some((12, 16)));
        assert_ne!(px, draw(Kind::Single, 16, 16, &m));
        // 8x16: 2 rows peak to peak and the line take 3, as the C
        // reference drew it
        assert_eq!(rows(&draw(Kind::Curly, 8, 16, &m), 8), Some((13, 16)));
    }

    // vtium-font gives a strike scaled by a whole factor the lines of the
    // scaled height when its own were `for_height` of its height, and its
    // own lines scaled otherwise (vtium/vtium#124). The room is the drawn
    // cell's, so an 8-pixel strike drawn at 16 has a 16-pixel cell's double.
    // covers: platform.compositor
    #[test]
    fn a_scaled_strike_takes_the_room_of_the_cell_it_draws() {
        let scaled = Decorations::for_height(16);
        let px = draw(Kind::Double, 16, 16, &scaled);
        let lit: Vec<bool> = px.chunks(16).map(|r| r[0] != 0).collect();
        assert_eq!(&lit[12..], &[false, true, false, true]);
        // the 8-pixel strike's own lines scaled by 2: two rows thick from
        // row 14, which leave no room for a double
        let own = Decorations {
            underline: 14,
            underline_thickness: 2,
            strike: 8,
            strike_thickness: 2,
        };
        assert_eq!(
            draw(Kind::Double, 16, 16, &own),
            draw(Kind::Single, 16, 16, &own)
        );
    }
}
