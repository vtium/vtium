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

    /// A key while a question is asked. `y` does what the question asks.
    /// The quit question also takes `n`, which quits without the unsaved
    /// changes, and the leave question takes `n`, which quits without
    /// installing. Esc and Ctrl-C cancel the question and change nothing.
    /// Every other key does nothing, and the question stays, so a stray
    /// key cannot answer it.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": the answer keys of
    ///   every question, as the maintainer amended them on 2026-11-16.
    pub(crate) fn confirm_key(&mut self, key: Key) -> Vec<Effect> {
        let leaves = matches!(
            self.nav.confirm.as_ref().map(|c| &c.then),
            Some(Then::Quit(_) | Then::Leave(_))
        );
        match key {
            Key::Char('y') => {}
            Key::Char('n') if leaves => {}
            Key::Escape | Key::Interrupt => {
                self.nav.confirm = None;
                // going back from the install the leave question started
                // leaves no install waiting
                self.shared.quit_after_install = None;
                return self.say(if leaves {
                    words::NOT_LEFT
                } else {
                    words::NOT_DONE
                });
            }
            _ => return Vec::new(),
        }
        let Some(confirm) = self.nav.confirm.take() else {
            return Vec::new();
        };
        self.shared.message = None;
        match confirm.then {
            Then::Quit(status) if key == Key::Char('n') => self.leave(status),
            Then::Quit(status) => self.save_and_leave(status),
            Then::Leave(status) if key == Key::Char('n') => self.save_and_leave(status),
            // Install saves first, and the TUI leaves once it ends well;
            // with no draft there is nothing to install, and nothing waits
            Then::Leave(status) => {
                if self.shared.draft.is_some() {
                    self.shared.quit_after_install = Some(status);
                }
                self.action('i')
            }
            Then::Change(change) => self.make(vec![change], None),
            Then::Desk(op) => vec![Effect::Desk(op)],
            Then::Install(plan) => {
                self.shared.installing = true;
                self.shared.message = Some(words::INSTALLING.to_owned());
                vec![Effect::Install(plan)]
            }
        }
    }

    /// `q`: leaves, or asks first. While changes wait for Save, it asks
    /// the quit question. While new frits or frit removals wait for an
    /// install, it asks the leave question instead, since leaving loses
    /// them. The leave question also names the changes that wait for
    /// Save, and its `y` saves them before the TUI leaves, as the quit
    /// question's does.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": what `q` asks, and
    ///   that the leave question names and saves the unsaved changes.
    pub(crate) fn quit(&mut self, status: u8) -> Vec<Effect> {
        if self.shared.pending.is_empty() && self.shared.unsaved > 0 {
            self.ask(
                words::quit_question(self.shared.unsaved),
                Then::Quit(status),
            );
            return Vec::new();
        }
        if self.shared.pending.is_empty() {
            return self.leave(status);
        }
        let added: Vec<String> = self
            .shared
            .pending
            .frits
            .iter()
            .map(|(n, _)| n.clone())
            .collect();
        let question =
            words::leave_question(&added, &self.shared.pending.removals, self.shared.unsaved);
        self.ask(question, Then::Leave(status));
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

    /// `s`: writes the unsaved changes into the draft (ADR-1403 D7).
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
    /// changes wait for Save (ADR-1403 D7). The message line shows the
    /// commands that make the same changes, and `select` is the item to
    /// select once the draft is checked.
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

    /// The action keys `i`, `d`, `v`, `r`, `o` and `n` (ADR-1403 D7).
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
        let tried = if matches!(op, Op::WriteChecked(_)) {
            self.shared.saving_tried.take()
        } else {
            None
        };
        let done = match done {
            Ok(done) => done,
            Err(why) => {
                self.flow.select_after(None);
                self.shared.quit_after_install = None;
                return self.say(&why);
            }
        };
        match done {
            Done::Draft(opened, present) => {
                if let Some(present) = present {
                    self.shared.present = present;
                }
                self.shared.came = None;
                // a check keeps the unsaved changes; every other op leaves
                // the draft as the store holds it
                match op {
                    Op::Check(_) => {}
                    Op::Write(_) if self.shared.unsaved > 0 => {
                        self.shared.message =
                            Some(words::saved_draft(self.shared.unsaved, &opened.name));
                        self.shared.unsaved = 0;
                    }
                    Op::WriteChecked(_) => {
                        self.shared.message = tried;
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
            Done::Plan(mut plan) => {
                // the frits go first, so vtium.conf never names a frit that
                // is not there yet (ADR-1403 D8)
                let frits = match self.frit_plan() {
                    Ok(frits) => frits,
                    Err(why) => {
                        self.shared.quit_after_install = None;
                        return self.say(&why);
                    }
                };
                let n = frits.files.len().saturating_add(frits.removals.len());
                let mut files = frits.files;
                files.append(&mut plan.files);
                plan.files = files;
                plan.removals = frits.removals;
                if plan.is_empty() {
                    return match self.shared.quit_after_install.take() {
                        Some(status) => self.leave(status),
                        None => self.say(words::NOTHING_TO_INSTALL),
                    };
                }
                // `y` at the leave question already said to install; a draft
                // with no complete route still asks its own question first
                if self.shared.quit_after_install.is_some() && !plan.no_route {
                    self.shared.installing = true;
                    self.shared.message = Some(words::INSTALLING.to_owned());
                    return vec![Effect::Install(plan)];
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
                // route installs only after this question (ADR-1403 D7)
                let question = if plan.no_route {
                    words::no_route_question(added, removed, n)
                } else {
                    words::install_question(added, removed, n)
                };
                self.ask(question, Then::Install(plan));
            }
            Done::Refused(why, problems) => {
                self.shared.quit_after_install = None;
                let mut lines = vec![why];
                lines.extend(problems);
                // the views beside the flow show one paragraph
                let sep = if matches!(op, Op::WriteChecked(_)) {
                    " "
                } else {
                    "\n"
                };
                self.shared.message = Some(lines.join(sep));
            }
            Done::History(kept) if kept.is_empty() => return self.say(words::NO_HISTORY),
            Done::History(kept) => self.flow.show_history(kept),
            Done::Present(present) => self.set_present(present),
            // only an install answers with this, as [`App::installed`]
            Done::Installed(..) => {}
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
            Ok(_) => return Vec::new(),
            Err(why) => {
                self.shared.quit_after_install = None;
                return self.say(&words::install_failed(&why));
            }
        };
        let message = match &outcome {
            Some(outcome) => crate::vts::message(&opened.name, outcome),
            None => words::installed(&opened.name),
        };
        self.shared.draft = Some(opened);
        self.frits_installed();
        self.rebuild(None);
        if let Some(status) = self.shared.quit_after_install.take() {
            self.shared.message = Some(message);
            return self.leave(status);
        }
        self.say(&message)
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
            changes: d.added.max(d.removed),
            problems: d.count,
        };
        let new = crate::flowmodel::build(&d.text, &self.shared.present, Some(draft));
        let old = std::mem::replace(&mut self.shared.inputs.flow, new);
        self.flow.carry_over(&old, &self.shared.inputs.flow, select);
    }
}
