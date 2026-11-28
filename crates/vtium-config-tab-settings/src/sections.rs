//! The sections of vtium.conf's top-level keys, as the tab shows them, and
//! the left pane that lists them. The sections, their groups and the order
//! of their keys come from vtium-settings' `SECTIONS`, the table that the
//! example vtium.conf and docs/config.md also follow, so the three never
//! disagree on where a key belongs. The tab lists only the keys a
//! configuration file sets at its top level: a key that only the command
//! line takes, or that only a look's or a head's section sets, is left
//! out, and so is a group or a section left with no key.
//!
//! Every key the tab shows sits once in one flat list, in the sections'
//! order, so a key has one number everywhere: in the search's matches, in
//! Find's items and in the tab's highlight. A section's keys are a run of
//! that list.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Settings bullet: the sections
//!   at the left, and the highlighted section's keys at the right.
//! - ADR-1201 D3, the Settings bullet: the sections, then their groups of
//!   keys.

use std::ops::Range;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::commands::value_text;
use vtium_config_core::document::Document;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_settings::keys::{self, Key, Kind, Scope};

use crate::words;

/// One section: its title, what it covers, its groups' titles, and the
/// run of the flat list that holds its keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub about: &'static str,
    pub groups: Vec<&'static str>,
    pub keys: Range<usize>,
}

/// One key of the flat list: the key, its section and its group, each by
/// its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: &'static Key,
    pub section: usize,
    pub group: usize,
}

/// The sections and every key they show, built once from the key table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub sections: Vec<Section>,
    pub entries: Vec<Entry>,
}

impl Table {
    /// The sections in the key table's order, each with the groups and
    /// keys a configuration file sets at its top level.
    pub fn new() -> Table {
        let mut sections = Vec::new();
        let mut entries = Vec::new();
        for s in &keys::SECTIONS {
            let first = entries.len();
            let mut groups = Vec::new();
            for g in s.groups {
                let shown: Vec<&'static Key> = g
                    .keys
                    .iter()
                    .copied()
                    .filter(|k| k.scope == Scope::Anywhere)
                    .collect();
                if shown.is_empty() {
                    continue;
                }
                let group = groups.len();
                groups.push(g.title);
                entries.extend(shown.into_iter().map(|key| Entry {
                    key,
                    section: sections.len(),
                    group,
                }));
            }
            if entries.len() > first {
                sections.push(Section {
                    title: s.title,
                    about: s.about,
                    groups,
                    keys: first..entries.len(),
                });
            }
        }
        Table { sections, entries }
    }

    /// The key numbered `at` in the flat list.
    pub fn entry(&self, at: usize) -> Option<&Entry> {
        self.entries.get(at)
    }

    /// The section numbered `at`.
    pub fn section(&self, at: usize) -> Option<&Section> {
        self.sections.get(at)
    }
}

impl Default for Table {
    fn default() -> Table {
        Table::new()
    }
}

/// A key's value in the open configuration, as the keys' pane shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    /// The value as `vtium config get` prints it, or `none` for a key
    /// binding that binds no key.
    pub text: String,
    /// Whether the configuration leaves the key at vtium's default.
    pub default: bool,
    /// Whether the configuration gives the key another value than the
    /// installed file, a change not saved.
    pub changed: bool,
}

impl Value {
    /// The mark beside the value: `changed`, `default`, or none for a
    /// value the installed file already gives.
    pub fn mark(&self) -> Option<&'static str> {
        if self.changed {
            Some(words::CHANGED)
        } else if self.default {
            Some(words::DEFAULT)
        } else {
            None
        }
    }

    /// The value and its mark on one line, as Find's fact and the full
    /// help write them.
    pub fn line(&self) -> String {
        match self.mark() {
            Some(mark) => format!("{}, {mark}", self.text),
            None => self.text.clone(),
        }
    }
}

/// The value `document` gives `key`, read from its vtium.conf as
/// `vtium config get` reads it, and compared with the installed file's.
pub fn value(key: &'static Key, document: &Document) -> Value {
    let now = value_text(key, Some(document.text()));
    let installed = value_text(key, document.installed().text.as_deref());
    let changed = now != installed;
    let (text, set) = now;
    let text = if set && text.is_empty() && key.kind == Kind::KeyBinding {
        words::NO_KEY.to_owned()
    } else {
        text
    };
    Value {
        text,
        default: !set,
        changed,
    }
}

/// How the left pane marks the highlighted section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// The sections have the focus: the row in reverse video.
    Focus,
    /// The keys have the focus: the section's row in bold, so it stays
    /// marked as the keys' section without color too.
    Open,
}

/// One row of the left pane: the section's number and the count it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub section: usize,
    pub count: usize,
}

/// The rows the left pane's sections start on, below the title and a
/// blank row.
pub const FIRST_ROW: u16 = 2;

/// What the left pane shows: `shown`'s sections from `offset`, each with
/// its count, the highlighted one marked as `highlight` says, and `title`
/// after `Settings` while the search filters, as `matching *bright`.
pub struct Pane<'a> {
    pub table: &'a Table,
    pub shown: &'a [Shown],
    pub highlight: Option<(usize, Mark)>,
    pub offset: usize,
    pub title: Option<&'a str>,
}

impl Pane<'_> {
    /// Draws the pane in `rect`.
    pub fn draw(&self, buf: &mut Buffer, rect: Rect, color: bool) {
        let width = usize::from(rect.width);
        let heading = Role::Heading.style(color).cell();
        let dim = Role::Dim.style(color).cell();
        let row = Role::Row.style(color).cell();
        let used = put(
            buf,
            rect.x.saturating_add(1),
            rect.y,
            words::SETTINGS,
            heading,
            rect,
        );
        if let Some(title) = self.title {
            put(buf, used.saturating_add(1), rect.y, title, dim, rect);
        }
        // `  NAME  COUNT `: two blanks before the name, the count's three
        // columns and a blank after it
        let name_cells = width.saturating_sub(8);
        let line_cells = width.saturating_sub(1);
        for (i, s) in self.shown.iter().skip(self.offset).enumerate() {
            let Ok(dy) = u16::try_from(i) else { break };
            let y = rect.y.saturating_add(FIRST_ROW).saturating_add(dy);
            if y >= rect.bottom() {
                break;
            }
            let Some(section) = self.table.section(s.section) else {
                continue;
            };
            let name = canvas::fit(section.title, name_cells);
            let line = format!("  {name:<name_cells$}{:>3} ", s.count);
            let line = canvas::fit(&line, line_cells);
            let style = match self.highlight {
                Some((h, Mark::Focus)) if h == s.section => Role::RowHighlight.style(color).cell(),
                Some((h, Mark::Open)) if h == s.section => heading,
                _ => row,
            };
            let line = format!("{line:<line_cells$}");
            put(buf, rect.x.saturating_add(1), y, &line, style, rect);
        }
    }
}

/// How many sections the left pane in `rect` shows at once.
pub fn rows(rect: Rect) -> usize {
    usize::from(rect.height.saturating_sub(FIRST_ROW))
}

/// Writes `text` at `x`, `y` in `style`, no further than `rect`'s right
/// edge, and returns the column after it.
pub fn put(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    style: ratatui_core::style::Style,
    rect: Rect,
) -> u16 {
    if y >= rect.bottom() || x >= rect.right() {
        return x;
    }
    let room = usize::from(rect.right().saturating_sub(x));
    buf.set_stringn(x, y, canvas::printable(text), room, style)
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key a configuration file sets at its top level is in one
    /// section, once, so no key is out of reach.
    #[test]
    fn every_top_level_key_is_in_one_section_once() {
        let table = Table::new();
        let shown: Vec<&str> = table.entries.iter().map(|e| e.key.name).collect();
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
        assert_eq!(shown.len(), takes.len());
    }

    #[test]
    fn each_section_s_keys_are_a_run_of_the_flat_list() {
        let table = Table::new();
        let mut next = 0;
        for (n, s) in table.sections.iter().enumerate() {
            assert_eq!(s.keys.start, next, "{}", s.title);
            assert!(!s.keys.is_empty(), "{} has no key", s.title);
            let run = table.entries.get(s.keys.clone()).unwrap_or_default();
            assert!(run.iter().all(|e| e.section == n));
            next = s.keys.end;
        }
        assert_eq!(next, table.entries.len());
        assert_eq!(
            table.sections.first().map(|s| s.title),
            Some("Fonts and text")
        );
    }
}
