//! The command line: `--key=value` or `--key value`, `--switch` and
//! `--no-switch`, the one-letter forms (`-c DIR`, `-cDIR`), and
//! `--login -- COMMAND...`, whose words after `--` are the session's
//! command, and the subcommand `check-config` wherever it stands among
//! the options, with the vtium.conf it checks in place of the installed
//! one, its `--no-devices`, and the draft's font sets that `--with-frit`
//! and `--without-frit` name. Each name is matched whole.

use std::ffi::OsString;
use std::path::PathBuf;

use super::keys::{self, Kind, Scope};
use super::{Assignment, Origin, Problem, Reports, Value, value};
use crate::check::{DraftFrits, NO_DEVICES, SUBCOMMAND, WITH_FRIT, WITHOUT_FRIT};

/// What the command line gives.
pub(super) struct Parsed {
    /// The values, in order.
    pub(super) assignments: Vec<Assignment>,
    /// The subcommand `check-config` stands among the options.
    pub(super) check: bool,
    /// The vtium.conf `check-config` checks in place of the installed one.
    pub(super) candidate: Option<PathBuf>,
    /// `check-config --no-devices`: judge the rules alone, without the
    /// devices present (ADR-1403 D3).
    pub(super) no_devices: bool,
    /// `check-config --with-frit` and `--without-frit`: the font sets a
    /// draft adds, replaces and removes (ADR-1403 D8).
    pub(super) draft_frits: DraftFrits,
}

/// The values `args` gives, in order, with what they could not give
/// reported. A word that is no option is the subcommand when it is
/// [`SUBCOMMAND`], wherever it stands before `--`, and the first such word
/// after the subcommand is the vtium.conf it checks (ADR-1403 D3).
/// [`NO_DEVICES`], [`WITH_FRIT`] and [`WITHOUT_FRIT`] belong to the
/// subcommand, wherever they stand, and are unknown options without it.
pub(super) fn parse(args: &[OsString], reports: &mut Reports) -> Parsed {
    let mut out = Vec::new();
    // A key binding given by its old name goes first, so the new name
    // wins wherever the command line gives both (ADR-1005 D10).
    let mut old_names = Vec::new();
    let mut check = false;
    let mut candidate = None;
    let mut no_devices = None;
    let mut draft_frits = DraftFrits::default();
    // the first `--with-frit` or `--without-frit`, which is an unknown
    // option where the subcommand is not given
    let mut frit_option = None;
    let mut login = false;
    // `--login`'s command is the words after `--`.
    let mut dashes = false;
    let mut words = args
        .iter()
        .enumerate()
        .map(|(n, a)| (n.saturating_add(1), a));
    while let Some((arg, word)) = words.next() {
        let origin = Origin::CommandLine { arg };
        let mut by_old_name = false;
        let Some(word) = word.to_str() else {
            reports.push(origin, Problem::NotUtf8);
            continue;
        };
        if word == NO_DEVICES {
            no_devices = Some(origin);
            continue;
        }
        if let Some((option, value)) = frit_option_of(word) {
            frit_option.get_or_insert((origin.clone(), option));
            let value = match value {
                Some(v) => v.to_owned(),
                None => match words.next().map(|(_, w)| w.to_str()) {
                    Some(Some(w)) => w.to_owned(),
                    Some(None) => {
                        reports.push(origin, Problem::NotUtf8);
                        continue;
                    }
                    None => {
                        reports.push(origin, Problem::NeedsValue(option));
                        continue;
                    }
                },
            };
            if option == WITHOUT_FRIT {
                draft_frits.remove(value);
            } else {
                let file = PathBuf::from(&value);
                match crate::check::with_frit_name(&file) {
                    Some(name) => draft_frits.add(name, file),
                    None => reports.push(origin, Problem::NotAFritFile(value)),
                }
            }
            continue;
        }
        if word == "--" {
            dashes = true;
            // The words are passed as given, bytes that are not UTF-8
            // included, since a command's arguments are bytes.
            let rest = words.by_ref().map(|(_, w)| w.clone()).collect::<Vec<_>>();
            if !login {
                for w in rest {
                    let w = w.to_string_lossy().into_owned();
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
                None => match keys::renamed(name) {
                    Some((old, key)) => {
                        let new = key.name;
                        reports.push(origin.clone(), Problem::Renamed { old, new });
                        by_old_name = true;
                        (key, false, inline)
                    }
                    None => {
                        reports.push(origin, Problem::UnknownKey(format!("--{name}")));
                        continue;
                    }
                },
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
        } else if word == SUBCOMMAND && !check {
            check = true;
            continue;
        } else if check && candidate.is_none() {
            candidate = Some(PathBuf::from(word));
            continue;
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
                // a look's own key, such as `cell`, takes its value word
                // and is refused
                if key.scope == Scope::Section {
                    reports.push(origin, Problem::SectionOnly(key.name));
                    continue;
                }
                match value::parse(key, &text, &origin, reports) {
                    Some(v) => v,
                    None => continue,
                }
            }
        };
        let to = if by_old_name {
            &mut old_names
        } else {
            &mut out
        };
        to.push(Assignment { key, value, origin });
    }
    old_names.extend(out);
    let out = old_names;
    if login && !dashes {
        reports.push(
            Origin::CommandLine {
                arg: args.len().max(1),
            },
            Problem::LoginWithoutCommand,
        );
    }
    if let Some(origin) = no_devices.clone().filter(|_| !check) {
        reports.push(origin, Problem::UnknownKey(NO_DEVICES.to_owned()));
    }
    if let Some((origin, option)) = frit_option.filter(|_| !check) {
        reports.push(origin, Problem::UnknownKey(option.to_owned()));
    }
    Parsed {
        assignments: out,
        check,
        candidate,
        no_devices: check && no_devices.is_some(),
        draft_frits: if check {
            draft_frits
        } else {
            DraftFrits::default()
        },
    }
}

/// `word` as [`WITH_FRIT`] or [`WITHOUT_FRIT`]: the option, and the value
/// given in the same word after `=`, if any.
fn frit_option_of(word: &str) -> Option<(&'static str, Option<&str>)> {
    [WITH_FRIT, WITHOUT_FRIT].into_iter().find_map(|option| {
        let rest = word.strip_prefix(option)?;
        match rest.strip_prefix('=') {
            Some(value) => Some((option, Some(value))),
            None => rest.is_empty().then_some((option, None)),
        }
    })
}
