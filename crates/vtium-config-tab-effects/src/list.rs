//! The list at the left: the filter line, then the vtlooks, the color maps
//! and the shaders in their groups, each row with its name and its marks.
//! It also holds what the tab knows of each item: its name, whether a
//! resource is active once the document's draft is counted, and the looks
//! that use it, and the choices of the list of groups that the filter line
//! opens.
//!
//! References:
//! - ADR-1403 D10, the Effects tab: the filter line, one list in three
//!   groups, No filter first, then the presets, then the administrator's
//!   vtlooks, where one that takes a preset's name hides the preset.
//! - ADR-1403 D11, "The tabs' layouts", the Effects bullet: the list at
//!   the left of the chain.
//! - ADR-1205 D4: a resource the batch does not list, or that fails the
//!   trust rule, counts as missing.

use std::borrow::Cow;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::batch::{Change as BatchChange, Through};
use vtium_config_core::found::effects::{Built, Effects, From, Group};
use vtium_config_core::found::{Activation, Origin, Resource};
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::tab::Choices;

use crate::words;

/// One item of the list: a vtlook, a color map or a shader, by its place
/// in what was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Vtlook(usize),
    Map(usize),
    Shader(usize),
}

impl Item {
    /// The group the item belongs to.
    pub const fn group(self) -> Group {
        match self {
            Item::Vtlook(_) => Group::Vtlooks,
            Item::Map(_) => Group::ColorMaps,
            Item::Shader(_) => Group::Shaders,
        }
    }
}

/// Whether `group` shows when the list shows `shown`.
pub fn shows(shown: Group, group: Group) -> bool {
    shown == Group::All || shown == group
}

/// The items the list shows for `group`, in its order.
pub fn items(effects: &Effects, group: Group) -> Vec<Item> {
    let mut out = Vec::new();
    if shows(group, Group::Vtlooks) {
        out.extend((0..effects.vtlooks.len()).map(Item::Vtlook));
    }
    if shows(group, Group::ColorMaps) {
        out.extend((0..effects.maps.len()).map(Item::Map));
    }
    if shows(group, Group::Shaders) {
        out.extend((0..effects.shaders.len()).map(Item::Shader));
    }
    out
}

/// The color map or shader `item` is, if it is one.
pub fn resource(effects: &Effects, item: Item) -> Option<&Resource> {
    match item {
        Item::Map(i) => effects.maps.get(i),
        Item::Shader(i) => effects.shaders.get(i).map(|s| &s.resource),
        Item::Vtlook(_) => None,
    }
}

/// The item's name as the list shows it, with control characters
/// replaced, since a file's name is the administrator's text.
pub fn name(effects: &Effects, item: Item) -> String {
    match item {
        Item::Vtlook(i) => effects.vtlooks.get(i).map_or_else(String::new, |v| {
            if v.from == From::BuiltIn {
                words::NO_FILTER.to_owned()
            } else {
                canvas::printable(&v.name)
            }
        }),
        Item::Map(_) | Item::Shader(_) => {
            resource(effects, item).map_or_else(String::new, |r| canvas::printable(&r.name()))
        }
    }
}

/// Whether the installed batch keeps `origin` active.
pub fn installed_active(origin: &Origin) -> bool {
    matches!(
        origin,
        Origin::BatchFile | Origin::FreshInstall | Origin::List { .. }
    )
}

/// Whether `r` is active once the open document's draft is counted: the
/// draft's change where it makes one, else the installed batch.
pub fn active(shared: &Shared, r: &Resource) -> bool {
    match r.activation(&shared.document().content().active) {
        Activation::BuiltIn => true,
        Activation::Draft(change) => change == BatchChange::Activate,
        Activation::Installed(origin) => installed_active(origin),
    }
}

/// The looks of the document that use the color map or shader `r`, each
/// with the vtlook it uses `r` through.
pub fn uses(shared: &Shared, r: &Resource) -> Vec<(String, Option<String>)> {
    shared
        .found
        .effects
        .uses(shared.document().text(), &r.path)
        .into_iter()
        .map(|u| {
            let through = match u.through {
                Through::Vtlook(v) => Some(v),
                Through::FontSet(_) | Through::Palette | Through::PointerTheme => None,
            };
            (words::look_name(u.look.as_deref()), through)
        })
        .collect()
}

/// The looks of the document that draw with the vtlook `name`.
pub fn vtlook_users(shared: &Shared, name: &str) -> Vec<String> {
    vtium_config_core::batch::drawing_vtlook(shared.document().text(), name)
        .iter()
        .map(|look| words::look_name(look.as_deref()))
        .collect()
}

/// A resource's marks: active or not, then the person's own folder or a
/// failed trust check, as one row of the list shows them.
pub fn resource_marks(shared: &Shared, r: &Resource) -> (String, Role) {
    if r.own {
        return (words::OWN.to_owned(), Role::Missing);
    }
    let on = active(shared, r);
    let mut more = Vec::new();
    if r.trust.is_err() {
        more.push(words::TRUST);
    }
    if r.hidden {
        more.push(words::HIDDEN);
    }
    // the circle goes where words follow it, so the row keeps its words
    let text = match (on, more.is_empty()) {
        (true, true) => words::ACTIVE.to_owned(),
        (false, true) => words::NOT_ACTIVE.to_owned(),
        (true, false) => format!("{} · {}", words::ACTIVE_WORD, more.join(" · ")),
        (false, false) => format!("{} · {}", words::NOT_ACTIVE_WORD, more.join(" · ")),
    };
    let role = match (on, r.trust.is_ok()) {
        (true, true) => Role::Active,
        (_, false) => Role::Missing,
        (false, true) => Role::Inactive,
    };
    (text, role)
}

/// An item's marks in its row: a vtlook's origin, whether it is hidden,
/// and whether it names a planned filter or does not read; a resource's
/// as [`resource_marks`] gives them.
pub fn marks(shared: &Shared, item: Item) -> (String, Role) {
    let effects = &shared.found.effects;
    match item {
        Item::Vtlook(i) => {
            let Some(v) = effects.vtlooks.get(i) else {
                return (String::new(), Role::Dim);
            };
            let mut parts = vec![words::from_short(v.from)];
            let mut role = Role::Dim;
            if v.hidden {
                parts.push(words::HIDDEN);
            }
            if v.filters
                .iter()
                .any(|f| matches!(f.built, Built::Planned { .. }))
            {
                parts.push(words::PLANNED);
            } else if v.reads.is_err() {
                parts.push(words::ERROR);
                role = Role::Warning;
            }
            (parts.join(", "), role)
        }
        Item::Map(_) | Item::Shader(_) => resource(effects, item)
            .map_or((String::new(), Role::Dim), |r| resource_marks(shared, r)),
    }
}

/// One row of the list as it is laid out, before the window cuts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// A group's heading, with the number of items it holds in the list.
    Heading(Group, usize),
    /// An item, by its place in the list the filter keeps.
    Item(usize),
}

/// The rows of the list for `shown`, the items of the groups it shows,
/// of which it keeps those `keep` keeps: each group's heading, then its
/// items kept.
/// While `filtering`, a group that keeps no item has no heading either.
pub fn lines(
    shown: &[Item],
    keep: &dyn Fn(usize) -> bool,
    filtering: bool,
    group: Group,
    effects: &Effects,
) -> Vec<Line> {
    let mut out = Vec::new();
    for g in [Group::Vtlooks, Group::ColorMaps, Group::Shaders] {
        if !shows(group, g) {
            continue;
        }
        let total = match g {
            Group::Vtlooks => effects.vtlooks.len(),
            Group::ColorMaps => effects.maps.len(),
            _ => effects.shaders.len(),
        };
        let kept: Vec<Line> = shown
            .iter()
            .enumerate()
            .filter(|(at, i)| i.group() == g && keep(*at))
            .map(|(at, _)| Line::Item(at))
            .collect();
        if filtering && kept.is_empty() {
            continue;
        }
        out.push(Line::Heading(g, total));
        out.extend(kept);
    }
    out
}

/// The rows of the pane the list's lines take: the pane less the filter
/// line and the blank under it.
pub fn rows(pane: Rect) -> usize {
    usize::from(pane.height.saturating_sub(FIRST_ROW))
}

/// The row of the pane the list's first line is drawn on.
pub const FIRST_ROW: u16 = 2;

/// The first line the window shows, so that the line `at` shows, moving
/// the window from `offset` as little as it must.
pub fn settled(lines: &[Line], at: Option<usize>, offset: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    let line = at.and_then(|a| lines.iter().position(|l| *l == Line::Item(a)));
    let mut offset = offset;
    match line {
        // the filter line: the list's top
        None if at.is_none() => offset = 0,
        None => {}
        Some(l) => {
            // a group's first item brings its heading with it
            let l = match l.checked_sub(1).and_then(|h| lines.get(h)) {
                Some(Line::Heading(..)) if l <= offset => l.saturating_sub(1),
                _ => l,
            };
            if l < offset {
                offset = l;
            } else if l >= offset.saturating_add(rows) {
                offset = l.saturating_add(1).saturating_sub(rows);
            }
        }
    }
    offset.min(lines.len().saturating_sub(rows))
}

/// The most cells the name's column takes, before the marks; a longer
/// name is cut with `…`, and the pane beside the list gives it whole.
pub const NAME_CELLS: usize = 12;

/// The fewest cells the name's column takes, which No filter fills.
const NAME_MIN: usize = 9;

/// The cells before an item's name.
const INDENT: usize = 2;

/// What the left pane draws.
pub struct Pane<'a> {
    pub shared: &'a Shared,
    pub group: Group,
    pub shown: &'a [Item],
    pub lines: &'a [Line],
    pub offset: usize,
    /// The highlighted item, by its place in `shown`, or `None` on the
    /// filter line.
    pub highlight: Option<usize>,
    /// Whether the highlight shows: not while a search matches nothing.
    pub lit: bool,
    /// The `?` search's entry, while a search filters the list.
    pub matching: Option<&'a str>,
}

impl Pane<'_> {
    /// Draws the pane into `area`.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let width = usize::from(area.width);
        let mut show = words::show(group_name(self.group));
        if let Some(entry) = self.matching {
            show.push_str(&words::matching(entry));
        }
        let rows = rows(area);
        // where the window stands among the items the list keeps
        let ordinal = |l: &Line| matches!(l, Line::Item(_));
        let count = self.lines.iter().filter(|l| ordinal(l)).count();
        let window = (self.lines.len() > rows).then(|| {
            let before = self
                .lines
                .iter()
                .take(self.offset)
                .filter(|l| ordinal(l))
                .count();
            let inside = self
                .lines
                .iter()
                .skip(self.offset)
                .take(rows)
                .filter(|l| ordinal(l))
                .count();
            words::window(
                before.saturating_add(1).min(count),
                before.saturating_add(inside),
                count,
            )
        });
        let line_style = if self.highlight.is_none() && self.lit {
            Role::RowHighlight
        } else {
            Role::Row
        };
        let left = format!(" {show}");
        // the window's place goes at the right where the row has room
        let filter_text = match &window {
            Some(w)
                if canvas::cells(&left)
                    .saturating_add(canvas::cells(w))
                    .saturating_add(2)
                    <= width =>
            {
                right_aligned(&left, w, width.saturating_sub(1))
            }
            _ => left,
        };
        put_row(
            buf,
            area,
            0,
            &canvas::fit(&filter_text, width.saturating_sub(1)),
            line_style,
            color,
        );
        for (r, line) in self.lines.iter().skip(self.offset).take(rows).enumerate() {
            let y = FIRST_ROW.saturating_add(u16::try_from(r).unwrap_or(u16::MAX));
            match *line {
                Line::Heading(g, total) => self.heading(buf, area, y, g, total, color),
                Line::Item(at) => self.item(buf, area, y, at, color),
            }
        }
    }

    /// The name column's cells: the longest name in the window, from
    /// [`NAME_MIN`] to [`NAME_CELLS`].
    fn name_cells(&self) -> usize {
        let effects = &self.shared.found.effects;
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::Item(at) => self.shown.get(*at),
                Line::Heading(..) => None,
            })
            .map(|i| canvas::cells(&name(effects, *i)))
            .max()
            .unwrap_or(0)
            .clamp(NAME_MIN, NAME_CELLS)
    }

    /// A group's heading, with what it adds: none installed for an empty
    /// group, and the planned mark of the shaders.
    fn heading(&self, buf: &mut Buffer, area: Rect, y: u16, g: Group, total: usize, color: bool) {
        let name = match g {
            Group::Vtlooks => words::VTLOOKS,
            Group::ColorMaps => words::COLOR_MAPS,
            _ => words::SHADERS,
        };
        let x = put(buf, area, 1, y, name, Role::Heading, color);
        let note = if total == 0 {
            Some(words::NONE_INSTALLED.to_owned())
        } else if g == Group::Shaders {
            Some(words::shaders_planned())
        } else {
            None
        };
        if let Some(note) = note {
            put(buf, area, x.saturating_add(2), y, &note, Role::Dim, color);
        }
    }

    /// An item's row: its name, then its marks, in reverse video while it
    /// is highlighted.
    fn item(&self, buf: &mut Buffer, area: Rect, y: u16, at: usize, color: bool) {
        let Some(&item) = self.shown.get(at) else {
            return;
        };
        let effects = &self.shared.found.effects;
        let width = usize::from(area.width).saturating_sub(1);
        let cells = self.name_cells();
        let name = canvas::fit(&name(effects, item), cells);
        let (marks, role) = marks(self.shared, item);
        let lit = self.lit && self.highlight == Some(at);
        if lit {
            let text = format!(
                "{}{:<w$} {marks}",
                " ".repeat(INDENT),
                name,
                w = pad(&name, cells)
            );
            put_row(
                buf,
                area,
                y,
                &canvas::fit(&text, width),
                Role::RowHighlight,
                color,
            );
            return;
        }
        let x = put(buf, area, INDENT, y, &name, Role::Row, color);
        let at_marks = (INDENT + 1).saturating_add(cells).max(x.saturating_add(1));
        let room = width.saturating_sub(at_marks);
        put(
            buf,
            area,
            at_marks,
            y,
            &canvas::fit(&marks, room),
            role,
            color,
        );
    }
}

/// The groups' names as the filter line names them.
pub fn group_name(group: Group) -> &'static str {
    let at = Group::ALL.iter().position(|g| *g == group).unwrap_or(0);
    words::GROUP_SHORT.get(at).copied().unwrap_or("")
}

/// The width `format!`'s padding takes for `text` to fill `cells` cells,
/// which counts characters where the console counts cells.
fn pad(text: &str, cells: usize) -> usize {
    cells
        .saturating_sub(canvas::cells(text))
        .saturating_add(text.chars().count())
}

/// `left`, then `right` at the right of `width` cells.
pub fn right_aligned(left: &str, right: &str, width: usize) -> String {
    let gap = width
        .saturating_sub(canvas::cells(left))
        .saturating_sub(canvas::cells(right))
        .max(1);
    format!("{left}{}{right}", " ".repeat(gap))
}

/// Writes `text` from column `x` of row `y` of `area` in `role`, cut at
/// the area's right edge, and gives the column after it.
pub fn put(
    buf: &mut Buffer,
    area: Rect,
    x: usize,
    y: u16,
    text: &str,
    role: Role,
    color: bool,
) -> usize {
    let room = usize::from(area.width).saturating_sub(x);
    if y >= area.height || room == 0 {
        return x;
    }
    let text = canvas::fit(text, room);
    let cx = area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX));
    buf.set_stringn(
        cx,
        area.y.saturating_add(y),
        &text,
        room,
        role.style(color).cell(),
    );
    x.saturating_add(canvas::cells(&text))
}

/// Writes `text` across the whole row `y` of `area` in `role`, padded to
/// the area's width, as a highlighted row is drawn.
pub fn put_row(buf: &mut Buffer, area: Rect, y: u16, text: &str, role: Role, color: bool) {
    let width = usize::from(area.width);
    let text = canvas::fit(text, width);
    let padded = format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(canvas::cells(&text)))
    );
    put(buf, area, 0, y, &padded, role, color);
}

/// The choices of the list of groups that the filter line opens.
pub struct Groups {
    /// The groups the list shows now.
    pub now: Group,
}

impl Choices<Group> for Groups {
    fn count(&self) -> usize {
        Group::ALL.len()
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        Cow::Borrowed(words::GROUP_CHOICES.get(choice).copied().unwrap_or(""))
    }

    fn current(&self) -> Option<usize> {
        Group::ALL.iter().position(|g| *g == self.now)
    }

    fn keep(&self, choice: usize) -> Group {
        Group::ALL.get(choice).copied().unwrap_or_default()
    }
}
