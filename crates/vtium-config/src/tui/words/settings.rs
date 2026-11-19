//! What the Settings tab says: the tab's name, the sections, the keys and
//! their values, and the typing of a value.

use super::*;

/// The Settings tab's name in the tab bar.
pub const SETTINGS: &str = "Settings";

/// The settings view.
pub fn default_value(value: impl Display) -> String {
    format!("{value} (default)")
}

/// Under the list of sections.
pub const SECTIONS_HINT: &str = "Press Enter to open a section and change its settings.";

/// Under a section's settings.
pub const SETTINGS_HINT: &str = "Press Enter to type a setting's value, or to pick it from a list when the setting takes a fixed set of values. Leave a typed value empty to use the default.";

/// Under a setting's list of values.
pub const VALUES_HINT: &str = "Press Enter to pick the highlighted value, or Esc to close the list and leave the setting as it is.";

/// The detail of a setting's default in its list of values.
pub const VALUE_DEFAULT: &str = "the default";

/// How many settings a section holds.
pub fn settings_count(n: usize) -> String {
    match n {
        1 => "1 setting".to_owned(),
        n => format!("{n} settings"),
    }
}

/// The title over an open section, or over a setting's values: where the
/// person is, from the view down.
pub fn settings_title(parts: &[&str]) -> String {
    std::iter::once(SETTINGS)
        .chain(parts.iter().copied())
        .collect::<Vec<_>>()
        .join(" › ")
}

pub const EDITING: &str = "Type the value and press Enter. Leave it empty to use the default.";

/// What a key binding's editor says (ADR-1005 D10): an empty value binds
/// no key, so the program gets the key, and the word [`BINDING_DEFAULT`]
/// returns the key binding to its default.
pub const EDITING_BINDING: &str = "Type the key combinations, separated by commas, and press Enter. Leave it empty to bind no key, so programs get it, or type default to use the default.";

/// The word that returns a key binding to its default in its editor.
pub const BINDING_DEFAULT: &str = "default";

/// A key binding that binds no key, as the Settings view and the base
/// card show it.
pub const NO_KEY: &str = "none";

pub const OFF: &str = "off";
