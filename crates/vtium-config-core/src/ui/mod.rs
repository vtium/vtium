//! The types that vtium-config-widgets' `Tab` trait names through its
//! type parameters: the shared state every tab reads ([`Shared`]), the
//! action the navigation module hands the open tab ([`Action`]), and the
//! commands a tab's update returns ([`Command`]). The shell and every tab
//! crate depend on this crate for them, and no tab crate depends on
//! another.
//!
//! References:
//! - ADR-1206 D9, "The types the trait names": the shared state, the action and the commands are defined below the tab crates, beside the document the shared state holds.
//! - ADR-1206 D2: the model, the message, update and a tab's commands.

pub mod action;
pub mod command;
pub mod shared;

pub use action::{Action, Carries};
pub use command::{Change, Command, Effect, Op, Request};
pub use shared::{Asker, Askers, Console, Installs, Live, Message, Pictures, Query, Shared};
