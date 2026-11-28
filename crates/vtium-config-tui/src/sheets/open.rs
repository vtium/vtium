//! Opening a configuration: File › Open ›'s entries, Enter on the Manage
//! sheet, and the configuration File › New… makes. Opening one keeps the
//! one that was open, with its changes and its undo history, so nothing
//! is asked. Each configuration's state against the installed file is
//! read here too, for the Manage sheet.
//!
//! References:
//! - ADR-1403 D11, "The menus" (Open ›) and "Managing configurations":
//!   opening one keeps the one open, and each shows its state.
//! - ADR-1403 D11, "Undo and redo": each configuration keeps its own
//!   history while vtium-config runs, and switching is no step.
//! - ADR-1403 D11, "New…: what is saved, or vtium's defaults": what each
//!   starting point holds.

use vtium_config_core::document::{Base, Content, Document};
use vtium_config_core::ui::Shared;
use vtium_config_core::{drafts, ops};

use super::new::Start;

/// Opens the configuration `name` in place of the open one, and returns
/// the message line's words; `None` where it is open already.
pub fn open(shared: &mut Shared, name: &str) -> Result<Option<String>, String> {
    let was = shared.document().name().to_owned();
    if was == name {
        return Ok(None);
    }
    let content = shared
        .documents
        .desk()
        .read(name)?
        .ok_or_else(|| words::no_such(name))?;
    let installed = shared.document().installed().clone();
    let kept = shared.document().count();
    shared
        .documents
        .switch(Document::new(name, content, installed));
    Ok(Some(words::opened(name, &was, kept)))
}

/// Makes the configuration `name` from `start` and opens it, keeping the
/// one open. Returns the message line's words.
pub fn make(shared: &mut Shared, name: &str, start: Start) -> String {
    let installed = shared.document().installed().clone();
    let text = match start {
        Start::Saved => installed.text.clone().unwrap_or_default(),
        Start::Defaults => ops::new_text(drafts::HEADER),
    };
    let content = Content {
        text,
        base: Base::of(installed.text.as_deref()),
        ..Content::default()
    };
    let was = shared.document().name().to_owned();
    let kept = shared.document().count();
    shared
        .documents
        .switch(Document::new(name, content, installed));
    words::made(name, &was, kept)
}

/// Each configuration kept, in the order of their names, with its count
/// of changes not saved against the installed file. The open one counts
/// as it stands now, and is listed even before its draft is written.
pub fn states(shared: &Shared) -> Vec<(String, usize)> {
    let open = shared.document();
    let mut names = shared.documents.desk().configurations().unwrap_or_default();
    if !names.iter().any(|n| n == open.name()) {
        names.push(open.name().to_owned());
        names.sort();
    }
    names
        .into_iter()
        .map(|name| {
            let count = if name == open.name() {
                open.count()
            } else {
                match shared.documents.desk().read(&name) {
                    Ok(Some(content)) => {
                        Document::new(name.as_str(), content, open.installed().clone()).count()
                    }
                    _ => 0,
                }
            };
            (name, count)
        })
        .collect()
}

/// The words of opening and making a configuration.
mod words {
    pub fn no_such(name: &str) -> String {
        format!("There is no configuration named {name}.")
    }

    /// What the one left keeps, as the end of a message.
    fn keeps(was: &str, kept: usize) -> String {
        match kept {
            0 => format!("{was} stays under File › Open."),
            1 => format!("{was} keeps its 1 change, not saved."),
            n => format!("{was} keeps its {n} changes, not saved."),
        }
    }

    pub fn opened(name: &str, was: &str, kept: usize) -> String {
        format!("Opened {name}. {}", keeps(was, kept))
    }

    pub fn made(name: &str, was: &str, kept: usize) -> String {
        format!("Made {name}. {}", keeps(was, kept))
    }
}
