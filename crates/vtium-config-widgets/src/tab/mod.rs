//! The seams between the TUI's shell and its tabs. Every tab implements
//! [`Tab`], and the shell holds the six tabs as `Box<dyn Tab<S, A, C>>`, so
//! it reaches a tab only through the trait and names no tab's crate. A tab
//! gains a shared behaviour by implementing that behaviour's own small
//! trait: [`Searchable`] for the `?` search over its list, and [`Choices`]
//! for the list box under a line with choices. Each trait gives default
//! methods, so a tab writes only what differs from them.
//!
//! The trait names the shared state, the action and the commands through
//! the type parameters `S`, `A` and `C`, because this crate sits below the
//! crate that defines them.
//!
//! References:
//! - ADR-1206 D9: the shell and the tabs are crates joined by traits with
//!   default methods, and the shell holds the tabs as trait objects.
//! - ADR-1206 D2: a tab's update takes an action and returns commands,
//!   and its view draws from the model without changing it.
//! - ADR-1206 D4: the navigation module asks each tab for its node, the
//!   full text behind its highlighted line and the items Find searches.
//! - ADR-1206 D3: the navigation state holds the search, and each tab
//!   keeps the indexes that match.
//! - ADR-1403 D11, "The menus": Actions › Install as a system font… acts
//!   on a font of the person's own, and is greyed out elsewhere.

mod choices;
mod search;

pub use choices::Choices;
pub use search::{Haystack, Searchable};

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;

use crate::contract::{self, Row};

/// What every tab of the TUI answers. `S` is the shared state every tab
/// reads, `A` the action the navigation module hands the open tab, and `C`
/// a command the tab's update returns.
pub trait Tab<S, A, C> {
    /// The tab's name on the top row, such as `Fonts`.
    fn name(&self) -> &'static str;

    /// The letter that, with Alt, goes to the tab, such as `n` for Fonts.
    /// The top row draws it bold and underlined in the tab's name.
    fn letter(&self) -> char;

    /// The node of the interface tree the tab is on: the tab's own node,
    /// or the level open in it, such as `fonts` or `fonts.font`.
    fn node(&self) -> &'static str;

    /// Draws the tab into `area`, the viewer's region that the frame gives
    /// it, from `shared` and the tab's own state. Drawing changes nothing:
    /// a selection the view needs to scroll comes from the tab's state.
    fn draw(&self, shared: &S, frame: &mut Frame<'_>, area: Rect);

    /// Acts on `action`, which the navigation module read from the key
    /// table's row for the key, and returns what follows: effects for the
    /// loop, changes to `shared`, and requests to the navigation module.
    /// The tab changes only its own state here, and `shared` through the
    /// commands it returns.
    fn update(&mut self, shared: &S, action: A) -> Vec<C>;

    /// The rows a key line and F1's list of keys show on the tab's node,
    /// in order of priority, from the compiled tables.
    fn key_line(&self) -> Vec<&'static Row> {
        contract::labelled(self.node())
    }

    /// The buttons of the action bar on the tab's node, in order of
    /// priority, from the compiled tables. The bar leaves out the lowest
    /// first where they do not fit.
    fn bar(&self) -> Vec<&'static Row> {
        contract::buttons(self.node())
    }

    /// The items the tab adds to the Actions menu, after the menu's own.
    /// A tab adds none unless it has an action of its own there.
    fn menu_items(&self, _shared: &S) -> Vec<MenuItem<A>> {
        Vec::new()
    }

    /// The full text behind the highlighted line, where the screen cut it
    /// to one line, which F1 opens. A tab that cuts no line has none.
    fn full_text(&self, _shared: &S) -> Option<String> {
        None
    }

    /// The items of the tab that Ctrl+F's Find searches. A tab that Find
    /// does not search has none.
    fn find_items(&self, _shared: &S) -> Vec<FindItem> {
        Vec::new()
    }

    /// The tab's `?` search, where the tab implements [`Searchable`]: such
    /// a tab answers `Some(self)`. A tab that offers no search answers
    /// `None`, and `?` then reaches no list in it.
    fn searchable(&self) -> Option<&dyn Searchable> {
        None
    }

    /// The `?` search row that the tab keeps itself, while one is open or
    /// kept, which the shell draws where the frame puts the search row. A
    /// tab whose search the shell holds, or that has none, answers `None`.
    fn search(&self) -> Option<&crate::search::Search> {
        None
    }

    /// Hands the tab the items that the shell's `?` search matches, by the
    /// tab's own indexes, after each key typed into the search row, so the
    /// tab lists only those. `None` clears the filter, and the tab lists
    /// every item again. A tab that offers no search ignores it.
    fn filtered(&mut self, _matches: Option<&[usize]>) {}

    /// Highlights the item named `name`, as another tab or Find asked when
    /// it sent the person here, such as a look's font set shown in the
    /// Fonts tab. A tab with no item of that name leaves its highlight
    /// where it is.
    fn show(&mut self, _shared: &S, _name: &str) {}

    /// The font file from the person's own folders that the tab
    /// highlights, which Actions › Install as a system font… copies into
    /// the system's fonts folder. A tab that highlights no such file
    /// answers `None`, and the menu greys the item out.
    fn own_font(&self, _shared: &S) -> Option<std::path::PathBuf> {
        None
    }
}

/// An item a tab adds to the Actions menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem<A> {
    /// The item's name, such as `Install as a system font`.
    pub label: String,
    /// The item's letter inside the open menu, drawn bold and underlined.
    pub letter: Option<char>,
    /// The key that does the same outside the menu, in the bar's form,
    /// which the menu's column of shortcuts writes bare.
    pub shortcut: Option<&'static str>,
    /// The action that choosing the item hands the tab.
    pub action: A,
    /// Why the item cannot act here, shown beside it greyed out. `None`
    /// where it can act.
    pub greyed: Option<String>,
}

/// An item that Find lists among its matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindItem {
    /// The item's name, which the search matches.
    pub name: String,
    /// The item's description, which the search also matches, where the
    /// item has one.
    pub description: Option<String>,
    /// Where the item lives, such as `Fonts` or `Settings › Fonts and text`.
    pub place: String,
    /// One fact about the item, shown beside it.
    pub fact: String,
    /// The tab's own index of the item, by which the tab highlights it
    /// when the person goes to the match.
    pub index: usize,
}
