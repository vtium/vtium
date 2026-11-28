//! What each action does to the tab: the moves through the list, Enter on
//! the filter line or an item, Space on a color map or a shader and the
//! question before one a look uses is deactivated, the list box's keys and
//! clicks, the detail's scrolling, the keys of the search row, and Find's
//! request to highlight an item. The navigation module hands the tab an
//! action by its row of the key table, so each arm here answers rows of
//! the tab's nodes.
//!
//! References:
//! - ADR-1206 D2: update hands the open tab an action from the key
//!   table, and the tab returns commands.
//! - ADR-1205 D1 and D8: Space activates or deactivates a resource, and
//!   asks first when a look uses it.
//! - ADR-1403 D11, "Search, on ?" and "The list box": their keys and
//!   clicks.
//! - ADR-1206 D4: Find's Enter is a request to the item's tab to
//!   highlight it.

use ratatui_core::layout::Position;
use vtium_config_core::batch::Change as BatchChange;
use vtium_config_core::document::{Change as DocChange, Edit};
use vtium_config_core::found::Resource;
use vtium_config_core::found::effects::Group;
use vtium_config_core::ui::{Action, Carries, Change, Command, Message, Request, Shared};
use vtium_config_widgets::input::{self, Button};
use vtium_config_widgets::keys::{Code, Key, Mods};
use vtium_config_widgets::list_box::{ListBox, Outcome as BoxOutcome};
use vtium_config_widgets::search::{Outcome as SearchOutcome, Search};

use crate::list::{self, Item};
use crate::{Effects, NODE_GROUPS, Open, Panes, detail, words};

/// The step of Home and End: past either end of any list.
const FAR: isize = isize::MAX / 2;

/// The row of the answer yes to the question before a deactivation, which
/// the question hands back to the tab with the item it asked about.
pub const YES: &str = "y.question";

/// Acts on `action` and returns the commands it makes.
pub fn update(tab: &mut Effects, shared: &Shared, action: Action) -> Vec<Command> {
    match (action.row, &action.carries) {
        ("enter.typing.find", Carries::Item(at)) => {
            go_to(tab, *at);
            return Vec::new();
        }
        (YES, Carries::Item(at)) => return answered(shared, *at),
        _ => {}
    }
    match &tab.open {
        Open::Detail { .. } => {
            detail_keys(tab, shared, &action);
            Vec::new()
        }
        Open::Groups(_) => {
            groups(tab, &action);
            Vec::new()
        }
        Open::Nothing if tab.filter.as_ref().is_some_and(Search::typing) => {
            searching(tab, &action);
            Vec::new()
        }
        Open::Nothing => browse(tab, shared, &action),
    }
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
        "space" => Code::Char(' '),
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

/// The rows a page holds in the list.
fn page(tab: &Effects) -> isize {
    let rows = list::rows(Panes::of(tab.seen.get()).list);
    isize::try_from(rows.max(1)).unwrap_or(1)
}

/// The keys of the list.
fn browse(tab: &mut Effects, shared: &Shared, action: &Action) -> Vec<Command> {
    if let Some(at) = click(action) {
        return clicked(tab, shared, at);
    }
    match verb(action) {
        "up" => step(tab, shared, -1),
        "down" => step(tab, shared, 1),
        "home" => step(tab, shared, -FAR),
        "end" => step(tab, shared, FAR),
        "pgup" => step(tab, shared, -page(tab)),
        "pgdn" => step(tab, shared, page(tab)),
        "enter" => enter(tab, shared),
        "space" => return space(tab, shared),
        "question" => open_search(tab),
        "esc" => {
            tab.filter = None;
        }
        _ => {}
    }
    Vec::new()
}

/// Moves the highlight `by` rows: among the matches where a filter is
/// kept, and otherwise through the filter line and the items, stopping at
/// either end.
fn step(tab: &mut Effects, shared: &Shared, by: isize) {
    if let Some(filter) = tab.filter.as_mut() {
        filter.step(by);
        tab.highlight = filter.highlighted();
        return;
    }
    let count = tab.items(shared).len();
    // row 0 is the filter line, and each item follows
    let row = tab.highlight.map_or(0, |h| h.saturating_add(1));
    let row = row.saturating_add_signed(by).min(count);
    tab.highlight = row.checked_sub(1);
}

/// Enter: on the filter line, opens the list of groups in the list box;
/// on an item, opens its detail.
fn enter(tab: &mut Effects, shared: &Shared) {
    match tab.highlighted(shared) {
        Some(item) => tab.open = Open::Detail { item, top: 0 },
        None if tab.highlight.is_none() => {
            tab.open = Open::Groups(Box::new(ListBox::open(NODE_GROUPS, &tab.choices())));
        }
        None => {}
    }
}

/// `?`: puts the cursor back in a kept filter's entry, or opens the
/// search row over the items of the groups shown, on the highlighted one.
fn open_search(tab: &mut Effects) {
    if let Some(filter) = tab.filter.as_mut() {
        filter.key(&Key::char('?'));
        return;
    }
    let search = Search::open(&*tab, tab.highlight);
    tab.highlight = search.highlighted().or(tab.highlight);
    tab.filter = Some(search);
}

/// A key typed in the search row: the entry changes and the list
/// filters, Up and Down move among the matches, Enter keeps the filter,
/// and Esc or Backspace on an empty entry clear it.
fn searching(tab: &mut Effects, action: &Action) {
    let Some(key) = pressed(action) else { return };
    let Some(filter) = tab.filter.as_mut() else {
        return;
    };
    match filter.key(&key) {
        SearchOutcome::Typed | SearchOutcome::Moved | SearchOutcome::Kept => {
            tab.highlight = filter.highlighted().or(tab.highlight);
        }
        SearchOutcome::Closed => tab.filter = None,
        SearchOutcome::Resumed | SearchOutcome::Ignored => {}
    }
}

/// A click on the filter line or an item: the first highlights it, and a
/// click on what is highlighted acts as Enter does.
fn clicked(tab: &mut Effects, shared: &Shared, at: Position) -> Vec<Command> {
    let places = tab.places(shared);
    if places.filter.contains(at) {
        if tab.highlight.is_none() {
            enter(tab, shared);
        } else {
            tab.highlight = None;
        }
        return Vec::new();
    }
    if let Some(&(_, item)) = places.items.iter().find(|(r, _)| r.contains(at)) {
        if tab.highlight == Some(item) {
            enter(tab, shared);
            return Vec::new();
        }
        tab.highlight = Some(item);
        if let Some(filter) = tab.filter.as_mut() {
            move_filter_to(filter, item);
        }
    }
    Vec::new()
}

/// Moves the filter's highlight to the item `item`, which it matches.
fn move_filter_to(filter: &mut Search, item: usize) {
    let at = |i: usize| filter.matches().iter().position(|&m| m == i);
    let (Some(from), Some(to)) = (filter.highlighted().and_then(at), at(item)) else {
        return;
    };
    let by = isize::try_from(to).unwrap_or(0) - isize::try_from(from).unwrap_or(0);
    filter.step(by);
}

/// Find's Enter on an item, by its place in the list of every group: the
/// list shows every group, the filter is cleared, and the item is
/// highlighted.
fn go_to(tab: &mut Effects, at: usize) {
    tab.group = Group::All;
    tab.filter = None;
    tab.open = Open::Nothing;
    tab.highlight = Some(at);
}

/// The keys and clicks of the list box of groups: a kept pick shows those
/// groups from the filter line, and Esc closes the box unchanged.
fn groups(tab: &mut Effects, action: &Action) {
    let choices = tab.choices();
    let at = click(action);
    let key = pressed(action);
    let Open::Groups(list) = &tab.open else {
        return;
    };
    let rect = tab.groups_rect(list);
    let Open::Groups(list) = &mut tab.open else {
        return;
    };
    let outcome = match (at, key) {
        (Some(at), _) => list.click(at, &choices, rect),
        (None, Some(key)) => list.key(&key, &choices, rect),
        (None, None) => BoxOutcome::Ignored,
    };
    match outcome {
        BoxOutcome::Kept(group) => {
            tab.group = group;
            tab.highlight = None;
            tab.offset = 0;
            tab.filter = None;
            tab.open = Open::Nothing;
        }
        BoxOutcome::Closed | BoxOutcome::Outside => tab.open = Open::Nothing,
        BoxOutcome::Moved | BoxOutcome::Searched | BoxOutcome::Ignored => {}
    }
}

/// The detail's keys: Up and Down scroll the lines under the title, and
/// Esc goes back to the list with the same item highlighted.
fn detail_keys(tab: &mut Effects, shared: &Shared, action: &Action) {
    let Open::Detail { item, top } = tab.open else {
        return;
    };
    let area = tab.seen.get();
    let last = detail::last_top(
        detail::lines(shared, item, usize::from(area.width).saturating_sub(1)).len(),
        area,
    );
    let top = top.min(last);
    tab.open = match verb(action) {
        "esc" => Open::Nothing,
        "up" => Open::Detail {
            item,
            top: top.saturating_sub(1),
        },
        "down" => Open::Detail {
            item,
            top: top.saturating_add(1).min(last),
        },
        _ => Open::Detail { item, top },
    };
}

/// A line for the message line, with the full text F1 opens behind it.
fn say(line: impl Into<String>, more: Option<String>) -> Vec<Command> {
    vec![Command::Change(Change::Say(Message {
        line: line.into(),
        more,
    }))]
}

/// Space: on a color map or a shader, activates it or deactivates it,
/// asking first when a look uses it; on a vtlook or the filter line, says
/// why it does nothing there.
fn space(tab: &Effects, shared: &Shared) -> Vec<Command> {
    let item = match tab.highlighted(shared) {
        None => return say(words::SPACE_ON_FILTER, None),
        Some(Item::Vtlook(_)) => return say(words::SPACE_ON_VTLOOK, None),
        Some(item) => item,
    };
    let effects = &shared.found.effects;
    let Some(index) = list::items(effects, Group::All)
        .iter()
        .position(|i| *i == item)
    else {
        return Vec::new();
    };
    match list::resource(effects, item) {
        Some(r) => toggle(shared, r, index, true),
        None => Vec::new(),
    }
}

/// The answer yes to the question before the item `at`, by its place in
/// the list of every group, is deactivated.
fn answered(shared: &Shared, at: usize) -> Vec<Command> {
    let effects = &shared.found.effects;
    let Some(item) = list::items(effects, Group::All).get(at).copied() else {
        return Vec::new();
    };
    match list::resource(effects, item) {
        Some(r) if list::active(shared, r) => toggle(shared, r, at, false),
        _ => Vec::new(),
    }
}

/// The change that turns `r` on or off: one change to the document's
/// active resources, or, where `ask` is set and a look uses an active
/// `r`, the question first, which hands back the item `index`.
fn toggle(shared: &Shared, r: &Resource, index: usize, ask: bool) -> Vec<Command> {
    let kind = r.kind.words();
    let path = r.path.display().to_string();
    if r.own {
        let (line, more) = words::space_own(kind, &path);
        return say(line, Some(more));
    }
    let on = list::active(shared, r);
    if !on && let Err(why) = &r.trust {
        let (line, more) = words::space_untrusted(kind, why);
        return say(line, Some(more));
    }
    let name = r.name();
    if on && ask {
        let uses = list::uses(shared, r);
        if !uses.is_empty() {
            return vec![Command::Request(Request::Ask {
                question: words::deactivate_question(kind, &name, &uses),
                then: Action::item(YES, index),
            })];
        }
    }
    let want = !on;
    // the draft holds a change only where it differs from what is installed
    let change = (want != list::installed_active(&r.origin)).then_some(if want {
        BatchChange::Activate
    } else {
        BatchChange::Deactivate
    });
    let named = if want {
        words::activate(&name)
    } else {
        words::deactivate(&name)
    };
    vec![Command::Change(Change::Document(DocChange::new(
        named,
        Edit::Active {
            path: r.path.clone(),
            change,
        },
    )))]
}
