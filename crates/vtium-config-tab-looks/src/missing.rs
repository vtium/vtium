//! The fonts each look uses and where each stands, the rows of a card's
//! "Fonts by role", what a look still misses, and Activate for one look.
//!
//! vtium-config-core's walk lists every font file a font set of the
//! document would draw, each with its state and the looks that use it, so
//! the tab marks a gap before the console draws without the font. The
//! walk judges each file against the batch, the list of active resources.
//! The shared state holds the resources found at start with each one's
//! activation, so this module gives the walk the batch those marks
//! describe: every font the marks call active, and every font the batch
//! file turns off with a `-` line. The walk reads a font set's layers and
//! a kbd console font's file, so the tab walks again only after the
//! document changes, never on a key that only moves the highlight.
//!
//! Activate puts the look's fonts that are installed and trusted but not
//! active into the document's batch as one change, which Undo takes back
//! and Save makes real. A font turned off on purpose stays off, and a
//! font that fails the trust rule or is missing is never activated.
//!
//! References:
//! - ADR-1205 D4, "Fonts a configuration uses but has not activated": the walk, the five states, and Activate missing.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": the marks and their roles, a look's own mark and its card's `[A] Activate`.
//! - ADR-1205 D8: a look's card says what it still misses, by role.

use std::path::{Path, PathBuf};

use vtium_config_core::batch::{Change as Batch, Folders, Start, View};
use vtium_config_core::document::{Change, Content, Edit};
use vtium_config_core::fontuse::{self, Activated, Counts, Font, Machine, State, Used, User};
use vtium_config_core::found::{Found, Origin};
use vtium_config_core::ui::{Change as UiChange, Command, Shared};
use vtium_config_widgets::roles::Role;

use crate::words;

/// The fonts the document uses, with what the walk read them against, so
/// the tab walks again only when either changes.
#[derive(Clone, Debug, Default)]
pub struct Walked {
    content: Content,
    fonts: usize,
    pub used: Used,
}

impl Walked {
    /// The walk of `shared`'s document, or `self` again where neither the
    /// document nor the fonts found changed since it was made.
    pub fn of(shared: &Shared, last: Option<Walked>) -> Walked {
        let content = shared.document().content();
        let fonts = shared.found.fonts.files().len();
        if let Some(last) = last.filter(|w| w.fonts == fonts && w.content == *content) {
            return last;
        }
        Walked {
            content: content.clone(),
            fonts,
            used: walk(shared),
        }
    }
}

/// Every font the document uses, judged against the batch the resources'
/// marks describe.
fn walk(shared: &Shared) -> Used {
    let view = view(&shared.found);
    let kbd = fontuse::kbd_dirs();
    let euid = shared.found.places.as_ref().map_or(0, |p| p.euid);
    let judge = |path: &Path| vtium_config_core::batch::trusted(path, euid);
    let machine = Machine {
        fonts: &shared.found.fonts,
        view: &view,
        layers: None,
        kbd_dirs: &kbd,
        judge: &judge,
    };
    Used::of(shared.document(), &machine)
}

/// The batch that the marks of the fonts found describe: each font whose
/// activation comes from the batch file, the list of a fresh install or a
/// package's list, and a `-` line for each the batch file turns off. It
/// reads no folder of lists.
fn view(found: &Found) -> View {
    let mut text = String::new();
    for file in found.fonts.files() {
        let path = file.path();
        let Some(shown) = path.to_str() else { continue };
        match &file.resource.origin {
            Origin::BatchFile | Origin::FreshInstall | Origin::List { .. } => {
                text.push_str(shown);
                text.push('\n');
            }
            Origin::TurnedOff { .. } => {
                if let Some(line) = vtium_settings::batch::off_line(path) {
                    text.push_str(&line);
                    text.push('\n');
                }
            }
            Origin::Nothing => {}
        }
    }
    let start = Start {
        fresh: vtium_settings::batch::Batch::default(),
        folders: Folders {
            package: PathBuf::new(),
            admin: PathBuf::new(),
        },
        trust: vtium_settings::Trust::running_as(0),
    };
    View::new(Some(text.as_bytes()), &start)
}

/// How many of the fonts `look` uses are in each state but active.
pub fn counts(used: &Used, look: &str) -> Counts {
    used.of_look(Some(look)).counts()
}

/// How many fonts `counts` counts as not active, whatever the reason.
pub fn not_active(counts: &Counts) -> usize {
    counts
        .not_active
        .saturating_add(counts.turned_off)
        .saturating_add(counts.cannot())
}

/// The mark beside a look whose fonts are not all active, as in `2 fonts
/// not active`; `None` where all are.
pub fn mark(used: &Used, look: &str) -> Option<String> {
    let counts = counts(used, look);
    (!counts.all_active()).then(|| fontuse::activate::words::not_active(not_active(&counts)))
}

/// One row of "Fonts by role": the role, the font, and the font's state,
/// `None` for a row that names no font file, such as a built-in font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleRow {
    pub role: String,
    pub font: String,
    pub state: Option<State>,
    /// Every font file the row stands for, which the detail lists.
    pub files: Vec<Font>,
}

/// The rows of "Fonts by role" for `look` in `used`: one for each way a
/// font set names a font, in the walk's order, the set's own sources as
/// Text and a layer's under the layer's name. A row of several files, such
/// as a family's faces, shows the state of the one furthest from active.
pub fn roles(used: &Used, look: &str) -> Vec<RoleRow> {
    rows_where(used, |u| u.look.as_deref() == Some(look))
}

/// The rows of "Fonts by role" for the font set `set`, as [`roles`] gives
/// a look's: empty where no look uses the set, so the walk did not reach
/// it.
pub fn set_roles(used: &Used, set: &str) -> Vec<RoleRow> {
    rows_where(used, |u| u.font_set == set)
}

/// The rows of the fonts whose first user `keep` keeps.
fn rows_where(used: &Used, keep: impl Fn(&User) -> bool) -> Vec<RoleRow> {
    let mut out: Vec<RoleRow> = Vec::new();
    for font in &used.fonts {
        let Some(user) = font.users.iter().find(|u| keep(u)) else {
            continue;
        };
        let role = user
            .layer
            .as_deref()
            .map_or_else(|| words::ROLE_TEXT.to_owned(), words::layer_role);
        let shown = words::named(&font.named);
        match out.iter_mut().find(|r| r.role == role && r.font == shown) {
            Some(row) => {
                if severity(&font.state) > row.state.as_ref().map_or(0, severity) {
                    row.state = Some(font.state.clone());
                }
                row.files.push(font.clone());
            }
            None => out.push(RoleRow {
                role,
                font: shown,
                state: Some(font.state.clone()),
                files: vec![font.clone()],
            }),
        }
    }
    out
}

/// How far a state is from active, for the row of several files.
fn severity(state: &State) -> u8 {
    match state {
        State::Active => 0,
        State::TurnedOff(_) => 1,
        State::NotActive => 2,
        State::FailsTrust { .. } => 3,
        State::Missing => 4,
    }
}

/// The mark of `state` in its parts, each in its role.
pub fn state_mark(state: &State) -> Vec<(String, Role)> {
    state
        .mark()
        .into_iter()
        .map(|(w, r)| (w.to_owned(), r))
        .collect()
}

/// The lines of what `look` still misses, behind Enter on its card: for
/// each font that is not active, its role, its name and why, in one line
/// each; and one line where nothing is missing.
pub fn misses(used: &Used, look: &str) -> Vec<String> {
    misses_of(&roles(used, look))
}

/// What the font set `set` still misses, as [`misses`] says it for a look.
pub fn set_misses(used: &Used, set: &str) -> Vec<String> {
    misses_of(&set_roles(used, set))
}

/// The lines of what the fonts of `rows` miss.
fn misses_of(rows: &[RoleRow]) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        for font in &row.files {
            let why = match &font.state {
                State::Active => continue,
                State::NotActive => words::MISS_NOT_ACTIVE.to_owned(),
                State::TurnedOff(_) => words::MISS_TURNED_OFF.to_owned(),
                State::FailsTrust { check, .. } => words::miss_trust(check),
                State::Missing => words::MISS_NOT_INSTALLED.to_owned(),
            };
            out.push(words::miss(&row.role, &words::named(&font.name()), &why));
        }
    }
    if out.is_empty() {
        out.push(words::MISSES_NOTHING.to_owned());
    }
    out
}

/// What Activate on `look`'s card does: the change that activates its
/// fonts that are installed and trusted but not active, as one step, and
/// the message that says what it did and what it could not do. With
/// nothing to activate, only the message.
pub fn activate(used: &Used, look: &str) -> Vec<Command> {
    let own = used.of_look(Some(look));
    let mut done = Activated::default();
    for font in &own.fonts {
        match &font.state {
            State::TurnedOff(_) => done.turned_off = done.turned_off.saturating_add(1),
            state => {
                if let Some(why) = state.reason() {
                    done.cannot.push((font.name(), why.to_owned()));
                }
            }
        }
    }
    done.activated = own.to_activate();
    let mut out = Vec::new();
    if !done.activated.is_empty() {
        let edits = done
            .activated
            .iter()
            .map(|path| Edit::Active {
                path: path.clone(),
                change: Some(Batch::Activate),
            })
            .collect();
        let name = fontuse::activate::words::step(done.activated.len());
        out.push(Command::Change(UiChange::Document(Change::new(
            name,
            Edit::All(edits),
        ))));
    }
    out.push(Command::Change(UiChange::Say(done.message())));
    out
}
