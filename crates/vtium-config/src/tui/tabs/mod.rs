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

use super::model::Effect;

/// What a tab's update asks for, which update performs once the tab's
/// update returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// An effect for the loop to perform, such as a write to the terminal.
    Effect(Effect),
}
