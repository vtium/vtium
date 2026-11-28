#![forbid(unsafe_code)]
// The tab reads keys, clicks and the administrator's files' names and
// errors, and nothing it is given can panic the TUI.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-effects: the Effects tab of vtium-config's TUI.
//!
//! The tab lists the effects a look can use, in three groups of one list:
//! the vtlooks, No filter first, then the presets, then the
//! administrator's; the color maps; and the shaders. Beside the list sits
//! the highlighted vtlook's filter chain, left to right in the order vtium
//! runs it, with a line for each filter's settings under it and the
//! picture of the sample screen through it after them. On a color map or
//! a shader the same place shows the facts its detail lists. A filter
//! vtium does not build yet is marked planned, in words.
//!
//! The filter line above the list opens the list of groups in the list
//! box, Enter on an item opens its detail, which changes nothing, and
//! Space activates or deactivates a color map or a shader, asking first
//! when a look uses it. The shell's `?` search filters the list by the
//! items' names: the tab hands it the names through `Searchable`, and
//! keeps the copy of the search the shell hands it after each key.
//!
//! The tab draws only into the viewer the frame gives it and reads the
//! shared state only. An activation is one command, which the shell's one
//! change function makes and records as one step that Undo takes back.
//! [`list`] holds the list and what the tab knows of each item, [`chain`]
//! what sits beside the list, [`detail`] the detail, and [`words`] what
//! the tab says.
//!
//! References:
//! - ADR-1206 D9: each tab is a crate that implements the `Tab` trait
//!   over the shared state, the action and the commands.
//! - ADR-1206 D2, D4: a tab's update returns commands, and its view
//!   draws from the shared state without changing it.
//! - ADR-1403 D11, "The tabs' layouts", the Effects bullet: the chain
//!   beside the list, left to right.
//! - ADR-1403 D11, "Search, on ?": the Effects list matches its items'
//!   names.
//! - ADR-1304 D13: the slices the planned filters wait for.

pub mod chain;
pub mod detail;
pub mod list;
pub mod words;

mod act;

use std::borrow::Cow;
use std::cell::{Cell, RefCell};

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use ratatui_core::widgets::Widget;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::Borders;
use vtium_config_core::found::effects::Group;
use vtium_config_core::preview::{Applied, Place, Previewed, Shows};
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_widgets::list_box::{self, ListBox};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{FindItem, Searchable, Tab};

use list::{Groups, Item, Line};

/// The node of the list.
const NODE_LIST: &str = "effects";
/// The node of the list of groups, in the list box under the filter line.
const NODE_GROUPS: &str = "effects.filter";
/// The node of an item's detail.
const NODE_DETAIL: &str = "effects.effect";
/// The node of the list box's search row while the person types in it.
const NODE_SEARCH: &str = "any.search";

/// The left pane's width: the list's columns, before the rule.
pub const LIST_CELLS: u16 = 32;

/// The most rows the picture's frame takes, its borders included.
pub const PICTURE_ROWS: u16 = 12;

/// The fewest rows the picture's frame is drawn in.
const PICTURE_MIN: u16 = 4;

/// What is open over the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// Nothing: the person moves through the list.
    Nothing,
    /// The list of groups, in the list box under the filter line.
    Groups(Box<ListBox>),
    /// An item's detail, with the first line under its title in sight.
    Detail { item: Item, top: usize },
}

/// The Effects tab: the groups shown, the highlighted item, the first line
/// in sight, what is open over the list, and the shell's `?` filter over
/// it.
#[derive(Debug)]
pub struct Effects {
    color: bool,
    group: Group,
    /// The highlighted item, by its place among the items of the groups
    /// shown, or `None` on the filter line.
    highlight: Option<usize>,
    offset: usize,
    open: Open,
    /// The copy of the shell's `?` search over the items of the groups
    /// shown that the shell handed the tab last, while its row is open or
    /// its filter kept.
    filter: Option<Search>,
    /// The names of the items of the groups shown, as the search matches
    /// them, taken from the shared state each time the tab is drawn or
    /// handed an action.
    names: RefCell<Vec<String>>,
    /// The viewer the tab was last drawn in, which a click and a page's
    /// step are read against, so they act on what the person saw.
    seen: Cell<Rect>,
}

/// Where each part of the tab goes in the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panes {
    /// The list, the filter line on its first row.
    pub list: Rect,
    /// The rule between the list and the pane beside it.
    pub rule: Rect,
    /// The chain or the facts beside the list.
    pub side: Rect,
}

impl Panes {
    /// The panes of `area`: the list at the left, the rule, then a blank
    /// and the side.
    pub fn of(area: Rect) -> Panes {
        let left = LIST_CELLS.min(area.width / 2);
        let list = Rect::new(area.x, area.y, left, area.height);
        let rule = Rect::new(area.x.saturating_add(left), area.y, 1, area.height);
        let side_x = rule.x.saturating_add(2);
        let side = Rect::new(
            side_x,
            area.y,
            area.right().saturating_sub(side_x).saturating_sub(1),
            area.height,
        );
        Panes { list, rule, side }
    }
}

/// Where each part of the tab is drawn, for clicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The filter line.
    pub filter: Rect,
    /// Each item's row, and the item's place among the items shown.
    pub items: Vec<(Rect, usize)>,
    /// The list box's parts, while the list of groups is open.
    pub groups: Option<list_box::Places>,
}

impl Effects {
    /// The tab on the filter line, showing every group. `color` is whether
    /// the TUI draws color ([`vtium_config_widgets::color::enabled`]).
    pub fn new(color: bool) -> Effects {
        Effects {
            color,
            group: Group::All,
            highlight: None,
            offset: 0,
            open: Open::Nothing,
            filter: None,
            names: RefCell::new(Vec::new()),
            seen: Cell::new(Rect::default()),
        }
    }

    /// The groups the list shows.
    pub fn group(&self) -> Group {
        self.group
    }

    /// The highlighted item, `None` on the filter line or while a search
    /// matches nothing.
    pub fn highlighted(&self, shared: &Shared) -> Option<Item> {
        let at = self.lit()?;
        self.items(shared).get(at).copied()
    }

    /// The items of the groups shown, before the filter.
    fn items(&self, shared: &Shared) -> Vec<Item> {
        list::items(&shared.found.effects, self.group)
    }

    /// Takes the names the search matches from `shared`.
    fn refresh(&self, shared: &Shared) {
        let effects = &shared.found.effects;
        *self.names.borrow_mut() = self
            .items(shared)
            .into_iter()
            .map(|i| list::name(effects, i))
            .collect();
    }

    /// The highlighted item's place, while the filter, where there is
    /// one, matches it.
    fn lit(&self) -> Option<usize> {
        let at = self.highlight?;
        match &self.filter {
            Some(f) => f.is_match(at).then_some(at),
            None => Some(at),
        }
    }

    /// The list's lines, with the filter's matches only while it filters.
    fn lines(&self, shared: &Shared) -> Vec<Line> {
        let items = self.items(shared);
        let keep = |at: usize| self.filter.as_ref().is_none_or(|f| f.is_match(at));
        list::lines(
            &items,
            &keep,
            self.filter.is_some(),
            self.group,
            &shared.found.effects,
        )
    }

    /// The offset the view draws the list from, after a move.
    fn settled(&self, shared: &Shared) -> usize {
        let lines = self.lines(shared);
        let rows = list::rows(Panes::of(self.seen.get()).list);
        list::settled(&lines, self.highlight, self.offset, rows)
    }

    /// Takes the offset the view would draw from as the tab's own.
    fn settle(&mut self, shared: &Shared) {
        self.offset = self.settled(shared);
    }

    /// The side pane's lines for the highlighted item.
    fn side(&self, shared: &Shared, width: usize) -> Vec<chain::Spans> {
        chain::lines(shared, self.highlighted(shared), width)
    }

    /// The picture's frame in the side pane, under its lines, where the
    /// highlighted item has a picture and the pane has the rows for it: a
    /// vtlook, a color map, or the filter line's plain sample screen.
    fn picture_frame(&self, shared: &Shared) -> Option<Rect> {
        if self.open != Open::Nothing {
            return None;
        }
        let side = Panes::of(self.seen.get()).side;
        let item = self.highlighted(shared);
        if matches!(item, Some(Item::Shader(_))) {
            return None;
        }
        if item.is_none() && self.highlight.is_some() {
            // a search that matches nothing previews nothing
            return None;
        }
        let used =
            u16::try_from(self.side(shared, usize::from(side.width)).len()).unwrap_or(u16::MAX);
        let top = if used == 0 { 0 } else { used.saturating_add(1) };
        let rows = side.height.saturating_sub(top).min(PICTURE_ROWS);
        (rows >= PICTURE_MIN)
            .then(|| Rect::new(side.x, side.y.saturating_add(top), side.width, rows))
    }

    /// The list box's place under the filter line, in the viewer last
    /// drawn.
    fn groups_rect(&self, list: &ListBox) -> Rect {
        let pane = Panes::of(self.seen.get()).list;
        list.place(&self.choices(), pane, pane.y, pane.x.saturating_add(1))
    }

    /// The choices of the list of groups.
    fn choices(&self) -> Groups {
        Groups { now: self.group }
    }

    /// Where each part of the tab is drawn on `shared`, in the viewer last
    /// drawn.
    pub fn places(&self, shared: &Shared) -> Places {
        let pane = Panes::of(self.seen.get()).list;
        let lines = self.lines(shared);
        let offset = self.settled(shared);
        let items = lines
            .iter()
            .skip(offset)
            .take(list::rows(pane))
            .enumerate()
            .filter_map(|(r, l)| match l {
                Line::Item(at) => {
                    let y = pane
                        .y
                        .saturating_add(list::FIRST_ROW)
                        .saturating_add(u16::try_from(r).ok()?);
                    Some((Rect::new(pane.x, y, pane.width, 1), *at))
                }
                Line::Heading(..) => None,
            })
            .collect();
        let groups = match &self.open {
            Open::Groups(list) => Some(list.places(&self.choices(), self.groups_rect(list))),
            _ => None,
        };
        Places {
            filter: Rect::new(pane.x, pane.y, pane.width, 1),
            items,
            groups,
        }
    }

    /// The highlighted item's facts whole, for F1: the side pane's lines,
    /// none cut.
    fn facts(&self, shared: &Shared) -> Option<String> {
        let item = self.highlighted(shared)?;
        let lines = chain::lines(shared, Some(item), usize::MAX);
        Some(chain::plain(&lines))
    }
}

/// The font the preview draws the sample screen in: the font of the
/// look of the console's terminal `head`, as `VTIUM_HEAD` names it, where
/// that look names one, else the document's top-level font-name, else the
/// font the console shows, else the first font built into vtium.
fn font(shared: &Shared, head: Option<&str>) -> Option<String> {
    let text = shared.document().text();
    if let Some(head) = head {
        let flow = vtium_config_core::flowmodel::build(text, &shared.present, None);
        let look = flow
            .heads
            .iter()
            .position(|h| h.name == head)
            .and_then(|h| flow.look_of(h))
            .and_then(|l| flow.looks.get(l))
            .filter(|l| !l.frit.is_empty());
        if let Some(look) = look {
            return Some(look.frit.clone());
        }
    }
    vtium_settings::setting(text, "font-name")
        .or_else(|| shared.console.shown_font.clone())
        .or_else(|| shared.found.fonts.built_in.first().map(|f| (*f).to_owned()))
}

impl Tab<Shared, Action, Command> for Effects {
    fn name(&self) -> &'static str {
        words::EFFECTS
    }

    fn letter(&self) -> char {
        'e'
    }

    /// The list, the list of groups, an item's detail, or the search row
    /// while the person types in the list box's search. The shell answers
    /// the node of its own `?` search while that is open.
    fn node(&self) -> &'static str {
        match &self.open {
            Open::Detail { .. } => NODE_DETAIL,
            Open::Groups(list) if list.filter().is_some_and(Search::typing) => NODE_SEARCH,
            Open::Groups(_) => NODE_GROUPS,
            Open::Nothing => NODE_LIST,
        }
    }

    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.seen.set(area);
        self.refresh(shared);
        let color = self.color;
        let buf = frame.buffer_mut();
        if let Open::Detail { item, top } = &self.open {
            let lines = detail::lines(shared, *item, usize::from(area.width).saturating_sub(1));
            let top = (*top).min(detail::last_top(lines.len(), area));
            detail::draw(&lines, top, buf, area, color);
            return;
        }
        let panes = Panes::of(area);
        let items = self.items(shared);
        // the list box of groups stands in the list's place while it is open
        let lines = match &self.open {
            Open::Groups(_) => Vec::new(),
            _ => self.lines(shared),
        };
        list::Pane {
            shared,
            group: self.group,
            shown: &items,
            lines: &lines,
            offset: self.settled(shared),
            highlight: self.highlight,
            lit: self.filter.is_none() || self.lit().is_some() || self.highlight.is_none(),
            matching: self.filter.as_ref().map(Search::entry),
        }
        .draw(buf, panes.list, color);
        Block::new()
            .borders(Borders::LEFT)
            .border_style(Role::Row.style(color).cell())
            .render(panes.rule, buf);
        let side = self.side(shared, usize::from(panes.side.width));
        for (y, line) in side.iter().enumerate() {
            detail::put_spans(
                buf,
                panes.side,
                u16::try_from(y).unwrap_or(u16::MAX),
                0,
                line,
                color,
            );
        }
        if let Some(rect) = self.picture_frame(shared) {
            let title = match self.highlighted(shared) {
                Some(item) => words::on_sample(&list::name(&shared.found.effects, item)),
                None => words::SAMPLE_PLAIN.to_owned(),
            };
            Block::bordered()
                .title(format!(" {title} "))
                .border_style(Role::Row.style(color).cell())
                .render(rect, buf);
        }
        if let Open::Groups(list) = &self.open {
            let rect = self.groups_rect(list);
            list.draw(&self.choices(), buf, rect, color);
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        self.refresh(shared);
        let out = act::update(self, shared, action);
        self.settle(shared);
        out
    }

    /// F1's text: the highlighted item's facts whole, where the pane
    /// beside the list cut a line, or where it has a line it shows in part
    /// only, such as an error. Nothing on the filter line or in a detail,
    /// which shows each line whole.
    fn full_text(&self, shared: &Shared) -> Option<String> {
        match &self.open {
            Open::Nothing => self.facts(shared),
            Open::Groups(_) | Open::Detail { .. } => None,
        }
    }

    /// Every vtlook, color map and shader, for Find: its name, what it is,
    /// and its marks, by its place in the list of every group.
    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        let effects = &shared.found.effects;
        list::items(effects, Group::All)
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let description = match item {
                    Item::Vtlook(i) => effects
                        .vtlooks
                        .get(i)
                        .map_or_else(String::new, |v| words::find_vtlook(v.from)),
                    Item::Map(_) => words::FIND_MAP.to_owned(),
                    Item::Shader(_) => words::FIND_SHADER.to_owned(),
                };
                FindItem {
                    name: list::name(effects, item),
                    description: Some(description),
                    place: words::EFFECTS.to_owned(),
                    fact: list::marks(shared, item).0,
                    index,
                }
            })
            .collect()
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    /// The list box's search, while the list of groups is open and the
    /// person searches it.
    fn search(&self) -> Option<&Search> {
        match &self.open {
            Open::Groups(list) => list.filter(),
            _ => None,
        }
    }

    /// Keeps the copy of the shell's search and lists only what it
    /// matches, with the highlight on its highlighted match; `None` lists
    /// every item again and leaves the highlight where it is.
    fn filtered(&mut self, shared: &Shared, search: Option<&Search>) {
        self.filter = search.cloned();
        if let Some(s) = &self.filter {
            self.highlight = s.highlighted().or(self.highlight);
        }
        self.settle(shared);
    }

    /// The highlighted item applied to the sample screen, in the font of
    /// the console's terminal's look where it names one, else the
    /// document's, the console's or the first built-in font, with the place inside
    /// the picture's frame in the viewer last drawn. The place is `None`
    /// where the tab draws no frame: on a shader, on a search that matches
    /// nothing, with the list of groups or a detail open, or where the
    /// pane has too few rows. Nothing is applied on the filter line, on a
    /// search that matches nothing, or with a level open.
    fn preview<'a>(&self, shared: &'a Shared) -> Option<Previewed<'a>> {
        let effects = &shared.found.effects;
        let effect = if self.open == Open::Nothing {
            match self.highlighted(shared) {
                Some(Item::Vtlook(i)) => effects
                    .vtlooks
                    .get(i)
                    .map_or(Applied::Nothing, Applied::Vtlook),
                Some(Item::Map(i)) => effects.maps.get(i).map_or(Applied::Nothing, Applied::Map),
                Some(Item::Shader(i)) => effects
                    .shaders
                    .get(i)
                    .map_or(Applied::Nothing, |s| Applied::Shader(&s.resource)),
                None => Applied::Nothing,
            }
        } else {
            Applied::Nothing
        };
        let place = self.picture_frame(shared).map(|r| {
            Place::new(
                r.x.saturating_add(1),
                r.y.saturating_add(1),
                r.width.saturating_sub(2),
                r.height.saturating_sub(2),
            )
        });
        Some(Previewed {
            font: font(shared, shared.machine.head.as_deref()),
            shows: Shows::Effect(effect),
            in_document: None,
            place,
        })
    }
}

/// The `?` search over the items of the groups shown, by their names.
impl Searchable for Effects {
    fn search_node(&self) -> &'static str {
        NODE_LIST
    }

    /// The highlighted item's place among the items of the groups shown,
    /// `None` on the filter line.
    fn highlighted(&self) -> Option<usize> {
        self.highlight
    }

    fn count(&self) -> usize {
        self.names.borrow().len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        if field != 0 {
            return None;
        }
        self.names.borrow().get(item).cloned().map(Cow::Owned)
    }
}
