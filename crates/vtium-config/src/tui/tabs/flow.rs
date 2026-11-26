//! The Flow tab: the configuration drawn as paths of parts, the file view
//! over the draft, and Restore's list of the installed history. Its state
//! is where the highlight is, what is open over the document, the path
//! being made in NEW PATH, and the item of the flow that the file view
//! marks.
//!
//! The arrows move the highlight from box to box, and PgUp and PgDn
//! scroll by a window. Space picks a piece into the editor or takes it
//! out, `a` adds the editor's path to the document, `e` revises an added
//! path, which moves it back into the editor, and Esc clears the editor.
//! Enter opens a box's layer: an added terminal's card of the settings
//! that do not change its pieces, a piece's narrowing choices, the editor
//! terminal's name and font set, or a font set's preview in the Fonts
//! view. `v` shows the draft's vtium.conf as text, read-only, in the file
//! view, where `s` and `i` save and install as they do on the paths, and
//! Esc there comes back with the same box highlighted.
//!
//! The tab changes the draft only by returning [`Change::Make`] with the
//! changes [`paths::plan`] made, which update makes through `src/ops.rs`.
//!
//! References:
//! - ADR-1403 D7: the Flow view, its layers and its keys.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::desk::{Kept, Op};
use super::super::edit::Change as DraftChange;
use super::super::flow::{Device, Flow as Wiring, Item, Kind, New};
use super::super::flowview::{self, Obj, Slot, State};
use super::super::input::Key;
use super::super::listbox::ListBox;
use super::super::model::{Live, Pager, Shared, Size, Then, step};
use super::super::parts::{self, Make, Pick, Stage};
use super::super::paths::{self, Adding, Doc, Path, Picked, Piece};
use super::super::{flowlayout, words};
use super::{Change, Command, Request};
use super::{Room, Tab};

/// Where the Flow tab stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    /// Where the highlight is on the document and what is open over it.
    state: flowview::State,
    /// The selected item of the flow, which the file view marks and a
    /// rebuild of the flow carries to the item that stands in its place.
    at: Item,
    /// The read-only file view over the draft, when it is open.
    pager: Option<Pager>,
    /// The installed history Restore lists, the entry highlighted, and
    /// the first entry its window shows.
    restore: Option<(Vec<Kept>, ListBox)>,
    /// The item to select once the desk answers the change in flight.
    select_after: Option<Item>,
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

    /// Restore's list of the installed history, with its highlighted
    /// entry and the first entry its window shows, when the list is open.
    pub fn restore(&self) -> Option<(&[Kept], ListBox)> {
        self.restore
            .as_ref()
            .map(|(kept, list)| (kept.as_slice(), *list))
    }

    /// Starts Restore's window at the entry `first`, which the layout
    /// gives so the highlighted entry shows.
    pub fn restore_show_from(&mut self, first: usize) {
        if let Some((_, list)) = self.restore.as_mut() {
            list.offset = first;
        }
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
        self.restore = Some((kept, ListBox::at(0)));
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
        let Some((kept, list)) = self.restore.as_mut() else {
            return Vec::new();
        };
        match key {
            Key::Escape => self.restore = None,
            Key::Enter => {
                let n = list.at.saturating_add(1);
                let restored = kept
                    .get(list.at)
                    .map(|k| k.text.clone())
                    .unwrap_or_default();
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
            _ => list.at = step(list.at, kept.len(), key),
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
        let layout = flowlayout::layout_of(&self.state, shared, size.cols);
        if let Some(o @ Obj::Doc(..)) = &self.state.layer
            && layout.find(o).is_none()
        {
            self.state.layer = None;
            self.state.typing = None;
        }
        let (_, height) = flowlayout::window(shared, size, keys);
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

impl Flow {
    /// A key in the Flow tab. `keys` is how many rows the key line takes
    /// in a terminal of `size`.
    pub fn update(&mut self, shared: &Shared, size: Size, keys: usize, key: Key) -> Vec<Command> {
        let state = self.state.clone();
        // the file view saves and installs as the paths do, and stays open
        if self.pager.is_some() {
            match key {
                Key::Char('s') => return change(Change::Save),
                Key::Char('i') => return change(Change::Action('i')),
                _ => {}
            }
        }
        if self.pager.is_some() {
            self.pager_key(shared, size, keys, key);
            return Vec::new();
        }
        if self.restore.is_some() {
            return self.restore_key(shared, key);
        }
        match &state.layer {
            Some(obj @ (Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal))) => {
                self.terminal_card_key(shared, &state, obj, key)
            }
            Some(Obj::Pal(Slot::Template(st)) | Obj::New(Slot::Empty(st))) => {
                let from_new = matches!(state.layer, Some(Obj::New(_)));
                self.make_key(shared, &state, *st, from_new, key)
            }
            Some(obj @ (Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_))) => match key {
                Key::Char('e') => self.revise(shared, obj),
                Key::Escape => {
                    self.state.layer = None;
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Some(Obj::Others) => {
                if key == Key::Escape {
                    self.state.layer = None;
                }
                Vec::new()
            }
            Some(obj) => self.choice_key(shared, &state, obj, key),
            None => self.overview_key(shared, size, keys, &state, key),
        }
    }

    /// A key on the document.
    fn overview_key(
        &mut self,
        shared: &Shared,
        size: Size,
        keys: usize,
        state: &State,
        key: Key,
    ) -> Vec<Command> {
        let layout = flowlayout::layout_of(&self.state, shared, size.cols);
        let (_, height) = flowlayout::window(shared, size, keys);
        let at = state.at.clone();
        let mut out = Vec::new();
        if !matches!(key, Key::Char(' ' | 'a' | 'e' | 'p' | 't' | 's')) {
            out.push(Command::Change(Change::Quiet));
        }
        out.extend(match key {
            Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End => {
                if let Some(to) = at.as_ref().and_then(|a| flowview::step(&layout, a, key)) {
                    self.state.go(&layout, to, height);
                }
                Vec::new()
            }
            Key::PageUp | Key::PageDown => {
                let down = key == Key::PageDown;
                self.state.page(&layout, down, height);
                Vec::new()
            }
            Key::Enter => self.open(shared, at),
            Key::Char(' ') => self.space(shared, state, at),
            Key::Char('a') => self.add(shared, state),
            Key::Char('e') => match &at {
                Some(obj @ (Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_))) => {
                    self.revise(shared, obj)
                }
                _ => say(words::REVISE_WHERE),
            },
            Key::Escape if !state.new.is_empty() => self.clear(shared, state),
            // the key line offers Esc to clear NEW PATH, so Esc says why
            // it has nothing to clear
            Key::Escape => say(words::NOTHING_TO_CLEAR),
            Key::Char('p') => say(match shared.live {
                Live::On => words::PREVIEW_PATH_LATER,
                _ => words::PREVIEW_PATH_NOT_HERE,
            }),
            Key::Char('t') => say(match shared.live {
                Live::On => words::LIVE_ON,
                Live::OpsOff => words::OPS_OFF,
                _ => words::NOT_VTIUM,
            }),
            // the file, with the highlighted box's line marked
            Key::Char('v') => {
                if let Some(item) = at.as_ref().and_then(|o| self.doc_item(shared, o)) {
                    self.at = item;
                }
                change(Change::Action('v'))
            }
            Key::Char('s') => change(Change::Save),
            Key::Char(c @ ('i' | 'd' | 'r' | 'o' | 'n' | 'x')) => change(Change::Action(c)),
            _ => Vec::new(),
        });
        out
    }

    /// The flow's item an added path's box stands for, whose line the file
    /// view opens at.
    fn doc_item(&self, shared: &Shared, obj: &Obj) -> Option<Item> {
        let flow = &shared.inputs.flow;
        if let Obj::Roam(slot) = obj {
            let roams = |sel: &str| {
                flow.rules.iter().position(|r| {
                    r.target == "roaming" && paths::expand(sel).contains(&r.selector.as_str())
                })
            };
            return match slot {
                Slot::Input(sel) => roams(sel),
                _ => flow.rules.iter().position(|r| r.target == "roaming"),
            }
            .map(Item::Rule);
        }
        let Obj::Doc(name, slot) = obj else {
            return None;
        };
        let h = flow.heads.iter().position(|x| x.name == *name)?;
        Some(match slot {
            Slot::Display(sel) => flow
                .rules
                .iter()
                .position(|r| r.selector == *sel)
                .map_or(Item::Head(h), Item::Rule),
            Slot::Input(sel) => flow
                .rules
                .iter()
                .position(|r| paths::expand(sel).contains(&r.selector.as_str()))
                .map_or(Item::HeadLook(h), Item::Rule),
            _ => Item::Head(h),
        })
    }

    /// Enter on the document: opens the highlighted box's layer, or shows
    /// a font set in the Looks tab's font sets.
    fn open(&mut self, shared: &Shared, at: Option<Obj>) -> Vec<Command> {
        let Some(obj) = at else {
            return Vec::new();
        };
        let mut out = vec![Command::Change(Change::Quiet)];
        let active = self.state.new.active;
        let none = self.state.new.none;
        out.extend(match &obj {
            Obj::Roam(Slot::Terminal) => say(words::ACTIVE_ABOUT),
            Obj::New(Slot::Terminal) if active => say(words::ACTIVE_ABOUT),
            Obj::Pal(Slot::Special(p))
            | Obj::Doc(_, Slot::Special(p))
            | Obj::New(Slot::Special(p)) => say(words::special_about(&parts::special_title(p))),
            Obj::Nothing(Slot::Terminal) => say(words::NO_TERMINAL_ABOUT),
            Obj::New(Slot::Terminal) if none => say(words::NO_TERMINAL_ABOUT),
            Obj::Pal(Slot::Template(_)) | Obj::New(Slot::Empty(_)) => {
                self.state.layer = Some(obj.clone());
                self.state.line = 0;
                self.state.picker = None;
                Vec::new()
            }
            Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
                let pick = self.card_pick(shared, &obj, 0);
                self.state.layer = Some(obj.clone());
                self.state.line = 0;
                self.state.pick = pick;
                Vec::new()
            }
            // a look shows its font set in the Looks tab, where the look
            // is made and changed. The default look is the draft's
            // `[look default]` section when the draft holds one.
            Obj::Pal(Slot::Look(l)) => {
                let frit = flowlayout::look_section(&shared.inputs.flow, l.as_deref())
                    .map(|x| x.frit.clone())
                    .filter(|f| !f.is_empty());
                let Some(frit) = frit else {
                    return say(words::DEFAULT_FONT_PREVIEW);
                };
                vec![Command::Request(Request::ShowFont(frit))]
            }
            _ => {
                let line = self.choice_line(shared, &obj);
                self.state.layer = Some(obj);
                self.state.line = line;
                Vec::new()
            }
        });
        out
    }

    /// The line a layer's cursor starts on: the piece's own choice, or for
    /// the editor's terminal, its name.
    fn choice_line(&self, shared: &Shared, obj: &Obj) -> usize {
        let now = match obj {
            Obj::New(Slot::Display(s)) | Obj::New(Slot::Input(s)) => Some(s.clone()),
            _ => return 0,
        };
        // the label is the detail's first line, then the choices
        flowlayout::choices_for(shared, obj)
            .iter()
            .position(|(_, _, v)| *v == now)
            .map_or(0, |at| at.saturating_add(1))
    }

    /// Space: picks a palette piece into the editor or takes it out, takes
    /// an editor piece out, and says why anything else cannot be picked.
    fn space(&mut self, shared: &Shared, state: &State, at: Option<Obj>) -> Vec<Command> {
        let flow = &shared.inputs.flow;
        let doc = flowlayout::shown_doc(&self.state, paths::read(flow));
        let Some(obj) = at else {
            return Vec::new();
        };
        match &obj {
            Obj::Pal(Slot::Template(st)) => say(words::template_space(st.name())),
            Obj::New(Slot::Empty(_)) => say(words::EMPTY_SPACE),
            Obj::Pal(Slot::Display(_)) if state.new.active => say(words::ACTIVE_NO_DISPLAY),
            Obj::Pal(Slot::Special(Piece::Active))
                if !state.new.active && !state.new.displays.is_empty() =>
            {
                say(words::ACTIVE_NO_DISPLAY)
            }
            Obj::Pal(Slot::Special(Piece::NoInput)) if state.new.active || state.new.none => {
                say(words::NO_INPUT_NEEDS_TERMINAL)
            }
            Obj::Pal(Slot::Special(p)) | Obj::New(Slot::Special(p)) => self.pick(shared, p.clone()),
            Obj::Pal(Slot::Display(sel)) if sel.is_empty() => {
                // a display with no key vtium can match it by, as one that
                // reports no EDID where no udev daemon runs, would make a
                // rule with no match, which vtium refuses, so it is not
                // picked as itself
                let name = flow
                    .displays
                    .iter()
                    .find(|d| d.found && paths::own_key(flow, Kind::Display, d).is_empty())
                    .map_or_else(|| "on this machine".to_owned(), |d| d.name.clone());
                say(words::display_no_match(&name))
            }
            Obj::Pal(Slot::Display(sel)) => {
                // a display is in one path for each VT, so picking it is
                // refused only when a path that holds it shares a VT with
                // the new path's terminal. A part that matches no one
                // device, as All displays, is held by the paths that name
                // its match.
                //
                // References:
                // - ADR-1403 D7: a display belongs to one path on each VT.
                let held = match flow.displays.iter().find(|d| d.keys.contains(sel)) {
                    Some(d) => flowview::holders(&doc.paths, Kind::Display, d),
                    None => doc
                        .paths
                        .iter()
                        .filter(|p| p.displays.iter().any(|s| s == sel))
                        .filter_map(|p| p.name.clone())
                        .collect(),
                };
                let clash = held.iter().find_map(|h| {
                    let vt = doc.path(h).and_then(|p| state.new.shared_vt(p))?;
                    Some((h, vt))
                });
                let picked = state.new.has(&Piece::Display(sel.clone()));
                if let (Some((h, vt)), false) = (clash, picked) {
                    let (title, _) = paths::title(flow, Kind::Display, sel);
                    return say(words::display_taken(&title, h, vt));
                }
                self.pick(shared, Piece::Display(sel.clone()))
            }
            Obj::Pal(Slot::Input(sel)) | Obj::New(Slot::Input(sel)) => {
                self.pick(shared, Piece::Input(sel.clone()))
            }
            Obj::New(Slot::Display(sel)) => self.pick(shared, Piece::Display(sel.clone())),
            Obj::Pal(Slot::Look(l)) => self.pick(shared, Piece::Look(l.clone())),
            Obj::Others => say(words::OTHERS_CANNOT),
            Obj::Doc(..) => say(words::IN_DOCUMENT),
            Obj::Roam(Slot::Input(_)) => say(words::IN_ROAMING),
            Obj::Roam(_) | Obj::Nothing(_) => say(words::IN_DOCUMENT),
            Obj::New(Slot::Terminal) => say(words::NEW_TERMINAL_SPACE),
            _ => Vec::new(),
        }
    }

    /// Picks `piece` into the editor, or takes it out, and says what that
    /// did, whether the input will roam, and what the path needs next.
    fn pick(&mut self, shared: &Shared, piece: Piece) -> Vec<Command> {
        let doc = flowlayout::shown_doc(&self.state, paths::read(&shared.inputs.flow));
        let flow = &shared.inputs.flow;
        let state = &mut self.state;
        let was_look = state.new.look.clone();
        let done = match &piece {
            // the default look is the absence of one
            Piece::Look(None) if was_look.is_some() => {
                state.new.look = None;
                Picked::Replaced(was_look.clone())
            }
            Piece::Look(None) => Picked::Added,
            _ => state.new.pick_part(flow, piece.clone()),
        };
        let new = state.new.clone();
        let (kind, sel) = match &piece {
            Piece::Display(s) => (Kind::Display, s.as_str()),
            Piece::Input(s) => (Kind::Input, s.as_str()),
            Piece::Active => {
                return say(match done {
                    Picked::Removed => words::ACTIVE_UNPICKED,
                    _ => words::ACTIVE_PICKED,
                });
            }
            Piece::NoInput | Piece::NoTerminal | Piece::Terminal(_) => {
                let title = parts::special_title(&piece);
                return say(match done {
                    Picked::Removed => words::piece_out(&title),
                    _ => words::special_picked(&title),
                });
            }
            Piece::Look(l) => {
                let now = l
                    .clone()
                    .unwrap_or_else(|| words::DEFAULT_LOOK_PART.to_owned());
                return say(match done {
                    Picked::Removed => words::look_default(),
                    Picked::Replaced(Some(old)) => words::look_instead(&now, &old),
                    _ => words::look_picked(&now),
                });
            }
        };
        // a device is named as a reader names it, `the 8BitDo Keyboard`;
        // a wider piece by its title, `any keyboard`
        let wide = sel == "*" || sel.starts_with("class:");
        let title = flow
            .devices(kind)
            .iter()
            .find(|d| !wide && d.found && d.keys.iter().any(|k| k == sel))
            .map_or_else(
                || paths::title(flow, kind, sel).0,
                |d| format!("the {}", d.name),
            );
        if done == Picked::Removed {
            return say(words::piece_out(&title));
        }
        let display = kind == Kind::Display;
        if new.none {
            return say(words::picked_aside(&title, display));
        }
        let roams = !display && (new.active || doc.would_roam(sel, &new));
        let pointers = done == Picked::AddedWithPointers;
        say(words::picked(&title, display, &new, pointers, roams))
    }

    /// `a`: adds the editor's path to the configuration, or says why it
    /// stays. A path being revised is still in the draft, so adding it
    /// back unchanged changes nothing.
    fn add(&mut self, shared: &Shared, state: &State) -> Vec<Command> {
        let flow = &shared.inputs.flow;
        let doc = flowlayout::shown_doc(&self.state, paths::read(flow));
        let mut new = state.new.clone();
        if new.name.is_none() && !new.active && !new.none {
            new.name = Some(paths::new_name(flow, &doc));
        }
        let name = new.name.clone().unwrap_or_default();
        let (changes, said, want) = match paths::check_add(flow, &doc, &new) {
            Err(why) => return say(why),
            // what is held already: a path being revised that is held
            // twice leaves the configuration
            Ok(Adding::Same(there)) => (paths::plan(flow, &doc), words::same_path(&there), None),
            Ok(Adding::SameActive) => {
                (paths::plan(flow, &doc), words::SAME_ACTIVE.to_owned(), None)
            }
            Ok(Adding::SameNone) => (paths::plan(flow, &doc), words::SAME_NONE.to_owned(), None),
            Ok(Adding::New) => {
                let (want, said) = if new.active {
                    (Obj::Roam(Slot::Terminal), words::ACTIVE_BACK.to_owned())
                } else if new.none {
                    (Obj::Nothing(Slot::Terminal), words::NONE_BACK.to_owned())
                } else {
                    (
                        Obj::Doc(name.clone(), Slot::Terminal),
                        words::path_added(&name),
                    )
                };
                (paths::plan(flow, &doc.with(new)), said, Some(want))
            }
        };
        let mut out = if changes.is_empty() {
            Vec::new()
        } else {
            match make(shared, changes, None) {
                Ok(out) => out,
                Err(said) => return said,
            }
        };
        self.state.new = Path::default();
        self.state.revising = None;
        self.state.want.clone_from(&want);
        if let Some(w) = want {
            self.state.at = Some(w);
        }
        out.extend(say(said));
        out
    }

    /// Esc on NEW PATH: clears it. A path being revised leaves the
    /// configuration then, as revising it said it would.
    fn clear(&mut self, shared: &Shared, state: &State) -> Vec<Command> {
        let out = match &state.revising {
            Some(_) => {
                let doc = flowlayout::shown_doc(&self.state, paths::read(&shared.inputs.flow));
                let changes = paths::plan(&shared.inputs.flow, &doc);
                let mut out = match make(shared, changes, None) {
                    Ok(out) => out,
                    Err(said) => return said,
                };
                out.extend(say(words::REVISED_DROPPED));
                out
            }
            None => say(words::EDITOR_CLEARED),
        };
        self.state.new = Path::default();
        self.state.revising = None;
        out
    }

    /// `e`: revises the path of a terminal, or the path of the active
    /// terminal, the roaming inputs: it goes into NEW PATH with its parts
    /// picked, and CONFIGURATION leaves it out. The draft keeps it until
    /// NEW PATH is added, which puts it back as it was when nothing in it
    /// changed, or cleared, which takes it out.
    fn revise(&mut self, shared: &Shared, obj: &Obj) -> Vec<Command> {
        let doc: Doc = paths::read(&shared.inputs.flow);
        if !self.state.new.is_empty() {
            return say(words::EDITOR_BUSY);
        }
        let (taken, said, revising, at) = match obj {
            Obj::Doc(name, _) => (
                doc.revise(name),
                words::path_revised(name),
                flowview::Revising::Path(name.clone()),
                Obj::New(Slot::Terminal),
            ),
            Obj::Roam(_) => (
                doc.revise_active(),
                words::ACTIVE_REVISED.to_owned(),
                flowview::Revising::Active,
                Obj::New(Slot::Terminal),
            ),
            Obj::Nothing(_) => (
                doc.revise_none(),
                words::NONE_REVISED.to_owned(),
                flowview::Revising::Nothing,
                Obj::New(Slot::Terminal),
            ),
            _ => return Vec::new(),
        };
        let Some((_, path)) = taken else {
            return Vec::new();
        };
        self.state.new = path;
        self.state.revising = Some(revising);
        self.state.layer = None;
        self.state.at = Some(at);
        say(said)
    }

    /// A key on a part's detail, whose first line is the part's label and
    /// whose other lines are its choices: Up and Down move; Enter on the
    /// label starts typing it, and Enter on a choice picks it into the
    /// editor, or puts it in the editor piece's place; Esc goes back.
    fn choice_key(&mut self, shared: &Shared, state: &State, obj: &Obj, key: Key) -> Vec<Command> {
        let list = flowlayout::choices_for(shared, obj);
        let Some((kind, sel)) = flowlayout::label_target(obj) else {
            self.state.layer = None;
            return Vec::new();
        };
        if let Some(typed) = state.typing.clone() {
            return self.label_key(shared, kind, &sel, typed, key);
        }
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, list.len().saturating_add(1), key);
                self.state.line = line;
                Vec::new()
            }
            Key::Escape => {
                self.state.layer = None;
                Vec::new()
            }
            Key::Enter if state.line == 0 => {
                let doc = paths::read(&shared.inputs.flow);
                let now = doc.label(kind, &sel).unwrap_or_default().to_owned();
                self.state.typing = Some(now);
                Vec::new()
            }
            Key::Enter => {
                let at = state.line.saturating_sub(1);
                let Some((_, _, Some(sel))) = list.get(at).cloned() else {
                    return Vec::new();
                };
                self.state.layer = None;
                match obj {
                    Obj::Pal(Slot::Display(_)) => {
                        if state.new.has(&Piece::Display(sel.clone())) {
                            return Vec::new();
                        }
                        self.pick(shared, Piece::Display(sel))
                    }
                    Obj::Pal(Slot::Input(_)) => {
                        if state.new.has(&Piece::Input(sel.clone())) {
                            return Vec::new();
                        }
                        self.pick(shared, Piece::Input(sel))
                    }
                    Obj::New(slot @ (Slot::Display(_) | Slot::Input(_))) => {
                        let Some(old) = slot.piece() else {
                            return Vec::new();
                        };
                        let new = match slot {
                            Slot::Display(_) => Piece::Display(sel.clone()),
                            _ => Piece::Input(sel.clone()),
                        };
                        let at = match slot {
                            Slot::Display(_) => Slot::Display(sel),
                            _ => Slot::Input(sel),
                        };
                        self.state.new.replace(&old, new);
                        self.state.at = Some(Obj::New(at));
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// A key while a part's label is typed: characters go into it, up to
    /// the most a label holds; Enter writes it to the draft, and Esc leaves
    /// the label as it was.
    fn label_key(
        &mut self,
        shared: &Shared,
        kind: Kind,
        sel: &str,
        mut typed: String,
        key: Key,
    ) -> Vec<Command> {
        match key {
            Key::Char(c)
                if super::typable(c) && typed.chars().count() < vtium_settings::LABEL_MAX =>
            {
                typed.push(c);
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Enter => self.set_label(shared, kind, sel, &typed),
            Key::Escape => {
                self.state.typing = None;
                vec![Command::Change(Change::Quiet)]
            }
            _ => Vec::new(),
        }
    }

    /// Gives the part of `kind` that matches by `sel` the label `typed`, or
    /// takes its label away when `typed` is blank, through the draft's
    /// changes, which write the `[labels]` section. A label that starts
    /// with `#` is refused in one sentence, since vtium.conf reads it as a
    /// comment.
    fn set_label(&mut self, shared: &Shared, kind: Kind, sel: &str, typed: &str) -> Vec<Command> {
        let label = typed.trim();
        if label.starts_with('#') {
            return say(words::LABEL_HASH);
        }
        let flow = &shared.inputs.flow;
        let mut doc = paths::read(flow);
        doc.set_label(kind, sel, label);
        let changes = paths::plan(flow, &doc);
        self.state.typing = None;
        if changes.is_empty() {
            return Vec::new();
        }
        let mut out = match make(shared, changes, None) {
            Ok(out) => out,
            Err(said) => return said,
        };
        out.extend(say(if label.is_empty() {
            words::LABEL_REMOVED.to_owned()
        } else {
            words::label_set(label)
        }));
        out
    }

    /// The choice a terminal card's line `line` starts on: the one the
    /// path holds now.
    fn card_pick(&self, shared: &Shared, obj: &Obj, line: usize) -> usize {
        let Some(path) = flowlayout::card_path(&self.state, shared, obj) else {
            return 0;
        };
        let lines = flowlayout::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(l) = lines.get(line) else {
            return 0;
        };
        flowlayout::card_chosen(shared, &path, l).unwrap_or(0)
    }

    /// A key on a terminal's card, an added path's or the editor's: Up and
    /// Down move between its lines, Left and Right between a line's
    /// choices, Enter sets the choice or starts typing the name or, on the
    /// login's Other…, a command, and Esc goes back. `e` on an added path's
    /// card revises the path.
    fn terminal_card_key(
        &mut self,
        shared: &Shared,
        state: &State,
        obj: &Obj,
        key: Key,
    ) -> Vec<Command> {
        let Some(path) = flowlayout::card_path(&self.state, shared, obj) else {
            self.state.layer = None;
            return Vec::new();
        };
        let lines = flowlayout::card_lines(&path, matches!(obj, Obj::New(_)));
        let Some(line) = lines.get(state.line).cloned() else {
            self.state.line = 0;
            return Vec::new();
        };
        if let Some(typed) = state.typing.clone() {
            return self.card_type_key(shared, obj, &path, &line, typed, key);
        }
        let choices = flowlayout::path_choices(shared, &path, &line);
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let at = step(state.line, lines.len(), key);
                let pick = self.card_pick(shared, obj, at);
                self.state.line = at;
                self.state.pick = pick;
                Vec::new()
            }
            Key::Left => {
                self.state.pick = self.state.pick.saturating_sub(1);
                Vec::new()
            }
            Key::Right => {
                let last = choices.len().saturating_sub(1);
                self.state.pick = self.state.pick.saturating_add(1).min(last);
                Vec::new()
            }
            Key::Enter if flowlayout::card_types(shared, &line, state.pick) => {
                let now = flowlayout::card_value(&path, &line);
                self.state.typing = Some(now);
                Vec::new()
            }
            Key::Enter => {
                let value = choices
                    .get(state.pick)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                self.card_apply(shared, obj, &path, &line, value)
            }
            Key::Char('e') => match obj {
                Obj::Doc(..) => self.revise(shared, obj),
                _ => Vec::new(),
            },
            Key::Escape => {
                self.state.layer = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A key while a card's name or login is typed: characters go into
    /// it, Enter keeps it and Esc drops it. A login command that cannot
    /// run is refused in one sentence, and stays to be fixed.
    fn card_type_key(
        &mut self,
        shared: &Shared,
        obj: &Obj,
        path: &Path,
        line: &flowlayout::CardLine,
        mut typed: String,
        key: Key,
    ) -> Vec<Command> {
        match key {
            Key::Char(c) if super::typable(c) && typed.chars().count() < 64 => {
                typed.push(c);
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Enter if *line == flowlayout::CardLine::Setting("login") => {
                match paths::check_login(&typed) {
                    Ok(command) => self.card_apply(shared, obj, path, line, command),
                    Err(why) => say(why),
                }
            }
            Key::Enter => self.card_apply(shared, obj, path, line, typed),
            Key::Escape => {
                self.state.typing = None;
                vec![Command::Change(Change::Quiet)]
            }
            _ => Vec::new(),
        }
    }

    /// Sets the card's line `line` to `value`: in the editor, on the path
    /// it holds; on an added path, through the draft's changes, which
    /// `paths::plan` makes from the configuration with the path changed.
    /// A name another terminal has, or one a terminal cannot take, is
    /// refused in one sentence.
    fn card_apply(
        &mut self,
        shared: &Shared,
        obj: &Obj,
        path: &Path,
        line: &flowlayout::CardLine,
        value: String,
    ) -> Vec<Command> {
        use flowlayout::CardLine;
        let flow = &shared.inputs.flow;
        let mut doc = paths::read(flow);
        let own = path.name.clone().unwrap_or_default();
        if *line == CardLine::Name && value != own {
            let taken = doc.path(&value).is_some() || flow.heads.iter().any(|h| h.name == value);
            let why = if !paths::valid_name(&value) {
                Some(words::name_invalid(&value))
            } else if taken {
                Some(words::name_taken(&value))
            } else {
                None
            };
            if let Some(why) = why {
                return say(why);
            }
        }
        let mut changed = path.clone();
        match line {
            CardLine::Name => changed.name = Some(value.clone()),
            CardLine::Look => changed.look = Some(value.clone()).filter(|v| !v.is_empty()),
            CardLine::Setting(key) => changed.set(key, &value),
            CardLine::Part(piece) => {
                changed.pick(piece.clone());
            }
        }
        self.state.typing = None;
        let mut out = vec![Command::Change(Change::Quiet)];
        match obj {
            Obj::New(_) => {
                if matches!(line, CardLine::Part(_)) {
                    self.state.line = self.state.line.saturating_sub(1);
                }
                self.state.new = changed;
                out
            }
            _ => {
                let Some(at) = doc
                    .paths
                    .iter()
                    .position(|p| p.name.as_deref() == Some(own.as_str()))
                else {
                    return out;
                };
                if let Some(slot) = doc.paths.get_mut(at) {
                    *slot = changed.clone();
                }
                let changes = paths::plan(&shared.inputs.flow, &doc);
                if changes.is_empty() {
                    return out;
                }
                match make(shared, changes, None) {
                    Ok(made) => out.extend(made),
                    Err(said) => {
                        out.extend(said);
                        return out;
                    }
                }
                let name = changed.name.clone().unwrap_or_else(|| own.clone());
                out.extend(say(words::card_changed(&name)));
                if *line == CardLine::Name {
                    let renamed = Obj::Doc(value, Slot::Terminal);
                    self.state.layer = Some(renamed.clone());
                    self.state.want = Some(renamed);
                }
                out
            }
        }
    }

    /// A key on a template's layer, the ways to make a part of `stage`,
    /// or on a filter's picker over it: Up and Down move, Enter makes the
    /// part, opens the picker or starts typing a value, and Esc goes back.
    fn make_key(
        &mut self,
        shared: &Shared,
        state: &State,
        stage: Stage,
        from_new: bool,
        key: Key,
    ) -> Vec<Command> {
        if let Some(typed) = state.typing.clone() {
            return self.typed_key(shared, stage, from_new, typed, key);
        }
        let looks = flowlayout::look_names(shared);
        let length = match &state.picker {
            Some(p) => parts::picker(&shared.inputs.flow, p.stage, p.key).len(),
            None => parts::makes(stage, &looks).len(),
        };
        match key {
            Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let line = step(state.line, length, key);
                self.state.line = line;
                Vec::new()
            }
            Key::Escape if state.picker.is_some() => {
                let back = parts::makes(stage, &looks)
                    .iter()
                    .position(|(_, _, m)| {
                        state
                            .picker
                            .as_ref()
                            .is_some_and(|p| *m == Make::Filter(p.key))
                    })
                    .unwrap_or(0);
                self.state.picker = None;
                self.state.line = back;
                Vec::new()
            }
            Key::Escape => {
                self.state.layer = None;
                Vec::new()
            }
            Key::Enter => match &state.picker {
                Some(p) => {
                    match parts::picker(&shared.inputs.flow, p.stage, p.key).get(state.line) {
                        Some(Pick::Value { sel, .. }) => {
                            let piece = match stage {
                                Stage::Display => Piece::Display(sel.clone()),
                                _ => Piece::Input(sel.clone()),
                            };
                            let title = parts::title_of(&shared.inputs.flow, &piece);
                            self.make_part(shared, piece, title, from_new)
                        }
                        Some(Pick::Typed) => {
                            self.state.typing = Some(String::new());
                            Vec::new()
                        }
                        None => Vec::new(),
                    }
                }
                None => match parts::makes(stage, &looks)
                    .get(state.line)
                    .map(|(_, _, m)| m.clone())
                {
                    Some(Make::Part(piece)) => {
                        let title = parts::title_of(&shared.inputs.flow, &piece);
                        self.make_part(shared, piece, title, from_new)
                    }
                    Some(Make::Filter(key)) => {
                        self.state.picker = Some(flowview::Picker {
                            stage,
                            key,
                            from_new,
                        });
                        self.state.line = 0;
                        Vec::new()
                    }
                    Some(Make::NewTerminal) => self.new_terminal(shared, from_new),
                    None => Vec::new(),
                },
            },
            _ => Vec::new(),
        }
    }

    /// A key while a value is typed on a filter's picker: characters go
    /// into it, Enter checks it as vtium.conf checks a rule's match and
    /// makes the part, or says what is wrong, and Esc drops it.
    fn typed_key(
        &mut self,
        shared: &Shared,
        stage: Stage,
        from_new: bool,
        mut typed: String,
        key: Key,
    ) -> Vec<Command> {
        match key {
            Key::Char(c) if super::typable(c) && typed.chars().count() < 200 => {
                typed.push(c);
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Backspace => {
                typed.pop();
                self.state.typing = Some(typed);
                Vec::new()
            }
            Key::Enter => {
                let key = self.state.picker.as_ref().map(|p| p.key);
                // a value typed without a match key takes the picker's key
                let bare = typed.trim();
                let keyed = vtium_settings::Token::KEYS
                    .iter()
                    .any(|k| bare.strip_prefix(k).is_some_and(|r| r.starts_with(':')));
                let typed = match key {
                    Some(k) if !keyed && !bare.is_empty() && bare != "*" => format!("{k}:{bare}"),
                    _ => typed,
                };
                match parts::check_typed(stage, &typed) {
                    Ok(sel) => {
                        let piece = match stage {
                            Stage::Display => Piece::Display(sel.clone()),
                            _ => Piece::Input(sel.clone()),
                        };
                        self.make_part(shared, piece, sel, from_new)
                    }
                    // the parser's sentence names a rule; the reader typed a
                    // value of the picker's kind, so the refusal names that
                    Err(_) => say(words::typed_refused(key.unwrap_or("path"))),
                }
            }
            Key::Escape => {
                self.state.typing = None;
                vec![Command::Change(Change::Quiet)]
            }
            _ => Vec::new(),
        }
    }

    /// "new terminal": makes a terminal part with the next free name. From
    /// NEW PATH it also goes into the new path, and the new path's
    /// terminal card opens, where its name and look are set.
    fn new_terminal(&mut self, shared: &Shared, from_new: bool) -> Vec<Command> {
        let flow = &shared.inputs.flow;
        let doc = flowlayout::shown_doc(&self.state, paths::read(flow));
        let made: Vec<String> = self
            .state
            .parts
            .iter()
            .filter_map(|p| match &p.piece {
                Piece::Terminal(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        let name = paths::new_name_besides(flow, &doc, &made);
        let out = self.make_part(shared, Piece::Terminal(name.clone()), name, from_new);
        if from_new {
            let obj = Obj::New(Slot::Terminal);
            let pick = self.card_pick(shared, &obj, 0);
            self.state.layer = Some(obj);
            self.state.line = 0;
            self.state.pick = pick;
        }
        out
    }

    /// Makes the part `piece`, named `title`, and adds it to PARTS, or
    /// finds the part PARTS holds with the same match. From NEW PATH the
    /// part also goes into the new path, and the highlight moves to it
    /// there; else the highlight moves to it in PARTS.
    fn make_part(
        &mut self,
        shared: &Shared,
        piece: Piece,
        title: String,
        from_new: bool,
    ) -> Vec<Command> {
        let flow = &shared.inputs.flow;
        let mut name = title.clone();
        let there = {
            let s = &mut self.state;
            let there = s.new.has(&piece);
            let at = parts::make(&mut s.parts, flow, piece.clone(), title);
            if let Some(p) = s.parts.get(at) {
                name.clone_from(&p.title);
            }
            s.layer = None;
            s.picker = None;
            s.typing = None;
            there
        };
        if !from_new {
            let at = Obj::Pal(Slot::of(&piece));
            self.state.want = Some(at.clone());
            self.state.at = Some(at);
            return say(words::part_made(&name));
        }
        let at = match &piece {
            Piece::Display(s) => Obj::New(Slot::Display(s.clone())),
            Piece::Input(s) => Obj::New(Slot::Input(s.clone())),
            Piece::NoInput => Obj::New(Slot::Special(Piece::NoInput)),
            _ => Obj::New(Slot::Terminal),
        };
        let out = if there {
            Vec::new()
        } else {
            self.pick(shared, piece)
        };
        self.state.want = Some(at.clone());
        self.state.at = Some(at);
        out
    }

    /// A click on the document: on a box, highlights it, and on the
    /// highlighted box, opens it.
    pub fn click(
        &mut self,
        shared: &Shared,
        size: Size,
        keys: usize,
        col: usize,
        row: usize,
    ) -> Vec<Command> {
        let state = self.state.clone();
        if state.layer.is_some() || state.typing.is_some() {
            return Vec::new();
        }
        let (top, height) = flowlayout::window(shared, size, keys);
        if row < top || row >= top.saturating_add(height) {
            return Vec::new();
        }
        let y = state.top.saturating_add(row.saturating_sub(top));
        let layout = flowlayout::layout_of(&self.state, shared, size.cols);
        let hit = layout
            .boxes
            .iter()
            .find(|b| col >= b.x && col < b.right() && y >= b.y && y < b.bottom())
            .map(|b| b.obj.clone());
        match hit {
            Some(o) if state.at.as_ref() == Some(&o) => self.open(shared, Some(o)),
            Some(o) => {
                self.state.go(&layout, o, height);
                Vec::new()
            }
            None => Vec::new(),
        }
    }
}

/// A change to the shared part alone.
fn change(change: Change) -> Vec<Command> {
    vec![Command::Change(change)]
}

/// Puts `what` on the message line.
fn say(what: impl Into<String>) -> Vec<Command> {
    change(Change::Say(what.into()))
}

/// The command that makes `changes` on the draft and selects `select`
/// once the draft is checked, or, as `Err`, the line that says why the
/// changes cannot be made. The tab asks first, so that what it does
/// after a change that is made does not happen to one that is refused.
fn make(
    shared: &Shared,
    changes: Vec<DraftChange>,
    select: Option<Item>,
) -> Result<Vec<Command>, Vec<Command>> {
    match shared.apply(&changes) {
        Ok(_) => Ok(change(Change::Make { changes, select })),
        Err(why) => Err(say(why)),
    }
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

impl Tab for Flow {
    fn name(&self) -> &'static str {
        words::FLOW
    }

    /// Revert, Restore and New beside Save and Install.
    fn actions(&self) -> &'static [(&'static str, &'static str)] {
        &words::FLOW_ACTIONS
    }

    fn node(&self) -> &'static str {
        Flow::node(self)
    }

    /// The Flow tab's own update reads every action key itself.
    fn takes_action(&self, _action: char) -> bool {
        false
    }

    fn update(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command> {
        Flow::update(self, shared, room.size, room.keys, key)
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::super::super::input::Input;
    use super::super::super::model::{App, Effect, Event, View};
    use super::super::super::paths::tests::{PORT_33, flow};
    use super::*;

    /// The Flow view on the test flow, at 120×32.
    fn app() -> App {
        let mut i = super::super::super::fixture::inputs();
        i.view = View::Flow;
        i.flow = flow();
        let (mut app, _) = App::new(i, 120, 32);
        app.flow_settle();
        app
    }

    fn keys(app: &mut App, keys: &[Key]) {
        for k in keys {
            app.event(Event::Input(Input::Key(*k)));
        }
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            keys(app, &[Key::Char(c)]);
        }
    }

    fn state(app: &App) -> &State {
        app.flow.state()
    }

    fn highlight(app: &mut App, obj: Obj) {
        app.flow.state.at = Some(obj);
    }

    #[test]
    fn enter_on_a_palette_look_shows_its_font_set_in_the_looks_tab() {
        let mut app = app();
        let name = app
            .shared
            .inputs
            .flow
            .looks
            .iter()
            .find(|l| !l.frit.is_empty())
            .map(|l| l.name.clone())
            .expect("a look that names a font set");
        highlight(&mut app, Obj::Pal(Slot::Look(Some(name.clone()))));
        keys(&mut app, &[Key::Enter]);
        // the Flow view asks update to show the look's font set, which the
        // Looks tab lists; it never writes another tab's state itself
        assert_eq!(app.nav.view, View::Looks, "{name}");
    }

    // vtium draws a terminal that names no look with the draft's
    // `[look default]` section, so Enter on "default look" shows that
    // look's font set
    #[test]
    fn enter_on_the_default_look_shows_the_font_set_of_look_default() {
        let mut app = app();
        let mut big = app
            .shared
            .inputs
            .flow
            .looks
            .first()
            .cloned()
            .expect("the look big");
        big.name = "default".into();
        big.frit = "unifont".into();
        app.shared.inputs.flow.looks.push(big);
        highlight(&mut app, Obj::Pal(Slot::Look(None)));
        keys(&mut app, &[Key::Enter]);
        assert_eq!(
            app.nav.view,
            View::Looks,
            "{:?}",
            app.shared.message.as_deref()
        );
    }

    // with no `[look default]` section, vtium draws such a terminal with
    // the font set the top of vtium.conf sets
    #[test]
    fn enter_on_the_default_look_with_no_look_default_says_vtium_conf_sets_it() {
        let mut app = app();
        highlight(&mut app, Obj::Pal(Slot::Look(None)));
        keys(&mut app, &[Key::Enter]);
        assert_eq!(app.nav.view, View::Flow);
        assert_eq!(
            app.shared.message.as_deref(),
            Some(words::DEFAULT_FONT_PREVIEW)
        );
    }

    #[test]
    fn a_template_refuses_to_be_placed_and_enter_makes_a_part_from_it() {
        let mut app = app();
        let template = Obj::Pal(Slot::Template(Stage::Input));
        highlight(&mut app, template.clone());
        keys(&mut app, &[Key::Char(' ')]);
        assert_eq!(
            app.shared.message.as_deref(),
            Some("The Input template adds inputs. Press Enter to add one.")
        );
        assert!(
            state(&app).new.is_empty(),
            "the template stays out of the new path"
        );
        // Enter opens the ways to make an input part; "by USB port" opens
        // its picker, whose first value is the receiver's port
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).layer, Some(template.clone()));
        keys(
            &mut app,
            &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter],
        );
        let picker = state(&app).picker.clone().expect("the picker");
        assert_eq!(
            (picker.stage, picker.key, picker.from_new),
            (Stage::Input, "path", false)
        );
        keys(&mut app, &[Key::Enter]);
        // the part joins PARTS, highlighted, and stays out of the new path
        let made = Piece::Input(PORT_33.into());
        let part = state(&app)
            .parts
            .iter()
            .find(|p| p.piece == made)
            .expect("the new part");
        assert!(part.made && part.connected);
        assert_eq!(part.title, "inputs on USB 3.3");
        assert_eq!(state(&app).at, Some(Obj::Pal(Slot::Input(PORT_33.into()))));
        assert!(state(&app).new.is_empty() && state(&app).layer.is_none());
    }

    #[test]
    fn a_part_made_from_an_empty_place_in_new_path_is_picked_into_it() {
        let mut app = app();
        highlight(&mut app, Obj::New(Slot::Empty(Stage::Input)));
        keys(&mut app, &[Key::Char(' ')]);
        assert_eq!(app.shared.message.as_deref(), Some(words::EMPTY_SPACE));
        // no input, the first choice
        keys(&mut app, &[Key::Enter, Key::Enter]);
        assert!(state(&app).new.no_input);
        assert!(state(&app).parts.iter().any(|p| p.piece == Piece::NoInput));
        assert_eq!(
            state(&app).at,
            Some(Obj::New(Slot::Special(Piece::NoInput)))
        );
        // a value typed on the picker: refused with the parser's words,
        // then made from the value alone, which takes the picker's key
        highlight(&mut app, Obj::New(Slot::Empty(Stage::Display)));
        keys(
            &mut app,
            &[Key::Enter, Key::Down, Key::Enter, Key::End, Key::Enter],
        );
        assert_eq!(state(&app).typing.as_deref(), Some(""));
        type_text(&mut app, "aaa:1:2");
        keys(&mut app, &[Key::Enter]);
        let why = app.shared.message.clone().unwrap_or_default();
        assert_eq!(
            why,
            "This monitor is not a valid EDID. An EDID looks like AAA:0x0001:LARGE00001."
        );
        assert!(state(&app).typing.is_some(), "the value stays to fix");
        keys(&mut app, &[Key::Escape, Key::Enter]);
        type_text(&mut app, "AAA:0x0001:LARGE00001");
        keys(&mut app, &[Key::Enter]);
        let edid = "edid:AAA:0x0001:LARGE00001".to_owned();
        assert_eq!(state(&app).new.displays, std::slice::from_ref(&edid));
        assert_eq!(state(&app).at, Some(Obj::New(Slot::Display(edid))));
    }
    /// Each value the Flow view types, a part's label, a terminal's name
    /// and a picker's value, takes printable characters and leaves out
    /// control characters, as the Settings view's values do. The C1
    /// control U+009B is CSI to a terminal that reads 8-bit controls, so
    /// in vtium.conf it would act on the terminal of whoever shows the
    /// file.
    #[test]
    fn the_typed_values_of_the_flow_view_leave_out_control_characters() {
        let typed = "a\u{9b}31m\u{85}\u{7}\u{1b}b";
        // a part's label: every keyboard, its detail, then Enter on the
        // label
        let mut label = app();
        highlight(&mut label, Obj::Pal(Slot::Template(Stage::Input)));
        keys(
            &mut label,
            &[Key::Enter, Key::Down, Key::Enter, Key::Enter, Key::Enter],
        );
        assert_eq!(state(&label).typing.as_deref(), Some(""), "the label");
        type_text(&mut label, typed);
        assert_eq!(state(&label).typing.as_deref(), Some("a31mb"), "the label");
        // the new path's terminal: its card, then Enter on the name,
        // which starts from the name the path has
        let mut name = app_with_new_path();
        highlight(&mut name, Obj::New(Slot::Terminal));
        keys(&mut name, &[Key::Enter, Key::Enter]);
        assert_eq!(state(&name).typing.as_deref(), Some("desk2"), "the name");
        type_text(&mut name, typed);
        assert_eq!(
            state(&name).typing.as_deref(),
            Some("desk2a31mb"),
            "the name"
        );
        // a picker's value: a display from an empty place in NEW PATH
        let mut value = app();
        highlight(&mut value, Obj::New(Slot::Empty(Stage::Display)));
        keys(
            &mut value,
            &[Key::Enter, Key::Down, Key::Enter, Key::End, Key::Enter],
        );
        assert_eq!(state(&value).typing.as_deref(), Some(""), "the value");
        type_text(&mut value, typed);
        assert_eq!(state(&value).typing.as_deref(), Some("a31mb"), "the value");
    }

    /// The Flow view with a new path that has a name and every display.
    fn app_with_new_path() -> App {
        let mut with = app();
        with.flow.state.new = Path {
            name: Some("desk2".into()),
            displays: vec!["*".into()],
            ..Default::default()
        };
        with
    }

    #[test]
    fn a_part_made_from_a_template_choice_carries_a_label() {
        let mut app = app();
        app.shared.draft = Some(super::super::super::desk::Opened {
            name: "north".into(),
            text: super::super::super::paths::tests::TEXT.into(),
            ..Default::default()
        });
        // every keyboard, the Input template's second choice
        highlight(&mut app, Obj::Pal(Slot::Template(Stage::Input)));
        keys(&mut app, &[Key::Enter, Key::Down, Key::Enter]);
        let every = Obj::Pal(Slot::Input(paths::ANY_KEYBOARD.into()));
        assert_eq!(state(&app).at, Some(every.clone()));
        // Enter opens its detail, whose first line is the label
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).layer, Some(every.clone()));
        assert_eq!(state(&app).line, 0);
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).typing.as_deref(), Some(""));
        type_text(&mut app, "Any board #1");
        let effects = app.event(Event::Input(Input::Key(Key::Enter)));
        let written = effects
            .iter()
            .find_map(|e| match e {
                Effect::Desk(super::super::super::desk::Op::Check(t)) => Some(t.clone()),
                _ => None,
            })
            .expect("the draft written");
        assert!(
            written.ends_with("[labels]\ninput class:keyboard = Any board \\#1\n"),
            "{written}"
        );
        assert!(state(&app).typing.is_none());
        // a label that starts with # is refused in one sentence
        keys(&mut app, &[Key::Enter]);
        type_text(&mut app, "#2");
        keys(&mut app, &[Key::Enter]);
        assert_eq!(app.shared.message.as_deref(), Some(words::LABEL_HASH));
        assert!(state(&app).typing.is_some(), "the label stays to fix");
    }

    // The maintainer's try-out: a terminal's Login is a pick list of the
    // command in effect, which leaves the head's key unset, and Other…,
    // a typed command that is checked before it is kept
    #[test]
    fn the_login_line_picks_the_default_or_a_checked_command() {
        let mut app = app();
        app.shared
            .inputs
            .flow
            .base
            .set
            .push(("login".into(), "/bin/sh -l".into()));
        let new = Path {
            name: Some("desk2".into()),
            displays: vec!["*".into()],
            ..Default::default()
        };
        app.flow.state.new = new;
        highlight(&mut app, Obj::New(Slot::Terminal));
        keys(&mut app, &[Key::Enter]);
        let login = flowlayout::CardLine::Setting("login");
        let at = flowlayout::card_lines(&state(&app).new, true)
            .iter()
            .position(|l| *l == login)
            .expect("the Login line");
        keys(&mut app, &vec![Key::Down; at]);
        let words: Vec<String> = flowlayout::card_choices(&app.shared, &login)
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(words, ["Default: /bin/sh -l", "Other\u{2026}"]);
        assert_eq!(state(&app).pick, 0, "Default is chosen");
        // Other… types a command, which must name a program that runs
        keys(&mut app, &[Key::Right, Key::Enter]);
        assert_eq!(state(&app).typing.as_deref(), Some(""));
        let refused = [
            ("login -p", words::login_not_absolute("login")),
            (
                "/nonexistent/vtium/login",
                words::login_missing("/nonexistent/vtium/login"),
            ),
            ("/etc/passwd", words::login_not_runnable("/etc/passwd")),
            ("/bin/sh -c 'a #b'", words::LOGIN_HASH.to_owned()),
            ("   ", words::LOGIN_EMPTY.to_owned()),
        ];
        for (typed, why) in refused {
            keys(&mut app, &[Key::Escape, Key::Enter]);
            type_text(&mut app, typed);
            keys(&mut app, &[Key::Enter]);
            assert_eq!(app.shared.message.as_deref(), Some(why.as_str()), "{typed}");
            assert!(state(&app).typing.is_some(), "{typed} stays to fix");
            assert_eq!(state(&app).new.setting("login"), "", "{typed}");
        }
        keys(&mut app, &[Key::Escape, Key::Enter]);
        type_text(&mut app, " /bin/sh -l ");
        keys(&mut app, &[Key::Enter]);
        assert!(state(&app).typing.is_none());
        assert_eq!(state(&app).new.setting("login"), "/bin/sh -l");
        assert_eq!(state(&app).pick, 1, "Other… holds the command");
        // Default takes the command out: the path writes no login key
        keys(&mut app, &[Key::Left, Key::Enter]);
        assert!(state(&app).typing.is_none());
        assert_eq!(state(&app).new.setting("login"), "");
        let doc = paths::read(&app.shared.inputs.flow).with(state(&app).new.clone());
        let text = paths::plan(&app.shared.inputs.flow, &doc)
            .iter()
            .fold(paths::tests::TEXT.to_owned(), |t, c| {
                c.apply(&t).expect("the change applies")
            });
        assert!(text.contains("[head desk2]"), "{text}");
        assert!(!text.contains("login"), "{text}");
    }

    // All displays matches no one device, and picking it is still refused
    // when a path on a VT the new path shares holds it.
    //
    // References:
    // - ADR-1403 D7: a display belongs to one path on each VT.
    #[test]
    fn picking_a_display_part_held_on_a_shared_vt_is_refused() {
        let mut app = app();
        let text = "version = 1\n[head desk]\nvt = 3\n[head panel]\nvt = 4\n[rules]\ndisplay * -> desk\ndisplay * -> panel\n";
        app.shared.inputs.flow =
            crate::flowmodel::build(text, &crate::flowmodel::Present::default(), None);
        app.flow_settle();
        highlight(&mut app, Obj::Pal(Slot::Display("*".into())));
        keys(&mut app, &[Key::Char(' ')]);
        assert_eq!(
            app.shared.message.as_deref(),
            Some(words::display_taken("All displays", "desk", Some(3)).as_str())
        );
        assert!(state(&app).new.displays.is_empty());
    }

    #[test]
    fn the_desk_s_answer_keeps_the_selected_item_on_the_rebuilt_flow() {
        use super::super::super::desk::{Done, Op, Opened};
        let text = super::super::super::paths::tests::TEXT;
        let mut app = app();
        app.shared.inputs.flow =
            crate::flowmodel::build(text, &crate::flowmodel::Present::default(), None);
        let opened = Opened {
            name: "north".into(),
            text: text.into(),
            ..Default::default()
        };
        app.shared.draft = Some(opened.clone());
        // a rule past the first item, which a rebuild of the same draft
        // keeps selected
        let last = app.shared.inputs.flow.rules.len().saturating_sub(1);
        app.flow.at = Item::Rule(last);
        assert_ne!(first_item(&app.shared.inputs.flow), app.flow.at());
        app.event(Event::Desked(
            Op::Check(text.into()),
            Ok(Done::Draft(opened, None)),
        ));
        assert_eq!(app.flow.at(), Item::Rule(last));
    }

    /// The Flow view on the draft `text`, with `head`'s card open.
    fn card_of(text: &str, head: &str) -> App {
        let mut app = app();
        app.shared.inputs.flow =
            crate::flowmodel::build(text, &crate::flowmodel::Present::default(), None);
        app.shared.draft = Some(super::super::super::desk::Opened {
            name: "north".into(),
            text: text.into(),
            ..Default::default()
        });
        app.flow_settle();
        let card = Obj::Doc(head.into(), Slot::Terminal);
        highlight(&mut app, card.clone());
        keys(&mut app, &[Key::Enter]);
        assert_eq!(state(&app).layer, Some(card));
        app
    }

    /// The draft text that Enter on the choice `pick` of `line` sends to
    /// check-config, from the card `card_of` opened.
    fn card_writes(app: &mut App, line: &flowlayout::CardLine, pick: usize) -> String {
        let path = flowlayout::card_path(
            app.flow.state(),
            &app.shared,
            &state(app).layer.clone().expect("a card"),
        )
        .expect("the card's path");
        let at = flowlayout::card_lines(&path, false)
            .iter()
            .position(|l| l == line)
            .expect("the line on the card");
        keys(app, &vec![Key::Down; at]);
        keys(app, &[Key::Left, Key::Left]);
        keys(app, &vec![Key::Right; pick]);
        app.event(Event::Input(Input::Key(Key::Enter)))
            .iter()
            .find_map(|e| match e {
                Effect::Desk(super::super::super::desk::Op::Check(t)) => Some(t.clone()),
                _ => None,
            })
            .expect("the draft written")
    }

    // The maintainer chose that vtium-config sets a head's
    // `switch-on-enter` (2026-11-12): its card shows the key as the file
    // has it, and a change writes that key's line alone, with none of the
    // head template's comments
    #[test]
    fn a_terminal_s_card_turns_switch_on_enter_on_and_off() {
        let line = flowlayout::CardLine::Setting("switch-on-enter");
        let off = "version = 1\npolicy = open\n[head desk]\nvt = 3\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
        let mut app = card_of(off, "desk");
        let words: Vec<String> = flowlayout::card_choices(&app.shared, &line)
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(words, [words::SWITCH_OFF, words::SWITCH_ON]);
        let path = paths::read(&app.shared.inputs.flow)
            .path("desk")
            .cloned()
            .expect("desk");
        assert_eq!(flowlayout::card_chosen(&app.shared, &path, &line), Some(0));
        assert_eq!(
            card_writes(&mut app, &line, 1),
            "version = 1\npolicy = open\n[head desk]\nswitch-on-enter = yes\nvt = 3\n[rules]\ndisplay * -> desk\ninput * -> desk\n"
        );
        let on = "version = 1\npolicy = open\n[head desk]\nswitch-on-enter = yes\nvt = 3\n[rules]\ndisplay * -> desk\ninput * -> desk\n";
        let mut app = card_of(on, "desk");
        let path = paths::read(&app.shared.inputs.flow)
            .path("desk")
            .cloned()
            .expect("desk");
        assert_eq!(flowlayout::card_chosen(&app.shared, &path, &line), Some(1));
        assert_eq!(card_writes(&mut app, &line, 0), off);
        // toggling on, off, on and off leaves the draft as it was, with no
        // line left behind, commented out or not (the maintainer,
        // 2026-11-11: "only what's set")
        let mut text = off.to_owned();
        for pick in [1, 0, 1, 0] {
            text = card_writes(&mut card_of(&text, "desk"), &line, pick);
        }
        assert_eq!(text, off);
    }

    // The switch on Enter works only under policy = open, for a head that
    // is not isolated and runs on a VT of its own from 1 to 12, and the
    // hint says when it does nothing.
    //
    // References:
    // - ADR-1401, "Enter on a head's own keyboard": when the switch works.
    #[test]
    fn the_switch_on_enter_hint_says_when_the_key_has_no_effect() {
        let line = flowlayout::CardLine::Setting("switch-on-enter");
        let hint = |text: &str| -> String {
            let app = card_of(text, "desk");
            let path = paths::read(&app.shared.inputs.flow)
                .path("desk")
                .cloned()
                .expect("desk");
            flowlayout::card_about(&app.shared, &path, &line, false).expect("a hint")
        };
        let rules = "[rules]\ndisplay * -> desk\ninput * -> desk\n";
        let cases = [
            ("policy = open\n[head desk]\nvt = 3\n", None),
            ("policy = open\n[head desk]\nvt = 13 3\n", None),
            (
                "policy = closed\n[head desk]\nvt = 3\n",
                Some(words::SWITCH_CLOSED),
            ),
            (
                "policy = open\n[head desk]\nvt = 3\nisolated = yes\n",
                Some(words::SWITCH_ISOLATED),
            ),
            ("policy = open\n[head desk]\n", Some(words::SWITCH_EVERY_VT)),
            (
                "policy = open\n[head desk]\nvt = 13\n",
                Some(words::SWITCH_PAST_12),
            ),
        ];
        for (head, none) in cases {
            let text = format!("version = 1\n{head}{rules}");
            let want = match none {
                None => words::SWITCH_ABOUT.to_owned(),
                Some(why) => format!("{} {why}", words::SWITCH_ABOUT),
            };
            assert_eq!(hint(&text), want, "{text}");
        }
        // a terminal with no input of its own is isolated too
        let text = "version = 1\npolicy = open\n[head desk]\nvt = 3\nisolated = yes\n[rules]\ndisplay * -> desk\n";
        assert_eq!(
            hint(text),
            format!("{} {}", words::SWITCH_ABOUT, words::SWITCH_ISOLATED)
        );
        // in NEW PATH, the card's own line VTs gives the terminal a VT
        let app = app();
        let new = Path {
            displays: vec!["*".into()],
            ..Default::default()
        };
        assert_eq!(
            flowlayout::card_about(&app.shared, &new, &line, true).as_deref(),
            Some(format!("{} {}", words::SWITCH_ABOUT, words::SWITCH_EVERY_VT_NEW).as_str())
        );
        // the control each hint names is the one on screen
        assert!(words::SWITCH_EVERY_VT.contains(&format!("line {}", words::VTS_LABEL)));
        assert!(words::SWITCH_EVERY_VT.contains("Press e"));
        assert!(
            super::super::super::contract::key_pages("flow.card")
                .is_some_and(|pages| pages.iter().any(|p| p.contains("e revise this path")))
        );
        assert!(words::SWITCH_EVERY_VT_NEW.contains(&format!("line {}", words::VTS_LABEL)));
        // each hint fits the three lines the card gives it at 80 columns
        for why in [
            words::SWITCH_CLOSED,
            words::SWITCH_ISOLATED,
            words::SWITCH_EVERY_VT,
            words::SWITCH_EVERY_VT_NEW,
            words::SWITCH_PAST_12,
        ] {
            let text = format!("{} {why}", words::SWITCH_ABOUT);
            assert!(
                super::super::super::canvas::wrap(&text, 76, 9).len() <= 3,
                "{text}"
            );
        }
    }

    // vtium replaces a first word written `$NAME` with that variable, so
    // the check takes it as vtium does; `$$` is a literal `$`
    #[test]
    fn a_login_command_may_start_with_a_variable() {
        assert_eq!(paths::check_login("$SHELL -l"), Ok("$SHELL -l".to_owned()));
        assert_eq!(
            paths::check_login("$$SHELL"),
            Err(words::login_not_absolute("$$SHELL"))
        );
    }

    #[test]
    fn the_login_in_effect_follows_the_top_level() {
        let mut f = flow();
        assert_eq!(paths::login_in_effect(&f), "/bin/login -p");
        f.base.set.push(("no-reset-env".into(), String::new()));
        assert_eq!(paths::login_in_effect(&f), "/bin/login");
        f.base.set.push(("reset-env".into(), String::new()));
        assert_eq!(paths::login_in_effect(&f), "/bin/login -p");
        f.base.set.push(("login".into(), "/usr/bin/fish".into()));
        assert_eq!(paths::login_in_effect(&f), "/usr/bin/fish");
    }
}
