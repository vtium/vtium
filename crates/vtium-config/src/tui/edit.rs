//! What the Flow view changes in the draft (ADR-1403 D7, D8). Each change
//! is one of [`crate::ops`]'s operations, the ones the `vtium config`
//! commands call, and it also names the command that makes the same
//! change, which the view shows.

use vtium_settings::Of;

use crate::ops::{self, RuleChange};
use crate::outcome::Fail;

/// One change to the draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Adds a rule, `KIND SELECTOR -> TARGET`, where a new rule goes.
    RuleAdd(String),
    /// Adds a rule before rule `before`, or after the last where `before`
    /// is one past it.
    RuleAddBefore {
        line: String,
        before: usize,
    },
    /// Changes rule `number`'s selector, its target, or both.
    RuleSet {
        number: usize,
        selector: Option<String>,
        target: Option<String>,
    },
    /// Moves rule `number` before rule `before`; one past the last rule
    /// moves it last.
    RuleMove {
        number: usize,
        before: usize,
    },
    RuleRm(usize),
    HeadAdd {
        name: String,
        look: Option<String>,
    },
    /// Sets a head's key, or leaves it at its default with `None`.
    HeadKey {
        name: String,
        key: &'static str,
        value: Option<String>,
    },
    HeadRename {
        name: String,
        new: String,
    },
    HeadRm(String),
    /// Adds a look with its font set, its size and its vtlook, each left
    /// out with `None`.
    LookAdd {
        name: String,
        frit: Option<String>,
        size: Option<String>,
        vtlook: Option<String>,
    },
    LookKey {
        name: String,
        key: &'static str,
        value: Option<String>,
    },
    LookRename {
        name: String,
        new: String,
    },
    /// Deletes a look, setting the heads that use it to the default look.
    LookRm(String),
    /// Sets a top-level key, or leaves it at its default with `None`.
    Top {
        key: &'static str,
        value: Option<String>,
    },
    /// Writes the `[labels]` section with these lines, after `[rules]`, or
    /// removes it when there are none.
    Labels(Vec<String>),
}

/// `word` quoted for a shell where it needs it.
fn shell(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:,=@+".contains(c))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The draft `text` with `changes` made in order. When a change edits the
/// sections and the draft has none, the compiled-in flow is written out
/// first, since the changes were worked out from the flow the view shows
/// ([`ops::with_flow`]).
pub fn apply_all(text: &str, changes: &[Change]) -> Result<String, Fail> {
    let sections = changes.iter().any(|c| !matches!(c, Change::Top { .. }));
    let start = if sections {
        ops::with_flow(text)
    } else {
        text.to_owned()
    };
    changes.iter().try_fold(start, |t, c| c.apply(&t))
}

impl Change {
    /// The draft `text` with the change made, through the operations the
    /// commands call.
    pub fn apply(&self, text: &str) -> Result<String, Fail> {
        match self {
            Change::RuleAdd(line) => ops::rule_add(text, line, None),
            Change::RuleAddBefore { line, before } => ops::rule_add(text, line, Some(*before)),
            Change::RuleSet {
                number,
                selector,
                target,
            } => ops::rule_set(
                text,
                *number,
                &RuleChange::Parts {
                    selector: selector.clone(),
                    target: target.clone(),
                },
            ),
            Change::RuleMove { number, before } => {
                let last = vtium_settings::rule_lines(text).len().saturating_add(1);
                let to = if *before >= last {
                    ops::To::Last
                } else {
                    ops::To::Before(*before)
                };
                ops::rule_move(text, *number, to)
            }
            Change::RuleRm(n) => ops::rule_rm(text, *n),
            Change::HeadAdd { name, look } => ops::head_add(text, name, look.as_deref()),
            Change::HeadKey { name, key, value } => {
                ops::section_keys(text, Of::Head(name), &[(key, value.as_deref())])
            }
            Change::HeadRename { name, new } => ops::rename(text, Of::Head(name), new),
            Change::HeadRm(name) => ops::head_rm(text, name),
            Change::LookAdd {
                name,
                frit,
                size,
                vtlook,
            } => ops::look_add(
                text,
                name,
                &[
                    ("font-name", frit.as_deref()),
                    ("font-size", size.as_deref()),
                    ("vtlook", vtlook.as_deref()),
                ],
            ),
            Change::LookKey { name, key, value } => {
                ops::section_keys(text, Of::Look(name), &[(key, value.as_deref())])
            }
            Change::LookRename { name, new } => ops::rename(text, Of::Look(name), new),
            Change::LookRm(name) => ops::look_rm(text, name, true),
            Change::Top {
                key,
                value: Some(v),
            } => ops::set_top(text, key, v),
            Change::Top { key, value: None } => ops::unset_top(text, key),
            Change::Labels(lines) => Ok(ops::labels(text, lines)),
        }
    }

    /// The `vtium config` command that makes the same change (ADR-1403
    /// D8).
    pub fn command(&self) -> String {
        let c = "vtium config";
        match self {
            Change::RuleAdd(line) => format!("{c} rule add {}", shell(line)),
            Change::RuleAddBefore { line, before } => {
                format!("{c} rule add {} --before {before}", shell(line))
            }
            Change::RuleSet {
                number,
                selector,
                target,
            } => {
                let mut out = format!("{c} rule set {number}");
                if let Some(s) = selector {
                    out.push_str(&format!(" --selector {}", shell(s)));
                }
                if let Some(t) = target {
                    out.push_str(&format!(" --target {}", shell(t)));
                }
                out
            }
            Change::RuleMove { number, before } => {
                format!("{c} rule move {number} --before {before}")
            }
            Change::RuleRm(n) => format!("{c} rule rm {n}"),
            Change::HeadAdd {
                name,
                look: Some(l),
            } => format!("{c} head add {} --look {}", shell(name), shell(l)),
            Change::HeadAdd { name, look: None } => format!("{c} head add {}", shell(name)),
            Change::HeadKey { name, key, value } => format!(
                "{c} head set {} --{key} {}",
                shell(name),
                shell(value.as_deref().unwrap_or_default())
            ),
            Change::HeadRename { name, new } => {
                format!("{c} head set {} --name {}", shell(name), shell(new))
            }
            Change::HeadRm(name) => format!("{c} head rm {}", shell(name)),
            Change::LookAdd {
                name,
                frit,
                size,
                vtlook,
            } => {
                let mut out = format!("{c} look add {}", shell(name));
                for (key, value) in [("font-name", frit), ("font-size", size)] {
                    if let (Some(option), Some(v)) = (ops::look_option(key), value) {
                        out.push_str(&format!(" {option} {}", shell(v)));
                    }
                }
                // no command of `vtium config` sets a vtlook yet
                if let Some(v) = vtlook {
                    out.push_str(&format!(
                        "\nvtium-config wrote vtlook = {v} into [look {name}]"
                    ));
                }
                out
            }
            Change::LookKey { name, key, value } => {
                let option = ops::look_option(key).unwrap_or(key);
                format!(
                    "{c} look set {} {option} {}",
                    shell(name),
                    shell(value.as_deref().unwrap_or_default())
                )
            }
            Change::LookRename { name, new } => {
                format!("{c} look set {} --name {}", shell(name), shell(new))
            }
            Change::LookRm(name) => format!("{c} look rm {} --force", shell(name)),
            Change::Top {
                key,
                value: Some(v),
            } => format!("{c} set {key} {}", shell(v)),
            Change::Top { key, value: None } => format!("{c} set --unset {key}"),
            // no command of `vtium config` writes labels yet
            Change::Labels(_) => "vtium-config wrote the [labels] section".to_owned(),
        }
    }
}
