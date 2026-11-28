//! Overwrite: the person saves the document knowingly over a change made
//! to the installed file on disk. On the screen for a file changed on
//! disk, [`overwrite`] records the installed file the review showed as the
//! base, as one step, and the Save that follows installs the document over
//! it. The save keeps the file it replaces in the installed history, as
//! every save does, so File › History brings it back. The privileged step
//! still refuses a file that changes again after the review showed it.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": Overwrite, on `[o]`, the replaced file kept in History, and the base each choice records.

use crate::document::{Base, Change, Edit};

use super::words;

/// Overwrite's change: `installed`, the installed vtium.conf as the review
/// showed it or `None`, recorded as the base.
pub fn overwrite(installed: Option<&str>) -> Change {
    Change::new(words::OVERWRITE, Edit::Base(Base::of(installed)))
}
