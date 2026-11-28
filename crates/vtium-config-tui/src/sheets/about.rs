//! F1's help, as a sheet over the screen. F1 opens, first, the rest of
//! the message on the viewer's last row where the message has more to
//! it, since the message is the newest thing on the screen; then the full
//! text behind the highlighted line where the open sheet or tab cut it to
//! one line; otherwise the list of every key of the screen the person is
//! on, then the keys that work everywhere. The list comes from the
//! compiled key table, so it is the same list the Help tab and the action
//! bar read. Up, Down, PgUp, PgDn, Home and End scroll it, and Esc closes
//! it.
//!
//! References:
//! - ADR-1403 D11, "F1, help for what is highlighted": the three things
//!   F1 opens, in that order, and where it does nothing.
//! - ADR-1206 D3, "The navigation module owns F1's help and Find": F1
//!   asks the message line first, then the open tab, then the tables.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_widgets::contract;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;

use super::Act;

/// The keys that work everywhere, by their rows, in the order F1's list
/// gives them after the screen's own.
const EVERYWHERE: &[&str] = &[
    "f10.view",
    "alt-f.view",
    "alt-a.view",
    "ctrl-s.view",
    "ctrl-z.view",
    "ctrl-y.view",
    "ctrl-f.view",
    "f1.view",
    "alt-p.view",
    "tab.view",
    "shift-tab.view",
    "q.view",
];

/// The title of the sheet that holds the rest of a message.
pub const MESSAGE: &str = "The message in full";

/// The heading of the keys that work everywhere.
const EVERYWHERE_HEADING: &str = "Everywhere";

/// F1's sheet: its title, its lines, and the first line in view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct About {
    pub title: String,
    lines: Vec<(String, Role)>,
    top: usize,
}

impl About {
    /// The sheet of `text` under `title`, one line of the sheet for each
    /// line of the text.
    pub fn text(title: impl Into<String>, text: &str) -> About {
        About {
            title: title.into(),
            lines: text.lines().map(|l| (l.to_owned(), Role::Row)).collect(),
            top: 0,
        }
    }

    /// The list of every key of the node `node`, which the screen called
    /// `screen` is, then the keys that work everywhere.
    pub fn keys(screen: &str, node: &str) -> About {
        let mut lines = Vec::new();
        let own: Vec<_> = contract::labelled(node)
            .into_iter()
            .filter(|r| !EVERYWHERE.contains(&r.id) && r.bar_key != "Ctrl+C")
            .collect();
        for row in &own {
            lines.push((key_line(row.bar_key, row.label, row.means), Role::Row));
        }
        lines.push((String::new(), Role::Row));
        lines.push((EVERYWHERE_HEADING.to_owned(), Role::Heading));
        for row in EVERYWHERE.iter().filter_map(|id| contract::row(id)) {
            lines.push((key_line(row.bar_key, row.label, row.means), Role::Row));
        }
        About {
            title: format!("Keys of {screen}"),
            lines,
            top: 0,
        }
    }

    /// How many lines the sheet holds.
    pub fn lines(&self) -> usize {
        self.lines.len()
    }

    /// Acts on `key`, which chose the row `id` of `any.about`.
    pub fn key(&mut self, key: &Key, id: &str) -> Act {
        match id {
            "esc.layer.about" => Act::Close,
            "scroll.layer.about" => {
                let last = self.lines.len().saturating_sub(1);
                self.top = match key.code {
                    Code::Up => self.top.saturating_sub(1),
                    Code::Down => self.top.saturating_add(1).min(last),
                    Code::PageUp => self.top.saturating_sub(PAGE),
                    Code::PageDown => self.top.saturating_add(PAGE).min(last),
                    Code::Home => 0,
                    Code::End => last,
                    _ => self.top,
                };
                Act::Stay
            }
            // F1 does nothing over its own help, and the sheet offers no
            // search
            "f1.layer.about" | "question.layer" => Act::Stay,
            _ => Act::Pass,
        }
    }

    /// The sheet's box in `viewer`: wide enough for its longest line,
    /// within the viewer less a column at each side.
    pub fn area(&self, viewer: Rect) -> Rect {
        let longest = self
            .lines
            .iter()
            .map(|(l, _)| l.chars().count())
            .chain(std::iter::once(
                self.title.chars().count().saturating_add(6),
            ))
            .max()
            .unwrap_or(0);
        let width = u16::try_from(longest.saturating_add(4))
            .unwrap_or(u16::MAX)
            .min(viewer.width.saturating_sub(2));
        super::top_box(viewer, width, self.lines.len())
    }

    /// Draws the lines in view into `area`, inside the sheet's box.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let width = usize::from(area.width);
        for (y, (line, role)) in self
            .lines
            .iter()
            .skip(self.top)
            .take(usize::from(area.height))
            .enumerate()
        {
            let y = u16::try_from(y).unwrap_or(u16::MAX);
            super::put(buf, area, 0, y, &super::cut(line, width), *role, color);
        }
    }
}

/// The step of PgUp and PgDn in the help.
const PAGE: usize = 10;

/// One key of F1's list: the key in brackets, its name, and what it does.
fn key_line(key: &str, label: &str, means: &str) -> String {
    let key = format!("[{key}]");
    let mut label = label.to_owned();
    if let Some(first) = label.get(..1) {
        label = format!("{}{}", first.to_uppercase(), label.get(1..).unwrap_or(""));
    }
    format!("{key:<10} {label:<16} {means}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_of_a_screen_come_before_the_keys_that_work_everywhere() {
        let about = About::keys("Fonts", "fonts");
        let at = |needle: &str| {
            about
                .lines
                .iter()
                .position(|(l, _)| l.contains(needle))
                .unwrap_or(usize::MAX)
        };
        let everywhere = at(EVERYWHERE_HEADING);
        assert!(everywhere < about.lines.len());
        assert!(at("[Ctrl+F]") > everywhere);
        assert!(at("[Alt+P]") > everywhere);
        assert!(about.lines.iter().take(everywhere).count() > 2);
    }
}
