//! History: the versions installed before. Picking one offers View
//! changes, the version against the open document in Show changes' form,
//! which reads only ([`view`]), and Use this version, which loads the
//! version into the document as one change ([`use_version`]), so Undo
//! takes it back and Save makes it real. A version holds vtium.conf
//! alone, so using one leaves the font set changes and the active fonts as
//! they are.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": History is the version history, and its sheet offers View changes and Use this version and counts nothing.
//! - ADR-1403 D8, "A draft's font set changes": the installed history holds vtium.conf alone.

use crate::document::{Change, Document, Edit, Version};

use super::diff::{Diff, diff};
use super::words;

/// View changes: what using `version` would change in `document`, the
/// document's lines that would go as removed and the version's that would
/// come as added. The diff is a value the screen draws; nothing in it
/// edits the document.
pub fn view(document: &Document, version: &Version) -> Diff {
    diff(document.text(), &version.text)
}

/// Use this version: `version`'s vtium.conf in place of the document's,
/// as one change named for when the version was replaced.
pub fn use_version(version: &Version) -> Change {
    Change::new(words::version(version), Edit::Text(version.text.clone()))
}
