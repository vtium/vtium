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

use vtium_config::tui::layout::too_small;
use vtium_config::tui::model::{App, Then, View};
use vtium_config::tui::nav;

use super::sim::Sim;
use super::tabs;

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

/// The name the abstract state gives `view`, as its tab's part of the walk
/// names it.
pub fn view_name(view: View) -> &'static str {
    tabs::TABS
        .iter()
        .flat_map(|t| t.views)
        .find(|(v, _)| *v == view)
        .map_or("", |(_, name)| *name)
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
    // in some layers, what the highlight is on decides what Enter opens,
    // as the tab's part of the walk says
    if let Some(focus) = tabs::TABS
        .iter()
        .find_map(|t| (t.layer_focus)(app, &abs.narrow))
    {
        abs.focus = focus;
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
                Then::InstallFont => "system font",
                Then::Desk(op) => match op {
                    vtium_config::tui::desk::Op::Revert => "revert",
                    vtium_config::tui::desk::Op::New => "new",
                    vtium_config::tui::desk::Op::Restore(_) => "restore",
                    _ => "desk",
                },
            },
        );
    }
    let narrow = tabs::TABS
        .iter()
        .flat_map(|t| t.narrow)
        .find(|(node, _)| *node == at)
        .map_or("", |(_, narrow)| *narrow);
    (kind, narrow)
}

/// What the highlight is on, where the keys act on it differently, as the
/// open tab's part of the walk says.
fn focus(app: &App) -> String {
    (tabs::of(app.nav.view).focus)(app)
}

/// What the screen shows, read from its text and its tab bar alone,
/// without the navigation module: the tab the tab bar highlights, and the
/// rows as text. The walks compare it with the node that the navigation
/// module names, so a tab whose `node` names the wrong node fails on the
/// screen as well as in the map.
pub struct Shown {
    /// The tab the tab bar highlights, by its id: the reversed text on
    /// row 0, as `fonts`.
    pub tab: String,
    rows: Vec<String>,
}

impl Shown {
    pub fn of(sim: &Sim) -> Shown {
        let screen = vtium_config::tui::view::screen(&sim.app);
        let tab = screen
            .first()
            .map(|row| {
                row.iter()
                    .filter(|s| s.style.reverse)
                    .map(|s| s.text.as_str())
                    .collect::<String>()
                    .trim()
                    .to_lowercase()
            })
            .unwrap_or_default();
        let rows = screen.iter().map(vtium_config::tui::view::text).collect();
        Shown { tab, rows }
    }

    /// The screen's rows as text.
    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    pub fn row(&self, at: usize) -> &str {
        self.rows.get(at).map_or("", String::as_str)
    }

    /// Whether a row starts with `text`.
    pub fn any(&self, text: &str) -> bool {
        self.rows.iter().any(|r| r.starts_with(text))
    }

    /// Whether a row under the tab bar's two lines, up to row 5, starts
    /// with `title`.
    pub fn top(&self, title: &str) -> bool {
        self.rows
            .iter()
            .take(6)
            .skip(1)
            .any(|r| r.starts_with(title))
    }

    /// The nodes the screen can be showing, as the part of the walk of the
    /// tab that the tab bar highlights reads them from the screen's rows.
    pub fn nodes(&self) -> &'static [&'static str] {
        tabs::TABS
            .iter()
            .find(|t| t.bar == self.tab)
            .map_or(&[], |t| (t.shown)(self))
    }

    /// Whether the screen can be showing the node `node`. A question, the
    /// live trial and the screen below the minimum size draw over a node
    /// and are not read here.
    pub fn agrees(&self, node: &str) -> Result<(), String> {
        let nodes = self.nodes();
        if node.starts_with("any.") || nodes.contains(&node) {
            Ok(())
        } else {
            Err(format!(
                "the screen shows {nodes:?} under the tab {:?}, and the TUI is in {node}",
                self.tab
            ))
        }
    }
}
