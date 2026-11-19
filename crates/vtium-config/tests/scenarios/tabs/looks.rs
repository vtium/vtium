//! The Looks tab's part of the scenario walk. The tab lists sizes today,
//! and every size takes the same keys, so the highlight decides nothing.

use vtium_config::tui::model::View;

use super::super::state::Shown;
use super::{Tab, no_focus, no_layer_focus};

pub const TAB: Tab = Tab {
    bar: "looks",
    views: &[(View::Looks, "looks")],
    narrow: &[],
    focus: no_focus,
    layer_focus: no_layer_focus,
    shown,
    today: &[],
};

fn shown(_: &Shown) -> &'static [&'static str] {
    &["looks"]
}
