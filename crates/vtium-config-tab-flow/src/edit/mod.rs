//! The Flow tab's editing: adding NEW PATH to the configuration and
//! revising a path ([`new_path`]), a terminal's card ([`card`]), the part
//! makers ([`makers`]) and their pickers ([`pickers`]), and a part's label
//! and narrowing choices ([`labels`]). [`plan`] turns the paths back into
//! vtium.conf's edits.
//!
//! Each change to the open configuration is one command, a change to the
//! document that the shell makes through its one change function and that
//! Undo takes back whole. What NEW PATH holds is the tab's own state, so
//! picking parts, revising a path and changing NEW PATH's card change no
//! document until `a` adds the path.
//!
//! References:
//! - ADR-1403 D7, "Composing and adding a path", "Templates and the part
//!   makers", "Labels" and "Editing an added path, and Revise".
//! - ADR-1403 D8, "What the Flow view writes": the edits each change makes.
//! - ADR-1206 D2: a tab changes the document only by returning commands.

pub mod card;
pub mod labels;
pub mod makers;
pub mod new_path;
pub mod pickers;
pub mod plan;
pub mod words;

use vtium_config_core::document::{self, ConfEdit, Edit};
use vtium_config_core::ui::{Action, Carries, Change, Command, Shared};
use vtium_config_widgets::input::{Button, Key};
use vtium_config_widgets::keys;

/// The one change to the document that makes `edits`, named `name` as
/// Undo and the File menu name it, or why the document cannot take them,
/// in one sentence. The tab asks first, so that what it does after a
/// change that is made, such as clearing NEW PATH, does not happen to a
/// change the document refuses.
pub fn change(shared: &Shared, name: String, edits: Vec<ConfEdit>) -> Result<Command, String> {
    document::conf::apply_all(shared.document().text(), &edits).map_err(|f| f.text().to_owned())?;
    Ok(Command::Change(Change::Document(document::Change::new(
        name,
        Edit::Conf(edits),
    ))))
}

/// The commands of a change and the message after it: the change when
/// there is one, and the message either way. A refused change says why
/// instead.
pub fn changed(
    shared: &Shared,
    name: String,
    edits: Vec<ConfEdit>,
    said: String,
) -> Result<Vec<Command>, Vec<Command>> {
    if edits.is_empty() {
        return Ok(crate::say(said));
    }
    match change(shared, name, edits) {
        Ok(c) => {
            let mut out = vec![c];
            out.extend(crate::say(said));
            Ok(out)
        }
        Err(why) => Err(crate::say(why)),
    }
}

/// The verb of an action's row, its first part, as `enter` in
/// `enter.layer.flow-card`.
pub fn verb(action: &Action) -> &str {
    action.row.split('.').next().unwrap_or("")
}

/// Whether the action is Enter on a level: its own row, or one of the
/// rows a node gives Enter on a line that opens a list.
pub fn is_enter(action: &Action) -> bool {
    matches!(verb(action), "enter" | "list")
}

/// The key an action stands for, for the list box: the key it carries,
/// or the key its row names, for an action the shell sends with its row
/// alone.
pub fn pressed(action: &Action) -> Option<keys::Key> {
    use keys::Code;
    if let Carries::Key(k) = &action.carries {
        let code = match k {
            Key::Up => Code::Up,
            Key::Down => Code::Down,
            Key::PageUp => Code::PageUp,
            Key::PageDown => Code::PageDown,
            Key::Home => Code::Home,
            Key::End => Code::End,
            Key::Enter => Code::Enter,
            Key::Escape => Code::Escape,
            Key::Char(c) => Code::Char(*c),
            _ => return None,
        };
        return Some(keys::Key::plain(code));
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
        "question" => Code::Char('?'),
        _ => return None,
    };
    Some(keys::Key::plain(code))
}

/// The screen cell a left click landed on, where the action carries one.
pub fn clicked(action: &Action) -> Option<(usize, usize)> {
    match &action.carries {
        Carries::Key(Key::Mouse(m)) if m.button == Button::Press(0) => Some((m.col, m.row)),
        _ => None,
    }
}

/// What a key did to a value being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    /// A character went in or came out.
    Edited,
    /// Enter: the value is to be kept.
    Keep,
    /// Esc: the value is dropped.
    Cancel,
    /// The key does nothing while a value is typed.
    Ignored,
}

/// Acts on `action` for the value `text` being typed, which holds at most
/// `max` characters. A control character never goes in, since vtium.conf
/// holds a value on one line.
pub fn typing(text: &mut String, action: &Action, max: usize) -> Typed {
    match action.row {
        "enter.typing" => Typed::Keep,
        "esc.typing" => Typed::Cancel,
        "backspace.typing" => {
            text.pop();
            Typed::Edited
        }
        "char.typing" => match &action.carries {
            Carries::Key(Key::Char(c)) if !c.is_control() && text.chars().count() < max => {
                text.push(*c);
                Typed::Edited
            }
            _ => Typed::Ignored,
        },
        _ => Typed::Ignored,
    }
}

/// The text of a value being typed as its line shows it: the value and
/// the cursor after it, cut from the left so the cursor shows in `room`
/// cells.
pub fn entry(text: &str, room: usize) -> String {
    use vtium_config_widgets::canvas::cells;
    let shown = format!("{text}_");
    let mut chars: Vec<char> = shown.chars().collect();
    while chars.len() > 1 && cells(&chars.iter().collect::<String>()) > room.max(1) {
        chars.remove(0);
    }
    chars.into_iter().collect()
}
