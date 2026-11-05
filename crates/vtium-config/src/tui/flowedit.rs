//! The Flow view's changes to the draft (ADR-1403 D7), as the state
//! machine's: the opened card's keys, connecting tags, the actions View,
//! Install, Revert, Restore, New and Delete draft with their questions,
//! the pager, and the devices read again at a hotplug or on `d`. Every
//! change is made through [`super::edit::Change`] on the draft's text and
//! handed to the desk to write, and the flow is rebuilt from what the desk
//! answers, with the selection kept where it was.

use super::desk::{Done, Op};
use super::edit::{self, Change, Field, What};
use super::flow::{Device, Draft, Flow, Item, Kind, New};
use super::input::Key;
use super::model::{App, Card, Confirm, Effect, Live, Pager, Then, step, switch};
use super::words;
use crate::flowmodel::Present;

impl App {
    /// The frits a look may name: the fonts the TUI lists that read.
    fn frit_names(&self) -> Vec<String> {
        self.inputs
            .fonts
            .iter()
            .filter(|f| !matches!(f.state, super::model::FontState::Refused(_)))
            .map(|f| f.name.clone())
            .collect()
    }

    /// The fields of the selected item's card; none without a draft,
    /// whose card only shows the item.
    pub fn fields(&self) -> Vec<Field> {
        if self.draft.is_none() {
            return Vec::new();
        }
        edit::fields(
            &self.inputs.flow,
            self.flow_at,
            &self.card.values,
            &self.frit_names(),
        )
    }

    /// Opens the selected item's card, its cursor on the first field's
    /// value now.
    pub(crate) fn open_card(&mut self) {
        self.opened = true;
        self.card = Card::default();
        self.card_to_current();
    }

    /// Puts the cursor on the value the field under it has now.
    fn card_to_current(&mut self) {
        let fields = self.fields();
        if let Some(f) = fields.get(self.card.field) {
            self.card.pick = f.options.iter().position(|o| *o == f.current).unwrap_or(0);
        }
        self.card.typed = None;
    }

    /// A key on the opened card; `None` passes it to the flow, as for a
    /// card with nothing to edit.
    pub(crate) fn card_key(&mut self, key: Key) -> Option<Vec<Effect>> {
        let fields = self.fields();
        let field = fields.get(self.card.field)?.clone();
        if let Some(typed) = self.card.typed.clone() {
            return Some(match key {
                Key::Enter => self.choose(&field, &typed),
                Key::Escape => {
                    self.card.typed = None;
                    Vec::new()
                }
                _ => Vec::new(),
            });
        }
        let slots = field.options.len().saturating_add(usize::from(field.typed));
        match key {
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                self.card.field = step(self.card.field, fields.len(), key);
                self.card_to_current();
                Some(Vec::new())
            }
            Key::Left => {
                self.card.pick = self.card.pick.saturating_sub(1);
                Some(self.preview(&field))
            }
            Key::Right => {
                self.card.pick = self
                    .card
                    .pick
                    .saturating_add(1)
                    .min(slots.saturating_sub(1));
                Some(self.preview(&field))
            }
            Key::Enter => Some(match field.options.get(self.card.pick) {
                Some(value) => {
                    let value = value.clone();
                    self.choose(&field, &value)
                }
                None if field.typed => {
                    self.card.typed = Some(field.current.clone());
                    Vec::new()
                }
                None => Vec::new(),
            }),
            Key::Escape => {
                self.opened = false;
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// On a live vtium console, tries the frit the look card's cursor is
    /// on, as the Fonts view tries the highlighted font (ADR-1201 D4).
    fn preview(&mut self, field: &Field) -> Vec<Effect> {
        let What::LookKey(_, "font-name") = field.what else {
            return Vec::new();
        };
        let Some(name) = field.options.get(self.card.pick).cloned() else {
            return Vec::new();
        };
        if self.live != Live::On
            || self.in_flight.is_some()
            || self.shown_font.as_deref() == Some(name.as_str())
        {
            return Vec::new();
        }
        self.in_flight = Some(name.clone());
        vec![Effect::Write(switch(&name))]
    }

    /// Picks `value` in `field`: a form keeps it and moves on, an action
    /// acts, and any other field changes the draft.
    fn choose(&mut self, field: &Field, value: &str) -> Vec<Effect> {
        self.card.typed = None;
        match field.what {
            What::Form(i) => {
                if self.card.values.len() <= i {
                    self.card.values.resize(i.saturating_add(1), String::new());
                }
                if let Some(v) = self.card.values.get_mut(i) {
                    value.clone_into(v);
                }
                self.card.field = self.card.field.saturating_add(1);
                self.card_to_current();
                Vec::new()
            }
            What::Actions => self.card_action(value),
            _ => match edit::pick(&self.inputs.flow, field, value) {
                Some(change) => self.make(vec![change], None),
                None => Vec::new(),
            },
        }
    }

    /// An action on the card's last row: move a rule, add a `new…` form,
    /// or delete the item, asking first.
    fn card_action(&mut self, action: &str) -> Vec<Effect> {
        let flow = &self.inputs.flow;
        match (action, self.flow_at) {
            (edit::UP | edit::DOWN, Item::Rule(r)) => {
                let Some(kind) = flow.rules.get(r).map(|x| x.kind) else {
                    return Vec::new();
                };
                let same: Vec<usize> = flow.rules_of(kind).map(|(i, _)| i).collect();
                let at = same.iter().position(|i| *i == r).unwrap_or(0);
                let (to, before) = if action == edit::UP {
                    match at.checked_sub(1).and_then(|p| same.get(p)) {
                        Some(&p) => (p, p.saturating_add(1)),
                        None => return self.say(words::RULE_FIRST),
                    }
                } else {
                    match same.get(at.saturating_add(1)) {
                        Some(&n) => (n, n.saturating_add(2)),
                        None => return self.say(words::RULE_LAST),
                    }
                };
                self.make(
                    vec![Change::RuleMove {
                        number: r.saturating_add(1),
                        before,
                    }],
                    Some(Item::Rule(to)),
                )
            }
            (edit::ADD, Item::New(new)) => match edit::add(new, &self.card.values) {
                Ok(change) => {
                    self.card = Card::default();
                    self.opened = false;
                    self.make(vec![change], None)
                }
                Err(why) => self.say(&why),
            },
            (edit::DELETE, item) => {
                let Some((question, change)) = self.delete_question(item) else {
                    return Vec::new();
                };
                self.ask(question, Then::Change(change));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The question a delete of `item` asks, naming what still uses it,
    /// and the change a yes makes.
    fn delete_question(&self, item: Item) -> Option<(String, Change)> {
        let flow = &self.inputs.flow;
        match item {
            Item::Rule(r) => {
                flow.rules.get(r)?;
                let n = r.saturating_add(1);
                Some((
                    words::delete_rule(n, &rule_line(flow, r)),
                    Change::RuleRm(n),
                ))
            }
            Item::Head(h) => {
                let head = flow.heads.get(h)?;
                Some((
                    words::delete_head(&head.name, &edit::rules_to(flow, &head.name)),
                    Change::HeadRm(head.name.clone()),
                ))
            }
            Item::HeadLook(_) | Item::Look(_) => {
                let look = edit::look_at(flow, item).and_then(|l| flow.looks.get(l))?;
                let users: Vec<String> = flow
                    .heads
                    .iter()
                    .filter(|h| h.look == look.name)
                    .map(|h| h.name.clone())
                    .collect();
                Some((
                    words::delete_look(&look.name, &users),
                    Change::LookRm(look.name.clone()),
                ))
            }
            Item::Display(_) | Item::Input(_) => {
                let r = edit::placeholder_rule(flow, item)?;
                let n = r.saturating_add(1);
                Some((
                    words::delete_placeholder(&flow.name(item), n, &rule_line(flow, r)),
                    Change::RuleRm(n),
                ))
            }
            _ => None,
        }
    }

    /// Asks `question`, which a yes answers with `then`.
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
            Then::Leave(_) => Vec::new(),
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
        if self.pending.is_empty() {
            return self.leave(status);
        }
        let added: Vec<String> = self.pending.frits.iter().map(|(n, _)| n.clone()).collect();
        let question = words::leave_question(&added, &self.pending.removals);
        self.ask(question, Then::Leave(status));
        Vec::new()
    }

    /// Makes `changes` on the draft, in order, and hands the result to the
    /// desk to write; all of them or, when one cannot be made, none. The
    /// message line shows the commands that make the same changes, and
    /// `select` is the item to select once the draft is written.
    pub(crate) fn make(&mut self, changes: Vec<Change>, select: Option<Item>) -> Vec<Effect> {
        let Some(draft) = &self.draft else {
            return self.say(words::NO_DESK);
        };
        let mut text = draft.text.clone();
        for change in &changes {
            match change.apply(&text) {
                Ok(next) => text = next,
                Err(fail) => return self.say(fail.text()),
            }
        }
        let commands: Vec<String> = changes.iter().map(Change::command).collect();
        self.message = Some(commands.join("\n"));
        self.select_after = select;
        vec![Effect::Desk(Op::Write(text))]
    }

    /// `c`: turns the tags into changes in the draft, writes them, and
    /// clears the tags; or says what is missing or does not fit.
    pub(crate) fn connect(&mut self) -> Vec<Effect> {
        match edit::connect(&self.inputs.flow, &self.tags) {
            Err(why) => self.say(&why),
            Ok(changes) if changes.is_empty() => {
                self.tags.clear();
                self.say(words::CONNECT_NO_CHANGE)
            }
            Ok(mut changes) => {
                // a new rule shifts the numbers of the rules after it, so
                // the changes to rules by number go first
                changes.sort_by_key(|c| match c {
                    Change::RuleSet { .. } => 0,
                    Change::RuleAdd(_) => 1,
                    _ => 2,
                });
                let out = self.make(changes, None);
                if !out.is_empty() {
                    self.tags.clear();
                }
                out
            }
        }
    }

    /// The action keys `i`, `d`, `v`, `r`, `o`, `n` and `x` (ADR-1403 D7).
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
            'i' => vec![Effect::Desk(Op::Prepare)],
            'd' => vec![Effect::Desk(Op::Devices)],
            'o' => vec![Effect::Desk(Op::History)],
            'v' => {
                self.open_pager();
                Vec::new()
            }
            'r' => {
                self.ask(
                    words::revert_question(lost.0, lost.1),
                    Then::Desk(Op::Revert),
                );
                Vec::new()
            }
            'n' => {
                self.ask(words::NEW_QUESTION.to_owned(), Then::Desk(Op::New));
                Vec::new()
            }
            'x' => {
                self.ask(
                    words::delete_draft_question(lost.0, lost.1),
                    Then::Desk(Op::Delete),
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
                if *op == Op::Open {
                    self.flow_at = super::model::first_item(&self.inputs.flow);
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
                self.ask(
                    words::install_question(added, removed, n),
                    Then::Install(plan),
                );
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
    /// keeps the selection and the tags on the items they were on.
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
        let tags: Vec<Item> = self
            .tags
            .iter()
            .filter_map(|t| Some(carry(&old, new, *t)).filter(|n| new.has(*n)))
            .collect();
        self.flow_at = at;
        self.tags = tags;
        if !self.fields().is_empty() {
            let fields = self.fields().len();
            self.card.field = self.card.field.min(fields.saturating_sub(1));
        }
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
        let room = self.rows.saturating_sub(4).max(1);
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
                    words::restore_question(n, added, removed),
                    Then::Desk(Op::Restore(n)),
                );
            }
            _ => *at = step(*at, kept.len(), key),
        }
        Vec::new()
    }
}

/// Rule `r`'s line as the file writes it, `KIND SELECTOR -> TARGET`.
fn rule_line(flow: &Flow, r: usize) -> String {
    flow.rules.get(r).map_or_else(String::new, |rule| {
        let kind = match rule.kind {
            Kind::Display => "display",
            Kind::Input => "input",
        };
        format!("{kind} {} -> {}", rule.selector, rule.target)
    })
}

/// The line of the draft `text`, counted from 1, that `item`'s card
/// shows: a rule's own line, a head's or a look's section heading, and
/// the first line for anything else.
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
        Item::HeadLook(_) | Item::Look(_) => edit::look_at(flow, item)
            .and_then(|l| flow.looks.get(l))
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
