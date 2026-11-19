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
//! A table and its clones share one set of nodes. The first lookup in a
//! table widens the nodes to 32-bit floats, and every later lookup in that
//! table or its clones reads the same widened nodes. So a head builds its
//! table once, and each of its displays costs only the color it remembers,
//! on the CPU path and the GPU path alike.
//!
//! References:
//! - ADR-1304 D2: the table's size, its nodes, how it is built and how each path reads it.

use std::collections::TryReserveError;
use std::sync::{Arc, OnceLock};

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

/// A look's color filters merged into one table. A clone shares the
/// table's nodes, so it costs no copy.
#[derive(Clone, Debug)]
pub struct Table {
    nodes: Arc<Nodes>,
}

/// The nodes a table and its clones share.
#[derive(Debug)]
struct Nodes {
    /// Each node's red, green and blue as half floats, red's index the
    /// fastest, then green's, then blue's, as a 3D texture lays its texels
    /// out.
    halves: Vec<[u16; 3]>,
    /// The same nodes as 32-bit floats, which the CPU path's lookup reads,
    /// made by the first lookup in the table.
    wide: OnceLock<Vec<[f32; 3]>>,
}

impl PartialEq for Table {
    fn eq(&self, other: &Table) -> bool {
        self.nodes.halves == other.nodes.halves
    }
}

impl Eq for Table {}

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
        Ok(Table {
            nodes: Arc::new(Nodes {
                halves: nodes,
                wide: OnceLock::new(),
            }),
        })
    }

    /// Each node's red, green and blue as half floats, in the texture's
    /// order: red's index the fastest, then green's, then blue's.
    pub fn nodes(&self) -> &[[u16; 3]] {
        &self.nodes.halves
    }

    /// The nodes as 32-bit floats, made at the first call and shared with
    /// every clone after it. The only failure is a want of memory.
    fn wide(&self) -> Result<&[[f32; 3]], TryReserveError> {
        if let Some(wide) = self.nodes.wide.get() {
            return Ok(wide);
        }
        let mut wide = Vec::new();
        wide.try_reserve_exact(self.nodes.halves.len())?;
        wide.extend(self.nodes.halves.iter().map(|n| n.map(from_half)));
        // another thread may have made them first; its copy is kept
        Ok(self.nodes.wide.get_or_init(|| wide))
    }

    /// The table as the GPU path uploads it: an RGBA16F texture 33 a side,
    /// each half little-endian, alpha 1. RGBA rather than RGB because
    /// OpenGL ES 3.0 filters RGBA16F linearly on every GPU.
    pub fn rgba16f(&self) -> Result<Vec<u8>, TryReserveError> {
        let mut out = Vec::new();
        out.try_reserve_exact(self.nodes.halves.len().saturating_mul(8))?;
        for &[r, g, b] in &self.nodes.halves {
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

/// The CPU path's lookup in a color table: the table, whose nodes it reads
/// as 32-bit floats, and the last color looked up.
#[derive(Clone, Debug)]
pub struct Lookup {
    table: Table,
    last: Option<(u32, u32)>,
}

impl Lookup {
    /// The lookup in `table`. The first lookup in a table widens its nodes
    /// to 32-bit floats, 421 KiB, which every later lookup in the table or
    /// its clones shares. The only failure is a want of memory for them.
    pub fn new(table: &Table) -> Result<Lookup, TryReserveError> {
        table.wide()?;
        Ok(Lookup {
            table: table.clone(),
            last: None,
        })
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
        // Lookup::new made the wide nodes, so they are always there
        let wide: &[[f32; 3]] = self.table.nodes.wide.get().map_or(&[], Vec::as_slice);
        let [(r, fr), (g, fg), (b, fb)] = [16, 8, 0].map(|shift| axis(px >> shift & 0xff));
        let node = |dr: usize, dg: usize, db: usize| -> [f32; 3] {
            let i = (b.saturating_add(db))
                .saturating_mul(SIDE)
                .saturating_add(g.saturating_add(dg))
                .saturating_mul(SIDE)
                .saturating_add(r.saturating_add(dr));
            wide.get(i).copied().unwrap_or_default()
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
    use super::super::{COLOR, Cube, Fixed, Given, Map, Shape, color};
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
            .map(|(name, m, _)| (*name, digest(&table(std::slice::from_ref(m)))))
            .collect();
        let want: Vec<(&str, u64)> = cases.iter().map(|(name, _, d)| (*name, *d)).collect();
        assert_eq!(got, want);
    }

    // Each filter's defaults make a map, but cube's, whose file has no
    // default, and gamma and contrast at their defaults change no color.
    #[test]
    fn gamma_and_contrast_at_their_defaults_are_the_identity() {
        let identity = table(&[]);
        for name in ["gamma", "contrast"] {
            let d = color(name).unwrap_or_else(|| panic!("{name}"));
            let m = Map::defaults(d).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(table(&[m]), identity, "{name}");
        }
        for d in COLOR {
            assert_eq!(Map::defaults(d).is_some(), d.name != "cube", "{}", d.name);
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
        let t = table(&[brighter.clone(), flatter.clone()]);
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
    // and blue with an alpha of 1, little-endian, for a table built from
    // a filter's formula and for one built from a .cube file's table, so
    // the two paths read the same nodes whatever made them.
    #[test]
    fn the_upload_holds_the_same_halves_with_an_alpha_of_one() {
        let swap = Map::cube(Arc::new(swap_cube(17)));
        for t in [table(&[map("gamma", &[n(1, 8000)])]), table(&[swap])] {
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

    /// A 3D table of `n` a side that swaps red and blue.
    fn swap_cube(n: usize) -> Cube {
        let last = (n - 1) as f64;
        let mut entries = Vec::new();
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    entries.push([b as f64 / last, g as f64 / last, r as f64 / last]);
                }
            }
        }
        Cube::new(Shape::Three(n), entries).unwrap_or_else(|| panic!("{n}"))
    }

    // A table of 33 a side passes each of its entries into the merged
    // table unchanged, since each node falls on an entry: every node holds
    // its entry's halves. The entries differ from node to node, so a
    // lookup that reads a neighbouring entry fails.
    #[test]
    fn a_cube_of_33_a_side_passes_into_the_table_unchanged() {
        let mut entries = Vec::new();
        for i in 0..33 * 33 * 33u32 {
            // three values with few bits, each different from its
            // neighbours along every axis
            let v = |k: u32| f64::from((i * k + k * k) % 257) / 256.0;
            entries.push([v(7), v(31), v(101)]);
        }
        let cube = Cube::new(Shape::Three(33), entries.clone()).unwrap_or_else(|| panic!("33"));
        let t = table(&[Map::cube(Arc::new(cube))]);
        for (i, (node, entry)) in t.nodes().iter().zip(&entries).enumerate() {
            assert_eq!(*node, entry.map(half), "node {i}");
        }
    }

    // An identity table returns the identity at 2, 17 and 65 a side, and
    // an identity cube after a tint changes nothing the tint gave.
    #[test]
    fn an_identity_cube_builds_the_identity_table() {
        let identity = |n: usize| {
            let last = (n - 1) as f64;
            let mut entries = Vec::new();
            for b in 0..n {
                for g in 0..n {
                    for r in 0..n {
                        entries.push([r as f64 / last, g as f64 / last, b as f64 / last]);
                    }
                }
            }
            Map::cube(Arc::new(
                Cube::new(Shape::Three(n), entries).unwrap_or_else(|| panic!("{n}")),
            ))
        };
        for n in [2, 17, 65] {
            assert_eq!(table(&[identity(n)]), table(&[]), "{n} a side");
        }
        let amber = map("tint", &[Given::Color([255, 176, 0]), n(1, 0)]);
        assert_eq!(
            table(&[amber.clone(), identity(33)]),
            table(&[amber]),
            "after a tint"
        );
    }

    // The warm table of ADR-1304 D14 scales each channel by a constant, so
    // the merged table takes it exactly: a mid gray becomes 0.5, 0.475 and
    // 0.425.
    #[test]
    fn the_warm_cube_scales_each_channel() {
        let mut entries = Vec::new();
        for b in [0.0, 0.85] {
            for g in [0.0, 0.95] {
                for r in [0.0, 1.0] {
                    entries.push([r, g, b]);
                }
            }
        }
        let cube = Cube::new(Shape::Three(2), entries).unwrap_or_else(|| panic!("warm"));
        let t = table(&[Map::cube(Arc::new(cube))]);
        assert_eq!(
            t.nodes()[(16 * SIDE + 16) * SIDE + 16],
            [0.5, 0.475, 0.425].map(half)
        );
        assert_eq!(
            node(&t, 32, 32, 32),
            [1.0, 0.95, 0.85].map(|x| from_half(half(x)))
        );
    }

    // The darken table of ADR-1304 D14, a 1D table of 5 points sampled
    // from a gamma of 0.8, gives the merged table of a gamma filter at 0.8
    // within the curve's own sampling: a straight line between 5 points
    // lies within 0.015 of the curve.
    #[test]
    fn the_darken_curve_follows_a_gamma_of_0_8() {
        let points = [0.0, 0.1768, 0.4204, 0.6986, 1.0].map(|x| [x; 3]);
        let cube = Cube::new(Shape::One(5), points.to_vec()).unwrap_or_else(|| panic!("darken"));
        let curve = table(&[Map::cube(Arc::new(cube))]);
        let gamma = table(&[map("gamma", &[n(0, 8000)])]);
        let mut worst = 0.0f32;
        for (a, b) in curve.nodes().iter().zip(gamma.nodes()) {
            for (x, y) in a.iter().zip(b) {
                worst = worst.max((from_half(*x) - from_half(*y)).abs());
            }
        }
        assert!(worst < 0.015, "{worst}");
        assert!(worst > 0.0, "the curve is not the formula");
    }

    // A cube's table is the same bytes on every machine, as each filter's
    // is: the blend uses only correctly rounded operations.
    #[test]
    fn a_cube_s_table_is_pinned_to_its_bytes() {
        let swap = table(&[Map::cube(Arc::new(swap_cube(17)))]);
        let curve = Cube::new(
            Shape::One(4),
            vec![
                [0.0, 0.1, 0.2],
                [0.3, 0.5, 0.2],
                [0.9, 0.6, 0.4],
                [1.0, 0.7, 1.0],
            ],
        )
        .unwrap_or_else(|| panic!("curve"));
        let curve = table(&[Map::cube(Arc::new(curve))]);
        assert_eq!(
            [digest(&swap), digest(&curve)],
            [0xdf17_bb36_45b1_b9eb, 0x6731_c4a0_4f36_1b71]
        );
    }
}
