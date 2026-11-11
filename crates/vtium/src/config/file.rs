//! vtium.conf's top level, the lines above its first section heading, in
//! the key syntax of `kmscon` configuration files (`kmscon.conf(5)`): one
//! key a line,
//! `key=value` or a switch's `key` or `no-key`, with the blanks around the
//! key and the value dropped (ADR-1403 D3).
//!
//! vtium-settings' [`split`] decides where the top level ends and leaves
//! out the flow's own `version` and `policy` lines, which the section
//! reader reads, and its [`key_line`] reads each line by vtium.conf's one
//! comment rule: a `#` starts a comment at the start of a line or after a
//! blank, `\#` is a literal `#`, and a `#` that opens a `#rrggbb` value is
//! part of it. vtium-config's line editor reads the file the same way.

use std::path::Path;

use vtium_settings::{key_line, split};

use super::keys::{self, Kind, Scope};
use super::{Assignment, LINE_MAX, Origin, Problem, Reports, Value, value};

/// The values the top level of a vtium.conf's bytes gives, in order, with
/// what they could not give reported. A line keeps its number in the whole
/// file.
pub(super) fn parse(bytes: &[u8], path: &Path, reports: &mut Reports) -> Vec<Assignment> {
    let mut out = Vec::new();
    // A key binding set by its old name goes first, so the new name wins
    // wherever the file sets both (ADR-1005 D10).
    let mut old_names = Vec::new();
    for (n, line) in split(bytes).key_lines() {
        let origin = Origin::File {
            path: path.to_owned(),
            line: n,
        };
        if line.len() > LINE_MAX {
            reports.push(origin, Problem::LineTooLong);
            continue;
        }
        if line.contains(&0) {
            reports.push(origin, Problem::Nul);
            continue;
        }
        let Some(read) = key_line(line) else {
            continue;
        };
        let (Ok(name), Ok(text)) = (
            std::str::from_utf8(read.key),
            read.value.as_deref().map(std::str::from_utf8).transpose(),
        ) else {
            reports.push(origin, Problem::NotUtf8);
            continue;
        };
        if name.is_empty() {
            reports.push(origin, Problem::NoKey);
            continue;
        }
        let (key, negated, old) = match value::lookup(name) {
            Some((key, negated)) => (key, negated, false),
            None => match keys::renamed(name) {
                Some((old, key)) => {
                    let new = key.name;
                    reports.push(origin.clone(), Problem::Renamed { old, new });
                    (key, false, true)
                }
                None => {
                    reports.push(origin, Problem::UnknownKey(name.to_owned()));
                    continue;
                }
            },
        };
        match key.scope {
            Scope::CommandLine => {
                reports.push(origin, Problem::CommandLineOnly(key.name));
                continue;
            }
            Scope::Section => {
                reports.push(origin, Problem::SectionOnly(key.name));
                continue;
            }
            Scope::Anywhere => {}
        }
        let value = match (key.kind, text) {
            (Kind::Switch, None) => Some(Value::Switch(!negated)),
            (_, None) => {
                reports.push(origin, Problem::NeedsValue(key.name));
                continue;
            }
            (_, Some(text)) => value::parse(key, text, &origin, reports),
        };
        if let Some(value) = value {
            let to = if old { &mut old_names } else { &mut out };
            to.push(Assignment { key, value, origin });
        }
    }
    old_names.extend(out);
    old_names
}
