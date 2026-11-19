//! `color-blind`: shows how a viewer with a color vision deficiency sees
//! the screen, or corrects the screen for such a viewer.
//!
//! The filter works in linear light. It decodes each channel from sRGB's
//! curve, applies the deficiency's matrix, and encodes the result again.
//! The matrices are Machado, Oliveira and Fernandes's, at full severity,
//! from "A Physiologically-based Model for Simulation of Color Vision
//! Deficiency" (IEEE TVCG, 2009).
//!
//! `simulate` shows the simulated color. `correct` takes the difference
//! between the color and its simulation, which the viewer cannot see, and
//! adds it to the channels the viewer can see, through the error matrix of
//! the daltonization method (Fidaner, Lin and Ozguven, 2005). For a red or
//! green deficiency the lost difference moves into green and blue, and for
//! a blue deficiency into red and green. `amount` mixes the result with the
//! input, in linear light.
//!
//! References:
//! - ADR-1304 D8: the filter, its parameters, Machado's matrices in linear light, and what correct does.

use super::math::{pow, unit};
use super::{Declaration, Fixed, Given, Kind, Layer, Param, number};

const KINDS: &[&str] = &["protanopia", "deuteranopia", "tritanopia"];
const MODES: &[&str] = &["simulate", "correct"];

pub(super) const DECLARATION: Declaration = Declaration {
    name: "color-blind",
    layer: Layer::Color,
    help: "Simulates a color vision deficiency, or corrects for it.",
    params: &[
        Param {
            name: "kind",
            kind: Kind::Choice(KINDS),
            default: Some(Given::Choice("deuteranopia")),
            help: "the deficiency",
        },
        Param {
            name: "mode",
            kind: Kind::Choice(MODES),
            default: Some(Given::Choice("correct")),
            help: "show it, or correct for it",
        },
        Param {
            name: "amount",
            kind: Kind::Number {
                min: Fixed::of(0, 0),
                max: Fixed::of(1, 0),
            },
            default: Some(Given::Number(Fixed::of(1, 0))),
            help: "how much of the result to show",
        },
    ],
};

type Matrix = [[f64; 3]; 3];

/// Machado et al.'s simulation of protanopia at severity 1.
const PROTANOPIA: Matrix = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];

/// Machado et al.'s simulation of deuteranopia at severity 1.
const DEUTERANOPIA: Matrix = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];

/// Machado et al.'s simulation of tritanopia at severity 1.
const TRITANOPIA: Matrix = [
    [1.255_528, -0.076_749, -0.178_779],
    [-0.078_411, 0.930_809, 0.147_602],
    [0.004_733, 0.691_367, 0.303_900],
];

/// Daltonization's error matrix for a red or green deficiency: the lost
/// difference in red moves into green and blue.
const SHIFT_RED_GREEN: Matrix = [[0.0, 0.0, 0.0], [0.7, 1.0, 0.0], [0.7, 0.0, 1.0]];

/// Daltonization's error matrix for a blue deficiency: the lost difference
/// in blue moves into red and green.
const SHIFT_BLUE: Matrix = [[1.0, 0.0, 0.7], [0.0, 1.0, 0.7], [0.0, 0.0, 0.0]];

/// The filter with its parameters set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ColorBlind {
    simulation: &'static Matrix,
    /// `None` to simulate, and the error matrix to correct.
    shift: Option<&'static Matrix>,
    amount: f64,
}

impl ColorBlind {
    /// The filter for `values`: the kind, the mode and the amount.
    pub(super) fn of(values: &[Given]) -> Option<ColorBlind> {
        let (Some(Given::Choice(kind)), Some(Given::Choice(mode))) =
            (values.first(), values.get(1))
        else {
            return None;
        };
        let (simulation, red_green) = match *kind {
            "protanopia" => (&PROTANOPIA, true),
            "deuteranopia" => (&DEUTERANOPIA, true),
            "tritanopia" => (&TRITANOPIA, false),
            _ => return None,
        };
        let shift = match *mode {
            "simulate" => None,
            "correct" if red_green => Some(&SHIFT_RED_GREEN),
            "correct" => Some(&SHIFT_BLUE),
            _ => return None,
        };
        Some(ColorBlind {
            simulation,
            shift,
            amount: number(values.get(2))?,
        })
    }

    pub(super) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        let linear = c.map(decode);
        let simulated = times(self.simulation, linear);
        let result = match self.shift {
            None => simulated,
            Some(shift) => {
                let lost = [0, 1, 2].map(|i| at(linear, i) - at(simulated, i));
                let moved = times(shift, lost);
                [0, 1, 2].map(|i| at(linear, i) + at(moved, i))
            }
        };
        [0, 1, 2].map(|i| {
            let x = at(linear, i);
            encode(unit(x + (at(result, i) - x) * self.amount))
        })
    }
}

/// The channel `i` of `c`.
fn at(c: [f64; 3], i: usize) -> f64 {
    c.get(i).copied().unwrap_or(0.0)
}

/// `m` times the column `c`.
fn times(m: &Matrix, c: [f64; 3]) -> [f64; 3] {
    m.map(|[a, b, d]| a * at(c, 0) + b * at(c, 1) + d * at(c, 2))
}

/// An sRGB-encoded channel in linear light, by sRGB's curve.
pub(super) fn decode(x: f64) -> f64 {
    if x <= 0.040_45 {
        x / 12.92
    } else {
        pow((x + 0.055) / 1.055, 2.4)
    }
}

/// A channel in linear light, encoded by sRGB's curve.
pub(super) fn encode(x: f64) -> f64 {
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * pow(x, 1.0 / 2.4) - 0.055
    }
}
