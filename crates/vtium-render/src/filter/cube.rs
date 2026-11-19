//! `cube`: maps each color through a table read from a `.cube` file, the
//! format that color grading and calibration tools write.
//!
//! A `.cube` file holds either a 3D table or a 1D table. A 3D table of N
//! a side holds N × N × N output colors, one for each input color whose
//! channels are whole steps of 1 / (N − 1), with red's index changing
//! fastest, then green's, then blue's. vtium blends the eight entries
//! around a color trilinearly: along red first, then green, then blue. A
//! 1D table of N entries holds one curve for each channel, and vtium blends
//! the two entries around each channel's value linearly, each channel on
//! its own.
//!
//! The merged color table samples this filter at its 33 nodes, as it
//! samples every color filter. A node falls exactly on an entry when the
//! entry's step divides the node's, so a 3D table of 33 a side, or of 17 or
//! 65, passes its entries into the merged table unchanged, and a larger
//! table is resampled to 33. A blend weight of 0 adds `0 × (b − a)` to `a`,
//! which IEEE 754 does exactly, so an entry the node falls on is not
//! touched by its neighbor.
//!
//! The file's values are read and checked by vtium-settings, which holds
//! the `.cube` reader. [`Cube`] holds the values once read, each within
//! 0 to 1, and refuses a size outside the bounds.
//!
//! References:
//! - ADR-1304 D9: the filter, its sizes, the clamp to 0 to 1, the trilinear and linear blends, and the resampling to 33.
//! - ADR-1304 D2: the merged table built from correctly rounded operations.

use std::ops::RangeInclusive;

use super::{Declaration, Kind, Layer, Param};

/// The sides a 3D table may have, `LUT_3D_SIZE`.
pub const SIDES_3D: RangeInclusive<usize> = 2..=65;

/// The entries a 1D table may have, `LUT_1D_SIZE`.
pub const SIZES_1D: RangeInclusive<usize> = 2..=4096;

pub(super) const DECLARATION: Declaration = Declaration {
    name: "cube",
    layer: Layer::Color,
    help: "Maps each color through a table read from a .cube file.",
    params: &[Param {
        name: "file",
        kind: Kind::File(&[".cube"]),
        default: None,
        help: "the color table it reads",
    }],
};

/// Whether a table is 3D or 1D, with its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A 3D table of this many entries a side.
    Three(usize),
    /// A 1D table of this many entries.
    One(usize),
}

/// A `.cube` table once read: its shape and its entries, each channel
/// within 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    shape: Shape,
    /// The entries in the file's order: for a 3D table red's index
    /// fastest, then green's, then blue's.
    entries: Vec<[f64; 3]>,
}

impl Cube {
    /// The table of shape `shape` with `entries` in the file's order.
    /// Each channel is held to 0 to 1, and a channel that is not a number
    /// becomes 0. `None` when the size is outside [`SIDES_3D`] or
    /// [`SIZES_1D`], or when `entries` does not hold one entry for each
    /// place of the table.
    pub fn new(shape: Shape, entries: Vec<[f64; 3]>) -> Option<Cube> {
        let count = match shape {
            Shape::Three(n) if SIDES_3D.contains(&n) => n.checked_pow(3)?,
            Shape::One(n) if SIZES_1D.contains(&n) => n,
            _ => return None,
        };
        if entries.len() != count {
            return None;
        }
        let entries = entries
            .into_iter()
            .map(|e| e.map(|x| if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }))
            .collect();
        Some(Cube { shape, entries })
    }

    /// The table's shape.
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// The color `c` maps to: the blend of the entries around it.
    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        match self.shape {
            Shape::Three(n) => self.three(n, c),
            Shape::One(n) => {
                let mut out = [0.0; 3];
                for (ch, (o, x)) in out.iter_mut().zip(c).enumerate() {
                    let (i, j, t) = place(n, x);
                    *o = lerp(self.channel(i, ch), self.channel(j, ch), t);
                }
                out
            }
        }
    }

    /// The trilinear blend in a 3D table of `n` a side.
    fn three(&self, n: usize, c: [f64; 3]) -> [f64; 3] {
        let [r, g, b] = c.map(|x| place(n, x));
        let at = |ri: usize, gi: usize, bi: usize| -> usize {
            bi.saturating_mul(n)
                .saturating_add(gi)
                .saturating_mul(n)
                .saturating_add(ri)
        };
        let mut out = [0.0; 3];
        for (ch, o) in out.iter_mut().enumerate() {
            let e = |ri, gi, bi| self.channel(at(ri, gi, bi), ch);
            let along_r = |gi, bi| lerp(e(r.0, gi, bi), e(r.1, gi, bi), r.2);
            let along_g = |bi| lerp(along_r(g.0, bi), along_r(g.1, bi), g.2);
            *o = lerp(along_g(b.0), along_g(b.1), b.2);
        }
        out
    }

    /// Channel `ch` of entry `i`, 0 past the table's end, which `place`
    /// never reaches.
    fn channel(&self, i: usize, ch: usize) -> f64 {
        self.entries
            .get(i)
            .and_then(|e| e.get(ch))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Where a channel `x` lies among `n` entries: the entry at or below it,
/// the entry above it, and how far toward the one above, 0 to 1. At the
/// last entry both are the last entry and the weight is 0. A channel
/// outside 0 to 1, or not a number, is held to the range first.
fn place(n: usize, x: f64) -> (usize, usize, f64) {
    let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
    let last = n.saturating_sub(1);
    let steps = u32::try_from(last).unwrap_or(u32::MAX);
    let p = x * f64::from(steps);
    let below = p.floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "below is a whole number from 0 to n - 1"
    )]
    let i = (below as usize).min(last);
    (i, i.saturating_add(1).min(last), p - below)
}

/// `a` toward `b` by `t`: a multiply and an add, rounded apart. At a `t`
/// of 0 it gives `a` exactly.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
#[expect(
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "a test fails by panicking, and its figures are small"
)]
mod tests {
    use super::*;

    /// The identity table of `n` a side.
    fn identity(n: usize) -> Cube {
        let last = (n - 1) as f64;
        let mut entries = Vec::new();
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    entries.push([r as f64 / last, g as f64 / last, b as f64 / last]);
                }
            }
        }
        Cube::new(Shape::Three(n), entries).unwrap_or_else(|| panic!("{n}"))
    }

    // The sizes outside the bounds, and a count of entries that does not
    // fill the table, make no table.
    #[test]
    fn a_table_outside_its_bounds_is_refused() {
        assert!(Cube::new(Shape::Three(1), vec![[0.0; 3]]).is_none());
        assert!(Cube::new(Shape::Three(66), vec![[0.0; 3]; 66 * 66 * 66]).is_none());
        assert!(Cube::new(Shape::Three(2), vec![[0.0; 3]; 7]).is_none());
        assert!(Cube::new(Shape::One(4097), vec![[0.0; 3]; 4097]).is_none());
        assert!(Cube::new(Shape::One(1), vec![[0.0; 3]]).is_none());
        assert!(Cube::new(Shape::One(2), vec![[0.0; 3]; 2]).is_some());
        assert!(Cube::new(Shape::Three(65), vec![[0.0; 3]; 65 * 65 * 65]).is_some());
    }

    // A value outside 0 to 1 is held to the range, and one that is not a
    // number becomes 0.
    #[test]
    fn each_value_is_held_to_0_to_1() {
        let c = Cube::new(Shape::One(2), vec![[-0.5, f64::NAN, 0.25], [1.5, 1.0, 2.0]])
            .unwrap_or_else(|| panic!("made"));
        assert_eq!(c.apply([0.0; 3]), [0.0, 0.0, 0.25]);
        assert_eq!(c.apply([1.0; 3]), [1.0, 1.0, 1.0]);
    }

    // An identity table maps each of the merged table's nodes to itself,
    // exactly, at 2, 17, 33 and 65 a side.
    #[test]
    fn an_identity_table_maps_each_node_to_itself() {
        for n in [2, 17, 33, 65] {
            let cube = identity(n);
            for v in 0..=32u32 {
                let x = f64::from(v) / 32.0;
                let c = [x, 1.0 - x, f64::from(v % 7) / 32.0];
                assert_eq!(cube.apply(c), c, "{n} a side, {v}");
            }
        }
    }

    // The blend runs along red, then green, then blue, from the entries
    // around the color: halfway between black and red's corner in a table
    // of 2 a side is half that corner's value.
    #[test]
    fn a_3d_table_blends_its_eight_entries() {
        let mut entries = vec![[0.0; 3]; 8];
        // the corner for red, (1, 0, 0), maps to (0.5, 0.25, 1)
        entries[1] = [0.5, 0.25, 1.0];
        let cube = Cube::new(Shape::Three(2), entries).unwrap_or_else(|| panic!("made"));
        assert_eq!(cube.apply([1.0, 0.0, 0.0]), [0.5, 0.25, 1.0]);
        assert_eq!(cube.apply([0.5, 0.0, 0.0]), [0.25, 0.125, 0.5]);
        assert_eq!(cube.apply([1.0, 0.5, 0.0]), [0.25, 0.125, 0.5]);
        assert_eq!(cube.apply([0.0, 1.0, 1.0]), [0.0; 3]);
    }

    // A 1D table is one curve for each channel, blended linearly.
    #[test]
    fn a_1d_table_is_a_curve_for_each_channel() {
        let cube = Cube::new(
            Shape::One(3),
            vec![[0.0, 1.0, 0.0], [0.25, 0.5, 0.5], [1.0, 0.0, 1.0]],
        )
        .unwrap_or_else(|| panic!("made"));
        assert_eq!(cube.apply([0.5, 0.5, 0.25]), [0.25, 0.5, 0.25]);
        assert_eq!(cube.apply([0.75, 0.0, 1.0]), [0.625, 1.0, 1.0]);
    }
}
