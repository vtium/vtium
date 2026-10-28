//! A configuration file in the C reference's key syntax (`kmscon.conf(5)`):
//! one key a line, `key=value` or a switch's `key` or `no-key`; `#` starts
//! a comment anywhere but where it opens a `#rrggbb` color value; spaces,
//! tabs and carriage returns around the key and the value are dropped.

use std::path::Path;

use super::keys::{Kind, Scope};
use super::{Assignment, LINE_MAX, Origin, Problem, Reports, Value, value};

/// Whether `s` starts with `#rrggbb` ended by the line, a blank or a
/// comment.
fn begins_rgb(s: &[u8]) -> bool {
    match s {
        [b'#', rgb @ ..] if rgb.len() >= 6 => {
            let (digits, rest) = rgb.split_at(6);
            digits.iter().all(u8::is_ascii_hexdigit)
                && matches!(rest.first(), None | Some(b' ' | b'\t' | b'\r' | b'#'))
        }
        _ => false,
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let blank = |b: &u8| matches!(b, b' ' | b'\t' | b'\r');
    let start = s.iter().position(|b| !blank(b)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|b| !blank(b))
        .map_or(start, |e| e.saturating_add(1));
    s.get(start..end).unwrap_or_default()
}

/// One line's key and value, comment and blanks dropped: `None` for a
/// blank or comment line.
fn split(line: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
    let end = line.iter().position(|&b| b == b'#' || b == b'=');
    let (key, rest) = match end {
        Some(at) => line.split_at(at),
        None => (line, &[][..]),
    };
    let value = match rest.split_first() {
        Some((b'=', after)) => {
            let lead = after
                .iter()
                .position(|b| !matches!(b, b' ' | b'\t'))
                .unwrap_or(after.len());
            let body = after.get(lead..).unwrap_or_default();
            let skip = usize::from(begins_rgb(body));
            let stop = body
                .iter()
                .skip(skip)
                .position(|&b| b == b'#')
                .map_or(body.len(), |p| p.saturating_add(skip));
            Some(trim(body.get(..stop).unwrap_or_default()))
        }
        _ => None,
    };
    let key = trim(key);
    (!key.is_empty() || value.is_some()).then_some((key, value))
}

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
