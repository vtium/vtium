//! Each tab's part of the scenario walk: the names the abstract state
//! gives the tab's views, what the highlight is on, the names the key table
//! narrows the tab's layers with, the nodes its screen can show, and what
//! the TUI does today where the tables plan something else. Each tab keeps
//! its part in a file of its own, and [`TABS`] lists them, so a new tab adds
//! its file and one line here.

pub mod effects;
pub mod flow;
pub mod fonts;
pub mod help;
pub mod looks;
pub mod settings;

use vtium_config::tui::model::{App, View};

use super::state::Shown;

/// What the scenario walk knows of one tab.
pub struct Tab {
    /// The tab's name as the tab bar highlights it, in lower case.
    pub bar: &'static str,
    /// The views the tab shows, each with the name the abstract state
    /// gives it.
    pub views: &'static [(View, &'static str)],
    /// The name the key table narrows each of the tab's layers and typed
    /// values with, by the node of the tree it is.
    pub narrow: &'static [(&'static str, &'static str)],
    /// What the highlight is on in the tab with nothing open over it,
    /// where the keys act on it differently.
    pub focus: fn(&App) -> String,
    /// What the highlight is on in the layer the key table narrows with
    /// the name given, where it decides what Enter opens; `None` for a
    /// layer of another tab, or one where it decides nothing.
    pub layer_focus: fn(&App, &str) -> Option<String>,
    /// The nodes the screen can be showing while the tab bar highlights
    /// the tab.
    pub shown: fn(&Shown) -> &'static [&'static str],
    /// What the TUI does today where the tables plan something else: the
    /// node, the key, and the node the key goes to.
    pub today: &'static [(&'static str, &'static str, &'static str)],
}

/// Every tab's part of the walk.
pub const TABS: [&Tab; 6] = [
    &flow::TAB,
    &fonts::TAB,
    &effects::TAB,
    &looks::TAB,
    &settings::TAB,
    &help::TAB,
];

/// The part of the walk of the tab that shows `view`.
pub fn of(view: View) -> &'static Tab {
    TABS.iter()
        .copied()
        .find(|t| t.views.iter().any(|(v, _)| *v == view))
        .unwrap_or(&flow::TAB)
}

/// A tab's focus where the highlight decides nothing.
pub fn no_focus(_: &App) -> String {
    String::new()
}

/// A tab's layer focus where no layer's highlight decides what Enter
/// opens.
pub fn no_layer_focus(_: &App, _: &str) -> Option<String> {
    None
}
