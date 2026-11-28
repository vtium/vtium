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
//! A reply, the end of a wait and the first resize go to the live
//! console's module, [`crate::live`]. While a live trial counts down, it
//! takes every key and click before the navigation module does. Quitting
//! gives the console its start font back first, and from then on only the
//! replies due, the end of the wait and a signal still count.
//!
//! References:
//! - ADR-1206 D2: update hands a key to the navigation module, routes the
//!   action, and applies changes, then requests, then returns effects.
//! - ADR-1206 D3: the undo history lives in the shared state, and every
//!   change passes the one place that records it.
//! - ADR-1403 D11, "Undo and redo": Ctrl+Z and Ctrl+Y act on the document
//!   and do nothing while a value is typed, in a menu or in Save's review.
//! - ADR-1403 D11, "Quitting, Ctrl-C and the signals": `q`, File › Quit,
//!   Ctrl-C and the signals quit without asking.
//! - ADR-1201 D4, "Giving the console back": the start font goes back as
//!   the TUI leaves.

use vtium_config_core::ui::{Action, Change, Command, Effect, Op, Request};

use super::{App, Event, menu_lines};
use crate::live::{self, leave, trial, words};
use crate::menus::{self, Line, Submenu, actions};
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
    /// The shell quits, giving the console its start font back first.
    Quit,
    /// Actions › Read devices again: the loop reads the devices and the
    /// resources again.
    Devices,
    /// Actions › Install as a system font…: the loop readies the font file
    /// the open tab highlights, and the shell asks before it installs it.
    SystemFont,
    /// The yes to the question before a system font is installed: the
    /// loop runs the install on the terminal it hands back.
    InstallFont,
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
        actions::READ_DEVICES => Route::Devices,
        actions::SYSTEM_FONT => Route::SystemFont,
        live::INSTALL_FONT => Route::InstallFont,
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
    "question.",
];

/// Whether the shell acts on the row `id` today: the File menu greys out
/// an item whose row it does not.
pub fn handled(id: &str) -> bool {
    !matches!(route(id), Route::Unbuilt(_))
}

impl App {
    /// Acts on `event`, and returns what the loop is to do. The first
    /// resize, which the loop hands over before anything else, asks the
    /// terminal what it is. While the TUI leaves, only the replies due, the
    /// end of the wait and a signal still count.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        if let Some(status) = self.leaving {
            return match event {
                Event::Reply(reply) => live::reply(&mut self.shared, &mut self.live, reply),
                Event::Timeout => live::timeout(&mut self.shared, &mut self.live),
                Event::Signal(_) => vec![Effect::Exit(status)],
                _ => Vec::new(),
            };
        }
        match event {
            Event::Key(key) => {
                // the message says what the last key did, until the next
                self.shared.message = None;
                if self.live.trial.is_some() {
                    return self.trial_key(&key);
                }
                let step = self.navigate(|nav, here, _| nav.key(&key, here));
                self.follow(step)
            }
            Event::Mouse(mouse) => {
                let Some(at) = pressed(&mouse) else {
                    return Vec::new();
                };
                self.shared.message = None;
                // a click while the trial counts down puts the font back
                // and does nothing else
                if self.live.trial.is_some() {
                    return trial::put_back(&mut self.shared, &mut self.live);
                }
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
                live::start(&mut self.shared, &mut self.live)
            }
            Event::Signal(n) => self.quit(u8::try_from(n.saturating_add(128)).unwrap_or(1)),
            Event::Reply(reply) => live::reply(&mut self.shared, &mut self.live, reply),
            Event::Timeout => live::timeout(&mut self.shared, &mut self.live),
            Event::Devices(present) => {
                self.devices(present);
                Vec::new()
            }
            Event::FontReady(ready) => {
                self.font_ready(ready);
                Vec::new()
            }
            Event::FontInstalled(done) => {
                self.font_installed(done);
                Vec::new()
            }
        }
    }

    /// A key while the live trial counts down: `k` keeps the font, Ctrl+C
    /// quits, and any other key puts the font back and does nothing else.
    fn trial_key(&mut self, key: &vtium_config_widgets::keys::Key) -> Vec<Effect> {
        match trial::key(&mut self.shared, &mut self.live, key) {
            trial::Keyed::Kept => Vec::new(),
            trial::Keyed::PutBack(effects) => effects,
            trial::Keyed::Quit => self.quit(0),
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
            // the question before a system font is the shell's own
            Step::Tab(action) if route(action.row) == Route::InstallFont => self.shell(action),
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
            Route::Devices => vec![Effect::Desk(Op::Devices)],
            Route::SystemFont => {
                let own = self
                    .tabs
                    .get(self.nav.tab())
                    .and_then(|t| t.own_font(&self.shared));
                match own {
                    Some(path) => vec![Effect::Desk(Op::SystemFont(path))],
                    None => {
                        self.shared.say(format!(
                            "Install as a system font… acts {}.",
                            words::NOT_OWN
                        ));
                        Vec::new()
                    }
                }
            }
            Route::InstallFont => match self.live.font.as_ref() {
                Some(_) => vec![Effect::InstallFont],
                None => Vec::new(),
            },
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
                Command::Change(change) => effects.extend(self.change(change)),
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
    /// the message line. A live trial changes the console's state, never
    /// the document, and returns the switch for the loop to write.
    fn change(&mut self, change: Change) -> Vec<Effect> {
        match change {
            Change::Document(change) => {
                let _ = self.shared.change(change);
            }
            Change::Undo => self.shared.undo(),
            Change::Redo => self.shared.redo(),
            Change::Say(message) => self.shared.message = Some(message),
            Change::Quiet => self.shared.message = None,
            Change::Try(font) => return trial::try_font(&mut self.shared, &mut self.live, font),
            Change::TryLook(look) => {
                return trial::try_look(&mut self.shared, &mut self.live, &look);
            }
            Change::TrySize { .. } => self.shared.message = Some(words::size_not_built()),
        }
        Vec::new()
    }

    /// A tab's request to the navigation module.
    fn request(&mut self, request: Request) {
        match request {
            Request::Open(node) => self.nav.open_level(node),
            Request::Close => self.nav.close_level(),
            Request::Ask { question, then } => self.nav.ask(question, then),
            Request::Show { tab, name } => {
                if let Some(i) = self.tabs.iter().position(|t| t.node() == tab) {
                    self.nav.go_to(i, self.tabs.len());
                    if let Some(t) = self.tabs.get_mut(i) {
                        t.show(&self.shared, &name);
                    }
                }
            }
        }
    }

    /// Leaves with `status`. The draft already holds every change, so
    /// nothing is asked. Where a trial changed the console's font, the
    /// console gets its start font back first, and the TUI leaves once the
    /// console answers or the wait ends.
    fn quit(&mut self, status: u8) -> Vec<Effect> {
        self.leaving = Some(status);
        leave::leave(&mut self.shared, &mut self.live, status)
    }

    /// What Read devices again found: the devices connected now, which
    /// replace those the Flow tab placed, and the resources read again.
    /// The message line names what came and what went.
    ///
    /// References:
    /// - ADR-1403 D7: Read devices again reads the connected displays and
    ///   input devices again.
    /// - ADR-1205 D3: the resources are read again on Read devices again,
    ///   and on no key.
    fn devices(&mut self, present: Option<vtium_config_core::flowmodel::Present>) {
        let Some(present) = present else {
            self.shared.message = Some(words::devices_unread());
            return;
        };
        let name = |d: &vtium_settings::Identity| {
            d.connector
                .clone()
                .or_else(|| d.name.clone())
                .unwrap_or_else(|| "a device".to_owned())
        };
        let old = &self.shared.present.devices;
        let came: Vec<String> = present
            .devices
            .iter()
            .filter(|d| !old.contains(d))
            .map(name)
            .collect();
        let went: Vec<String> = old
            .iter()
            .filter(|d| !present.devices.contains(d))
            .map(name)
            .collect();
        self.shared.present = present;
        self.shared.found.again();
        self.shared.message = Some(words::devices(&came, &went));
    }

    /// The font file the loop readied as a system font: the question,
    /// which names the command first, or why it cannot be installed.
    ///
    /// References:
    /// - ADR-1205 D5: the question before a font of the person's own is
    ///   installed shows the exact command first.
    fn font_ready(&mut self, ready: Result<live::machine::Ready, String>) {
        use vtium_config_core::apply::Placement;
        let ready = match ready {
            Ok(ready) => ready,
            Err(why) => {
                self.shared.message = Some(words::system_font_failed(&why));
                return;
            }
        };
        let target = ready.target.display().to_string();
        if ready.placement == Placement::Identical {
            self.shared.message = Some(words::system_font_identical(&target));
            return;
        }
        let question = words::system_font_question(&ready.source.display().to_string());
        self.live.font = Some(ready);
        self.nav.ask(question, Action::of(live::INSTALL_FONT));
    }

    /// The install of a system font ended: on success the fonts are read
    /// again, so the font shows among the system fonts.
    fn font_installed(&mut self, done: Result<(), String>) {
        let Some(ready) = self.live.font.take() else {
            return;
        };
        self.shared.message = Some(match done {
            Ok(()) => {
                self.shared.found.again();
                words::system_font_installed(&ready.target.display().to_string())
            }
            Err(why) => words::system_font_failed(&why),
        });
    }
}
