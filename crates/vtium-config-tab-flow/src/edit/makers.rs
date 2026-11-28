//! The part makers. Enter on a template in PARTS, or on an empty place in
//! NEW PATH, opens its stage's maker, which asks one question and lists
//! the choices that answer it: the whole-kind and nothing choices first,
//! then, under OR CHOOSE BY, the ways to choose by one match key, each of
//! which opens its picker ([`super::pickers`]). A choice makes a part,
//! which joins its group in PARTS; from NEW PATH it also goes into the new
//! path. New terminal… makes a terminal with the next free name, and from
//! NEW PATH opens the new terminal's card.
//!
//! Making a part changes no document: the parts are the tab's own until a
//! path that holds them is added.
//!
//! References:
//! - ADR-1403 D7, "Templates and the part makers": the makers' headings,
//!   questions, choices and words, and what a choice makes.
//! - ADR-1403 D7, "The nothing choices": No input and Nowhere.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::ui::{Action, Change, Command, Request};
use vtium_config_widgets::canvas::{cells, fit};
use vtium_config_widgets::roles::Role;

use super::card::Card;
use super::pickers::{self, Pick, Picker};
use super::{Typed, plan, typing, verb, words};
use crate::page::Model;
use crate::page::draw::Sheet;
use crate::page::layout::{Obj, Slot};
use crate::parts::{self, Stage};
use crate::paths::{ANY_KEYBOARD, ANY_POINTER, Piece};
use crate::{FlowTab, Level, say};

/// An open part maker: its stage, whether it was opened from NEW PATH, the
/// choice under the cursor, and its picker, while one is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maker {
    pub stage: Stage,
    pub from_new: bool,
    pub line: usize,
    pub picker: Option<Picker>,
}

impl Maker {
    /// The node of the interface tree the maker is on.
    pub fn node(&self) -> &'static str {
        match &self.picker {
            None => "flow.maker",
            Some(Picker { typing: None, .. }) => "flow.maker.picker",
            Some(_) => "flow.maker.picker.typing",
        }
    }
}

/// What choosing a maker's choice does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Make {
    /// Makes this part, as All keyboards or No input.
    Part(Piece),
    /// Opens the picker of the values of this match key.
    Filter(&'static str),
    /// Makes a terminal of its own, with the next free name.
    NewTerminal,
}

/// One choice of a maker: its label, what it catches, and what choosing
/// it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub about: String,
    pub make: Make,
}

/// The choices that match by one key, each with its key, its label and
/// what it catches: a display's first, then an input's.
const FILTERS: [(&str, &str, &str); 11] = [
    (
        "edid",
        "This monitor",
        "one monitor by its EDID, on any port",
    ),
    (
        "connector",
        "Connector",
        "the display on one connector, such as HDMI-A-1",
    ),
    (
        "connector-path",
        "Hub port",
        "the display on one port of a DisplayPort hub",
    ),
    (
        "card",
        "Graphics card",
        "every display on one graphics card",
    ),
    ("path", "USB port", "devices plugged into one USB port"),
    (
        "serial",
        "This one device",
        "one device by its serial number, in any port",
    ),
    ("id", "Model", "every device of one make and model"),
    (
        "bluetooth",
        "Bluetooth device",
        "one paired Bluetooth device",
    ),
    ("name", "Device name", "devices that report a given name"),
    (
        "class",
        "Device type",
        "touchscreens, tablets, joysticks and other kinds",
    ),
    (
        "udev",
        "udev property",
        "devices with a given udev property",
    ),
];

/// The keys a display's choices match by; every other key is an input's.
const DISPLAY_KEYS: [&str; 4] = ["edid", "connector", "connector-path", "card"];

/// The choices of the maker of `stage`: the whole-kind and nothing parts
/// first, then the choices that match by one key. `looks` are the names
/// of the draft's looks, which a terminal can draw with.
pub fn choices(stage: Stage, looks: &[String]) -> Vec<Choice> {
    use crate::words as w;
    let one = |label: &str, about: &str, make: Make| Choice {
        label: label.to_owned(),
        about: about.to_owned(),
        make,
    };
    let input = |s: &str| Make::Part(Piece::Input(s.to_owned()));
    let filters = |display: bool| {
        FILTERS
            .iter()
            .filter(move |(key, _, _)| DISPLAY_KEYS.contains(key) == display)
            .map(move |(key, label, about)| one(label, about, Make::Filter(key)))
    };
    match stage {
        Stage::Display => std::iter::once(one(
            w::ANY_DISPLAY,
            w::EVERY_DISPLAY,
            Make::Part(Piece::Display("*".to_owned())),
        ))
        .chain(filters(true))
        .collect(),
        Stage::Input => [
            one(w::NO_INPUT, w::NO_INPUT_DETAIL, Make::Part(Piece::NoInput)),
            one(
                w::ANY_KEYBOARD_TITLE,
                w::EVERY_KEYBOARD,
                input(ANY_KEYBOARD),
            ),
            one(w::ANY_POINTER_TITLE, w::EVERY_POINTER, input(ANY_POINTER)),
            one(w::ANY_INPUT_TITLE, w::EVERY_INPUT, input("*")),
        ]
        .into_iter()
        .chain(filters(false))
        .collect(),
        Stage::Terminal => vec![
            one(
                words::NEW_TERMINAL_CHOICE,
                w::NEW_TERMINAL_DETAIL,
                Make::NewTerminal,
            ),
            one(
                w::ACTIVE_TERMINAL,
                w::ACTIVE_DETAIL,
                Make::Part(Piece::Active),
            ),
            one(
                w::NO_TERMINAL,
                w::NO_TERMINAL_DETAIL,
                Make::Part(Piece::NoTerminal),
            ),
        ],
        Stage::Look => std::iter::once(one(
            w::DEFAULT_LOOK_PART,
            w::DEFAULT_LOOK_DETAIL,
            Make::Part(Piece::Look(None)),
        ))
        .chain(
            looks
                .iter()
                .map(|l| one(l, w::LOOK, Make::Part(Piece::Look(Some(l.clone()))))),
        )
        .collect(),
    }
}

/// The names of the looks `model`'s draft holds.
pub fn looks(model: &Model) -> Vec<String> {
    model.flow.looks.iter().map(|l| l.name.clone()).collect()
}

/// The rows above a maker's first choice: the breadcrumb, a blank row and
/// the heading.
const TOP: usize = 3;

/// Where each of a maker's choices is drawn, by its row below [`TOP`]:
/// the label OR CHOOSE BY takes a row before the first choice that
/// filters.
fn rows(list: &[Choice]) -> Vec<usize> {
    let first_filter = list.iter().position(|c| matches!(c.make, Make::Filter(_)));
    (0..list.len())
        .map(|i| match first_filter {
            Some(f) if i >= f => i.saturating_add(1),
            _ => i,
        })
        .collect()
}

impl FlowTab {
    /// Opens the maker of `stage`, from NEW PATH where `from_new` holds.
    pub(crate) fn open_maker(&mut self, stage: Stage, from_new: bool) -> Vec<Command> {
        self.open_level(Level::Maker(Box::new(Maker {
            stage,
            from_new,
            line: 0,
            picker: None,
        })))
    }

    /// An action on an open maker or its picker.
    pub(crate) fn maker_action(
        &mut self,
        model: &Model,
        mut maker: Maker,
        action: &Action,
    ) -> Vec<Command> {
        if let Some(p) = maker.picker.as_mut()
            && let Some(typed) = p.typing.as_mut()
        {
            return match typing(typed, action, 200) {
                Typed::Edited => {
                    self.level = Level::Maker(Box::new(maker));
                    Vec::new()
                }
                Typed::Ignored => Vec::new(),
                Typed::Cancel => {
                    p.typing = None;
                    self.level = Level::Maker(Box::new(maker));
                    vec![Command::Change(Change::Quiet)]
                }
                Typed::Keep => {
                    let value = pickers::keyed(p.key, typed);
                    match pickers::check_typed(maker.stage, p.key, &value) {
                        Ok(sel) => {
                            let piece = part_of(maker.stage, sel.clone());
                            self.make_part(model, piece, sel, maker.from_new)
                        }
                        Err(why) => say(why),
                    }
                }
            };
        }
        let rows = usize::from(self.viewer().height);
        let page = rows.saturating_sub(TOP).max(1);
        let looks = looks(model);
        let count = match &maker.picker {
            Some(p) => pickers::values(&model.flow, maker.stage, p.key).len(),
            None => choices(maker.stage, &looks).len(),
        };
        let line = maker.picker.as_ref().map_or(maker.line, |p| p.line);
        let last = count.saturating_sub(1);
        let to = match verb(action) {
            "up" | "wheel-up" => line.saturating_sub(1),
            "down" | "wheel-down" => line.saturating_add(1).min(last),
            "home" => 0,
            "end" => last,
            "pgup" => line.saturating_sub(page),
            "pgdn" => line.saturating_add(page).min(last),
            "esc" => return self.maker_back(maker),
            "enter" | "list" => return self.maker_enter(model, maker, line),
            "click" => return self.maker_click(model, maker, action),
            _ => return Vec::new(),
        };
        match maker.picker.as_mut() {
            Some(p) => p.line = to,
            None => maker.line = to,
        }
        self.level = Level::Maker(Box::new(maker));
        Vec::new()
    }

    /// Esc on a maker: from its picker back to the maker's choice that
    /// filters, and from the maker back to the page.
    fn maker_back(&mut self, mut maker: Maker) -> Vec<Command> {
        let Some(p) = maker.picker.take() else {
            return self.close_level();
        };
        maker.line = choices(maker.stage, &[])
            .iter()
            .position(|c| c.make == Make::Filter(p.key))
            .unwrap_or(0);
        self.level = Level::Maker(Box::new(maker));
        Vec::new()
    }

    /// A click on a maker's choice or a picker's value highlights it, and
    /// a click on the highlighted one chooses it, as Enter does.
    fn maker_click(&mut self, model: &Model, maker: Maker, action: &Action) -> Vec<Command> {
        let Some((x, y)) = super::clicked(action) else {
            return Vec::new();
        };
        let viewer = self.viewer();
        let inside = x >= usize::from(viewer.x) && x < usize::from(viewer.right());
        let Some(row) = y
            .checked_sub(usize::from(viewer.y).saturating_add(TOP))
            .filter(|_| inside)
        else {
            return Vec::new();
        };
        let rows = usize::from(viewer.height);
        let hit = match &maker.picker {
            Some(p) => {
                let first = pickers::first_shown(p.line, rows);
                let at = first.saturating_add(row);
                (at < pickers::values(&model.flow, maker.stage, p.key).len()).then_some(at)
            }
            None => rows_of(maker.stage, &looks(model))
                .iter()
                .position(|r| *r == row),
        };
        let Some(at) = hit else {
            return Vec::new();
        };
        let line = maker.picker.as_ref().map_or(maker.line, |p| p.line);
        if at == line {
            return self.maker_enter(model, maker, line);
        }
        let mut maker = maker;
        match maker.picker.as_mut() {
            Some(p) => p.line = at,
            None => maker.line = at,
        }
        self.level = Level::Maker(Box::new(maker));
        Vec::new()
    }

    /// Enter on a maker's choice `line`, or on its picker's value `line`:
    /// makes the part, opens the picker, or starts typing a value.
    fn maker_enter(&mut self, model: &Model, mut maker: Maker, line: usize) -> Vec<Command> {
        if let Some(p) = maker.picker.as_mut() {
            return match pickers::values(&model.flow, maker.stage, p.key).get(line) {
                Some(Pick::Value { sel, .. }) => {
                    let piece = part_of(maker.stage, sel.clone());
                    let title = parts::title_of(&model.flow, &model.own, &piece);
                    self.make_part(model, piece, title, maker.from_new)
                }
                Some(Pick::Typed) => {
                    p.typing = Some(String::new());
                    self.level = Level::Maker(Box::new(maker));
                    vec![Command::Change(Change::Quiet)]
                }
                None => Vec::new(),
            };
        }
        let Some(choice) = choices(maker.stage, &looks(model)).into_iter().nth(line) else {
            return Vec::new();
        };
        match choice.make {
            Make::Part(piece) => {
                let title = parts::title_of(&model.flow, &model.own, &piece);
                self.make_part(model, piece, title, maker.from_new)
            }
            Make::Filter(key) => {
                maker.picker = Some(Picker {
                    key,
                    label: FILTERS
                        .iter()
                        .find(|(k, _, _)| *k == key)
                        .map_or("udev property", |(_, l, _)| l),
                    line: 0,
                    typing: None,
                });
                self.level = Level::Maker(Box::new(maker));
                vec![Command::Change(Change::Quiet)]
            }
            Make::NewTerminal => {
                let made: Vec<String> = self
                    .state
                    .parts
                    .iter()
                    .filter_map(|p| match &p.piece {
                        Piece::Terminal(n) => Some(n.clone()),
                        _ => None,
                    })
                    .collect();
                let name = plan::new_name_besides(&model.flow, &model.doc, &made);
                let mut out =
                    self.make_part(model, Piece::Terminal(name.clone()), name, maker.from_new);
                if maker.from_new {
                    let card = Card::new(Obj::New(Slot::Terminal), &self.state.new, model);
                    self.level = Level::Card(Box::new(card));
                    out.push(Command::Request(Request::Open("flow.new-card")));
                }
                out
            }
        }
    }

    /// Makes the part `piece`, named `title`, and adds it to PARTS, or
    /// finds the part PARTS holds with the same match, and closes the
    /// maker. From NEW PATH the part also goes into the new path, where
    /// the highlight moves to it; else the highlight moves to it in PARTS.
    pub(crate) fn make_part(
        &mut self,
        model: &Model,
        piece: Piece,
        title: String,
        from_new: bool,
    ) -> Vec<Command> {
        let mut out = self.close_level();
        let there = self.state.new.has(&piece);
        let at = parts::make(&mut self.state.parts, &model.flow, piece.clone(), title);
        let name = self
            .state
            .parts
            .get(at)
            .map(|p| p.title.clone())
            .unwrap_or_default();
        if !from_new {
            self.state.at = Some(Obj::Pal(Slot::of(&piece)));
            out.extend(say(words::part_made(&name)));
            return out;
        }
        let at = match &piece {
            Piece::Display(s) => Obj::New(Slot::Display(s.clone())),
            Piece::Input(s) => Obj::New(Slot::Input(s.clone())),
            Piece::NoInput => Obj::New(Slot::Special(Piece::NoInput)),
            _ => Obj::New(Slot::Terminal),
        };
        if !there {
            out.extend(self.pick(model, piece));
        }
        self.state.at = Some(at);
        out
    }
}

/// The part of `stage` that matches by `sel`.
fn part_of(stage: Stage, sel: String) -> Piece {
    match stage {
        Stage::Display => Piece::Display(sel),
        _ => Piece::Input(sel),
    }
}

/// The rows below [`TOP`] where the maker of `stage` draws its choices.
fn rows_of(stage: Stage, looks: &[String]) -> Vec<usize> {
    rows(&choices(stage, looks))
}

/// Draws the maker `m` into `area`: its heading and question, then its
/// choices, the cursor's highlighted; or its picker, while one is open.
pub fn draw(model: &Model, m: &Maker, buf: &mut Buffer, area: Rect, color: bool) {
    if let Some(p) = &m.picker {
        pickers::draw(&model.flow, m.stage, p, buf, area, color);
        return;
    }
    let list = choices(m.stage, &looks(model));
    let at = rows(&list);
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    let mut s = Sheet::new(buf, area, color);
    s.crumb(m.stage.name());
    let (heading, question) = words::maker_heading(m.stage.name());
    let x = s.put_bold(1, 2, &heading, Role::Heading);
    s.put(
        x.saturating_add(3),
        2,
        &fit(question, width.saturating_sub(x).saturating_sub(4)),
        Role::Dim,
    );
    let label_w = list
        .iter()
        .map(|c| cells(&c.label))
        .max()
        .unwrap_or(0)
        .min(width / 2);
    // the window of rows scrolls just enough to show the cursor's choice
    let room = height.saturating_sub(TOP).max(1);
    let cursor_row = at.get(m.line).copied().unwrap_or(0);
    let first = cursor_row.saturating_add(1).saturating_sub(room);
    if let Some(f) = list.iter().position(|c| matches!(c.make, Make::Filter(_)))
        && let Some(row) = at.get(f).and_then(|r| r.checked_sub(1))
        && row >= first
    {
        s.put_bold(
            3,
            TOP.saturating_add(row - first),
            words::OR_CHOOSE_BY,
            Role::Heading,
        );
    }
    for (i, c) in list.iter().enumerate() {
        let Some(row) = at.get(i).and_then(|r| r.checked_sub(first)) else {
            continue;
        };
        let y = TOP.saturating_add(row);
        if y >= height {
            break;
        }
        let cursor = i == m.line;
        if cursor {
            s.highlight(y);
        }
        let role = if cursor {
            Role::RowHighlight
        } else {
            Role::Row
        };
        let dim = if cursor {
            Role::RowHighlight
        } else {
            Role::Dim
        };
        s.put(3, y, "○", dim);
        s.put(5, y, &fit(&c.label, label_w), role);
        let dx = 5usize.saturating_add(label_w).saturating_add(3);
        let x = s.put(
            dx,
            y,
            &fit(&c.about, width.saturating_sub(dx).saturating_sub(1)),
            dim,
        );
        if c.make == Make::Filter("udev") {
            s.put(x.saturating_add(1), y, words::ADVANCED, dim);
        }
    }
}

/// The full text behind the maker's highlighted choice, for F1: its
/// label, what it catches, and what choosing it does.
pub fn full_text(model: &Model, m: &Maker) -> Option<String> {
    if let Some(p) = &m.picker {
        return match pickers::values(&model.flow, m.stage, p.key).get(p.line)? {
            Pick::Value { sel, devices } => Some(format!("{sel}: {}.", devices.join(", "))),
            Pick::Typed => Some(format!(
                "{}: {} for {}.",
                words::OTHER,
                words::TYPE_A_VALUE,
                p.label
            )),
        };
    }
    let c = choices(m.stage, &looks(model)).into_iter().nth(m.line)?;
    Some(format!("{}: {}.", c.label, c.about))
}
