//! The abstraction: an [`App`] reduced to a short abstract state, which
//! names the kind of terminal, the view, what is highlighted in it, what
//! is open over it, and whether changes wait to be saved or installed. Two
//! apps with the same abstract state answer the keys of the alphabet the
//! same way, as far as the map is concerned, so the explorer keeps one of
//! them and tries every key there.
//!
//! The kind of state is the one ADR-1403's key table uses: `view`,
//! `layer`, `typing`, `question`, `trial` and `too-small`, each narrowed
//! by a name after a colon.

use vtium_config::tui::flowview::{Obj, Slot};
use vtium_config::tui::layout::too_small;
use vtium_config::tui::model::{App, FontState, Then, View};
use vtium_config::tui::nav;
use vtium_settings::keys::Kind;

use super::sim::Sim;

/// The kind of state, as the key table names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StateKind {
    View,
    Layer,
    Typing,
    Question,
    Trial,
    TooSmall,
    /// The TUI has exited.
    Left,
}

impl StateKind {
    pub fn name(&self) -> &'static str {
        match self {
            StateKind::View => "view",
            StateKind::Layer => "layer",
            StateKind::Typing => "typing",
            StateKind::Question => "question",
            StateKind::Trial => "trial",
            StateKind::TooSmall => "too-small",
            StateKind::Left => "left",
        }
    }
}

/// An abstract state.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Abs {
    pub term: &'static str,
    /// The view in front, by the name the key table uses.
    pub view: &'static str,
    /// What the highlight is on, where it matters to the keys.
    pub focus: String,
    pub kind: StateKind,
    /// The name that narrows the kind: the layer, the field typed into,
    /// the question.
    pub narrow: String,
    /// Changes wait for Save.
    pub unsaved: bool,
    /// New or removed font sets wait for an install.
    pub pending: bool,
    /// The node of the interface tree the TUI is in, as the navigation
    /// module names it; empty once the TUI has left.
    pub at: &'static str,
}

impl Abs {
    /// The node of ADR-1403's interface tree, `ADR-1403-tree.tsv`, that
    /// this state is: `flow.card`, `any.question.quit`, `settings`. The
    /// walk and the tree meet here: every state the explorer reaches names
    /// a node, and the test fails on one that does not.
    pub fn node(&self) -> String {
        if self.kind == StateKind::Left {
            "left".to_owned()
        } else {
            self.at.to_owned()
        }
    }
}

impl Abs {
    /// The state as the key table writes it: `view`, `view:flow`,
    /// `question:leave`.
    pub fn table_state(&self) -> String {
        if self.narrow.is_empty() {
            self.kind.name().to_owned()
        } else {
            format!("{}:{}", self.kind.name(), self.narrow)
        }
    }
}

impl std::fmt::Display for Abs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind == StateKind::Left {
            return write!(f, "{} left", self.term);
        }
        write!(f, "{} {}", self.term, self.node())?;
        if !self.focus.is_empty() {
            write!(f, "({})", self.focus)?;
        }
        // below the minimum size, a question asked there or the trial
        if self.view.is_empty() && self.kind != StateKind::TooSmall {
            f.write_str(" below 80×25")?;
        }
        if self.unsaved {
            f.write_str(" +unsaved")?;
        }
        if self.pending {
            f.write_str(" +pending")?;
        }
        Ok(())
    }
}

pub fn view_name(view: View) -> &'static str {
    match view {
        View::Flow => "flow",
        View::Looks => "looks",
        View::Fonts => "fonts",
        View::Settings => "settings",
        View::Help => "help",
        View::New => "new-frit",
    }
}

/// The abstract state of `sim`.
pub fn of(sim: &Sim) -> Abs {
    let app = &sim.app;
    let term = sim.term.name();
    let mut abs = Abs {
        term,
        view: view_name(app.nav.view),
        focus: String::new(),
        kind: StateKind::View,
        narrow: String::new(),
        unsaved: app.shared.unsaved > 0,
        pending: !app.shared.pending.is_empty(),
        at: nav::node(app),
    };
    if sim.exited.is_some() {
        abs.kind = StateKind::Left;
        abs.view = "";
        abs.unsaved = false;
        abs.pending = false;
        abs.at = "";
        return abs;
    }
    // below the minimum size no view shows, and only a question asked
    // there or the live trial's countdown changes what the keys do
    if too_small(app.size) {
        abs.view = "";
    }
    let (kind, narrow) = open(app, abs.at);
    abs.kind = kind;
    if abs.kind == StateKind::TooSmall {
        return abs;
    }
    abs.narrow = narrow.to_owned();
    // the highlight matters only to the keys of a view with nothing open
    // over it, and a view's own name narrows it, as in `view:flow`
    // a part maker's line, and the picker's, decide what Enter opens: a
    // part, the picker, or the value typed after Other…
    if matches!(abs.narrow.as_str(), "flow-maker" | "picker") {
        abs.focus = format!("line {}", app.flow.state().line);
    }
    // the new-frit form's row: Enter steps from field to field, and the
    // last row adds the font set
    if abs.narrow == "new-frit" {
        abs.focus = format!("row {}", app.fonts.form().at);
    }
    // a Settings key's kind decides what Enter opens on it: its typing,
    // or its values as a list
    if abs.narrow == "settings-section" {
        abs.focus = key_kind(app.settings.key());
    }
    if abs.kind == StateKind::View {
        abs.focus = focus(app);
        abs.narrow = abs.view.to_owned();
    }
    abs
}

/// The kind of state of the node `at`, which the navigation module gives,
/// and the name that narrows it, as the key table writes it: `layer` and
/// `file-view` for `flow.file`. A question's name says what its yes does,
/// a finer split than the tree's three questions, so that the walk tries
/// the keys at each of them.
fn open(app: &App, at: &str) -> (StateKind, &'static str) {
    let kind = match nav::kind(at) {
        nav::Kind::View => StateKind::View,
        nav::Kind::Layer => StateKind::Layer,
        nav::Kind::Typing => StateKind::Typing,
        nav::Kind::Question => StateKind::Question,
        nav::Kind::Trial => StateKind::Trial,
        nav::Kind::TooSmall => StateKind::TooSmall,
    };
    if let Some(confirm) = &app.nav.confirm {
        return (
            kind,
            match &confirm.then {
                Then::Quit(_) => "quit",
                Then::Leave(_) => "leave",
                Then::Change(_) => "change",
                Then::Install(_) => "install",
                Then::Desk(op) => match op {
                    vtium_config::tui::desk::Op::Revert => "revert",
                    vtium_config::tui::desk::Op::New => "new",
                    vtium_config::tui::desk::Op::Restore(_) => "restore",
                    _ => "desk",
                },
            },
        );
    }
    let narrow = NARROW
        .iter()
        .find(|(node, _)| *node == at)
        .map_or("", |(_, narrow)| *narrow);
    (kind, narrow)
}

/// The name the key table narrows each layer and typed value with, by the
/// node of the tree it is.
const NARROW: &[(&str, &str)] = &[
    ("flow.card.typing", "flow-name"),
    ("flow.detail.typing", "flow-label"),
    ("flow.maker.picker.typing", "flow-value"),
    ("settings.section.typing", "settings"),
    ("looks.new-frit.typing", "new-frit"),
    ("looks.new-frit", "new-frit"),
    ("settings.section", "settings-section"),
    ("settings.section.list", "settings-list"),
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
];

/// What the highlight is on, where the keys act on it differently: the
/// kind of font in the Fonts view, the kind of setting in Settings, the
/// kind of box in Flow, the page in Help, the add row of the new-frit form.
fn focus(app: &App) -> String {
    match app.nav.view {
        View::Fonts => {
            let Some(font) = app.font() else {
                return "none".into();
            };
            if app.shared.pending.removals.contains(&font.name) {
                return "marked".into();
            }
            match font.state {
                FontState::Reads => "frit".into(),
                FontState::BuiltIn => "builtin".into(),
                FontState::Refused(_) => "refused".into(),
            }
        }
        View::Help => if app.help.about() { "about" } else { "help" }.into(),
        View::Flow => {
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
                        .find(|d| {
                            d.name == *name || d.keys.iter().any(|k| k.starts_with(name.as_str()))
                        })
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
        // on the list of sections, the first section apart from the
        // rest, so the walk opens a section other than the first
        View::Settings if app.settings.section_at() == 0 => "first".into(),
        View::Settings => "later".into(),
        View::Looks | View::New => String::new(),
    }
}

/// The kind of a Settings key, as far as Enter on it differs: a switch or
/// a choice opens its values as a list, and a key binding or any other
/// key starts typing.
fn key_kind(key: Option<&vtium_settings::keys::Key>) -> String {
    match key.map(|k| k.kind) {
        Some(Kind::Switch) => "switch",
        Some(Kind::Choice(_)) => "choice",
        Some(Kind::KeyBinding) => "binding",
        Some(_) => "value",
        None => "none",
    }
    .into()
}
