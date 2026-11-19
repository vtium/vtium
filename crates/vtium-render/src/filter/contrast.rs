//! `contrast`: multiplies each channel's distance from middle gray by the
//! contrast, then adds the brightness. A contrast above 1 spreads the tones
//! apart, and one below 1 pulls them toward gray.
//!
//! References:
//! - ADR-1304 D8: the filter, its parameters, their ranges and their defaults.

use super::{Declaration, Fixed, Given, Kind, Layer, Param, number};

pub(super) const DECLARATION: Declaration = Declaration {
    name: "contrast",
    layer: Layer::Color,
    help: "Scales each channel's distance from middle gray, then shifts it.",
    params: &[
        Param {
            name: "contrast",
            kind: Kind::Number {
                min: Fixed::of(0, 0),
                max: Fixed::of(2, 0),
            },
            default: Some(Given::Number(Fixed::of(1, 0))),
            help: "above 1 spreads the tones apart",
        },
        Param {
            name: "brightness",
            kind: Kind::Number {
                min: Fixed(-5000),
                max: Fixed::of(0, 5000),
            },
            default: Some(Given::Number(Fixed::of(0, 0))),
            help: "added to each channel",
        },
    ],
};

/// The filter with its parameters set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Contrast {
    contrast: f64,
    brightness: f64,
}

impl Contrast {
    /// The filter for `values`, the contrast and then the brightness.
    pub(super) fn of(values: &[Given]) -> Option<Contrast> {
        Some(Contrast {
            contrast: number(values.first())?,
            brightness: number(values.get(1))?,
        })
    }

    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        c.map(|x| (x - 0.5) * self.contrast + 0.5 + self.brightness)
    }
}
