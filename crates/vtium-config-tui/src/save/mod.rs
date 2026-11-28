//! Save: the one step that writes the configuration to
//! `/etc/vtium/vtium.conf`. The first Ctrl+S reads the installed file
//! again, through the loop, and opens one of three screens as a sheet
//! over the tab:
//!
//! - [`review`]: Save's review of what saving changes, with the problems
//!   the check reports. Ctrl+S again, or Enter, saves.
//! - [`changed`]: the installed file changed on disk while the person
//!   worked, so the review shows that change beside theirs, with Update,
//!   Overwrite and Revert.
//! - [`settle`]: Update found lines both sides changed, and lists them one
//!   at a time for `[m]` mine or `[i]` installed.
//!
//! [`privileged`] is the step that saves: the check, the install bundle
//! and `vtium-config apply` through sudo, which the loop runs while the
//! TUI steps aside, so sudo asks for the password on the terminal once.
//!
//! The functions behind these screens are vtium-config-core's: the
//! document's review, `versions` for the change on disk, and `drafts` for
//! the install. This module draws them and routes their keys. Every key
//! that changes the document makes one step of its undo history, through
//! the one change function, and Ctrl+Z and Ctrl+Y do nothing on these
//! screens.
//!
//! References:
//! - ADR-1403 D8, "Save: the review, then sudo once": the review, the
//!   second Ctrl+S, the question for a file with no complete route, and
//!   the base after a save.
//! - ADR-1403 D8, "When the installed file changed on disk": what counts
//!   as a change on disk, Update, Overwrite, Revert, and settling lines
//!   both sides changed with `[m]` and `[i]`.
//! - ADR-1403 D3, "A valid configuration wires at least one complete
//!   route": Save asks before it saves a file with no complete route.
//! - ADR-1403 D11, "One flow for every change": one Save makes the
//!   document real, and nothing reaches `/etc` another way.
//! - ADR-1201 D5: the privileged step `vtium-config apply DIR`, and where
//!   vtium reads the file it writes.

pub mod changed;
pub mod privileged;
pub mod review;
pub mod settle;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::document::Installed;
use vtium_config_core::draft::Plan;
use vtium_config_core::ui::{Action, Effect, Message, Op, Shared};
use vtium_config_core::versions::{self, base::Disk, merge::Side};
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::Bar;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;

use crate::nav::Nav;
use crate::sheets::{Act, Sheet};

/// The row a yes to the question for a file with no complete route hands
/// back: save it anyway.
pub const ANYWAY: &str = "y.question.save-anyway";

/// The row a yes to Revert's question hands back: start again from the
/// installed file.
pub const REVERTED: &str = "y.question.save-revert";

/// The key of Save's screens that the shell acts on, by its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Ctrl+S on a tab, File › Save… or the bar's Save: reads the
    /// installed file again, then opens the review.
    Open,
    /// Ctrl+S or Enter on the review: saves.
    Save,
    /// The yes to the question for a file with no complete route.
    Anyway,
    /// `[u]` Update.
    Update,
    /// `[o]` Overwrite.
    Overwrite,
    /// `[r]` Revert, which asks first.
    Revert,
    /// The yes to Revert's question.
    Reverted,
    /// `[m]` or `[i]`, which settles the lines on screen.
    Settle(Side),
}

/// What the loop hands back for Save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Came {
    /// What is installed now, read again for the review, or why it cannot
    /// be read.
    Read(Result<Installed, String>),
    /// The privileged step ended: what is installed after it, or why
    /// nothing was saved.
    Saved(Result<Saved, String>),
}

/// A save that went through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// What is installed after the save, which the document now counts
    /// against.
    pub installed: Installed,
    /// Everything the step printed, which F1 opens behind the done line.
    pub said: String,
}

/// Save's sheet: the screen it shows, and what is installed as Save read
/// it, which Update, Overwrite and Revert take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Save {
    screen: Screen,
    installed: Installed,
    /// The first line in view.
    top: usize,
}

/// The screen Save's sheet shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Review(review::Review),
    Changed(changed::Changed),
    Settle(settle::Settle),
}

impl Save {
    /// The sheet on `screen`, with `installed` as Save read it.
    pub fn new(screen: Screen, installed: Installed) -> Save {
        Save {
            screen,
            installed,
            top: 0,
        }
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The sheet's node of the interface tree.
    pub fn node(&self) -> &'static str {
        match self.screen {
            Screen::Review(_) => "any.save",
            Screen::Changed(_) => "any.save.changed",
            Screen::Settle(_) => "any.save.settle",
        }
    }

    /// The action bar: the node's buttons, without Save while a problem
    /// stops it, and without Update while it cannot merge.
    pub fn bar(&self) -> Bar {
        let rows = contract::buttons(self.node());
        let gone = match &self.screen {
            Screen::Review(r) if r.stops() => Some("ctrl-s.layer.save"),
            Screen::Changed(c) if c.cannot_merge().is_some() => Some("u.layer.save-changed"),
            _ => None,
        };
        Bar::from_rows(rows.into_iter().filter(|r| Some(r.id) != gone))
    }

    /// The screen's lines, each with its role.
    fn lines(&self) -> Vec<(String, Role)> {
        match &self.screen {
            Screen::Review(r) => r.lines(),
            Screen::Changed(c) => c.lines(),
            Screen::Settle(s) => s.lines(),
        }
    }

    /// Acts on `key`, which chose the row `id` of the sheet's node: the
    /// sheet scrolls, shows the whole file, or goes back itself, and
    /// leaves every other row to the shell.
    pub fn key(&mut self, key: &Key, id: &str) -> Act {
        match id {
            "scroll.layer.save" => {
                let last = self.lines().len().saturating_sub(1);
                self.top = match key.code {
                    Code::Up => self.top.saturating_sub(1),
                    Code::Down => self.top.saturating_add(1).min(last),
                    Code::PageUp => self.top.saturating_sub(PAGE),
                    Code::PageDown => self.top.saturating_add(PAGE).min(last),
                    Code::Home => 0,
                    _ => last,
                };
                Act::Stay
            }
            "w.layer.save" => {
                if let Screen::Review(r) = &mut self.screen {
                    r.toggle_whole();
                    self.top = 0;
                }
                Act::Stay
            }
            "esc.layer.save" => Act::Close,
            // Esc while settling cancels the update: the document stays as
            // it was, and the screen for the file changed on disk is back
            "esc.layer.save-settle" => {
                if let Screen::Settle(s) = &self.screen {
                    self.screen = Screen::Changed(s.back());
                    self.top = 0;
                }
                Act::Stay
            }
            _ => Act::Pass,
        }
    }

    /// The sheet's box in `viewer`: the whole viewer.
    pub fn area(&self, viewer: Rect) -> Rect {
        viewer
    }

    /// Draws the screen's lines into `area`, from the first in view.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let width = usize::from(area.width);
        let lines = self.lines();
        let top = self.top.min(lines.len().saturating_sub(1));
        for (y, (text, role)) in lines
            .iter()
            .skip(top)
            .take(usize::from(area.height))
            .enumerate()
        {
            let y = u16::try_from(y).unwrap_or(u16::MAX);
            crate::sheets::put(
                buf,
                area,
                0,
                y,
                &crate::sheets::cut(text, width),
                *role,
                color,
            );
        }
    }
}

/// The step of PgUp and PgDn.
const PAGE: usize = 10;

/// The sheet's title, which names where Save writes.
pub const TITLE: &str = words::TITLE;

/// What a key of Save's screens does, given the shared state and the
/// navigation state: the effects for the loop.
pub(crate) fn press(press: Press, shared: &mut Shared, nav: &mut Nav) -> Vec<Effect> {
    match press {
        Press::Open => open(shared),
        Press::Save => save(shared, nav),
        Press::Anyway => install(shared, true),
        Press::Update => update(shared, nav),
        Press::Overwrite => {
            if let Some(s) = top(nav) {
                let installed = s.installed.text.clone();
                let change = versions::overwrite::overwrite(installed.as_deref());
                if shared.change(change).is_ok() {
                    s.screen = Screen::Review(review::Review::of(shared, None));
                    s.top = 0;
                }
            }
            Vec::new()
        }
        Press::Revert => {
            nav.ask(words::revert_question(shared), Action::of(REVERTED));
            Vec::new()
        }
        Press::Reverted => {
            let installed = top(nav).map(|s| s.installed.text.clone());
            if let Some(installed) = installed {
                let change = versions::revert::revert(installed.as_deref());
                if shared.change(change).is_ok() {
                    nav.close_sheet();
                    shared.say(words::REVERTED.to_owned());
                }
            }
            Vec::new()
        }
        Press::Settle(side) => {
            let Some(s) = top(nav) else {
                return Vec::new();
            };
            let Screen::Settle(settle) = &mut s.screen else {
                return Vec::new();
            };
            settle.settle(side);
            // once the last lines are settled, the merge is one change and
            // the review shows the merged document
            if let Some(change) = settle.update() {
                if shared.change(change).is_ok() {
                    s.screen = Screen::Review(review::Review::of(shared, None));
                }
                s.top = 0;
            }
            Vec::new()
        }
    }
}

/// What the loop handed back for Save.
pub(crate) fn came(came: Came, shared: &mut Shared, nav: &mut Nav) -> Vec<Effect> {
    match came {
        Came::Read(Ok(installed)) => {
            read(installed, shared, nav);
        }
        Came::Read(Err(why)) => shared.say(why),
        Came::Saved(done) => {
            shared.installs.running = false;
            while matches!(nav.sheets().last(), Some(Sheet::Save(_))) {
                nav.close_sheet();
            }
            shared.message = Some(match done {
                Ok(saved) => {
                    let n = shared.document().count();
                    shared.documents.saved(saved.installed);
                    Message {
                        line: words::saved(n),
                        more: Some(saved.said),
                    }
                }
                Err(why) => Message {
                    line: words::not_saved(&why),
                    more: Some(why),
                },
            });
        }
    }
    Vec::new()
}

/// Ctrl+S on a tab: "Nothing to save" with nothing changed, else the loop
/// reads the installed file again for the review.
fn open(shared: &mut Shared) -> Vec<Effect> {
    if shared.document().count() == 0 {
        shared.say(words::NOTHING.to_owned());
        return Vec::new();
    }
    vec![Effect::Desk(Op::Prepare)]
}

/// The installed file, read again: the document counts against it, and
/// Save opens the review, or the screen for a file changed on disk.
fn read(installed: Installed, shared: &mut Shared, nav: &mut Nav) {
    shared.documents.refresh(installed.clone());
    let disk = versions::base::disk(
        &shared.document().content().clone(),
        installed.text.as_deref(),
    );
    let mut note = None;
    let screen = match disk {
        Disk::Same | Disk::Missing => None,
        // no change of the person's meets the change on disk, so the
        // installed file comes in as the base without a choice
        Disk::Taken => {
            let text = installed.text.clone().unwrap_or_default();
            if shared.change(versions::base::take(&text)).is_ok() {
                note = Some(words::TAKEN.to_owned());
            }
            None
        }
        Disk::Changed(c) => {
            let versions = shared.documents.desk().versions().unwrap_or_default();
            let base = &shared.document().content().base;
            Some(Screen::Changed(changed::Changed::new(c, base, &versions)))
        }
    };
    let screen = match screen {
        Some(screen) => screen,
        None if shared.document().count() == 0 => {
            shared.say(note.map_or_else(
                || words::NOTHING.to_owned(),
                |n| format!("{n} {}", words::NOTHING),
            ));
            return;
        }
        None => Screen::Review(review::Review::of(shared, note)),
    };
    nav.open_sheet(Sheet::Save(Box::new(Save::new(screen, installed))));
}

/// Ctrl+S on the review: refuses while a problem stops Save, asks first
/// where the only problem is a missing complete route, else saves.
fn save(shared: &mut Shared, nav: &mut Nav) -> Vec<Effect> {
    let Some(Screen::Review(r)) = top(nav).map(|s| &s.screen) else {
        return Vec::new();
    };
    if r.stops() {
        if r.no_route_only() {
            let question = words::no_route_question(shared);
            nav.ask(question, Action::of(ANYWAY));
        } else {
            shared.say(words::STOPPED.to_owned());
        }
        return Vec::new();
    }
    install(shared, false)
}

/// The privileged step, with `no_route` the person's yes to saving a file
/// with no complete route, which the install bundle records.
fn install(shared: &mut Shared, no_route: bool) -> Vec<Effect> {
    shared.installs.running = true;
    vec![Effect::Install(Plan {
        no_route,
        ..Plan::default()
    })]
}

/// Update on the screen for a file changed on disk: the merge as one
/// change and the review, or the lines both sides changed to settle.
fn update(shared: &mut Shared, nav: &mut Nav) -> Vec<Effect> {
    let Some(s) = top(nav) else {
        return Vec::new();
    };
    let Screen::Changed(c) = &s.screen else {
        return Vec::new();
    };
    match c.merge() {
        Err(why) => shared.say(why.to_owned()),
        Ok(merge) if merge.touches() => {
            s.screen = Screen::Settle(settle::Settle::new(merge, c.clone()));
            s.top = 0;
        }
        Ok(merge) => {
            let installed = c.installed().to_owned();
            if let Some(change) = versions::merge::update(&merge, &installed)
                && shared.change(change).is_ok()
            {
                s.screen = Screen::Review(review::Review::of(shared, None));
                s.top = 0;
            }
        }
    }
    Vec::new()
}

/// Save's sheet, while it is the sheet on top.
fn top(nav: &mut Nav) -> Option<&mut Save> {
    match nav.sheet_mut() {
        Some(Sheet::Save(s)) => Some(s),
        _ => None,
    }
}

/// The lines of `diff`, as the review and the screen for a file changed
/// on disk show them: each hunk's place, then its lines with their numbers
/// and `-` or `+`, so the mark survives without color.
pub(crate) fn hunks(diff: &versions::diff::Diff, out: &mut Vec<(String, Role)>) {
    for hunk in &diff.hunks {
        out.push((format!(" @@ {}", hunk.section), Role::Dim));
        for line in &hunk.lines {
            out.push(diff_line(line));
        }
    }
}

/// One line of a diff, with its number and its mark.
pub(crate) fn diff_line(line: &versions::diff::Line) -> (String, Role) {
    use versions::diff::Mark;
    let (sign, role) = match line.mark {
        Mark::Same => (' ', Role::DiffContext),
        Mark::Removed => ('-', Role::DiffRemoved),
        Mark::Added => ('+', Role::DiffAdded),
    };
    (format!(" {:>5} {sign}{}", line.at, line.text), role)
}

/// The words of Save's screens and lines.
pub mod words {
    use vtium_config_core::ui::Shared;

    /// Where Save writes, as its sheet's title names it.
    pub const TITLE: &str = "Save to /etc/vtium/vtium.conf";

    /// Save on a loop with no machine to save on.
    pub const NO_MACHINE: &str = "vtium-config cannot save here: it has no machine to save on.";

    /// Ctrl+S with nothing changed since the last save.
    pub const NOTHING: &str = "Nothing to save.";

    /// The review's line when the installed file changed and no change of
    /// the person's meets it.
    pub const TAKEN: &str =
        "The file changed on disk, and none of your changes touches it, so yours now build on it.";

    /// Ctrl+S on the review while a problem stops Save.
    pub const STOPPED: &str = "Save is greyed out: vtium check-config refuses this configuration. Fix the problems listed first.";

    /// The message after Revert on the screen for a file changed on disk.
    pub const REVERTED: &str = "Reverted to the file on disk. [Ctrl+Z] brings your changes back.";

    /// The last line of the review: where the change takes effect.
    pub const TAKES_EFFECT: &str =
        "Each terminal uses the change at its next login. sudo asks for your password once.";

    /// `n` changes, as the review's title row counts them.
    pub fn changes(n: usize) -> String {
        match n {
            1 => "1 change".to_owned(),
            n => format!("{n} changes"),
        }
    }

    /// The done line after a save of `n` changes: what it wrote, and where
    /// vtium reads the file.
    ///
    /// References:
    /// - ADR-1201 D5: vtium reads vtium.conf when it starts on a VT.
    pub fn saved(n: usize) -> String {
        format!(
            "Saved {}: {}. vtium reads it when it starts on a VT; History keeps the old file. [F1] More",
            super::privileged::TARGET,
            changes(n)
        )
    }

    /// The line after a save that failed, from what the step said.
    pub fn not_saved(why: &str) -> String {
        let first = why.lines().next().unwrap_or_default().trim();
        format!("Nothing saved: {first} [F1] More")
    }

    /// Revert's question, which counts what is lost.
    pub fn revert_question(shared: &Shared) -> String {
        let lost = vtium_config_core::versions::revert::lost(shared.document());
        format!(
            "Reload the file from disk and drop your changes? {}",
            vtium_config_core::versions::words::lost(&lost)
        )
    }

    /// The question before Save saves a file with no complete route.
    ///
    /// References:
    /// - ADR-1403 D3: Save asks before it saves a file with no complete
    ///   route, and saves it on a yes.
    pub fn no_route_question(shared: &Shared) -> String {
        format!(
            "{} After saving, nobody at this machine may be able to type into vtium. Save anyway? {}.",
            vtium_config_core::words::NO_ROUTE,
            changes(shared.document().count())
        )
    }
}
