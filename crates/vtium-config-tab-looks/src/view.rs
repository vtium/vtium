//! The tab's screens: the list with the highlighted item's card or facts
//! beside it above the preview; a look's card over the viewer, with the
//! list box under a line; what a look still misses; and a detail. Each
//! screen keeps every fact the old screen showed, one line where a
//! paragraph was, and the rest behind F1 or the detail. This module also
//! says where each part went, for clicks, and gives F1's text and Find's
//! items.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Looks bullet: the card beside the list, above the preview, and on another item the facts its detail lists.
//! - ADR-1403 D11, "F1, help for what is highlighted": the full text behind a line the screen cut.
//! - crates/AGENTS.md, "Strings on screen", rules 7 and 8: one line where a paragraph was, and every key in brackets.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Color as Cell;
use ratatui_core::terminal::Frame;
use vtium_config_core::fontuse::{State, Used};
use vtium_config_core::preview::{self, console};
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::tab::FindItem;

use crate::card::{self, Line};
use crate::draw::{self, Panes, put, put_parts};
use crate::list::{self, Item, Look, Row};
use crate::missing::{self, RoleRow};
use crate::resources::{self, Detail, Row as DetailRow};
use crate::{Card, CardOpen, Looks, Open, Places, words};

/// The columns of a card line's label.
const LABEL: usize = 15;

/// The card's first row of lines, below its title and a blank row.
const LINES_AT: u16 = 2;

/// The card's row of "Fonts by role", below the lines and the caption.
const ROLES_AT: u16 = 11;

/// Draws the tab's screen in `area`.
pub fn draw(tab: &Looks, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
    let buf = frame.buffer_mut();
    let color = tab.color;
    match &tab.open {
        Open::Nothing => list_screen(tab, shared, buf, area),
        Open::Card(c) => match &c.open {
            CardOpen::Missing { top } => misses_screen(tab, shared, c, *top, buf, area),
            _ => {
                let cursor = card_screen(tab, shared, c, buf, area);
                if let CardOpen::List { list, line } = &c.open
                    && let Some(look) = list::look(shared.document().text(), &c.name)
                {
                    let choices = card::choices(shared, &look, *line);
                    let rect = list_box_rect(c, list, &choices, area);
                    list.draw(&choices, buf, rect, color);
                }
                if let Some(at) = cursor {
                    frame.set_cursor_position(at);
                }
            }
        },
        Open::Detail { item, top } => {
            let used = tab.used(shared);
            if let Some(d) = resources::detail(shared, item, &used) {
                detail_screen(&d, *top, buf, area, color);
            }
        }
    }
}

/// The list's rows, the offset that keeps the highlight in sight, and
/// the items.
fn list_rows(tab: &Looks, shared: &Shared, area: Rect) -> (Vec<Item>, Vec<Row>, usize) {
    let items = list::items(shared);
    let rows = list::rows(&items, tab.filter.as_ref());
    let at = rows.iter().position(|r| *r == Row::Item(tab.highlight));
    let height = usize::from(Panes::of(area).list.height);
    let offset = list::settled(at, tab.offset, height, rows.len());
    (items, rows, offset)
}

/// The first row of the list the tab shows in `area`, which the tab takes
/// as its own after the highlight moved.
pub fn settled_offset(tab: &Looks, shared: &Shared, area: Rect) -> usize {
    list_rows(tab, shared, area).2
}

/// The list screen: the list, the rule, and at the right the highlighted
/// item's card or facts above the preview.
fn list_screen(tab: &Looks, shared: &Shared, buf: &mut Buffer, area: Rect) {
    let color = tab.color;
    let panes = Panes::of(area);
    let (items, rows, offset) = list_rows(tab, shared, area);
    let used = tab.used(shared);
    let marks = |name: &str| missing::mark(&used, name);
    let highlight = rows
        .contains(&Row::Item(tab.highlight))
        .then_some(tab.highlight);
    list::Pane {
        shared,
        items: &items,
        rows: &rows,
        highlight,
        offset,
        marks: &marks,
    }
    .draw(buf, panes.list, color);
    draw::rule(buf, panes.rule, color);
    let Some(item) = highlight.and_then(|h| items.get(h)) else {
        if let Some(f) = &tab.filter {
            put(
                buf,
                panes.card.x,
                panes.card.y,
                &words::no_match(f.entry()),
                Role::Dim.style(color).cell(),
                panes.card,
            );
        }
        return;
    };
    match item {
        Item::Look(name) => {
            if let Some(look) = list::look(shared.document().text(), name) {
                side_card(shared, &look, &used, buf, panes.card, color);
                look_preview(tab, shared, buf, &panes);
                put(
                    buf,
                    panes.caption.x,
                    panes.caption.y,
                    words::SIZE_APPLIES,
                    Role::Dim.style(color).cell(),
                    panes.caption,
                );
            }
        }
        Item::NoLooks => {
            put(
                buf,
                panes.card.x,
                panes.card.y,
                words::NO_LOOKS,
                Role::Heading.style(color).cell(),
                panes.card,
            );
            put(
                buf,
                panes.card.x,
                panes.card.y.saturating_add(2),
                words::NO_LOOKS_NOTE,
                Role::Dim.style(color).cell(),
                panes.card,
            );
        }
        _ => {
            let Some(d) = resources::detail(shared, item, &used) else {
                return;
            };
            side_facts(&d, buf, panes.card, color);
            match item {
                Item::FontSet(_) => look_preview(tab, shared, buf, &panes),
                Item::Scheme(_) => {
                    if let Some(rect) = panes.preview {
                        scheme_box(&d, buf, rect, color);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The preview's box under the card: in a vtium console, the console's
/// own text in its font; elsewhere the place the picture of the console
/// goes, which the shell draws there.
fn look_preview(tab: &Looks, shared: &Shared, buf: &mut Buffer, panes: &Panes) {
    let Some(rect) = panes.preview else { return };
    let in_console = preview::text_preview(&shared.console);
    let title = if tab.picture {
        words::PICTURE_TITLE
    } else {
        words::SAMPLE_TITLE
    };
    let lines = match (in_console, tab.picture) {
        (false, _) => Vec::new(),
        (true, false) => console::sample_rows(),
        (true, true) => vtium_config_core::specimen::detail_lines(&tab.locale)
            .into_iter()
            .map(|l| vec![canvas::span(l, vtium_config_widgets::color::Style::PLAIN)])
            .collect(),
    };
    draw::preview(buf, rect, title, lines, tab.color);
}

/// A color scheme's box: its 16 colors, as many to a row as the box
/// holds, and a line in its foreground on its background.
fn scheme_box(d: &Detail, buf: &mut Buffer, rect: Rect, color: bool) {
    draw::preview(buf, rect, &words::colors_title(&d.title), Vec::new(), color);
    let inside = Rect::new(
        rect.x.saturating_add(2),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(3),
        rect.height.saturating_sub(2),
    );
    let swatches: Vec<(u8, vtium_term::Rgb)> = d
        .rows
        .iter()
        .filter_map(|r| match r {
            DetailRow::Swatches(s) => Some(s.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let per_row = (usize::from(inside.width) / SWATCH_CELLS).max(1);
    let mut y = inside.y;
    for chunk in swatches.chunks(per_row) {
        detail_row(
            &DetailRow::Swatches(chunk.to_vec()),
            buf,
            inside.x,
            y,
            inside,
            color,
        );
        y = y.saturating_add(1);
    }
    if let Some(sample) = d
        .rows
        .iter()
        .find(|r| matches!(r, DetailRow::Sample { .. }))
    {
        detail_row(sample, buf, inside.x, y.saturating_add(1), inside, color);
    }
}

/// The cells one swatch takes: its number, its block and its value.
const SWATCH_CELLS: usize = 15;

/// The highlighted look's card beside the list.
fn side_card(shared: &Shared, look: &Look, used: &Used, buf: &mut Buffer, r: Rect, color: bool) {
    title(buf, r, &look.name, &users_line(shared, &look.name), color);
    let mut y = r.y.saturating_add(LINES_AT);
    for line in Line::ALL.iter().skip(1) {
        let text = format!(
            "{:<LABEL$}{}",
            line.label(),
            card::shown(shared, look, *line)
        );
        put(buf, r.x, y, &text, Role::Row.style(color).cell(), r);
        y = y.saturating_add(1);
    }
    let counts = missing::counts(used, &look.name);
    let mut parts = vec![(format!("{:<LABEL$}", words::LINE_FONTS), Role::Row)];
    if counts.all_active() {
        parts.push((words::ALL_ACTIVE.to_owned(), Role::Active));
    } else {
        parts.push((
            format!(
                "○ {}",
                words::fonts_not_active(missing::not_active(&counts))
            ),
            Role::Warning,
        ));
    }
    put_parts(buf, r.x, y, &parts, color, r);
    if !counts.all_active() {
        put(
            buf,
            r.x.saturating_add(u16::try_from(LABEL).unwrap_or(0)),
            y.saturating_add(1),
            words::ENTER_FOR_WHICH,
            Role::Dim.style(color).cell(),
            r,
        );
    }
}

/// The facts of a font set, a color scheme or a pointer theme beside the
/// list: its title and the first of its detail's rows.
fn side_facts(d: &Detail, buf: &mut Buffer, r: Rect, color: bool) {
    title(buf, r, &d.title, &d.right, color);
    let mut y = r.y.saturating_add(LINES_AT);
    let mut rows: Vec<&DetailRow> = d
        .rows
        .iter()
        .take_while(|row| !matches!(row, DetailRow::Swatches(_) | DetailRow::Sample { .. }))
        .collect();
    // a heading whose rows the box below shows, and a blank row before
    // it, leave the facts
    while rows
        .last()
        .is_some_and(|row| is_heading(row) || row.words().is_empty())
    {
        rows.pop();
    }
    let room = usize::from(r.height.saturating_sub(LINES_AT));
    for row in rows.into_iter().take(room) {
        detail_row(row, buf, r.x, y, r, color);
        y = y.saturating_add(1);
    }
}

/// Whether `row` is a heading of a detail.
fn is_heading(row: &DetailRow) -> bool {
    matches!(row, DetailRow::Text(parts) if parts.len() == 1
        && parts.first().is_some_and(|(_, role)| *role == Role::Heading))
}

/// A title in its role, and `right` at the right of its row.
fn title(buf: &mut Buffer, r: Rect, text: &str, right: &str, color: bool) {
    let x = put(buf, r.x, r.y, text, Role::Heading.style(color).cell(), r);
    let cells = u16::try_from(canvas::cells(right)).unwrap_or(u16::MAX);
    let at = r.right().saturating_sub(cells).max(x.saturating_add(2));
    put(buf, at, r.y, right, Role::Dim.style(color).cell(), r);
}

/// Which terminals draw the look `name`, as the card's title says it.
fn users_line(shared: &Shared, name: &str) -> String {
    words::drawn_by(&list::users(shared.document().text(), name))
}

/// One of a detail's rows at `x`, `y`.
fn detail_row(row: &DetailRow, buf: &mut Buffer, x: u16, y: u16, r: Rect, color: bool) {
    match row {
        DetailRow::Text(parts) => {
            put_parts(buf, x, y, parts, color, r);
        }
        DetailRow::Swatches(swatches) => {
            let mut at = x;
            for (n, rgb) in swatches {
                at = put(
                    buf,
                    at,
                    y,
                    &format!("{n:>2} "),
                    Role::Row.style(color).cell(),
                    r,
                );
                let block = if color {
                    ratatui_core::style::Style::default().fg(Cell::Rgb(rgb.r, rgb.g, rgb.b))
                } else {
                    Role::Row.style(false).cell()
                };
                at = put(buf, at, y, "██", block, r);
                at = put(
                    buf,
                    at,
                    y,
                    &format!(" {}  ", resources::hex(*rgb)),
                    Role::Row.style(color).cell(),
                    r,
                );
            }
        }
        DetailRow::Sample { fg, bg } => {
            let style = if color {
                ratatui_core::style::Style::default()
                    .fg(Cell::Rgb(fg.r, fg.g, fg.b))
                    .bg(Cell::Rgb(bg.r, bg.g, bg.b))
            } else {
                Role::Row.style(false).cell()
            };
            put(buf, x, y, words::SAMPLE_LINE, style, r);
        }
    }
}

/// A detail over the viewer: its title, then its rows from `top`.
fn detail_screen(d: &Detail, top: usize, buf: &mut Buffer, area: Rect, color: bool) {
    let r = inset(area);
    title(buf, r, &d.title, &d.right, color);
    let mut y = r.y.saturating_add(LINES_AT);
    for row in d.rows.iter().skip(top) {
        if y >= r.bottom() {
            break;
        }
        detail_row(row, buf, r.x.saturating_add(2), y, r, color);
        y = y.saturating_add(1);
    }
}

/// The rows a detail can scroll to, so its last row stays in sight in
/// `area`.
pub fn detail_last_top(d: &Detail, area: Rect) -> usize {
    let room = usize::from(area.height.saturating_sub(LINES_AT));
    d.rows.len().saturating_sub(room)
}

/// `area` less a column at each side.
fn inset(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    )
}

/// The rows of a card over `area`: each line's and each role's row, by
/// its number, and the role rows' data.
fn card_rows(area: Rect, roles: usize) -> Vec<(Rect, usize)> {
    let r = inset(area);
    let mut out = Vec::new();
    for i in 0..Line::ALL.len() {
        let y =
            r.y.saturating_add(LINES_AT)
                .saturating_add(u16::try_from(i).unwrap_or(0));
        out.push((Rect::new(r.x, y, r.width, 1), i));
    }
    for j in 0..roles {
        let y =
            r.y.saturating_add(ROLES_AT)
                .saturating_add(1)
                .saturating_add(u16::try_from(j).unwrap_or(u16::MAX));
        if y < r.bottom() {
            out.push((
                Rect::new(r.x, y, r.width, 1),
                Line::ALL.len().saturating_add(j),
            ));
        }
    }
    out
}

/// The look's card over the viewer. Returns where the cursor goes while
/// the name is typed.
fn card_screen(
    tab: &Looks,
    shared: &Shared,
    c: &Card,
    buf: &mut Buffer,
    area: Rect,
) -> Option<ratatui_core::layout::Position> {
    let color = tab.color;
    let r = inset(area);
    let Some(look) = list::look(shared.document().text(), &c.name) else {
        put(
            buf,
            r.x,
            r.y,
            &words::look_gone(&c.name),
            Role::Warning.style(color).cell(),
            r,
        );
        return None;
    };
    title(buf, r, &look.name, &users_line(shared, &look.name), color);
    let used = tab.used(shared);
    let roles = missing::roles(&used, &look.name);
    let rows = card_rows(area, roles.len());
    let mut cursor = None;
    for (rect, i) in &rows {
        let lit = c.row == *i && !matches!(c.open, CardOpen::List { .. });
        let style = if lit { Role::RowHighlight } else { Role::Row }
            .style(color)
            .cell();
        put(
            buf,
            rect.x,
            rect.y,
            &" ".repeat(usize::from(rect.width)),
            style,
            *rect,
        );
        if let Some(line) = Line::ALL.get(*i) {
            let value = match (&c.open, line) {
                (CardOpen::Typing(text), Line::Name) => {
                    let x = rect
                        .x
                        .saturating_add(u16::try_from(LABEL.saturating_add(2)).unwrap_or(0));
                    let cells = u16::try_from(canvas::cells(text)).unwrap_or(0);
                    cursor = Some(ratatui_core::layout::Position::new(
                        x.saturating_add(cells),
                        rect.y,
                    ));
                    text.clone()
                }
                _ => card::shown(shared, &look, *line),
            };
            let text = format!("  {:<LABEL$}{value}", line.label());
            put(buf, rect.x, rect.y, &text, style, *rect);
            if lit && line.key().is_some() {
                let at = rect.right().saturating_sub(5);
                put(buf, at, rect.y, words::STEPS, style, *rect);
            }
        } else if let Some(role) = roles.get(i.saturating_sub(Line::ALL.len())) {
            role_row(role, lit, buf, *rect, color);
        }
    }
    let caption_y = r.y.saturating_add(LINES_AT).saturating_add(7);
    put(
        buf,
        r.x,
        caption_y,
        words::SIZE_APPLIES,
        Role::Dim.style(color).cell(),
        r,
    );
    let heading_y = r.y.saturating_add(ROLES_AT);
    let x = put(
        buf,
        r.x,
        heading_y,
        words::FONTS_BY_ROLE,
        Role::Heading.style(color).cell(),
        r,
    );
    let counts = missing::counts(&used, &look.name);
    let right = if counts.not_active > 0 {
        words::activate_line(counts.not_active)
    } else if counts.all_active() {
        words::ALL_ACTIVE.to_owned()
    } else {
        vtium_config_core::fontuse::NOT_ALL_ACTIVE.0.to_owned()
    };
    let cells = u16::try_from(canvas::cells(&right)).unwrap_or(0);
    let at = r.right().saturating_sub(cells).max(x.saturating_add(2));
    let role = if counts.all_active() {
        Role::Active
    } else {
        Role::Warning
    };
    put(buf, at, heading_y, &right, role.style(color).cell(), r);
    if roles.is_empty() {
        let text = match card::font_set(shared, &look) {
            Some(f) if vtium_font_helper::frit::RESERVED.contains(&f.as_str()) => {
                words::built_in_text(&f)
            }
            _ => words::NO_FONT_FILES.to_owned(),
        };
        put(
            buf,
            r.x.saturating_add(2),
            heading_y.saturating_add(1),
            &text,
            Role::Dim.style(color).cell(),
            r,
        );
    }
    cursor
}

/// One row of "Fonts by role": the role, the font, its mark, and what a
/// key does about it.
fn role_row(role: &RoleRow, lit: bool, buf: &mut Buffer, rect: Rect, color: bool) {
    let base = if lit { Role::RowHighlight } else { Role::Row };
    let font_w = usize::from(rect.width)
        .saturating_sub(LABEL + 2 + 28)
        .clamp(10, 47);
    let font = canvas::fit(&role.font, font_w.saturating_sub(1));
    let mut parts = vec![(format!("  {:<LABEL$}{font:<font_w$}", role.role), base)];
    if let Some(state) = &role.state {
        if lit {
            parts.push((state.words(), base));
        } else {
            parts.extend(missing::state_mark(state));
        }
        let hint = match state {
            State::Missing | State::FailsTrust { .. } => Some(words::SAYS_WHY),
            State::NotActive => Some(words::ACTIVATES_IT),
            _ => None,
        };
        if let Some(hint) = hint {
            parts.push((format!("  {hint}"), if lit { base } else { Role::Dim }));
        }
    }
    put_parts(buf, rect.x, rect.y, &parts, color, rect);
}

/// Where the list box of `line` goes, under the line's row.
pub fn list_box_rect(
    c: &Card,
    list: &vtium_config_widgets::list_box::ListBox,
    choices: &card::LineChoices,
    area: Rect,
) -> Rect {
    let rows = card_rows(area, 0);
    let row = Line::ALL
        .iter()
        .position(|l| *l == choices.line)
        .unwrap_or(c.row);
    let y = rows
        .iter()
        .find(|(_, i)| *i == row)
        .map_or(area.y, |(r, _)| r.y);
    // the box's border sits one column before the line's value
    let x = inset(area)
        .x
        .saturating_add(u16::try_from(LABEL.saturating_add(1)).unwrap_or(0));
    list.place(choices, area, y, x)
}

/// What the look `c` still misses, over the viewer, from line `top`.
fn misses_screen(tab: &Looks, shared: &Shared, c: &Card, top: usize, buf: &mut Buffer, area: Rect) {
    let color = tab.color;
    let r = inset(area);
    put(
        buf,
        r.x,
        r.y,
        &words::misses_title(&c.name),
        Role::Heading.style(color).cell(),
        r,
    );
    let lines = misses_lines(tab, shared, &c.name, usize::from(r.width).saturating_sub(4));
    let mut y = r.y.saturating_add(LINES_AT);
    for line in lines.iter().skip(top) {
        if y >= r.bottom() {
            break;
        }
        put(
            buf,
            r.x.saturating_add(2),
            y,
            line,
            Role::Row.style(color).cell(),
            r,
        );
        y = y.saturating_add(1);
    }
}

/// The lines of what the look `name` misses, wrapped to `width`, and the
/// line that says what `[A]` does where a font can be activated.
pub fn misses_lines(tab: &Looks, shared: &Shared, name: &str, width: usize) -> Vec<String> {
    let used = tab.used(shared);
    let mut out = Vec::new();
    for line in missing::misses(&used, name) {
        out.extend(draw::wrap(&line, width, 6));
    }
    let n = missing::counts(&used, name).not_active;
    if n > 0 {
        out.push(String::new());
        out.push(words::activate_hint(n));
    }
    out
}

/// Where each part of the tab is drawn in `area`.
pub fn places(tab: &Looks, shared: &Shared, area: Rect) -> Places {
    match &tab.open {
        Open::Nothing => {
            let (_, rows, offset) = list_rows(tab, shared, area);
            let list = Panes::of(area).list;
            let items = (0..list.height)
                .zip(rows.iter().skip(offset))
                .filter_map(|(dy, row)| match row {
                    Row::Item(i) => Some((
                        Rect::new(list.x, list.y.saturating_add(dy), list.width, 1),
                        *i,
                    )),
                    Row::Heading(_) => None,
                })
                .collect();
            Places {
                items,
                ..Places::default()
            }
        }
        Open::Card(c) => {
            let used = tab.used(shared);
            let roles = missing::roles(&used, &c.name).len();
            let list = match &c.open {
                CardOpen::List { list, line } => {
                    list::look(shared.document().text(), &c.name).map(|look| {
                        let choices = card::choices(shared, &look, *line);
                        let rect = list_box_rect(c, list, &choices, area);
                        list.places(&choices, rect)
                    })
                }
                _ => None,
            };
            Places {
                card: match c.open {
                    CardOpen::Missing { .. } => Vec::new(),
                    _ => card_rows(area, roles),
                },
                list,
                ..Places::default()
            }
        }
        Open::Detail { .. } => Places::default(),
    }
}

/// F1's text for what is highlighted, or nothing while a name is typed.
pub fn full_text(tab: &Looks, shared: &Shared) -> Option<String> {
    let used = tab.used(shared);
    match &tab.open {
        Open::Nothing => match tab.highlighted(shared)? {
            Item::Look(name) => Some(look_text(shared, &name, &used)),
            Item::NoLooks => Some(words::NO_LOOKS_NOTE.to_owned()),
            item => resources::detail(shared, &item, &used).map(|d| d.text()),
        },
        Open::Card(c) => match &c.open {
            CardOpen::Typing(_) => None,
            CardOpen::Missing { .. } => {
                drop(used);
                Some(misses_lines(tab, shared, &c.name, usize::MAX).join("\n"))
            }
            CardOpen::List { .. } | CardOpen::Nothing => {
                let look = list::look(shared.document().text(), &c.name)?;
                match c.line() {
                    Some(line) => Some(line_text(shared, &look, line)),
                    None => {
                        let roles = missing::roles(&used, &c.name);
                        let role = roles.get(c.row.saturating_sub(Line::ALL.len()))?;
                        Some(role_text(role))
                    }
                }
            }
        },
        Open::Detail { item, .. } => resources::detail(shared, item, &used).map(|d| d.text()),
    }
}

/// A look's card in full, as F1 shows it beside the list.
fn look_text(shared: &Shared, name: &str, used: &Used) -> String {
    let Some(look) = list::look(shared.document().text(), name) else {
        return String::new();
    };
    let mut out = format!("{name} · {}\n\n", users_line(shared, name));
    for line in Line::ALL.iter().skip(1) {
        out.push_str(&format!(
            "{:<LABEL$}{}\n",
            line.label(),
            card::shown(shared, &look, *line)
        ));
    }
    out.push('\n');
    out.push_str(words::FONTS_BY_ROLE);
    out.push('\n');
    for role in missing::roles(used, name) {
        out.push_str(&role_text(&role));
        out.push('\n');
    }
    for line in missing::misses(used, name) {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// A card line in full: its value and its choices.
fn line_text(shared: &Shared, look: &Look, line: Line) -> String {
    let mut out = format!("{}: {}", line.label(), card::shown(shared, look, line));
    let c = card::choices(shared, look, line);
    if !c.choices.is_empty() {
        out.push_str("\n\n");
        out.push_str(words::ITS_CHOICES);
        for choice in &c.choices {
            out.push_str("\n  ");
            out.push_str(&choice.text);
        }
    }
    out
}

/// A role's fonts in full: each file and its state, with why it is not
/// active.
fn role_text(role: &RoleRow) -> String {
    let mut out = format!("{}: {}", role.role, role.font);
    for font in &role.files {
        out.push_str(&format!("\n  {}  {}", font.name(), font.state.words()));
        if let Some(why) = font.state.reason() {
            out.push_str(&format!("\n    {why}"));
        }
    }
    out
}

/// Find's item for `item`, at its place `index` in the list.
pub fn find_item(shared: &Shared, item: &Item, index: usize, used: &Used) -> FindItem {
    let name = item.name(shared);
    let (kind, fact) = match item {
        Item::Look(n) => {
            let look = list::look(shared.document().text(), n).unwrap_or_default();
            let fact = format!(
                "{} · {}",
                card::shown(shared, &look, Line::FontSet),
                card::shown(shared, &look, Line::Size)
            );
            let fact = match missing::mark(used, n) {
                Some(m) => format!("{fact} · {m}"),
                None => fact,
            };
            (words::KIND_LOOK, fact)
        }
        Item::FontSet(n) => (
            words::KIND_FONT_SET,
            words::used_by(
                &list::looks(shared.document().text())
                    .into_iter()
                    .filter(|l| card::font_set(shared, l).as_deref() == Some(n.as_str()))
                    .map(|l| l.name)
                    .collect::<Vec<_>>(),
            ),
        ),
        Item::Scheme(_) => (
            words::KIND_SCHEME,
            resources::detail(shared, item, used)
                .and_then(|d| d.rows.get(1).map(DetailRow::words))
                .unwrap_or_default(),
        ),
        Item::Pointer(_) => (
            words::KIND_POINTER,
            resources::detail(shared, item, used)
                .and_then(|d| {
                    d.rows
                        .iter()
                        .find(|r| r.words().starts_with(words::STATE))
                        .map(DetailRow::words)
                })
                .unwrap_or_default(),
        ),
        Item::NoLooks => (words::KIND_LOOK, String::new()),
    };
    FindItem {
        name,
        description: Some(kind.to_owned()),
        place: words::LOOKS.to_owned(),
        fact: canvas::fit(fact.trim(), 60),
        index,
    }
}
