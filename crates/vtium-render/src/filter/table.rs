//! The color table: a look's color filters merged into one lattice of
//! 33 steps in each of red, green and blue, and the CPU path's lookup in it.
//!
//! The node at (r, g, b) holds what the look's color filters, applied in
//! the file's order, give the color (r/32, g/32, b/32). Each filter's
//! output is held to 0 to 1 before the next filter takes it. A node holds
//! three half-precision floats, which the GPU path uploads as a 3D texture
//! and the CPU path widens to 32-bit floats. However many filters a look
//! holds, each pixel then costs one lookup.
//!
//! The CPU path finds the eight nodes around a pixel's color and blends
//! them trilinearly in 32-bit floats, red first, then green, then blue,
//! with no fused multiply-add. It rounds each channel to 8 bits as
//! floor(x × 255 + 0.5). It remembers the last color it looked up, so a run
//! of one color, such as a background, costs one lookup.
//!
//! References:
//! - ADR-1304 D2: the table's size, its nodes, how it is built and how each path reads it.

use std::collections::TryReserveError;

use super::Map;
use super::math::{from_half, half, unit};
use crate::bounds::COLOR_TABLE_SIDE;

/// Nodes on each side.
const SIDE: usize = COLOR_TABLE_SIDE;

/// The last node's index on a side, 32: a node's color is its index over
/// this.
const LAST: u16 = 32;

/// The highest index a blend starts from, the node before the last.
const LAST_START: u16 = 31;

/// A look's color filters merged into one table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Each node's red, green and blue as half floats, red's index the
    /// fastest, then green's, then blue's, as a 3D texture lays its texels
    /// out.
    nodes: Vec<[u16; 3]>,
}

impl Table {
    /// The table of the color filters `maps`, applied in order. An empty
    /// list gives the identity, which maps each color to itself. The only
    /// failure is a want of memory.
    pub fn build(maps: &[Map]) -> Result<Table, TryReserveError> {
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(SIDE.saturating_mul(SIDE).saturating_mul(SIDE))?;
        for b in 0..=LAST {
            for g in 0..=LAST {
                for r in 0..=LAST {
                    let mut c = [r, g, b].map(|i| f64::from(i) / f64::from(LAST));
                    for m in maps {
                        c = m.apply(c).map(unit);
                    }
                    nodes.push(c.map(half));
                }
            }
        }
        Ok(Table { nodes })
    }

    /// Each node's red, green and blue as half floats, in the texture's
    /// order: red's index the fastest, then green's, then blue's.
    pub fn nodes(&self) -> &[[u16; 3]] {
        &self.nodes
    }

    /// The table as the GPU path uploads it: an RGBA16F texture 33 a side,
    /// each half little-endian, alpha 1. RGBA rather than RGB because
    /// OpenGL ES 3.0 filters RGBA16F linearly on every GPU.
    pub fn rgba16f(&self) -> Result<Vec<u8>, TryReserveError> {
        let mut out = Vec::new();
        out.try_reserve_exact(self.nodes.len().saturating_mul(8))?;
        for &[r, g, b] in &self.nodes {
            for h in [r, g, b, 0x3c00] {
                out.extend_from_slice(&h.to_le_bytes());
            }
        }
        Ok(out)
    }

    /// Nodes on each side of the table.
    pub const fn side() -> usize {
        SIDE
    }
}

/// The CPU path's lookup in a color table: the nodes widened to 32-bit
/// floats, 421 KiB, and the last color looked up.
#[derive(Clone, Debug)]
pub struct Lookup {
    nodes: Vec<[f32; 3]>,
    last: Option<(u32, u32)>,
}

impl Lookup {
    /// The lookup in `table`. The only failure is a want of memory.
    pub fn new(table: &Table) -> Result<Lookup, TryReserveError> {
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(table.nodes.len())?;
        nodes.extend(table.nodes.iter().map(|n| n.map(from_half)));
        Ok(Lookup { nodes, last: None })
    }

    /// The color `px` maps to, both `0x00RRGGBB`. The byte above red passes
    /// through unchanged.
    pub fn map(&mut self, px: u32) -> u32 {
        if let Some((from, to)) = self.last
            && from == px
        {
            return to;
        }
        let to = self.blend(px);
        self.last = Some((px, to));
        to
    }

    /// Maps each pixel of `pixels` in place.
    pub fn map_all(&mut self, pixels: &mut [u32]) {
        for p in pixels {
            *p = self.map(*p);
        }
    }

    /// The trilinear blend of the eight nodes around `px`'s color.
    fn blend(&self, px: u32) -> u32 {
        let [(r, fr), (g, fg), (b, fb)] = [16, 8, 0].map(|shift| axis(px >> shift & 0xff));
        let node = |dr: usize, dg: usize, db: usize| -> [f32; 3] {
            let i = (b.saturating_add(db))
                .saturating_mul(SIDE)
                .saturating_add(g.saturating_add(dg))
                .saturating_mul(SIDE)
                .saturating_add(r.saturating_add(dr));
            self.nodes.get(i).copied().unwrap_or_default()
        };
        let mut out = px & 0xff00_0000;
        for (c, shift) in [(0, 16), (1, 8), (2, 0)] {
            let ch = |n: [f32; 3]| n.get(c).copied().unwrap_or(0.0);
            let along_r = |dg, db| lerp(ch(node(0, dg, db)), ch(node(1, dg, db)), fr);
            let along_g = |db| lerp(along_r(0, db), along_r(1, db), fg);
            let x = lerp(along_g(0), along_g(1), fb);
            out |= level(x) << shift;
        }
        out
    }
}

/// The node below an 8-bit channel `v` on its side, and how far `v` lies
/// toward the next node, 0 to 1. 255 lies at the last node, which the
/// blend reaches from the node below it with a weight of 1.
fn axis(v: u32) -> (usize, f32) {
    let v = u16::try_from(v & 0xff).unwrap_or(0);
    let p = f32::from(v) / 255.0 * f32::from(LAST);
    let below = p.floor().min(f32::from(LAST_START));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "below is a whole number from 0 to 31"
    )]
    let i = below as usize;
    (i, p - below)
}

/// `a` toward `b` by `t`: a multiply and an add, rounded apart.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A channel from 0 to 1 as an 8-bit level, floor(x × 255 + 0.5).
fn level(x: f32) -> u32 {
    let v = (x * 255.0 + 0.5).floor();
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "v is a whole number from 1 to 255 once held"
    )]
    let v = v.min(255.0) as u32;
    v
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a test fails by panicking, and its figures are small"
)]
mod tests {
    use super::super::{COLOR, Fixed, Given, Map, color};
    use super::*;

    /// The map of the color filter `name` with `values`.
    fn map(name: &str, values: &[Given]) -> Map {
        let d = color(name).unwrap_or_else(|| panic!("{name}"));
        Map::new(d, values).unwrap_or_else(|| panic!("{name} {values:?}"))
    }

    fn n(whole: i32, fraction: i32) -> Given {
        Given::Number(Fixed::of(whole, fraction))
    }

    fn table(maps: &[Map]) -> Table {
        Table::build(maps).unwrap_or_else(|e| panic!("{e}"))
    }

    fn lookup(t: &Table) -> Lookup {
        Lookup::new(t).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The node at (r, g, b) as 32-bit floats.
    fn node(t: &Table, r: usize, g: usize, b: usize) -> [f32; 3] {
        t.nodes()[(b * SIDE + g) * SIDE + r].map(from_half)
    }

    fn gray(v: u32) -> u32 {
        v << 16 | v << 8 | v
    }

    /// FNV-1a over the table's halves, little-endian, in the texture's
    /// order: the digest a pin compares.
    fn digest(t: &Table) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for node in t.nodes() {
            for half in node {
                for byte in half.to_le_bytes() {
                    h ^= u64::from(byte);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        h
    }

    // With no filter the table is the identity, and the CPU path's lookup
    // returns every one of the 16,777,216 colors unchanged.
    #[test]
    fn the_identity_table_returns_every_color_unchanged() {
        let t = table(&[]);
        assert_eq!(t.nodes().len(), 35_937);
        let mut l = lookup(&t);
        for px in 0..0x0100_0000u32 {
            let got = l.blend(px);
            if got != px {
                panic!("{px:06x} looked up as {got:06x}");
            }
        }
        // the byte above red passes through
        assert_eq!(l.map(0xab12_3456), 0xab12_3456);
    }

    // Each filter's table, at its defaults and at one setting that is not
    // its default, is the same bytes on every machine: these digests hold
    // on x86 and on ARM, where CI's arm-native job runs this test.
    #[test]
    fn each_filter_s_table_is_pinned_to_its_bytes() {
        let cases: [(&str, Map, u64); 8] = [
            (
                "gamma, default",
                map("gamma", &[n(1, 0)]),
                0xe8d2e82df62d5eeb,
            ),
            ("gamma 1.8", map("gamma", &[n(1, 8000)]), 0xe17966a491e0e052),
            (
                "contrast, default",
                map("contrast", &[n(1, 0), n(0, 0)]),
                0xe8d2e82df62d5eeb,
            ),
            (
                "contrast 1.3, brightness -0.1",
                map("contrast", &[n(1, 3000), Given::Number(Fixed(-1000))]),
                0x8e7b07c064275d08,
            ),
            (
                "tint, default",
                map("tint", &[Given::Color([255, 176, 0]), n(1, 0)]),
                0xc1ddbebd0111b927,
            ),
            (
                "tint green, 0.6",
                map("tint", &[Given::Color([51, 255, 102]), n(0, 6000)]),
                0x13f41a9641670daa,
            ),
            (
                "color-blind, default",
                map(
                    "color-blind",
                    &[
                        Given::Choice("deuteranopia"),
                        Given::Choice("correct"),
                        n(1, 0),
                    ],
                ),
                0xd8ffdbd30587b7ff,
            ),
            (
                "color-blind protanopia simulate 0.75",
                map(
                    "color-blind",
                    &[
                        Given::Choice("protanopia"),
                        Given::Choice("simulate"),
                        n(0, 7500),
                    ],
                ),
                0x5723704575332e55,
            ),
        ];
        let got: Vec<(&str, u64)> = cases
            .iter()
            .map(|(name, m, _)| (*name, digest(&table(&[*m]))))
            .collect();
        let want: Vec<(&str, u64)> = cases.iter().map(|(name, _, d)| (*name, *d)).collect();
        assert_eq!(got, want);
    }

    // Each filter's defaults make a map, and gamma and contrast at their
    // defaults change no color.
    #[test]
    fn gamma_and_contrast_at_their_defaults_are_the_identity() {
        let identity = table(&[]);
        for name in ["gamma", "contrast"] {
            let d = color(name).unwrap_or_else(|| panic!("{name}"));
            let m = Map::defaults(d).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(table(&[m]), identity, "{name}");
        }
        for d in COLOR {
            assert!(Map::defaults(d).is_some(), "{}", d.name);
        }
    }

    // gamma raises each channel to the power 1 / gamma: at 2, a quarter
    // becomes a half. contrast doubles each channel's distance from middle
    // gray, then adds the brightness.
    #[test]
    fn gamma_and_contrast_follow_their_formulas() {
        let t = table(&[map("gamma", &[n(2, 0)])]);
        assert_eq!(node(&t, 8, 16, 32), [0.5, 0.70703125, 1.0]);
        let t = table(&[map("contrast", &[n(2, 0), n(0, 0)])]);
        assert_eq!(node(&t, 8, 16, 24), [0.0, 0.5, 1.0]);
        let t = table(&[map("contrast", &[n(1, 0), Given::Number(Fixed(-2500))])]);
        assert_eq!(node(&t, 8, 16, 32), [0.0, 0.25, 0.75]);
    }

    // At an amount of 1, tint shows each pixel as its gray times the color:
    // white becomes the color, black stays black, and a gray of half
    // becomes half the color. At an amount of 0 it changes nothing.
    #[test]
    fn tint_turns_each_color_toward_its_gray_times_the_color() {
        let mut l = lookup(&table(&[map(
            "tint",
            &[Given::Color([255, 176, 0]), n(1, 0)],
        )]));
        assert_eq!(l.map(0xffffff), 0xffb000);
        assert_eq!(l.map(0x000000), 0x000000);
        // pure red's gray is 77/256 of white
        let red = (77.0f64 / 256.0 * 255.0).round() as u32;
        let red_amber = red << 16 | ((77.0f64 / 256.0 * 176.0).round() as u32) << 8;
        let got = l.map(0xff0000);
        for shift in [16, 8, 0] {
            let (a, b) = (got >> shift & 0xff, red_amber >> shift & 0xff);
            assert!(a.abs_diff(b) <= 1, "{got:06x} against {red_amber:06x}");
        }
        assert_eq!(
            table(&[map("tint", &[Given::Color([255, 176, 0]), n(0, 0)])]),
            table(&[])
        );
    }

    // Machado's matrices keep each gray a gray, since each row sums to 1,
    // and a gray loses nothing a viewer cannot see, so both modes leave
    // the grays where they are. A color moves, and simulate's red, seen
    // with deuteranopia, matches the matrix applied in linear light.
    #[test]
    fn color_blind_keeps_the_grays_and_moves_the_colors() {
        for kind in ["protanopia", "deuteranopia", "tritanopia"] {
            for mode in ["simulate", "correct"] {
                let m = map(
                    "color-blind",
                    &[Given::Choice(kind), Given::Choice(mode), n(1, 0)],
                );
                let mut l = lookup(&table(&[m]));
                for v in [0, 1, 64, 128, 200, 255] {
                    let got = l.map(gray(v));
                    for shift in [16, 8, 0] {
                        assert!(
                            (got >> shift & 0xff).abs_diff(v) <= 1,
                            "{kind} {mode}: {v} became {got:06x}"
                        );
                    }
                }
                assert_ne!(l.map(0xff0000), 0xff0000, "{kind} {mode}");
            }
        }
        // red, decoded, is linear (1, 0, 0), and deuteranopia's first
        // column holds what the viewer sees of it
        let t = table(&[map(
            "color-blind",
            &[
                Given::Choice("deuteranopia"),
                Given::Choice("simulate"),
                n(1, 0),
            ],
        )]);
        let encode = |x: f64| {
            if x <= 0.003_130_8 {
                x * 12.92
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            }
        };
        let want = [0.367_322, 0.280_085, 0.0].map(|x: f64| encode(x));
        for (got, want) in node(&t, 32, 0, 0).into_iter().zip(want) {
            assert!(
                (f64::from(got) - want).abs() < 1.0 / 1024.0,
                "{got} against {want}"
            );
        }
    }

    // The filters run in the file's order, and each filter's output is
    // held to 0 to 1 before the next takes it: a brightness of 0.5 holds
    // white at 1, and a contrast of 0.5 then takes it to 0.75, not to 1.
    #[test]
    fn filters_run_in_order_and_each_output_is_held() {
        let brighter = map("contrast", &[n(1, 0), n(0, 5000)]);
        let flatter = map("contrast", &[n(0, 5000), n(0, 0)]);
        let t = table(&[brighter, flatter]);
        assert_eq!(node(&t, 32, 32, 32), [0.75; 3]);
        let other = table(&[flatter, brighter]);
        assert_eq!(node(&other, 32, 32, 32), [1.0; 3]);
        assert_ne!(t, other);
    }

    // Between its nodes the table is a straight line. A gamma of 1.2 stays
    // within one level of the exact curve at every gray, and a gamma of 2
    // is off by at most 12 levels, the figures the record measured.
    #[test]
    fn the_table_stays_near_the_exact_curve() {
        for (gamma, worst) in [((1, 2000), 1), ((2, 0), 12)] {
            let g = f64::from(gamma.0) + f64::from(gamma.1) / 10_000.0;
            let mut l = lookup(&table(&[map("gamma", &[n(gamma.0, gamma.1)])]));
            let mut most = 0;
            for v in 0..=255u32 {
                let exact = (f64::from(v) / 255.0).powf(1.0 / g) * 255.0;
                let got = f64::from(l.map(gray(v)) & 0xff);
                most = most.max((got - exact).abs().round() as u32);
            }
            assert!(most <= worst, "gamma {g}: {most} levels");
        }
    }

    // A channel rounds to the nearest of the 256 levels, a half rounding
    // up, as floor(x × 255 + 0.5) gives.
    #[test]
    fn a_channel_rounds_to_the_nearest_level() {
        for k in 0..255u32 {
            let k_f = k as f32;
            assert_eq!(level((k_f + 0.4) / 255.0), k, "{k}.4");
            assert_eq!(level((k_f + 0.6) / 255.0), k + 1, "{k}.6");
        }
        assert_eq!(level(-0.1), 0);
        assert_eq!(level(1.5), 255);
        assert_eq!(level(f32::NAN), 0);
    }

    // The GPU path's upload holds the same halves, each node's red, green
    // and blue with an alpha of 1, little-endian.
    #[test]
    fn the_upload_holds_the_same_halves_with_an_alpha_of_one() {
        let t = table(&[map("gamma", &[n(1, 8000)])]);
        let bytes = t.rgba16f().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(bytes.len(), 35_937 * 8);
        let (texels, rest) = bytes.as_chunks::<8>();
        assert!(rest.is_empty());
        for (texel, node) in texels.iter().zip(t.nodes()) {
            let h = |i: usize| u16::from_le_bytes([texel[i], texel[i + 1]]);
            assert_eq!([h(0), h(2), h(4)], *node);
            assert_eq!(h(6), 0x3c00);
        }
    }
}
