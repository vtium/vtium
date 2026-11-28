//! What each action does to the tab: the moves through the list or
//! among the matches of a kept filter, Enter on an item, `t`, `p`, `w` and
//! Space, a card's moves, steps, list boxes, typed name and Activate, a
//! detail's scroll, the clicks on each, and Find's request to highlight
//! an item. The navigation module hands the tab an action by its row of
//! the key table, so each arm here answers rows of the tab's nodes. The
//! shell keeps the `?` search's row and its keys.
//!
//! References:
//! - ADR-1206 D2: update hands the open tab an action from the key table, and the tab returns commands.
//! - ADR-1403 D11, "The list box", "Search, on ?" and "Fonts the configuration uses but has not activated".
//! - ADR-1201 D4: `t` tries a look or a font set on the live console, and `w` lets programs resize the grid after a question.

use ratatui_core::layout::Position;
use vtium_config_core::document::{Change, ConfEdit, Edit};
use vtium_config_core::ui::{
    Action, Carries, Change as UiChange, Command, Live, Message, Request, Shared,
};
use vtium_config_widgets::input::{self, Button};
use vtium_config_widgets::keys::{Code, Key, Mods};
use vtium_config_widgets::list_box::{ListBox, Outcome as BoxOutcome};

use crate::card::{self, Line};
use crate::draw::Panes;
use crate::list::{self, Item};
use crate::{Card, CardOpen, Looks, NODE_CARD_LIST, Open, missing, resources, view, words};

/// The step of Home and End: past either end of any list.
const FAR: isize = isize::MAX / 2;

/// The row whose action, carrying this item, is the yes to the question
/// of letting programs resize the grid.
const WINDOW_OPS: &str = "window-ops.view.looks";

/// What `WINDOW_OPS` carries when the person answered yes.
const ANSWERED_YES: usize = 1;

/// Acts on `action` and returns the commands it makes.
pub fn update(tab: &mut Looks, shared: &Shared, action: Action) -> Vec<Command> {
    if let (true, Carries::Item(at)) = (action.is("enter.typing.find"), &action.carries) {
        go_to(tab, shared, *at);
        return Vec::new();
    }
    if action.is(WINDOW_OPS) && action.carries == Carries::Item(ANSWERED_YES) {
        return window_ops_on();
    }
    let out = match &tab.open {
        Open::Nothing => browse(tab, shared, &action),
        Open::Card(_) => card_action(tab, shared, &action),
        Open::Detail { .. } => {
            detail(tab, shared, &action);
            Vec::new()
        }
    };
    tab.offset = view::settled_offset(tab, shared, tab.seen.get());
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
        _ => return None,
    };
    Some(Key::plain(code))
}

/// The rebuilt TUI's key for the older decoder's, which the action
/// carries.
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

/// A line on the message line.
fn say(line: impl Into<String>) -> Vec<Command> {
    vec![Command::Change(UiChange::Say(Message {
        line: line.into(),
        more: None,
    }))]
}

/// The keys of the list with nothing open over it.
fn browse(tab: &mut Looks, shared: &Shared, action: &Action) -> Vec<Command> {
    if let Some(at) = click(action) {
        return clicked_list(tab, shared, at);
    }
    let item = tab.highlighted(shared);
    match (verb(action), action.row, &item) {
        ("up", ..) => step(tab, shared, -1),
        ("down", ..) => step(tab, shared, 1),
        ("home", ..) => step(tab, shared, -FAR),
        ("end", ..) => step(tab, shared, FAR),
        ("pgup", ..) => step(tab, shared, -page(tab)),
        ("pgdn", ..) => step(tab, shared, page(tab)),
        ("enter", ..) => return open(tab, item),
        (_, "space.view.resources", Some(i)) => return resources::space(shared, i),
        (_, "t.view.looks", Some(i)) => return try_live(tab, shared, i),
        (_, "p.view.looks", Some(Item::Look(_) | Item::FontSet(_))) => {
            tab.picture = !tab.picture;
        }
        (_, WINDOW_OPS, Some(Item::Look(_) | Item::NoLooks | Item::FontSet(_))) => {
            return window_ops(shared);
        }
        _ => {}
    }
    Vec::new()
}

/// The rows a page holds in the list.
fn page(tab: &Looks) -> isize {
    isize::try_from(Panes::of(tab.seen.get()).list.height.max(1)).unwrap_or(1)
}

/// Moves the highlight `by` items, among those the filter keeps where
/// there is one, and stops at the ends.
fn step(tab: &mut Looks, shared: &Shared, by: isize) {
    if let Some(filter) = tab.filter.as_mut() {
        filter.step(by);
        if let Some(h) = filter.highlighted() {
            tab.highlight = h;
        }
        return;
    }
    let last = list::items(shared).len().saturating_sub(1);
    tab.highlight = tab.highlight.saturating_add_signed(by).min(last);
}

/// Enter on `item`: a look's card, or a detail.
fn open(tab: &mut Looks, item: Option<Item>) -> Vec<Command> {
    match item {
        Some(Item::Look(name)) => {
            tab.open = Open::Card(Card {
                name,
                row: 0,
                open: CardOpen::Nothing,
            });
            Vec::new()
        }
        Some(Item::NoLooks) => say(words::NO_LOOK_TO_OPEN),
        Some(item) => {
            tab.open = Open::Detail { item, top: 0 };
            Vec::new()
        }
        None => Vec::new(),
    }
}

/// A click on an item's row: the first highlights it, and a click on the
/// highlighted row opens it, as Enter does.
fn clicked_list(tab: &mut Looks, shared: &Shared, at: Position) -> Vec<Command> {
    let places = tab.places(shared);
    let Some(&(_, i)) = places.items.iter().find(|(r, _)| r.contains(at)) else {
        return Vec::new();
    };
    if i == tab.highlight {
        return open(tab, tab.highlighted(shared));
    }
    tab.highlight = i;
    if let Some(filter) = tab.filter.as_mut() {
        let place = |x: usize| filter.matches().iter().position(|&m| m == x);
        if let (Some(from), Some(to)) = (filter.highlighted().and_then(place), place(i)) {
            let by = isize::try_from(to).unwrap_or(0) - isize::try_from(from).unwrap_or(0);
            filter.step(by);
        }
    }
    Vec::new()
}

/// Find's Enter on an item: the item is highlighted in the list, with
/// nothing open over it and the filter cleared so the item shows.
fn go_to(tab: &mut Looks, shared: &Shared, at: usize) {
    if at < list::items(shared).len() {
        tab.highlight = at;
        tab.open = Open::Nothing;
        tab.filter = None;
    }
}

/// `t`: tries the look or the font set on the live console, or says why
/// it cannot: the console is not a vtium console that takes fonts, or
/// the font set only the document adds, or it does not read.
///
/// References:
/// - ADR-1403 D8, "The live trial": a font set only the document holds can be tried once it is saved.
fn try_live(tab: &Looks, shared: &Shared, item: &Item) -> Vec<Command> {
    match shared.console.live {
        Live::On => {}
        Live::OpsOff => return say(words::TRY_OPS_OFF),
        Live::NotVtium => return say(words::TRY_NOT_VTIUM),
        Live::Asking => return say(words::TRY_ASKING),
    }
    let font = match item {
        Item::Look(name) => {
            list::look(shared.document().text(), name).and_then(|l| card::font_set(shared, &l))
        }
        Item::FontSet(name) => Some(name.clone()),
        _ => return Vec::new(),
    };
    if let Some(name) = &font
        && let Some(why) = untried(tab, shared, name)
    {
        return say(why);
    }
    match item {
        Item::Look(name) => vec![Command::Change(UiChange::TryLook(name.clone()))],
        _ => font
            .map(|f| vec![Command::Change(UiChange::Try(f))])
            .unwrap_or_default(),
    }
}

/// Why the console cannot try the font set `name`: only the document adds
/// it, or it does not read. `None` where it can.
fn untried(_tab: &Looks, shared: &Shared, name: &str) -> Option<String> {
    if vtium_font_helper::frit::RESERVED.contains(&name) {
        return None;
    }
    let document = shared.document();
    if document.content().font_sets.kept.contains_key(name)
        && !document.installed().frits.contains_key(name)
    {
        return Some(words::try_after_save(name));
    }
    match crate::sizes::font_set(shared, name) {
        Some(Ok(_)) | None => None,
        Some(Err(_)) => Some(words::try_does_not_read(name)),
    }
}

/// `w`: asks whether to let programs resize the console's grid, or says
/// that the document already lets them.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": `w` asks, and only a yes sets `allow-window-ops`.
fn window_ops(shared: &Shared) -> Vec<Command> {
    let on = vtium_settings::keys::find("allow-window-ops").is_some_and(|k| {
        vtium_config_core::commands::value_text(k, Some(shared.document().text())).0 == "on"
    });
    if on {
        return say(words::WINDOW_OPS_ON);
    }
    vec![Command::Request(Request::Ask {
        question: words::WINDOW_OPS_QUESTION.to_owned(),
        then: Action::item(WINDOW_OPS, ANSWERED_YES),
    })]
}

/// The yes to `w`'s question: `allow-window-ops = on`, as one change.
fn window_ops_on() -> Vec<Command> {
    let key = vtium_settings::keys::find("allow-window-ops").map_or("allow-window-ops", |k| k.name);
    vec![Command::Change(UiChange::Document(Change::new(
        words::WINDOW_OPS_STEP,
        Edit::Conf(vec![ConfEdit::Top {
            key,
            value: Some("on".to_owned()),
        }]),
    )))]
}

/// A detail's keys: Up and Down scroll it, and Esc goes back to the list
/// with the same item highlighted.
fn detail(tab: &mut Looks, shared: &Shared, action: &Action) {
    let last = match &tab.open {
        Open::Detail { item, .. } => {
            let used = tab.used(shared);
            resources::detail(shared, item, &used)
                .map_or(0, |d| view::detail_last_top(&d, tab.seen.get()))
        }
        _ => return,
    };
    if verb(action) == "esc" {
        tab.open = Open::Nothing;
        return;
    }
    let Open::Detail { top, .. } = &mut tab.open else {
        return;
    };
    match verb(action) {
        "up" => *top = top.saturating_sub(1),
        "down" => *top = top.saturating_add(1).min(last),
        _ => {}
    }
}

/// The keys and clicks of an open card.
fn card_action(tab: &mut Looks, shared: &Shared, action: &Action) -> Vec<Command> {
    let Open::Card(c) = &tab.open else {
        return Vec::new();
    };
    let Some(look) = list::look(shared.document().text(), &c.name) else {
        tab.open = Open::Nothing;
        return Vec::new();
    };
    match &c.open {
        CardOpen::Typing(_) => typing(tab, shared, action),
        CardOpen::List { .. } => list_box(tab, shared, &look, action),
        CardOpen::Missing { .. } => missing_view(tab, shared, action),
        CardOpen::Nothing => card_keys(tab, shared, &look, action),
    }
}

/// The card's own keys: Up and Down between its rows, Left and Right
/// step a line, Enter opens a line's list box, types the name or opens
/// what the look misses, `A` activates its fonts, `t` tries it, and Esc
/// goes back to the list with the look highlighted.
fn card_keys(tab: &mut Looks, shared: &Shared, look: &list::Look, action: &Action) -> Vec<Command> {
    if let Some(at) = click(action) {
        return clicked_card(tab, shared, look, at);
    }
    let roles = missing::roles(&tab.used(shared), &look.name).len();
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    let last = Line::ALL.len().saturating_add(roles).saturating_sub(1);
    match action.row {
        "up.layer.look" => c.row = c.row.saturating_sub(1),
        "down.layer.look" => c.row = c.row.saturating_add(1).min(last),
        "left.layer.look" | "right.layer.look" => {
            let by = if action.row.starts_with("left") {
                -1
            } else {
                1
            };
            return c
                .line()
                .map(|line| card::step(shared, look, line, by))
                .unwrap_or_default();
        }
        "esc.layer.look" => {
            let name = c.name.clone();
            tab.open = Open::Nothing;
            if let Some(at) = list::items(shared)
                .iter()
                .position(|i| *i == Item::Look(name.clone()))
            {
                tab.highlight = at;
            }
        }
        "shift-a.layer.look" => return missing::activate(&tab.used(shared), &look.name),
        "t.layer.look" => return try_live(tab, shared, &Item::Look(look.name.clone())),
        row if verb(action) == "enter" || row.starts_with("list.") => {
            return enter_card(tab, shared, look);
        }
        _ => {}
    }
    Vec::new()
}

/// Enter on the card's highlighted row.
fn enter_card(tab: &mut Looks, shared: &Shared, look: &list::Look) -> Vec<Command> {
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    match c.line() {
        Some(Line::Name) => c.open = CardOpen::Typing(look.name.clone()),
        Some(line) => {
            let choices = card::choices(shared, look, line);
            c.open = CardOpen::List {
                list: Box::new(ListBox::open(NODE_CARD_LIST, &choices)),
                line,
            };
        }
        None => c.open = CardOpen::Missing { top: 0 },
    }
    Vec::new()
}

/// A click on a card's row: the first highlights it, and a click on the
/// highlighted row acts as Enter does.
fn clicked_card(tab: &mut Looks, shared: &Shared, look: &list::Look, at: Position) -> Vec<Command> {
    let places = tab.places(shared);
    let Some(&(_, row)) = places.card.iter().find(|(r, _)| r.contains(at)) else {
        return Vec::new();
    };
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    if c.row == row {
        return enter_card(tab, shared, look);
    }
    c.row = row;
    Vec::new()
}

/// The keys and clicks of a line's list box: a kept pick is one change,
/// and Esc closes the box unchanged.
fn list_box(tab: &mut Looks, shared: &Shared, look: &list::Look, action: &Action) -> Vec<Command> {
    let area = tab.seen.get();
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    let CardOpen::List { list, line } = &mut c.open else {
        return Vec::new();
    };
    let line = *line;
    let choices = card::choices(shared, look, line);
    let probe = Card {
        name: c.name.clone(),
        row: c.row,
        open: CardOpen::Nothing,
    };
    let rect = view::list_box_rect(&probe, list, &choices, area);
    let outcome = match (click(action), pressed(action)) {
        (Some(at), _) => list.click(at, &choices, rect),
        (None, Some(key)) => list.key(&key, &choices, rect),
        (None, None) => BoxOutcome::Ignored,
    };
    match outcome {
        BoxOutcome::Kept(value) => {
            c.open = CardOpen::Nothing;
            card::pick(&look.name, line, value)
        }
        BoxOutcome::Closed => {
            c.open = CardOpen::Nothing;
            Vec::new()
        }
        BoxOutcome::Moved | BoxOutcome::Searched | BoxOutcome::Ignored | BoxOutcome::Outside => {
            Vec::new()
        }
    }
}

/// The keys of the look's name being typed: a character or Backspace
/// edits it, Enter renames the look as one change, and Esc keeps the name
/// it had. A name the document refuses stays typed, and the message line
/// says why.
fn typing(tab: &mut Looks, shared: &Shared, action: &Action) -> Vec<Command> {
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    let CardOpen::Typing(text) = &mut c.open else {
        return Vec::new();
    };
    match action.row {
        "enter.typing" => match card::rename(shared, &c.name, text) {
            Ok(out) => {
                c.name = text.trim().to_owned();
                c.open = CardOpen::Nothing;
                out
            }
            Err(why) => say(why),
        },
        "esc.typing" => {
            c.open = CardOpen::Nothing;
            Vec::new()
        }
        "backspace.typing" => {
            text.pop();
            Vec::new()
        }
        "char.typing" | "q.typing" => {
            if let Some(ch) = pressed(action).and_then(|k| k.typed()) {
                text.push(ch);
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The keys of what the look still misses: Up and Down scroll it, `A`
/// activates what can be activated, and Esc goes back to the card.
fn missing_view(tab: &mut Looks, shared: &Shared, action: &Action) -> Vec<Command> {
    let name = match &tab.open {
        Open::Card(c) => c.name.clone(),
        _ => return Vec::new(),
    };
    let width = usize::from(tab.seen.get().width).saturating_sub(6);
    let rows = view::misses_lines(tab, shared, &name, width).len();
    let room = usize::from(tab.seen.get().height.saturating_sub(2));
    let last = rows.saturating_sub(room);
    if action.row == "shift-a.layer.look" {
        return missing::activate(&tab.used(shared), &name);
    }
    let Open::Card(c) = &mut tab.open else {
        return Vec::new();
    };
    let CardOpen::Missing { top } = &mut c.open else {
        return Vec::new();
    };
    match action.row {
        "esc.layer.look" => c.open = CardOpen::Nothing,
        "up.layer.look" => *top = top.saturating_sub(1),
        "down.layer.look" => *top = top.saturating_add(1).min(last),
        _ => {}
    }
    Vec::new()
}
