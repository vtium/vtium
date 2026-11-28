//! File › Revert to saved…: drops every change since the last save, so the
//! document is the installed file again, font set changes and changes to
//! the active fonts included. It asks first, in one line that counts what
//! is lost, and the change is one step, so Ctrl+Z brings every dropped
//! change back. `r` in the Flow tab opens it too. With nothing to drop, it
//! asks nothing and says so, and the File menu greys the item out.
//!
//! The count and the change come from vtium-config-core's
//! [`vtium_config_core::versions::revert`]; this module asks and routes.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": Revert to saved asks
//!   first, and its question counts what is lost.
//! - ADR-1403 D11, "Undo and redo": Revert to saved is one step.

use vtium_config_core::document::{Change, Document};
use vtium_config_core::versions::revert::{self, Lost};
use vtium_config_core::versions::words as said;

/// The row that opens it: `r` in the Flow tab, which File › Revert to
/// saved… also takes.
pub const OPEN: &str = "r.view.flow";

/// The row a yes to its question hands the shell.
pub const YES: &str = "y.question.revert";

/// What asking to revert `document` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Asks this question, which counts what is lost.
    Question(String),
    /// Nothing to drop: says so.
    Nothing(&'static str),
}

/// The question before `document` is reverted, or why none is asked.
pub fn ask(document: &Document) -> Ask {
    let lost = revert::lost(document);
    if lost.changes == 0 {
        Ask::Nothing(words::NOTHING)
    } else {
        Ask::Question(words::question(&lost))
    }
}

/// The yes: the change that reverts `document` to the installed file it
/// counts against, and the line that says what it did.
pub fn revert(document: &Document) -> (Change, String) {
    let n = document.count();
    (
        revert::revert(document.installed().text.as_deref()),
        words::done(n),
    )
}

/// The words Revert to saved puts on the screen.
pub mod words {
    use super::{Lost, said};

    /// The File menu's reason, and the line, with nothing to drop.
    pub const NOTHING: &str = "Nothing to revert: the configuration is what is saved.";

    /// The File menu's reason beside the greyed item.
    pub const NOTHING_TO_DROP: &str = "nothing to drop";

    /// The question, which counts what is lost.
    pub fn question(lost: &Lost) -> String {
        format!("{} {}", said::REVERT_QUESTION, said::lost(lost))
    }

    /// After the revert of `n` changes.
    pub fn done(n: usize) -> String {
        let back = match n {
            1 => "the change".to_owned(),
            n => format!("the {n} changes"),
        };
        format!(
            "Reverted to saved, as /etc/vtium/vtium.conf holds it. [Ctrl+Z] brings back {back}."
        )
    }
}
