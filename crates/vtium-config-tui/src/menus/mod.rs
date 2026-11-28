//! The File and Actions menus as lists of items: each item's name, its own
//! letter inside the open menu, its shortcut, its submenu or check mark,
//! and the action that choosing it hands update. [`file`](mod@file) builds the File
//! menu and [`actions`] the Actions menu. The navigation module moves
//! through these lists, and the view draws them through
//! vtium-config-widgets' menus.
//!
//! An item whose action the shell does not handle yet is greyed out with
//! the reason "not built yet", so nothing on a menu pretends to act. The
//! item comes alive once the card that builds it routes its action in
//! [`crate::app::update`].
//!
//! References:
//! - ADR-1403 D11, "The menus": the items, letters, shortcuts, submenus and
//!   separators of File and Actions.
//! - ADR-1206 D3: a chosen item becomes the action its key table row
//!   names, and an item that cannot act here is greyed out with its
//!   reason.

pub mod actions;
pub mod file;

use vtium_config_core::ui::{Action, Shared};
use vtium_config_widgets::menu::{Entry, Item};

use crate::app::update;

/// The reason beside an item whose action no card has built yet.
pub const NOT_BUILT: &str = "not built yet";

/// What choosing an item does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    /// Hands update the action of the item's row.
    Act(Action),
    /// Opens the item's submenu.
    Sub(Submenu),
    /// Nothing yet: the item's action has no row of the key table.
    Nothing,
}

/// A submenu of the File menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submenu {
    /// File › Open: the configurations, the open one marked.
    Open,
    /// File › History: the versions installed before, newest first.
    History,
}

impl Submenu {
    /// The submenu's node of the interface tree.
    pub const fn node(self) -> &'static str {
        match self {
            Submenu::Open => "any.menu.file.open",
            Submenu::History => "any.menu.file.history",
        }
    }
}

/// One item of a menu and what choosing it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub item: Item,
    pub then: Then,
}

impl Choice {
    /// An item whose action is the row `row`'s, greyed out as not built
    /// yet unless the shell handles the row.
    pub fn acting(item: Item, row: &'static str) -> Choice {
        let item = if update::handled(row) {
            item
        } else {
            item.greyed(NOT_BUILT)
        };
        Choice {
            item,
            then: Then::Act(Action::of(row)),
        }
    }

    /// Whether choosing the item can act here: it is not greyed out.
    pub fn acts(&self) -> bool {
        self.item.greyed.is_none()
    }
}

/// A line of a menu: an item, or a separator between groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Choice(Choice),
    Separator,
}

impl Line {
    /// The line's item, where it is one.
    pub fn choice(&self) -> Option<&Choice> {
        match self {
            Line::Choice(c) => Some(c),
            Line::Separator => None,
        }
    }
}

/// What the menus read to build their items.
pub struct Context<'a> {
    /// The shared state: the document, its undo history and its desk.
    pub shared: &'a Shared,
    /// The node of the tab under the menu, whose rows decide which
    /// shortcuts the File menu shows, such as `n` on the Flow tab.
    pub node: &'static str,
}

/// The lines as vtium-config-widgets' menus draw them.
pub fn entries(lines: &[Line]) -> Vec<Entry> {
    lines
        .iter()
        .map(|l| match l {
            Line::Choice(c) => Entry::Item(c.item.clone()),
            Line::Separator => Entry::Separator,
        })
        .collect()
}

/// The lines of the submenu `which`.
pub fn submenu(which: Submenu, cx: &Context<'_>) -> Vec<Line> {
    match which {
        Submenu::Open => file::open(cx),
        Submenu::History => file::history(cx),
    }
}
