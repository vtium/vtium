//! The Help tab's part of the scenario walk.

use vtium_config::tui::model::{App, View};

use super::super::state::Shown;
use super::{Tab, no_layer_focus};

pub const TAB: Tab = Tab {
    bar: "help",
    views: &[(View::Help, "help")],
    narrow: &[],
    focus,
    layer_focus: no_layer_focus,
    shown,
    today: &[],
};

/// The page that shows: the keys and commands, or About.
fn focus(app: &App) -> String {
    if app.help.about() { "about" } else { "help" }.into()
}

fn shown(_: &Shown) -> &'static [&'static str] {
    &["help"]
}
