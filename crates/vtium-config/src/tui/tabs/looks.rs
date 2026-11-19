//! The Looks tab. Until looks are built, it lists the sizes the console's
//! font takes, automatic first, and Enter sets the highlighted size as the
//! draft's `font-size`.
//!
//! References:
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::input::Key;
use super::super::listbox::{ListBox, step};
use super::super::model::SIZES;
use super::{Change, Command};

/// Where the Looks tab stands: the highlighted size, counted from 0 for
/// automatic, and the window of the list on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Looks {
    list: ListBox,
}

impl Looks {
    /// The highlighted row, counted from 0 for automatic.
    pub fn at(&self) -> usize {
        self.list.at
    }

    /// The first row the window of the list shows.
    pub fn offset(&self) -> usize {
        self.list.offset
    }

    /// Keeps the list in view from its row `row`, which the layout gives
    /// after every event.
    pub fn show_from(&mut self, row: usize) {
        self.list.offset = row;
    }

    /// The highlighted size, in points, 0 for automatic.
    pub fn points(&self) -> u32 {
        match self.list.at {
            0 => 0,
            i => SIZES.get(i.saturating_sub(1)).copied().unwrap_or(0),
        }
    }

    /// The node of the interface tree the tab is at, which has no level
    /// that opens over it.
    pub fn node(&self) -> &'static str {
        "looks"
    }

    /// A key the navigation module hands to the tab: the moving keys move
    /// the highlight, and Enter sets the highlighted size as the draft's
    /// `font-size`.
    pub fn update(&mut self, key: Key) -> Vec<Command> {
        self.list.at = step(self.list.at, SIZES.len().saturating_add(1), key);
        if key != Key::Enter {
            return Vec::new();
        }
        vec![Command::Change(Change::Top {
            key: "font-size",
            value: Some(self.points().to_string()),
        })]
    }
}
