//! `tint`: turns the screen toward one color, as a single-color phosphor
//! does. Each pixel moves, by the amount, toward its gray times the color.
//! At an amount of 1 the screen shows only shades of the color, amber by
//! default.
//!
//! The gray is BT.601's luma, with the weights 77, 150 and 29 out of 256
//! that the 1-bit path's cut uses, so a tinted screen and a 1-bit panel
//! agree on how light each color is.
//!
//! References:
//! - ADR-1304 D8: the filter, its parameters and the luma it takes from mono.rs.

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
    ],
};

/// The filter with its parameters set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tint {
    color: [f64; 3],
    amount: f64,
}

impl Tint {
    /// The filter for `values`, the color and then the amount.
    pub(super) fn of(values: &[Given]) -> Option<Tint> {
        let Some(Given::Color(rgb)) = values.first() else {
            return None;
        };
        Some(Tint {
            color: rgb.map(|c| f64::from(c) / 255.0),
            amount: number(values.get(1))?,
        })
    }

    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        let [r, g, b] = c;
        let gray = (77.0 * r + 150.0 * g + 29.0 * b) / 256.0;
        let mut out = c;
        for (x, tint) in out.iter_mut().zip(self.color) {
            *x += (gray * tint - *x) * self.amount;
        }
        out
    }
}
