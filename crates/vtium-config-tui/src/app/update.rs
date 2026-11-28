//! The one update, [`App::event`]. A key goes to the navigation module,
//! which moves where its row says and answers with what follows: an action
//! for the open tab, or one for the shell's own handler. The open tab's
//! update returns commands; update applies their changes to the shared
//! state in one place, through the document's one change function, which
//! records each change as one step of the undo history; then it hands
//! their requests to the navigation module; then it returns their effects
//! to the loop.
//!
//! [`route`] says who acts on each row of the key table: the open tab, the
//! shell, or nobody yet. A later card builds a row of the shell's by
//! giving it a [`Route`] here and its handler in `App::shell`; the File
//! menu's item for that row then stops being greyed out.
//!
//! References:
//! - ADR-1206 D2: update hands a key to the navigation module, routes the
//!   action, and applies changes, then requests, then returns effects.
//! - ADR-1206 D3: the undo history lives in the shared state, and every
//!   change passes the one place that records it.
//! - ADR-1403 D11, "Undo and redo": Ctrl+Z and Ctrl+Y act on the document
//!   and do nothing while a value is typed, in a menu or in Save's review.
//! - ADR-1403 D11, "Quitting, Ctrl-C and the signals": `q`, File › Quit,
//!   Ctrl-C and the signals quit at once.

use vtium_config_core::ui::{Action, Change, Command, Effect, Request};

use super::{App, Event, menu_lines};
use crate::menus::{self, Line, Submenu};
use crate::nav::menus::Which;
use crate::nav::{Here, Step, pressed};
use crate::view;

/// Who acts on a row of the key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The open tab's update.
    Tab,
    /// The shell undoes the last change to the document.
    Undo,
    /// The shell redoes the change the last undo took back.
    Redo,
    /// The shell quits at once.
    Quit,
    /// The row is the shell's, and does nothing where it acts, as Ctrl+Z
    /// while a value is typed.
    Nothing,
    /// The row is the shell's, and the card that builds it has not landed
    /// yet: the message line says so, naming what the row opens.
    Unbuilt(&'static str),
}

/// Who acts on the row `id`.
pub fn route(id: &str) -> Route {
    match id {
        "ctrl-z.view" | "ctrl-z.layer" => Route::Undo,
        "ctrl-y.view" | "ctrl-y.layer" => Route::Redo,
        "q.view" | "q.layer" | "q.too-small" | "ctrl-c.any" | "ctrl-c.too-small" => Route::Quit,
        // the Flow view's letters for the File menu's items
        "n.view.flow" => Route::Unbuilt("New…"),
        "r.view.flow" => Route::Unbuilt("Revert to saved…"),
        "v.view.flow" => Route::Unbuilt("Show changes"),
        "o.view.flow" | "enter.layer.menu.history" => Route::Unbuilt("History"),
        "e.layer.changes" => Route::Unbuilt("Edit as text…"),
        "enter.layer.menu.open" => Route::Unbuilt("Opening a configuration"),
        "alt-p.view" => Route::Unbuilt("The Problems list"),
        "click-facts.view" => Route::Unbuilt("Opening the top row's counts"),
        "q.typing" => Route::Tab,
        _ if id.starts_with("ctrl-s.") => Route::Unbuilt("Save"),
        _ if id.starts_with("f1.") => Route::Unbuilt("Help on F1"),
        _ if id.starts_with("ctrl-f.") => Route::Unbuilt("Find"),
        _ if SHELL.iter().any(|p| id.starts_with(p)) || id.ends_with(".too-small") => {
            Route::Nothing
        }
        _ => Route::Tab,
    }
}

/// The rows that are the shell's, by the start of their ids: the keys
/// that are the same everywhere, and the top row's, the menus' and the
/// questions' keys and clicks.
const SHELL: &[&str] = &[
    "tab.",
    "shift-tab.",
    "ctrl-z.",
    "ctrl-y.",
    "ctrl-c.",
    "q.",
    "period.",
    "alt-",
    "f10.",
    "click-menu.",
    "click-tab.view",
    "click-button.",
    "click-item.",
    "click-outside.",
    "y.question",
    "n.question",
    "esc.question",
    "other.question",
];

/// Whether the shell acts on the row `id` today: the File menu greys out
/// an item whose row it does not.
pub fn handled(id: &str) -> bool {
    !matches!(route(id), Route::Unbuilt(_))
}

impl App {
    /// Acts on `event`, and returns what the loop is to do.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        if self.leaving.is_some() {
            return Vec::new();
        }
        match event {
            Event::Key(key) => {
                // the message says what the last key did, until the next
                self.shared.message = None;
                let step = self.navigate(|nav, here, _| nav.key(&key, here));
                self.follow(step)
            }
            Event::Mouse(mouse) => {
                let Some(at) = pressed(&mouse) else {
                    return Vec::new();
                };
                self.shared.message = None;
                let step = self.navigate(|nav, here, shown| {
                    nav.click(
                        at,
                        shown.frame.places(),
                        &shown.bar,
                        shown.menu.as_ref().map(|m| m.layout.places()),
                        here,
                    )
                });
                self.follow(step)
            }
            Event::Resize(size) => {
                self.size = size;
                Vec::new()
            }
            Event::Signal(n) => self.quit(u8::try_from(n.saturating_add(128)).unwrap_or(1)),
            // the live console's replies and waits, which rebuild-shell-live
            // reads
            Event::Reply(_) | Event::Timeout => Vec::new(),
        }
    }

    /// Lays the screen out as the view draws it, then hands the navigation
    /// module its state and what it reads.
    fn navigate(
        &mut self,
        act: impl FnOnce(&mut crate::nav::Nav, &Here<'_>, &view::Layout) -> Step,
    ) -> Step {
        let shown = view::layout(self);
        let App {
            nav,
            tabs,
            shared,
            size,
            ..
        } = self;
        let tab_node = nav.tab_node(tabs);
        let open = tabs.get(nav.tab());
        let menu = |which: Which| menu_lines(shared, open, tab_node, which, &shown.more);
        let submenu = |which: Submenu| -> Vec<Line> {
            menus::submenu(
                which,
                &menus::Context {
                    shared,
                    node: tab_node,
                },
            )
        };
        let here = Here {
            tabs,
            menu: &menu,
            submenu: &submenu,
            too_small: size.too_small(),
        };
        act(nav, &here, &shown)
    }

    /// Carries out what the navigation module answered.
    fn follow(&mut self, step: Step) -> Vec<Effect> {
        match step {
            Step::Nothing => Vec::new(),
            Step::Tab(action) => {
                let Some(tab) = self.tabs.get_mut(self.nav.tab()) else {
                    return Vec::new();
                };
                let commands = tab.update(&self.shared, action);
                self.apply(commands)
            }
            Step::Shell(action) => self.shell(action),
        }
    }

    /// The shell's own handler of an action.
    fn shell(&mut self, action: Action) -> Vec<Effect> {
        match route(action.row) {
            Route::Undo => self.apply(vec![Command::Change(Change::Undo)]),
            Route::Redo => self.apply(vec![Command::Change(Change::Redo)]),
            Route::Quit => self.quit(0),
            Route::Nothing => Vec::new(),
            Route::Unbuilt(what) => {
                self.shared.say(format!("{what} is not built yet."));
                Vec::new()
            }
            Route::Tab => self.follow(Step::Tab(action)),
        }
    }

    /// Applies `commands`: every change to the shared state first, in the
    /// one place, then every request to the navigation module; and returns
    /// the effects for the loop.
    fn apply(&mut self, commands: Vec<Command>) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut requests = Vec::new();
        for command in commands {
            match command {
                Command::Change(change) => self.change(change),
                Command::Request(request) => requests.push(request),
                Command::Effect(effect) => effects.push(effect),
            }
        }
        for request in requests {
            self.request(request);
        }
        effects
    }

    /// The one place a change reaches the shared state. A change to the
    /// document goes through its one change function, which writes the
    /// draft and records one step of the undo history; a refusal goes on
    /// the message line.
    fn change(&mut self, change: Change) {
        match change {
            Change::Document(change) => {
                let _ = self.shared.change(change);
            }
            Change::Undo => self.shared.undo(),
            Change::Redo => self.shared.redo(),
            Change::Say(message) => self.shared.message = Some(message),
            Change::Quiet => self.shared.message = None,
            Change::Try(_) | Change::TryLook(_) | Change::TrySize { .. } => {
                self.shared.say("Try live is not built yet.".to_owned());
            }
        }
    }

    /// A tab's request to the navigation module.
    fn request(&mut self, request: Request) {
        match request {
            Request::Open(node) => self.nav.open_level(node),
            Request::Close => self.nav.close_level(),
            Request::Ask { question, then } => self.nav.ask(question, then),
            Request::Show { tab, .. } => {
                if let Some(i) = self.tabs.iter().position(|t| t.node() == tab) {
                    self.nav.go_to(i, self.tabs.len());
                }
            }
        }
    }

    /// Leaves at once with `status`. The draft already holds every change,
    /// so nothing is asked.
    fn quit(&mut self, status: u8) -> Vec<Effect> {
        self.leaving = Some(status);
        vec![Effect::Exit(status)]
    }
}
