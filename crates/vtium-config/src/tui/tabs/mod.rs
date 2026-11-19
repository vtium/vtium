//! The TUI's tabs, each a module of its own that holds the tab's state and
//! its update. A tab's update takes the tab's state mutably, the model's
//! shared part read-only, and the key that the navigation module hands to
//! the open tab. It returns [`Command`]s, which update performs in one
//! place: effects go to the loop, changes go to the shared part, and
//! requests go to the navigation module. So no tab changes the shared
//! part or another tab's state itself.
//!
//! References:
//! - ADR-1206 D2: the model's four parts, and the three kinds of command a
//!   tab's update returns.
//! - ADR-1206 D4: a tab's update calls its own state, the shared part
//!   read-only and words.rs, and never another tab's module.

pub mod help;
pub mod settings;

use super::input::Key;
use super::model::{Effect, TYPED_MAX};

/// What a tab's update asks for, which update performs once the tab's
/// update returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// An effect for the loop to perform, such as a write to the terminal.
    Effect(Effect),
    /// A change to the shared part of the model.
    Change(Change),
}

/// A change to the shared part of the model, which update makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Sets the top-level key `key` to `value` in the draft, or leaves it
    /// at its default with `None`.
    Top {
        key: &'static str,
        value: Option<String>,
    },
}

/// A character or Backspace typed into `text`, the value being edited. A
/// control character does nothing, and `text` holds at most
/// [`TYPED_MAX`] bytes.
pub fn type_into(text: &mut String, key: Key) {
    match key {
        Key::Char(c) if !c.is_control() && text.len() < TYPED_MAX => text.push(c),
        Key::Backspace => {
            text.pop();
        }
        _ => {}
    }
}
