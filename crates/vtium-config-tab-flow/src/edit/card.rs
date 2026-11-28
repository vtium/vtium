//! A terminal's card. Enter on the terminal of a path in CONFIGURATION
//! opens its card, which changes in place what allocates no display or
//! input: the terminal's name, its look, its consoles, its login, what
//! happens when its screen is unplugged, and Switch on Enter. Each change
//! there is one change to the document. Enter on NEW PATH's terminal opens
//! the same card for the path being built, which also sets its VTs and
//! takes its parts out, and changes NEW PATH alone.
//!
//! Up and Down move between the lines. A line with two choices shows them
//! in place, Left and Right step between them, and Enter sets the one
//! under the cursor. A line with more choices opens the list box under
//! itself, and a kept pick is one change. Enter on the name, or on the
//! login's Other…, types a value. Under the lines, the card shows what the
//! highlighted line means, how the rest of the path changes, and what the
//! terminal draws: its font set, its look and its displays, and its
//! effects.
//!
//! References:
//! - ADR-1403 D7, "Editing an added path, and Revise": what the card
//!   changes, and that a path's VTs change only through Revise.
//! - ADR-1403 D7, "The terminal's subflow: font set and effects".
//! - ADR-1403 D11, "The list box": the list box under a line, a kept pick
//!   as one change, and Esc closing it unchanged.
//! - ADR-1401 D3, "Enter on a head's own keyboard (`switch-on-enter`)":
//!   when the key has no effect.

use std::borrow::Cow;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_core::flow::Kind;
use vtium_config_core::ui::{Action, Change, Command, Shared};
use vtium_config_widgets::canvas::{cells, fit};
use vtium_config_widgets::list_box::{ListBox, Outcome};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::tab::Choices;

use super::plan;
use super::{Typed, entry, is_enter, typing, verb, words};
use crate::page::Model;
use crate::page::colors::marked;
use crate::page::draw::Sheet;
use crate::page::layout::{Obj, Slot};
use crate::parts;
use crate::paths::{self, Path, Piece};
use crate::{FlowTab, Level, say};

/// The node of the list box under a card's line.
pub const LIST_NODE: &str = "flow.card.list";

/// The cell where a card's values start, after the longest label.
const VALUE_X: usize = 34;

/// The rows above a card's first line: the breadcrumb, a blank row and
/// the heading.
const TOP: usize = 3;

/// The rows under a card's lines: a blank row, what the line means in up
/// to two rows, the note, a blank row, and what the terminal draws in
/// three rows.
const UNDER: usize = 9;

/// The most characters a typed value holds: a terminal's name, or a login
/// command.
const TYPED_MAX: usize = 200;

/// One line of a terminal's card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Name,
    /// The look the terminal draws with, one of the draft's looks.
    Look,
    /// The VTs the terminal runs on, which only NEW PATH's card changes.
    Vts,
    /// A head's key, such as `login`.
    Setting(&'static str),
    /// A part of NEW PATH, which Enter takes out.
    Part(Piece),
}

/// What is open on a card's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Open {
    Nothing,
    /// The list box of the line's choices.
    List(ListBox),
    /// A value being typed: the name, or a login command.
    Typing(String),
}

/// An open card: the terminal box it was opened on, the line under the
/// cursor, the choice under the cursor on a line that shows its choices,
/// and what is open on the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub obj: Obj,
    pub line: usize,
    pub pick: usize,
    pub open: Open,
}

impl Card {
    /// The card of the terminal box `obj`, on its first line.
    pub fn new(obj: Obj, new: &Path, model: &Model) -> Card {
        let mut card = Card {
            obj,
            line: 0,
            pick: 0,
            open: Open::Nothing,
        };
        if let Some(path) = path_of(&card, new, model) {
            card.pick = chosen(model, &path, &Line::Name).unwrap_or(0);
        }
        card
    }

    /// The node of the interface tree the card is on.
    pub fn node(&self) -> &'static str {
        match (&self.open, self.editor()) {
            (Open::List(_), _) => LIST_NODE,
            (Open::Typing(_), _) => "flow.card.typing",
            (Open::Nothing, true) => "flow.new-card",
            (Open::Nothing, false) => "flow.card",
        }
    }

    /// Whether the card is NEW PATH's.
    pub fn editor(&self) -> bool {
        matches!(self.obj, Obj::New(_))
    }
}

/// The path a card shows: an added path's, or NEW PATH's with the name it
/// will take.
pub fn path_of(card: &Card, new: &Path, model: &Model) -> Option<Path> {
    match &card.obj {
        Obj::Doc(name, Slot::Terminal) => model.full.path(name).cloned(),
        Obj::New(Slot::Terminal) => {
            let mut p = new.clone();
            if p.name.is_none() {
                p.name = Some(paths::new_name(&model.flow, &model.doc));
            }
            Some(p)
        }
        _ => None,
    }
}

/// The settings a card offers, besides `isolated`, which the path's
/// inputs decide, and `vt`, its own line.
const CARD_SETTINGS: [&str; 4] = ["consoles", "login", "when-display-lost", "switch-on-enter"];

/// The card's lines for `path`. NEW PATH's card also lists the path's
/// parts.
pub fn lines(path: &Path, editor: bool) -> Vec<Line> {
    let mut out = vec![Line::Name, Line::Look, Line::Vts];
    out.extend(CARD_SETTINGS.iter().map(|k| Line::Setting(k)));
    if editor {
        out.extend(
            path.displays
                .iter()
                .map(|d| Line::Part(Piece::Display(d.clone()))),
        );
        out.extend(
            path.inputs
                .iter()
                .map(|i| Line::Part(Piece::Input(i.clone()))),
        );
        if path.no_input {
            out.push(Line::Part(Piece::NoInput));
        }
    }
    out
}

/// Whether a line shows its choices in place, where Left and Right step
/// between them; a line with more choices opens the list box instead.
fn inline(line: &Line) -> bool {
    matches!(
        line,
        Line::Setting("login" | "when-display-lost" | "switch-on-enter")
    )
}

/// The choices of one card line: each in a reader's words and as the
/// value the path holds for it, empty for the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineChoices {
    pub words: Vec<String>,
    pub values: Vec<String>,
    pub current: Option<usize>,
}

impl Choices<String> for LineChoices {
    fn count(&self) -> usize {
        self.words.len()
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        Cow::Borrowed(self.words.get(choice).map_or("", String::as_str))
    }

    fn current(&self) -> Option<usize> {
        self.current
    }

    fn keep(&self, choice: usize) -> String {
        self.values.get(choice).cloned().unwrap_or_default()
    }
}

/// The value `path` holds for `line`, empty for the default.
fn value(path: &Path, line: &Line) -> String {
    match line {
        Line::Name => path.name.clone().unwrap_or_default(),
        Line::Look => path.look.clone().unwrap_or_default(),
        Line::Vts => path.setting("vt"),
        Line::Setting("consoles") => Some(path.setting("consoles"))
            .filter(|v| v != "1")
            .unwrap_or_default(),
        Line::Setting(k) => path.setting(k),
        Line::Part(_) => String::new(),
    }
}

/// The choices `line` offers on `path`'s card. The Look line of a
/// terminal that names a look the draft does not hold offers that look
/// last, marked, so the line starts on it.
pub fn choices(model: &Model, path: &Path, line: &Line) -> LineChoices {
    let mut words: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut add = |w: String, v: String| {
        words.push(w);
        values.push(v);
    };
    match line {
        Line::Look => {
            add(words::DEFAULT_LOOK.to_owned(), String::new());
            for l in &model.flow.looks {
                add(l.name.clone(), l.name.clone());
            }
            if let Some(l) = missing_look(model, path) {
                add(words::missing_look(l), l.to_owned());
            }
        }
        Line::Vts => {
            add(crate::words::EVERY_VT.to_owned(), String::new());
            for n in 1u16..=12 {
                add(crate::words::vt_words(&[n]), n.to_string());
            }
        }
        Line::Setting("consoles") => {
            add("1".to_owned(), String::new());
            for n in 2..=6 {
                add(n.to_string(), n.to_string());
            }
        }
        Line::Setting("login") => {
            add(
                words::login_default(&plan::login_in_effect(&model.flow)),
                String::new(),
            );
            add(words::LOGIN_OTHER.to_owned(), path.setting("login"));
        }
        Line::Setting("when-display-lost") => {
            add(words::KEEP_SESSIONS.to_owned(), String::new());
            add(words::END_SESSIONS.to_owned(), "hangup".to_owned());
        }
        Line::Setting("switch-on-enter") => {
            add(words::SWITCH_OFF.to_owned(), String::new());
            add(words::SWITCH_ON.to_owned(), "yes".to_owned());
        }
        _ => {}
    }
    let now = value(path, line);
    let current = match line {
        Line::Setting("login") => Some(usize::from(!now.is_empty())),
        _ => values.iter().position(|v| *v == now),
    };
    LineChoices {
        words,
        values,
        current,
    }
}

/// The choice of `line` that `path` holds now.
fn chosen(model: &Model, path: &Path, line: &Line) -> Option<usize> {
    choices(model, path, line).current
}

/// The look `path` names when the draft holds no look of that name.
fn missing_look<'a>(model: &Model, path: &'a Path) -> Option<&'a str> {
    path.look
        .as_deref()
        .filter(|l| !model.flow.looks.iter().any(|x| x.name == *l))
}

/// A line's label.
fn label(model: &Model, line: &Line) -> String {
    match line {
        Line::Name => words::NAME.to_owned(),
        Line::Look => words::LOOK.to_owned(),
        Line::Vts => words::VTS.to_owned(),
        Line::Setting("consoles") => words::CONSOLES.to_owned(),
        Line::Setting("login") => words::LOGIN.to_owned(),
        Line::Setting("when-display-lost") => words::WHEN_UNPLUGGED.to_owned(),
        Line::Setting("switch-on-enter") => words::SWITCH.to_owned(),
        Line::Setting(k) => (*k).to_owned(),
        Line::Part(Piece::Display(d)) => format!(
            "{} {}",
            words::DISPLAY,
            paths::title(&model.flow, Kind::Display, d).0
        ),
        Line::Part(Piece::Input(i)) => format!(
            "{} {}",
            words::INPUT,
            paths::title(&model.flow, Kind::Input, i).0
        ),
        Line::Part(p) => format!("{} {}", words::INPUT, parts::special_title(p)),
    }
}

/// A line's value in a reader's words.
fn shown(model: &Model, path: &Path, line: &Line) -> String {
    match line {
        Line::Name => value(path, line),
        Line::Setting("login") if !path.setting("login").is_empty() => path.setting("login"),
        Line::Part(p) if parts::connected(&model.flow, p) => words::CONNECTED.to_owned(),
        Line::Part(_) => crate::words::NOT_PLUGGED.to_owned(),
        _ => {
            let c = choices(model, path, line);
            c.current
                .and_then(|i| c.words.get(i).cloned())
                .unwrap_or_else(|| value(path, line))
        }
    }
}

/// What the highlighted line means, in one or two rows under the card:
/// the Login line's program, a look the draft does not hold, and Switch
/// on Enter with why it has no effect now, as vtium decides.
fn about(model: &Model, path: &Path, line: &Line, editor: bool) -> Vec<String> {
    match line {
        Line::Look => missing_look(model, path)
            .map(words::missing_look_about)
            .into_iter()
            .collect(),
        Line::Setting("login") => vec![words::LOGIN_ABOUT.to_owned()],
        Line::Setting("switch-on-enter") => {
            let vts = path.vts();
            let why = if model.flow.policy == "closed" {
                Some(words::SWITCH_CLOSED)
            } else if path.no_input || path.setting("isolated") == "yes" {
                Some(words::SWITCH_ISOLATED)
            } else if vts.is_empty() && editor {
                Some(words::SWITCH_EVERY_VT_NEW)
            } else if vts.is_empty() {
                Some(words::SWITCH_EVERY_VT)
            } else if vts.iter().all(|v| *v > 12) {
                Some(words::SWITCH_PAST_12)
            } else {
                None
            };
            std::iter::once(words::SWITCH_ABOUT.to_owned())
                .chain(why.map(str::to_owned))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Where a card's lines are drawn in a viewer `rows` rows tall: the first
/// line shown, and how many lines show.
fn window(card: &Card, count: usize, rows: usize) -> (usize, usize) {
    let room = rows.saturating_sub(TOP).saturating_sub(UNDER).max(3);
    let first = card.line.saturating_add(1).saturating_sub(room);
    (first, room.min(count.saturating_sub(first)))
}

/// The place of the list box under the card's highlighted line, in
/// `area`, the viewer.
fn list_rect(card: &Card, list: &ListBox, c: &LineChoices, count: usize, area: Rect) -> Rect {
    let (first, _) = window(card, count, usize::from(area.height));
    let row = TOP.saturating_add(card.line).saturating_sub(first);
    let line = area
        .y
        .saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    let x = area
        .x
        .saturating_add(u16::try_from(VALUE_X).unwrap_or(u16::MAX))
        .saturating_sub(3);
    list.place(c, area, line, x)
}

impl FlowTab {
    /// Opens the card of the terminal box `obj`.
    pub(crate) fn open_card(&mut self, model: &Model, obj: Obj) -> Vec<Command> {
        let card = Card::new(obj, &self.state.new, model);
        self.open_level(Level::Card(Box::new(card)))
    }

    /// An action on an open card: its lines' keys, the list box's, or the
    /// keys of a value being typed.
    pub(crate) fn card_action(
        &mut self,
        shared: &Shared,
        model: &Model,
        mut card: Card,
        action: &Action,
    ) -> Vec<Command> {
        let Some(path) = path_of(&card, &self.state.new, model) else {
            return self.close_level();
        };
        let all = lines(&path, card.editor());
        card.line = card.line.min(all.len().saturating_sub(1));
        let Some(line) = all.get(card.line).cloned() else {
            return self.close_level();
        };
        let area = self.viewer();
        match std::mem::replace(&mut card.open, Open::Nothing) {
            Open::Typing(mut typed) => match typing(&mut typed, action, TYPED_MAX) {
                Typed::Edited | Typed::Ignored => {
                    card.open = Open::Typing(typed);
                    self.level = Level::Card(Box::new(card));
                    Vec::new()
                }
                Typed::Cancel => {
                    self.level = Level::Card(Box::new(card));
                    vec![Command::Change(Change::Quiet)]
                }
                Typed::Keep if line == Line::Setting("login") => match plan::check_login(&typed) {
                    Ok(command) => self.card_set(shared, model, card, &path, &line, command),
                    Err(why) => {
                        card.open = Open::Typing(typed);
                        self.level = Level::Card(Box::new(card));
                        say(why)
                    }
                },
                Typed::Keep => {
                    let name = typed.trim().to_owned();
                    self.card_set(shared, model, card, &path, &line, name)
                }
            },
            Open::List(mut list) => {
                let c = choices(model, &path, &line);
                let rect = list_rect(&card, &list, &c, all.len(), area);
                let outcome = match (super::clicked(action), super::pressed(action)) {
                    (Some((x, y)), _) => list.click(position(x, y), &c, rect),
                    (None, Some(key)) => list.key(&key, &c, rect),
                    (None, None) => Outcome::Ignored,
                };
                match outcome {
                    Outcome::Kept(v) => self.card_set(shared, model, card, &path, &line, v),
                    Outcome::Closed | Outcome::Outside => {
                        self.level = Level::Card(Box::new(card));
                        Vec::new()
                    }
                    Outcome::Moved | Outcome::Searched | Outcome::Ignored => {
                        card.open = Open::List(list);
                        self.level = Level::Card(Box::new(card));
                        Vec::new()
                    }
                }
            }
            Open::Nothing => self.card_key(shared, model, card, &path, &all, action),
        }
    }

    /// A key on a card's lines.
    fn card_key(
        &mut self,
        shared: &Shared,
        model: &Model,
        mut card: Card,
        path: &Path,
        all: &[Line],
        action: &Action,
    ) -> Vec<Command> {
        let Some(line) = all.get(card.line).cloned() else {
            return Vec::new();
        };
        let last = all.len().saturating_sub(1);
        let c = choices(model, path, &line);
        let moved = |card: &mut Card, to: usize| {
            card.line = to;
            card.pick = all
                .get(to)
                .and_then(|l| chosen(model, path, l))
                .unwrap_or(0);
        };
        match verb(action) {
            "up" | "wheel-up" => {
                let to = card.line.saturating_sub(1);
                moved(&mut card, to);
            }
            "down" | "wheel-down" => {
                let to = card.line.saturating_add(1).min(last);
                moved(&mut card, to);
            }
            "home" | "pgup" => moved(&mut card, 0),
            "end" | "pgdn" => moved(&mut card, last),
            "left" if inline(&line) => card.pick = card.pick.saturating_sub(1),
            "right" if inline(&line) => {
                card.pick = card.pick.saturating_add(1).min(c.count().saturating_sub(1));
            }
            "esc" => return self.close_level(),
            "e" if !card.editor() => {
                let obj = card.obj.clone();
                return self.revise(model, &obj);
            }
            "t" => return try_look(shared, path),
            "click" => return self.card_click(shared, model, card, path, all, action),
            _ if is_enter(action) => return self.card_enter(shared, model, card, path, &line),
            _ => return Vec::new(),
        }
        self.level = Level::Card(Box::new(card));
        Vec::new()
    }

    /// A click on a card's line highlights it, and a click on the
    /// highlighted line acts as Enter does.
    fn card_click(
        &mut self,
        shared: &Shared,
        model: &Model,
        mut card: Card,
        path: &Path,
        all: &[Line],
        action: &Action,
    ) -> Vec<Command> {
        let Some((_, y)) = super::clicked(action) else {
            return Vec::new();
        };
        let area = self.viewer();
        let (first, shown) = window(&card, all.len(), usize::from(area.height));
        let Some(row) = y
            .checked_sub(usize::from(area.y).saturating_add(TOP))
            .filter(|r| *r < shown)
        else {
            return Vec::new();
        };
        let at = first.saturating_add(row);
        if at == card.line {
            let Some(line) = all.get(at).cloned() else {
                return Vec::new();
            };
            return self.card_enter(shared, model, card, path, &line);
        }
        card.line = at;
        card.pick = all
            .get(at)
            .and_then(|l| chosen(model, path, l))
            .unwrap_or(0);
        self.level = Level::Card(Box::new(card));
        Vec::new()
    }

    /// Enter on a card's line: types the name, opens the list box, sets
    /// the choice under the cursor, or takes a part out of NEW PATH.
    fn card_enter(
        &mut self,
        shared: &Shared,
        model: &Model,
        mut card: Card,
        path: &Path,
        line: &Line,
    ) -> Vec<Command> {
        let c = choices(model, path, line);
        let out = match line {
            Line::Name => {
                card.open = Open::Typing(value(path, line));
                vec![Command::Change(Change::Quiet)]
            }
            Line::Vts if !card.editor() => say(words::VTS_READ_ONLY),
            Line::Setting("login") if card.pick == 1 => {
                card.open = Open::Typing(path.setting("login"));
                vec![Command::Change(Change::Quiet)]
            }
            l if inline(l) => {
                let v = c.keep(card.pick);
                return self.card_set(shared, model, card, path, line, v);
            }
            Line::Part(p) => {
                self.state.new.pick(p.clone());
                card.line = card.line.saturating_sub(1);
                vec![Command::Change(Change::Quiet)]
            }
            _ => {
                card.open = Open::List(ListBox::open(LIST_NODE, &c));
                vec![Command::Change(Change::Quiet)]
            }
        };
        self.level = Level::Card(Box::new(card));
        out
    }

    /// Sets the card's line `line` to `value`: on NEW PATH, which the
    /// card's path is; on an added path, as one change to the document,
    /// which [`plan::plan`] makes from the configuration with the path
    /// changed. A name another terminal has, or one no terminal can take,
    /// is refused in one line, and the entry stays to be fixed.
    fn card_set(
        &mut self,
        shared: &Shared,
        model: &Model,
        mut card: Card,
        path: &Path,
        line: &Line,
        value: String,
    ) -> Vec<Command> {
        let own = path.name.clone().unwrap_or_default();
        if *line == Line::Name && value != own {
            let taken = model.full.path(&value).is_some()
                || model.flow.heads.iter().any(|h| h.name == value);
            let why = if !plan::valid_name(&value) {
                Some(words::name_invalid(&value))
            } else if taken {
                Some(words::name_taken(&value))
            } else {
                None
            };
            if let Some(why) = why {
                card.open = Open::Typing(value);
                self.level = Level::Card(Box::new(card));
                return say(why);
            }
        }
        let mut changed = path.clone();
        match line {
            Line::Name => changed.name = Some(value.clone()),
            Line::Look => changed.look = Some(value.clone()).filter(|v| !v.is_empty()),
            Line::Vts => plan::set(&mut changed, "vt", &value),
            Line::Setting(key) => plan::set(&mut changed, key, &value),
            Line::Part(_) => {}
        }
        card.open = Open::Nothing;
        card.pick = chosen(model, &changed, line).unwrap_or(0);
        if card.editor() {
            self.state.new = changed;
            self.level = Level::Card(Box::new(card));
            return vec![Command::Change(Change::Quiet)];
        }
        let mut doc = model.full.clone();
        if let Some(slot) = doc
            .paths
            .iter_mut()
            .find(|p| p.name.as_deref() == Some(own.as_str()))
        {
            *slot = changed.clone();
        }
        let shown_value = shown(model, &changed, line);
        let name = changed.name.clone().unwrap_or_else(|| own.clone());
        let line_label = label(model, line);
        let edits = plan::plan(&model.flow, &doc);
        match super::changed(
            shared,
            words::setting(&line_label, &shown_value, &own),
            edits,
            words::card_set(&line_label, &shown_value, &name),
        ) {
            Ok(out) => {
                if *line == Line::Name {
                    card.obj = Obj::Doc(name.clone(), Slot::Terminal);
                    self.state.at = Some(card.obj.clone());
                }
                self.level = Level::Card(Box::new(card));
                out
            }
            Err(refused) => {
                self.level = Level::Card(Box::new(card));
                refused
            }
        }
    }
}

/// `t` on a card: tries the card's look on the live console, or says why
/// the live trial cannot run here.
fn try_look(shared: &Shared, path: &Path) -> Vec<Command> {
    match crate::live_refusal(shared.console.live) {
        Some(refused) => refused,
        None => vec![Command::Change(Change::TryLook(
            path.look.clone().unwrap_or_else(|| "default".to_owned()),
        ))],
    }
}

/// The screen position of the cell `x`, `y`.
fn position(x: usize, y: usize) -> Position {
    Position::new(
        u16::try_from(x).unwrap_or(u16::MAX),
        u16::try_from(y).unwrap_or(u16::MAX),
    )
}

/// Draws the card `card` into `area`: its heading, its lines, what the
/// highlighted line means, how the rest of the path changes, and what the
/// terminal draws; with the list box under its line, while it is open.
pub fn draw(
    shared: &Shared,
    model: &Model,
    new: &Path,
    card: &Card,
    buf: &mut Buffer,
    area: Rect,
    color: bool,
) {
    let Some(path) = path_of(card, new, model) else {
        return;
    };
    let editor = card.editor();
    let all = lines(&path, editor);
    let width = usize::from(area.width);
    let rows = usize::from(area.height);
    let name = path.name.clone().unwrap_or_default();
    let mut s = Sheet::new(buf, area, color);
    s.crumb(if editor {
        crate::words::NEW_PATH
    } else {
        &name
    });
    let vts = crate::words::vt_words(&path.vts());
    if editor {
        s.put_bold(
            1,
            2,
            &format!("{} · {vts}", words::NEW_TERMINAL),
            Role::Heading,
        );
    } else {
        let x = s.put_bold(1, 2, words::TERMINAL, Role::Heading);
        let x = s.put_bold(x.saturating_add(1), 2, &name, Role::Terminal);
        s.put(x, 2, &format!(" · {vts}"), Role::Heading);
    }
    let (first, count) = window(card, all.len(), rows);
    let value_room = width.saturating_sub(VALUE_X).saturating_sub(1);
    for (i, line) in all.iter().enumerate().skip(first).take(count) {
        let y = TOP.saturating_add(i.saturating_sub(first));
        let here = i == card.line;
        if here {
            s.highlight(y);
        }
        let row = if here { Role::RowHighlight } else { Role::Row };
        let dim = if here { Role::RowHighlight } else { Role::Dim };
        s.put(
            3,
            y,
            &fit(&label(model, line), VALUE_X.saturating_sub(4)),
            dim,
        );
        match (&card.open, here) {
            (Open::Typing(t), true) => {
                s.put(VALUE_X, y, &entry(t, value_room), row);
            }
            (_, true) if inline(line) => {
                let c = choices(model, &path, line);
                let mut x = VALUE_X;
                for (k, w) in c.words.iter().enumerate() {
                    let mark = if c.current == Some(k) { "●" } else { "○" };
                    let style = row.style(color);
                    let style = if k == card.pick { marked(style) } else { style };
                    let text = format!("{mark} {w}");
                    if x.saturating_add(cells(&text)) >= width {
                        break;
                    }
                    x = s.put_style(x, y, &text, style).saturating_add(3);
                }
            }
            (_, true) if *line == Line::Name => {
                let x = s.put(
                    VALUE_X,
                    y,
                    &fit(&shown(model, &path, line), value_room),
                    row,
                );
                s.put(
                    x.saturating_add(2),
                    y,
                    &fit(words::NAME_HINT, width.saturating_sub(x).saturating_sub(3)),
                    row,
                );
            }
            _ => {
                s.put(
                    VALUE_X,
                    y,
                    &fit(&shown(model, &path, line), value_room),
                    row,
                );
            }
        }
    }
    let mut y = TOP.saturating_add(count).saturating_add(1);
    if let Some(line) = all.get(card.line) {
        for a in about(model, &path, line, editor) {
            s.put(3, y, &fit(&a, width.saturating_sub(4)), Role::Dim);
            y = y.saturating_add(1);
        }
    }
    let note = if editor {
        words::NEW_PATH_NOTE
    } else {
        crate::words::REVISE_NOTE
    };
    s.put(3, y, &fit(note, width.saturating_sub(4)), Role::Waiting);
    if !path.active && !path.none {
        subflow(&mut s, shared, model, &path, y.saturating_add(2), width);
    }
    if let (Open::List(list), Some(line)) = (&card.open, all.get(card.line)) {
        let c = choices(model, &path, line);
        let rect = list_rect(card, list, &c, all.len(), area);
        list.draw(&c, buf, rect, color);
    }
}

/// The terminal's subflow under its card, from row `y`: its look's font
/// set, then the look, then its displays, left to right, and a line under
/// them that names the vtlook the look draws, as the Effects tab found
/// it.
fn subflow(s: &mut Sheet<'_>, shared: &Shared, model: &Model, path: &Path, y: usize, width: usize) {
    let flow = &model.flow;
    s.put_bold(1, y, words::SUBFLOW_TITLE, Role::Heading);
    let section = flow
        .looks
        .iter()
        .find(|l| l.name == path.look.as_deref().unwrap_or("default"));
    let font = section
        .map(|l| l.frit.clone())
        .filter(|f| !f.is_empty())
        .map_or_else(
            || words::FONT_SET_FROM_CONF.to_owned(),
            |f| words::font_set_box(&f),
        );
    let look = match (&path.look, section) {
        (None, Some(_)) => words::DEFAULT_LOOK.to_owned(),
        (None, None) => words::DEFAULT_LOOK_BOX.to_owned(),
        (Some(l), Some(_)) => words::look_box(l),
        (Some(l), None) => words::look_box(&words::missing_look(l)),
    };
    let displays: Vec<String> = path
        .displays
        .iter()
        .map(|d| paths::title(flow, Kind::Display, d).0)
        .collect();
    let displays = if displays.is_empty() {
        words::NO_DISPLAY_BOX.to_owned()
    } else {
        displays.join(", ")
    };
    let row = y.saturating_add(1);
    let mut x = 3usize;
    for (i, (text, role)) in [
        (font, Role::FontSet),
        (look, Role::Row),
        (displays, Role::Display),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            x = s.put(x, row, "  \u{2192}  ", Role::Dim);
        }
        x = s.put(
            x,
            row,
            &fit(&text, width.saturating_sub(x).saturating_sub(1)),
            role,
        );
    }
    let vtlook = section
        .and_then(|l| l.vtlook.clone())
        .or_else(|| {
            flow.base
                .set
                .iter()
                .rev()
                .find(|(k, _)| k == "vtlook")
                .map(|(_, v)| v.clone())
        })
        .filter(|v| v != vtium_settings::vtlook::DEFAULT);
    let effects = match vtlook {
        None => words::NO_VTLOOK.to_owned(),
        Some(name) => {
            let read = shared.found.effects.vtlook(&name).map(|v| v.reads.is_ok());
            words::vtlook_line(&name, read)
        }
    };
    s.put(
        3,
        y.saturating_add(2),
        &fit(&effects, width.saturating_sub(4)),
        Role::Dim,
    );
}

/// The full text behind the card's highlighted line, for F1: its label,
/// its value, and what it means.
pub fn full_text(model: &Model, new: &Path, card: &Card) -> Option<String> {
    let path = path_of(card, new, model)?;
    let line = lines(&path, card.editor()).into_iter().nth(card.line)?;
    let mut text = format!("{}: {}.", label(model, &line), shown(model, &path, &line));
    for a in about(model, &path, &line, card.editor()) {
        text.push(' ');
        text.push_str(&a);
    }
    Some(text)
}
