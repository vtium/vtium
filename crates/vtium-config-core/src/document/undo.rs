//! The undo history of one document. Each change to the document is one
//! [`Step`], which keeps what the document held before and after it:
//! vtium.conf's text, the font set changes, the changes to the active
//! resources and the base. Ctrl+Z puts the step's before in place, and
//! Ctrl+Y its after. A new change after an undo drops the steps the undo
//! took back, as a word processor drops them.
//!
//! The history lives only while vtium-config runs. The draft keeps the
//! document across a quit, but the command line may edit the same draft
//! between two runs, and a history kept across a quit could then undo onto
//! text it never recorded.
//!
//! References:
//! - ADR-1403 D11, "Undo and redo": one change is one step, redo, undo past a Save, and the history lasting while vtium-config runs.
//! - ADR-1206 D3, "The undo history lives in `shared`": what a step keeps.

use super::{Content, Installed};
use crate::batch::{Change, Changes};

/// One change to the document, as the history keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The change's name, which the message line and the File menu show.
    pub name: String,
    /// What the document held before the change.
    pub before: Content,
    /// What it held after.
    pub after: Content,
}

/// The steps of one document, oldest first, and how many of them are
/// made: the steps after that many are the ones undo took back, which
/// redo makes again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    steps: Vec<Step>,
    made: usize,
}

impl History {
    /// Records `step`, made after the steps made so far. The steps an undo
    /// took back go, since redo cannot make them on top of a new change.
    pub fn record(&mut self, step: Step) {
        self.steps.truncate(self.made);
        self.steps.push(step);
        self.made = self.steps.len();
    }

    /// The step Ctrl+Z would take back.
    pub fn to_undo(&self) -> Option<&Step> {
        self.steps.get(self.made.checked_sub(1)?)
    }

    /// The step Ctrl+Y would make again.
    pub fn to_redo(&self) -> Option<&Step> {
        self.steps.get(self.made)
    }

    /// Counts the step to undo as taken back.
    pub(super) fn back(&mut self) {
        self.made = self.made.saturating_sub(1);
    }

    /// Counts the step to redo as made again.
    pub(super) fn forward(&mut self) {
        self.made = self.made.saturating_add(1).min(self.steps.len());
    }

    /// Every step, oldest first.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// How many steps are made, which undo can take back.
    pub fn made(&self) -> usize {
        self.made
    }

    /// Changes what every step keeps before and after it with `f`.
    pub(super) fn rebase(&mut self, mut f: impl FnMut(&mut Content)) {
        for step in &mut self.steps {
            f(&mut step.before);
            f(&mut step.after);
        }
    }
}

/// Counts `content`'s font set changes and changes to the active resources
/// again after a save: they were counted against `was`, what the system
/// held before the save, and the save installed `saved`'s changes to the
/// active resources and left `now` installed. A font set or a resource
/// that `content` holds as the save installed it is then no change, and
/// one that `content` holds as the system held it before the save is now
/// a change back. So undoing past a save brings back what the document
/// held then, as a change that is not saved.
pub(super) fn rebase(content: &mut Content, saved: &Changes, was: &Installed, now: &Installed) {
    // each font set as `content` would leave the system, then counted
    // against what the save left installed
    let mut names: std::collections::BTreeSet<String> = content
        .font_sets
        .kept
        .keys()
        .chain(&content.font_sets.removed)
        .cloned()
        .collect();
    names.extend(
        was.frits
            .iter()
            .filter(|(n, b)| now.frits.get(*n) != Some(*b))
            .map(|(n, _)| n.clone()),
    );
    names.extend(
        now.frits
            .keys()
            .filter(|n| !was.frits.contains_key(*n))
            .cloned(),
    );
    for name in names {
        let wanted: Option<Vec<u8>> = match content.font_sets.kept.get(&name) {
            Some(text) => Some(text.clone().into_bytes()),
            None if content.font_sets.removed.contains(&name) => None,
            None => was.frits.get(&name).cloned(),
        };
        let sets = &mut content.font_sets;
        sets.kept.remove(&name);
        sets.removed.remove(&name);
        match wanted {
            Some(bytes) if now.frits.get(&name) != Some(&bytes) => {
                // a font set that is not UTF-8 text came from the disk and
                // cannot be a draft's change; the store refuses it too
                if let Ok(text) = String::from_utf8(bytes) {
                    sets.kept.insert(name, text);
                }
            }
            None if now.frits.contains_key(&name) => {
                sets.removed.insert(name);
            }
            _ => {}
        }
    }
    // a resource is active or not, so the save's change tells what the
    // system held before it
    for (path, installed) in saved.iter() {
        let path = path.to_path_buf();
        let back = match installed {
            Change::Activate => Change::Deactivate,
            Change::Deactivate => Change::Activate,
        };
        match content.active.get(&path) {
            Some(c) if c == installed => content.active.set(path, None),
            Some(_) => {}
            None => content.active.set(path, Some(back)),
        }
    }
}
