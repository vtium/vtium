//! Update's changes to the draft and the desk's answers: the questions and
//! their answer keys, Quit, Save, the actions View file, Install, Revert,
//! Restore and New, and the devices read again at a hotplug or on `d`.
//! Every change is made through [`super::edit::Change`] on the draft's
//! text and handed to the desk to write. The flow is rebuilt from what the
//! desk answers, and the Flow tab carries its selected item over to the
//! rebuilt flow.
//!
//! References:
//! - ADR-1403 D7: the actions, their questions and the answer keys.
//! - ADR-1403 D8: an install writes the frits before vtium.conf.

use super::desk::{Done, Op};
use super::edit::Change;
use super::flow::{Draft, Flow, Item};
use super::input::Key;
use super::model::{App, Confirm, Effect, Then};
use super::words;
use crate::flowmodel::Present;

impl App {
    pub(crate) fn ask(&mut self, question: String, then: Then) {
        self.shared.message = Some(question.clone());
        self.nav.confirm = Some(Confirm { question, then });
    }

    fn say(&mut self, what: &str) -> Vec<Effect> {
        self.shared.message = Some(what.to_owned());
        Vec::new()
    }

    /// Closes the question with nothing done, as Esc does.
    fn cancel_question(&mut self, leaves: bool) -> Vec<Effect> {
        self.nav.confirm = None;
        self.shared.font_install = None;
        self.say(if leaves {
            words::NOT_LEFT
        } else {
            words::NOT_DONE
        })
    }

    /// A key while a question is asked. `y` does what the question asks.
    /// The quit question also takes `n`, which quits without the unsaved
    /// changes. Esc and Ctrl-C cancel the question and change nothing.
    /// Every other key does nothing, and the question stays, so a stray
    /// key cannot answer it.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": the answer keys of
    ///   every question, as the maintainer amended them on 2026-11-16.
    pub(crate) fn confirm_key(&mut self, key: Key) -> Vec<Effect> {
        let leaves = matches!(
            self.nav.confirm.as_ref().map(|c| &c.then),
            Some(Then::Quit(_))
        );
        // the question whether to let programs resize the grid takes `n`
        // as a no, as Esc is
        let no = super::nav::node(self) == "any.question.window-ops";
        match key {
            Key::Char('y') => {}
            Key::Char('n') if leaves => {}
            Key::Escape | Key::Interrupt => return self.cancel_question(leaves),
            Key::Char('n') if no => return self.cancel_question(leaves),
            _ => return Vec::new(),
        }
        let Some(confirm) = self.nav.confirm.take() else {
            return Vec::new();
        };
        self.shared.message = None;
        match confirm.then {
            Then::Quit(status) if key == Key::Char('n') => self.leave(status),
            Then::Quit(status) => self.save_and_leave(status),
            Then::Change(change) => self.make(vec![change], None),
            Then::Desk(op) => vec![Effect::Desk(op)],
            Then::Desks(ops) => ops.into_iter().map(Effect::Desk).collect(),
            Then::Install(plan) => {
                self.shared.installing = true;
                self.shared.message = Some(words::INSTALLING.to_owned());
                vec![Effect::Install(plan)]
            }
            Then::InstallFont => {
                self.shared.font_install = None;
                self.shared.installing = true;
                self.shared.message = Some(words::INSTALLING.to_owned());
                vec![Effect::InstallFont]
            }
        }
    }

    /// `q`: leaves, or asks the quit question first while changes wait
    /// for Save. The new and removed font sets that wait for an install
    /// stay in the draft's folder, so leaving loses none of them, and `q`
    /// asks nothing about them.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": what `q` asks.
    /// - ADR-1403 D8, "A draft's font set changes": the folder keeps the
    ///   font set changes, so quitting loses nothing.
    pub(crate) fn quit(&mut self, status: u8) -> Vec<Effect> {
        if self.shared.unsaved == 0 {
            return self.leave(status);
        }
        self.ask(
            words::quit_question(self.shared.unsaved),
            Then::Quit(status),
        );
        Vec::new()
    }

    /// Leaves with `status`, writing the unsaved changes into the draft
    /// first: Ctrl-C, a signal, and `y` to the question `q` asks.
    ///
    /// References:
    /// - ADR-1403 D7, "What carries over": a signal saves the unsaved
    ///   changes before the TUI leaves.
    pub(crate) fn save_and_leave(&mut self, status: u8) -> Vec<Effect> {
        // a TUI already leaving has answered for its changes
        if self.shared.leaving.is_some() {
            return self.leave(status);
        }
        let mut out = self.save_effect();
        out.extend(self.leave(status));
        out
    }

    /// The desk's write of the unsaved changes, where there are any.
    fn save_effect(&self) -> Vec<Effect> {
        match &self.shared.draft {
            Some(d) if self.shared.unsaved > 0 => vec![Effect::Desk(Op::Write(d.text.clone()))],
            _ => Vec::new(),
        }
    }

    /// `s`: writes the unsaved changes into the draft.
    ///
    /// References:
    /// - ADR-1403 D7: Save writes the changes into the draft.
    pub(crate) fn save(&mut self) -> Vec<Effect> {
        if self.shared.draft.is_none() {
            return self.say(words::NO_DESK);
        }
        if self.shared.unsaved == 0 {
            return self.say(words::NOTHING_TO_SAVE);
        }
        self.save_effect()
    }

    /// Makes `changes` on the draft, in order, and hands the result to the
    /// desk to check; all of them or, when one cannot be made, none. The
    /// changes wait for Save. The message line shows the commands that
    /// make the same changes, and `select` is the item to select once the
    /// draft is checked.
    ///
    /// References:
    /// - ADR-1403 D7: a change waits for Save.
    pub(crate) fn make(&mut self, changes: Vec<Change>, select: Option<Item>) -> Vec<Effect> {
        let text = match self.shared.apply(&changes) {
            Ok(text) => text,
            Err(why) => return self.say(&why),
        };
        let commands: Vec<String> = changes.iter().map(Change::command).collect();
        self.shared.message = Some(commands.join("\n"));
        self.flow.select_after(select);
        // one action of the person's is one change, whatever it edits
        self.shared.unsaved = self.shared.unsaved.saturating_add(1);
        // the draft holds the change at once, so a key that comes before
        // the check answers, such as `i` or `s`, acts on it
        if let Some(d) = self.shared.draft.as_mut() {
            d.text.clone_from(&text);
        }
        vec![Effect::Desk(Op::Check(text))]
    }

    /// The action keys `i`, `d`, `v`, `r`, `o` and `n`.
    ///
    /// References:
    /// - ADR-1403 D7: the action keys and the questions they ask.
    pub(crate) fn action(&mut self, key: char) -> Vec<Effect> {
        if self.shared.draft.is_none() {
            return self.say(words::NO_DESK);
        }
        let lost = self
            .shared
            .draft
            .as_ref()
            .map(|d| (d.added, d.removed))
            .unwrap_or_default();
        match key {
            // Install saves first (ADR-1403 D7, D8)
            'i' => {
                let mut out = self.save_effect();
                out.push(Effect::Desk(Op::Prepare));
                out
            }
            'd' => vec![Effect::Desk(Op::Devices)],
            'o' => vec![Effect::Desk(Op::History)],
            'v' => {
                self.flow.open_pager(&self.shared, self.size);
                Vec::new()
            }
            'r' => {
                self.ask(
                    words::losing_unsaved(
                        words::revert_question(lost.0, lost.1),
                        self.shared.unsaved,
                    ),
                    Then::Desk(Op::Revert),
                );
                Vec::new()
            }
            'n' => {
                self.ask(
                    words::losing_unsaved(words::NEW_QUESTION.to_owned(), self.shared.unsaved),
                    Then::Desk(Op::New),
                );
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// What the desk answered `op` with.
    pub(crate) fn desked(&mut self, op: &Op, done: Result<Done, String>) -> Vec<Effect> {
        let done = match done {
            Ok(done) => done,
            Err(why) => {
                self.flow.select_after(None);
                return self.say(&why);
            }
        };
        match done {
            Done::Draft(opened, present) => {
                if let Some(present) = present {
                    self.shared.present = present;
                }
                self.shared.came = None;
                // a check, and a font set kept in the draft's folder, keep
                // the unsaved changes; every other op leaves the draft as
                // the store holds it
                match op {
                    Op::Check(_) | Op::FontSet { .. } => {}
                    Op::Write(_) if self.shared.unsaved > 0 => {
                        self.shared.message =
                            Some(words::saved_draft(self.shared.unsaved, &opened.name));
                        self.shared.unsaved = 0;
                    }
                    _ => self.shared.unsaved = 0,
                }
                match op {
                    Op::Revert => self.shared.message = Some(words::REVERTED.to_owned()),
                    Op::New => self.shared.message = Some(words::NEW_DONE.to_owned()),
                    Op::Delete => self.shared.message = Some(words::deleted_draft(&opened.name)),
                    Op::Restore(_) => self.shared.message = Some(words::RESTORED.to_owned()),
                    _ => {}
                }
                // the marks follow the draft the desk left open, its
                // batch included, after every op that answers with it
                self.shared.inputs.effects.set_states(&opened.resources);
                self.shared.inputs.font_files.set_states(&opened.resources);
                self.shared.draft = Some(opened);
                let select = self.flow.take_select();
                self.rebuild(select);
                // the draft read at start, or one that replaces it: the
                // highlight goes to the first box of CONFIGURATION
                // (ADR-1403 D7, "This screen")
                if matches!(op, Op::Open | Op::Revert | Op::New | Op::Restore(_)) {
                    self.flow.restart(&self.shared.inputs.flow);
                }
            }
            // the desk's plan holds the draft's font sets from its folder,
            // frits first, so vtium.conf never names a frit that is not
            // there yet
            //
            // References:
            // - ADR-1403 D8: an install writes the frits before vtium.conf.
            Done::Plan(plan) => {
                let n = plan.font_sets();
                if plan.is_empty() {
                    return self.say(words::NOTHING_TO_INSTALL);
                }
                let (added, removed) = match plan.files.last() {
                    Some(f) if self.shared.draft.as_ref().is_some_and(|d| d.text == f.new) => self
                        .shared
                        .draft
                        .as_ref()
                        .map(|d| (d.added, d.removed))
                        .unwrap_or_default(),
                    _ => (0, 0),
                };
                // a draft whose only problem is that it wires no complete
                // route installs only after this question
                //
                // References:
                // - ADR-1403 D7: the question before such an install.
                // the changes to the active resources install with the
                // batch file, which the plan holds when they change it
                let batch = if plan.writes_batch() {
                    self.shared.draft.as_ref().map_or(0, |d| d.batch)
                } else {
                    0
                };
                let question = if plan.no_route {
                    words::no_route_question(added, removed, batch, n)
                } else {
                    words::install_question(added, removed, batch, n)
                };
                self.ask(question, Then::Install(plan));
            }
            Done::Refused(why, problems) => {
                let mut lines = vec![why];
                lines.extend(problems);
                self.shared.message = Some(lines.join("\n"));
            }
            Done::History(kept) if kept.is_empty() => return self.say(words::NO_HISTORY),
            Done::History(kept) => self.flow.show_history(kept),
            Done::Present { present, densities } => {
                self.shared.inputs.densities = densities;
                self.set_present(present);
            }
            Done::Resources {
                states,
                said,
                changes,
            } => {
                self.shared.inputs.effects.set_states(&states);
                self.shared.inputs.font_files.set_states(&states);
                // the draft keeps the desk's answer, so a later step that
                // brings the marks back from the draft, such as a system
                // font's install, keeps this change, and the status line
                // counts it at once
                if let Some(draft) = self.shared.draft.as_mut() {
                    draft.batch = changes;
                    draft.resources = states;
                }
                self.rebuild(None);
                return self.say(&said);
            }
            // a font of the person's readied as a system font: the
            // question shows the command before anything runs, and an
            // identical file needs nothing
            //
            // References:
            // - ADR-1205 D5: the exact command, asked each time; both files
            //   on a clash; nothing when they are identical.
            Done::SystemFont(ready) => {
                let question = match (ready.placement, &ready.facts) {
                    (crate::apply::Placement::Identical, _) => {
                        return self.say(&words::system_font_identical(&ready.target));
                    }
                    (_, Some(_)) => words::system_font_replace_question(&ready.target),
                    (_, None) => words::system_font_question(&ready.target),
                };
                // the font's detail shows the files and the command while
                // the question is open, since the message line is short
                self.shared.font_install = Some(ready);
                self.ask(question, Then::InstallFont);
            }
            // only an install answers with these, as [`App::installed`]
            Done::Installed(..) | Done::FontInstalled { .. } => {}
        }
        Vec::new()
    }

    /// The privileged step's end: the draft installed and recorded in the
    /// installed history, with the frits beside it, and what the step after
    /// the install did on the VTs (ADR-1201 D5), or why nothing is.
    pub(crate) fn installed(&mut self, result: Result<Done, String>) -> Vec<Effect> {
        self.shared.installing = false;
        let (opened, outcome) = match result {
            Ok(Done::Draft(opened, _)) => (opened, None),
            Ok(Done::Installed(opened, outcome)) => (opened, Some(outcome)),
            Ok(Done::FontInstalled {
                target,
                said,
                untrusted,
                found,
            }) => return self.font_installed(&target, &said, untrusted.as_deref(), found),
            Ok(_) => return Vec::new(),
            Err(why) => {
                return self.say(&words::install_failed(&why));
            }
        };
        let message = match &outcome {
            Some(outcome) => crate::vts::message(&opened.name, outcome),
            None => words::installed(&opened.name),
        };
        self.shared.inputs.effects.set_states(&opened.resources);
        self.shared.inputs.font_files.set_states(&opened.resources);
        // the font sets the install wrote and removed: those the folder
        // held before and no longer holds, since the install takes out of
        // the folder each change the system now matches
        let before = self.shared.frits().clone();
        let mut done = crate::store::FritChanges::default();
        for (name, text) in before.kept {
            if !opened.frits.kept.contains_key(&name) {
                done.kept.insert(name, text);
            }
        }
        for name in before.removed {
            if !opened.frits.removed.contains(&name) {
                done.removed.insert(name);
            }
        }
        // the Looks tab's place, read while the list still holds the font
        // sets only the old draft added
        let spot = self.looks.spot(&self.shared);
        self.shared.draft = Some(opened);
        self.frits_installed(done, spot);
        self.rebuild(None);
        self.say(&message)
    }

    /// The privileged step installed a font of the person's as the system
    /// font `target`: the Fonts tab lists the fonts found after it, the
    /// new system font among them, and the font helper describes the
    /// files it has not described yet. When the scan fails, the list stays
    /// as it was with the new system font added, and its note says why.
    /// Installing never activates the font, so the message line says how
    /// to, or, when the copy fails the trust rule, why vtium cannot use it.
    fn font_installed(
        &mut self,
        target: &std::path::Path,
        said: &str,
        untrusted: Option<&str>,
        found: Result<Vec<crate::discover::Found>, String>,
    ) -> Vec<Effect> {
        let highlighted = self.fonts.highlighted_path(&self.shared);
        match found {
            Ok(found) => self.shared.inputs.font_files.rescanned(found),
            Err(why) => self.shared.inputs.font_files.added_unscanned(target, &why),
        }
        // the install may have replaced a file of the same name, so the
        // helper describes the file at `target` again
        self.shared.inputs.font_files.forget(target);
        if let Some(draft) = &self.shared.draft {
            self.shared.inputs.font_files.set_states(&draft.resources);
        }
        self.fonts.keep(&self.shared, highlighted.as_deref());
        let target = crate::onscreen::path(target);
        let mut out = self.say(&match untrusted {
            Some(why) => words::system_font_untrusted(&target, why),
            None => format!("{} {said}", words::system_font_installed(&target)),
        });
        let waiting = self.shared.inputs.font_files.ask();
        if !waiting.is_empty() {
            out.push(Effect::Describe(waiting));
        }
        out
    }

    /// Takes the devices present, says what came and went, and redraws
    /// the flow on them.
    fn set_present(&mut self, present: Present) {
        let names = |flow: &Flow| -> Vec<String> {
            flow.displays
                .iter()
                .chain(&flow.inputs)
                .filter(|d| d.found)
                .map(|d| d.name.clone())
                .collect()
        };
        let before = names(&self.shared.inputs.flow);
        self.shared.present = present;
        self.rebuild(None);
        let after = names(&self.shared.inputs.flow);
        let came: Vec<String> = after
            .iter()
            .filter(|n| !before.contains(n))
            .cloned()
            .collect();
        let went: Vec<String> = before
            .iter()
            .filter(|n| !after.contains(n))
            .cloned()
            .collect();
        self.shared.came = Some(words::came_went(&came, &went));
    }

    /// Builds the flow again from the draft and the devices present, and
    /// has the Flow tab carry its selected item over to the new flow, to
    /// `select` where the new flow has it.
    fn rebuild(&mut self, select: Option<Item>) {
        let Some(d) = &self.shared.draft else {
            return;
        };
        let draft = Draft {
            name: d.name.clone(),
            changes: d.added.max(d.removed).saturating_add(d.batch),
            problems: d.count,
        };
        let new = crate::flowmodel::build(&d.text, &self.shared.present, Some(draft));
        let old = std::mem::replace(&mut self.shared.inputs.flow, new);
        self.flow.carry_over(&old, &self.shared.inputs.flow, select);
    }
}
