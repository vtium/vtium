#![forbid(unsafe_code)]
// The terminal's bytes are untrusted input, and no key, click or reply
// may panic the TUI.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tui: the shell of vtium-config's TUI.
//!
//! The binary builds the six tabs, each from a crate of its own, and hands
//! them to [`run`] as trait objects in the tabs' order, so this crate
//! names no tab crate. The shell holds them in [`app::App`], beside the
//! shared state every tab reads and the navigation state that says where
//! the person is.
//!
//! [`app`] holds the model and its one update, which hands each key to
//! [`nav`], the one navigation module, and routes the action that comes
//! back to the open tab or to the shell's own handler. [`view`] draws the
//! frame around the open tab, with an open menu over it. [`menus`] holds
//! the File and Actions menus' items. [`port`] is the loop around the App
//! and the terminal it reads and writes.
//!
//! The modules [`live`], [`sheets`], [`save`], [`changes`], [`history`],
//! [`editor`], [`revert`] and [`player`] are filled by the cards after
//! this one.
//!
//! References:
//! - ADR-1206 D9: the shell is a crate of its own and holds the tabs as
//!   trait objects that the binary hands it.
//! - ADR-1206 D2: the model, the message, the one update and the view.

pub mod app;
pub mod changes;
pub mod editor;
pub mod history;
pub mod live;
pub mod menus;
pub mod nav;
pub mod player;
pub mod port;
pub mod revert;
pub mod save;
pub mod sheets;
pub mod view;

use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_widgets::tab::Tab;

/// A tab as the shell holds it: a trait object over the shared state, the
/// action and the commands of vtium-config-core.
pub type BoxTab = Box<dyn Tab<Shared, Action, Command>>;

/// Runs the TUI on the terminal of standard input and output, with `tabs`
/// in the top row's order and `shared` as the state every tab reads, until
/// the person quits. Returns the exit status. Once the terminal is back as
/// it was, prints the one line that says how many changes wait unsaved.
///
/// References:
/// - ADR-1403 D11, "Quitting, Ctrl-C and the signals": one line on leaving
///   while changes are not saved.
pub fn run(tabs: Vec<BoxTab>, shared: Shared, color: bool) -> std::io::Result<u8> {
    let left = {
        let mut tty = port::tty::Tty::open()?;
        let size = port::Port::size(&mut tty);
        let app = app::App::new(tabs, shared, size, color);
        port::drive(&mut tty, app)?
    };
    if let Some(line) = left.line {
        println!("{line}");
    }
    Ok(left.status)
}
