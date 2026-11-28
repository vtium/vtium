//! Revert to saved: the document becomes the installed file again, with
//! no font set change and no change to the active fonts, as one change, so
//! Ctrl+Z brings back everything it dropped. Its question counts what is
//! lost first ([`lost`]).
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": Revert to saved drops every change since the last save, font set changes included, and asks first, counting what is lost.
//! - ADR-1403 D11, "Undo and redo": Revert to saved is one step.

use crate::document::{Base, Change, Document, Edit};

use super::words;

/// What Revert to saved drops, as its question counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lost {
    /// Every change not saved, the count the top row shows.
    pub changes: usize,
    /// The lines of vtium.conf added and removed against the installed
    /// file.
    pub added: usize,
    pub removed: usize,
    /// The font sets the document adds, replaces or removes.
    pub font_sets: usize,
    /// The resources it activates or deactivates, the active fonts among
    /// them.
    pub active: usize,
}

/// What reverting `document` to the installed file would drop.
pub fn lost(document: &Document) -> Lost {
    let review = document.review();
    let installed = document.installed().text.as_deref().unwrap_or_default();
    let (added, removed) = crate::drafts::counts(installed, document.text());
    Lost {
        changes: review.count(),
        added,
        removed,
        font_sets: review.font_sets.len(),
        active: review.active.len(),
    }
}

/// Revert to saved's change, given `installed`, the installed vtium.conf
/// as it is now or `None`. The document takes that file's text, or the
/// short header a draft starts from when none is installed, drops its font
/// set changes and its changes to the active resources, and takes that
/// file as its base. Revert on the screen for a file changed on disk is
/// this same change, with the file read from disk again.
pub fn revert(installed: Option<&str>) -> Change {
    Change::new(
        words::REVERT,
        Edit::All(vec![
            Edit::Text(installed.map_or_else(|| crate::drafts::HEADER.to_owned(), str::to_owned)),
            Edit::Drop,
            Edit::Base(Base::of(installed)),
        ]),
    )
}
