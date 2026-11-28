//! What each action does to the tab: the moves through the list, Enter
//! on the filter line, the configuration's row or a font, Space and
//! Activate missing, `t` and `p`, the list box of the filters, the
//! detail's scroll and its Enter, a click, and Find's request to
//! highlight a font. The navigation module hands the tab an action by its
//! row of the key table, so each arm here answers rows of the tab's
//! nodes. The shell takes the keys of the `?` search's row, and the
//! highlight follows the search the shell hands the tab ([`follow`]).
//!
//! References:
//! - ADR-1206 D2: update hands the open tab an action from the key
//!   table, and the tab returns commands.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   Activate missing as one change, its message, and Space as the
//!   knowing choice that turns on a font turned off on purpose.
//! - ADR-1205 D8: deactivating a font a look uses asks first.
//! - ADR-1206 D3: the navigation state holds the search, and each tab
//!   keeps the indexes that match.

use ratatui_core::layout::Position;
use vtium_config_core::batch::Change as Batch;
use vtium_config_core::document::{self, Edit};
use vtium_config_core::fontuse::{Activated, State, activate::words as activate_words};
use vtium_config_core::ui::{Action, Carries, Change, Command, Message, Request, Shared};
use vtium_config_widgets::input::{self, Button};
use vtium_config_widgets::keys::{Code, Key, Mods};
use vtium_config_widgets::list_box::{ListBox, Outcome as BoxOutcome};
use vtium_config_widgets::search::Search;

use crate::layout::Panes;
use crate::list::{self, Sel};
use crate::states::{Item, Known};
use crate::{FilterChoices, Fonts, NODE_FILTERS, Open, no_images, own, trial, words};

/// The step of Home and End: past either end of the list.
const FAR: isize = isize::MAX / 2;

/// Acts on `action` and returns the commands it makes.
pub fn update(tab: &mut Fonts, shared: &Shared, action: Action) -> Vec<Command> {
    if let (true, Carries::Item(at)) = (action.is("enter.typing.find"), &action.carries) {
        go_to(tab, shared, *at);
        return Vec::new();
    }
    let out = match &tab.open {
        Open::Detail { .. } => in_detail(tab, shared, &action),
        Open::Filters(_) => in_filters(tab, &action),
        Open::Nothing => browse(tab, shared, &action),
    };
    settle(tab, shared);
    out
}

/// The verb of a row's id, its first part, as `up` in `up.view`.
fn verb(action: &Action) -> &str {
    action.row.split('.').next().unwrap_or("")
}

/// The key an action stands for: the key it carries, or the key its row
/// names, for an action the shell sends with nothing but its row.
fn pressed(action: &Action) -> Option<Key> {
    if let Carries::Key(k) = &action.carries {
        return key_of(k);
    }
    let code = match verb(action) {
        "up" => Code::Up,
        "down" => Code::Down,
        "home" => Code::Home,
        "end" => Code::End,
        "pgup" => Code::PageUp,
        "pgdn" => Code::PageDown,
        "enter" => Code::Enter,
        "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "question" => Code::Char('?'),
        _ => return None,
    };
    Some(Key::plain(code))
}

/// The rebuilt TUI's key for the old decoder's, which the action carries.
fn key_of(key: &input::Key) -> Option<Key> {
    let code = match key {
        input::Key::Up => Code::Up,
        input::Key::Down => Code::Down,
        input::Key::Left => Code::Left,
        input::Key::Right => Code::Right,
        input::Key::PageUp => Code::PageUp,
        input::Key::PageDown => Code::PageDown,
        input::Key::Home => Code::Home,
        input::Key::End => Code::End,
        input::Key::Enter => Code::Enter,
        input::Key::Tab => Code::Tab,
        input::Key::BackTab => return Some(Key::plain(Code::Tab).with(Mods::SHIFT)),
        input::Key::Backspace => Code::Backspace,
        input::Key::Interrupt => return Some(Key::ctrl('c')),
        input::Key::Escape => Code::Escape,
        input::Key::Char(c) => Code::Char(*c),
        input::Key::Ctrl(c) => return Some(Key::ctrl(*c)),
        input::Key::Mouse(_) => return None,
    };
    Some(Key::plain(code))
}

/// The cell a left click landed on, where the action carries one.
fn click(action: &Action) -> Option<Position> {
    match &action.carries {
        Carries::Key(input::Key::Mouse(m)) if m.button == Button::Press(0) => Some(Position::new(
            u16::try_from(m.col).unwrap_or(u16::MAX),
            u16::try_from(m.row).unwrap_or(u16::MAX),
        )),
        _ => None,
    }
}

/// A message with nothing behind it.
fn say(line: impl Into<String>) -> Vec<Command> {
    vec![Command::Change(Change::Say(Message {
        line: line.into(),
        more: None,
    }))]
}

/// The keys of the list while nothing is open over it.
fn browse(tab: &mut Fonts, shared: &Shared, action: &Action) -> Vec<Command> {
    if let Some(at) = click(action) {
        return clicked(tab, shared, at);
    }
    match action.row {
        "space.view.resources" => return space(tab, shared, action),
        "shift-a.view.fonts" => return activate_missing(&tab.known(shared)),
        "t.view.fonts" => return try_live(tab, shared),
        "p.view.fonts" => {
            if no_images(shared) {
                return say(words::P_NO_IMAGES);
            }
            tab.picture = !tab.picture;
            return Vec::new();
        }
        _ => {}
    }
    match verb(action) {
        "up" => step(tab, shared, -1),
        "down" => step(tab, shared, 1),
        "home" => step(tab, shared, -FAR),
        "end" => step(tab, shared, FAR),
        "pgup" => step(tab, shared, -page(tab, shared)),
        "pgdn" => step(tab, shared, page(tab, shared)),
        "enter" => return enter(tab, shared),
        _ => {}
    }
    Vec::new()
}

/// The rows a page holds in the list's body.
fn page(tab: &Fonts, shared: &Shared) -> isize {
    let rows = usize::from(Panes::of(tab.seen.get()).list.height)
        .saturating_sub(list::header(&tab.known(shared)).len());
    isize::try_from(rows.max(1)).unwrap_or(1)
}

/// Moves the highlight `by` rows among those it can stand on, and stops
/// at the ends.
fn step(tab: &mut Fonts, shared: &Shared, by: isize) {
    let known = tab.known(shared);
    let order = tab.order(&known, shared);
    let at = order.iter().position(|s| *s == tab.sel).unwrap_or(0);
    let last = order.len().saturating_sub(1);
    let to = at.saturating_add_signed(by).min(last);
    let sel = order.get(to).copied().unwrap_or(Sel::Filter);
    drop(known);
    tab.sel = sel;
}

/// Enter: the filters on the filter line, Activate missing on the
/// configuration's row, and a font's detail on a font.
fn enter(tab: &mut Fonts, shared: &Shared) -> Vec<Command> {
    match tab.sel {
        Sel::Filter => {
            let choices = FilterChoices::new(tab.filter);
            tab.open = Open::Filters(Box::new(ListBox::open(NODE_FILTERS, &choices)));
            Vec::new()
        }
        Sel::Activate => activate_missing(&tab.known(shared)),
        Sel::Item(item) => {
            let own = tab.file(shared).is_some_and(own::is_own);
            tab.open = Open::Detail { item, top: 0, own };
            Vec::new()
        }
    }
}

/// Activate missing: every font the configuration uses that is installed
/// and trusted but not active goes into the document's batch as one
/// change. The message says what it activated, and names apart the fonts
/// turned off on purpose, which stay off, and those it cannot activate,
/// with why behind F1. The message goes first, so a change the document
/// refuses replaces it with the reason.
pub fn activate_missing(known: &Known) -> Vec<Command> {
    let activated = known.to_activate();
    let done = Activated {
        activated: activated.clone(),
        turned_off: known.counts.turned_off,
        cannot: known
            .used
            .fonts
            .iter()
            .filter_map(|f| Some((f.name(), f.state.reason()?.to_owned())))
            .collect(),
    };
    let mut out = vec![Command::Change(Change::Say(done.message()))];
    if !activated.is_empty() {
        let edits = activated
            .iter()
            .map(|path| Edit::Active {
                path: path.clone(),
                change: Some(Batch::Activate),
            })
            .collect();
        out.push(Command::Change(Change::Document(document::Change::new(
            activate_words::step(activated.len()),
            Edit::All(edits),
        ))));
    }
    out
}

/// Space on the highlighted font: activates it, or deactivates it where
/// it is active, as one change. Deactivating a font a look uses asks
/// first, and the question's yes hands the action back carrying the
/// font's place in the list. A font of your own, one that fails the trust
/// rule, one not installed and a built-in font are not changed, and the
/// message says why.
fn space(tab: &Fonts, shared: &Shared, action: &Action) -> Vec<Command> {
    let known = tab.known(shared);
    let item = match tab.sel {
        Sel::Filter => return say(words::SPACE_ON_FILTER),
        Sel::Activate => return say(words::SPACE_ON_ACTIVATE),
        Sel::Item(item) => item,
    };
    let Some(at) = known.position(item) else {
        return Vec::new();
    };
    let Item::File(i) = item else {
        return match item {
            Item::Named(_) => say(words::space_missing(
                known.fields.get(at).map_or("", |f| f[0].as_str()),
            )),
            _ => Vec::new(),
        };
    };
    let Some(f) = shared.found.fonts.files().get(i) else {
        return Vec::new();
    };
    let name = vtium_config_widgets::canvas::printable(&f.resource.name());
    let Some(state) = known.state(at) else {
        return Vec::new();
    };
    let active = match state {
        State::FailsTrust { own: true, .. } => return say(words::space_own(&name)),
        State::FailsTrust { check, .. } => {
            return vec![Command::Change(Change::Say(Message {
                line: words::space_untrusted(&name),
                more: Some(check.clone()),
            }))];
        }
        State::Missing => return say(words::space_missing(&name)),
        State::Active => true,
        State::NotActive | State::TurnedOff(_) => false,
    };
    let confirmed = matches!(action.carries, Carries::Item(c) if c == at);
    let users = known.users_of(f.path());
    if active && !users.is_empty() && !confirmed {
        return vec![Command::Request(Request::Ask {
            question: words::deactivate_question(&name, &words::users(users, 2)),
            then: Action::item("space.view.resources", at),
        })];
    }
    let installed = known.installed.get(f.path()).copied().unwrap_or(false);
    let want = if active {
        Batch::Deactivate
    } else {
        Batch::Activate
    };
    // the change goes back to what is installed where it undoes the
    // draft's own change
    let change = (installed == active).then_some(want);
    let step = if active {
        words::deactivate_step(&name)
    } else {
        words::activate_step(&name)
    };
    vec![Command::Change(Change::Document(document::Change::new(
        step,
        Edit::Active {
            path: f.path().to_path_buf(),
            change,
        },
    )))]
}

/// `t`: the highlighted font's path or name for the shell's live trial,
/// or why the console would refuse it.
fn try_live(tab: &Fonts, shared: &Shared) -> Vec<Command> {
    let known = tab.known(shared);
    let item = match tab.sel {
        Sel::Item(item) => item,
        _ => return say(words::TRY_NO_FONT),
    };
    match item {
        Item::BuiltIn(name) => vec![Command::Change(Change::Try(name.to_owned()))],
        Item::Named(_) => {
            let name = known
                .position(item)
                .and_then(|at| known.fields.get(at))
                .map_or_else(String::new, |f| f[0].clone());
            say(words::try_missing(&name))
        }
        Item::File(i) => {
            let Some(f) = shared.found.fonts.files().get(i) else {
                return Vec::new();
            };
            let Some(state) = known.position(item).and_then(|at| known.state(at)) else {
                return Vec::new();
            };
            let installed = known.installed.get(f.path()).copied().unwrap_or(false);
            match trial::path_to_try(shared.document().text(), f, state, installed) {
                Ok(path) => vec![Command::Change(Change::Try(path))],
                Err(why) => vec![Command::Change(Change::Say(Message {
                    line: why.line,
                    more: why.more,
                }))],
            }
        }
    }
}

/// The highlight follows the search's.
pub fn follow(tab: &mut Fonts, shared: &Shared) {
    let Some(at) = tab.search.as_ref().and_then(Search::highlighted) else {
        return;
    };
    let item = tab.known(shared).items.get(at).copied();
    if let Some(item) = item {
        tab.sel = Sel::Item(item);
    }
}

/// A click on a row of the list: the first highlights it, and a click on
/// what is highlighted acts as Enter does.
fn clicked(tab: &mut Fonts, shared: &Shared, at: Position) -> Vec<Command> {
    let places = tab.places(shared);
    let Some(&(_, sel)) = places.rows.iter().find(|(r, _)| r.contains(at)) else {
        return Vec::new();
    };
    if tab.sel == sel {
        return enter(tab, shared);
    }
    tab.sel = sel;
    Vec::new()
}

/// Find's Enter on a font: the font is highlighted, with the filter and
/// the search cleared where they would hide it, and whatever was open
/// over the list closed.
fn go_to(tab: &mut Fonts, shared: &Shared, at: usize) {
    let item = tab.known(shared).items.get(at).copied();
    let Some(item) = item else { return };
    tab.open = Open::Nothing;
    tab.search = None;
    if !list::keeps(tab.filter, item, shared) {
        tab.filter = vtium_config_core::found::fonts::Filter::All;
    }
    tab.sel = Sel::Item(item);
    settle(tab, shared);
}

/// The keys and clicks of the list box of the filters: a kept pick
/// shows the fonts that pass it, and Esc closes the box unchanged.
fn in_filters(tab: &mut Fonts, action: &Action) -> Vec<Command> {
    let choices = FilterChoices::new(tab.filter);
    let rect = tab.filters_rect(&choices);
    let at = click(action);
    let key = pressed(action);
    let Open::Filters(lb) = &mut tab.open else {
        return Vec::new();
    };
    let outcome = match (at, key) {
        (Some(at), _) => lb.click(at, &choices, rect),
        (None, Some(key)) => lb.key(&key, &choices, rect),
        (None, None) => BoxOutcome::Ignored,
    };
    match outcome {
        BoxOutcome::Kept(filter) => {
            tab.filter = filter;
            tab.open = Open::Nothing;
            tab.sel = Sel::Filter;
            tab.top = 0;
        }
        BoxOutcome::Closed | BoxOutcome::Outside => tab.open = Open::Nothing,
        BoxOutcome::Moved | BoxOutcome::Searched | BoxOutcome::Ignored => {}
    }
    Vec::new()
}

/// The keys of a font's detail: Up and Down scroll it, Esc goes back,
/// and Enter on a font of your own starts its install as a system font.
fn in_detail(tab: &mut Fonts, shared: &Shared, action: &Action) -> Vec<Command> {
    let Open::Detail { item, top, own } = tab.open else {
        return Vec::new();
    };
    let area = tab.seen.get();
    let last = {
        let known = tab.known(shared);
        let (title, entries) = crate::detail::entries(&known, shared, item);
        let rows = crate::detail::rows(&title, &entries, usize::from(area.width));
        crate::detail::last_top(rows.len(), usize::from(area.height))
    };
    match action.row {
        "up.layer.detail" => {
            tab.open = Open::Detail {
                item,
                top: top.min(last).saturating_sub(1),
                own,
            };
        }
        "down.layer.detail" => {
            tab.open = Open::Detail {
                item,
                top: top.saturating_add(1).min(last),
                own,
            };
        }
        "enter.layer.font" => {
            return match tab.system_font(shared) {
                Ok(path) => {
                    tab.open = Open::Detail { item, top: 0, own };
                    own::install(path)
                }
                Err(why) => say(why),
            };
        }
        _ if verb(action) == "esc" => tab.open = Open::Nothing,
        _ => {}
    }
    Vec::new()
}

/// After an action: the highlight stays on a row the list shows, and the
/// window moves so it shows the highlight.
pub fn settle(tab: &mut Fonts, shared: &Shared) {
    let known = tab.known(shared);
    let sel = tab.settled_sel(&known, shared);
    let shown = list::shown(&known, shared, tab.filter, tab.search.as_ref());
    let body = list::body(&known, &shown, tab.search.as_ref());
    let rows = usize::from(Panes::of(tab.seen.get()).list.height)
        .saturating_sub(list::header(&known).len());
    let top = list::settled(&body, &known, sel, tab.top, rows);
    drop(known);
    tab.sel = sel;
    tab.top = top;
}
