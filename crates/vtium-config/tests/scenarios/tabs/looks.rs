//! The Looks tab's part of the scenario walk: the tab's list of looks,
//! font sets, color schemes and pointer themes, a look's card with its
//! shared list and its typed name, the detail of a font set, a color
//! scheme or a pointer theme, and the form for a new font set, which opens
//! over the tab.

use vtium_config::tui::model::{App, FontState, View};
use vtium_config::tui::tabs::looks::card::Line;
use vtium_config::tui::tabs::looks::palettes::Scheme;
use vtium_config::tui::tabs::looks::{Entry, Looks};

use super::super::state::Shown;
use super::Tab;

pub const TAB: Tab = Tab {
    bar: "looks",
    views: &[(View::Looks, "looks"), (View::New, "new-frit")],
    narrow: &[
        ("looks.look", "look"),
        ("looks.look.list", "list"),
        ("looks.font-set", "detail"),
        ("looks.palette", "detail"),
        ("looks.pointer", "detail"),
        ("looks.new-frit.typing", "new-frit"),
        ("looks.new-frit.list", "new-frit"),
        ("looks.new-frit", "new-frit"),
    ],
    focus,
    layer_focus,
    shown,
    today: TODAY,
};

/// What the moving keys do on the rows of the form for a new font set,
/// which the key table's rows say only in words: each moves the highlight
/// between the rows and stays in the form.
const TODAY: &[(&str, &str, &str)] = &[
    ("looks.new-frit", "Up", "looks.new-frit"),
    ("looks.new-frit", "Down", "looks.new-frit"),
    ("looks.new-frit", "Home", "looks.new-frit"),
    ("looks.new-frit", "End", "looks.new-frit"),
    ("looks.new-frit", "PgUp", "looks.new-frit"),
    ("looks.new-frit", "PgDn", "looks.new-frit"),
];

/// What the highlight is on in the list decides what Enter, `n`, `r`,
/// `t` and Space do: a look, the row that says there is none, a font set
/// of each kind, a built-in color scheme, or a scheme file or a pointer
/// theme by its marks, which decide whether Space activates it,
/// deactivates it, asks first, or only says why it cannot. The row the form for a new font set is on decides where `n`
/// opens it, so a form left on another row than the first is a state of
/// its own.
fn focus(app: &App) -> String {
    if app.nav.view == View::New {
        return String::new();
    }
    let shared = &app.shared;
    let kind = match app.looks.highlighted(shared) {
        None => "none".to_owned(),
        Some(Entry::Look(_)) => "look".to_owned(),
        Some(Entry::NoLooks) => "no looks".to_owned(),
        Some(Entry::FontSet(at)) => match shared.inputs.fonts.get(at) {
            None => "none".to_owned(),
            Some(font) if shared.pending.removals.contains(&font.name) => "marked".to_owned(),
            Some(font) => match font.state {
                FontState::Reads => "frit",
                FontState::BuiltIn => "builtin",
                FontState::Refused(_) => "refused",
            }
            .to_owned(),
        },
        Some(Entry::Palette(Scheme::BuiltIn(_))) => "built-in scheme".to_owned(),
        Some(e @ (Entry::Palette(_) | Entry::Pointer(_))) => match Looks::resource(shared, &e) {
            None => "none".to_owned(),
            Some((r, uses)) => {
                let used = if uses.is_empty() { "" } else { ", used" };
                format!("{}: {}{used}", r.kind, r.row())
            }
        },
    };
    match app.looks.form().at {
        0 => kind,
        row => format!("{kind}, form on row {row}"),
    }
}

/// The line of a look's card decides what Enter opens and whether Left
/// and Right step it; the row of the form for a new font set decides what
/// Enter does there.
fn layer_focus(app: &App, narrow: &str) -> Option<String> {
    match narrow {
        "look" => app.looks.card().map(|c| line_name(c.line()).to_owned()),
        "new-frit" => Some(format!("row {}", app.looks.form().at)),
        _ => None,
    }
}

/// The name the walk gives each line of a look's card.
pub fn line_name(line: Line) -> &'static str {
    match line {
        Line::Name => "name",
        Line::FontSet => "font set",
        Line::Size => "size",
        Line::Palette => "color scheme",
        Line::Vtlook => "vtlook",
        Line::Pointer => "pointer theme",
    }
}

/// The form for a new font set shows its list's hint while a role's list
/// is open, its typing hint while a value is typed, and its add row on its
/// rows. A list of choices marks each choice with a circle, a card starts
/// with its name line, a font set's detail starts with its title, and the
/// tab's list is the rest.
fn shown(s: &Shown) -> &'static [&'static str] {
    if s.any(" Enter picks, and Esc closes the list.") {
        &["looks.new-frit.list"]
    } else if s.any(" Type a font by its family") {
        &["looks.new-frit.typing"]
    } else if s.rows().iter().any(|r| r.contains("Add this font set")) {
        &["looks.new-frit"]
    } else if s.any("   ● ") || s.any("   ○ ") {
        &["looks.look.list"]
    } else if s.any(" Type the look's name") {
        &["looks.look.typing"]
    } else if s.any("   Name ") || s.any(" The draft no longer has the look") {
        &["looks.look"]
    } else if s.top(" Font set ") {
        &["looks.font-set"]
    } else if s.top(" Color scheme ") {
        &["looks.palette"]
    } else if s.top(" Pointer theme ") {
        &["looks.pointer"]
    } else {
        // the list is one node on a look or a font set and another on a
        // color scheme or a pointer theme, and both show the same list
        &["looks", "looks-resources"]
    }
}
