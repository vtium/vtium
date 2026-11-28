//! The view: the frame of every screen around the open tab, the search row
//! where one is open, the sheets open over the tab, the message on the
//! viewer's last row, and an open menu over everything. [`layout`] is the one pure function that places
//! it all, which [`draw`] draws from and which update reads where a click
//! lands, so a click lands on what it hits.
//!
//! References:
//! - ADR-1206 D4, "One frame for every screen": one layout function
//!   computes the frame, hands the viewer to the open tab, and returns
//!   the places a click hits.
//! - ADR-1206 D2: the view takes the model read-only.
//! - ADR-1403 D11, "The frame", "The action bar": the top row, the
//!   viewer, the message's row and the bar, whose Undo and Redo show only
//!   while there is a step.

use ratatui_core::layout::{Position, Rect};
use ratatui_core::terminal::Frame;
use vtium_config_core::ui::Action;
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::{
    Bar, Count, Facts, FrameLayout, Message, Options, TabName, TopRow,
};
use vtium_config_widgets::menu::{self, MenuLayout, Open as Drawn, Opens, Pull};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search;

use crate::app::{self, App, MIN_SIZE};
use crate::menus::{self, Choice, Line, Then};
use crate::nav::menus::Which;

/// The screen as the view draws it and update reads it.
pub struct Layout {
    /// The node the person is on.
    pub node: &'static str,
    /// The action bar's buttons.
    pub bar: Bar,
    /// The frame's regions and the places a click hits.
    pub frame: FrameLayout,
    /// The lines of More's menu: the buttons the bar has no room for.
    pub more: Vec<Line>,
    /// The open menu: its lines, its submenu's lines, and where it is
    /// drawn.
    pub menu: Option<MenuShown>,
}

/// An open menu as it is drawn.
pub struct MenuShown {
    pub lines: Vec<Line>,
    pub sub: Vec<Line>,
    pub layout: MenuLayout,
}

/// The layout of the screen for `app`.
pub fn layout(app: &App) -> Layout {
    let size = app.size();
    let area = Rect::new(0, 0, size.cols, size.rows);
    let nav = app.nav();
    let shared = app.shared();
    let node = app.node();
    let tab_node = nav.tab_node(app.tabs());
    // More's menu opens over the bar of the screen under it, and lists
    // what that bar has no room for
    let bar_node = match nav.menu() {
        Some(m) if m.which == Which::More => nav.under_menu(app.tabs(), size.too_small()),
        _ => node,
    };
    let bar = bar_of(app, bar_node, tab_node);
    let names: Vec<TabName<'_>> = app
        .tabs()
        .iter()
        .map(|t| TabName {
            name: t.name(),
            letter: t.letter(),
        })
        .collect();
    let open_menu = nav.menu().and_then(|m| match m.which {
        Which::File => Some(vtium_config_widgets::frame::MenuName::File),
        Which::Actions => Some(vtium_config_widgets::frame::MenuName::Actions),
        Which::More => None,
    });
    let document = shared.document();
    let top = TopRow {
        tabs: &names,
        open_tab: nav.tab(),
        open_menu,
        facts: Facts {
            name: document.name(),
            mode: None,
            not_saved: Count::opening(document.count()),
            problems: Count::opening(crate::sheets::problems::gather(shared).len()),
        },
    };
    // Find takes the rows of its matches and its row; the search row one
    let find_rows = nav.sheets().last().map_or(0, |s| s.search_rows());
    let options = Options {
        search_rows: find_rows.max(u16::from(nav.search().is_some())),
        message: shared.message.is_some() || nav.question().is_some(),
    };
    let frame = FrameLayout::new(area, options, &top, &bar);
    let places = frame.places();
    let more: Vec<Line> = places
        .hidden
        .iter()
        .filter_map(|&i| bar.buttons.get(i))
        .zip(bar.more_menu(&places.hidden))
        .filter_map(|(button, entry)| match entry {
            menu::Entry::Item(item) => Some(Line::Choice(Choice {
                item,
                then: Then::Act(Action::of(button.action?)),
            })),
            menu::Entry::Separator => None,
        })
        .collect();
    let menu = nav.menu().map(|open| {
        let tab = app.tabs().get(nav.tab());
        let lines = app::menu_lines(shared, tab, tab_node, open.which, &more);
        let cx = menus::Context {
            shared,
            node: tab_node,
        };
        let sub = open
            .sub
            .map(|s| menus::submenu(s.which, &cx))
            .unwrap_or_default();
        let menu_area = places.menu_area();
        let (at, opens) = match open.which {
            Which::File => (Position::new(places.menus[0].x, menu_area.y), Opens::Down),
            Which::Actions => (Position::new(places.menus[1].x, menu_area.y), Opens::Down),
            Which::More => (
                Position::new(places.more.map_or(0, |m| m.x), places.regions.bottom_rule.y),
                Opens::Up,
            ),
        };
        let entries = menus::entries(&lines);
        let sub_entries = menus::entries(&sub);
        let offset = shown_from(menu_area, at, opens, entries.len(), open.highlight);
        let pull = Pull {
            entries: &entries,
            highlight: open.highlight,
            offset,
        };
        let sub_pull = open.sub.map(|s| {
            (
                s.item,
                Pull {
                    entries: &sub_entries,
                    highlight: s.highlight,
                    offset: 0,
                },
            )
        });
        let layout = menu::layout(
            menu_area,
            &Drawn {
                at,
                opens,
                menu: pull,
                sub: sub_pull,
            },
        );
        MenuShown { lines, sub, layout }
    });
    Layout {
        node,
        bar,
        frame,
        more,
        menu,
    }
}

/// Draws the screen for `app` into `frame`.
pub fn draw(app: &App, frame: &mut Frame<'_>) {
    let size = app.size();
    if size.too_small() {
        too_small(app, frame);
        return;
    }
    let shown = layout(app);
    let regions = *shown.frame.regions();
    let nav = app.nav();
    if let Some(tab) = app.tabs().get(nav.tab()) {
        tab.draw(app.shared(), frame, regions.viewer);
    }
    let message = message(app);
    let color = app.color();
    let buf = frame.buffer_mut();
    shown.frame.draw(buf, message.as_ref(), color);
    let find = nav.sheets().last().is_some_and(|s| s.search().is_some());
    if let (Some(search), Some(area), false) = (nav.search(), regions.search, find) {
        search::row::draw(search, buf, area, color);
    }
    for sheet in nav.sheets() {
        sheet.draw(buf, &sheet.place(&regions), color);
    }
    if let Some(menu) = &shown.menu {
        menu.layout.draw(buf, color);
    }
}

/// The message on the viewer's last row: the question while one is open,
/// or the message line's.
fn message(app: &App) -> Option<Message> {
    if let Some(q) = app.nav().question() {
        return Some(Message {
            text: q.text.clone(),
            role: Role::Heading,
        });
    }
    app.shared().message.as_ref().map(|m| Message {
        text: m.line.clone(),
        role: Role::Row,
    })
}

/// The screen below the minimum size: one line that says how large the
/// screen must be.
fn too_small(app: &App, frame: &mut Frame<'_>) {
    let size = app.size();
    let line = format!(
        "vtium-config needs {}×{}; this screen is {}×{}. [q] Quit",
        MIN_SIZE.0, MIN_SIZE.1, size.cols, size.rows
    );
    let style = Role::Row.style(app.color()).cell();
    let area = frame.area();
    frame
        .buffer_mut()
        .set_stringn(0, 0, &line, usize::from(area.width), style);
}

/// The action bar on `node`: the open tab's own bar while nothing of the
/// shell is open over it, otherwise the node's buttons, with Undo only
/// while there is a step to undo and Redo only while there is one to
/// redo.
fn bar_of(app: &App, node: &'static str, tab_node: &'static str) -> Bar {
    // a sheet on top names its own buttons
    if let Some(sheet) = app.nav().sheets().last().filter(|s| s.node() == node) {
        return sheet.bar();
    }
    let rows = match app.tabs().get(app.nav().tab()) {
        Some(tab) if node == tab_node => tab.bar(),
        _ => contract::buttons(node),
    };
    let history = app.shared().documents.history();
    let (undo, redo) = (history.to_undo().is_some(), history.to_redo().is_some());
    Bar::from_rows(rows.into_iter().filter(|r| {
        let id = r.id;
        !(id.starts_with("ctrl-z.") && !undo || id.starts_with("ctrl-y.") && !redo)
    }))
}

/// The first entry a menu of `len` entries shows so that `highlight`
/// stays in view.
fn shown_from(
    area: Rect,
    at: Position,
    opens: Opens,
    len: usize,
    highlight: Option<usize>,
) -> usize {
    let shown = menu::rows_shown(area, at, opens, len, 0).max(1);
    match highlight {
        Some(h) if h >= shown => h + 1 - shown,
        _ => 0,
    }
}
