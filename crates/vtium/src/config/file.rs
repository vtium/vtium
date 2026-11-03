//! A configuration file in the C reference's key syntax (`kmscon.conf(5)`):
//! one key a line, `key=value` or a switch's `key` or `no-key`; `#` starts
//! a comment anywhere but where it opens a `#rrggbb` color value; spaces,
//! tabs and carriage returns around the key and the value are dropped.
//! vtium-settings' `split_line` reads each line, the one reading vtium-config's
//! editor shares.

use std::path::Path;

use vtium_settings::split_line as split;

use super::keys::{Kind, Scope};
use super::{Assignment, LINE_MAX, Origin, Problem, Reports, Value, value};

/// The values a file's bytes give, in order, with what they could not
/// give reported.
pub(super) fn parse(bytes: &[u8], path: &Path, reports: &mut Reports) -> Vec<Assignment> {
    let mut out = Vec::new();
    for (n, line) in bytes.split(|&b| b == b'\n').enumerate() {
        let origin = Origin::File {
            path: path.to_owned(),
            line: n.saturating_add(1),
        };
        if line.len() > LINE_MAX {
            reports.push(origin, Problem::LineTooLong);
            continue;
        }
        if line.contains(&0) {
            reports.push(origin, Problem::Nul);
            continue;
        }
        let Some((key, text)) = split(line) else {
            continue;
        };
        let (Ok(name), Ok(text)) = (
            std::str::from_utf8(key),
            text.map(std::str::from_utf8).transpose(),
        ) else {
            reports.push(origin, Problem::NotUtf8);
            continue;
        };
        if name.is_empty() {
            reports.push(origin, Problem::NoKey);
            continue;
        }
        let Some((key, negated)) = value::lookup(name) else {
            reports.push(origin, Problem::UnknownKey(name.to_owned()));
            continue;
        };
        if key.scope == Scope::CommandLine {
            reports.push(origin, Problem::CommandLineOnly(key.name));
            continue;
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
            out.push(Assignment { key, value, origin });
        }
    }
    out
}
