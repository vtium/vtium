//! What the Settings tab says on screen: its titles and counts, the marks
//! beside a value, the hints along the foot, the full help F1 opens, and
//! the names of the changes it makes, which the message line and Undo show.
//!
//! References:
//! - crates/AGENTS.md, "Strings on screen", rules 7 and 8: one line where
//!   a paragraph was, and every key named for pressing in brackets.

/// The tab's name on the top row, and the title over the sections.
pub const SETTINGS: &str = "Settings";

/// The mark beside a value the open configuration changes from the
/// installed file.
pub const CHANGED: &str = "changed";

/// The mark beside a value the configuration leaves at vtium's default.
pub const DEFAULT: &str = "default";

/// A key binding that binds no key.
pub const NO_KEY: &str = "none";

/// The word that returns a key binding to its default while it is typed.
pub const BINDING_DEFAULT: &str = "default";

/// The list box's choice that leaves a key at its default, for a key whose
/// default is no value at all.
pub const UNSET: &str = "unset";

/// The list box's choice that lets a name be typed instead of picked.
pub const OTHER: &str = "Other…";

/// The foot while a value is typed.
pub const TYPING: &str = "Type the value and press [Enter]. Leave it empty to use the default.";

/// The foot while a key binding is typed: an empty value binds no key, so
/// programs get it, and the word `default` returns it to its default.
pub const TYPING_BINDING: &str = "Type key combinations, separated by commas, and press [Enter]. Empty binds no key; default restores it.";

/// The right pane while the search matches no key.
pub const NO_MATCH: &str = "No setting matches. [Esc] clears the search.";

/// The full help's label for a key's default.
pub const LABEL_DEFAULT: &str = "Default";

/// The full help's label for the value the configuration gives a key.
pub const LABEL_NOW: &str = "Now";

/// The full help's label for the values a key takes.
pub const LABEL_TAKES: &str = "Takes";

/// How many settings a section holds.
pub fn settings_count(n: usize) -> String {
    match n {
        1 => "1 setting".to_owned(),
        n => format!("{n} settings"),
    }
}

/// How many of a section's settings the search matches.
pub fn matching_count(matching: usize, of: usize) -> String {
    format!("{matching} of {}", settings_count(of))
}

/// Beside the title while the search filters the keys.
pub fn matching(entry: &str) -> String {
    format!("matching {entry}")
}

/// Where a key or a section is, from the tab down, as the full help and
/// Find write it.
pub fn place(parts: &[&str]) -> String {
    std::iter::once(SETTINGS)
        .chain(parts.iter().copied())
        .collect::<Vec<_>>()
        .join(" › ")
}

/// The foot's line for a key: its name, then its first sentence.
pub fn about_key(name: &str, sentence: &str) -> String {
    format!("{name}: {}", lower_first(sentence))
}

/// The words of a description the search matched, as the foot quotes them
/// after the key's first sentence.
pub fn quoted(excerpt: &str) -> String {
    format!(" · “…{excerpt}…”")
}

/// The name of the change that sets `key` to `value`, or leaves it at its
/// default with `None`.
pub fn change_name(key: &str, value: Option<&str>) -> String {
    match value {
        Some("") => format!("{key} = (empty)"),
        Some(v) => format!("{key} = {v}"),
        None => format!("{key} back to its default"),
    }
}

/// A choice of the list box that is the key's default.
pub fn default_choice(word: &str) -> String {
    format!("{word} (default)")
}

/// What a whole number takes: its bounds and unit, and the word it takes
/// in place of a number, where it has one.
pub fn number(min: u64, max: u64, unit: &str, word: Option<&str>) -> String {
    let unit = if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    };
    match word {
        Some(w) => format!("a whole number from {min} to {max}{unit}, or {w}"),
        None => format!("a whole number from {min} to {max}{unit}"),
    }
}

/// `sentence` with its first letter in lowercase where the first word is
/// an ordinary capitalized word, so it reads on after `name: `. A word
/// written in capitals, such as `DRM`, or a name in code keeps its case.
fn lower_first(sentence: &str) -> String {
    let mut chars = sentence.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => first
            .to_lowercase()
            .chain(sentence.chars().skip(1))
            .collect(),
        _ => sentence.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_foot_reads_on_after_the_key_s_name() {
        assert_eq!(
            about_key("font-name", "The console's font."),
            "font-name: the console's font."
        );
        assert_eq!(
            about_key("hwaccel", "DRM rendering on the GPU."),
            "hwaccel: DRM rendering on the GPU."
        );
        assert_eq!(about_key("x", "`x` sets it."), "x: `x` sets it.");
    }

    #[test]
    fn a_change_names_the_key_and_its_value() {
        assert_eq!(
            change_name("glyph-source", Some("auto")),
            "glyph-source = auto"
        );
        assert_eq!(
            change_name("glyph-source", None),
            "glyph-source back to its default"
        );
        assert_eq!(change_name("key-copy", Some("")), "key-copy = (empty)");
    }
}
