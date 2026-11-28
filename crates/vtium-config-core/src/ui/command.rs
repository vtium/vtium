//! What a tab's update returns: [`Command`]s of three kinds, which update
//! performs in one place once the tab's update returns. Effects go to the
//! loop, changes go to the shared state, and requests go to the navigation
//! module. So no tab changes the shared state, the navigation state or the
//! terminal itself.
//!
//! References:
//! - ADR-1206 D2: a tab's update returns effects, changes and requests, and update applies the changes, then the requests, then returns the effects.

use std::path::PathBuf;
use std::time::Duration;

use super::Action;
use crate::document;

/// One thing a tab's update asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// An effect for the loop to perform.
    Effect(Effect),
    /// A change to the shared state.
    Change(Change),
    /// A request to the navigation module.
    Request(Request),
}

/// One thing the loop does: the only place that writes to the terminal,
/// runs the privileged step, asks the font helper or reads the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Bytes to write to the terminal.
    Write(Vec<u8>),
    /// Starts a wait, which ends in a timeout unless another wait replaces
    /// it.
    Wait(Duration),
    /// Runs the privileged step that installs the plan: Save.
    Install(crate::draft::Plan),
    /// Runs the privileged step that installs the font file the desk
    /// readied as a system font.
    InstallFont,
    /// Draws the detailed specimen of the font set `font` at `height`
    /// pixels, at most `max` pixels, its summary counted on the display
    /// `display`.
    Specimen {
        font: String,
        height: u32,
        max: (u32, u32),
        display: (u32, u32),
    },
    /// Asks the font helper for the font set `font`'s character cell at
    /// `height` pixels.
    Cell { font: String, height: u32 },
    /// Draws the picture of the console the shot names.
    Picture(crate::specimen::Shot),
    /// Draws the TUI's own screen at another size into the shot, then the
    /// picture of it. The loop draws the screen, because update never
    /// draws.
    ScreenPicture(crate::specimen::Shot),
    /// Summarizes the specimen of the font set `font` at `height` pixels,
    /// for a terminal that shows no pictures.
    Summary { font: String, height: u32 },
    /// Asks the font helper which sources each of these font sets lacks.
    Sources(Vec<String>),
    /// Asks the font helper what each of these font files is.
    Describe(Vec<PathBuf>),
    /// Hands the op to the desk.
    Desk(Op),
    /// Edit as text: runs the person's editor on a copy of `text`, the
    /// configuration `name`'s vtium.conf, while the TUI steps aside, then
    /// checks the copy with `vtium check-config`.
    Editor { name: String, text: String },
    /// Leaves, with this exit status.
    Exit(u8),
}

/// Something the desk does apart from writing the draft, which every
/// change does through the one change function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Checks the open document with `vtium check-config` and lists what
    /// fails.
    Check,
    /// Readies a save: the plan the privileged step installs, or why the
    /// document cannot be saved.
    Prepare,
    /// Reads the devices present again, and the connected displays'
    /// densities.
    Devices,
    /// Lists the versions installed before.
    Versions,
    /// Readies the install of the font file at this path, one of the
    /// person's own, as a system font.
    SystemFont(PathBuf),
}

/// A change to the shared state, which update makes in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A change to the open document, which the one change function makes,
    /// writes into the draft and records as one step of the undo history.
    Document(document::Change),
    /// Takes the last change to the document back.
    Undo,
    /// Makes again the change the last undo took back.
    Redo,
    /// Puts this message on the message line.
    Say(super::Message),
    /// Clears the message line.
    Quiet,
    /// Tries this font set on the live console.
    Try(String),
    /// Tries the look of this name on the live console: its font set, its
    /// color scheme and its size.
    TryLook(String),
    /// Tries the size `points` on the live console's grid, measured in the
    /// font set `font`, or the font the console draws where it is `None`.
    TrySize { font: Option<String>, points: u32 },
}

/// A request to the navigation module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Opens a level of the interface tree over the tab, by the node's id,
    /// as `looks.card`.
    Open(&'static str),
    /// Closes the level open at the top.
    Close,
    /// Asks this question, and on a yes hands the tab `then`.
    Ask { question: String, then: Action },
    /// Goes to the tab `tab`, by its node's id, and highlights its item
    /// named `name`, as a look's font set is shown in the Fonts tab.
    Show { tab: &'static str, name: String },
}
