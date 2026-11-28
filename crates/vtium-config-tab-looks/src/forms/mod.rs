//! The forms and questions of the Looks tab's second half: `n` makes a
//! look ([`new_look`]) or opens the form for a new font set
//! ([`new_frit`]), and `r` marks a font set for removal or asks before it
//! deletes a look ([`remove`]). Space on an active color scheme file or
//! pointer theme that a look uses asks before it deactivates it
//! ([`remove`] too). [`words`] holds what they say.
//!
//! [`update`] sees each action before the list's own keys do. It answers
//! every action while the new font set's form is open, and on the list it
//! answers `n`, `r`, a question's yes, and Space where Space must ask
//! first. It leaves every other action to the list.
//!
//! References:
//! - ADR-1206 D2: the tab's update returns commands, and the shell makes each change in one place.
//! - ADR-1403 D11, "The list box": a role's list opens under its line, as a card line's does.

pub mod new_frit;
pub mod new_look;
pub mod remove;
pub mod words;

use ratatui_core::layout::Position;
use vtium_config_core::ui::{Action, Carries, Change as UiChange, Command, Message, Shared};
use vtium_config_widgets::input::{self, Button};
use vtium_config_widgets::keys::{Code, Key, Mods};
use vtium_config_widgets::search::Search;

use crate::list::Item;
use crate::{Looks, Open, view};

/// Acts on `action` where a form or a question owns it, and returns the
/// commands it makes; `None` where the list's own keys act on it.
pub fn update(tab: &mut Looks, shared: &Shared, action: &Action) -> Option<Vec<Command>> {
    if tab.form.is_some() {
        return Some(new_frit::update(tab, shared, action));
    }
    if let Some(at) = remove::answered(action) {
        if action.is(remove::DELETE) {
            return Some(remove::delete(shared, at));
        }
        if action.is(remove::DEACTIVATE) {
            return Some(remove::deactivate(shared, at));
        }
    }
    if !matches!(tab.open, Open::Nothing) || tab.filter.as_ref().is_some_and(Search::typing) {
        return None;
    }
    let at = tab.highlight;
    let out = match (action.row, tab.highlighted(shared)?) {
        ("n.view.looks", Item::Look(_) | Item::NoLooks) => new_look::make(tab, shared),
        ("n.view.looks", Item::FontSet(_)) => {
            tab.form = Some(new_frit::Form::default());
            Vec::new()
        }
        ("r.view.looks", Item::Look(name)) => remove::ask_delete(shared, &name, at),
        ("r.view.looks", Item::NoLooks) => say(words::NO_LOOK_TO_DELETE),
        ("r.view.looks", Item::FontSet(name)) => remove::font_set(shared, &name),
        ("space.view.resources", item) => remove::ask_deactivate(shared, &item, at)?,
        _ => return None,
    };
    tab.offset = view::settled_offset(tab, shared, tab.seen.get());
    Some(out)
}

/// A line on the message line.
pub fn say(line: impl Into<String>) -> Vec<Command> {
    vec![Command::Change(UiChange::Say(Message {
        line: line.into(),
        more: None,
    }))]
}

/// The verb of a row's id, its first part, as `up` in `up.layer.list`.
pub fn verb(action: &Action) -> &str {
    action.row.split('.').next().unwrap_or("")
}

/// The key an action stands for: the key it carries, or the key its row
/// names, for an action the shell sends with nothing but its row.
pub fn pressed(action: &Action) -> Option<Key> {
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

/// The rebuilt TUI's key for the older decoder's, which an action
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
pub fn click(action: &Action) -> Option<Position> {
    match &action.carries {
        Carries::Key(input::Key::Mouse(m)) if m.button == Button::Press(0) => Some(Position::new(
            u16::try_from(m.col).unwrap_or(u16::MAX),
            u16::try_from(m.row).unwrap_or(u16::MAX),
        )),
        _ => None,
    }
}
