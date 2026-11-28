//! The state of one font file that a configuration uses, how vtium-config
//! judges it, and the marks every screen shows for it.
//!
//! A file is judged against the open document's batch: what vtium reads
//! from batch.conf and the lists of both `batch.d` folders, less what
//! batch.conf turns off, with the document's own changes to the active
//! resources over it. The trust rule's verdict comes from the scan of the
//! font folders, which judged each file once when vtium-config started,
//! so a judgement opens no file.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the five states and how each is found.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": the marks, their words and their color roles.

use std::path::{Path, PathBuf};

use vtium_config_widgets::roles::Role;

use crate::batch::{Change, Changes};

use super::Machine;

/// How a font that is installed and trusted was turned off on purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Off {
    /// A `-PATH` line of batch.conf turns it off.
    BatchFile,
    /// Root's entry at this path, in `/etc/vtium/batch.d/`, turns off or
    /// replaces the package's list that names the font.
    List(PathBuf),
    /// The open document deactivates it, which Save makes real.
    Document,
}

/// Where one font file that the configuration uses stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// The batch holds the file, or the document activates it, and the
    /// file passes the trust rule.
    Active,
    /// The file is installed and passes the trust rule, but nothing in the
    /// batch lists it, so the configuration draws without it. Activate
    /// missing activates it.
    NotActive,
    /// The file is installed and passes the trust rule, and root turned it
    /// off, or the document does. Activate missing leaves it off.
    TurnedOff(Off),
    /// A file of the font is installed but fails the trust rule. `check`
    /// names the file and the check it failed, in the words of `vtium
    /// check-config`. `own` is set for a file in the person's own folders,
    /// where vtium takes no font until it is installed as a system font.
    FailsTrust { check: String, own: bool },
    /// No file of the font is installed: the reason is [`NOT_INSTALLED`].
    Missing,
}

/// The reason a missing font gives, the word `vtium config active` prints
/// for a listed file that is not at its path.
pub use crate::batch::NOT_INSTALLED;

/// The mark of a look, or of the configuration, whose fonts are not all
/// active, as in `My-Look · 2 fonts not active`.
pub const NOT_ALL_ACTIVE: (&str, Role) = ("○ not all active", Role::Warning);

impl State {
    /// The mark every screen shows for the state, in one or two parts,
    /// each with its color role. The words carry the meaning, so the mark
    /// reads the same without color: `● active`, `○ not active`, `○
    /// turned off on purpose`, `○ not active · fails trust`, `not usable
    /// yet` and `✗ missing`.
    pub fn mark(&self) -> Vec<(&'static str, Role)> {
        match self {
            State::Active => vec![("● active", Role::Active)],
            State::NotActive => vec![("○ not active", Role::Inactive)],
            State::TurnedOff(_) => vec![("○ turned off on purpose", Role::Inactive)],
            State::FailsTrust { own: true, .. } => vec![("not usable yet", Role::Warning)],
            State::FailsTrust { own: false, .. } => vec![
                ("○ not active · ", Role::Inactive),
                ("fails trust", Role::Warning),
            ],
            State::Missing => vec![("✗ missing", Role::Missing)],
        }
    }

    /// The mark's words, its parts joined, as a line without color shows
    /// them.
    pub fn words(&self) -> String {
        self.mark().into_iter().map(|(w, _)| w).collect()
    }

    /// Why the font cannot be activated, for the list behind `[F1]`: the
    /// check it failed, or that it is not installed. `None` for a font
    /// that is active, not active or turned off on purpose.
    pub fn reason(&self) -> Option<&str> {
        match self {
            State::FailsTrust { check, .. } => Some(check),
            State::Missing => Some(NOT_INSTALLED),
            _ => None,
        }
    }
}

/// How many fonts a configuration, or one look, uses in each state but
/// active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// The fonts Activate missing activates.
    pub not_active: usize,
    /// The fonts turned off on purpose, which Activate missing leaves off
    /// and the Problems list does not warn of.
    pub turned_off: usize,
    /// The fonts that fail the trust rule, the person's own among them.
    pub fails_trust: usize,
    pub missing: usize,
}

impl Counts {
    /// Whether every font is active, so the look or the configuration
    /// carries no mark of its own.
    pub fn all_active(&self) -> bool {
        *self == Counts::default()
    }

    /// The fonts Activate missing cannot activate, which its message
    /// counts as "more".
    pub fn cannot(&self) -> usize {
        self.fails_trust.saturating_add(self.missing)
    }
}

/// The state of the font file at `path` against `machine` and the
/// document's changes to the active resources `changes`, the one rule for
/// a file a font set of the document names and for a file the Fonts tab
/// lists that none names. The scan's trust mark stands for a file the
/// scan found, and `machine.judge` judges any other.
pub fn judge(path: &Path, changes: &Changes, machine: &Machine<'_>) -> State {
    let found = machine.fonts.file(path);
    if found.is_none() && std::fs::symlink_metadata(path).is_err() {
        return State::Missing;
    }
    let (trust, own) = match found {
        Some(f) => (f.resource.trust.clone(), f.resource.own),
        None => ((machine.judge)(path), false),
    };
    match trust {
        Err(check) => return State::FailsTrust { check, own },
        Ok(()) if own => {
            return State::FailsTrust {
                check: format!(
                    "{}: this file is in your own folders, where vtium takes no font. Install it as a system font to use it.",
                    path.display()
                ),
                own,
            };
        }
        Ok(()) => {}
    }
    match changes.get(path) {
        Some(Change::Activate) => State::Active,
        Some(Change::Deactivate) => State::TurnedOff(Off::Document),
        None if machine.view.batch().holds(path) => State::Active,
        None => match machine.view.off(path) {
            Some(crate::batch::Off::BatchFile) => State::TurnedOff(Off::BatchFile),
            Some(crate::batch::Off::List(by)) => State::TurnedOff(Off::List(by)),
            None => State::NotActive,
        },
    }
}
