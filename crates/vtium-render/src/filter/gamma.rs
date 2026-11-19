//! `gamma`: raises each channel to the power 1 / gamma, so that a gamma
//! above 1 lightens the middle tones and one below 1 darkens them. Black
//! and white stay where they are.
//!
//! References:
//! - ADR-1304 D8: the filter, its parameter, its range and its default.

use super::{Declaration, Fixed, Given, Kind, Layer, Param, number};

pub(super) const DECLARATION: Declaration = Declaration {
    name: "gamma",
    layer: Layer::Color,
    help: "Raises each channel to the power 1 / gamma.",
    params: &[Param {
        name: "gamma",
        kind: Kind::Number {
            min: Fixed::of(0, 5000),
            max: Fixed::of(2, 0),
        },
        default: Some(Given::Number(Fixed::of(1, 0))),
        help: "above 1 lightens the middle tones",
    }],
};

/// The filter with its parameter set: the power each channel is raised to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Gamma {
    power: f64,
}

impl Gamma {
    /// The filter for `values`, its one parameter's value.
    pub(super) fn of(values: &[Given]) -> Option<Gamma> {
        let gamma = number(values.first())?;
        (gamma > 0.0).then(|| Gamma { power: 1.0 / gamma })
    }

    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        c.map(|x| super::math::pow(x, self.power))
    }
}
