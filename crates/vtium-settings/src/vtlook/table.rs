//! A vtlook's color filters as one color table, which both drawing paths
//! read.
//!
//! The reader has checked every value against its parameter's kind and
//! range, so building the table can fail only for want of memory, or for a
//! filter that has no part in the table yet. Either failure is a report of
//! the building stage, and the head draws the built-in vtlook.
//!
//! References:
//! - ADR-1304 D2: the look's color filters merge into one table, built on the CPU once for each head.
//! - ADR-1304 D6: the building stage, and its one outcome.

use vtium_render::filter::{Given, Layer, Map, Table};

use super::read::{Filter, Value, Vtlook};
use super::report::{Failure, Part, Report, Stage};

/// The color table of `look`'s color filters, applied in the order of
/// their sections, or the report of why vtium cannot build it.
pub fn table(look: &Vtlook) -> Result<Table, Failure> {
    let of = look.color.len();
    let mut maps = Vec::new();
    for (i, f) in look.color.iter().enumerate() {
        let Some(map) = map(f) else {
            return Err(fails(
                look,
                Some((f, i.saturating_add(1), of)),
                format!("{} has no color table", f.declaration.name),
            ));
        };
        maps.push(map);
    }
    Table::build(&maps).map_err(|e| fails(look, None, format!("{e}")))
}

/// The filter `f` as a map from one color to another, `None` when one of
/// its values has no part in the table.
fn map(f: &Filter) -> Option<Map> {
    let values: Option<Vec<Given>> = f.values.iter().map(|(_, v)| given(v)).collect();
    Map::new(f.declaration, &values?)
}

/// A value as the table takes it. A file's name is not one: the filter
/// that reads a file builds its table from the file.
fn given(v: &Value) -> Option<Given> {
    Some(match *v {
        Value::Number(n) => Given::Number(n),
        Value::Integer(n) => Given::Integer(n),
        Value::Color(c) => Given::Color(c),
        Value::Choice(w) => Given::Choice(w),
        Value::Switch(b) => Given::Switch(b),
        Value::File(_) => return None,
    })
}

/// The failure of the building stage, at filter `filter` when one is at
/// fault: the filter, its place from 1 and the number of color filters.
fn fails(look: &Vtlook, filter: Option<(&Filter, usize, usize)>, message: String) -> Failure {
    Failure {
        reports: vec![Report {
            vtlook: look.name.clone(),
            file: look.file.clone(),
            line: filter.map(|(f, ..)| f.line),
            part: filter.map(|(f, place, of)| Part::Filter {
                layer: Layer::Color,
                name: f.declaration.name.to_owned(),
                place,
                of,
            }),
            stage: Stage::Building,
            message,
        }],
        more: 0,
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::vtlook::read_text;
    use std::path::Path;

    fn look(text: &str) -> Vtlook {
        read_text(
            "amber",
            Path::new("/etc/vtium/looks/amber.vtlook"),
            text.as_bytes(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    }

    // A vtlook's color filters build the table the filters give in the
    // order of their sections, each parameter left out at its default.
    #[test]
    fn a_vtlook_builds_the_table_of_its_color_filters_in_order() {
        let text =
            "version = 1\n[color tint]\ncolor = 255,176,0\n[color contrast]\ncontrast = 1.15\n";
        let got = table(&look(text)).unwrap_or_else(|e| panic!("{e:?}"));
        let decl = |name| vtium_render::filter::color(name).unwrap_or_else(|| panic!("{name}"));
        let tint = Map::defaults(decl("tint")).unwrap_or_else(|| panic!("tint"));
        let contrast = Map::new(
            decl("contrast"),
            &[
                Given::Number(vtium_render::filter::Fixed::of(1, 1500)),
                Given::Number(vtium_render::filter::Fixed::of(0, 0)),
            ],
        )
        .unwrap_or_else(|| panic!("contrast"));
        let want = Table::build(&[tint, contrast]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(got, want);
        let reversed = Table::build(&[contrast, tint]).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(got, reversed);
        // a vtlook with no color filter builds the identity
        let none = table(&look("version = 1\n")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(none, Table::build(&[]).unwrap_or_else(|e| panic!("{e}")));
    }

    // A filter whose value has no part in the table is an error of the
    // building stage, which names the filter and its place.
    #[test]
    fn a_filter_with_no_table_fails_at_the_building_stage() {
        let mut l = look("version = 1\n[color gamma]\n");
        if let Some(f) = l.color.first_mut() {
            f.values = vec![("gamma", Value::File("warm.cube".to_owned()))];
        }
        let Err(failure) = table(&l) else {
            panic!("built");
        };
        let said: Vec<String> = failure.reports.iter().map(ToString::to_string).collect();
        assert_eq!(
            said,
            ["color filter 1 of 1, gamma: building the color table: gamma has no color table"]
        );
        assert_eq!(failure.reports.first().and_then(|r| r.line), Some(2));
    }
}
