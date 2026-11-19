//! The Fonts tab's part of the scenario walk. The tab lists the font sets
//! today, and `n` opens the form for a new font set, which belongs to the
//! Looks tab.

use vtium_config::tui::model::{App, FontState, View};

use super::super::state::Shown;
use super::{Tab, no_layer_focus};

pub const TAB: Tab = Tab {
    bar: "fonts",
    views: &[(View::Fonts, "fonts")],
    narrow: &[],
    focus,
    layer_focus: no_layer_focus,
    shown,
    today: TODAY,
};

/// What the TUI does today where the tables plan something else, as their
/// notes say, so that the walks reach a value being typed. `n.view.fonts`
/// says that `n` in the Fonts tab opens the form for a new font set, which
/// the tree's `looks.new-frit.typing` opens on its first field.
const TODAY: &[(&str, &str, &str)] = &[("fonts", "n", "looks.new-frit.typing")];

/// The kind of font the highlight is on in the list, and the row the
/// form for a new font set is on, since `n` opens the form there.
fn focus(app: &App) -> String {
    let kind = match app.font() {
        None => "none",
        Some(font) if app.shared.pending.removals.contains(&font.name) => "marked",
        Some(font) => match font.state {
            FontState::Reads => "frit",
            FontState::BuiltIn => "builtin",
            FontState::Refused(_) => "refused",
        },
    };
    // the row the new-frit form is on decides where `n` opens it,
    // so a form left on another row than the first is a state of
    // its own
    match app.looks.form().at {
        0 => kind.into(),
        row => format!("{kind}, form on row {row}"),
    }
}

/// The Fonts tab shows its list alone.
fn shown(_: &Shown) -> &'static [&'static str] {
    &["fonts"]
}
