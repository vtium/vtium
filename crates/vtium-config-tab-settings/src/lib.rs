#![forbid(unsafe_code)]
// The tab reads keys, clicks and the configuration's text, and nothing it
// is given can panic the TUI.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-settings: the Settings tab of vtium-config's TUI.
//!
//! The tab shows vtium.conf's top-level keys in the sections of
//! vtium-settings' key table. The sections sit at the left and the
//! highlighted section's keys at the right, each key with its value and
//! whether it is changed or the default, so moving between sections shows
//! their keys without opening them. Enter moves into the keys, and Enter
//! on a key opens its allowed values in the list box, or lets the person
//! type its value. The highlighted key's first sentence sits along the
//! viewer's foot, and F1 opens its whole help. `?` filters the keys by
//! their names and descriptions, so a setting is found by what it does.
//!
//! The tab draws only into the viewer the frame gives it and reads the
//! shared state only. A change to a value is one command, which the
//! shell's one change function makes and records as one step that Undo
//! takes back. [`sections`] holds the sections and the left pane,
//! [`keys`] the right pane and what the tab says about a key, [`value`]
//! the list box's values, typing and the change, and [`layout`] where
//! each part goes.
//!
//! References:
//! - ADR-1206 D9: each tab is a crate that implements the `Tab` trait
//!   over the shared state, the action and the commands.
//! - ADR-1206 D2, D4: a tab's update returns commands, and its view
//!   draws from the shared state without changing it.
//! - ADR-1403 D11, "The tabs' layouts", the Settings bullet: the two
//!   panes, the key's description along the foot, and F1's whole help.
//! - ADR-1403 D11, "Search, on ?": Settings matches a key and its
//!   description.

pub mod keys;
pub mod layout;
pub mod sections;
pub mod value;
pub mod words;

mod act;

use std::borrow::Cow;
use std::cell::Cell;

use ratatui_core::layout::{Position, Rect};
use ratatui_core::terminal::Frame;
use ratatui_core::widgets::Widget;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::Borders;
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_widgets::canvas;
use vtium_config_widgets::list_box::{self, ListBox};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{FindItem, Searchable, Tab};
use vtium_settings::keys::{Key, Kind};

use keys::Line;
use layout::Panes;
use sections::{Mark, Shown, Table};
use value::Values;

/// The node of the list of sections.
const NODE_SECTIONS: &str = "settings";
/// The node of a section's keys.
const NODE_KEYS: &str = "settings.section";
/// The node of a value being typed.
const NODE_TYPING: &str = "settings.section.typing";
/// The node of the list box of a key's values.
const NODE_LIST: &str = "settings.section.list";
/// The node of the list box's search row while the person types in it.
const NODE_SEARCH: &str = "any.search";

/// Which pane has the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Sections,
    Keys,
}

/// What is open over the panes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// Nothing: the person moves through the sections or the keys.
    Nothing,
    /// The highlighted key's values, in the list box under its line.
    Values { list: Box<ListBox>, values: Values },
    /// The highlighted key's value, being typed.
    Typing(String),
}

/// The Settings tab: the sections and their keys, the highlighted section
/// and key, what is open over them, and the shell's `?` filter over the
/// keys.
#[derive(Debug)]
pub struct Settings {
    color: bool,
    table: Table,
    focus: Focus,
    section: usize,
    /// The highlighted key, by its number in the flat list, while the keys
    /// have the focus.
    key: usize,
    left_offset: usize,
    right_offset: usize,
    open: Open,
    /// The copy of the shell's search over every key, while its row is
    /// open or its filter kept. The tab moves the copy's highlight among
    /// the matches while the filter is kept.
    filter: Option<Search>,
    /// The viewer the tab was last drawn in, which a click and a page's
    /// step are read against, so they act on what the person saw.
    seen: Cell<Rect>,
}

/// Where each part of the tab is drawn, for clicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// Each section's row, and the section's number.
    pub sections: Vec<(Rect, usize)>,
    /// Each key's row, and the key's number in the flat list.
    pub keys: Vec<(Rect, usize)>,
    /// The list box's parts, while it is open.
    pub list: Option<list_box::Places>,
}

impl Settings {
    /// The tab on the list of sections, with the first highlighted.
    /// `color` is whether the TUI draws color
    /// ([`vtium_config_widgets::color::enabled`]).
    pub fn new(color: bool) -> Settings {
        Settings {
            color,
            table: Table::new(),
            focus: Focus::Sections,
            section: 0,
            key: 0,
            left_offset: 0,
            right_offset: 0,
            open: Open::Nothing,
            filter: None,
            seen: Cell::new(Rect::default()),
        }
    }

    /// The sections and their keys.
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// The highlighted section's number.
    pub fn section(&self) -> usize {
        self.section
    }

    /// The highlighted key, while the keys have the focus.
    pub fn key(&self) -> Option<&'static Key> {
        self.lit_key()
            .and_then(|at| self.table.entry(at))
            .map(|e| e.key)
    }

    /// The sections the left pane shows, each with its count: every
    /// section, or while the search filters, those that keep a key, each
    /// with how many it keeps.
    fn shown(&self) -> Vec<Shown> {
        self.table
            .sections
            .iter()
            .enumerate()
            .map(|(section, s)| Shown {
                section,
                count: match &self.filter {
                    Some(f) => s.keys.clone().filter(|&k| f.is_match(k)).count(),
                    None => s.keys.len(),
                },
            })
            .filter(|s| self.filter.is_none() || s.count > 0)
            .collect()
    }

    /// The rows of the right pane for the highlighted section.
    fn lines(&self) -> Vec<Line> {
        keys::lines(&self.table, self.section, self.filter.as_ref())
    }

    /// The highlighted key while the keys have the focus, and while the
    /// filter, where there is one, matches something.
    fn lit_key(&self) -> Option<usize> {
        let matched = self
            .filter
            .as_ref()
            .is_none_or(|f| f.highlighted().is_some());
        (self.focus == Focus::Keys && matched).then_some(self.key)
    }

    /// The right pane as it shows on `shared`.
    fn right_pane<'a>(&'a self, shared: &'a Shared, lines: &'a [Line]) -> keys::Pane<'a> {
        let rows = keys::rows(Panes::of(self.seen.get()).right);
        keys::Pane {
            table: &self.table,
            document: shared.document(),
            section: self.section,
            lines,
            offset: keys::settled(lines, self.lit_key(), self.right_offset, rows),
            highlight: self.lit_key(),
            typing: match &self.open {
                Open::Typing(text) => Some(text.as_str()),
                _ => None,
            },
            matching: self.filter.as_ref().map(|f| {
                self.table
                    .section(self.section)
                    .map_or(0, |s| s.keys.clone().filter(|&k| f.is_match(k)).count())
            }),
        }
    }

    /// The first section the left pane shows, so the highlighted one does.
    fn left_settled(&self, shown: &[Shown]) -> usize {
        let rows = sections::rows(Panes::of(self.seen.get()).left).max(1);
        let at = shown
            .iter()
            .position(|s| s.section == self.section)
            .unwrap_or(0);
        let mut offset = self.left_offset;
        if at < offset {
            offset = at;
        } else if at >= offset.saturating_add(rows) {
            offset = at.saturating_add(1).saturating_sub(rows);
        }
        offset.min(shown.len().saturating_sub(rows))
    }

    /// Takes the offsets the view would draw from as the tab's own, after
    /// the highlight moved.
    fn settle(&mut self) {
        let shown = self.shown();
        self.left_offset = self.left_settled(&shown);
        let lines = self.lines();
        let rows = keys::rows(Panes::of(self.seen.get()).right);
        self.right_offset = keys::settled(&lines, self.lit_key(), self.right_offset, rows);
    }

    /// The list box's place under the highlighted key's line, in the
    /// viewer last drawn.
    fn list_rect(&self, shared: &Shared, values: &Values) -> Rect {
        let panes = Panes::of(self.seen.get());
        let lines = self.lines();
        let pane = self.right_pane(shared, &lines);
        let line = pane
            .places(panes.right)
            .into_iter()
            .find(|(_, k)| *k == self.key)
            .map_or(panes.right.y, |(r, _)| r.y);
        let columns = pane.columns(panes.right);
        let x = panes
            .right
            .x
            .saturating_add(u16::try_from(columns.name).unwrap_or(0));
        match &self.open {
            Open::Values { list, .. } => list.place(values, panes.right, line, x),
            _ => Rect::default(),
        }
    }

    /// Where each part of the tab is drawn on `shared`, in the viewer last
    /// drawn.
    pub fn places(&self, shared: &Shared) -> Places {
        let panes = Panes::of(self.seen.get());
        let shown = self.shown();
        let offset = self.left_settled(&shown);
        let sections = shown
            .iter()
            .skip(offset)
            .take(sections::rows(panes.left))
            .enumerate()
            .filter_map(|(i, s)| {
                let dy = u16::try_from(i).ok()?;
                let y = panes
                    .left
                    .y
                    .saturating_add(sections::FIRST_ROW)
                    .saturating_add(dy);
                Some((Rect::new(panes.left.x, y, panes.left.width, 1), s.section))
            })
            .collect();
        let lines = self.lines();
        let keys = self.right_pane(shared, &lines).places(panes.right);
        let list = match &self.open {
            Open::Values { list, values } => {
                Some(list.places(values, self.list_rect(shared, values)))
            }
            _ => None,
        };
        Places {
            sections,
            keys,
            list,
        }
    }

    /// The foot's line: what is typed, the highlighted key's first
    /// sentence, or the highlighted section's description.
    fn foot(&self) -> String {
        let cells = self.foot_cells();
        match (&self.open, self.focus) {
            (Open::Typing(_), _) => match self.table.entry(self.key).map(|e| e.key.kind) {
                Some(Kind::KeyBinding) => words::TYPING_BINDING.to_owned(),
                _ => words::TYPING.to_owned(),
            },
            (_, Focus::Keys) => self.lit_key().map_or(String::new(), |at| {
                keys::foot(&self.table, at, self.filter.as_ref(), cells)
            }),
            (_, Focus::Sections) => self
                .table
                .section(self.section)
                .map_or(String::new(), |s| s.about.to_owned()),
        }
    }

    /// The cells the foot's line has, in the viewer last drawn.
    fn foot_cells(&self) -> usize {
        usize::from(Panes::of(self.seen.get()).foot.width).saturating_sub(3)
    }
}

impl Tab<Shared, Action, Command> for Settings {
    fn name(&self) -> &'static str {
        words::SETTINGS
    }

    fn letter(&self) -> char {
        's'
    }

    /// The list of sections, a section's keys, a value being typed, a
    /// key's values in the list box, or the search row while the person
    /// types in the list box's search. The shell answers the node of its
    /// own `?` search row, and a kept filter leaves the node of the keys
    /// it filters, whose keys act on what matches.
    fn node(&self) -> &'static str {
        match &self.open {
            Open::Typing(_) => NODE_TYPING,
            Open::Values { list, .. } if list.filter().is_some_and(Search::typing) => NODE_SEARCH,
            Open::Values { .. } => NODE_LIST,
            Open::Nothing => match self.focus {
                Focus::Keys => NODE_KEYS,
                Focus::Sections => NODE_SECTIONS,
            },
        }
    }

    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.seen.set(area);
        let panes = Panes::of(area);
        let color = self.color;
        let buf = frame.buffer_mut();
        let shown = self.shown();
        let entry = self.filter.as_ref().map(|f| words::matching(f.entry()));
        let mark = match self.focus {
            Focus::Sections => Mark::Focus,
            Focus::Keys => Mark::Open,
        };
        sections::Pane {
            table: &self.table,
            shown: &shown,
            highlight: (self.filter.is_none() || shown.iter().any(|s| s.section == self.section))
                .then_some((self.section, mark)),
            offset: self.left_settled(&shown),
            title: entry.as_deref(),
        }
        .draw(buf, panes.left, color);
        Block::new()
            .borders(Borders::LEFT)
            .border_style(Role::Row.style(color).cell())
            .render(panes.rule, buf);
        let lines = self.lines();
        let cursor = self
            .right_pane(shared, &lines)
            .draw(buf, panes.right, color);
        let foot = canvas::fit(&self.foot(), self.foot_cells());
        sections::put(
            buf,
            panes.foot.x.saturating_add(2),
            panes.foot.y,
            &foot,
            Role::Dim.style(color).cell(),
            panes.foot,
        );
        if let Open::Values { list, values } = &self.open {
            let rect = self.list_rect(shared, values);
            list.draw(values, buf, rect, color);
        }
        if let Some(at) = cursor {
            frame.set_cursor_position(at);
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        act::update(self, shared, action)
    }

    /// F1's text: on a key, its whole help, its default and its value now;
    /// on a section, its description where the foot cut it. Nothing while
    /// a value is typed, where F1 does nothing.
    fn full_text(&self, shared: &Shared) -> Option<String> {
        match (&self.open, self.focus) {
            (Open::Typing(_), _) => None,
            (_, Focus::Keys) => keys::full_help(&self.table, self.lit_key()?, shared.document()),
            (_, Focus::Sections) => {
                let s = self.table.section(self.section)?;
                (canvas::cells(s.about) > self.foot_cells())
                    .then(|| format!("{}\n\n{}", words::place(&[s.title]), s.about))
            }
        }
    }

    /// Every key, for Find: its name and description, its section, and
    /// its value now.
    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        (0..self.table.entries.len())
            .filter_map(|at| keys::find_item(&self.table, at, shared.document()))
            .collect()
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    /// The search of the list box of a key's values, while the box is
    /// open and its search is open or kept.
    fn search(&self) -> Option<&Search> {
        match &self.open {
            Open::Values { list, .. } => list.filter(),
            _ => None,
        }
    }

    /// Keeps the copy of the shell's search, and the highlight follows
    /// its highlighted key into the key's section. `None` clears the
    /// filter, and every section and key shows again.
    fn filtered(&mut self, _shared: &Shared, search: Option<&Search>) {
        self.filter = search.cloned();
        act::follow(self);
        self.settle();
    }
}

/// The `?` search over every key the tab shows, by its name and its
/// description.
impl Searchable for Settings {
    fn search_node(&self) -> &'static str {
        match self.focus {
            Focus::Keys => NODE_KEYS,
            Focus::Sections => NODE_SECTIONS,
        }
    }

    fn count(&self) -> usize {
        self.table.entries.len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let key = self.table.entry(item)?.key;
        match field {
            0 => Some(Cow::Borrowed(key.name)),
            1 => Some(Cow::Owned(key.what_line())),
            _ => None,
        }
    }

    /// The highlighted key, or on the list of sections the highlighted
    /// section's first key.
    fn highlighted(&self) -> Option<usize> {
        match self.focus {
            Focus::Keys => Some(self.key),
            Focus::Sections => self.table.section(self.section).map(|s| s.keys.start),
        }
    }
}

/// The cell `at` as a position.
fn position(col: usize, row: usize) -> Position {
    Position::new(
        u16::try_from(col).unwrap_or(u16::MAX),
        u16::try_from(row).unwrap_or(u16::MAX),
    )
}
