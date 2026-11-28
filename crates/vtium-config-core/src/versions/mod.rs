//! The versions of the document: the installed file it started from, the
//! installed file as it is now, and the versions installed before. These
//! are the functions behind Save's screen for a file changed on disk,
//! File › Revert to saved, File › History and File › Show changes; the
//! shell's screens call them.
//!
//! - [`base`] tells whether the installed file changed on disk since the
//!   document's base, and finds a base's text in the installed history.
//! - [`merge`] is Update's three-way merge, with the places both sides
//!   changed for the person to settle one at a time.
//! - [`overwrite`], [`revert`] and [`history`] build Overwrite's, Revert
//!   to saved's and Use this version's changes, and History's View
//!   changes.
//! - [`diff`] gives the hunks Show changes shows, with the words that
//!   changed inside a line.
//! - [`command`] holds the command line's forms: `vtium config update`,
//!   `install --overwrite` and `draft rename`.
//!
//! Every change built here is a [`crate::document::Change`], so it passes
//! the document's one change function and is one step of the undo
//! history, which keeps the base from before it.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": Update, Overwrite and Revert, and the command line's forms of them.
//! - ADR-1403 D11, "One flow for every change": Revert to saved and History.
//! - ADR-1403 D11, "Show changes and Edit as text": the diff.
//! - ADR-1403 D11, "Undo and redo": Update, Overwrite, Revert, Revert to saved and a version from History are one step each.

pub mod base;
pub mod command;
pub mod diff;
pub mod history;
pub mod merge;
pub mod overwrite;
pub mod revert;

/// The words of the versions' changes, which the message line, the File
/// menu and the command line show.
pub mod words {
    use crate::document::Version;

    use super::merge::Both;
    use super::revert::Lost;

    /// Update's step.
    pub const UPDATE: &str = "Update from the installed file";

    /// The step that brings in an installed file no change of the
    /// person's meets.
    pub const TAKEN: &str = "the installed file's change";

    /// Overwrite's step.
    pub const OVERWRITE: &str = "Overwrite";

    /// Revert's step, on the screen for a file changed on disk and in the
    /// File menu.
    pub const REVERT: &str = "Revert to saved";

    /// Why Update is greyed out for a draft whose base vtium-config knows
    /// only by its hash.
    pub const NO_BASE_TEXT: &str = "vtium-config does not have the file this draft was made from, so it cannot merge the change made on disk.";

    /// The step of a version from History, named for when it was replaced.
    pub fn version(version: &Version) -> String {
        format!(
            "the version of {} UTC",
            crate::report::utc_text(version.when)
        )
    }

    /// Revert to saved's question.
    pub const REVERT_QUESTION: &str = "Drop every change since the last save?";

    /// What Revert to saved drops, as its question counts it.
    pub fn lost(lost: &Lost) -> String {
        let changes = match lost.changes {
            1 => "1 change is lost".to_owned(),
            n => format!("{n} changes are lost"),
        };
        let mut parts = Vec::new();
        if lost.added > 0 || lost.removed > 0 {
            parts.push(crate::drafts::lines(lost.added, lost.removed));
        }
        if lost.font_sets > 0 {
            parts.push(crate::drafts::frit_count(lost.font_sets));
        }
        if lost.active > 0 {
            parts.push(match lost.active {
                1 => "1 change to the active fonts".to_owned(),
                n => format!("{n} changes to the active fonts"),
            });
        }
        match parts.split_last() {
            None => format!("{changes}."),
            Some((last, [])) => format!("{changes}: {last}."),
            Some((last, rest)) => format!("{changes}: {}, and {last}.", rest.join(", ")),
        }
    }

    /// `update` with no installed file.
    pub fn nothing_missing(installed: &dyn std::fmt::Display) -> String {
        format!("Nothing to update: {installed} is missing, so nothing changed on disk.")
    }

    /// `update` when the installed file is the draft's base.
    pub fn nothing_changed(installed: &dyn std::fmt::Display, name: &str) -> String {
        format!(
            "Nothing to update: {installed} has not changed since draft {name} was made from it."
        )
    }

    /// `update` after it merged.
    pub fn updated(
        name: &str,
        installed: &dyn std::fmt::Display,
        added: usize,
        removed: usize,
    ) -> String {
        format!(
            "updated draft {name} with the change made to {installed}: {}",
            crate::drafts::lines(added, removed)
        )
    }

    /// Why `update` changed nothing, for a draft whose base vtium-config
    /// knows only by its hash.
    pub fn cannot_merge(why: &str, name: &str) -> String {
        format!(
            "Nothing changed in draft {name}: {why} Run vtium config install --overwrite to install the draft over it, or vtium config revert to start again from the installed file."
        )
    }

    /// Why `update` changed nothing, where both sides changed the same
    /// lines.
    pub fn both_changed(places: usize, name: &str, installed: &dyn std::fmt::Display) -> String {
        let places = match places {
            1 => "1 place".to_owned(),
            n => format!("{n} places"),
        };
        format!(
            "Nothing changed in draft {name}: it and {installed} changed the same lines in {places}, listed above. Settle them in vtium config tui, or edit the draft with vtium config edit."
        )
    }

    /// One place both sides changed, as `update` lists it: where it is,
    /// then the draft's lines and the installed file's.
    pub fn both_lines(both: &Both, name: &str, installed: &dyn std::fmt::Display) -> String {
        let mut out = format!(
            "draft {name} line {} and {installed} line {}:\n",
            both.mine_line, both.installed_line
        );
        let side = |out: &mut String, who: &str, text: &str| {
            if text.is_empty() {
                out.push_str(&format!("  {who:<9}  (no lines)\n"));
            }
            for line in text.lines() {
                out.push_str(&format!("  {who:<9}  {line}\n"));
            }
        };
        side(&mut out, "draft", &both.mine);
        side(&mut out, "installed", &both.installed);
        out
    }

    /// Why a draft cannot take the name `name`.
    pub fn bad_name(name: &str) -> String {
        format!(
            "{name:?} cannot be a draft's name: a name has 1 to 64 letters, digits, '.', '_', '+' or '-'"
        )
    }

    /// `draft rename` after it renamed.
    pub fn renamed(name: &str, new: &str) -> String {
        format!("renamed draft {name} to {new}")
    }
}
