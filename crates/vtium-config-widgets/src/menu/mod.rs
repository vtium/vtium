//! The menus as they are drawn: the File and Actions menus that pull down
//! from their names on the top row, the submenus that open to the right of
//! their items, and the small menu that `[.] More` opens over the action
//! bar. [`sheet`] draws the frame of a sheet, such as the Problems list.
//!
//! A menu's items follow IBM's Common User Access conventions. A submenu
//! opens to the right of its item, which ends with ›. An item that turns
//! something on or off carries a check mark while it is on. Separators
//! group the items. Each shortcut sits right-aligned in a column of its
//! own, written bare, and the highlighted item is a bar across the menu's
//! whole width. Each item's own letter, which chooses it while the menu is
//! open, is bold and underlined. An item that cannot act here is greyed
//! out, with its reason beside it after a dot, so the reason says it where
//! no color shows.
//!
//! Which menu is open, its highlighted item and its open submenu are a
//! value the shell's navigation module holds. This module draws that value
//! and returns the places of the items, so a click maps to the item it
//! lands on.
//!
//! References:
//! - ADR-1403 D11, "The menus": what an item shows, each item's letter,
//!   and the greyed item with its reason.
//! - ADR-1206 D3: the navigation module owns the menus' state and their
//!   keys and clicks; vtium-config draws its own menus, as ratatui has no
//!   menu widget.
//! - ADR-1206 D4, the roles `menu`, `menu-highlight`, `menu-greyed` and
//!   `letter`.

mod draw;
pub mod sheet;

pub use draw::{MenuLayout, layout};

use ratatui_core::layout::{Position, Rect};

use crate::tab::MenuItem;

/// An item of a menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    /// The item's name, such as `Save…` or `History`.
    pub label: String,
    /// The item's own letter inside the open menu, drawn bold and
    /// underlined where it first appears in the label, in either case.
    pub letter: Option<char>,
    /// The key that does the same with no menu open, written bare in the
    /// column of shortcuts, such as `Ctrl+S`.
    pub shortcut: Option<String>,
    /// Whether the item opens a submenu, which marks it with ›.
    pub submenu: bool,
    /// For an item that turns something on or off, whether it is on now.
    /// `None` for any other item.
    pub check: Option<bool>,
    /// Why the item cannot act here, such as `nothing to undo`, where it
    /// cannot. The item is then greyed out with the reason beside it.
    pub greyed: Option<String>,
}

impl Item {
    /// An item named `label`.
    pub fn new(label: impl Into<String>) -> Item {
        Item {
            label: label.into(),
            ..Item::default()
        }
    }

    /// The item with its own letter `letter`.
    pub fn letter(mut self, letter: char) -> Item {
        self.letter = Some(letter);
        self
    }

    /// The item with the shortcut `key`, in the bar's form.
    pub fn shortcut(mut self, key: impl Into<String>) -> Item {
        self.shortcut = Some(key.into());
        self
    }

    /// The item marked as opening a submenu.
    pub fn submenu(mut self) -> Item {
        self.submenu = true;
        self
    }

    /// The item as one that turns something on or off, on now where `on`.
    pub fn check(mut self, on: bool) -> Item {
        self.check = Some(on);
        self
    }

    /// The item greyed out, with `reason` beside it.
    pub fn greyed(mut self, reason: impl Into<String>) -> Item {
        self.greyed = Some(reason.into());
        self
    }
}

impl<A> From<&MenuItem<A>> for Item {
    fn from(item: &MenuItem<A>) -> Item {
        Item {
            label: item.label.clone(),
            letter: item.letter,
            shortcut: item.shortcut.map(str::to_owned),
            submenu: false,
            check: None,
            greyed: item.greyed.clone(),
        }
    }
}

/// A line of a menu: an item, or a separator between groups of items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Item(Item),
    Separator,
}

/// One open menu: its entries, the highlighted one and the first one
/// shown, by their indexes in `entries`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pull<'a> {
    pub entries: &'a [Entry],
    /// The highlighted entry, if any.
    pub highlight: Option<usize>,
    /// The first entry shown, where the screen cannot hold them all. The
    /// navigation module keeps the highlight in view, and
    /// [`rows_shown`] says how many rows the menu has for its entries.
    pub offset: usize,
}

impl<'a> Pull<'a> {
    /// `entries`, with nothing highlighted, from the first.
    pub fn new(entries: &'a [Entry]) -> Pull<'a> {
        Pull {
            entries,
            highlight: None,
            offset: 0,
        }
    }

    /// The same menu with the entry `i` highlighted.
    pub fn highlight(self, i: usize) -> Pull<'a> {
        Pull {
            highlight: Some(i),
            ..self
        }
    }
}

/// Which way a menu opens from its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    /// Down from the anchor, as a menu pulls down from the top row. The
    /// box's top border lies on the anchor's row.
    Down,
    /// Up from the anchor, as More's menu opens over the action bar. The
    /// box's bottom border lies on the anchor's row.
    Up,
}

/// A menu that is open, and its submenu if one is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open<'a> {
    /// Where the menu's box starts: its left column, and the row its top
    /// border lies on, or its bottom border where it opens up.
    pub at: Position,
    pub opens: Opens,
    pub menu: Pull<'a>,
    /// The submenu that is open: the index of the item it belongs to, and
    /// the submenu's own entries.
    pub sub: Option<(usize, Pull<'a>)>,
}

/// An item a click lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// An entry of the menu, by its index.
    Item(usize),
    /// An entry of the open submenu, by its index.
    Sub(usize),
}

/// What a click hits while a menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuHit {
    /// An item, which the click chooses.
    Item(Pick),
    /// The menu's box but no item, such as a border or a separator, where
    /// a click does nothing.
    Inside,
    /// Outside the menu and its submenu, where a click closes the menu.
    Outside,
}

/// The places of an open menu's boxes and items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuPlaces {
    /// The menu's box, and the submenu's after it where one is open.
    pub boxes: Vec<Rect>,
    /// Each item shown and its row, across the box's whole width.
    pub items: Vec<(Rect, Pick)>,
}

impl MenuPlaces {
    /// What a click at `at` hits.
    pub fn hit(&self, at: Position) -> MenuHit {
        if let Some((_, pick)) = self.items.iter().find(|(r, _)| r.contains(at)) {
            MenuHit::Item(*pick)
        } else if self.boxes.iter().any(|r| r.contains(at)) {
            MenuHit::Inside
        } else {
            MenuHit::Outside
        }
    }
}

/// How many of a menu's `entries` entries show from `offset` when it
/// opens at `at` in `area`, so the navigation module can keep the
/// highlight in view. Where the screen holds fewer rows than entries, a
/// row at each end that has more beyond it says how many.
pub fn rows_shown(area: Rect, at: Position, opens: Opens, entries: usize, offset: usize) -> usize {
    draw::window(draw::room(area, at, opens), entries, offset).count
}
