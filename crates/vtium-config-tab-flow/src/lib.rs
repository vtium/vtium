#![forbid(unsafe_code)]
// A vtium.conf and the devices present are untrusted input, and nothing
// the page lays out from them can panic the tool.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-flow: vtium-config's Flow tab. The tab draws the open
//! configuration as one page of paths from the displays to the inputs, in
//! three sections: CONFIGURATION, the paths vtium.conf holds; NEW PATH,
//! the path being built; and PARTS, the displays, terminals, inputs and
//! looks to build it from. The page scrolls as a whole, and the arrows
//! move the highlight from box to box.
//!
//! [`FlowTab`] implements vtium-config-widgets' `Tab` over
//! vtium-config-core's `Shared`, `Action` and `Command`. It reads the
//! shared state only, and asks for every change by returning commands. Its
//! levels are a part's detail ([`parts`]), a display or an input of a
//! path, read-only ([`info`]), and the list of the other inputs
//! ([`inputs`]). [`page`] lays the page out and draws it, and [`paths`]
//! reads the configuration as paths. [`edit`] holds the editing that adds
//! and revises paths.
//!
//! References:
//! - ADR-1206 D9: each tab is a crate that implements the `Tab` trait,
//!   and the shell holds the tabs as trait objects.
//! - ADR-1206 D2: a tab's update takes an action and returns commands, and
//!   its view draws from the model without changing it.
//! - ADR-1403 D7: the Flow view, its sections, its keys and its mouse.
//! - ADR-1403 D11, "The frame" and "The keys that change": the page in the
//!   viewer, and the draft's letters that open the File menu's items.

pub mod edit;
pub mod info;
pub mod inputs;
pub mod page;
pub mod parts;
pub mod paths;
pub mod words;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::flow::Kind;
use vtium_config_core::ui::{
    Action, Carries, Change, Command, Effect, Live, Message, Op, Request, Shared,
};
use vtium_config_widgets::input::{Button, Key};
use vtium_config_widgets::tab::{FindItem, Tab};

use crate::page::layout::{Layout, Obj, Slot};
use crate::page::{Model, Move, State};
use crate::paths::{Picked, Piece};

/// The viewer the tab lays the page out in until it is first drawn: the
/// viewer of a screen of 100×30, the size vtium-config is designed for.
const FIRST_VIEWER: Rect = Rect::new(0, 2, 100, 26);

/// What the Flow tab shows: the page, or a level open over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Level {
    /// The page of paths.
    Page,
    /// A part's detail, with the cursor on `line`: 0 is the label, and
    /// each line after it a choice that narrows the part.
    Detail { obj: Obj, line: usize },
    /// A display or an input of a path, read-only.
    Info(Obj),
    /// The list of the inputs that are neither keyboards nor pointers.
    Others,
    /// A part's detail with its label being typed.
    Label { obj: Obj, typed: String },
    /// A terminal's card: an added path's, or NEW PATH's.
    Card(Box<edit::card::Card>),
    /// A part maker, with its picker while one is open.
    Maker(Box<edit::makers::Maker>),
}

/// The letters the File menu's items keep on the Flow page, and the level
/// each opens, as the interface tree names it: `r` and `n` ask first, `v`
/// shows the changes, and `o` lists the versions installed before.
///
/// References:
/// - ADR-1403 D11, "The keys that change": `r`, `n`, `v` and `o` keep
///   working in the Flow view and open File › Revert to saved…, New…,
///   Show changes and History ›.
pub const FILE_LETTERS: [(&str, &str); 4] = [
    ("r.view.flow", "any.question.confirm"),
    ("n.view.flow", "any.question.confirm"),
    ("v.view.flow", "any.changes"),
    ("o.view.flow", "any.menu.file.history"),
];

/// The Flow tab: where the highlight is, NEW PATH, the level open, and
/// whether it draws in color.
#[derive(Debug)]
pub struct FlowTab {
    color: bool,
    /// The viewer the tab was last drawn in, which a key and a click are
    /// laid out against, so they land on what the viewer shows.
    viewer: Cell<Rect>,
    state: State,
    level: Level,
    /// The model of the document and the devices it was made from, kept
    /// until either changes, so a key or a frame on a large machine does
    /// not read the configuration again.
    model: RefCell<Option<Cached>>,
}

/// A model and what it was made from.
#[derive(Debug)]
struct Cached {
    text: String,
    present: vtium_config_core::flowmodel::Present,
    revising: Option<edit::plan::Revising>,
    model: Rc<Model>,
}

impl FlowTab {
    /// The tab at its start: nothing highlighted until its first layout,
    /// NEW PATH empty, drawn in color where `color` holds, as
    /// `vtium_config_widgets::color::enabled` says of the terminal.
    pub fn new(color: bool) -> FlowTab {
        FlowTab {
            color,
            viewer: Cell::new(FIRST_VIEWER),
            state: State::default(),
            level: Level::Page,
            model: RefCell::new(None),
        }
    }

    /// The page's model of the shared state: the one kept, while the open
    /// document's text, the devices present and the path NEW PATH revises
    /// are what it was made from, else a new one.
    fn model(&self, shared: &Shared) -> Rc<Model> {
        let text = shared.document().text();
        let revising = self.state.revising.as_ref();
        let mut kept = self.model.borrow_mut();
        if let Some(c) = kept.as_ref()
            && c.text == text
            && c.present == shared.present
            && c.revising.as_ref() == revising
        {
            return Rc::clone(&c.model);
        }
        let model = Rc::new(Model::of(shared, revising));
        *kept = Some(Cached {
            text: text.to_owned(),
            present: shared.present.clone(),
            revising: revising.cloned(),
            model: Rc::clone(&model),
        });
        model
    }

    /// Draws in color from now on where `color` holds, as the shell finds
    /// out from the terminal and `NO_COLOR`.
    pub fn set_color(&mut self, color: bool) {
        self.color = color;
    }

    /// Where the highlight is, NEW PATH, and the parts PARTS keeps.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// The level open over the page, or the page itself.
    pub fn level(&self) -> &Level {
        &self.level
    }

    /// The viewer the tab was last drawn in.
    pub fn viewer(&self) -> Rect {
        self.viewer.get()
    }

    /// The tab's state settled on `model` as the viewer shows it: the parts
    /// merged with the flow's, the highlight back on its box, and the
    /// window where it keeps that box in sight; with the page's layout.
    fn settled(&self, model: &Model) -> (State, Layout) {
        let viewer = self.viewer.get();
        let mut state = self.state.clone();
        state.parts = model.parts(&self.state.parts);
        let layout = model.layout(&state.new, &state.parts, usize::from(viewer.width));
        state.settle(&layout, usize::from(viewer.height));
        (state, layout)
    }

    /// Opens `level` and asks the navigation module to open its node.
    fn open_level(&mut self, level: Level) -> Vec<Command> {
        self.level = level;
        vec![Command::Request(Request::Open(self.node()))]
    }

    /// Closes the level open and comes back to the page, with the same box
    /// highlighted.
    fn close_level(&mut self) -> Vec<Command> {
        self.level = Level::Page;
        vec![Command::Request(Request::Close)]
    }
}

/// A message with nothing behind it.
fn say(line: impl Into<String>) -> Vec<Command> {
    vec![Command::Change(Change::Say(Message {
        line: line.into(),
        more: None,
    }))]
}

/// What `t` says where the live trial cannot run here, or `None` where it
/// can.
fn live_refusal(live: Live) -> Option<Vec<Command>> {
    match live {
        Live::On => None,
        Live::OpsOff => Some(say_more(words::OPS_OFF, words::OPS_OFF_MORE)),
        Live::Asking => Some(say(words::TRIAL_ASKING)),
        Live::NotVtium => Some(say(words::NOT_VTIUM)),
    }
}

/// A message with a longer text behind it, which F1 opens.
fn say_more(line: impl Into<String>, more: impl Into<String>) -> Vec<Command> {
    vec![Command::Change(Change::Say(Message {
        line: line.into(),
        more: Some(more.into()),
    }))]
}

/// The move an action's row makes on the page or on a level's lines.
fn move_of(row: &str) -> Option<Move> {
    let key = row.split('.').next().unwrap_or_default();
    Some(match key {
        "up" | "wheel-up" => Move::Up,
        "down" | "wheel-down" => Move::Down,
        "left" => Move::Left,
        "right" => Move::Right,
        "home" => Move::First,
        "end" => Move::Last,
        _ => return None,
    })
}

impl Tab<Shared, Action, Command> for FlowTab {
    fn name(&self) -> &'static str {
        words::FLOW
    }

    fn letter(&self) -> char {
        'o'
    }

    fn node(&self) -> &'static str {
        match &self.level {
            Level::Page => "flow",
            Level::Detail { line: 0, .. } => "flow.detail",
            Level::Detail { .. } => "flow.detail.choices",
            Level::Info(_) => "flow.info",
            Level::Others => "flow.others",
            Level::Label { .. } => "flow.detail.typing",
            Level::Card(card) => card.node(),
            Level::Maker(maker) => maker.node(),
        }
    }

    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.viewer.set(area);
        let model = self.model(shared);
        let buf = frame.buffer_mut();
        match &self.level {
            Level::Page => {
                let (state, layout) = self.settled(&model);
                page::draw::draw_page(
                    &model.flow,
                    &layout,
                    state.at.as_ref(),
                    state.top,
                    buf,
                    area,
                    self.color,
                );
            }
            Level::Detail { obj, line } => {
                parts::draw_detail(&model, obj, *line, buf, area, self.color);
            }
            Level::Info(obj) => info::draw(&model.flow, &model.doc, obj, buf, area, self.color),
            Level::Others => inputs::draw_others(&model.flow, buf, area, self.color),
            Level::Label { obj, typed } => {
                edit::labels::draw(&model, obj, typed, buf, area, self.color);
            }
            Level::Card(card) => {
                edit::card::draw(shared, &model, &self.state.new, card, buf, area, self.color);
            }
            Level::Maker(maker) => edit::makers::draw(&model, maker, buf, area, self.color),
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        let model = self.model(shared);
        let (state, layout) = self.settled(&model);
        self.state = state;
        if let (true, Carries::Item(item)) = (action.is("enter.typing.find"), &action.carries) {
            return self.go_to_item(&layout, *item);
        }
        match self.level.clone() {
            Level::Page => self.page_action(shared, &model, &layout, &action),
            Level::Detail { obj, line } => self.detail_action(&model, &obj, line, &action),
            Level::Info(obj) => match action.row {
                "esc.layer" => self.close_level(),
                "e.layer.flow-info" => self.revise(&model, &obj),
                _ => Vec::new(),
            },
            Level::Others => match action.row {
                "esc.layer" => self.close_level(),
                _ => Vec::new(),
            },
            Level::Label { obj, typed } => self.label_key(shared, &model, &obj, typed, &action),
            Level::Card(card) => self.card_action(shared, &model, *card, &action),
            Level::Maker(maker) => self.maker_action(&model, *maker, &action),
        }
    }

    fn full_text(&self, shared: &Shared) -> Option<String> {
        let model = self.model(shared);
        match &self.level {
            Level::Page => {
                let (state, layout) = self.settled(&model);
                let b = state.at.as_ref().and_then(|o| layout.find(o))?;
                let about = about(&b.obj, &state);
                Some(match about {
                    Some(more) => format!("{}: {}. {more}", b.title, b.detail),
                    None => format!("{}: {}.", b.title, b.detail),
                })
            }
            Level::Detail { obj, line } => {
                let list = parts::choices(&model.flow, obj);
                match line.checked_sub(1) {
                    None => {
                        let (kind, sel) = parts::detail_target(obj)?;
                        let about = match kind {
                            Kind::Display => words::DISPLAY_LABEL_ABOUT,
                            Kind::Input => words::INPUT_LABEL_ABOUT,
                        };
                        let label = model.doc.label(kind, sel).unwrap_or(words::NO_LABEL);
                        Some(format!("{}: {label}, {about}.", words::LABEL_FIELD))
                    }
                    Some(i) => list.get(i).map(|(t, d, s)| format!("{t}: {d} ({s})")),
                }
            }
            Level::Info(obj) => {
                let (kind, sel, heading) = info::target(obj)?;
                let (title, detail) = paths::title(&model.flow, kind, sel);
                Some(format!(
                    "{heading} {title}, {detail}. {}: {sel}. {}: {}.",
                    words::MATCHED_BY,
                    words::WHERE_IT_GOES,
                    info::goes(&model.doc, obj)
                ))
            }
            Level::Others => Some(inputs::others_text(&model.flow)),
            Level::Label { .. } => None,
            Level::Card(card) => edit::card::full_text(&model, &self.state.new, card),
            Level::Maker(maker) => edit::makers::full_text(&model, maker),
        }
    }

    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        let model = self.model(shared);
        let (_, layout) = self.settled(&model);
        found(&layout)
            .into_iter()
            .enumerate()
            .map(|(index, b)| FindItem {
                name: b.title.trim_start_matches("✓ ").to_owned(),
                description: (!b.detail.is_empty()).then(|| b.detail.clone()),
                place: words::place(section(&b.obj)),
                fact: kind_word(&b.obj).to_owned(),
                index,
            })
            .collect()
    }
}

/// The boxes Find searches, in reading order: every box that holds a part
/// or a terminal, leaving out the templates and NEW PATH's empty places.
fn found(layout: &Layout) -> Vec<&page::layout::Boxed> {
    layout
        .reading()
        .into_iter()
        .filter(|b| !b.template)
        .collect()
}

/// The section of the page a box is in.
fn section(obj: &Obj) -> &'static str {
    match obj {
        Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_) => words::CONFIGURATION,
        Obj::New(_) => words::NEW_PATH,
        Obj::Pal(_) | Obj::Others => words::PARTS,
    }
}

/// The kind of box, the fact Find shows beside it.
fn kind_word(obj: &Obj) -> &'static str {
    match page::colors::obj_stage(obj) {
        parts::Stage::Display => "display",
        parts::Stage::Terminal => "terminal",
        parts::Stage::Input => "input",
        parts::Stage::Look => "look",
    }
}

/// The longer text about a box that Enter on it says in a line, for F1.
fn about(obj: &Obj, state: &State) -> Option<&'static str> {
    match obj {
        Obj::Roam(Slot::Terminal) => Some(words::ACTIVE_MORE),
        Obj::New(Slot::Terminal) if state.new.active => Some(words::ACTIVE_MORE),
        Obj::Pal(Slot::Special(p)) | Obj::Doc(_, Slot::Special(p)) | Obj::New(Slot::Special(p)) => {
            match p {
                Piece::NoInput => Some(words::NO_INPUT_MORE),
                Piece::Active => Some(words::ACTIVE_MORE),
                Piece::NoTerminal => Some(words::NO_TERMINAL_ABOUT),
                _ => None,
            }
        }
        Obj::Nothing(Slot::Terminal) => Some(words::NO_TERMINAL_ABOUT),
        Obj::Roam(_) => Some(words::REVISE_ROAMING_NOTE),
        Obj::Doc(..) | Obj::Nothing(_) => Some(words::REVISE_NOTE),
        _ => None,
    }
}

impl FlowTab {
    /// The rows the viewer has, which a page of the window takes.
    fn window(&self) -> usize {
        usize::from(self.viewer.get().height)
    }

    /// An action on the page.
    fn page_action(
        &mut self,
        shared: &Shared,
        model: &Model,
        layout: &Layout,
        action: &Action,
    ) -> Vec<Command> {
        let quiet = vec![Command::Change(Change::Quiet)];
        if let Some(to) = move_of(action.row).filter(|_| action.row.ends_with(".view.flow")) {
            if let Some(next) = self
                .state
                .at
                .as_ref()
                .and_then(|a| page::step(layout, a, to))
            {
                self.state.go(layout, next, self.window());
            }
            return quiet;
        }
        if let Some((_, node)) = FILE_LETTERS.iter().find(|(row, _)| action.is(row)) {
            return vec![Command::Request(Request::Open(node))];
        }
        match action.row {
            "pgup.view.flow" | "pgdn.view.flow" => {
                let down = action.row == "pgdn.view.flow";
                self.state.page(layout, down, self.window());
                quiet
            }
            "space.view.flow" => self.space(model),
            "enter.view.flow" => self.open(model),
            "esc.view.flow" if self.state.revising.is_some() => self.drop_revised(shared, model),
            "esc.view.flow" if self.state.new.is_empty() => say(words::NOTHING_TO_CLEAR),
            "esc.view.flow" => {
                self.state.new = paths::Path::default();
                say(words::NEW_PATH_CLEARED)
            }
            "d.view.flow" => vec![Command::Effect(Effect::Desk(Op::Devices))],
            "t.view.flow" => {
                live_refusal(shared.console.live).unwrap_or_else(|| say(words::LIVE_ON))
            }
            "p.view.flow" => match shared.console.live {
                Live::On | Live::OpsOff => {
                    say_more(words::PREVIEW_LATER, words::PREVIEW_LATER_MORE)
                }
                Live::Asking => say(words::PREVIEW_ASKING),
                Live::NotVtium => say(words::PREVIEW_NOT_HERE),
            },
            "click-box.view.flow" => self.click(model, layout, &action.carries),
            "a.view.flow" => self.add(shared, model),
            "e.view.flow" => match self.state.at.clone() {
                Some(at) => self.revise(model, &at),
                None => say(edit::words::REVISE_WHERE),
            },
            _ => Vec::new(),
        }
    }

    /// A click on the page: a click on a box highlights it, and a click on
    /// the highlighted box opens it, as Enter does.
    fn click(&mut self, model: &Model, layout: &Layout, carries: &Carries) -> Vec<Command> {
        let Carries::Key(Key::Mouse(m)) = carries else {
            return Vec::new();
        };
        if m.button != Button::Press(0) {
            return Vec::new();
        }
        let viewer = self.viewer.get();
        let (Some(x), Some(y)) = (
            m.col.checked_sub(usize::from(viewer.x)),
            m.row.checked_sub(usize::from(viewer.y)),
        ) else {
            return Vec::new();
        };
        if x >= usize::from(viewer.width) || y >= usize::from(viewer.height) {
            return Vec::new();
        }
        let top = self.state.top;
        let end = top.saturating_add(self.window());
        // only a box whose title shows is drawn, so only it takes a click
        let Some(b) = layout
            .at(x, top.saturating_add(y))
            .filter(|b| b.title_row() >= top && b.title_row() < end)
        else {
            return Vec::new();
        };
        if self.state.at.as_ref() == Some(&b.obj) {
            return self.open(model);
        }
        let obj = b.obj.clone();
        self.state.go(layout, obj, self.window());
        vec![Command::Change(Change::Quiet)]
    }

    /// Goes to Find's match `item`: closes any level and highlights the box.
    fn go_to_item(&mut self, layout: &Layout, item: usize) -> Vec<Command> {
        let Some(obj) = found(layout).get(item).map(|b| b.obj.clone()) else {
            return Vec::new();
        };
        self.level = Level::Page;
        self.state.go(layout, obj, self.window());
        Vec::new()
    }

    /// Enter on the page: opens the highlighted box's level, says what a
    /// special part is, or shows a look's font set in the Looks tab.
    fn open(&mut self, model: &Model) -> Vec<Command> {
        let Some(obj) = self.state.at.clone() else {
            return Vec::new();
        };
        let new = &self.state.new;
        match &obj {
            Obj::Roam(Slot::Terminal) => say_more(words::ACTIVE_ABOUT, words::ACTIVE_MORE),
            Obj::New(Slot::Terminal) if new.active => {
                say_more(words::ACTIVE_ABOUT, words::ACTIVE_MORE)
            }
            Obj::Nothing(Slot::Terminal) => say(words::NO_TERMINAL_ABOUT),
            Obj::New(Slot::Terminal) if new.none => say(words::NO_TERMINAL_ABOUT),
            Obj::Pal(Slot::Special(p))
            | Obj::Doc(_, Slot::Special(p))
            | Obj::New(Slot::Special(p)) => match p {
                Piece::NoInput => say_more(words::NO_INPUT_ABOUT, words::NO_INPUT_MORE),
                Piece::Active => say_more(words::ACTIVE_ABOUT, words::ACTIVE_MORE),
                Piece::NoTerminal => say(words::NO_TERMINAL_ABOUT),
                other => say(words::new_terminal_about(&parts::special_title(other))),
            },
            Obj::Pal(Slot::Look(l)) => {
                let frit = model
                    .flow
                    .looks
                    .iter()
                    .find(|x| x.name == l.as_deref().unwrap_or("default"))
                    .map(|x| x.frit.clone())
                    .filter(|f| !f.is_empty());
                match frit {
                    Some(frit) => vec![Command::Request(Request::Show {
                        tab: "looks",
                        name: frit,
                    })],
                    None => say(words::DEFAULT_FONT_PREVIEW),
                }
            }
            Obj::Doc(_, Slot::Display(_) | Slot::Input(_))
            | Obj::Roam(Slot::Input(_))
            | Obj::Nothing(Slot::Display(_) | Slot::Input(_)) => self.open_level(Level::Info(obj)),
            Obj::Others => self.open_level(Level::Others),
            Obj::Pal(Slot::Display(_) | Slot::Input(_)) => {
                self.open_level(Level::Detail { obj, line: 0 })
            }
            Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => self.open_card(model, obj),
            Obj::Pal(Slot::Template(stage)) => self.open_maker(*stage, false),
            Obj::New(Slot::Empty(stage)) => self.open_maker(*stage, true),
            // a part of NEW PATH opens on its own choice
            Obj::New(Slot::Display(s) | Slot::Input(s)) => {
                let line = parts::choices(&model.flow, &obj)
                    .iter()
                    .position(|(_, _, v)| v == s)
                    .map_or(0, |at| at.saturating_add(1));
                self.open_level(Level::Detail { obj, line })
            }
            _ => Vec::new(),
        }
    }

    /// An action on a part's detail, whose line 0 is the label and whose
    /// other lines are the choices that narrow the part.
    fn detail_action(
        &mut self,
        model: &Model,
        obj: &Obj,
        line: usize,
        action: &Action,
    ) -> Vec<Command> {
        let count = parts::choices(&model.flow, obj).len();
        let rows = usize::from(self.viewer.get().height);
        let page = rows.saturating_sub(parts::CHOICES_TOP).max(1);
        let to = match action.row {
            "esc.layer" | "esc.layer.flow-detail" => return self.close_level(),
            "down.layer.flow-detail" | "down.layer.flow-choices" | "wheel-down.layer" => {
                line.saturating_add(1).min(count)
            }
            "up.layer.flow-choices" | "wheel-up.layer" => line.saturating_sub(1),
            "home.layer.flow-choices" => 0,
            "end.layer.flow-detail" | "end.layer.flow-choices" => count,
            "pgdn.layer.flow-detail" | "pgdn.layer.flow-choices" => {
                line.saturating_add(page).min(count)
            }
            "pgup.layer.flow-choices" => line.saturating_sub(page),
            "enter.layer.flow-detail" => return self.start_label(model, obj),
            "enter.layer.flow-choices" => {
                return match line.checked_sub(1) {
                    Some(at) => self.choose(model, obj, at),
                    None => self.start_label(model, obj),
                };
            }
            _ => return Vec::new(),
        };
        self.level = Level::Detail {
            obj: obj.clone(),
            line: to,
        };
        Vec::new()
    }

    /// Space on the page: picks a part into NEW PATH or takes it out, and
    /// says why anything else cannot be picked.
    fn space(&mut self, model: &Model) -> Vec<Command> {
        let Some(obj) = self.state.at.clone() else {
            return Vec::new();
        };
        let (flow, doc) = (&model.flow, &model.doc);
        let new = &self.state.new;
        match &obj {
            Obj::Pal(Slot::Template(st)) => say(words::template_space(st.name())),
            Obj::New(Slot::Empty(_)) => say(words::EMPTY_SPACE),
            Obj::Pal(Slot::Display(_)) if new.active => {
                say_more(words::ACTIVE_NO_DISPLAY, words::ACTIVE_NO_DISPLAY_MORE)
            }
            Obj::Pal(Slot::Special(Piece::Active)) if !new.active && !new.displays.is_empty() => {
                say_more(words::ACTIVE_NO_DISPLAY, words::ACTIVE_NO_DISPLAY_MORE)
            }
            Obj::Pal(Slot::Special(Piece::NoInput)) if new.active || new.none => {
                say(words::NO_INPUT_NEEDS_TERMINAL)
            }
            Obj::Pal(Slot::Special(p)) | Obj::New(Slot::Special(p)) => self.pick(model, p.clone()),
            Obj::Pal(Slot::Display(sel)) if sel.is_empty() => {
                // a display with no key vtium can match it by, as one that
                // reports no EDID where no udev daemon runs, would make a
                // rule with no match, which vtium refuses
                let name = flow
                    .displays
                    .iter()
                    .find(|d| d.found && paths::own_key(flow, Kind::Display, d).is_empty())
                    .map_or_else(|| "this display".to_owned(), |d| d.name.clone());
                say_more(
                    words::display_no_match(&name),
                    words::display_no_match_more(&name),
                )
            }
            Obj::Pal(Slot::Display(sel)) => {
                // a display is in one path for each VT, so picking it is
                // refused only when a path that holds it shares a VT with
                // NEW PATH's terminal
                let held = match flow.displays.iter().find(|d| d.keys.contains(sel)) {
                    Some(d) => inputs::holders(&doc.paths, Kind::Display, d),
                    None => doc
                        .paths
                        .iter()
                        .filter(|p| p.displays.iter().any(|s| s == sel))
                        .filter_map(|p| p.name.clone())
                        .collect(),
                };
                let clash = held.iter().find_map(|h| {
                    let vt = doc.path(h).and_then(|p| new.shared_vt(p))?;
                    Some((h, vt))
                });
                if let (Some((h, vt)), false) = (clash, new.has(&Piece::Display(sel.clone()))) {
                    let (title, _) = paths::title(flow, Kind::Display, sel);
                    let (line, more) = words::display_taken(&title, h, vt);
                    return say_more(line, more);
                }
                self.pick(model, Piece::Display(sel.clone()))
            }
            Obj::Pal(Slot::Input(sel)) | Obj::New(Slot::Input(sel)) => {
                self.pick(model, Piece::Input(sel.clone()))
            }
            Obj::New(Slot::Display(sel)) => self.pick(model, Piece::Display(sel.clone())),
            Obj::Pal(Slot::Look(l)) => self.pick(model, Piece::Look(l.clone())),
            Obj::Others => say(words::OTHERS_CANNOT),
            Obj::Roam(Slot::Input(_)) => say(words::IN_ROAMING),
            Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_) => say(words::IN_DOCUMENT),
            Obj::New(Slot::Terminal) => say(words::NEW_TERMINAL_SPACE),
            _ => Vec::new(),
        }
    }

    /// Picks `piece` into NEW PATH, or takes it out, and says what that
    /// did, whether the input will roam, and what the path needs next.
    fn pick(&mut self, model: &Model, piece: Piece) -> Vec<Command> {
        let (flow, doc) = (&model.flow, &model.doc);
        let new = &mut self.state.new;
        let was_look = new.look.clone();
        let done = match &piece {
            // the default look is the absence of one
            Piece::Look(None) if was_look.is_some() => {
                new.look = None;
                Picked::Replaced(was_look)
            }
            Piece::Look(None) => Picked::Added,
            _ => new.pick_part(flow, piece.clone()),
        };
        let new = new.clone();
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
                    Picked::Removed => words::LOOK_DEFAULT.to_owned(),
                    Picked::Replaced(Some(old)) => words::look_instead(&now, &old),
                    _ => words::look_picked(&now),
                });
            }
        };
        // a device is named as a reader names it, `the 8BitDo Keyboard`;
        // a wider part by its title, All keyboards
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
        let next = match (new.inputs.is_empty(), new.displays.is_empty()) {
            (false, true) => words::Next::Display,
            (true, false) => words::Next::Input,
            _ => words::Next::Complete,
        };
        let (line, more) = words::picked(&title, display, next, pointers, roams);
        say_more(line, more)
    }
}
