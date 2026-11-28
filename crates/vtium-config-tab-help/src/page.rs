//! The Help tab as it is drawn: the topics at the left, a rule, and the
//! highlighted topic at the right, its name on the first row and its page
//! under it. The page is the topic's entries laid out for the pane's
//! width, which scrolls by a window where it is longer than the pane.
//!
//! [`Layout`] works out where each part goes in the viewer's Rect, so a
//! click is read against the places the view draws. [`lay`] turns a
//! topic's entries into lines, and [`draw`] writes the whole tab into the
//! buffer.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: the topics at the
//!   left and the highlighted topic at the right, scrolled by PgUp and
//!   PgDn where it is longer than the pane.
//! - ADR-1206 D4: the view draws from the state and changes nothing, and
//!   update reads the same places the view draws.
//! - ADR-1206 D4, the color roles: every colored element is drawn through
//!   a named role.
//! - ADR-1206 D10: every screen is usable at 80×25.

use std::collections::BTreeSet;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style as CellStyle;
use vtium_config_widgets::canvas::{cells, wrap};
use vtium_config_widgets::color::sgr_cells;
use vtium_config_widgets::roles::Role;

use crate::topics::{Entry, TOPICS};

/// The width of the topics' column, the rule included, on a viewer at
/// least [`WIDE`] cells wide.
const LEFT_WIDE: u16 = 21;

/// The width of the topics' column on a narrower viewer, such as 80×25's.
const LEFT_NARROW: u16 = 18;

/// The narrowest viewer that takes the wide column of topics.
const WIDE: u16 = 90;

/// The widest a key column grows. Keys wider than this push what they do
/// onto the lines below.
const KEYS_MAX: usize = 16;

/// The widest a command's column grows.
const SYNOPSIS_MAX: usize = 24;

/// The blanks before a key, a command or a row of the grid.
const INDENT: usize = 2;

/// The rows above the page: the topic's name, then a blank row.
const HEAD_ROWS: u16 = 2;

/// A run of text and the look it is drawn in.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub look: Look,
}

/// How a run is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    /// Through a color role.
    Role(Role),
    /// In the style the logo's own SGR sets: the logo is a picture, as
    /// the console's picture is, and keeps its own colors.
    Own(CellStyle),
}

/// One line of a page.
pub type Line = Vec<Run>;

/// A topic laid out for a width: its lines, and the first line of each
/// entry, so a step by a window keeps an entry whole where it can.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub lines: Vec<Line>,
    pub starts: Vec<usize>,
}

impl Page {
    /// Adds `lines` as one entry.
    fn entry(&mut self, lines: Vec<Line>) {
        if lines.is_empty() {
            return;
        }
        self.starts.push(self.lines.len());
        self.lines.extend(lines);
    }
}

/// The lines of one topic that the `?` search keeps: each entry that
/// matches, and each cell of a grid that matches, by the entry's index
/// and the cell's.
pub type Kept = BTreeSet<(usize, Option<usize>)>;

/// A run of `text` through `role`.
fn run(text: impl Into<String>, role: Role) -> Run {
    Run {
        text: text.into(),
        look: Look::Role(role),
    }
}

/// `text` wrapped to `width`, each line after `indent` blanks.
fn wrapped(text: &str, indent: usize, width: usize, role: Role) -> Vec<Line> {
    wrap(text, width.saturating_sub(indent).max(1), usize::MAX)
        .into_iter()
        .map(|l| vec![run(format!("{:indent$}{l}", ""), role)])
        .collect()
}

/// A key or a command in its column, then what it does wrapped under the
/// column. A key wider than the column takes the lines it needs of its
/// own, wrapped between its words where it is wider than the page.
fn columned(keys: &str, keys_role: Role, does: &str, column: usize, width: usize) -> Vec<Line> {
    let indent = INDENT.saturating_add(column).saturating_add(2);
    let mut does = wrap(does, width.saturating_sub(indent).max(1), usize::MAX).into_iter();
    let mut out = Vec::new();
    let pad = " ".repeat(INDENT);
    if cells(keys) > column {
        for k in wrap(keys, width.saturating_sub(INDENT).max(1), usize::MAX) {
            out.push(vec![run(pad.clone(), Role::Row), run(k, keys_role)]);
        }
    } else {
        let gap = " ".repeat(column.saturating_sub(cells(keys)).saturating_add(2));
        let first = does.next().unwrap_or_default();
        out.push(vec![
            run(pad, Role::Row),
            run(keys, keys_role),
            run(format!("{gap}{first}"), Role::Row),
        ]);
    }
    out.extend(does.map(|l| vec![run(format!("{:indent$}{l}", ""), Role::Row)]));
    out
}

/// The grid of cells, as lines of `width`: each key in its bracket and
/// each cell's label after its keys, cells two blanks apart. A cell that
/// does not fit at the end of a line starts the next, and a cell wider
/// than a whole line breaks between its words.
fn grid(cells_of: &[&crate::topics::Cell], width: usize) -> Vec<Line> {
    let room = width.saturating_sub(INDENT).max(1);
    let mut out: Vec<Line> = Vec::new();
    let mut line: Line = Vec::new();
    let mut used = 0usize;
    for cell in cells_of {
        let words: Vec<(&str, Role)> = cell
            .keys
            .iter()
            .map(|k| (k.as_str(), Role::BarKey))
            .chain(std::iter::once((cell.label.as_str(), Role::Row)))
            .collect();
        let whole = words
            .iter()
            .map(|(w, _)| cells(w))
            .sum::<usize>()
            .saturating_add(words.len().saturating_sub(1));
        if used > 0 && used.saturating_add(2).saturating_add(whole) > room {
            out.push(std::mem::take(&mut line));
            used = 0;
        }
        for (i, (word, role)) in words.into_iter().enumerate() {
            let gap = match (used, i) {
                (0, _) => 0,
                (_, 0) => 2,
                _ => 1,
            };
            if used > 0 && used.saturating_add(gap).saturating_add(cells(word)) > room {
                out.push(std::mem::take(&mut line));
                used = 0;
            }
            if used == 0 {
                line.push(run(" ".repeat(INDENT), Role::Row));
            } else {
                line.push(run(" ".repeat(gap), Role::Row));
                used = used.saturating_add(gap);
            }
            line.push(run(word, role));
            used = used.saturating_add(cells(word));
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// The logo's row as runs in the styles its SGR sets, or nothing where it
/// is wider than `width` or no color is drawn.
fn art(text: &str, width: usize, color: bool) -> Vec<Line> {
    let styled = sgr_cells(text);
    if !color || styled.len() > width {
        return Vec::new();
    }
    let mut line: Line = Vec::new();
    for (c, style) in styled {
        let c = if c.is_control() { '?' } else { c };
        match line.last_mut() {
            Some(Run {
                text,
                look: Look::Own(s),
            }) if *s == style => text.push(c),
            _ => line.push(Run {
                text: c.to_string(),
                look: Look::Own(style),
            }),
        }
    }
    vec![line]
}

/// The column every key of `entries` lines up in: the widest keys that
/// fit [`KEYS_MAX`].
fn key_column(entries: &[&Entry]) -> usize {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Key { keys, .. } => Some(cells(&keys.join(" "))),
            _ => None,
        })
        .filter(|w| *w <= KEYS_MAX)
        .max()
        .unwrap_or(0)
}

/// The column every command of `entries` lines up in.
fn command_column(entries: &[&Entry]) -> usize {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Command { synopsis, .. } => Some(cells(synopsis)),
            _ => None,
        })
        .filter(|w| *w <= SYNOPSIS_MAX)
        .max()
        .unwrap_or(0)
}

/// A topic's `entries` laid out for `width`. With `kept`, only the lines
/// the search keeps: the entries and the cells of a grid that match, with
/// no heading, blank line or logo. `color` says whether the logo shows.
pub fn lay(entries: &[Entry], kept: Option<&Kept>, width: usize, color: bool) -> Page {
    let shown: Vec<(usize, &Entry)> = entries
        .iter()
        .enumerate()
        .filter(|(i, e)| match kept {
            None => true,
            Some(k) => match e {
                Entry::Grid(_) => k.iter().any(|(entry, _)| entry == i),
                _ => k.contains(&(*i, None)),
            },
        })
        .collect();
    let all: Vec<&Entry> = shown.iter().map(|(_, e)| *e).collect();
    let keys = key_column(&all);
    let synopsis = command_column(&all);
    let mut page = Page::default();
    for (i, e) in shown {
        let lines = match e {
            Entry::Heading(h) => vec![vec![run(*h, Role::Heading)]],
            Entry::Text(t) => wrapped(t, 0, width, Role::Row),
            Entry::Key { keys: k, means, .. } => {
                columned(&k.join(" "), Role::BarKey, means, keys, width)
            }
            Entry::Command { synopsis: s, does } => columned(s, Role::Row, does, synopsis, width),
            Entry::Grid(cells_of) => {
                let cells_of: Vec<&crate::topics::Cell> = cells_of
                    .iter()
                    .enumerate()
                    .filter(|(c, _)| kept.is_none_or(|k| k.contains(&(i, Some(*c)))))
                    .map(|(_, c)| c)
                    .collect();
                grid(&cells_of, width)
            }
            Entry::Art(a) => art(a, width, color),
            Entry::Blank => vec![Vec::new()],
        };
        page.entry(lines);
    }
    page
}

/// Where each part of the tab goes in the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The viewer the tab draws in.
    pub area: Rect,
    /// The column of topics, the rule excluded.
    pub topics: Rect,
    /// The rule between the topics and the page.
    pub rule: Rect,
    /// The topic's name, at the top of the right pane.
    pub head: Rect,
    /// The page's window, under the topic's name.
    pub page: Rect,
}

impl Layout {
    /// The tab's layout in the viewer `area`.
    pub fn of(area: Rect) -> Layout {
        let left = if area.width >= WIDE {
            LEFT_WIDE
        } else {
            LEFT_NARROW
        }
        .min(area.width);
        let right_x = area.x.saturating_add(left).saturating_add(2);
        // one blank column at the right edge, as at the left
        let right_w = area.right().saturating_sub(right_x).saturating_sub(1);
        Layout {
            area,
            topics: Rect::new(area.x, area.y, left, area.height),
            rule: Rect::new(area.x.saturating_add(left), area.y, 1, area.height),
            head: Rect::new(right_x, area.y, right_w, area.height.min(1)),
            page: Rect::new(
                right_x,
                area.y.saturating_add(HEAD_ROWS),
                right_w,
                area.height.saturating_sub(HEAD_ROWS),
            ),
        }
    }

    /// The rows of the page's window.
    pub fn window(&self) -> usize {
        usize::from(self.page.height)
    }

    /// The place of the `n`th topic the column lists, or `None` where the
    /// column has no room for it. The topics start on the column's third
    /// row, under the word Help and a blank row.
    pub fn topic(&self, n: usize) -> Option<Rect> {
        let y = self
            .topics
            .y
            .saturating_add(HEAD_ROWS)
            .saturating_add(u16::try_from(n).ok()?);
        (y < self.topics.bottom()).then(|| {
            Rect::new(
                self.topics.x.saturating_add(1),
                y,
                self.topics.width.saturating_sub(1),
                1,
            )
        })
    }
}

/// What the tab shows now, which [`draw`] draws.
pub struct View<'a> {
    /// The topics the column lists, by their index in [`TOPICS`]: every
    /// topic, or those with a line the search keeps.
    pub listed: &'a [usize],
    /// The highlighted topic, which the right pane shows.
    pub topic: Option<usize>,
    /// The lines the search keeps of each listed topic, counted.
    pub counts: Option<&'a [usize]>,
    /// The highlighted topic laid out for the pane.
    pub page: &'a Page,
    /// The page's first line in sight.
    pub top: usize,
    pub color: bool,
}

/// The name the tab's column of topics is headed with.
pub const HEADING: &str = "Help";

/// Writes `runs` from `x` on row `y`, in at most `width` cells.
fn put(buf: &mut Buffer, x: u16, y: u16, width: u16, runs: &[Run], color: bool) {
    let mut x = x;
    let end = x.saturating_add(width).min(buf.area.right());
    if y < buf.area.top() || y >= buf.area.bottom() {
        return;
    }
    for r in runs {
        if x >= end {
            break;
        }
        let style = match r.look {
            Look::Role(role) => role.style(color).cell(),
            Look::Own(s) => s,
        };
        let (nx, _) = buf.set_stringn(x, y, &r.text, usize::from(end.saturating_sub(x)), style);
        x = nx;
    }
}

/// The topic's name, then, while the search filters, how many lines
/// match, or, where the page is longer than its window, which lines show.
fn head(view: &View<'_>, window: usize) -> Line {
    let Some(topic) = view.topic else {
        return vec![run("No topic matches", Role::Dim)];
    };
    let name = TOPICS.get(topic).map_or("", |t| t.name);
    let mut out = vec![run(name, Role::Heading)];
    if let Some(n) = view.counts.and_then(|c| {
        view.listed
            .iter()
            .position(|t| *t == topic)
            .and_then(|i| c.get(i))
    }) {
        let lines = if *n == 1 {
            "line matches"
        } else {
            "lines match"
        };
        out.push(run(format!("  {n} {lines}"), Role::Dim));
    }
    let len = view.page.lines.len();
    if len > window {
        let first = view.top.saturating_add(1);
        let last = view.top.saturating_add(window).min(len);
        out.push(run(
            format!("  lines {first} to {last} of {len}  "),
            Role::Dim,
        ));
        out.push(run("[PgUp] [PgDn]", Role::BarKey));
    }
    out
}

/// Draws the tab into the viewer `layout.area` of `buf`.
pub fn draw(buf: &mut Buffer, layout: &Layout, view: &View<'_>) {
    let color = view.color;
    let t = layout.topics;
    put(
        buf,
        t.x.saturating_add(1),
        t.y,
        t.width,
        &[run(HEADING, Role::Heading)],
        color,
    );
    for (n, topic) in view.listed.iter().enumerate() {
        let Some(place) = layout.topic(n) else {
            break;
        };
        let name = TOPICS.get(*topic).map_or("", |t| t.name);
        let count = view
            .counts
            .and_then(|c| c.get(n))
            .map(|c| c.to_string())
            .unwrap_or_default();
        let width = usize::from(place.width);
        let text = format!(
            " {name}{:>pad$} ",
            count,
            pad = width.saturating_sub(cells(name)).saturating_sub(2)
        );
        let role = if view.topic == Some(*topic) {
            Role::RowHighlight
        } else {
            Role::Row
        };
        put(
            buf,
            place.x,
            place.y,
            place.width,
            &[run(text, role)],
            color,
        );
    }
    for y in layout.rule.top()..layout.rule.bottom() {
        put(buf, layout.rule.x, y, 1, &[run("│", Role::Row)], color);
    }
    let window = layout.window();
    put(
        buf,
        layout.head.x,
        layout.head.y,
        layout.head.width,
        &head(view, window),
        color,
    );
    let p = layout.page;
    for (row, line) in view
        .page
        .lines
        .iter()
        .skip(view.top)
        .take(window)
        .enumerate()
    {
        let y = p.y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
        put(buf, p.x, y, p.width, line, color);
    }
}

/// The page's top after a step of a window from `top`, up for PgUp and
/// down for PgDn. The step keeps each entry whole on one screen: PgDn
/// starts the next window at the entry the window cuts or that starts
/// below it, and PgUp starts the window at the first entry that fits whole
/// above the old top. Only an entry taller than the window is cut, and
/// then the step is a whole window. The top never passes the last window.
pub fn step(page: &Page, top: usize, window: usize, down: bool) -> usize {
    let last = page.lines.len().saturating_sub(window);
    let window = window.max(1);
    let at = top.min(last);
    let starts = &page.starts;
    let to = if down {
        let below = at.saturating_add(window);
        starts
            .iter()
            .copied()
            .rev()
            .find(|s| *s > at && *s <= below)
            .unwrap_or(below)
    } else {
        let above = at.saturating_sub(window);
        starts
            .iter()
            .copied()
            .find(|s| (above..at).contains(s))
            .unwrap_or(above)
    };
    to.min(last)
}
