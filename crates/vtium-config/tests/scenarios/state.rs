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
use vtium_config::tui::model::{App, FontState, Then, View};
use vtium_config::tui::view::too_small;
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
        write!(f, "{} {}", self.term, self.view)?;
        if !self.focus.is_empty() {
            write!(f, "({})", self.focus)?;
        }
        write!(f, " {}", self.table_state())?;
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
        view: view_name(app.view),
        focus: String::new(),
        kind: StateKind::View,
        narrow: String::new(),
        unsaved: app.unsaved > 0,
        pending: !app.pending.is_empty(),
    };
    if sim.exited.is_some() {
        abs.kind = StateKind::Left;
        abs.view = "";
        abs.unsaved = false;
        abs.pending = false;
        return abs;
    }
    if too_small(app) {
        abs.kind = StateKind::TooSmall;
        abs.view = "";
        return abs;
    }
    let (kind, narrow) = open(app);
    abs.kind = kind;
    abs.narrow = narrow.to_owned();
    // the highlight matters only to the keys of a view with nothing open
    // over it, and a view's own name narrows it, as in `view:flow`
    if abs.kind == StateKind::View {
        abs.focus = focus(app);
        abs.narrow = abs.view.to_owned();
    }
    abs
}

/// What is open over the view, in the order `App::key` reads the keys.
fn open(app: &App) -> (StateKind, &'static str) {
    if app.trial.is_some() {
        return (StateKind::Trial, "");
    }
    if app.view == View::Flow && app.flow_view.typing.is_some() {
        return (StateKind::Typing, "flow-name");
    }
    match app.view {
        View::Settings if app.editing.is_some() => return (StateKind::Typing, "settings"),
        View::New if app.new_frit.at < vtium_config::tui::model::NEW_FIELDS => {
            return (StateKind::Typing, "new-frit");
        }
        _ => {}
    }
    if let Some(confirm) = &app.confirm {
        return (
            StateKind::Question,
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
    if app.view == View::New {
        return (StateKind::Layer, "new-frit");
    }
    if app.view != View::Flow {
        return (StateKind::View, "");
    }
    if app.pager.is_some() {
        return (StateKind::Layer, "file-view");
    }
    if app.restore.is_some() {
        return (StateKind::Layer, "restore");
    }
    if app.flow_view.picker.is_some() {
        return (StateKind::Layer, "flow-picker");
    }
    match &app.flow_view.layer {
        None => (StateKind::View, ""),
        Some(Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal)) => {
            (StateKind::Layer, "flow-card")
        }
        Some(Obj::Pal(Slot::Template(_)) | Obj::New(Slot::Empty(_))) => {
            (StateKind::Layer, "flow-maker")
        }
        Some(Obj::Others) => (StateKind::Layer, "flow-others"),
        Some(Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_)) => (StateKind::Layer, "flow-detail"),
        Some(_) => (StateKind::Layer, "flow-choice"),
    }
}

/// What the highlight is on, where the keys act on it differently: the
/// kind of font in the Fonts view, the kind of setting in Settings, the
/// kind of box in Flow, the page in Help, the add row of the new-frit form.
fn focus(app: &App) -> String {
    match app.view {
        View::Fonts => {
            let Some(font) = app.font() else {
                return "none".into();
            };
            if app.pending.removals.contains(&font.name) {
                return "marked".into();
            }
            match font.state {
                FontState::Reads => "frit".into(),
                FontState::BuiltIn => "builtin".into(),
                FontState::Refused(_) => "refused".into(),
            }
        }
        View::Settings => {
            let keys = vtium_config::tui::model::setting_keys();
            match keys.get(app.setting_at).map(|k| k.kind) {
                Some(Kind::Switch) => "switch".into(),
                Some(Kind::Choice(_)) => "choice".into(),
                Some(Kind::KeyBinding) => "binding".into(),
                Some(_) => "value".into(),
                None => "none".into(),
            }
        }
        View::Help => if app.about { "about" } else { "help" }.into(),
        View::Flow => {
            let obj = match &app.flow_view.at {
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
                Slot::Input(_) => "input",
                Slot::Terminal => "terminal",
                Slot::Font(_) => "font",
                Slot::Special(_) => "special",
                Slot::Template(_) => "template",
                Slot::Empty(_) => "empty",
            };
            format!("{place} {slot}")
        }
        View::Looks | View::New => String::new(),
    }
}
