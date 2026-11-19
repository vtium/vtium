//! The Effects tab's part of the scenario walk. The tab shows one line
//! that says it is coming, so the walk reaches only the tab itself.

use vtium_config::tui::model::View;

use super::super::state::Shown;
use super::{Tab, no_focus, no_layer_focus};

pub const TAB: Tab = Tab {
    bar: "effects",
    views: &[(View::Effects, "effects")],
    narrow: &[],
    focus: no_focus,
    layer_focus: no_layer_focus,
    shown,
    today: &[],
};

fn shown(_: &Shown) -> &'static [&'static str] {
    &["effects"]
}
