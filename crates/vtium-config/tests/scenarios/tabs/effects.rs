//! The Effects tab's part of the scenario walk: the tab's list, the list
//! of groups that its filter line opens, and an item's detail.

use vtium_config::tui::model::{App, View};
use vtium_config::tui::resources::looks_using_file;
use vtium_config::tui::tabs::effects::Effects;

use super::super::state::Shown;
use super::{Tab, no_layer_focus};

pub const TAB: Tab = Tab {
    bar: "effects",
    views: &[(View::Effects, "effects")],
    narrow: &[("effects.filter", "list"), ("effects.effect", "detail")],
    focus,
    layer_focus: no_layer_focus,
    shown,
    today: &[],
};

/// What the highlight is on decides what Enter and Space do: the filter
/// line opens the list of groups, a vtlook opens its detail and takes no
/// Space, and a color map's marks decide whether Space activates it,
/// deactivates it, asks first, or only says why it cannot.
fn focus(app: &App) -> String {
    let shared = &app.shared;
    let Some(item) = app.effects.highlighted(shared) else {
        return "filter".into();
    };
    let Some(r) = Effects::resource(shared, item) else {
        return "vtlook".into();
    };
    let conf = shared.conf_text().unwrap_or_default();
    let used = looks_using_file(conf, &shared.inputs.effects, &r.path).is_empty();
    let used = if used { "" } else { ", used" };
    format!("{}: {}{used}", r.kind, r.row())
}

/// The list starts with its filter line, the list of groups marks each
/// group with a circle, and a detail starts with what it describes.
fn shown(s: &Shown) -> &'static [&'static str] {
    if s.any(" Show: ") {
        &["effects"]
    } else if s.any("   ● ") || s.any("   ○ ") {
        &["effects.filter"]
    } else if s.any(" vtlook ") || s.any(" color map ") || s.any(" shader ") {
        &["effects.effect"]
    } else {
        &[]
    }
}
