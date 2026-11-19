//! Where the shell of the TUI's screen puts things, as pure functions of
//! the model that update and the view both call: whether the grid is too
//! small, the places of the tab bar, the key line's rows, the message
//! line, the window a tab's body scrolls in, the Help page's window, and
//! where a picture goes. Each tab lays its own body out, from its state,
//! the shared part and the size; [`body`] hands the open tab's body on.
//! Each function reads the model and the terminal's size and changes
//! nothing. Update reads these facts here, so it never calls the view to
//! learn them, and the view draws what these functions place.
//!
//! References:
//! - ADR-1206 D4, "Layout is a pure function": update calls layout
//!   functions and never a view, and each tab's layout belongs to the
//!   tab.

use super::canvas::wrap;
use super::listbox::{Body, LIST_MIN};
use super::model::{App, Size, View};
use super::tabs::Room;

/// The smallest grid a view is drawn in: below it, the one screen that
/// says so.
///
/// References:
/// - ADR-1201 D3, "Where it runs": the TUI needs a terminal of at least
///   80×25 cells.
/// - ADR-1403, Summary: the Flow view needs the same 80×25 cells.
pub const MIN_COLS: usize = 80;
pub const MIN_ROWS: usize = 25;

/// Whether a grid of `size` is below the minimum.
pub fn too_small(size: Size) -> bool {
    size.cols < MIN_COLS || size.rows < MIN_ROWS
}

/// The body of `app`'s view, which the open tab lays out from its own
/// state, the shared part and the terminal's size.
pub fn body(app: &App) -> Body {
    app.tab().body(&app.shared, app.size)
}

/// Where a picture goes in the open tab, row and column from 1, as the
/// tab places it.
pub fn picture_at(app: &App) -> (usize, usize) {
    app.tab().picture_at(&app.shared, app.size)
}

/// What the shell of the screen leaves the open tab: the terminal's size,
/// the rows of the key line, and the rows of the body between the rule
/// under the tab bar and the message.
pub fn room(app: &App) -> Room {
    Room {
        size: app.size,
        keys: key_rows(app).len(),
        body: list_window(app, 0, 0).0,
    }
}

/// The first row a picture takes by default, from 1: the row under the
/// rule below the tab bar.
pub const PICTURE_ROW: usize = 4;

/// The Flow view's first row of columns, from 0.
pub const FLOW_TOP: usize = 4;

/// The first column a picture takes by default, from 1: beside a list.
pub const PICTURE_COL: usize = 26;
/// One place in the tab bar: a view's name, or an action's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    View(View),
    Action(usize),
}

/// The tab bar's places, each with its first column and its width: the
/// views on the left, the current one marked, and the actions against the
/// right edge, in their short forms where the long ones do not fit.
pub fn tab_places(app: &App) -> Vec<(Tab, usize, String)> {
    let mut out = Vec::new();
    let mut x = 0usize;
    for view in View::TABS.iter() {
        let text = format!(" {} ", app.tab_of(*view).name());
        let w = text.chars().count();
        out.push((Tab::View(*view), x, text));
        x = x.saturating_add(w);
    }
    let tabs_end = x;
    let actions = |long: bool, gap: usize| -> Vec<String> {
        let list = app.tab_of(app.nav.view.tab()).actions();
        list.iter()
            .map(|(l, s)| {
                let name = if long { l } else { s };
                format!("{}{name}{}", " ".repeat(gap), " ".repeat(gap))
            })
            .collect()
    };
    let fits = |a: &[String]| {
        let w: usize = a.iter().map(|t| t.chars().count()).sum::<usize>();
        tabs_end.saturating_add(1).saturating_add(w) <= app.size.cols
    };
    let shown = [actions(true, 1), actions(false, 1), actions(false, 0)]
        .into_iter()
        .find(|a| fits(a))
        .unwrap_or_default();
    let width: usize = shown
        .iter()
        .map(|t| t.chars().count())
        .sum::<usize>()
        .saturating_add(if shown.first().is_some_and(|t| !t.starts_with(' ')) {
            shown.len().saturating_sub(1)
        } else {
            0
        });
    let mut x = app.size.cols.saturating_sub(width);
    let tight = shown.first().is_some_and(|t| !t.starts_with(' '));
    for (i, text) in shown.into_iter().enumerate() {
        let w = text.chars().count();
        out.push((Tab::Action(i), x, text));
        x = x.saturating_add(w).saturating_add(usize::from(tight));
    }
    out
}

/// The view whose name is at cell (`col`, `row`), as a click finds it.
pub fn tab_at(app: &App, col: usize, row: usize) -> Option<View> {
    if row != 0 {
        return None;
    }
    tab_places(app)
        .into_iter()
        .find_map(|(tab, x, text)| match tab {
            Tab::View(v) if (x..x.saturating_add(text.chars().count())).contains(&col) => Some(v),
            _ => None,
        })
}

/// The most rows the message line wraps to.
pub const MESSAGE_ROWS: usize = 3;

/// The key line for the state `app` is in, each key and what it does two
/// spaces apart: the page that shows of the key line that the contract
/// gives the node of the interface tree the TUI is in.
///
/// References:
/// - ADR-1403 D7, "The contract": the key lines are generated from the
///   key table and the tree.
/// - ADR-1403 D7, "The same keys in every view": the key line is one row
///   that shows its keys in pages, and `.` shows the next page.
pub fn key_line(app: &App) -> &'static str {
    match super::contract::key_pages(super::nav::node(app)) {
        Some(pages) => pages
            .get(app.nav.key_page)
            .or_else(|| pages.first())
            .copied()
            .unwrap_or_default(),
        // a node still planned keeps the key line its tab writes by hand,
        // until its work moves the node to supported
        None => app.tab().planned_keys(&app.shared),
    }
}

/// The rows the key line takes: one. Each page of a generated key line
/// fits on a row of 80 columns, the smallest the TUI draws, and so do the
/// hand-written key lines of the views whose nodes are still planned.
pub fn key_rows(app: &App) -> Vec<String> {
    vec![key_line(app).to_owned()]
}

/// Items two spaces apart, in rows of at most `width` columns. An item
/// that fits on a row stays whole: it goes onto the next row rather than
/// break. An item wider than a row wraps between its words onto rows of
/// its own, so no part of it is cut off at the edge.
pub fn item_rows(items: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for item in items.split("  ").filter(|i| !i.is_empty()) {
        let len = item.chars().count();
        match rows.last_mut() {
            Some(row) if row.chars().count().saturating_add(2).saturating_add(len) <= width => {
                row.push_str("  ");
                row.push_str(item);
            }
            _ if len > width => rows.extend(wrap(item, width, usize::MAX)),
            _ => rows.push(item.to_owned()),
        }
    }
    rows
}

/// The cells a picture may cover, columns and rows: from its place to the
/// right edge, and down to the row above the message line, so it never
/// covers the keys.
pub fn picture_area(app: &App) -> (usize, usize) {
    if app.nav.view == View::Flow {
        // the rows above the message at its longest
        let bottom = app
            .size
            .rows
            .saturating_sub(key_rows(app).len())
            .saturating_sub(MESSAGE_ROWS);
        return (
            app.size.cols.saturating_sub(2),
            bottom.saturating_sub(FLOW_TOP),
        );
    }
    // the rule, the keys, and the message at its longest
    let footer = 1usize
        .saturating_add(key_rows(app).len())
        .saturating_add(MESSAGE_ROWS);
    let (row, col) = picture_at(app);
    (
        app.size.cols.saturating_sub(col.saturating_sub(1)),
        app.size
            .rows
            .saturating_sub(footer)
            .saturating_sub(row.saturating_sub(1)),
    )
}

/// The message under a view's list, wrapped to the screen.
pub fn message_rows(app: &App) -> Vec<String> {
    wrap(
        app.shared.message.as_deref().unwrap_or(""),
        app.size.cols.saturating_sub(2),
        MESSAGE_ROWS,
    )
}

/// The rows a view's list fills, between the tab bar's rule and the
/// message, and whether the sentences under the list show. They show whole
/// while the list keeps at least `LIST_MIN` rows; otherwise they are
/// dropped and the list takes their rows.
pub fn list_window(app: &App, list: usize, tail: usize) -> (usize, bool) {
    let room = app
        .size
        .rows
        .saturating_sub(4)
        .saturating_sub(message_rows(app).len())
        .saturating_sub(key_rows(app).len());
    let show_tail =
        tail > 0 && (list.saturating_add(tail) <= room || room >= LIST_MIN.saturating_add(tail));
    let list_room = if show_tail {
        room.saturating_sub(tail)
    } else {
        room
    };
    (list_room, show_tail)
}

/// The first row of the open view's list that the window shows: the row
/// that puts the last row of the highlighted item at the window's
/// bottom, or the list's first row while the highlight fits above that.
/// An item can take several rows, so the window can start partway
/// through an item.
pub fn list_offset(app: &App) -> usize {
    let Body { list, keep, tail } = body(app);
    let (room, _) = list_window(app, list.len(), tail.len());
    keep.saturating_add(1).saturating_sub(room)
}

/// The rows of the message under the Flow view's file view and Restore's
/// list: the message, or else the line that says which font the preview
/// shows, wrapped to at most [`MESSAGE_ROWS`] rows.
pub fn flow_message_rows(app: &App) -> Vec<String> {
    app.shared
        .message
        .clone()
        .or_else(|| app.preview().preview_line())
        .map(|m| wrap(&m, app.size.cols.saturating_sub(2), MESSAGE_ROWS))
        .unwrap_or_default()
}

/// The first row of the message under the file view and Restore's list,
/// which is the row under the last one they draw on.
pub fn flow_bottom(app: &App) -> usize {
    app.size
        .rows
        .saturating_sub(key_rows(app).len())
        .saturating_sub(flow_message_rows(app).len())
}

/// Restore's window: how many entries of the installed history show
/// under its title, at least one, and the first entry it shows. The
/// window ends on the row above the message, and it starts at the entry
/// that puts the highlighted one on its last row, or at the first entry
/// while the highlighted one fits above that.
pub fn restore_window(app: &App) -> (usize, usize) {
    let room = flow_bottom(app)
        .saturating_sub(FLOW_TOP.saturating_add(1))
        .max(1);
    let first = app.flow.restore().map_or(0, |(kept, list)| {
        list.at
            .saturating_add(1)
            .saturating_sub(room)
            .min(kept.len().saturating_sub(room))
    });
    (room, first)
}

/// The Help view's page: its rows, with where each entry starts, and how
/// many of them the screen shows at once. The page's top row goes as far
/// as the row that puts the page's last row at the bottom of the screen.
pub fn help_window(app: &App) -> (super::tabs::help::Rows, usize) {
    let rows = app.help.rows(&app.shared, app.size);
    let room = list_window(app, rows.len, 0).0;
    (rows, room)
}
