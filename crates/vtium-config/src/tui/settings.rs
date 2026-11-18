//! The Settings view's organization: the sections of vtium.conf's keys and
//! their titled groups, from vtium-settings' key table, and where the view
//! stands in them. The view opens on the list of sections. Enter opens a
//! section, which shows its groups as headed blocks with their keys
//! beneath, and Enter on a key either starts typing its value or opens its
//! allowed values as the shared list.
//!
//! The sections come from `SECTIONS`, the table that the example vtium.conf
//! and docs/config.md also follow, so the three never disagree on where a
//! key belongs. The view lists only the keys a configuration file sets at
//! its top level, so a key that only the command line takes, or that only
//! a `[look]` or `[head]` section sets, is left out, and so is a group or
//! a section left with no key.
//!
//! References:
//! - ADR-1201 D3, the Settings bullet: the sections first, Enter opens one
//!   and Esc goes back with the same section highlighted.
//! - ADR-1403 D7, "One list for every set of items": a key's allowed values
//!   open in the shared list.

use vtium_settings::keys::{self, Key, Kind, Scope};

use super::listbox::ListBox;

/// A section of the view: its title, what it covers, and its groups, each
/// a title and its keys.
#[derive(Clone, Debug)]
pub struct Section {
    pub title: &'static str,
    pub about: &'static str,
    pub groups: Vec<(&'static str, Vec<&'static Key>)>,
}

impl Section {
    /// The section's keys in order, group after group.
    pub fn keys(&self) -> Vec<&'static Key> {
        self.groups
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect()
    }
}

/// The view's sections, in the key table's order, each with the groups and
/// keys a configuration file sets at its top level.
pub fn sections() -> Vec<Section> {
    keys::SECTIONS
        .iter()
        .map(|s| Section {
            title: s.title,
            about: s.about,
            groups: s
                .groups
                .iter()
                .map(|g| {
                    let keys: Vec<&'static Key> = g
                        .keys
                        .iter()
                        .copied()
                        .filter(|k| k.scope == Scope::Anywhere)
                        .collect();
                    (g.title, keys)
                })
                .filter(|(_, keys)| !keys.is_empty())
                .collect(),
        })
        .filter(|s| !s.groups.is_empty())
        .collect()
}

/// The values a key takes from a fixed set, which the view offers as a
/// list: `on` and `off` for a switch, and a choice's words. `None` for a
/// key whose value is typed.
pub fn fixed_values(key: &Key) -> Option<Vec<&'static str>> {
    match key.kind {
        Kind::Switch => Some(vec!["on", "off"]),
        Kind::Choice(words) => Some(words.to_vec()),
        _ => None,
    }
}

/// Where the Settings view stands: the highlighted section, the section
/// open over the list with its highlighted key, and a key's allowed values
/// open over the section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub sections: ListBox,
    /// The open section's index, and the highlighted key among its keys.
    pub open: Option<(usize, ListBox)>,
    /// The highlighted key's allowed values, open as the shared list.
    pub values: Option<ListBox>,
}

impl Settings {
    /// The open section, if one is.
    pub fn section(&self) -> Option<Section> {
        let (at, _) = self.open?;
        sections().into_iter().nth(at)
    }

    /// The highlighted key of the open section.
    pub fn key(&self) -> Option<&'static Key> {
        let (_, list) = self.open?;
        self.section()?.keys().get(list.at).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key a configuration file sets at its top level is in one
    /// section of the view, once, so no key is out of reach.
    #[test]
    fn every_top_level_key_is_in_one_section_once() {
        let shown: Vec<&str> = sections()
            .iter()
            .flat_map(Section::keys)
            .map(|k| k.name)
            .collect();
        let takes: Vec<&str> = keys::KEYS
            .iter()
            .filter(|k| k.scope == Scope::Anywhere)
            .map(|k| k.name)
            .collect();
        for name in &takes {
            assert_eq!(
                shown.iter().filter(|n| *n == name).count(),
                1,
                "{name} is not in exactly one section"
            );
        }
        assert_eq!(shown.len(), takes.len(), "{shown:?}");
    }

    #[test]
    fn the_sections_follow_the_key_table_and_skip_an_empty_group() {
        let all = sections();
        assert_eq!(all.first().map(|s| s.title), Some("Fonts and text"));
        assert!(
            all.iter()
                .all(|s| s.groups.iter().all(|(_, k)| !k.is_empty()))
        );
        assert_eq!(
            all.first()
                .and_then(|s| s.keys().first().copied())
                .map(|k| k.name),
            Some("font-name")
        );
    }

    #[test]
    fn a_switch_and_a_choice_have_fixed_values_and_a_text_key_has_none() {
        assert_eq!(fixed_values(&keys::ALLOW_FONT_OPS), Some(vec!["on", "off"]));
        assert!(
            keys::KEYS.iter().any(|k| matches!(k.kind, Kind::Choice(_))
                && fixed_values(k).is_some_and(|v| !v.is_empty()))
        );
        assert_eq!(fixed_values(&keys::FONT_NAME), None);
    }
}
