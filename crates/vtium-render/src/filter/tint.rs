//! `tint`: turns the screen toward one color, as a single-color phosphor
//! does. Each pixel moves, by the amount, toward its gray times the color.
//! At an amount of 1 the screen shows only shades of the color, amber by
//! default.
//!
//! The gray is BT.601's luma, with the weights 77, 150 and 29 out of 256
//! that the 1-bit path's cut uses, so a tinted screen and a 1-bit panel
//! agree on how light each color is.
//!
//! A single-color phosphor shows only how light a color is, and blue is
//! dark, so a blue that a program draws on a black background is hard to
//! see under the tint. The floor lifts every gray above black to
//! `floor + gray × (1 − floor)` before the tint applies its color, so a
//! dark color rises toward the floor while black stays black and white
//! keeps the phosphor's full color. The order of the grays never changes,
//! and neither does the hue. At the default floor of 0 the lift adds 0 and
//! multiplies by 1, which IEEE 754 does exactly, so the table is the bytes
//! it is with no floor.
//!
//! References:
//! - ADR-1304 D8: the filter, its parameters and the luma it takes from mono.rs.
//! - ADR-1304 D21: the floor, its formula, its default of 0 and the figures its test checks.

use super::{Declaration, Fixed, Given, Kind, Layer, Param, number};

pub(super) const DECLARATION: Declaration = Declaration {
    name: "tint",
    layer: Layer::Color,
    help: "Turns the screen toward one color, as a single-color phosphor.",
    params: &[
        Param {
            name: "color",
            kind: Kind::Color,
            default: Some(Given::Color([255, 176, 0])),
            help: "the phosphor's color",
        },
        Param {
            name: "amount",
            kind: Kind::Number {
                min: Fixed::of(0, 0),
                max: Fixed::of(1, 0),
            },
            default: Some(Given::Number(Fixed::of(1, 0))),
            help: "how far toward it",
        },
        Param {
            name: "floor",
            kind: Kind::Number {
                min: Fixed::of(0, 0),
                max: Fixed::of(1, 0),
            },
            default: Some(Given::Number(Fixed::of(0, 0))),
            help: "how far dark colors rise above black",
        },
    ],
};

/// The filter with its parameters set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tint {
    color: [f64; 3],
    amount: f64,
    floor: f64,
}

impl Tint {
    /// The filter for `values`: the color, the amount and the floor.
    pub(super) fn of(values: &[Given]) -> Option<Tint> {
        let Some(Given::Color(rgb)) = values.first() else {
            return None;
        };
        Some(Tint {
            color: rgb.map(|c| f64::from(c) / 255.0),
            amount: number(values.get(1))?,
            floor: number(values.get(2))?,
        })
    }

    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        let [r, g, b] = c;
        let gray = (77.0 * r + 150.0 * g + 29.0 * b) / 256.0;
        // black stays black, so a black background stays black
        let gray = if gray > 0.0 {
            self.floor + gray * (1.0 - self.floor)
        } else {
            gray
        };
        let mut out = c;
        for (x, tint) in out.iter_mut().zip(self.color) {
            *x += (gray * tint - *x) * self.amount;
        }
        out
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::super::{Fixed, Given, Lookup, Map, Table, color};

    fn n(whole: i32, fraction: i32) -> Given {
        Given::Number(Fixed::of(whole, fraction))
    }

    fn table(values: &[Given]) -> Table {
        let d = color("tint").unwrap_or_else(|| panic!("tint"));
        let m = Map::new(d, values).unwrap_or_else(|| panic!("{values:?}"));
        Table::build(&[m]).unwrap_or_else(|e| panic!("{e}"))
    }

    /// FNV-1a over the table's halves, little-endian, in the texture's
    /// order, as table.rs's pins take it.
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

    // A floor of 0 changes no byte: the table is the one table.rs pins for
    // the tint's defaults, which it pinned before the floor existed, and
    // the one the floor's default gives.
    #[test]
    fn a_floor_of_0_leaves_the_table_byte_for_byte() {
        const AMBER_DIGEST: u64 = 0xc1dd_bebd_0111_b927;
        let amber = Given::Color([255, 176, 0]);
        assert_eq!(digest(&table(&[amber, n(1, 0), n(0, 0)])), AMBER_DIGEST);
        assert_eq!(digest(&table(&[amber, n(1, 0)])), AMBER_DIGEST);
        let d = color("tint").unwrap_or_else(|| panic!("tint"));
        let defaults = Map::defaults(d).unwrap_or_else(|| panic!("tint"));
        assert_eq!(
            digest(&Table::build(&[defaults]).unwrap_or_else(|e| panic!("{e}"))),
            AMBER_DIGEST
        );
    }

    // At a floor of 0.3 under amber, the legacy palette's dark blue and red
    // rise to readable shades of amber, 95,66,0 and 120,83,0, and blue stays
    // the darker of the two. The light gray moves a little, white keeps the
    // full amber and black stays black. The test also pins the digest of the
    // whole table at this setting, so any change to the floor's arithmetic
    // shows up even where none of the five colors moves.
    //
    // References:
    // - ADR-1304 D21: the five colors and their values at a floor of 0.3.
    // - ADR-1304 D2: each filter's table pinned by digest at one setting that is not its default.
    #[test]
    fn a_floor_of_0_3_lifts_dark_colors_and_keeps_black_and_white() {
        let t = table(&[Given::Color([255, 176, 0]), n(1, 0), n(0, 3000)]);
        let mut l = Lookup::new(&t).unwrap_or_else(|e| panic!("{e}"));
        let mut rgb = |px: u32| {
            let got = l.map(px);
            [got >> 16 & 0xff, got >> 8 & 0xff, got & 0xff]
        };
        assert_eq!(rgb(0x0000ee), [95, 66, 0], "legacy blue");
        assert_eq!(rgb(0xcd0000), [120, 83, 0], "legacy red");
        assert_eq!(rgb(0xe5e5e5), [237, 163, 0], "legacy light gray");
        assert_eq!(rgb(0xffffff), [255, 176, 0], "white");
        assert_eq!(rgb(0x000000), [0, 0, 0], "black");
        assert_eq!(digest(&t), 0x5fb9_cc9e_ecce_9342);
    }
}
