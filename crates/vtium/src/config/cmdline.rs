//! The command line: `--key=value` or `--key value`, `--switch` and
//! `--no-switch`, the one-letter forms (`-c DIR`, `-cDIR`), and
//! `--login -- COMMAND...`, whose words after `--` are the session's
//! command. Each name is matched whole.

use std::ffi::OsString;

use super::keys::{self, Kind};
use super::{Assignment, Origin, Problem, Reports, Value, value};

/// The values `args` gives, in order, with what they could not give
/// reported.
pub(super) fn parse(args: &[OsString], reports: &mut Reports) -> Vec<Assignment> {
    let mut out = Vec::new();
    let mut login = false;
    let mut words = args
        .iter()
        .enumerate()
        .map(|(n, a)| (n.saturating_add(1), a));
    while let Some((arg, word)) = words.next() {
        let origin = Origin::CommandLine { arg };
        let Some(word) = word.to_str() else {
            reports.push(origin, Problem::NotUtf8);
            continue;
        };
        if word == "--" {
            let rest = words
                .by_ref()
                .map(|(_, w)| w.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            if !login {
                for w in rest {
                    reports.push(origin.clone(), Problem::StrayArgument(w));
                }
            } else if rest.is_empty() {
                reports.push(origin, Problem::LoginWithoutCommand);
            } else {
                let key = &keys::LOGIN;
                out.push(Assignment {
                    key,
                    value: Value::Command(rest),
                    origin,
                });
            }
            break;
        }
        // The key, whether it is a `no-` form, and a value given in the
        // same word.
        let (key, negated, inline) = if let Some(long) = word.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, v)) => (name, Some(v)),
                None => (long, None),
            };
            match value::lookup(name) {
                Some((key, negated)) => (key, negated, inline),
                None => {
                    reports.push(origin, Problem::UnknownKey(format!("--{name}")));
                    continue;
                }
            }
        } else if let Some(short) = word.strip_prefix('-').filter(|s| !s.is_empty()) {
            let mut chars = short.chars();
            let found = chars.next().and_then(keys::find_short);
            let attached = chars.as_str();
            match found {
                Some(key) if key.kind != Kind::Switch && key.kind != Kind::Command => {
                    (key, false, (!attached.is_empty()).then_some(attached))
                }
                Some(key) if attached.is_empty() => (key, false, None),
                _ => {
                    reports.push(origin, Problem::UnknownKey(word.to_owned()));
                    continue;
                }
            }
        } else {
            reports.push(origin, Problem::StrayArgument(word.to_owned()));
            continue;
        };

        let value = match key.kind {
            // `--login` says the words after `--` are the command.
            Kind::Command if inline.is_none() => {
                login = true;
                continue;
            }
            Kind::Switch | Kind::Command => {
                if inline.is_some() {
                    reports.push(origin, Problem::TakesNoValue(key.name));
                    continue;
                }
                Value::Switch(!negated)
            }
            _ => {
                let text = match inline {
                    Some(text) => text.to_owned(),
                    None => match words.next() {
                        Some((_, w)) => match w.to_str() {
                            Some(w) => w.to_owned(),
                            None => {
                                reports.push(origin, Problem::NotUtf8);
                                continue;
                            }
                        },
                        None => {
                            reports.push(origin, Problem::NeedsValue(key.name));
                            continue;
                        }
                    },
                };
                match value::parse(key, &text, &origin, reports) {
                    Some(v) => v,
                    None => continue,
                }
            }
        };
        out.push(Assignment { key, value, origin });
    }
    out
}
