//! Any input as a vtlook, read against vtium's own filters' declarations
//! or against a fixed list that declares a parameter of every kind.
//!
//! A vtlook is a file root installs and any user can hand to `vtium
//! check-config`, and the console reads it at start, so its reader must
//! never panic. The first byte chooses the declarations. vtium's own list
//! holds the color filters, and a vtlook that reads cleanly against it
//! goes on to build its color table, which must succeed, since the reader
//! has checked every value it builds from. A `cube` filter is the one
//! exception: the target gives it no batch to read its file through, so
//! the file counts as missing at the reading stage. The fixed list lets the input
//! reach every value parser: numbers with a sign and a fraction, integers,
//! colors, choices, switches, file names, and a parameter with no default.
//!
//! The target also checks that the reader's answer is consistent. A
//! vtlook that reads cleanly comes with no error, and each filter it holds
//! has one value for each declared parameter, of the parameter's kind and
//! within its range. One that fails names at least one error, keeps at
//! most `ERRORS_MAX` of them, and each is about the file it was given, on
//! a line the file has.
//!
//! References:
//! - ADR-1304 D1: a filter's declaration and its parameters' kinds.
//! - ADR-1304 D2: a look's color filters build one color table.
//! - ADR-1304 D4: the vtlook's syntax and bounds.
//! - ADR-1304 D6: any error fails the whole vtlook, and each report names its file and line.
#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use vtium_render::filter::{Declaration, Fixed, Given, Kind, Layer, Param};
use vtium_settings::ERRORS_MAX;
use vtium_settings::vtlook::{Value, read_text, read_text_with};

/// A filter with a parameter of every kind, a negative range among them.
const EVERY: Declaration = Declaration {
    name: "every",
    layer: Layer::Color,
    help: "A parameter of every kind.",
    params: &[
        Param {
            name: "number",
            kind: Kind::Number {
                min: Fixed::of(-2, 0),
                max: Fixed::of(2, 5000),
            },
            default: Some(Given::Number(Fixed::of(1, 0))),
            help: "a decimal",
        },
        Param {
            name: "integer",
            kind: Kind::Integer { min: -8, max: 8 },
            default: Some(Given::Integer(3)),
            help: "a whole number",
        },
        Param {
            name: "color",
            kind: Kind::Color,
            default: Some(Given::Color([255, 176, 0])),
            help: "a color",
        },
        Param {
            name: "choice",
            kind: Kind::Choice(&["warm", "cold"]),
            default: Some(Given::Choice("warm")),
            help: "a word",
        },
        Param {
            name: "switch",
            kind: Kind::Switch,
            default: Some(Given::Switch(true)),
            help: "yes or no",
        },
        Param {
            name: "file",
            kind: Kind::File(&[".cube"]),
            default: None,
            help: "a file with no default",
        },
    ],
};

/// A second filter, whose one parameter has a default, so a section of
/// it alone can read cleanly.
const GAMMA: Declaration = Declaration {
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

const FIXED: &[&Declaration] = &[&EVERY, &GAMMA];

/// Whether `value` is of the kind `kind` and within its range.
fn fits(kind: Kind, value: &Value) -> bool {
    match (kind, value) {
        (Kind::Number { min, max }, Value::Number(n)) => (min..=max).contains(n),
        (Kind::Integer { min, max }, Value::Integer(n)) => (min..=max).contains(n),
        (Kind::Color, Value::Color(_)) | (Kind::Switch, Value::Switch(_)) => true,
        (Kind::Choice(words), Value::Choice(w)) => words.contains(w),
        (Kind::File(suffixes), Value::File(name)) => {
            !name.contains('/') && suffixes.iter().any(|s| name.ends_with(s))
        }
        _ => false,
    }
}

fuzz_target!(|data: &[u8]| {
    let (fixed, data) = match data.split_first() {
        Some((first, rest)) => (first & 1 == 1, rest),
        None => (false, data),
    };
    let file = Path::new("/etc/vtium/looks/fuzz.vtlook");
    let lines = data.split(|&b| b == b'\n').count();
    let read = if fixed {
        read_text_with("fuzz", file, data, FIXED)
    } else {
        read_text("fuzz", file, data)
    };
    match read {
        Ok(look) => {
            assert_eq!(look.name, "fuzz");
            if !fixed {
                // vtium's own color filters build their table from any
                // values the reader accepts, and it has no spatial filter.
                // A cube filter reads a file, which this call is given no
                // gate to read, so it counts as missing at the reading
                // stage and names the .cube file.
                assert!(look.spatial.is_empty());
                if !look.color.is_empty() {
                    let built = vtium_settings::vtlook::table(&look);
                    let cube = look.color.iter().any(|f| f.declaration.name == "cube");
                    match built {
                        Ok(_) => assert!(!cube),
                        Err(failure) => {
                            assert!(cube, "{failure:?}");
                            for r in &failure.reports {
                                assert_eq!(r.stage, vtium_settings::vtlook::Stage::Reading);
                                assert!(r.file.extension().is_some_and(|e| e == "cube"));
                            }
                        }
                    }
                }
            }
            for f in look.color.iter().chain(&look.spatial) {
                let names: Vec<&str> = f.values.iter().map(|(n, _)| *n).collect();
                let declared: Vec<&str> = f.declaration.params.iter().map(|p| p.name).collect();
                assert_eq!(names, declared);
                for (p, (_, v)) in f.declaration.params.iter().zip(&f.values) {
                    assert!(fits(p.kind, v), "{}: {v:?}", p.name);
                }
                assert!((1..=lines).contains(&f.line));
            }
        }
        Err(failure) => {
            assert!(!failure.reports.is_empty());
            assert!(failure.reports.len() <= ERRORS_MAX);
            for r in &failure.reports {
                assert_eq!(r.file, file);
                assert_eq!(r.vtlook, "fuzz");
                if let Some(n) = r.line {
                    assert!((1..=lines).contains(&n), "line {n} of {lines}");
                }
                let _shown = format!("{}: {r}", r.at());
            }
            let _logged = vtium_settings::vtlook::log_line(&failure, "console");
        }
    }
});
