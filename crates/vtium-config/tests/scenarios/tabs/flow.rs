//! The Flow tab's part of the scenario walk.

use vtium_config::tui::flowview::{Obj, Slot};
use vtium_config::tui::model::{App, View};

use super::super::state::Shown;
use super::Tab;

pub const TAB: Tab = Tab {
    bar: "flow",
    views: &[(View::Flow, "flow")],
    narrow: &[
        ("flow.card.typing", "flow-name"),
        ("flow.detail.typing", "flow-label"),
        ("flow.maker.picker.typing", "flow-value"),
        ("flow.file", "file-view"),
        ("flow.restore", "restore"),
        ("flow.maker.picker", "picker"),
        ("flow.card", "flow-card"),
        ("flow.new-card", "flow-new-card"),
        ("flow.info", "flow-info"),
        ("flow.maker", "flow-maker"),
        ("flow.others", "flow-others"),
        ("flow.detail", "flow-detail"),
        ("flow.detail.choices", "flow-choice"),
    ],
    focus,
    layer_focus,
    shown,
    today: &[],
};

/// The kind of box the highlight is on, and where the box sits.
fn focus(app: &App) -> String {
    let obj = match &app.flow.state().at {
        None => return "none".into(),
        Some(obj) => obj,
    };
    let (place, slot) = match obj {
        Obj::Doc(_, s) => ("doc", s),
        Obj::New(s) => ("new", s),
        Obj::Pal(s) => ("parts", s),
        Obj::Roam(s) => ("active", s),
        Obj::Nothing(s) => ("nothing", s),
        Obj::Others => return "others".into(),
    };
    let slot = match slot {
        Slot::Display(_) => "display",
        // a keyboard and a pointer sit apart in PARTS, and the box
        // of other inputs comes after them; the slot holds the
        // selector that names the device
        Slot::Input(name) => {
            let sort = app
                .shared
                .inputs
                .flow
                .inputs
                .iter()
                .find(|d| d.name == *name || d.keys.iter().any(|k| k.starts_with(name.as_str())))
                .map(vtium_config::tui::flowview::sort);
            match sort {
                Some(vtium_config::tui::flowview::Sort::Keyboard) => "keyboard",
                Some(vtium_config::tui::flowview::Sort::Pointer) => "pointer",
                _ if name == "*" => "any input",
                _ => "input",
            }
        }
        Slot::Terminal => "terminal",
        Slot::Font(_) => "font",
        Slot::Special(_) => "special",
        Slot::Template(_) => "template",
        Slot::Empty(_) => "empty",
    };
    format!("{place} {slot}")
}

/// A part maker's line, and the picker's, decide what Enter opens: a part,
/// the picker, or the value typed after Other…
fn layer_focus(app: &App, narrow: &str) -> Option<String> {
    matches!(narrow, "flow-maker" | "picker").then(|| format!("line {}", app.flow.state().line))
}

/// A layer of the Flow view puts its name after a `›` on row 1 and its
/// title on row 3, and the file view and Restore's list show their titles
/// near the top. Some layers share a title, so one screen can name a few
/// nodes.
fn shown(s: &Shown) -> &'static [&'static str] {
    let title = s.row(3);
    if s.row(1).contains("  ›  ") {
        if title.starts_with(" OTHER INPUTS") {
            &["flow.others"]
        } else if title.starts_with(" THE NEW PATH'S TERMINAL") {
            &["flow.new-card", "flow.card.typing"]
        } else if title.starts_with(" TERMINAL ") {
            &["flow.card", "flow.card.typing"]
        } else if title.contains(" PART ") {
            &["flow.detail", "flow.detail.choices", "flow.detail.typing"]
        } else if title.starts_with(" ADD A") {
            &["flow.maker"]
        } else {
            &["flow.info", "flow.maker.picker", "flow.maker.picker.typing"]
        }
    } else if s.top(" The draft (read only)") {
        &["flow.file"]
    } else if s.top(" Restore an earlier vtium.conf") {
        &["flow.restore"]
    } else {
        &["flow"]
    }
}
