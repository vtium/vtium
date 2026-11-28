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
//! shell, or nobody yet. The shell's handlers of Show changes, Edit as
//! text, History and Revert to saved route to their modules,
//! [`crate::changes`], [`crate::editor`], [`crate::history`] and
//! [`crate::revert`]. A later card builds a row of the shell's by
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
use crate::save::{self, Press};
use crate::sheets::{self, Act, Sheet};
use crate::view;
use vtium_config_core::versions::merge::Side;
use vtium_config_widgets::keys::{Code, Key};

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
    /// F1: help for what is highlighted.
    About,
    /// Ctrl+F: Find, which closes a menu first.
    Find,
    /// Alt+P, or a click on the problems count: the Problems list.
    Problems,
    /// File › New….
    New,
    /// File › Open ›'s entry, or Manage configurations….
    Open,
    /// A yes to the Manage sheet's question: deletes the configuration it
    /// named.
    Delete,
    /// Save's keys: the review, the screen for a file changed on disk, and
    /// settling the lines both sides changed.
    Save(crate::save::Press),
    /// File › Show changes, `v` in the Flow tab, or a click on the count
    /// not saved; carrying a line of vtium.conf, the whole file at it.
    Changes,
    /// File › Edit as text…, `e` in Show changes, or a yes to edit again:
    /// the loop runs the person's editor.
    Edit,
    /// File › Revert to saved…, or `r` in the Flow tab: asks first.
    Revert,
    /// The yes to Revert's question.
    Reverted,
    /// `o` in the Flow tab: File open on History's submenu.
    HistoryMenu,
    /// An entry of File › History ›: the version's sheet.
    Version,
    /// View changes on the version's sheet.
    ViewVersion,
    /// Use this version on the version's sheet.
    UseVersion,
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
        "n.view.flow" => Route::New,
        crate::revert::OPEN => Route::Revert,
        crate::revert::YES => Route::Reverted,
        // the count not saved opens Show changes too
        crate::changes::OPEN | "click-facts.view" => Route::Changes,
        crate::history::MENU => Route::HistoryMenu,
        crate::history::PICK => Route::Version,
        crate::history::VIEW => Route::ViewVersion,
        crate::history::USE => Route::UseVersion,
        "e.layer.changes" | crate::editor::AGAIN => Route::Edit,
        "enter.layer.menu.open" => Route::Open,
        "alt-p.view" => Route::Problems,
        crate::sheets::manage::DELETE => Route::Delete,
        "q.typing" => Route::Tab,
        "ctrl-s.layer.save" => Route::Save(Press::Save),
        save::ANYWAY => Route::Save(Press::Anyway),
        "u.layer.save-changed" => Route::Save(Press::Update),
        "o.layer.save-changed" => Route::Save(Press::Overwrite),
        "r.layer.save-changed" => Route::Save(Press::Revert),
        save::REVERTED => Route::Save(Press::Reverted),
        "m.layer.save-settle" => Route::Save(Press::Settle(Side::Mine)),
        "i.layer.save-settle" => Route::Save(Press::Settle(Side::Installed)),
        _ if id.starts_with("ctrl-s.") => Route::Save(Press::Open),
        // F1 does nothing while a value is typed, in a menu, over its own
        // help, and on the Help tab, whose topics are in view
        "f1.typing" | "f1.layer.menu" | "f1.layer.about" | "f1.view.help" => Route::Nothing,
        _ if id.starts_with("f1.") => Route::About,
        "ctrl-f.typing" => Route::Nothing,
        _ if id.starts_with("ctrl-f.") => Route::Find,
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
    /// end of the wait and a signal still count. After each event the open
    /// tab gets the `?` search as it now stands, and its preview is asked
    /// for.
    pub fn event(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = self.handle(event);
        if self.leaving.is_none() {
            self.hand_search();
            effects.extend(self.preview());
        }
        effects
    }

    fn handle(&mut self, event: Event) -> Vec<Effect> {
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
                // the message says what the last key did, until the next,
                // and F1 opens the rest of a message that has more
                let more = self
                    .shared
                    .message
                    .as_ref()
                    .is_some_and(|m| m.more.is_some());
                if !(more && key == Key::plain(Code::F(1))) {
                    self.shared.message = None;
                }
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
            Event::Save(came) => save::came(came, &mut self.shared, &mut self.nav),
            Event::Said(said) => {
                self.shared.message = Some(said);
                Vec::new()
            }
            Event::Edited(done) => {
                self.edited(done);
                Vec::new()
            }
            Event::Checked { document, found } => {
                if let Ok((checked, shown)) = found
                    && &document == self.shared.document()
                {
                    self.shared.checked(checked, shown);
                }
                self.checked = Some(document);
                Vec::new()
            }
            Event::FontReads(reads) => {
                self.shared.read_fonts(reads);
                Vec::new()
            }
        }
    }

    /// Hands the open tab the `?` search after a key or a click changed it,
    /// and `None` once it closes. The search filters the tab it opened
    /// over, so going to another tab closes it and clears that tab's
    /// filter.
    ///
    /// References:
    /// - ADR-1206 D3, "One search component": the navigation state holds
    ///   the search, and each tab keeps the indexes that match.
    fn hand_search(&mut self) {
        let tab = self.nav.tab();
        if let Some((left, _)) = self.handed.take_if(|(t, _)| *t != tab) {
            self.nav.close_search();
            if let Some(t) = self.tabs.get_mut(left) {
                t.filtered(&self.shared, None);
            }
        }
        let now = self.nav.search();
        if now == self.handed.as_ref().map(|(_, s)| s) {
            return;
        }
        if let Some(t) = self.tabs.get_mut(tab) {
            t.filtered(&self.shared, now);
        }
        self.handed = now.map(|s| (tab, s.clone()));
    }

    /// Asks for the picture of what the open tab previews, where nothing
    /// covers the tab: the pictures ask the loop for the font helper's
    /// steps, and say on the message line why no picture shows, unless the
    /// live trial's countdown holds the line.
    ///
    /// References:
    /// - ADR-1201 D4: the preview of a font or an effect as a picture of
    ///   the console, which leaves the live console alone.
    /// - ADR-1206 D2: update hands a picture to the pictures, and the loop
    ///   alone asks the font helper.
    fn preview(&mut self) -> Vec<Effect> {
        let covered = !self.nav.sheets().is_empty()
            || self.nav.menu().is_some()
            || self.nav.question().is_some()
            || self.size.too_small();
        let Some(tab) = self.tabs.get(self.nav.tab()).filter(|_| !covered) else {
            return Vec::new();
        };
        let facts = self.facts();
        // the pictures change while the preview reads the rest of the
        // shared state
        let mut pictures = std::mem::take(&mut self.shared.pictures);
        let answer = tab.preview(&self.shared).and_then(|wanted| {
            let place = wanted.place?;
            Some(pictures.ask(&wanted.on(&self.shared, &facts), place))
        });
        self.shared.pictures = pictures;
        let Some(answer) = answer else {
            return Vec::new();
        };
        if let Some(say) = answer.say
            && self.live.trial.is_none()
        {
            self.shared.say(say);
        }
        answer.effects
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
            Step::Sheet(act) => self.sheet(act),
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
            Route::About => {
                self.about();
                Vec::new()
            }
            Route::Find => {
                self.find();
                Vec::new()
            }
            Route::Problems => {
                self.nav
                    .open_sheet(Sheet::Problems(sheets::Problems::of(&self.shared)));
                Vec::new()
            }
            Route::New => {
                self.new_sheet();
                Vec::new()
            }
            Route::Open => {
                self.open(action);
                Vec::new()
            }
            Route::Delete => {
                self.delete();
                Vec::new()
            }
            Route::Save(press) => save::press(press, &mut self.shared, &mut self.nav),
            Route::Changes => {
                self.show_changes(&action);
                Vec::new()
            }
            Route::Edit => {
                let document = self.shared.document();
                vec![Effect::Editor {
                    name: document.name().to_owned(),
                    text: document.text().to_owned(),
                }]
            }
            Route::Revert => {
                match crate::revert::ask(self.shared.document()) {
                    crate::revert::Ask::Question(q) => {
                        self.nav.ask(q, Action::of(crate::revert::YES));
                    }
                    crate::revert::Ask::Nothing(said) => self.shared.say(said.to_owned()),
                }
                Vec::new()
            }
            Route::Reverted => {
                let (change, said) = crate::revert::revert(self.shared.document());
                self.document_change(change, said);
                Vec::new()
            }
            Route::HistoryMenu => {
                self.history_menu();
                Vec::new()
            }
            Route::Version => {
                self.version(&action);
                Vec::new()
            }
            Route::ViewVersion => {
                if let Some(view) = crate::history::view::of_top(self.nav.sheets(), &self.shared) {
                    self.nav.open_sheet(Sheet::Changes(view));
                }
                Vec::new()
            }
            Route::UseVersion => {
                if let Some(Sheet::Version(v)) = self.nav.sheets().last() {
                    let (change, said) = crate::history::use_version(v.version());
                    self.nav.close_sheet();
                    self.document_change(change, said);
                }
                Vec::new()
            }
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
                self.refresh();
            }
            Change::Undo => {
                self.shared.undo();
                self.refresh();
            }
            Change::Redo => {
                self.shared.redo();
                self.refresh();
            }
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
                if let Some(i) = self.tab_at(tab) {
                    self.nav.go_to(i, self.tabs.len());
                    if let Some(t) = self.tabs.get_mut(i) {
                        t.show(&self.shared, &name);
                    }
                }
            }
        }
    }

    /// The tab whose own node is `tab`, such as `looks`, whatever level is
    /// open in it or whichever node it is on now, such as
    /// `looks-resources`.
    fn tab_at(&self, tab: &str) -> Option<usize> {
        self.tabs.iter().position(|t| tab_of(t.node()) == tab)
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

/// The columns of the viewer that F1's sheet keeps for its box and its
/// borders around the lines of a Help topic.
const ABOUT_MARGIN: u16 = 6;

/// The shell's handlers of the sheets' rows and of what the sheets ask
/// for; the sheets themselves are in [`crate::sheets`].
impl App {
    /// F1: the rest of the message where it has more, else the full text
    /// behind the highlighted line of the sheet on top or of the open tab,
    /// else every key of the screen the person is on: the Help tab's topic
    /// for the screen where a tab gives one, else the node's keys.
    fn about(&mut self) {
        let under = self.nav.under_menu(&self.tabs, self.size.too_small());
        let top = self.nav.sheets().last();
        let tab = self.tabs.get(self.nav.tab());
        let more = self.shared.message.as_ref().and_then(|m| m.more.clone());
        let about = if let Some(more) = more {
            sheets::About::text(sheets::about::MESSAGE, &more)
        } else if let Some(sheet) = top {
            match sheet.full_text() {
                Some((title, text)) => sheets::About::text(title, &text),
                None => sheets::About::keys(&sheet.title(), under),
            }
        } else {
            let name = tab.map_or("", |t| t.name());
            // the sheet's lines fit inside its box in the viewer
            let width = view::layout(self)
                .frame
                .regions()
                .viewer
                .width
                .saturating_sub(ABOUT_MARGIN);
            let topic = || self.tabs.iter().find_map(|t| t.keys_help(under, width));
            match tab.and_then(|t| t.full_text(&self.shared)) {
                Some(text) => sheets::About::text(name, &text),
                None => match topic() {
                    Some(lines) => sheets::About::topic(name, lines),
                    None => sheets::About::keys(name, under),
                },
            }
        };
        self.nav.open_sheet(Sheet::About(about));
    }

    /// Ctrl+F: opens Find over what is open, with every tab's items read
    /// once. Opening a sheet closes a menu first.
    fn find(&mut self) {
        let found = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.name(), t.find_items(&self.shared)))
            .collect();
        self.nav.open_sheet(Sheet::Find(sheets::Find::new(found)));
    }

    /// File › New…, offering the machine's hostname as the name.
    fn new_sheet(&mut self) {
        let doc = self.shared.document();
        let taken = self
            .shared
            .documents
            .desk()
            .configurations()
            .unwrap_or_default();
        let new = sheets::New::open(
            &vtium_config_core::store::hostname(),
            doc.name(),
            doc.count(),
            taken,
            doc.installed().text.is_some(),
        );
        self.nav.open_sheet(Sheet::New(new));
    }

    /// File › Open ›'s entry `action` carries, by its place among the
    /// configurations, or Manage configurations…, which carries none.
    fn open(&mut self, action: Action) {
        match action.carries {
            vtium_config_core::ui::Carries::Item(i) => {
                let names = self.shared.documents.desk().configurations();
                match names.map(|n| n.get(i).cloned()) {
                    Ok(Some(name)) => {
                        self.sheet(Act::Open(name));
                    }
                    Ok(None) => {}
                    Err(why) => self.shared.say(why),
                }
            }
            _ => {
                let states = sheets::open::states(&self.shared);
                let manage = sheets::Manage::new(states, self.shared.document().name());
                self.nav.open_sheet(Sheet::Manage(manage));
            }
        }
    }

    /// A yes to the Manage sheet's question: deletes the configuration it
    /// named, never the open one.
    fn delete(&mut self) {
        if let Some(Sheet::Manage(m)) = self.nav.sheet_mut() {
            m.delete(&mut self.shared);
        }
    }

    /// Carries out what a sheet asked for.
    fn sheet(&mut self, act: Act) -> Vec<Effect> {
        match act {
            Act::Pass | Act::Stay => Vec::new(),
            Act::Close => {
                self.nav.close_sheet();
                Vec::new()
            }
            Act::Go { tab, action } => {
                self.nav.go_clear(tab, self.tabs.len());
                match action {
                    Some(action) => self.follow(Step::Tab(action)),
                    None => Vec::new(),
                }
            }
            Act::GoNode { tab, action } => match self.tab_at(tab) {
                Some(tab) => self.sheet(Act::Go { tab, action }),
                None => Vec::new(),
            },
            Act::Ask { question, then } => {
                self.nav.ask(question, then);
                Vec::new()
            }
            Act::Shell(action) => self.shell(action),
            Act::Activate(n) => {
                if let Some(fonts) = self.tab_at("fonts") {
                    self.nav.go_clear(fonts, self.tabs.len());
                    self.nav.ask(
                        vtium_config_core::fontuse::activate::words::question(n),
                        Action::of("shift-a.view.fonts"),
                    );
                }
                Vec::new()
            }
            Act::Open(name) => {
                match sheets::open::open(&mut self.shared, &name) {
                    Ok(said) => {
                        let len = self.tabs.len();
                        let tab = self.nav.tab();
                        self.nav.go_clear(tab, len);
                        if let Some(said) = said {
                            self.shared.say(said);
                        }
                    }
                    Err(why) => self.shared.say(why),
                }
                Vec::new()
            }
            Act::Rename { from, to } => {
                if let Some(Sheet::Manage(m)) = self.nav.sheet_mut() {
                    m.rename(&mut self.shared, &from, &to);
                }
                Vec::new()
            }
            Act::New { name, start } => {
                let said = sheets::open::make(&mut self.shared, &name, start);
                let (tab, len) = (self.nav.tab(), self.tabs.len());
                self.nav.go_clear(tab, len);
                self.shared.say(said);
                Vec::new()
            }
        }
    }
}

/// The shell's handlers of Show changes, Edit as text, History and Revert
/// to saved; the screens and their words are in [`crate::changes`],
/// [`crate::editor`], [`crate::history`] and [`crate::revert`].
impl App {
    /// Reads every open Show changes again after a change to the document,
    /// so its diff follows each change, undo and redo.
    fn refresh(&mut self) {
        crate::changes::refresh(self.nav.sheets_mut(), &self.shared);
    }

    /// Makes `change` on the document, as one step, and says `said` once
    /// it is made; a refusal says why instead.
    fn document_change(&mut self, change: vtium_config_core::document::Change, said: String) {
        if self.shared.change(change).is_ok() {
            self.shared.say(said);
        }
        self.refresh();
    }

    /// Opens Show changes over what is open, or, for `action` carrying a
    /// line of vtium.conf, its whole file at that line.
    fn show_changes(&mut self, action: &Action) {
        let changes = crate::changes::Changes::of_document(&self.shared);
        let changes = match action.carries {
            vtium_config_core::ui::Carries::Item(line) => changes.at_line(line),
            _ => changes,
        };
        self.nav.open_sheet(Sheet::Changes(changes));
    }

    /// The editor exited: the text goes into the document as one change,
    /// and a failed check asks whether to edit again.
    fn edited(&mut self, done: Result<crate::editor::Edited, String>) {
        match crate::editor::back(self.shared.document(), done) {
            crate::editor::Back::Say(said) => self.shared.say(said),
            crate::editor::Back::Change {
                change,
                said,
                again,
            } => {
                self.document_change(change, said);
                if let Some(question) = again {
                    self.nav.ask_on(
                        crate::editor::QUESTION,
                        question,
                        Action::of(crate::editor::AGAIN),
                    );
                }
            }
        }
    }

    /// `o` in the Flow tab: File open on History's submenu.
    fn history_menu(&mut self) {
        let tab_node = self.nav.tab_node(&self.tabs);
        let cx = menus::Context {
            shared: &self.shared,
            node: tab_node,
        };
        let lines = menus::file::lines(&cx);
        let open = crate::history::menu(&lines, &|which| menus::submenu(which, &cx));
        self.nav.show_menu(open);
    }

    /// File › History ›'s entry that `action` carries, by its place among
    /// the versions: the version's sheet.
    fn version(&mut self, action: &Action) {
        let vtium_config_core::ui::Carries::Item(item) = action.carries else {
            return;
        };
        match crate::history::pick(&self.shared, item) {
            Ok(Some(version)) => self
                .nav
                .open_sheet(Sheet::Version(crate::history::Sheet::new(version))),
            Ok(None) => {}
            Err(why) => self.shared.say(why),
        }
    }
}

/// The tab's own node that `node` belongs to: `node` itself where it is a
/// tab's, else the first of its parents in the interface tree that is.
fn tab_of(node: &'static str) -> &'static str {
    let mut at = node;
    // the tree is a few levels deep, and the bound keeps a cycle finite
    for _ in 0..TREE_DEPTH {
        match vtium_config_widgets::contract::node(at) {
            Some(n) if !matches!(n.parent, "tui" | "-" | "") => at = n.parent,
            _ => break,
        }
    }
    at
}

/// More levels than the interface tree has under a tab.
const TREE_DEPTH: usize = 8;
