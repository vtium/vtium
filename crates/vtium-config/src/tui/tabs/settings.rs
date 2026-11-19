//! The Settings tab: the sections of vtium.conf's keys and
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

use super::super::input::Key as Pressed;
use super::super::listbox::{Act, ListBox};
use super::super::model::Shared;
use super::{Change, Command};

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

/// Where the Settings tab stands: the highlighted section, the section
/// open over the list with its highlighted key, a key's allowed values
/// open over the section, and the value being typed for the highlighted
/// key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    sections: ListBox,
    /// The open section's index, and the highlighted key among its keys.
    open: Option<(usize, ListBox)>,
    /// The highlighted key's allowed values, open as the shared list.
    values: Option<ListBox>,
    /// The value being typed for the highlighted key, while it is edited.
    editing: Option<String>,
}

impl Settings {
    /// The highlighted section, counted from 0.
    pub fn section_at(&self) -> usize {
        self.sections.at
    }

    /// The open section's index, and the highlighted key among its keys.
    pub fn open(&self) -> Option<(usize, ListBox)> {
        self.open
    }

    /// The highlighted key's allowed values, while they are open.
    pub fn values(&self) -> Option<ListBox> {
        self.values
    }

    /// The value being typed for the highlighted key, while it is edited.
    pub fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }

    /// The node of the interface tree the tab is at: the list of
    /// sections, a section, a key's values, or a value being typed.
    pub fn node(&self) -> &'static str {
        if self.editing.is_some() {
            "settings.section.typing"
        } else if self.values.is_some() {
            "settings.section.list"
        } else if self.open.is_some() {
            "settings.section"
        } else {
            "settings"
        }
    }

    /// The tab is left for another: a value being typed is dropped.
    pub fn left(&mut self) {
        self.editing = None;
    }

    /// A character or Backspace typed into the value being edited.
    pub fn type_key(&mut self, key: Pressed) {
        if let Some(text) = self.editing.as_mut() {
            super::type_into(text, key);
        }
    }

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

    /// A key the navigation module hands to the tab. On the list of
    /// sections, Enter opens the highlighted section. In a section, Enter on a switch or a key
    /// of a fixed set of words opens its values as the shared list, with
    /// the value it has now highlighted, and Enter on any other key starts
    /// typing its value. Esc goes back to the sections, with the same
    /// section highlighted. In the list of values, Enter writes the
    /// highlighted value into the draft and Esc closes the list. While a
    /// value is typed, Enter writes it and Esc drops it, and the moving
    /// keys do nothing. An empty typed value goes back to the default,
    /// except for a key binding, where it binds no key and the word
    /// `default` returns the key to its default.
    ///
    /// References:
    /// - ADR-1201 D3, the Settings bullet: sections, then their keys.
    /// - ADR-1403 D7, "One list for every set of items": the values list.
    /// - ADR-1005 D10: the empty value of a key binding.
    pub fn update(&mut self, shared: &Shared, key: Pressed) -> Vec<Command> {
        if self.editing.is_some() && !matches!(key, Pressed::Enter | Pressed::Escape) {
            return Vec::new();
        }
        if let Some(typed) = self.editing.take() {
            let Some(k) = self.key().filter(|_| key == Pressed::Enter) else {
                return Vec::new();
            };
            let typed = typed.trim().to_owned();
            let value = if k.kind == Kind::KeyBinding {
                // the word `default`, or its default's own combinations,
                // leave a key binding at its default
                let default = crate::commands::value_text(k, None).0;
                (typed != super::super::words::BINDING_DEFAULT && typed != default).then_some(typed)
            } else {
                Some(typed).filter(|t| !t.is_empty())
            };
            return top(k.name, value);
        }
        if let Some(mut list) = self.values {
            let words = self.key().and_then(fixed_values).unwrap_or_default();
            return match list.key(key, words.len()) {
                Act::Moved => {
                    self.values = Some(list);
                    Vec::new()
                }
                Act::Closed => {
                    self.values = None;
                    Vec::new()
                }
                Act::Picked(i) => {
                    self.values = None;
                    match (self.key(), words.get(i)) {
                        (Some(k), Some(w)) => top(k.name, Some((*w).to_owned())),
                        _ => Vec::new(),
                    }
                }
                Act::Ignored => Vec::new(),
            };
        }
        if let Some((at, mut list)) = self.open {
            let keys = self.section().map(|s| s.keys()).unwrap_or_default();
            match list.key(key, keys.len()) {
                Act::Moved => self.open = Some((at, list)),
                Act::Closed => self.open = None,
                Act::Picked(i) => {
                    let Some(k) = keys.get(i).copied() else {
                        return Vec::new();
                    };
                    let (now, default) = shared.value(k);
                    match fixed_values(k) {
                        Some(words) => {
                            let at = words.iter().position(|w| *w == now).unwrap_or(0);
                            self.values = Some(ListBox::at(at));
                        }
                        // a key binding's editor starts from its
                        // combinations, its default's included, so Enter
                        // alone never empties it
                        None => {
                            let start = default && k.kind != Kind::KeyBinding;
                            self.editing = Some(if start { String::new() } else { now });
                        }
                    }
                }
                Act::Ignored => {}
            }
            return Vec::new();
        }
        if let Act::Picked(i) = self.sections.key(key, sections().len()) {
            self.open = Some((i, ListBox::default()));
        }
        Vec::new()
    }
}

/// The command that sets the top-level key `key` to `value` in the
/// draft, or leaves it at its default with `None`.
fn top(key: &'static str, value: Option<String>) -> Vec<Command> {
    vec![Command::Change(Change::Top { key, value })]
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
