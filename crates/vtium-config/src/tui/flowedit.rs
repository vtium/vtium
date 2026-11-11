//! The Flow view's changes to the draft (ADR-1403 D7, D8), as the state
//! machine's: the actions View file, Install, Revert, Restore and New with
//! their questions, the file view, and the devices read again at a hotplug
//! or on `d`. Every change is made through [`super::edit::Change`] on the
//! draft's text and handed to the desk to write, and the flow is rebuilt
//! from what the desk answers.

use super::desk::{Done, Op};
use super::edit::Change;
use super::flow::{Device, Draft, Flow, Item, New};
use super::input::Key;
use super::model::{App, Confirm, Effect, Pager, Then, step};
use super::words;
use crate::flowmodel::Present;

impl App {
    fn ask(&mut self, question: String, then: Then) {
        self.message = Some(question.clone());
        self.confirm = Some(Confirm { question, then });
    }

    fn say(&mut self, what: &str) -> Vec<Effect> {
        self.message = Some(what.to_owned());
        Vec::new()
    }

    /// A key while a question is asked: `y` acts, any other key does not.
    pub(crate) fn confirm_key(&mut self, key: Key) -> Vec<Effect> {
        let Some(confirm) = self.confirm.take() else {
            return Vec::new();
        };
        if let Then::Quit(status) = confirm.then {
            self.message = None;
            return match key {
                Key::Char('y') => self.save_and_leave(status),
                Key::Char('n') => self.leave(status),
                _ => self.say(words::NOT_LEFT),
            };
        }
        if let Then::Leave(status) = confirm.then {
            self.message = None;
            return match key {
                Key::Char('l') => self.leave(status),
                Key::Char('i') => self.action('i'),
                _ => self.say(words::NOT_LEFT),
            };
        }
        if key != Key::Char('y') {
            return self.say(words::NOT_DONE);
        }
        self.message = None;
        match confirm.then {
            Then::Change(change) => self.make(vec![change], None),
            Then::Desk(op) => vec![Effect::Desk(op)],
            Then::Leave(_) | Then::Quit(_) => Vec::new(),
            Then::Install(plan) => {
                self.installing = true;
                self.message = Some(words::INSTALLING.to_owned());
                vec![Effect::Install(plan)]
            }
        }
    }

    /// `q` or Ctrl-C: leaves, or, while new frits or frit removals wait
    /// for an install, asks first, since leaving loses them (ADR-1403
    /// D7): `i` installs, `l` leaves anyway, any other key goes back.
    pub(crate) fn quit(&mut self, status: u8) -> Vec<Effect> {
        if self.pending.is_empty() && self.unsaved > 0 {
            self.ask(words::quit_question(self.unsaved), Then::Quit(status));
            return Vec::new();
        }
        if self.pending.is_empty() {
            return self.leave(status);
        }
        let added: Vec<String> = self.pending.frits.iter().map(|(n, _)| n.clone()).collect();
        let question = words::leave_question(&added, &self.pending.removals);
        self.ask(question, Then::Leave(status));
        Vec::new()
    }

    /// Leaves with `status`, writing the unsaved changes into the draft
    /// first: Ctrl-C, a signal, and `y` to the question `q` asks (ADR-1403
    /// D7).
    pub(crate) fn save_and_leave(&mut self, status: u8) -> Vec<Effect> {
        // a TUI already leaving has answered for its changes
        if self.leaving.is_some() {
            return self.leave(status);
        }
        let mut out = self.save_effect();
        out.extend(self.leave(status));
        out
    }

    /// The desk's write of the unsaved changes, where there are any.
    fn save_effect(&self) -> Vec<Effect> {
        match &self.draft {
            Some(d) if self.unsaved > 0 => vec![Effect::Desk(Op::Write(d.text.clone()))],
            _ => Vec::new(),
        }
    }

    /// `s`: writes the unsaved changes into the draft (ADR-1403 D7).
    pub(crate) fn save(&mut self) -> Vec<Effect> {
        if self.draft.is_none() {
            return self.say(words::NO_DESK);
        }
        if self.unsaved == 0 {
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
        let Some(draft) = &self.draft else {
            return self.say(words::NO_DESK);
        };
        let text = match super::edit::apply_all(&draft.text, &changes) {
            Ok(text) => text,
            Err(fail) => return self.say(fail.text()),
        };
        let commands: Vec<String> = changes.iter().map(Change::command).collect();
        self.message = Some(commands.join("\n"));
        self.select_after = select;
        // one action of the person's is one change, whatever it edits
        self.unsaved = self.unsaved.saturating_add(1);
        // the draft holds the change at once, so a key that comes before
        // the check answers, such as `i` or `s`, acts on it
        if let Some(d) = self.draft.as_mut() {
            d.text.clone_from(&text);
        }
        vec![Effect::Desk(Op::Check(text))]
    }

    /// The action keys `i`, `d`, `v`, `r`, `o` and `n` (ADR-1403 D7).
    pub(crate) fn action(&mut self, key: char) -> Vec<Effect> {
        if self.draft.is_none() {
            return self.say(words::NO_DESK);
        }
        let lost = self
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
                self.open_pager();
                Vec::new()
            }
            'r' => {
                self.ask(
                    words::losing_unsaved(words::revert_question(lost.0, lost.1), self.unsaved),
                    Then::Desk(Op::Revert),
                );
                Vec::new()
            }
            'n' => {
                self.ask(
                    words::losing_unsaved(words::NEW_QUESTION.to_owned(), self.unsaved),
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
                self.select_after = None;
                return self.say(&why);
            }
        };
        match done {
            Done::Draft(opened, present) => {
                if let Some(present) = present {
                    self.present = present;
                }
                self.came = None;
                // a check keeps the unsaved changes; every other op leaves
                // the draft as the store holds it
                match op {
                    Op::Check(_) => {}
                    Op::Write(_) if self.unsaved > 0 => {
                        self.message = Some(words::saved_draft(self.unsaved, &opened.name));
                        self.unsaved = 0;
                    }
                    _ => self.unsaved = 0,
                }
                match op {
                    Op::Revert => self.message = Some(words::REVERTED.to_owned()),
                    Op::New => self.message = Some(words::NEW_DONE.to_owned()),
                    Op::Delete => self.message = Some(words::deleted_draft(&opened.name)),
                    Op::Restore(_) => self.message = Some(words::RESTORED.to_owned()),
                    _ => {}
                }
                self.draft = Some(opened);
                let select = self.select_after.take();
                self.rebuild(select);
                // the draft read at start, or one that replaces it: the
                // highlight goes to the first box of CONFIGURATION
                // (ADR-1403 D7, "This screen")
                if matches!(op, Op::Open | Op::Revert | Op::New | Op::Restore(_)) {
                    self.flow_at = super::model::first_item(&self.inputs.flow);
                    self.flow_view.at = None;
                    self.flow_view.top = 0;
                }
            }
            Done::Plan(mut plan) => {
                // the frits go first, so vtium.conf never names a frit that
                // is not there yet (ADR-1403 D8)
                let frits = match self.frit_plan() {
                    Ok(frits) => frits,
                    Err(why) => return self.say(&why),
                };
                let n = frits.files.len().saturating_add(frits.removals.len());
                let mut files = frits.files;
                files.append(&mut plan.files);
                plan.files = files;
                plan.removals = frits.removals;
                if plan.is_empty() {
                    return self.say(words::NOTHING_TO_INSTALL);
                }
                let (added, removed) = match plan.files.last() {
                    Some(f) if self.draft.as_ref().is_some_and(|d| d.text == f.new) => self
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
                let mut lines = vec![why];
                lines.extend(problems);
                self.message = Some(lines.join("\n"));
            }
            Done::History(kept) if kept.is_empty() => return self.say(words::NO_HISTORY),
            Done::History(kept) => self.restore = Some((kept, 0)),
            Done::Present(present) => self.set_present(present),
        }
        Vec::new()
    }

    /// The privileged step's end: the draft installed and recorded in the
    /// installed history, with the frits beside it, or why nothing is.
    pub(crate) fn installed(&mut self, result: Result<Done, String>) -> Vec<Effect> {
        self.installing = false;
        match result {
            Ok(Done::Draft(opened, _)) => {
                let name = opened.name.clone();
                self.draft = Some(opened);
                self.frits_installed();
                self.rebuild(None);
                self.say(&words::installed(&name))
            }
            Ok(_) => Vec::new(),
            Err(why) => self.say(&words::install_failed(&why)),
        }
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
        let before = names(&self.inputs.flow);
        self.present = present;
        self.rebuild(None);
        let after = names(&self.inputs.flow);
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
        self.came = Some(words::came_went(&came, &went));
    }

    /// Builds the flow again from the draft and the devices present, and
    /// keeps the item the file view marks on the item it was on.
    fn rebuild(&mut self, select: Option<Item>) {
        let Some(d) = &self.draft else {
            return;
        };
        let draft = Draft {
            name: d.name.clone(),
            changes: d.added.max(d.removed),
            problems: d.count,
        };
        let new = crate::flowmodel::build(&d.text, &self.present, Some(draft));
        let old = std::mem::replace(&mut self.inputs.flow, new);
        let new = &self.inputs.flow;
        let at = select
            .filter(|i| new.has(*i))
            .unwrap_or_else(|| carry(&old, new, self.flow_at));
        self.flow_at = at;
    }

    /// Opens the pager at the line of the card selected in the flow.
    fn open_pager(&mut self) {
        let Some(d) = &self.draft else {
            return;
        };
        let mark = item_line(&self.inputs.flow, &d.text, self.flow_at);
        let room = self.rows.saturating_sub(4).max(1);
        self.pager = Some(Pager {
            top: mark.saturating_sub(room / 2).saturating_sub(1),
            left: 0,
            mark,
        });
    }

    /// A key in the pager: `j` `k` and the arrows scroll, `h` `l`
    /// sideways, `g` `G` to the top and the bottom, Ctrl-d Ctrl-u by half a
    /// page, `q` and Esc back to the flow.
    pub(crate) fn pager_key(&mut self, key: Key) {
        let lines = self.draft.as_ref().map_or(0, |d| d.text.lines().count());
        // the draft's lines run from row 4 to the row above the key line
        let room = self.rows.saturating_sub(5).max(1);
        let last = lines.saturating_sub(room);
        let half = (room / 2).max(1);
        let Some(p) = self.pager.as_mut() else {
            return;
        };
        match key {
            Key::Char('q') | Key::Escape => self.pager = None,
            Key::Char('j') | Key::Down => p.top = p.top.saturating_add(1).min(last),
            Key::Char('k') | Key::Up => p.top = p.top.saturating_sub(1),
            Key::Char('l') | Key::Right => p.left = p.left.saturating_add(8),
            Key::Char('h') | Key::Left => p.left = p.left.saturating_sub(8),
            Key::Char('g') | Key::Home => p.top = 0,
            Key::Char('G') | Key::End => p.top = last,
            Key::Ctrl('d') | Key::PageDown => p.top = p.top.saturating_add(half).min(last),
            Key::Ctrl('u') | Key::PageUp => p.top = p.top.saturating_sub(half),
            _ => {}
        }
    }

    /// A key on Restore's list of the installed history: Up and Down move,
    /// Enter asks to copy the file into the draft, Esc and `q` close it.
    pub(crate) fn restore_key(&mut self, key: Key) -> Vec<Effect> {
        let Some((kept, at)) = self.restore.as_mut() else {
            return Vec::new();
        };
        match key {
            Key::Char('q') | Key::Escape => self.restore = None,
            Key::Enter => {
                let n = at.saturating_add(1);
                let restored = kept.get(*at).map(|k| k.text.clone()).unwrap_or_default();
                let now = self
                    .draft
                    .as_ref()
                    .map(|d| d.text.clone())
                    .unwrap_or_default();
                let (added, removed) = crate::drafts::counts(&restored, &now);
                self.restore = None;
                self.ask(
                    words::losing_unsaved(words::restore_question(n, added, removed), self.unsaved),
                    Then::Desk(Op::Restore(n)),
                );
            }
            _ => *at = step(*at, kept.len(), key),
        }
        Vec::new()
    }
}

/// The line of the draft `text`, counted from 1, that the file view opens
/// at for `item`: a rule's own line, a head's or a look's section heading,
/// and the first line for anything else.
fn item_line(flow: &Flow, text: &str, item: Item) -> usize {
    let heading = |want: String| {
        text.lines()
            .position(|l| l.trim() == want)
            .map_or(1, |i| i.saturating_add(1))
    };
    match item {
        Item::Rule(r) => flow.rules.get(r).map_or(1, |r| r.line),
        Item::Head(h) => flow
            .heads
            .get(h)
            .map_or(1, |h| heading(format!("[head {}]", h.name))),
        Item::HeadLook(h) => flow
            .look_of(h)
            .and_then(|l| flow.looks.get(l))
            .map_or(1, |l| heading(format!("[look {}]", l.name))),
        Item::Look(l) => flow
            .looks
            .get(l)
            .map_or(1, |l| heading(format!("[look {}]", l.name))),
        Item::Display(_) | Item::Input(_) => {
            let rule = match item {
                Item::Display(d) => flow.displays.get(d).and_then(|d| d.rule),
                Item::Input(d) => flow.inputs.get(d).and_then(|d| d.rule),
                _ => None,
            };
            rule.and_then(|r| flow.rules.get(r)).map_or(1, |r| r.line)
        }
        _ => 1,
    }
}

/// The item of `new` that stands where `item` stood in `old`: the same
/// device by its match keys, the same head or look by its name; a device that left becomes its
/// `configured, not found` block, or its neighbor in the column; a rule
/// keeps its number.
fn carry(old: &Flow, new: &Flow, item: Item) -> Item {
    let device = |olds: &[Device], news: &[Device], i: usize, make: fn(usize) -> Item| {
        let dev = olds.get(i)?;
        // a device is itself by its match keys, which tell apart two
        // devices of one name; among several alike, the one at its place
        let same: Vec<usize> = news
            .iter()
            .enumerate()
            .filter(|(_, d)| d.keys == dev.keys && d.name == dev.name && d.found == dev.found)
            .map(|(k, _)| k)
            .collect();
        same.iter()
            .copied()
            .find(|k| *k == i)
            .or_else(|| same.first().copied())
            .or_else(|| {
                news.iter()
                    .position(|d| !d.found && d.rule.is_some() && d.rule == dev.rule)
            })
            .or_else(|| (!news.is_empty()).then(|| i.min(news.len().saturating_sub(1))))
            .map(make)
    };
    let carried = match item {
        Item::Display(i) => device(&old.displays, &new.displays, i, Item::Display),
        Item::Input(i) => device(&old.inputs, &new.inputs, i, Item::Input),
        Item::Head(h) | Item::HeadLook(h) => {
            let name = old.heads.get(h).map(|x| x.name.as_str());
            let at = new
                .heads
                .iter()
                .position(|x| Some(x.name.as_str()) == name)
                .or_else(|| {
                    (!new.heads.is_empty()).then(|| h.min(new.heads.len().saturating_sub(1)))
                });
            match (item, at) {
                (Item::Head(_), Some(at)) => Some(Item::Head(at)),
                (_, Some(at)) => Some(Item::HeadLook(at)),
                (_, None) => Some(Item::New(New::Head)),
            }
        }
        Item::Look(l) => {
            let name = old.looks.get(l).map(|x| x.name.clone());
            new.looks
                .iter()
                .position(|x| Some(&x.name) == name.as_ref())
                .map(|at| {
                    if new.used(at) {
                        new.heads
                            .iter()
                            .position(|h| Some(&h.look) == name.as_ref())
                            .map_or(Item::Look(at), Item::HeadLook)
                    } else {
                        Item::Look(at)
                    }
                })
        }
        Item::Rule(r) => {
            let kind = old.rules.get(r).map(|x| x.kind);
            let same = new.rules.get(r).map(|x| x.kind) == kind;
            if same {
                Some(Item::Rule(r))
            } else {
                new.rules
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| Some(x.kind) == kind)
                    .map(|(i, _)| i)
                    .rfind(|i| *i < r)
                    .map(Item::Rule)
            }
        }
        other => Some(other),
    };
    carried
        .filter(|i| new.has(*i))
        .unwrap_or_else(|| super::model::first_item(new))
}
