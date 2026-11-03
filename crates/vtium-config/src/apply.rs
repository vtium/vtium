//! `apply DIR` (ADR-1201 D5): the privileged step that checks a draft and
//! installs it, and the TUI's side of it, which writes the draft and runs
//! `sudo vtium-config apply DIR`.

use std::ffi::OsString;
use std::io;

use crate::draft::Plan;
use crate::tui::Paths;

/// What the TUI calls to save: writes `plan`'s draft and runs the
/// privileged step through `run`.
pub type Saver =
    Box<dyn FnMut(&Plan, &mut dyn FnMut(&[OsString]) -> io::Result<bool>) -> Result<usize, String>>;

/// The TUI's saver for `paths`.
pub fn saver(paths: Paths) -> Saver {
    let _ = paths;
    Box::new(|_, _| Err("saving is not built yet".to_owned()))
}
