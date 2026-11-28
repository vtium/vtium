//! File › History: the versions installed before, newest first, as a
//! submenu of File, which `o` in the Flow tab also opens. Choosing one
//! opens its sheet ([`sheet`]), which counts nothing and offers two
//! buttons, each a key as well: `[v] View changes`, the version against
//! the open document in Show changes' form, read-only ([`view`]), and
//! `[a] Use this version`, which loads the version into the document as
//! one change, so Undo takes it back and Save makes it real. With no
//! earlier version, the submenu holds one greyed line that says so.
//!
//! The versions and the change come from vtium-config-core's
//! [`vtium_config_core::versions::history`]; this module routes them.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": History is the version
//!   history, and its sheet offers View changes and Use this version and
//!   counts nothing.
//! - ADR-1403 D11, "Undo and redo": loading a version from History is one
//!   step.

pub mod sheet;
pub mod view;

use vtium_config_core::document::{Change, Version};
use vtium_config_core::ui::Shared;

use crate::menus::{Line, Submenu, Then};
use crate::nav::menus::{Open, Which};

pub use sheet::Sheet;

/// The row of an entry of History's submenu, carrying the entry's place
/// among the versions.
pub const PICK: &str = "enter.layer.menu.history";

/// View changes, on the version's sheet.
pub const VIEW: &str = "v.layer.history-version";

/// Use this version, on the version's sheet.
pub const USE: &str = "a.layer.history-version";

/// `o` in the Flow tab, which opens File › History ›.
pub const MENU: &str = "o.view.flow";

/// The version at `item` among the versions installed before, newest
/// first, or `None` where there is none there.
pub fn pick(shared: &Shared, item: usize) -> Result<Option<Version>, String> {
    Ok(shared.documents.desk().versions()?.get(item).cloned())
}

/// The File menu of `lines`, open on History's submenu, with its first
/// version highlighted. `sub_of` gives a submenu's lines.
pub fn menu(lines: &[Line], sub_of: &dyn Fn(Submenu) -> Vec<Line>) -> Open {
    let mut open = Open::new(Which::File, lines);
    open.highlight = lines.iter().position(|l| {
        l.choice()
            .is_some_and(|c| c.then == Then::Sub(Submenu::History))
    });
    open.right(lines, sub_of);
    open
}

/// Use this version: the change that loads `version` into the document,
/// and the line that says so.
pub fn use_version(version: &Version) -> (Change, String) {
    (
        vtium_config_core::versions::history::use_version(version),
        words::used(&crate::menus::file::when(version.when)),
    )
}

/// The words History puts on the screen.
pub mod words {
    /// The one line of History's submenu with no earlier version.
    pub const NONE: &str = "No earlier version";

    /// Why there is none, beside it.
    pub const NONE_WHY: &str = "no save replaced one yet";

    /// After Use this version.
    pub fn used(when: &str) -> String {
        format!("Using the version of {when}: one change, not saved. [Ctrl+Z] takes it back.")
    }
}
