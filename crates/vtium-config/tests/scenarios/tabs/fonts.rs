//! The Fonts tab's part of the scenario walk, with the new-frit form that
//! opens over the tab today.

use vtium_config::tui::model::{App, FontState, View};

use super::super::state::Shown;
use super::Tab;

pub const TAB: Tab = Tab {
    bar: "fonts",
    views: &[(View::Fonts, "fonts"), (View::New, "new-frit")],
    narrow: &[
        ("looks.new-frit.typing", "new-frit"),
        ("looks.new-frit", "new-frit"),
    ],
    focus,
    layer_focus,
    shown,
    today: TODAY,
};

/// What the TUI does today where the tables plan something else, as their
/// notes say, so that the walks reach a value being typed. `n.view.fonts`
/// says that `n` in the Fonts tab opens the form for a new font set, which
/// the tree's `looks.new-frit.typing` opens on its first field.
/// `esc.typing` says that Esc on a field of the form closes the form. The
/// form opens under the Fonts tab today, as the `now` column of
/// `looks.new-frit` says, so Esc on a field or on the add row goes back to
/// the Fonts tab.
const TODAY: &[(&str, &str, &str)] = &[
    ("fonts", "n", "looks.new-frit.typing"),
    ("looks.new-frit.typing", "Esc", "fonts"),
    ("looks.new-frit", "Esc", "fonts"),
];

/// The fields of the form for a new font set, as the TUI has them today:
/// the name, three fonts, the character cell and the size. Enter on a
/// field goes to the next, and Enter on the last field goes to the form's
/// add row, as the tree's `looks.new-frit.typing` says. The tree plans a
/// field for each role in place of these, so the count is not in the
/// tables yet.
pub const FORM_FIELDS: usize = 6;

/// The kind of font the highlight is on in the list, and the row the
/// new-frit form is on. The form itself is a layer, so it has no focus of
/// its own here.
fn focus(app: &App) -> String {
    if app.nav.view == View::New {
        return String::new();
    }
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
    match app.fonts.form().at {
        0 => kind.into(),
        row => format!("{kind}, form on row {row}"),
    }
}

/// The new-frit form's row: Enter steps from field to field, and the last
/// row adds the font set.
fn layer_focus(app: &App, narrow: &str) -> Option<String> {
    (narrow == "new-frit").then(|| format!("row {}", app.fonts.form().at))
}

/// The new-frit form shows its add row, highlighted when the form is on
/// it.
fn shown(s: &Shown) -> &'static [&'static str] {
    if s.rows()
        .iter()
        .any(|r| r.starts_with(" > Add this font set"))
    {
        &["looks.new-frit"]
    } else if s.rows().iter().any(|r| r.contains("Add this font set")) {
        &["looks.new-frit.typing"]
    } else {
        &["fonts"]
    }
}
