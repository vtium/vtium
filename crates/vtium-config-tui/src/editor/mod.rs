//! Edit as text: the configuration's vtium.conf opened in the person's
//! editor, through the path `vtium config edit` takes. The loop writes a
//! copy of the document's text, runs `$VISUAL`, else `$EDITOR`, else `vi`
//! on it while the TUI steps aside, and runs `vtium check-config` on the
//! copy when the editor exits ([`run`]). Update then puts the copy's text
//! in the document as one change, and asks whether to edit again when the
//! check found problems ([`back`]). The document keeps the text either
//! way, since it may be unfinished, and Save refuses a configuration the
//! check fails.
//!
//! File › Edit as text… and `e` in Show changes start it. Each return of
//! the editor that changed the text is one step of the undo history, so an
//! edit again is a step of its own.
//!
//! References:
//! - ADR-1403 D11, "Show changes and Edit as text": the editor, the check
//!   on return, the question to edit again, and the text kept either way.
//! - ADR-1403 D8, "`vtium config edit`, the way visudo edits sudoers": the
//!   editor named by `$VISUAL`, then `$EDITOR`, then `vi`, and the check.
//! - ADR-1403 D11, "Undo and redo": the text Edit as text brings back is
//!   one step.

use std::path::Path;

use vtium_config_core::document::{Change, Document, Edit};
use vtium_config_core::drafts::Session;

/// The row a yes to the question after a failed check hands the shell:
/// the editor opens again on the configuration.
pub const AGAIN: &str = "y.question.edit-again";

/// The node of that question.
pub const QUESTION: &str = "any.question.edit-again";

/// What the person's editor left: the copy's text, and what `vtium
/// check-config` found in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edited {
    /// The copy's text once the editor exited.
    pub text: String,
    /// The problems the check reports, empty where it passed or found
    /// only fonts this machine lacks, as `vtium config edit` takes it.
    pub problems: Vec<String>,
    /// How many problems the check counts among those lines.
    pub count: usize,
}

/// Runs the editor `var` names on a copy of `text`, the configuration
/// `name`'s vtium.conf, kept under the store of `session`, then checks the
/// copy against the configuration directory with the font set changes of
/// the configuration. The copy is removed afterwards. A text the editor
/// left as it was is not checked. `var` reads the environment.
pub fn run(
    session: &Session<'_>,
    name: &str,
    text: &str,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Edited, String> {
    let copy = session.store.edit_copy(name).map_err(fail)?;
    vtium_config_core::store::write(&copy, text).map_err(fail)?;
    let edited = edit(&copy, var);
    let _ = std::fs::remove_file(&copy);
    let edited = edited?;
    if edited == text {
        return Ok(Edited {
            text: edited,
            ..Edited::default()
        });
    }
    let checked = session
        .check(name, &edited, Path::new("vtium.conf"))
        .map_err(fail)?;
    let (problems, count) = if checked.clears() {
        (Vec::new(), 0)
    } else {
        let count = checked.count();
        (checked.problems, count)
    };
    Ok(Edited {
        text: edited,
        problems,
        count,
    })
}

/// Runs the editor on `copy`, as `vtium config edit` runs it, and reads
/// the copy back.
fn edit(copy: &Path, var: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let editor = vtium_config_core::flowcli::editor(var);
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(copy)
        .status()
        .map_err(|e| format!("{editor}: {e}"))?;
    if !status.success() {
        return Err(words::failed(&editor));
    }
    Ok(vtium_config_core::places::read_conf(copy)?.unwrap_or_default())
}

fn fail(f: vtium_config_core::outcome::Fail) -> String {
    f.text().to_owned()
}

/// What update does with the editor's return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Back {
    /// Only a line on the message line: the editor did not run, or left
    /// the text as it was.
    Say(String),
    /// The change that puts the text in the document, the line that says
    /// what it changed, and the question to edit again where the check
    /// found problems.
    Change {
        change: Change,
        said: String,
        again: Option<String>,
    },
}

/// What the editor's return `done` does to `document`.
pub fn back(document: &Document, done: Result<Edited, String>) -> Back {
    let edited = match done {
        Ok(edited) => edited,
        Err(why) => return Back::Say(why),
    };
    if edited.text == document.text() {
        return Back::Say(words::UNCHANGED.to_owned());
    }
    let (added, removed) = vtium_config_core::drafts::counts(document.text(), &edited.text);
    let again = (!edited.problems.is_empty())
        .then(|| words::again(edited.count.max(1), edited.problems.first()));
    Back::Change {
        change: Change::new(words::STEP, Edit::Text(edited.text)),
        said: words::changed(added, removed),
        again,
    }
}

/// The words Edit as text puts on the screen.
pub mod words {
    /// The step of the undo history, as the message line and the File
    /// menu name it.
    pub const STEP: &str = "Edit as text";

    /// A loop with no editor to run.
    pub const NO_EDITOR: &str = "Edit as text cannot run an editor here.";

    /// The editor left the text as it was.
    pub const UNCHANGED: &str = "Edit as text changed nothing.";

    /// The editor ended with a failure.
    pub fn failed(editor: &str) -> String {
        format!("The editor {editor} failed, so the configuration is unchanged.")
    }

    /// The text came back changed.
    pub fn changed(added: usize, removed: usize) -> String {
        format!(
            "Edit as text: {}, not saved. [Ctrl+Z] takes it back.",
            vtium_config_core::drafts::lines(added, removed)
        )
    }

    /// The question after the check found `n` problems, the first of
    /// them `first`.
    pub fn again(n: usize, first: Option<&String>) -> String {
        let found = match n {
            1 => "The check found 1 problem".to_owned(),
            n => format!("The check found {n} problems"),
        };
        match first {
            Some(line) => format!("{found}: {}. Edit again?", line.trim_end_matches('.')),
            None => format!("{found}. Edit again?"),
        }
    }
}
