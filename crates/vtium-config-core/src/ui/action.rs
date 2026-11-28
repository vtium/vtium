//! The action the navigation module hands the open tab: the row of the key
//! table that the key or the click chose, with what the key carries.
//!
//! References:
//! - ADR-1206 D2: update hands a key to the navigation module, which answers with an action from the key table.
//! - ADR-1206 D3: the navigation module reads the compiled tables, and a menu's chosen item becomes the action its row names.

use vtium_config_widgets::input::Key;

/// One action: a key table row, by its id, as `ctrl-z.view` or
/// `enter.layer.manage`, and what the key carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    /// The row's id in the compiled key table.
    pub row: &'static str,
    pub carries: Carries,
}

/// What an action carries beyond its row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Carries {
    /// The row says all there is, as Undo's does.
    Nothing,
    /// The key itself, where the row acts on any key of a kind: a letter
    /// typed into a value or a pattern, a digit, or a click with its
    /// place on the screen.
    Key(Key),
    /// The item the action picks, by the tab's own index: a list row that
    /// was clicked, or the match of Find that the person goes to.
    Item(usize),
}

impl Action {
    /// The action of the row `row`, carrying nothing.
    pub const fn of(row: &'static str) -> Action {
        Action {
            row,
            carries: Carries::Nothing,
        }
    }

    /// The action of the row `row` for the key `key`.
    pub const fn key(row: &'static str, key: Key) -> Action {
        Action {
            row,
            carries: Carries::Key(key),
        }
    }

    /// The action of the row `row` on the tab's item `item`.
    pub const fn item(row: &'static str, item: usize) -> Action {
        Action {
            row,
            carries: Carries::Item(item),
        }
    }

    /// Whether the action is the row `row`'s.
    pub fn is(&self, row: &str) -> bool {
        self.row == row
    }
}
