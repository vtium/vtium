//! The Settings tab's part of the scenario walk.

use vtium_config::tui::model::{App, View};
use vtium_settings::keys::Kind;

use super::super::state::Shown;
use super::Tab;

pub const TAB: Tab = Tab {
    bar: "settings",
    views: &[(View::Settings, "settings")],
    narrow: &[
        ("settings.section.typing", "settings"),
        ("settings.section", "settings-section"),
        ("settings.section.list", "settings-list"),
    ],
    focus,
    layer_focus,
    shown,
    today: &[],
};

/// On the list of sections, the first section apart from the rest, so the
/// walk opens a section other than the first.
fn focus(app: &App) -> String {
    if app.settings.section_at() == 0 {
        "first".into()
    } else {
        "later".into()
    }
}

/// A Settings key's kind decides what Enter opens on it: its typing, or
/// its values as a list.
fn layer_focus(app: &App, narrow: &str) -> Option<String> {
    (narrow == "settings-section").then(|| key_kind(app.settings.key()))
}

/// The kind of a Settings key, as far as Enter on it differs: a switch or
/// a choice opens its values as a list, and a key binding or any other
/// key starts typing.
fn key_kind(key: Option<&vtium_settings::keys::Key>) -> String {
    match key.map(|k| k.kind) {
        Some(Kind::Switch) => "switch",
        Some(Kind::Choice(_) | Kind::Name(_)) => "choice",
        Some(Kind::KeyBinding) => "binding",
        Some(_) => "value",
        None => "none",
    }
    .into()
}

/// The Settings tab says under its list what Enter does there, which tells
/// the list of sections, a section, a list of values and a value being
/// typed apart.
fn shown(s: &Shown) -> &'static [&'static str] {
    if s.any(" Press Enter to open a section") {
        &["settings"]
    } else if s.any(" Press Enter to type a setting's value") {
        &["settings.section"]
    } else if s.any(" Press Enter to pick the highlighted value") {
        &["settings.section.list"]
    } else if s.any(" Type the value and press Enter") {
        &["settings.section.typing"]
    } else {
        &[]
    }
}
