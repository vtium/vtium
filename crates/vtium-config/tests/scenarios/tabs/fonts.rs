//! The Fonts tab's part of the scenario walk: the tab's list of font
//! files, the list of filters that its filter line opens, and a font's
//! detail.

use vtium_config::tui::model::{App, View};
use vtium_config::tui::tabs::fonts::uses;

use super::super::state::Shown;
use super::Tab;

pub const TAB: Tab = Tab {
    bar: "fonts",
    views: &[(View::Fonts, "fonts")],
    narrow: &[("fonts.filter", "list"), ("fonts.font", "detail")],
    focus,
    layer_focus,
    shown,
    today: &[],
};

/// What the highlight is on decides what Enter and Space do: the filter
/// line opens the list of filters, and a font's marks decide whether Space
/// activates it, deactivates it, asks first, or only says why it cannot,
/// as for a font in the person's own folder, whose detail offers the
/// install as a system font.
fn focus(app: &App) -> String {
    let Some(font) = app.font() else {
        return "filter".into();
    };
    let unused = uses(&app.shared, font.path()).is_empty();
    let used = if unused { "" } else { ", used" };
    format!("font: {}{used}", font.resource.row())
}

/// Whose font a detail shows decides what Enter does there: in the
/// detail of a font of the person's own, Enter asks whether to install it
/// as a system font, and in any other detail it does nothing.
fn layer_focus(app: &App, narrow: &str) -> Option<String> {
    let font = app.font().filter(|_| narrow == "detail")?;
    Some(
        if font.resource.own {
            "a font of yours"
        } else {
            "a system font"
        }
        .to_owned(),
    )
}

/// The list starts with its filter line and ends with the sentence that
/// says what its keys do, which stays when the filter line scrolls out of
/// a short window. The list of filters marks each filter with a circle,
/// and a detail starts with the font it describes.
fn shown(s: &Shown) -> &'static [&'static str] {
    if s.any(" Show: ") || s.any(" Space activates or deactivates the highlighted font.") {
        &["fonts"]
    } else if s.any("   ● ") || s.any("   ○ ") {
        &["fonts.filter"]
    } else if s.any(" font ") {
        &["fonts.font"]
    } else {
        &[]
    }
}
