//! The Looks tab's group of looks: each `[look NAME]` section of the
//! draft, with its font set, its size and the heads that draw it. When the
//! draft has no look, the group holds one row that says so, where `n`
//! makes the first.
//!
//! References:
//! - ADR-1403 D4: a look is a `[look NAME]` section, which a head names
//!   with `look =`.
//! - ADR-1403 D10: the looks are the first group of the tab's list.

use super::super::super::listbox::Item;
use super::super::super::model::Shared;
use super::super::super::words;

/// One look of the draft, with the keys of it that its card shows, each
/// `None` where the look leaves the key to vtium.conf's top level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    pub name: String,
    pub frit: Option<String>,
    pub size: Option<String>,
    pub palette: Option<String>,
    pub vtlook: Option<String>,
    /// The look's own `pointer-theme`: the SVG file or cursor theme folder
    /// whose pictures its pointer takes.
    pub pointer: Option<String>,
}

/// The looks of the configuration the tab shows, the draft or else the
/// installed vtium.conf, in the file's order. A draft whose sections do
/// not read has none.
pub fn looks(shared: &Shared) -> Vec<Look> {
    let Some(reading) = shared
        .conf_text()
        .and_then(|c| vtium_settings::sections(c.as_bytes()))
    else {
        return Vec::new();
    };
    reading
        .parsed
        .config
        .looks
        .into_iter()
        .map(|l| {
            let value = |key: &str| {
                l.settings
                    .iter()
                    .find(|s| s.key.name == key)
                    .map(|s| s.value.clone())
            };
            Look {
                frit: value("font-name"),
                size: value("font-size"),
                palette: value("palette"),
                vtlook: l.vtlook.clone(),
                pointer: value("pointer-theme"),
                name: l.name,
            }
        })
        .collect()
}

/// The look named `name`, where the draft has it.
pub fn look(shared: &Shared, name: &str) -> Option<Look> {
    looks(shared).into_iter().find(|l| l.name == name)
}

/// The heads of the draft that draw the look `name`: those whose
/// `look =` line names it, and for the look `default` also those with no
/// `look =` line, which draw `default`. A draft that defines no head and
/// no rule runs vtium's built-in terminal, whose head `console` names no
/// look and so draws `default`. vtium reads what survives of the
/// sections, so a draft with errors elsewhere still answers.
///
/// References:
/// - ADR-1401 D3: vtium runs the built-in default terminal when the file
///   defines no head and no rule.
pub fn heads(shared: &Shared, name: &str) -> Vec<String> {
    shared
        .conf_text()
        .and_then(|c| vtium_settings::sections(c.as_bytes()))
        .map(|r| {
            let config = r.parsed.config;
            let heads = if config.uses_builtin() {
                vtium_settings::Config::builtin().heads
            } else {
                config.heads
            };
            heads
                .into_iter()
                .filter(|h| h.look == name)
                .map(|h| h.name)
                .collect()
        })
        .unwrap_or_default()
}

/// The terminal of the draft that the console belongs to: the one that
/// `VTIUM_HEAD` names, else the draft's only terminal. With several
/// terminals and no `VTIUM_HEAD`, vtium-config cannot tell which.
///
/// References:
/// - ADR-1401 D5: vtium sets `VTIUM_HEAD` in each console's session.
pub fn console_head(shared: &Shared) -> Option<String> {
    let heads: Vec<String> = shared
        .conf_text()
        .and_then(|c| vtium_settings::sections(c.as_bytes()))
        .map(|r| r.parsed.config.heads.into_iter().map(|h| h.name).collect())
        .unwrap_or_default();
    match (&shared.inputs.head, heads.as_slice()) {
        (Some(head), _) if heads.contains(head) => Some(head.clone()),
        (_, [only]) => Some(only.clone()),
        _ => None,
    }
}

/// Whether the terminal `head` of the draft names no look of its own, so
/// it draws the look `default`.
pub fn names_no_look(shared: &Shared, head: &str) -> bool {
    heads(shared, words::DEFAULT_LOOK).iter().any(|h| h == head)
}

/// The look `look` as an item of the tab's list: its name, then the
/// sentence that says its font set, its size and the heads that draw it.
pub fn item(shared: &Shared, look: &Look) -> Item {
    Item::new(
        look.name.clone(),
        words::look_about(
            &look.name,
            look.frit.as_deref(),
            look.size.as_deref(),
            &heads(shared, &look.name),
        ),
    )
}

/// A name for a new look that no look of the draft has: `new-look`, else
/// `new-look-2`, `new-look-3` and so on.
pub fn new_name(shared: &Shared) -> String {
    let taken: Vec<String> = looks(shared).into_iter().map(|l| l.name).collect();
    std::iter::once(words::NEW_LOOK_NAME.to_owned())
        .chain((2..).map(|n| format!("{}-{n}", words::NEW_LOOK_NAME)))
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}
