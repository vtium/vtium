//! The one navigation module: where the person is, and what each key and
//! click does there. The state holds the open tab, each tab's stack of
//! the levels open in it, the question over everything, the menu that is
//! open with its highlighted item and submenu, the search row, and the
//! sheets open over the tab ([`crate::sheets`]), the top last.
//!
//! For the node that is open, [`Nav::key`] looks up the key's row in the
//! compiled key table. It handles itself the rows that are the same
//! everywhere, Tab, Shift-Tab, the digits, the Alt letters, F10, `.`, Esc
//! on a level, a question's answers and a menu's keys, and answers with a
//! [`Step`] for the rest: an action for the open tab, or one for the
//! shell's own handler in [`crate::app::update`]. It never reads or
//! changes a tab's state: it asks a tab only for its name, its letter, its
//! node and its search.
//!
//! The `?` search over the open tab's list is the navigation state's. It
//! opens with the highlight on the item the list highlights, takes every
//! key of the search row, and goes back to that item when `?` resumes a
//! kept filter. The App hands the tab the search after each key.
//!
//! References:
//! - ADR-1206 D3: one navigation module owns where the person is, the
//!   menus and the search row's state, and reads the compiled tables.
//! - ADR-1206 D4: the navigation module calls the tables and each tab's
//!   queries, and never a tab's state.
//! - ADR-1403 D11, "Alt letters", "The menus", "The mouse": the keys and
//!   clicks of the top row, the menus and the action bar.

pub mod levels;
pub mod menus;

use ratatui_core::layout::Position;
use vtium_config_core::ui::Action;
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::{Bar, Hit, MenuName, Places};
use vtium_config_widgets::input;
use vtium_config_widgets::keys::{Code, Key, Mods, Mouse};
use vtium_config_widgets::menu::{MenuHit, MenuPlaces, Pick};
use vtium_config_widgets::search::{Outcome, Search};

use crate::BoxTab;
use crate::menus::{Line, Submenu, Then};
use crate::sheets::{Act, Sheet};
use levels::Kind;
use menus::{Done, Open, Which};

/// A question over everything, such as "Delete the look big?": its words,
/// its node of the interface tree, and the action a yes hands the tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub text: String,
    pub node: &'static str,
    pub then: Action,
}

/// What follows a key or a click once the navigation module has moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing more.
    Nothing,
    /// The open tab's update takes the action.
    Tab(Action),
    /// The shell's own handler takes the action, such as Undo or Quit.
    Shell(Action),
    /// The shell carries out what a sheet asked for.
    Sheet(Act),
}

/// What the navigation module reads besides its own state: the tabs, for
/// their names, letters, nodes and searches, the lines of each menu, and
/// whether the screen is below the minimum size.
pub struct Here<'a> {
    pub tabs: &'a [BoxTab],
    /// The lines of a menu, which the menus build from the shared state.
    pub menu: &'a dyn Fn(Which) -> Vec<Line>,
    /// The lines of a submenu.
    pub submenu: &'a dyn Fn(Submenu) -> Vec<Line>,
    pub too_small: bool,
}

/// Where the person is.
#[derive(Clone, Debug, Default)]
pub struct Nav {
    tab: usize,
    /// For each tab, the levels the tab asked to open over it, the top
    /// last. Each tab keeps its own, so going back to a tab finds the
    /// place the person left.
    levels: Vec<Vec<&'static str>>,
    question: Option<Question>,
    menu: Option<Open>,
    search: Option<Search>,
    /// The sheets open over the tab, the top last: F1's help over the
    /// Problems list, say, or Find over a sheet.
    sheets: Vec<Sheet>,
}

impl Nav {
    /// The navigation state of `tabs` tabs, on the first with nothing
    /// open.
    pub fn new(tabs: usize) -> Nav {
        Nav {
            levels: vec![Vec::new(); tabs],
            ..Nav::default()
        }
    }

    /// The open tab's index.
    pub fn tab(&self) -> usize {
        self.tab
    }

    /// The levels open over the tab `tab`, the top last.
    pub fn levels(&self, tab: usize) -> &[&'static str] {
        self.levels.get(tab).map_or(&[], Vec::as_slice)
    }

    /// The question over everything, where one is open.
    pub fn question(&self) -> Option<&Question> {
        self.question.as_ref()
    }

    /// The open menu, where one is open.
    pub fn menu(&self) -> Option<&Open> {
        self.menu.as_ref()
    }

    /// The search row, where one is open.
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// The sheets open over the tab, the top last.
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// The sheet on top, to change.
    pub fn sheet_mut(&mut self) -> Option<&mut Sheet> {
        self.sheets.last_mut()
    }

    /// Every open sheet, to change, as Show changes reads its lines again
    /// after a change to the document.
    pub fn sheets_mut(&mut self) -> &mut [Sheet] {
        &mut self.sheets
    }

    /// Opens `sheet` over everything open, closing a menu, as a menu
    /// closes when the person turns to something else.
    pub fn open_sheet(&mut self, sheet: Sheet) {
        self.menu = None;
        self.sheets.push(sheet);
    }

    /// Closes the sheet on top.
    pub fn close_sheet(&mut self) {
        self.sheets.pop();
    }

    /// Closes every sheet, the search row and the levels open over the tab
    /// at `index`, and goes to that tab, as Find's Enter does.
    pub fn go_clear(&mut self, index: usize, tabs: usize) {
        self.sheets.clear();
        self.search = None;
        self.menu = None;
        if let Some(levels) = self.levels.get_mut(index) {
            levels.clear();
        }
        self.go_to(index, tabs);
    }

    /// The node of the interface tree the person is on: the question, the
    /// screen below the minimum size, the open menu or submenu, the search
    /// row, the level open at the top of the tab, or the tab's own node.
    pub fn node(&self, tabs: &[BoxTab], too_small: bool) -> &'static str {
        if self.question.is_none()
            && !too_small
            && let Some(menu) = &self.menu
        {
            return match menu.sub {
                Some(sub) => sub.which.node(),
                None => menu.which.node(),
            };
        }
        self.under_menu(tabs, too_small)
    }

    /// The node under the open menu: the node the person is on once the
    /// menu closes. More's menu lists what this node's action bar has no
    /// room for.
    pub fn under_menu(&self, tabs: &[BoxTab], too_small: bool) -> &'static str {
        if let Some(q) = &self.question {
            return q.node;
        }
        if too_small {
            return "any.too-small";
        }
        if let Some(sheet) = self.sheets.last() {
            return sheet.node();
        }
        if let Some(search) = &self.search {
            return if search.typing() {
                "any.search"
            } else {
                "any.filter"
            };
        }
        self.tab_node(tabs)
    }

    /// The node of the open tab with no menu, question or search over it:
    /// the level at the top of its stack, or the tab's own node.
    pub fn tab_node(&self, tabs: &[BoxTab]) -> &'static str {
        if let Some(top) = self.levels(self.tab).last() {
            return top;
        }
        tabs.get(self.tab).map_or("tui", |t| t.node())
    }

    /// Acts on `key` where the person is, and says what follows.
    pub fn key(&mut self, key: &Key, here: &Here<'_>) -> Step {
        let node = self.node(here.tabs, here.too_small);
        let kind = levels::kind(node);
        if kind == Kind::Menu {
            return self.menu_key(key, node, here);
        }
        if let Some(step) = self.sheet_key(key, node) {
            return step;
        }
        if matches!(node, "any.search" | "any.filter")
            && let Some(step) = self.search_key(key, node, here)
        {
            return step;
        }
        // the top row's Alt letters act while nothing is open over a tab
        if kind == Kind::View
            && let Some(letter) = key.alt_letter()
        {
            return self.alt(letter, node, here);
        }
        let row = levels::row_for(node, key).or_else(|| {
            (node == "any.filter").then(|| levels::row_for(self.tab_node(here.tabs), key))?
        });
        let Some(row) = row else {
            return Step::Nothing;
        };
        self.row(row.id, key, here)
    }

    /// Acts on the row `id`, which a key or a button chose.
    fn row(&mut self, id: &'static str, key: &Key, here: &Here<'_>) -> Step {
        match id {
            "tab.view" => self.step_tab(true, here),
            "shift-tab.view" => self.step_tab(false, here),
            "1.view" | "2.view" | "3.view" | "4.view" | "5.view" | "6.view" => {
                if let Some(i) = id
                    .split('.')
                    .next()
                    .and_then(|d| d.parse::<usize>().ok())
                    .and_then(|d| d.checked_sub(1))
                    .filter(|&i| i < here.tabs.len())
                {
                    self.tab = i;
                }
                Step::Nothing
            }
            "alt-f.view" | "f10.view" => self.open_menu(Which::File, here),
            "alt-a.view" => self.open_menu(Which::Actions, here),
            "period.view" | "period.layer" => self.open_menu(Which::More, here),
            // a yes goes where its action's row goes: the tab's question
            // to the tab, a sheet's to the shell
            "y.question" => match self.question.take() {
                Some(q) => self.row_of_item(q.then),
                None => Step::Nothing,
            },
            "n.question" | "esc.question" | "ctrl-c.question" => {
                self.question = None;
                Step::Nothing
            }
            // a list box's `?` is its own search, which the tab holds
            "question.layer.list" => Step::Tab(carrying(id, key)),
            // `?` opens the search row wherever the node's `search`
            // column names fields, and does nothing elsewhere
            _ if id.starts_with("question.") => {
                self.open_search(here);
                Step::Nothing
            }
            _ if id.starts_with("esc.layer") => {
                if let Some(levels) = self.levels.get_mut(self.tab) {
                    levels.pop();
                }
                Step::Tab(Action::of(id))
            }
            _ => match crate::app::update::route(id) {
                crate::app::update::Route::Tab => Step::Tab(carrying(id, key)),
                _ => Step::Shell(Action::of(id)),
            },
        }
    }

    /// An Alt letter on a tab: File, Actions, the Problems list, or the tab
    /// whose letter it is, regardless of case.
    fn alt(&mut self, letter: char, node: &'static str, here: &Here<'_>) -> Step {
        let listed = |id| levels::listed(node, id).is_some();
        match letter {
            'f' if listed("alt-f.view") => self.open_menu(Which::File, here),
            'a' if listed("alt-a.view") => self.open_menu(Which::Actions, here),
            'p' if listed("alt-p.view") => Step::Shell(Action::of("alt-p.view")),
            _ => {
                let to = here.tabs.iter().position(|t| {
                    t.letter().to_lowercase().eq(std::iter::once(letter)) && listed("alt-tab.view")
                });
                if let Some(i) = to {
                    self.tab = i;
                }
                Step::Nothing
            }
        }
    }

    fn step_tab(&mut self, next: bool, here: &Here<'_>) -> Step {
        let n = here.tabs.len();
        if n > 0 {
            self.tab = if next {
                (self.tab + 1) % n
            } else {
                (self.tab + n - 1) % n
            };
        }
        Step::Nothing
    }

    /// Opens the menu `which`, unless it has no item at all, as More's
    /// menu has none where every button fits.
    fn open_menu(&mut self, which: Which, here: &Here<'_>) -> Step {
        let lines = (here.menu)(which);
        if !lines.is_empty() {
            self.menu = Some(Open::new(which, &lines));
        }
        Step::Nothing
    }

    /// A key while a menu is open.
    fn menu_key(&mut self, key: &Key, node: &'static str, here: &Here<'_>) -> Step {
        let Some(mut open) = self.menu else {
            return Step::Nothing;
        };
        let lines = (here.menu)(open.which);
        let sub_lines = open
            .sub
            .map(|s| (here.submenu)(s.which))
            .unwrap_or_default();
        let Some(row) = levels::row_for(node, key) else {
            return Step::Nothing;
        };
        let done = match row.id {
            "updown.layer.menu" => {
                open.move_by(key.code == Code::Down, &lines, &sub_lines);
                Done::Stay
            }
            "leftright.layer.menu" if key.code == Code::Right => open.right(&lines, here.submenu),
            "leftright.layer.menu" => open.left(),
            "esc.layer.menu" => open.escape(),
            "f10.layer.menu" => Done::Closed,
            id if id.starts_with("enter.layer.menu") => open.enter(&lines, here.submenu),
            id if id.starts_with("letters.layer.menu") => match key.typed() {
                Some(c) if open.sub.is_none() => open.letter(c, &lines, here.submenu),
                _ => Done::Stay,
            },
            // the keys that quit, and the rows that do nothing in a menu,
            // Ctrl+Z and Ctrl+Y among them
            id => {
                self.menu = Some(open);
                return match crate::app::update::route(id) {
                    crate::app::update::Route::Tab => Step::Nothing,
                    _ => Step::Shell(Action::of(id)),
                };
            }
        };
        self.after_menu(open, done, here)
    }

    /// What follows a menu's key or click.
    fn after_menu(&mut self, mut open: Open, done: Done, here: &Here<'_>) -> Step {
        match done {
            Done::Stay => {
                self.menu = Some(open);
                Step::Nothing
            }
            Done::Closed => {
                self.menu = None;
                Step::Nothing
            }
            Done::Beside(which) => {
                open = Open::new(which, &(here.menu)(which));
                self.menu = Some(open);
                Step::Nothing
            }
            Done::Chose(then) => {
                self.menu = None;
                match then {
                    // More's items are the bar's buttons, which do what
                    // their keys do on the screen under the menu
                    Then::Act(action) if open.which == Which::More => self.press(action.row, here),
                    Then::Act(action) => self.row_of_item(action),
                    Then::Sub(_) | Then::Nothing => Step::Nothing,
                }
            }
        }
    }

    /// A menu item's action, which goes where its row's action goes.
    fn row_of_item(&self, action: Action) -> Step {
        match crate::app::update::route(action.row) {
            crate::app::update::Route::Tab => Step::Tab(action),
            _ => Step::Shell(action),
        }
    }

    /// Does what the row `id`'s first key does where the person is, as a
    /// click on a button of the action bar does.
    fn press(&mut self, id: &str, here: &Here<'_>) -> Step {
        let key =
            contract::row(id).and_then(|r| Key::named(r.key.split(", ").next().unwrap_or(r.key)));
        match key {
            Some(key) => self.key(&key, here),
            None => Step::Nothing,
        }
    }

    /// Opens the search row over the open tab's list, where the node's
    /// `search` column names fields and the tab offers its search.
    fn open_search(&mut self, here: &Here<'_>) {
        let node = self.tab_node(here.tabs);
        if contract::search(node).is_empty() {
            return;
        }
        if let Some(list) = here.tabs.get(self.tab).and_then(|t| t.searchable()) {
            self.search = Some(Search::open(list, list.highlighted()));
        }
    }

    /// Closes the `?` search and clears its filter, as when the person
    /// leaves the tab it filters.
    pub fn close_search(&mut self) {
        self.search = None;
    }

    /// A key on the sheet on top, while nothing is over it: what the sheet
    /// asks for, or `None` for a key the sheet leaves to its row.
    fn sheet_key(&mut self, key: &Key, node: &'static str) -> Option<Step> {
        let sheet = self.sheets.last_mut().filter(|s| s.node() == node)?;
        match sheet.key(key, levels::row_for(node, key)) {
            Act::Pass => None,
            Act::Stay => Some(Step::Nothing),
            Act::Close => {
                self.sheets.pop();
                Some(Step::Nothing)
            }
            act => Some(Step::Sheet(act)),
        }
    }

    /// A key while the search row is open: the row's own keys, or `None`
    /// for a key the row leaves to the list.
    fn search_key(&mut self, key: &Key, node: &'static str, here: &Here<'_>) -> Option<Step> {
        // Ctrl+C quits from the search row as from everywhere
        if *key == Key::ctrl('c') {
            return Some(Step::Shell(Action::of("ctrl-c.any")));
        }
        let search = self.search.as_mut()?;
        match search.key(key) {
            Outcome::Closed => {
                self.search = None;
                Some(Step::Nothing)
            }
            Outcome::Ignored if node == "any.search" => Some(Step::Nothing),
            Outcome::Ignored => None,
            // the list moved its highlight while the filter was kept, and
            // the entry resumes from there
            Outcome::Resumed => {
                let list = here.tabs.get(self.tab).and_then(|t| t.searchable());
                if let Some(item) = list.and_then(|l| l.highlighted()) {
                    search.highlight(item);
                }
                Some(Step::Nothing)
            }
            Outcome::Typed | Outcome::Moved | Outcome::Kept => Some(Step::Nothing),
        }
    }

    /// Acts on a click at `at`, which the frame's `places` and the open
    /// menu's `menu` say what it hits. A click in the viewer goes to the
    /// open tab as its row of a click, with the cell counted from the
    /// viewer's top left corner.
    pub fn click(
        &mut self,
        at: Position,
        places: &Places,
        bar: &Bar,
        menu: Option<&MenuPlaces>,
        here: &Here<'_>,
    ) -> Step {
        let node = self.node(here.tabs, here.too_small);
        match levels::kind(node) {
            Kind::Menu => return self.menu_click(at, places, menu, here),
            Kind::Question | Kind::TooSmall => return Step::Nothing,
            _ => {}
        }
        // a click on the sheet on top is the sheet's, and one elsewhere in
        // the viewer reaches nothing under it
        if let Some(sheet) = self.sheets.last_mut() {
            let placed = sheet.place(&places.regions);
            let on = placed.inner.contains(at)
                || placed.frame.as_ref().is_some_and(|f| f.area().contains(at));
            if on {
                return match sheet.click(at, &placed) {
                    Act::Pass | Act::Stay => Step::Nothing,
                    Act::Close => {
                        self.sheets.pop();
                        Step::Nothing
                    }
                    act => Step::Sheet(act),
                };
            }
            if places.regions.viewer.contains(at) {
                return Step::Nothing;
            }
        }
        let listed = |id| levels::listed(node, id).is_some();
        match places.hit(at) {
            Some(Hit::Menu(name)) if listed("click-menu.view") => {
                self.open_menu(which_of(name), here)
            }
            Some(Hit::Tab(i)) if listed("click-tab.view") && i < here.tabs.len() => {
                self.tab = i;
                Step::Nothing
            }
            // a click on the problems count does what Alt+P does
            Some(Hit::Problems) if listed("click-facts.view") => {
                Step::Shell(Action::of("alt-p.view"))
            }
            Some(Hit::NotSaved) if listed("click-facts.view") => {
                Step::Shell(Action::of("click-facts.view"))
            }
            Some(Hit::More) => self.open_menu(Which::More, here),
            Some(Hit::Menu(_) | Hit::Tab(_) | Hit::NotSaved | Hit::Problems) => Step::Nothing,
            Some(Hit::Button(i)) => match bar.buttons.get(i).and_then(|b| b.action) {
                Some(id) => self.press(id, here),
                None => Step::Nothing,
            },
            None => self.viewer_click(at, places, node),
        }
    }

    fn viewer_click(&mut self, at: Position, places: &Places, node: &'static str) -> Step {
        let viewer = places.regions.viewer;
        if !viewer.contains(at) {
            return Step::Nothing;
        }
        let row = contract::node(node).and_then(|n| {
            n.rows()
                .filter(|r| r.supported && r.key.starts_with("Click"))
                .find(|r| crate::app::update::route(r.id) == crate::app::update::Route::Tab)
        });
        match row {
            Some(row) => Step::Tab(Action::key(
                row.id,
                input::Key::Mouse(input::Mouse {
                    button: input::Button::Press(0),
                    col: usize::from(at.x.saturating_sub(viewer.x)),
                    row: usize::from(at.y.saturating_sub(viewer.y)),
                }),
            )),
            None => Step::Nothing,
        }
    }

    fn menu_click(
        &mut self,
        at: Position,
        places: &Places,
        menu: Option<&MenuPlaces>,
        here: &Here<'_>,
    ) -> Step {
        let Some(mut open) = self.menu else {
            return Step::Nothing;
        };
        let lines = (here.menu)(open.which);
        let hit = menu.map_or(MenuHit::Outside, |m| m.hit(at));
        let done = match hit {
            MenuHit::Item(Pick::Item(i)) => open.click_item(i, &lines, here.submenu),
            MenuHit::Item(Pick::Sub(i)) => open.click_sub(i, here.submenu),
            MenuHit::Inside => Done::Stay,
            // a click on the other menu's name opens it in this one's place
            MenuHit::Outside => match places.hit(at) {
                Some(Hit::Menu(name)) if which_of(name) != open.which => {
                    Done::Beside(which_of(name))
                }
                _ => Done::Closed,
            },
        };
        self.after_menu(open, done, here)
    }

    /// Opens the level `node` over the open tab, as the tab asked.
    pub fn open_level(&mut self, node: &'static str) {
        if let Some(levels) = self.levels.get_mut(self.tab) {
            levels.push(node);
        }
    }

    /// Closes the level at the top of the open tab, as the tab asked.
    pub fn close_level(&mut self) {
        if let Some(levels) = self.levels.get_mut(self.tab) {
            levels.pop();
        }
    }

    /// Asks `question` over everything, a yes handing the tab `then`.
    pub fn ask(&mut self, text: String, then: Action) {
        self.ask_on("any.question.confirm", text, then);
    }

    /// Asks `question` as the question node `node`, such as Edit as text's
    /// question to edit again, a yes handing `then` to whoever acts on its
    /// row.
    pub fn ask_on(&mut self, node: &'static str, text: String, then: Action) {
        self.menu = None;
        self.question = Some(Question { text, node, then });
    }

    /// Opens the menu `open`, as `o` in the Flow tab opens File on its
    /// History submenu.
    pub fn show_menu(&mut self, open: Open) {
        self.menu = Some(open);
    }

    /// Goes to the tab at `index`.
    pub fn go_to(&mut self, index: usize, tabs: usize) {
        if index < tabs {
            self.tab = index;
        }
    }
}

/// The action of the row `id` for `key`: a key a row takes as it is, such
/// as a letter typed or a digit, travels with the action.
fn carrying(id: &'static str, key: &Key) -> Action {
    match old_key(key) {
        Some(k) => Action::key(id, k),
        None => Action::of(id),
    }
}

/// `key` in the older key type that vtium-config-core's action carries,
/// where that type has the key.
fn old_key(key: &Key) -> Option<input::Key> {
    use input::Key as K;
    let ctrl = key.mods.contains(Mods::CTRL);
    Some(match key.code {
        Code::Up => K::Up,
        Code::Down => K::Down,
        Code::Left => K::Left,
        Code::Right => K::Right,
        Code::Home => K::Home,
        Code::End => K::End,
        Code::PageUp => K::PageUp,
        Code::PageDown => K::PageDown,
        Code::Enter => K::Enter,
        Code::Tab if key.mods.contains(Mods::SHIFT) => K::BackTab,
        Code::Tab => K::Tab,
        Code::Backspace => K::Backspace,
        Code::Escape => K::Escape,
        Code::Char('c') if ctrl => K::Interrupt,
        Code::Char(c) if ctrl => K::Ctrl(c),
        Code::Char(c) if !key.mods.contains(Mods::ALT) => K::Char(c),
        _ => return None,
    })
}

fn which_of(name: MenuName) -> Which {
    match name {
        MenuName::File => Which::File,
        MenuName::Actions => Which::Actions,
    }
}

/// A mouse report in the shell's terms: a press of the left button, at
/// its cell, or `None` for anything else, which the TUI does not read.
pub fn pressed(mouse: &Mouse) -> Option<Position> {
    match mouse.button {
        input::Button::Press(0) => Some(Position::new(
            u16::try_from(mouse.col).unwrap_or(u16::MAX),
            u16::try_from(mouse.row).unwrap_or(u16::MAX),
        )),
        _ => None,
    }
}
