//! History's View changes: the version on the sheet against the open
//! document, read-only, in Show changes' form, with Changes only and
//! Whole file and the file view's scrolling. Its red lines leave the
//! document if the version is used, and its green ones come with it.
//! Nothing here edits the document, and Esc goes back to the version's
//! sheet.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": View changes shows the
//!   version against the open document in Show changes' form, read-only.

use vtium_config_core::ui::Shared;

use crate::changes::Changes;
use crate::sheets::Sheet;

/// The view of the version whose sheet is on top of `sheets`, where one
/// is.
pub fn of_top(sheets: &[Sheet], shared: &Shared) -> Option<Changes> {
    match sheets.last() {
        Some(Sheet::Version(v)) => Some(Changes::of_version(shared, v.version().clone())),
        _ => None,
    }
}
