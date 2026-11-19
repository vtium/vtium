//! The Flow tab: the configuration drawn as paths of parts, the file view
//! over the draft, and Restore's list of the installed history. Its state
//! is where the highlight is, what is open over the document, the path
//! being made in NEW PATH, and the item of the flow that the file view
//! marks.
//!
//! References:
//! - ADR-1403 D7: the Flow view, its layers and its keys.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::desk::{Kept, Op};
use super::super::flow::{Device, Flow as Wiring, Item, New};
use super::super::flowview::{self, Obj, Slot};
use super::super::input::Key;
use super::super::model::{Pager, Shared, Size, Then, step};
use super::super::{flowdraw, parts, paths, words};
use super::{Command, Request};

/// Where the Flow tab stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    /// Where the highlight is on the document and what is open over it.
    pub(crate) state: flowview::State,
    /// The selected item of the flow, which the file view marks and a
    /// rebuild of the flow carries to the item that stands in its place.
    pub(crate) at: Item,
    /// The read-only file view over the draft, when it is open.
    pub(crate) pager: Option<Pager>,
    /// The installed history Restore lists, and the entry highlighted.
    pub(crate) restore: Option<(Vec<Kept>, usize)>,
    /// The item to select once the desk answers the change in flight.
    pub(crate) select_after: Option<Item>,
}

impl Flow {
    /// The tab at its start on `flow`, with its first item selected.
    pub fn new(flow: &Wiring) -> Flow {
        Flow {
            state: flowview::State::default(),
            at: first_item(flow),
            pager: None,
            restore: None,
            select_after: None,
        }
    }

    /// Where the highlight is on the document and what is open over it.
    pub fn state(&self) -> &flowview::State {
        &self.state
    }

    /// The selected item of the flow.
    pub fn at(&self) -> Item {
        self.at
    }

    /// The file view's place in the draft, when the file view is open.
    pub fn pager(&self) -> Option<Pager> {
        self.pager
    }

    /// Restore's list of the installed history and its highlighted entry,
    /// when the list is open.
    pub fn restore(&self) -> Option<&(Vec<Kept>, usize)> {
        self.restore.as_ref()
    }

    /// Carries the selected item over a rebuild of the flow from `old` to
    /// `new`: to `select` where `new` has it, else to the item of `new`
    /// that stands where the selected item stood in `old`.
    pub fn carry_over(&mut self, old: &Wiring, new: &Wiring, select: Option<Item>) {
        self.at = select
            .filter(|i| new.has(*i))
            .unwrap_or_else(|| carry(old, new, self.at));
    }

    /// Starts again on `flow`, the flow of a draft that replaces the one
    /// before: the first item selected, and the highlight on the first box
    /// of CONFIGURATION at the top of the document.
    pub fn restart(&mut self, flow: &Wiring) {
        self.at = first_item(flow);
        self.state.at = None;
        self.state.top = 0;
    }

    /// Sets the item to select once the desk answers the change in flight,
    /// or none.
    pub fn select_after(&mut self, select: Option<Item>) {
        self.select_after = select;
    }

    /// Takes the item to select now that the desk has answered.
    pub fn take_select(&mut self) -> Option<Item> {
        self.select_after.take()
    }

    /// Opens Restore's list on `kept`, the installed history, with its
    /// first entry highlighted.
    pub fn show_history(&mut self, kept: Vec<Kept>) {
        self.restore = Some((kept, 0));
    }

    /// Opens the pager at the line of the card selected in the flow.
    pub fn open_pager(&mut self, shared: &Shared, size: Size) {
        let Some(d) = &shared.draft else {
            return;
        };
        let mark = item_line(&shared.inputs.flow, &d.text, self.at);
        let room = size.rows.saturating_sub(4).max(1);
        self.pager = Some(Pager {
            top: mark.saturating_sub(room / 2).saturating_sub(1),
            left: 0,
            mark,
        });
    }

    /// A key in the pager: `j` `k` and the arrows scroll, `h` `l`
    /// sideways, `g` `G` to the top and the bottom, Ctrl-d Ctrl-u by half a
    /// page, and Esc back to the flow. `q` quits, as it does everywhere.
    /// `keys` is how many rows the key line takes in a terminal of `size`.
    pub(crate) fn pager_key(&mut self, shared: &Shared, size: Size, keys: usize, key: Key) {
        let lines = shared.draft.as_ref().map_or(0, |d| d.text.lines().count());
        // the draft's lines run from row 4 to the row above the key line
        let room = size.rows.saturating_sub(4).saturating_sub(keys).max(1);
        let last = lines.saturating_sub(room);
        let half = (room / 2).max(1);
        let Some(p) = self.pager.as_mut() else {
            return;
        };
        match key {
            Key::Escape => self.pager = None,
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
    /// Enter asks to copy the file into the draft, and Esc closes it. `q`
    /// quits, as it does everywhere.
    pub(crate) fn restore_key(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        let Some((kept, at)) = self.restore.as_mut() else {
            return Vec::new();
        };
        match key {
            Key::Escape => self.restore = None,
            Key::Enter => {
                let n = at.saturating_add(1);
                let restored = kept.get(*at).map(|k| k.text.clone()).unwrap_or_default();
                let now = shared
                    .draft
                    .as_ref()
                    .map(|d| d.text.clone())
                    .unwrap_or_default();
                let (added, removed) = crate::drafts::counts(&restored, &now);
                self.restore = None;
                return vec![Command::Request(Request::Ask {
                    question: words::losing_unsaved(
                        words::restore_question(n, added, removed),
                        shared.unsaved,
                    ),
                    then: Then::Desk(Op::Restore(n)),
                })];
            }
            _ => *at = step(*at, kept.len(), key),
        }
        Vec::new()
    }

    /// Lays the tab out again after every event: the parts enumerated
    /// again from the flow, a layer closed when its object is gone, the
    /// highlight back on its object, and the window where it keeps that
    /// object at the same height. `keys` is how many rows the key line
    /// takes in a terminal of `size`.
    pub fn settle(&mut self, shared: &Shared, size: Size, keys: usize) {
        // the parts, enumerated again and merged by their matches, so a
        // device that comes or goes after any event shows at once
        let flow = &shared.inputs.flow;
        let now = parts::enumerate(flow, &paths::read(flow));
        parts::merge(&mut self.state.parts, flow, now);
        let layout = flowdraw::layout_of(&self.state, shared, size.cols);
        if let Some(o @ Obj::Doc(..)) = &self.state.layer
            && layout.find(o).is_none()
        {
            self.state.layer = None;
            self.state.typing = None;
        }
        let (_, height) = flowdraw::window(shared, size, keys);
        self.state.settle(&layout, height);
    }

    /// The node of the interface tree the tab is at: a value being typed,
    /// the file view, Restore's list, a filter's picker, a layer open over
    /// the document, or the document itself.
    pub fn node(&self) -> &'static str {
        let flow = &self.state;
        if flow.typing.is_some() && flow.picker.is_some() {
            return "flow.maker.picker.typing";
        }
        if flow.typing.is_some() {
            return match flow.layer {
                Some(Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal)) => "flow.card.typing",
                _ => "flow.detail.typing",
            };
        }
        if self.pager.is_some() {
            return "flow.file";
        }
        if self.restore.is_some() {
            return "flow.restore";
        }
        if flow.picker.is_some() {
            return "flow.maker.picker";
        }
        match &flow.layer {
            None => "flow",
            Some(Obj::Doc(_, Slot::Terminal)) => "flow.card",
            Some(Obj::New(Slot::Terminal)) => "flow.new-card",
            Some(Obj::Pal(Slot::Template(_)) | Obj::New(Slot::Empty(_))) => "flow.maker",
            Some(Obj::Others) => "flow.others",
            Some(Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_)) => "flow.info",
            // a part's detail: its first line is the label, and the
            // lines below it are the choices
            Some(_) if flow.line == 0 => "flow.detail",
            Some(_) => "flow.detail.choices",
        }
    }
}

/// The item the Flow view selects first: the first display, else the
/// first item of the first column that has one.
pub(crate) fn first_item(flow: &Wiring) -> Item {
    flow.columns()
        .iter()
        .find_map(|c| {
            c.iter()
                .copied()
                .find(|i| *i != Item::Base && !matches!(i, Item::New(_)))
        })
        .unwrap_or(Item::Base)
}

/// The line of the draft `text`, counted from 1, that the file view opens
/// at for `item`: a rule's own line, a head's or a look's section heading,
/// and the first line for anything else.
fn item_line(flow: &Wiring, text: &str, item: Item) -> usize {
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
fn carry(old: &Wiring, new: &Wiring, item: Item) -> Item {
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
        .unwrap_or_else(|| first_item(new))
}
