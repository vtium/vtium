//! Frame filters: the declarations of the color filters vtium builds in.
//!
//! A frame filter is a step that changes the finished frame before the
//! display shows it. A color filter maps each pixel's color to a new color
//! using that color alone, and a spatial filter reads the pixels around the
//! one it writes. A vtlook (a look file) chooses filters and sets their
//! parameters, and vtium-settings reads it against the declarations here.
//!
//! A declaration names its filter, its layer and its parameters, and gives
//! each parameter a kind, a range, a default and a line of help. That one
//! declaration drives the vtlook reader, `vtium check-config`, vtium-config's
//! display of a filter, and the work that builds the color table, so a
//! parameter is described in one place only.
//!
//! [`COLOR`] is the one list of the color filters: `gamma`, `contrast`,
//! `tint` and `color-blind`. Each has a module of its own, which holds its
//! declaration beside its function. A [`Map`] is one color filter with its
//! parameters set, and [`Table`] merges a look's maps into one color
//! table, which the CPU path reads through a [`Lookup`] and the GPU path
//! uploads as a texture.
//!
//! References:
//! - ADR-1304 D1: a filter's declaration and the kinds a parameter takes.
//! - ADR-1304 D2: the color layer's contract, and the one table it merges into.
//! - ADR-1304 D8: the first color filters.
//! - ADR-1304 D10: the color filters live here, behind the `filters` feature.

mod color_blind;
mod contrast;
mod gamma;
mod math;
mod table;
mod tint;

pub use table::{Lookup, Table};

/// The layer a filter runs in. The spatial filters run first, then the
/// display's turn, then the color filters.
///
/// References:
/// - ADR-1303 D1: the order of the slots the two layers fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// Maps each pixel's color using that color alone. Both the CPU path
    /// and the GPU path run it, through one table.
    Color,
    /// Writes each pixel from the pixels near it, on the GPU path only.
    Spatial,
}

impl Layer {
    /// The word a vtlook's section heading names the layer by, as in
    /// `[color gamma]`.
    pub const fn word(self) -> &'static str {
        match self {
            Layer::Color => "color",
            Layer::Spatial => "spatial",
        }
    }

    /// The layer a section heading's word names, `None` for any other
    /// word.
    pub fn of_word(word: &str) -> Option<Layer> {
        [Layer::Color, Layer::Spatial]
            .into_iter()
            .find(|l| l.word() == word)
    }
}

/// A number of a `number` parameter, in ten-thousandths: a vtlook writes a
/// number with at most four digits after the point, so this holds every
/// value exactly, and two machines read the same file to the same value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(pub i32);

impl Fixed {
    /// The ten-thousandths in one.
    pub const ONE: i32 = 10_000;

    /// The number `whole` plus `fraction` ten-thousandths, for a
    /// declaration: `Fixed::of(0, 3000)` is 0.3.
    pub const fn of(whole: i32, fraction: i32) -> Fixed {
        Fixed(whole.saturating_mul(Fixed::ONE).saturating_add(fraction))
    }
}

impl std::fmt::Display for Fixed {
    /// Writes the number in its shortest form, as `2`, `0.5` or `-0.25`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let n = self.0.unsigned_abs();
        let one = Fixed::ONE.unsigned_abs();
        let whole = n.checked_div(one).unwrap_or(0);
        let fraction = n.checked_rem(one).unwrap_or(0);
        if fraction == 0 {
            return write!(f, "{sign}{whole}");
        }
        let digits = format!("{fraction:04}");
        write!(f, "{sign}{whole}.{}", digits.trim_end_matches('0'))
    }
}

/// The kind of value a parameter takes, with its range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A decimal with an optional sign and at most four digits after the
    /// point, from `min` to `max` inclusive.
    Number { min: Fixed, max: Fixed },
    /// A whole number from `min` to `max` inclusive.
    Integer { min: i64, max: i64 },
    /// A color, `r,g,b`, each channel 0 to 255.
    Color,
    /// One word of the list.
    Choice(&'static [&'static str]),
    /// `yes` or `no`.
    Switch,
    /// The name of a file in the looks directory, with one of these
    /// suffixes and no path.
    File(&'static [&'static str]),
}

/// A parameter's default, of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Given {
    Number(Fixed),
    Integer(i64),
    Color([u8; 3]),
    /// The word, which the kind's list holds.
    Choice(&'static str),
    Switch(bool),
}

/// One parameter of a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    /// Lowercase letters, digits and `-`, unique within its filter.
    pub name: &'static str,
    pub kind: Kind,
    /// `None` when the parameter has no default, so a vtlook that names the
    /// filter must set it.
    pub default: Option<Given>,
    /// One line that says what the parameter does.
    pub help: &'static str,
}

/// A filter's declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// Lowercase letters, digits and `-`, unique within its layer.
    pub name: &'static str,
    pub layer: Layer,
    /// One line that says what the filter does.
    pub help: &'static str,
    pub params: &'static [Param],
}

impl Declaration {
    /// The parameter named `name`.
    pub fn param(&self, name: &str) -> Option<&'static Param> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// The color filters vtium builds in, in the order a template lists them.
pub const COLOR: &[&Declaration] = &[
    &gamma::DECLARATION,
    &contrast::DECLARATION,
    &tint::DECLARATION,
    &color_blind::DECLARATION,
];

/// One color filter with its parameters set: the function from one color to
/// another that the table is built from. A color filter reads only the
/// color it maps, never the pixel's place, its cell or the time, so the
/// whole layer can be a table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map(Step);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Gamma(gamma::Gamma),
    Contrast(contrast::Contrast),
    Tint(tint::Tint),
    ColorBlind(color_blind::ColorBlind),
}

impl Map {
    /// The color filter `declaration` with `values`, one for each of its
    /// parameters in the declaration's order. `None` when `declaration` is
    /// not one of [`COLOR`], or when a value is not of its parameter's
    /// kind. The vtlook reader has already checked each value's range.
    pub fn new(declaration: &Declaration, values: &[Given]) -> Option<Map> {
        if values.len() != declaration.params.len() {
            return None;
        }
        let step = match declaration.name {
            "gamma" => Step::Gamma(gamma::Gamma::of(values)?),
            "contrast" => Step::Contrast(contrast::Contrast::of(values)?),
            "tint" => Step::Tint(tint::Tint::of(values)?),
            "color-blind" => Step::ColorBlind(color_blind::ColorBlind::of(values)?),
            _ => return None,
        };
        Some(Map(step))
    }

    /// The color filter `declaration` with every parameter at its default,
    /// `None` when a parameter has none.
    pub fn defaults(declaration: &Declaration) -> Option<Map> {
        let values: Option<Vec<Given>> = declaration.params.iter().map(|p| p.default).collect();
        Map::new(declaration, &values?)
    }

    /// The color `c` maps to, each channel from 0 to 1. The result may fall
    /// outside 0 to 1, and the table holds it there.
    pub(crate) fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        match &self.0 {
            Step::Gamma(f) => f.apply(c),
            Step::Contrast(f) => f.apply(c),
            Step::Tint(f) => f.apply(c),
            Step::ColorBlind(f) => f.apply(c),
        }
    }
}

/// A number parameter's value as a float, `None` for another kind.
fn number(value: Option<&Given>) -> Option<f64> {
    match value? {
        Given::Number(n) => Some(f64::from(n.0) / f64::from(Fixed::ONE)),
        _ => None,
    }
}

/// The color filter named `name`.
pub fn color(name: &str) -> Option<&'static Declaration> {
    COLOR.iter().copied().find(|d| d.name == name)
}

/// Whether `name` is a filter's or a parameter's name: one or more
/// lowercase letters, digits and `-`.
pub fn good_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// What is wrong with a declaration, for the test that checks every
/// declaration of a list: `None` when nothing is.
pub fn fault(d: &Declaration) -> Option<String> {
    if !good_name(d.name) {
        return Some(format!("filter name {:?}", d.name));
    }
    for (i, p) in d.params.iter().enumerate() {
        if !good_name(p.name) {
            return Some(format!("{}: parameter name {:?}", d.name, p.name));
        }
        if d.params
            .iter()
            .skip(i.saturating_add(1))
            .any(|q| q.name == p.name)
        {
            return Some(format!("{}: parameter {} twice", d.name, p.name));
        }
        if let Some(why) = default_fault(p) {
            return Some(format!("{}: {}: {why}", d.name, p.name));
        }
    }
    None
}

/// What is wrong with a parameter's range or its default.
fn default_fault(p: &Param) -> Option<&'static str> {
    let fits = match (p.kind, p.default) {
        (Kind::Number { min, max }, _) if min > max => return Some("range out of order"),
        (Kind::Integer { min, max }, _) if min > max => return Some("range out of order"),
        (Kind::Choice([]), _) => return Some("no choices"),
        (_, None) => true,
        (Kind::Number { min, max }, Some(Given::Number(n))) => (min..=max).contains(&n),
        (Kind::Integer { min, max }, Some(Given::Integer(n))) => (min..=max).contains(&n),
        (Kind::Color, Some(Given::Color(_))) | (Kind::Switch, Some(Given::Switch(_))) => true,
        (Kind::Choice(list), Some(Given::Choice(w))) => list.contains(&w),
        _ => return Some("default of another kind"),
    };
    (!fits).then_some("default out of range")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every declaration of the list is well formed: its names, its ranges
    // and its defaults, and each makes a map at its defaults. The check is
    // also run on declarations made here, to show it catches a fault.
    #[test]
    fn each_declaration_is_well_formed() {
        for d in COLOR {
            assert_eq!(fault(d), None);
            assert_eq!(d.layer, Layer::Color, "{}", d.name);
            assert!(Map::defaults(d).is_some(), "{}", d.name);
        }
        let names: Vec<&str> = COLOR.iter().map(|d| d.name).collect();
        assert_eq!(names, ["gamma", "contrast", "tint", "color-blind"]);
        const GOOD: Declaration = Declaration {
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
        assert_eq!(fault(&GOOD), None);
        const OUT_OF_RANGE: &[Param] = &[Param {
            name: "gamma",
            kind: Kind::Number {
                min: Fixed::of(0, 5000),
                max: Fixed::of(2, 0),
            },
            default: Some(Given::Number(Fixed::of(3, 0))),
            help: "",
        }];
        let mut bad = GOOD;
        bad.params = OUT_OF_RANGE;
        assert_eq!(
            fault(&bad).as_deref(),
            Some("gamma: gamma: default out of range")
        );
        bad.name = "Gamma";
        assert!(fault(&bad).is_some());
    }

    #[test]
    fn a_number_is_written_in_its_shortest_form() {
        let cases = [
            (Fixed::of(2, 0), "2"),
            (Fixed::of(0, 5000), "0.5"),
            (Fixed::of(1, 1500), "1.15"),
            (Fixed(-2500), "-0.25"),
            (Fixed(1), "0.0001"),
            (Fixed(0), "0"),
        ];
        for (n, text) in cases {
            assert_eq!(n.to_string(), text);
        }
    }

    #[test]
    fn a_layer_is_named_by_its_word() {
        assert_eq!(Layer::of_word("color"), Some(Layer::Color));
        assert_eq!(Layer::of_word("spatial"), Some(Layer::Spatial));
        assert_eq!(Layer::of_word("Color"), None);
        assert_eq!(color("gamma").map(|d| d.name), Some("gamma"));
        assert_eq!(color("sepia"), None);
    }
}
