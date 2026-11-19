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

pub mod flow;
pub mod fonts;
pub mod help;
pub mod looks;
pub mod settings;

use super::edit::Change as DraftChange;
use super::flow::Item;
use super::input::Key;
use super::model::{Effect, Pending, TYPED_MAX, Then, View};

/// What a tab's update asks for, which update performs once the tab's
/// update returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// An effect for the loop to perform, such as a write to the terminal.
    Effect(Effect),
    /// A change to the shared part of the model.
    Change(Change),
    /// A request to the navigation module.
    Request(Request),
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
    /// Puts this line on the message line.
    Say(String),
    /// Replaces the frits that wait for the install with these.
    Pending(Pending),
    /// Saves the font the console shows, which the live trial put there.
    SaveTried,
    /// Tries this font on the live console.
    Try(String),
    /// Clears the message line.
    Quiet,
    /// Shows the font at this place in the list of fonts in the previews:
    /// the Fonts tab's highlight, which every tab's preview reads from the
    /// shared part.
    Preview(usize),
    /// Makes `changes` on the draft, in order, and hands the result to the
    /// desk to check, then selects `select` in the Flow view once the desk
    /// answers. The tab checks first that the changes can be made, with
    /// [`super::model::Shared::apply`].
    Make {
        changes: Vec<DraftChange>,
        select: Option<Item>,
    },
    /// Writes the unsaved changes into the draft: Save.
    Save,
    /// One of the action keys `i`, `d`, `v`, `r`, `o` and `n`.
    Action(char),
}

/// A request to the navigation module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Goes to this view.
    Go(View),
    /// Asks this question, which does `then` on a yes.
    Ask { question: String, then: Then },
    /// Shows the font set `name` in the Fonts tab, highlighted.
    ShowFont(String),
}

/// Whether a value being typed takes the character `c`. Every tab types
/// through this test, so no value takes a control character, C0 or C1.
/// A value goes into vtium.conf as it is typed, and a C1 control such as
/// U+009B is CSI to a terminal that reads 8-bit controls, so the file
/// would act on the terminal of whoever shows it.
pub fn typable(c: char) -> bool {
    !c.is_control()
}

/// A character or Backspace typed into `text`, the value being edited. A
/// character that [`typable`] refuses does nothing, and `text` holds at
/// most [`TYPED_MAX`] bytes.
pub fn type_into(text: &mut String, key: Key) {
    match key {
        Key::Char(c) if typable(c) && text.len() < TYPED_MAX => text.push(c),
        Key::Backspace => {
            text.pop();
        }
        _ => {}
    }
}
