//! Activate missing: every font the configuration uses that is installed
//! and trusted but not active goes into the document's batch as one
//! change, through the one change function, so Undo takes it back as one
//! step and Save makes it real. Nothing is activated outside Save.
//!
//! Each font goes in as Space on one font puts it in, an activation of
//! its path in the document's changes to the active resources, and the
//! install then adds a line to batch.conf only where no list would
//! activate the path ([`crate::batch::applied`]). A font turned off on
//! purpose stays off, a font that fails the trust rule or is missing is
//! never activated, and the message counts each kind apart.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": Activate missing as one change through the normal activation path, and fonts turned off on purpose staying off.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": the action's count, its message, a look's own action, and the Problems list's line and question.

use std::path::PathBuf;

use crate::batch::Change as Batch;
use crate::document::{Change, Documents, Done, Edit};
use crate::ui::Message;

use super::{State, Used};

/// Which fonts the action activates: every font the configuration uses,
/// as the Fonts tab's `[A]` and the Problems list do, or one look's, the
/// top level's for `None`, as a look's card does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope<'a> {
    All,
    Look(Option<&'a str>),
}

/// What Activate missing did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activated {
    /// The files it activated in the document, in the walk's order.
    pub activated: Vec<PathBuf>,
    /// How many fonts turned off on purpose it left off.
    pub turned_off: usize,
    /// Each font it cannot activate, by name, with the reason: the check
    /// it failed, or that it is not installed.
    pub cannot: Vec<(String, String)>,
}

/// Activates in the open document every font in `used`, within `scope`,
/// that is installed and trusted but not active, as one change. `Err` is
/// the document's refusal, and leaves the document as it was. With
/// nothing to activate, the document is left as it was and the result
/// activates nothing.
pub fn activate(
    documents: &mut Documents,
    used: &Used,
    scope: Scope<'_>,
) -> Result<Activated, String> {
    let used = match scope {
        Scope::All => used.clone(),
        Scope::Look(look) => used.of_look(look),
    };
    let mut done = Activated::default();
    for font in &used.fonts {
        match &font.state {
            State::TurnedOff(_) => done.turned_off = done.turned_off.saturating_add(1),
            state => {
                if let Some(why) = state.reason() {
                    done.cannot.push((font.name(), why.to_owned()));
                }
            }
        }
    }
    done.activated = used.to_activate();
    if done.activated.is_empty() {
        return Ok(done);
    }
    let edits = done
        .activated
        .iter()
        .map(|path| Edit::Active {
            path: path.clone(),
            change: Some(Batch::Activate),
        })
        .collect();
    let name = words::step(done.activated.len());
    match documents.change(Change::new(name, Edit::All(edits)))? {
        Done::Made => Ok(done),
        // the document already activates each of them
        Done::Same => Ok(Activated {
            activated: Vec::new(),
            ..done
        }),
    }
}

impl Activated {
    /// The message line after the action, one line at 80 columns, with
    /// the fonts it cannot activate and why behind `[F1]`, where there
    /// are any. The line names the fonts it activated, then those turned
    /// off on purpose and those it cannot activate, each counted apart.
    /// Where the line would name both, it says so in fewer words, and the
    /// hint that Undo takes the change back goes, as the action bar shows
    /// `[Ctrl+Z] Undo` beside it.
    pub fn message(&self) -> Message {
        let (n, off, cannot) = (self.activated.len(), self.turned_off, self.cannot.len());
        let lead = match n {
            0 => words::NOTHING.to_owned(),
            n => words::activated(n),
        };
        let line = match (off, cannot) {
            (0, 0) if n == 0 => words::ALL_ACTIVE.to_owned(),
            (0, 0) => format!("{lead} {}", words::UNDO),
            (off, 0) => format!("{lead} {}", words::stays_off(off)),
            (0, cannot) => format!("{lead} {}", words::cannot(cannot, n > 0)),
            (off, cannot) => words::both(n, off, cannot),
        };
        let more = (cannot > 0).then(|| {
            self.cannot
                .iter()
                .map(|(font, why)| format!("{font}: {why}"))
                .collect::<Vec<_>>()
                .join("\n")
        });
        Message { line, more }
    }
}

/// The words of Activate missing wherever it shows.
pub mod words {
    /// "1 font" or "N fonts".
    fn fonts(n: usize) -> String {
        match n {
            1 => "1 font".to_owned(),
            n => format!("{n} fonts"),
        }
    }

    /// The change's name, which Undo and the File menu show.
    pub fn step(n: usize) -> String {
        format!("Activate missing, {}", fonts(n))
    }

    /// How the message starts after the action activated `n` fonts.
    pub fn activated(n: usize) -> String {
        format!("Activated {} the configuration uses.", fonts(n))
    }

    /// How the message ends when it says nothing more.
    pub const UNDO: &str = "[Ctrl+Z] undoes it.";

    /// How the message starts when no font waits to be activated.
    pub const NOTHING: &str = "Nothing to activate.";

    /// The message when every font the configuration uses is active.
    pub const ALL_ACTIVE: &str = "Every font the configuration uses is active.";

    /// What the message adds for `n` fonts turned off on purpose.
    pub fn stays_off(n: usize) -> String {
        match n {
            1 => "1 turned off on purpose stays off.".to_owned(),
            n => format!("{n} turned off on purpose stay off."),
        }
    }

    /// What the message adds for `n` fonts it cannot activate, `more`
    /// where it activated others.
    pub fn cannot(n: usize, more: bool) -> String {
        let more = if more { " more" } else { "" };
        format!("{n}{more} cannot be activated ([F1]).")
    }

    /// The message after the action activated `n` fonts, left `off`
    /// turned off on purpose, and could not activate `cannot`.
    pub fn both(n: usize, off: usize, cannot: usize) -> String {
        let lead = match n {
            0 => NOTHING.to_owned(),
            n => format!("Activated {}.", fonts(n)),
        };
        let stay = if off == 1 { "stays" } else { "stay" };
        format!("{lead} {off} {stay} off on purpose, and {cannot} cannot be activated ([F1]).")
    }

    /// The action bar's button, `[A] Activate missing (3)`, by its label.
    pub fn button(n: usize) -> String {
        format!("Activate missing ({n})")
    }

    /// The Fonts list's first item.
    pub fn item(n: usize) -> String {
        format!("Activate missing fonts ({n})")
    }

    /// A look's mark beside its name, as in `My-Look · 2 fonts not
    /// active`.
    pub fn not_active(n: usize) -> String {
        format!("{} not active", fonts(n))
    }

    /// The Problems list's warning line.
    pub fn problem(n: usize) -> String {
        match n {
            1 => "1 font the configuration uses is not active".to_owned(),
            n => format!("{n} fonts the configuration uses are not active"),
        }
    }

    /// The question Enter on the Problems list's line asks.
    pub fn question(n: usize) -> String {
        match n {
            1 => "Activate the font the configuration uses that is not active?".to_owned(),
            n => format!("Activate the {n} fonts the configuration uses that are not active?"),
        }
    }
}
