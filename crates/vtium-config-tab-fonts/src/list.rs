//! The list at the left: the filter line, the configuration's own row
//! with Activate missing, and the fonts in their groups, each with its
//! kind and its mark. The filter line and the configuration's row stay at
//! the top while the fonts scroll under them.
//!
//! The list shows the fonts that the filter keeps and the `?` search
//! matches. Both read only what the tab already knows ([`crate::states`])
//! and what the font helper said of each file, so a filter or a typed
//! character filters again without reading a folder.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Fonts bullet, and its figure:
//!   the filter line with its count, the groups, and each row's kind and
//!   state.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the list's first item, Activate missing fonts with its count.
//! - ADR-1205 D3: the filters, and that only All fonts keeps a font the
//!   font helper has not described.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use vtium_config_core::fontuse::State;
use vtium_config_core::found::fonts::{Filter, FontFile};
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_font_helper::describe::Group as FontGroup;

use crate::states::{Item, Known};
use crate::words::{self, Group};

/// What the highlight stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    /// The filter line.
    Filter,
    /// The configuration's row, Activate missing fonts.
    Activate,
    /// A font.
    Item(Item),
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Filter,
    Activate,
    Heading(Group),
    /// The font at this position of what the tab knows.
    Item(usize),
    /// A line that says why the list shows no font.
    Note(String),
}

impl Line {
    /// What the highlight stands on on this row, where it can.
    pub fn sel(&self, known: &Known) -> Option<Sel> {
        match self {
            Line::Filter => Some(Sel::Filter),
            Line::Activate => Some(Sel::Activate),
            Line::Item(at) => known.items.get(*at).map(|i| Sel::Item(*i)),
            Line::Heading(_) | Line::Note(_) => None,
        }
    }
}

/// Whether `filter` keeps `item`. A built-in font is a monospaced bitmap
/// font; a font no file is found for is known by its name alone, so only
/// All fonts keeps it.
pub fn keeps(filter: Filter, item: Item, shared: &Shared) -> bool {
    match item {
        Item::File(i) => shared
            .found
            .fonts
            .files()
            .get(i)
            .is_some_and(|f| filter.keeps(f)),
        Item::BuiltIn(_) => matches!(filter, Filter::All | Filter::Bitmap | Filter::Monospaced),
        Item::Named(_) => filter == Filter::All,
    }
}

/// The positions of the fonts the list shows: those `filter` keeps and,
/// where the search is open or kept, those it matches.
pub fn shown(
    known: &Known,
    shared: &Shared,
    filter: Filter,
    search: Option<&Search>,
) -> Vec<usize> {
    (0..known.items.len())
        .filter(|&at| search.is_none_or(|s| s.is_match(at)))
        .filter(|&at| {
            known
                .items
                .get(at)
                .is_some_and(|&i| keeps(filter, i, shared))
        })
        .collect()
}

/// The rows that stay at the top: the filter line, and the
/// configuration's own row while its fonts are not all active.
pub fn header(known: &Known) -> Vec<Line> {
    let mut out = vec![Line::Filter];
    if !known.counts.all_active() {
        out.push(Line::Activate);
    }
    out
}

/// The rows under the header: each group's heading and its fonts, or a
/// line that says why there is none.
pub fn body(known: &Known, shown: &[usize], search: Option<&Search>) -> Vec<Line> {
    if known.items.is_empty() {
        return vec![Line::Note(words::EMPTY.to_owned())];
    }
    if shown.is_empty() {
        return match search.filter(|s| !s.entry().is_empty()) {
            Some(s) => vec![
                Line::Note(words::none_match(s.entry())),
                Line::Note(words::NONE_MATCH_HINT.to_owned()),
            ],
            None => vec![Line::Note(words::NONE_PASS.to_owned())],
        };
    }
    let mut out = Vec::new();
    let mut group = None;
    for &at in shown {
        let g = known.groups.get(at).copied();
        if g != group {
            if let Some(g) = g {
                out.push(Line::Heading(g));
            }
            group = g;
        }
        out.push(Line::Item(at));
    }
    out
}

/// The rows of the body a window of `rows` shows, from `top`, moved so
/// the row of `sel` shows.
pub fn settled(body: &[Line], known: &Known, sel: Sel, top: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    let mut top = top;
    if let Some(at) = body.iter().position(|l| l.sel(known) == Some(sel)) {
        // a group's heading shows with its first font
        let from = match at.checked_sub(1).and_then(|h| body.get(h)) {
            Some(Line::Heading(_)) => at.saturating_sub(1),
            _ => at,
        };
        if from < top {
            top = from;
        } else if at >= top.saturating_add(rows) {
            top = at.saturating_add(1).saturating_sub(rows);
        }
    }
    top.min(body.len().saturating_sub(rows))
}

/// Writes `text` at `x`, `y` in `style`, no further than `right`, and
/// returns the column after it.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, right: u16) -> u16 {
    if x >= right || !buf.area.contains((x, y).into()) {
        return x;
    }
    let room = usize::from(right.saturating_sub(x));
    buf.set_stringn(x, y, canvas::printable(text), room, style)
        .0
}

/// Writes `spans` from `x`, `y`, each in its role's style, or in `over`'s
/// where it is given, as a highlight draws them, and no further than
/// `right`. A row wider than the room left is cut with `…` where it
/// crosses the edge. Returns the column after the last cell written.
pub fn put_spans(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    spans: &[(String, Role)],
    over: Option<Role>,
    color: bool,
    right: u16,
) -> u16 {
    let room = usize::from(right.saturating_sub(x));
    let whole: usize = spans.iter().map(|(t, _)| canvas::cells(t)).sum();
    let mut x = x;
    let mut used = 0usize;
    for (text, role) in spans {
        let left = room.saturating_sub(used);
        if left == 0 {
            break;
        }
        let shown = if whole > room && canvas::cells(text) >= left {
            canvas::fit(text, left)
        } else {
            text.clone()
        };
        used = used.saturating_add(canvas::cells(&shown));
        let style = over.unwrap_or(*role).style(color).cell();
        x = put(buf, x, y, &shown, style, right);
    }
    x
}

/// The list as it shows on one screen.
pub struct ListPane<'a> {
    pub known: &'a Known,
    pub shared: &'a Shared,
    pub filter: Filter,
    pub search: Option<&'a Search>,
    pub sel: Sel,
    /// The first body row the window shows.
    pub top: usize,
    pub color: bool,
}

/// The columns of a font's row in a list `width` cells wide: the blanks
/// before the name, the name's cells, and the kind's cells, none where
/// the list is too narrow for the kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub name: usize,
    pub kind: usize,
}

/// The blanks before a font's name, under its group's heading.
const INDENT: usize = 3;

/// The cells a whole mark with a look's name beside it takes.
const STATE_WIDE: usize = 27;

/// The cells of the state column where the list drops the kind.
const STATE_NARROW: usize = 22;

/// The cells of the kind column, its blank included.
const KIND: usize = 9;

/// The narrowest list that still shows the kind column.
const KIND_FROM: usize = 56;

impl Columns {
    /// The columns of a list `width` cells wide.
    pub fn of(width: usize) -> Columns {
        if width >= KIND_FROM {
            Columns {
                name: width.saturating_sub(INDENT + KIND + STATE_WIDE),
                kind: KIND,
            }
        } else {
            Columns {
                name: width.saturating_sub(INDENT + STATE_NARROW),
                kind: 0,
            }
        }
    }
}

impl ListPane<'_> {
    /// The rows the list shows, header first, from the window's top.
    pub fn rows(&self, rect: Rect) -> Vec<Line> {
        let header = header(self.known);
        let shown = shown(self.known, self.shared, self.filter, self.search);
        let body = body(self.known, &shown, self.search);
        let rows = usize::from(rect.height).saturating_sub(header.len());
        let top = settled(&body, self.known, self.sel, self.top, rows);
        header
            .into_iter()
            .chain(body.into_iter().skip(top).take(rows))
            .collect()
    }

    /// Where each row that the highlight can stand on is drawn in `rect`.
    pub fn places(&self, rect: Rect) -> Vec<(Rect, Sel)> {
        self.rows(rect)
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let y = rect.y.saturating_add(u16::try_from(i).ok()?);
                Some((Rect::new(rect.x, y, rect.width, 1), line.sel(self.known)?))
            })
            .collect()
    }

    /// Draws the list in `rect`.
    pub fn draw(&self, buf: &mut Buffer, rect: Rect) {
        let width = usize::from(rect.width);
        let columns = Columns::of(width);
        for (i, line) in self.rows(rect).iter().enumerate() {
            let Ok(dy) = u16::try_from(i) else { break };
            let y = rect.y.saturating_add(dy);
            let lit = line.sel(self.known) == Some(self.sel);
            let spans = self.spans(line, columns, width);
            let right = rect.right();
            let over = lit.then_some(Role::RowHighlight);
            let mut x = put_spans(buf, (rect.x, y), &spans, over, self.color, right);
            if lit {
                let fill = Role::RowHighlight.style(self.color).cell();
                while x < right {
                    x = put(buf, x, y, " ", fill, right);
                }
            }
        }
    }

    /// The spans of `line` in a list `width` cells wide.
    fn spans(&self, line: &Line, columns: Columns, width: usize) -> Vec<(String, Role)> {
        match line {
            Line::Filter => self.filter_line(width),
            Line::Activate => self.activate_line(),
            Line::Heading(g) => {
                let mut out = vec![(format!(" {}", g.heading()), Role::Heading)];
                if let Some(aside) = g.aside() {
                    out.push((format!("  {aside}"), Role::Dim));
                }
                out
            }
            Line::Item(at) => self.item_line(*at, columns, width),
            Line::Note(text) => vec![(format!(" {text}"), Role::Dim)],
        }
    }

    /// The filter line: which filter shows and what the search matches at
    /// the left, and the count at the right.
    fn filter_line(&self, width: usize) -> Vec<(String, Role)> {
        let shown = shown(self.known, self.shared, self.filter, self.search);
        let fonts = |at: &usize| !matches!(self.known.items.get(*at), Some(Item::Named(_)) | None);
        let n = shown.iter().filter(|a| fonts(a)).count();
        let mut left = words::show(self.filter);
        let right = match self.search {
            Some(s) => {
                left.push_str(&words::matching(s.entry()));
                let all = (0..self.known.items.len()).filter(|a| fonts(a)).count();
                words::count_of(n, all)
            }
            None => {
                let active = shown
                    .iter()
                    .filter(|a| fonts(a) && self.known.active(**a))
                    .count();
                words::count(n, active)
            }
        };
        let room = width.saturating_sub(1);
        let gap = room
            .saturating_sub(canvas::cells(&left))
            .saturating_sub(canvas::cells(&right))
            .max(1);
        let left = canvas::fit(&left, room.saturating_sub(canvas::cells(&right) + 1));
        vec![
            (left, Role::Row),
            (" ".repeat(gap), Role::Row),
            (right, Role::Dim),
        ]
    }

    /// The configuration's own row: its mark, then Activate missing with
    /// its count, or what keeps its fonts from being all active.
    fn activate_line(&self) -> Vec<(String, Role)> {
        let counts = &self.known.counts;
        let n = counts.not_active;
        let (mark, role) = vtium_config_core::fontuse::NOT_ALL_ACTIVE;
        let name = self.shared.document().name();
        let mut out = vec![(" ".to_owned(), Role::Row), (mark.to_owned(), role)];
        if n > 0 {
            out.push((" · ".to_owned(), Role::Row));
            out.push((words::activate_row(n), Role::Heading));
        } else {
            out.push((
                words::not_all_row(name, counts.fails_trust, counts.missing),
                Role::Dim,
            ));
        }
        out
    }

    /// A font's row: its name, its kind and its mark, and the look that
    /// uses it beside a mark that is not active.
    fn item_line(&self, at: usize, columns: Columns, width: usize) -> Vec<(String, Role)> {
        let Some(&item) = self.known.items.get(at) else {
            return Vec::new();
        };
        let (name, kind) = match item {
            Item::File(i) => {
                let f = self.shared.found.fonts.files().get(i);
                (
                    f.map(|f| f.resource.name()).unwrap_or_default(),
                    words::kind(f.and_then(kind_of)),
                )
            }
            Item::BuiltIn(name) => (name.to_owned(), words::kind(Some((false, false)))),
            Item::Named(_) => (
                self.known
                    .fields
                    .get(at)
                    .map(|f| f[0].clone())
                    .unwrap_or_default(),
                "",
            ),
        };
        let mut out = vec![
            (" ".repeat(INDENT), Role::Row),
            (pad(&name, columns.name), Role::Row),
        ];
        if columns.kind > 0 {
            out.push((pad(kind, columns.kind), Role::Dim));
        }
        let state_cells = width.saturating_sub(INDENT + columns.name + columns.kind + 1);
        let marks: Vec<(String, Role)> = match self.known.state(at) {
            None => vec![(words::ALWAYS.0.to_owned(), words::ALWAYS.1)],
            Some(state) => words::short_mark(state, state_cells)
                .into_iter()
                .map(|(w, r)| (w.to_owned(), r))
                .collect(),
        };
        let used = canvas::cells(&marks.iter().map(|(w, _)| w.as_str()).collect::<String>());
        out.extend(marks);
        let inactive = self.known.state(at).is_some_and(|s| *s != State::Active);
        if inactive && let Some(look) = words::first_look(self.known.users(self.shared, item)) {
            let room = state_cells.saturating_sub(used).saturating_sub(3);
            if room >= 4 {
                out.push((" · ".to_owned(), Role::Row));
                out.push((canvas::fit(&look, room), Role::Warning));
            }
        }
        out
    }
}

/// Whether a font file has outlines and whether it is a color emoji font,
/// from its first face, once the font helper has described it.
pub fn kind_of(f: &FontFile) -> Option<(bool, bool)> {
    let face = f.faces().first()?;
    Some((face.outline, face.groups.contains(&FontGroup::ColorEmoji)))
}

/// `text` cut to `cells` and padded to them, a blank kept at the end.
fn pad(text: &str, cells: usize) -> String {
    let cut = canvas::fit(text, cells.saturating_sub(1));
    let room = cells.saturating_sub(canvas::cells(&cut));
    format!("{cut}{}", " ".repeat(room))
}
