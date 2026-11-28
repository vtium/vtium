//! What each action does to the tab: the moves through the sections and
//! the keys, Enter on a section or a key, the list box's keys and clicks,
//! the keys of a value being typed and of the search row, and Find's
//! request to highlight a key. The navigation module hands the tab an
//! action by its row of the key table, so each arm here answers rows of
//! the tab's nodes.
//!
//! References:
//! - ADR-1206 D2: update hands the open tab an action from the key
//!   table, and the tab returns commands.
//! - ADR-1403 D11, "Search, on ?": the search row's keys, and a kept
//!   filter's first Esc.
//! - ADR-1403 D11, "The list box": its keys and clicks.
//! - ADR-1206 D4: Find's Enter is a request to the item's tab to
//!   highlight it, which the tab answers as it answers a move.

use ratatui_core::layout::Position;
use vtium_config_core::ui::{Action, Carries, Command, Shared};
use vtium_config_widgets::input::{self, Button};
use vtium_config_widgets::keys::{Code, Key, Mods};
use vtium_config_widgets::list_box::ListBox;
use vtium_config_widgets::list_box::Outcome as BoxOutcome;
use vtium_config_widgets::search::{Outcome as SearchOutcome, Search};

use crate::layout::Panes;
use crate::value::{self, Pick, Values};
use crate::{Focus, NODE_LIST, Open, Settings, keys, position, sections};

/// The step of Home and End: past either end of any list.
const FAR: isize = isize::MAX / 2;

/// Acts on `action` and returns the commands it makes.
pub fn update(tab: &mut Settings, shared: &Shared, action: Action) -> Vec<Command> {
    if let (true, Carries::Item(at)) = (action.is("enter.typing.find"), &action.carries) {
        go_to(tab, *at);
        return Vec::new();
    }
    let out = match &tab.open {
        Open::Typing(_) => typing(tab, shared, &action),
        Open::Values { .. } => values(tab, shared, &action),
        Open::Nothing if tab.filter.as_ref().is_some_and(Search::typing) => {
            searching(tab, &action);
            Vec::new()
        }
        Open::Nothing => browse(tab, shared, &action),
    };
    tab.settle();
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
        "enter" | "list" => Code::Enter,
        "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "question" => Code::Char('?'),
        "q" => Code::Char('q'),
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
        Carries::Key(input::Key::Mouse(m)) if m.button == Button::Press(0) => {
            Some(position(m.col, m.row))
        }
        _ => None,
    }
}

/// The rows a page holds in the pane that has the focus.
fn page(tab: &Settings) -> isize {
    let panes = Panes::of(tab.seen.get());
    let rows = match tab.focus {
        Focus::Sections => sections::rows(panes.left),
        Focus::Keys => keys::rows(panes.right),
    };
    isize::try_from(rows.max(1)).unwrap_or(1)
}

/// The keys of the list of sections and of a section's keys.
fn browse(tab: &mut Settings, shared: &Shared, action: &Action) -> Vec<Command> {
    if let Some(at) = click(action) {
        return clicked(tab, shared, at);
    }
    if let Carries::Item(at) = action.carries
        && verb(action).starts_with("click")
    {
        go_to(tab, at);
        return Vec::new();
    }
    match verb(action) {
        "up" => step(tab, -1),
        "down" => step(tab, 1),
        "home" => step(tab, -FAR),
        "end" => step(tab, FAR),
        "pgup" => step(tab, -page(tab)),
        "pgdn" => step(tab, page(tab)),
        "enter" | "list" => return enter(tab, shared),
        "question" => open_search(tab),
        "esc" => back(tab),
        _ => {}
    }
    Vec::new()
}

/// Enter: on a section, moves into its keys; on a key, opens its values
/// in the list box, or lets the person type its value.
fn enter(tab: &mut Settings, shared: &Shared) -> Vec<Command> {
    match tab.focus {
        Focus::Sections => {
            if let Some(s) = tab.table.section(tab.section) {
                tab.key = s.keys.start;
                tab.focus = Focus::Keys;
            }
        }
        Focus::Keys => {
            let Some(entry) = tab.lit_key().and_then(|at| tab.table.entry(at)) else {
                return Vec::new();
            };
            let text = shared.document().text();
            tab.open = match Values::of(entry.key, text) {
                Some(values) => Open::Values {
                    list: Box::new(ListBox::open(NODE_LIST, &values)),
                    values,
                },
                None => Open::Typing(value::typing_starts(entry.key, text)),
            };
        }
    }
    Vec::new()
}

/// Esc: clears a kept filter first, then goes from the keys back to the
/// sections, with the same section highlighted.
fn back(tab: &mut Settings) {
    if tab.filter.take().is_some() {
        return;
    }
    tab.focus = Focus::Sections;
}

/// Moves the highlight `by` rows in the pane that has the focus, among
/// the keys the filter keeps where it is kept, and stops at the ends.
fn step(tab: &mut Settings, by: isize) {
    if let Some(filter) = tab.filter.as_mut() {
        filter.step(by);
        follow(tab);
        return;
    }
    match tab.focus {
        Focus::Sections => {
            let last = tab.table.sections.len().saturating_sub(1);
            tab.section = tab.section.saturating_add_signed(by).min(last);
            tab.right_offset = 0;
        }
        Focus::Keys => {
            let Some(s) = tab.table.section(tab.section) else {
                return;
            };
            let last = s.keys.end.saturating_sub(1);
            tab.key = tab.key.saturating_add_signed(by).clamp(s.keys.start, last);
        }
    }
}

/// The highlight follows the filter's, with the section of the key it
/// stands on.
fn follow(tab: &mut Settings) {
    let Some(at) = tab.filter.as_ref().and_then(Search::highlighted) else {
        return;
    };
    tab.key = at;
    tab.focus = Focus::Keys;
    if let Some(e) = tab.table.entry(at) {
        tab.section = e.section;
    }
}

/// `?`: puts the cursor back in a kept filter's entry, or opens the
/// search row over every key, on the highlighted key or the section's
/// first.
fn open_search(tab: &mut Settings) {
    if let Some(filter) = tab.filter.as_mut() {
        filter.key(&Key::char('?'));
        return;
    }
    let from = match tab.focus {
        Focus::Keys => Some(tab.key),
        Focus::Sections => tab.table.section(tab.section).map(|s| s.keys.start),
    };
    tab.filter = Some(Search::open(&*tab, from));
    follow(tab);
}

/// A key typed in the search row: the entry changes and the keys filter,
/// Up and Down move among the matches, Enter keeps the filter, and Esc or
/// Backspace on an empty entry clear it.
fn searching(tab: &mut Settings, action: &Action) {
    let Some(key) = pressed(action) else { return };
    let Some(filter) = tab.filter.as_mut() else {
        return;
    };
    match filter.key(&key) {
        SearchOutcome::Typed | SearchOutcome::Moved | SearchOutcome::Kept => follow(tab),
        SearchOutcome::Closed => tab.filter = None,
        SearchOutcome::Resumed | SearchOutcome::Ignored => {}
    }
}

/// A click on a section or a key: the first highlights it, and a click on
/// what is highlighted acts as Enter does.
fn clicked(tab: &mut Settings, shared: &Shared, at: Position) -> Vec<Command> {
    let places = tab.places(shared);
    if let Some(&(_, section)) = places.sections.iter().find(|(r, _)| r.contains(at)) {
        if tab.focus == Focus::Sections && tab.section == section {
            return enter(tab, shared);
        }
        tab.focus = Focus::Sections;
        tab.section = section;
        tab.right_offset = 0;
        if let Some(filter) = tab.filter.as_mut() {
            // with a filter kept, the section's first match takes the
            // highlight
            if let Some(first) = tab
                .table
                .section(section)
                .and_then(|s| s.keys.clone().find(|&k| filter.is_match(k)))
            {
                move_filter_to(filter, first);
                follow(tab);
            }
        }
        return Vec::new();
    }
    if let Some(&(_, key)) = places.keys.iter().find(|(r, _)| r.contains(at)) {
        if tab.focus == Focus::Keys && tab.key == key {
            return enter(tab, shared);
        }
        tab.focus = Focus::Keys;
        tab.key = key;
        if let Some(filter) = tab.filter.as_mut() {
            move_filter_to(filter, key);
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

/// Find's Enter on a key: the key is highlighted in its section, and the
/// filter, if any, is cleared so the key shows.
fn go_to(tab: &mut Settings, at: usize) {
    let Some(e) = tab.table.entry(at) else { return };
    tab.section = e.section;
    tab.key = at;
    tab.focus = Focus::Keys;
    tab.open = Open::Nothing;
    tab.filter = None;
    tab.settle();
}

/// The keys and clicks of the list box of a key's values: a kept pick is
/// one change, Other… lets the person type a name, and Esc closes the box
/// unchanged.
fn values(tab: &mut Settings, shared: &Shared, action: &Action) -> Vec<Command> {
    let Open::Values { values, .. } = &tab.open else {
        return Vec::new();
    };
    let rect = tab.list_rect(shared, values);
    let at = click(action);
    let key = pressed(action);
    let Open::Values { list, values } = &mut tab.open else {
        return Vec::new();
    };
    let outcome = match (at, key) {
        (Some(at), _) => list.click(at, values, rect),
        (None, Some(key)) => list.key(&key, values, rect),
        (None, None) => BoxOutcome::Ignored,
    };
    let key = values.key();
    match outcome {
        BoxOutcome::Kept(Pick::Set(v)) => {
            tab.open = Open::Nothing;
            vec![value::set(key, v)]
        }
        BoxOutcome::Kept(Pick::Other) => {
            tab.open = Open::Typing(value::typing_starts(key, shared.document().text()));
            Vec::new()
        }
        BoxOutcome::Closed => {
            tab.open = Open::Nothing;
            Vec::new()
        }
        BoxOutcome::Moved | BoxOutcome::Searched | BoxOutcome::Ignored | BoxOutcome::Outside => {
            Vec::new()
        }
    }
}

/// The keys of a value being typed: a character or Backspace edits it,
/// Enter keeps it as one change, and Esc drops it. A value vtium.conf
/// cannot hold keeps the entry open and says why.
fn typing(tab: &mut Settings, _shared: &Shared, action: &Action) -> Vec<Command> {
    let Some(entry) = tab.table.entry(tab.key) else {
        tab.open = Open::Nothing;
        return Vec::new();
    };
    let key = entry.key;
    let Open::Typing(text) = &mut tab.open else {
        return Vec::new();
    };
    match action.row {
        "enter.typing.settings" => {
            let v = value::typed_value(key, text);
            if let Some(why) = value::refusal(key, v.as_deref()) {
                return vec![value::say(why)];
            }
            tab.open = Open::Nothing;
            vec![value::set(key, v)]
        }
        "esc.typing" => {
            tab.open = Open::Nothing;
            Vec::new()
        }
        "backspace.typing" => {
            text.pop();
            Vec::new()
        }
        "char.typing" | "q.typing" => {
            if let Some(c) = pressed(action).and_then(|k| k.typed()) {
                text.push(c);
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}
