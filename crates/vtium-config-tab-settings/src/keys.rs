//! The right pane, which holds the highlighted section's keys in their
//! headed groups, each key with its value and its mark, and what the tab
//! says about one key: the one line along the foot, the whole help that F1
//! opens, and the item Find lists.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Settings bullet: each key with
//!   its value and whether it is changed, and its one-line description at
//!   the foot.
//! - ADR-1403 D11, "F1, help for what is highlighted": F1 on a key opens
//!   its whole help, with its default and its value now.
//! - ADR-1206 D4: Find asks each tab for its items, each with its name,
//!   its description, where it lives and one fact.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_core::document::Document;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::{Search, matcher};
use vtium_config_widgets::tab::FindItem;
use vtium_settings::keys::{Default as KeyDefault, Key, Kind};

use crate::sections::{self, FIRST_ROW, Table, put};
use crate::words;

/// One row of the right pane: a group's title, or a key by its number in
/// the flat list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Group(&'static str),
    Key(usize),
}

/// The widest a key's name column gets, the blanks after it included.
const NAME_MAX: usize = 27;

/// The value column's width where the pane has room for more.
const VALUE_MAX: usize = 24;

/// The mark column's width: `changed` and `default` are seven letters.
const MARK: usize = 7;

/// The rows of the section numbered `section`: each group's title, then
/// its keys. Where `filter` is open or kept, only the keys it matches,
/// and only the groups that keep a key.
pub fn lines(table: &Table, section: usize, filter: Option<&Search>) -> Vec<Line> {
    let Some(s) = table.section(section) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut group = None;
    for at in s.keys.clone() {
        let Some(e) = table.entry(at) else { continue };
        if filter.is_some_and(|f| !f.is_match(at)) {
            continue;
        }
        if group != Some(e.group) {
            group = Some(e.group);
            out.extend(s.groups.get(e.group).map(|g| Line::Group(g)));
        }
        out.push(Line::Key(at));
    }
    out
}

/// How many rows of keys the right pane in `rect` shows at once.
pub fn rows(rect: Rect) -> usize {
    usize::from(rect.height.saturating_sub(FIRST_ROW))
}

/// The first of `lines` to show in `rows` rows, from `offset`, so that the
/// highlighted key shows, with its group's title where the key is the
/// group's first.
pub fn settled(lines: &[Line], highlight: Option<usize>, offset: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    let mut offset = offset;
    if let Some(at) = highlight.and_then(|h| lines.iter().position(|l| *l == Line::Key(h))) {
        let top = match at.checked_sub(1).and_then(|p| lines.get(p)) {
            Some(Line::Group(_)) => at.saturating_sub(1),
            _ => at,
        };
        if top < offset {
            offset = top;
        } else if at >= offset.saturating_add(rows) {
            offset = at.saturating_add(1).saturating_sub(rows);
        }
    }
    offset.min(lines.len().saturating_sub(rows))
}

/// What the right pane shows, and where.
pub struct Pane<'a> {
    pub table: &'a Table,
    pub document: &'a Document,
    pub section: usize,
    pub lines: &'a [Line],
    pub offset: usize,
    /// The highlighted key, where the keys have the focus.
    pub highlight: Option<usize>,
    /// The value being typed for the highlighted key.
    pub typing: Option<&'a str>,
    /// How many keys the section holds, and how many the search keeps.
    pub matching: Option<usize>,
}

/// The columns of a key's row: its name, its value and its mark, each
/// from the pane's left edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub name: usize,
    pub value: usize,
}

impl Pane<'_> {
    /// The columns of the section's keys in `rect`: the name column as
    /// wide as its longest name, and the value column as wide as the rest
    /// allows, at most 24 cells.
    pub fn columns(&self, rect: Rect) -> Columns {
        let longest = self
            .table
            .section(self.section)
            .map(|s| {
                s.keys
                    .clone()
                    .filter_map(|at| self.table.entry(at))
                    .map(|e| e.key.name.len())
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let name = longest.saturating_add(2).min(NAME_MAX);
        let room = usize::from(rect.width)
            .saturating_sub(2)
            .saturating_sub(name)
            .saturating_sub(MARK);
        Columns {
            name,
            value: room.min(VALUE_MAX),
        }
    }

    /// The row on screen of each key shown, with its number.
    pub fn places(&self, rect: Rect) -> Vec<(Rect, usize)> {
        let shown = rows(rect);
        self.lines
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(shown)
            .filter_map(|(i, l)| match l {
                Line::Key(at) => {
                    let dy = u16::try_from(i.saturating_sub(self.offset)).ok()?;
                    let y = rect.y.saturating_add(FIRST_ROW).saturating_add(dy);
                    Some((highlight_rect(rect, y), *at))
                }
                Line::Group(_) => None,
            })
            .collect()
    }

    /// Draws the pane in `rect`, and returns where the cursor stands
    /// while a value is typed.
    pub fn draw(&self, buf: &mut Buffer, rect: Rect, color: bool) -> Option<Position> {
        let heading = Role::Heading.style(color).cell();
        let dim = Role::Dim.style(color).cell();
        let row = Role::Row.style(color).cell();
        let section = self.table.section(self.section)?;
        let count = match self.matching {
            Some(n) => words::matching_count(n, section.keys.len()),
            None => words::settings_count(section.keys.len()),
        };
        let count_x = rect
            .right()
            .saturating_sub(u16::try_from(canvas::cells(&count)).unwrap_or(0));
        let title_cells = usize::from(count_x.saturating_sub(rect.x)).saturating_sub(2);
        put(
            buf,
            rect.x,
            rect.y,
            &canvas::fit(section.title, title_cells),
            heading,
            rect,
        );
        put(buf, count_x, rect.y, &count, dim, rect);
        if self.lines.is_empty() {
            let y = rect.y.saturating_add(FIRST_ROW);
            put(buf, rect.x, y, words::NO_MATCH, dim, rect);
            return None;
        }
        let columns = self.columns(rect);
        let mut cursor = None;
        for (i, line) in self
            .lines
            .iter()
            .skip(self.offset)
            .take(rows(rect))
            .enumerate()
        {
            let Ok(dy) = u16::try_from(i) else { break };
            let y = rect.y.saturating_add(FIRST_ROW).saturating_add(dy);
            match line {
                Line::Group(title) => {
                    put(buf, rect.x, y, title, row, rect);
                }
                Line::Key(at) => {
                    let Some(entry) = self.table.entry(*at) else {
                        continue;
                    };
                    let lit = self.highlight == Some(*at);
                    let typing = self.typing.filter(|_| lit);
                    let (text, at_cursor) = self.key_row(entry.key, typing, columns, rect);
                    let style = if lit {
                        Role::RowHighlight.style(color).cell()
                    } else {
                        row
                    };
                    let place = highlight_rect(rect, y);
                    let line = format!(" {text}");
                    let line = canvas::fit(&line, usize::from(place.width));
                    let pad = usize::from(place.width).saturating_sub(canvas::cells(&line));
                    put(
                        buf,
                        place.x,
                        y,
                        &format!("{line}{}", " ".repeat(pad)),
                        style,
                        place,
                    );
                    if !lit && let Some(mark) = self.mark_of(entry.key) {
                        let x = rect
                            .x
                            .saturating_add(column(2 + columns.name + columns.value));
                        let style = if mark == words::CHANGED { heading } else { dim };
                        put(buf, x, y, mark, style, rect);
                    }
                    if let Some(c) = at_cursor {
                        cursor = Some(Position::new(c.min(rect.right().saturating_sub(1)), y));
                    }
                }
            }
        }
        cursor
    }

    /// A key's row, from the pane's left edge: its name, its value or what
    /// is typed, and its mark; and the column of the cursor while a value
    /// is typed.
    fn key_row(
        &self,
        key: &'static Key,
        typing: Option<&str>,
        columns: Columns,
        rect: Rect,
    ) -> (String, Option<u16>) {
        let name = canvas::fit(key.name, columns.name.saturating_sub(1));
        let name = format!("{name:<w$}", w = columns.name);
        match typing {
            Some(typed) => {
                // what is typed may run past the value column, to the
                // pane's edge, cut at its start where it is longer
                let room = usize::from(rect.width)
                    .saturating_sub(3)
                    .saturating_sub(columns.name);
                let shown = tail(typed, room);
                let at = rect
                    .x
                    .saturating_add(column(2 + columns.name + canvas::cells(&shown)));
                (format!("  {name}{shown}_"), Some(at))
            }
            None => {
                let value = sections::value(key, self.document);
                let text = canvas::fit(&value.text, columns.value.saturating_sub(1));
                let text = format!("{text:<w$}", w = columns.value);
                let mark = value.mark().unwrap_or("");
                (format!("  {name}{text}{mark}"), None)
            }
        }
    }

    /// The mark beside the key's value, drawn in its own role.
    fn mark_of(&self, key: &'static Key) -> Option<&'static str> {
        sections::value(key, self.document).mark()
    }
}

/// The highlight of a key's row on row `y`: from the blank column after
/// the rule to the pane's right edge.
fn highlight_rect(rect: Rect, y: u16) -> Rect {
    Rect::new(rect.x.saturating_sub(1), y, rect.width.saturating_add(1), 1)
}

/// A count of cells as a column offset.
fn column(cells: usize) -> u16 {
    u16::try_from(cells).unwrap_or(u16::MAX)
}

/// The end of `text` that fits in `room` cells, after `…` where the whole
/// does not.
fn tail(text: &str, room: usize) -> String {
    if canvas::cells(text) <= room {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut kept = String::new();
    let mut used = 1;
    for c in chars.iter().rev() {
        let w = canvas::cells(&c.to_string());
        if used + w > room {
            break;
        }
        used += w;
        kept.insert(0, *c);
    }
    format!("…{kept}")
}

/// The foot's line for the key numbered `at` in `cells` cells: its name
/// and first sentence, then, while the search matched the key by a word
/// its description holds further on, those words quoted. Where both do not
/// fit, the quote takes the sentence's place, since it says why the key
/// matched.
pub fn foot(table: &Table, at: usize, filter: Option<&Search>, cells: usize) -> String {
    let Some(entry) = table.entry(at) else {
        return String::new();
    };
    let key = entry.key;
    let first = key.paragraphs().next().unwrap_or("");
    let line = words::about_key(key.name, first);
    let Some(quote) = filter.and_then(|f| excerpt(key, f.entry())) else {
        return line;
    };
    let full = line + &words::quoted(&quote);
    if canvas::cells(&full) <= cells {
        full
    } else {
        format!("{}{}", key.name, words::quoted(&quote))
    }
}

/// The words of `key`'s description around the first word of `entry`'s
/// pattern that only the description's later paragraphs hold.
fn excerpt(key: &'static Key, entry: &str) -> Option<String> {
    let pattern = matcher::pattern(entry);
    let what = key.what_line();
    let first = key.paragraphs().next().unwrap_or("").to_lowercase();
    let lower = what.to_lowercase();
    // the excerpt is cut from the text itself, so it needs the same byte
    // offsets in both cases
    if lower.len() != what.len() {
        return None;
    }
    let word = pattern
        .words()
        .iter()
        .map(|w| w.text.as_str())
        .find(|w| !w.is_empty() && !first.contains(w) && !key.name.contains(w))?;
    let at = lower.find(word)?;
    let end = what
        .get(at..)?
        .find(|c: char| c.is_whitespace())
        .map_or(what.len(), |e| at + e);
    let before = what.get(..at)?;
    let start = before
        .char_indices()
        .rev()
        .filter(|(_, c)| c.is_whitespace())
        .nth(2)
        .map_or(0, |(i, _)| i + 1);
    let quote = what.get(start..end)?.trim_end_matches(['.', ',', ';', ':']);
    Some(quote.to_owned())
}

/// The whole help of the key numbered `at`, which F1 opens: where it is,
/// every paragraph of its description, what it takes, its default and the
/// value the configuration gives it now.
pub fn full_help(table: &Table, at: usize, document: &Document) -> Option<String> {
    let entry = table.entry(at)?;
    let section = table.section(entry.section)?;
    let key = entry.key;
    let mut out = words::place(&[section.title, key.name]);
    for paragraph in key.paragraphs() {
        out.push_str("\n\n");
        out.push_str(paragraph);
    }
    out.push('\n');
    let value = sections::value(key, document);
    let rows = [
        takes(key).map(|t| (words::LABEL_TAKES, t)),
        Some((words::LABEL_DEFAULT, default_text(key))),
        Some((words::LABEL_NOW, value.line())),
    ];
    for (label, text) in rows.into_iter().flatten() {
        out.push_str(&format!("\n  {label:<11}{text}"));
    }
    Some(out)
}

/// The values a key takes, where its kind says them in a few words.
fn takes(key: &Key) -> Option<String> {
    match key.kind {
        Kind::Switch => Some("on, off".to_owned()),
        Kind::Choice(words) => Some(words.join(", ")),
        Kind::Name(names) => Some(format!("{}, or a file's name", names.join(", "))),
        Kind::Number {
            min,
            max,
            unit,
            word,
        } => Some(words::number(min, max, unit, word)),
        _ => None,
    }
}

/// A key's default as `vtium config get` prints it for a key the file
/// leaves out.
fn default_text(key: &'static Key) -> String {
    match key.default {
        KeyDefault::Unset => words::UNSET.to_owned(),
        _ => vtium_config_core::commands::value_text(key, None).0,
    }
}

/// Find's item for the key numbered `at`: its name and description, its
/// section, and its value now.
pub fn find_item(table: &Table, at: usize, document: &Document) -> Option<FindItem> {
    let entry = table.entry(at)?;
    let section = table.section(entry.section)?;
    Some(FindItem {
        name: entry.key.name.to_owned(),
        description: Some(entry.key.what_line()),
        place: words::place(&[section.title]),
        fact: sections::value(entry.key, document).line(),
        index: at,
    })
}
