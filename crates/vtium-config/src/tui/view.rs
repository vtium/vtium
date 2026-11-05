//! The TUI's screen (ADR-1201 D3, ADR-1403 D7): the state drawn on a
//! [`Canvas`], and the bytes that draw it whole, with ECMA-48's cursor
//! position, erase and SGR sequences only (D7).
//!
//! Every view has the tab bar, with the views and the actions, and the
//! status line. The Flow view then draws the base card, the columns, the
//! card Enter opens and its keys ([`super::flowdraw`]). The other views
//! draw a rule, their body, a message line, a rule and their keys; a body
//! longer than the room scrolls so the highlighted row shows. Below 80 by
//! 25 cells no view is drawn: one screen says the size the TUI needs, the
//! commands that work at any size, and the keys that leave and that undo a
//! font preview, so the user can always get out.

use super::canvas::{Canvas, Row, Span, span, wrap};
use super::color::{Style, palette};
use super::flow::Item;
use super::model::{App, DpiFrom, FontState, Live, NEW_FIELDS, SIZES, View, setting_keys};
use super::words;
use crate::specimen::Sources;

/// The smallest grid a view is drawn in (ADR-1201 D3): below it, the one
/// screen that says so.
pub const MIN_COLS: usize = 80;
pub const MIN_ROWS: usize = 25;

/// Whether `app`'s grid is below the minimum.
pub fn too_small(app: &App) -> bool {
    app.cols < MIN_COLS || app.rows < MIN_ROWS
}

fn plain(text: impl Into<String>) -> Row {
    vec![span(text, Style::PLAIN)]
}

fn marked(text: impl Into<String>) -> Row {
    vec![span(text, Style::PLAIN.reversed())]
}

/// The row's text, without its styles.
pub fn text(row: &Row) -> String {
    row.iter().map(|s| s.text.as_str()).collect()
}

/// `left` and `right` on one line of `cols`, `right` against the edge.
fn sides(left: &str, right: &str, cols: usize) -> String {
    let (l, r) = (left.chars().count(), right.chars().count());
    let gap = cols.saturating_sub(l.saturating_add(r)).max(1);
    format!("{left}{}{right}", " ".repeat(gap))
}

/// A view's body: its list, the row of the list to keep in sight, and the
/// sentences under it, which show whole or not at all.
struct Body {
    list: Vec<Row>,
    keep: usize,
    tail: Vec<Row>,
}

/// The fewest list rows a body keeps before it drops its sentences.
const LIST_MIN: usize = 5;

/// The most rows a setting's description takes under the list.
const WHAT_ROWS: usize = 4;

/// A list of `items`, each a name in a column of `width` and its note, the
/// note wrapped by words under its column where it is longer than the
/// line, and starting on the row below a name longer than the column,
/// with the item `at` highlighted; and the list's last row of that
/// item, to keep in sight.
fn list(items: &[(String, String)], at: usize, width: usize, cols: usize) -> (Vec<Row>, usize) {
    let column = width.saturating_add(4);
    let room = cols.saturating_sub(column).saturating_sub(1).max(8);
    let mut rows = Vec::new();
    let mut keep = 0;
    for (i, (name, note)) in items.iter().enumerate() {
        let mark = if i == at { '>' } else { ' ' };
        // a name longer than its column has its note on the rows below
        let long = name.chars().count() > width;
        let parts = std::iter::repeat_n(String::new(), usize::from(long)).chain(
            wrap(note, room, usize::MAX)
                .into_iter()
                .filter(|p| !(long && p.is_empty())),
        );
        for (n, part) in parts.enumerate() {
            let line = if n == 0 {
                format!(" {mark} {name:<width$} {part}")
            } else {
                format!("{:column$}{part}", "")
            };
            rows.push(if i == at { marked(line) } else { plain(line) });
            if i == at {
                keep = rows.len().saturating_sub(1);
            }
        }
    }
    (rows, keep)
}

/// `text` as rows of the body, wrapped by words to the screen's width.
fn prose(text: &str, cols: usize) -> Vec<Row> {
    wrap(text, cols.saturating_sub(2), usize::MAX)
        .into_iter()
        .map(|l| plain(format!(" {l}")))
        .collect()
}

/// `text` as a body's sentences, with a blank row above them.
fn tail(text: &str, cols: usize) -> Vec<Row> {
    std::iter::once(plain(""))
        .chain(prose(text, cols))
        .collect()
}

/// The body of `app`'s view.
fn body(app: &App) -> Body {
    let cols = app.cols;
    match app.view {
        View::Fonts => {
            let width = app
                .inputs
                .fonts
                .iter()
                .map(|f| f.name.chars().count())
                .max()
                .unwrap_or(0)
                .clamp(8, 24);
            let configured = app.configured("font-name");
            let items: Vec<(String, String)> = app
                .inputs
                .fonts
                .iter()
                .map(|f| {
                    let mut note = match (&f.state, app.sources.get(&f.name)) {
                        (FontState::Reads, Some(Sources::LeftOut(left))) if !left.is_empty() => {
                            format!("{}, {}", words::READS, words::sources_missing(left.len()))
                        }
                        (FontState::Reads, Some(Sources::DrawsNothing(_))) => {
                            words::DRAWS_NOTHING.to_owned()
                        }
                        (FontState::Reads, Some(Sources::Unknown(_))) => {
                            format!("{}, {}", words::READS, words::SOURCES_UNKNOWN)
                        }
                        (FontState::Reads, _) => words::READS.to_owned(),
                        (FontState::BuiltIn, _) => words::BUILT_IN.to_owned(),
                        (FontState::Refused(why), _) => words::does_not_read(why),
                    };
                    if app.refused.contains(&f.name) {
                        note = words::KEPT.to_owned();
                    }
                    if app.pending.removals.contains(&f.name) {
                        note = format!("{note}, {}", words::TO_REMOVE);
                    }
                    if app.changed("font-name")
                        && app.drafted("font-name").as_deref() == Some(&f.name)
                    {
                        note = format!("{note}, {}", words::TO_SAVE);
                    } else if configured.as_deref() == Some(&f.name) {
                        note = format!("{note}, {}", words::SAVED);
                    }
                    (f.name.clone(), note)
                })
                .collect();
            let (list, keep) = list(&items, app.font_at, width, cols);
            Body {
                list,
                keep,
                tail: Vec::new(),
            }
        }
        View::Looks => {
            let configured = app
                .configured("font-size")
                .and_then(|s| s.parse::<u32>().ok());
            let items: Vec<(String, String)> = std::iter::once(0)
                .chain(SIZES)
                .map(|size| {
                    let name = match size {
                        0 => words::AUTOMATIC.to_owned(),
                        n => words::points(n),
                    };
                    let drafted = app.drafted("font-size").and_then(|s| s.parse::<u32>().ok());
                    let note = if app.changed("font-size") && drafted.unwrap_or(0) == size {
                        words::TO_SAVE
                    } else if configured.unwrap_or(0) == size {
                        words::SAVED
                    } else {
                        ""
                    };
                    (name, note.to_owned())
                })
                .collect();
            let (list, keep) = list(&items, app.size_at, 10, cols);
            let summary = app
                .summary
                .as_ref()
                .filter(|(f, h, _)| {
                    app.font().is_some_and(|font| font.name == *f)
                        && *h == crate::specimen::pixel_height(app.size(), app.inputs.dpi.0)
                })
                .map(|(_, _, s)| match s {
                    Ok(s) => words::size_summary(s),
                    Err(why) => words::no_summary(why),
                });
            let sentence = if !app.graphics && app.size() > 0 {
                match summary {
                    Some(s) => format!("{} {s}", words::NO_PICTURES),
                    None => words::NO_PICTURES.to_owned(),
                }
            } else if !app.graphics {
                words::NO_PICTURES.to_owned()
            } else if app.size() == 0 {
                words::AUTOMATIC_PICTURE.to_owned()
            } else {
                let (dpi, from) = app.inputs.dpi;
                let from = match from {
                    DpiFrom::Conf => words::DPI_FROM_CONF,
                    DpiFrom::Edid => words::DPI_FROM_EDID,
                    DpiFrom::Assumed => words::DPI_ASSUMED,
                };
                words::density(dpi, from)
            };
            let mut list_rows = prose(words::LOOKS_SIZES, cols);
            list_rows.push(plain(""));
            let keep = keep.saturating_add(list_rows.len());
            list_rows.extend(list);
            Body {
                list: list_rows,
                keep,
                tail: tail(&sentence, cols),
            }
        }
        View::Settings => {
            let all = setting_keys();
            let items: Vec<(String, String)> = all
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let value = match &app.editing {
                        Some(typed) if i == app.setting_at => format!("{typed}_"),
                        _ => match app.value(key) {
                            (v, true) => words::default_value(v),
                            (v, false) => v,
                        },
                    };
                    let value = if app.changed(key.name) {
                        format!("{value}, {}", words::TO_SAVE)
                    } else {
                        value
                    };
                    (key.name.to_owned(), value)
                })
                .collect();
            // the key's column takes at most half the screen
            let width = all
                .iter()
                .map(|k| k.name.chars().count())
                .max()
                .unwrap_or(0)
                .min(24)
                .min((cols / 2).saturating_sub(4));
            let (list, keep) = list(&items, app.setting_at, width, cols);
            let hint = if app.editing.is_some() {
                words::EDITING
            } else {
                words::SETTINGS_HINT
            };
            let what = all
                .get(app.setting_at)
                .map_or(String::new(), |k| k.what_line());
            let mut tail = tail(hint, cols);
            tail.extend(
                wrap(&what, cols.saturating_sub(2), WHAT_ROWS)
                    .into_iter()
                    .map(|l| plain(format!(" {l}"))),
            );
            Body { list, keep, tail }
        }
        View::New => {
            let mut items: Vec<(String, String)> = words::NEW_FIELD_NAMES
                .iter()
                .zip(&app.new_frit.fields)
                .enumerate()
                .map(|(i, (name, text))| {
                    let cursor = if i == app.new_frit.at { "_" } else { "" };
                    ((*name).to_owned(), format!("{text}{cursor}"))
                })
                .collect();
            items.push((words::NEW_ADD.to_owned(), String::new()));
            let (list, keep) = list(&items, app.new_frit.at.min(NEW_FIELDS), 8, cols);
            Body {
                list,
                keep,
                tail: tail(words::NEW_HINT, cols),
            }
        }
        View::Help => Body {
            keep: app.help_at,
            list: help_rows(app),
            tail: Vec::new(),
        },
        // the Flow view draws on the canvas itself
        View::Flow => Body {
            list: Vec::new(),
            keep: 0,
            tail: Vec::new(),
        },
    }
}

/// The Help view's rows: on the Help page, what each view does and its
/// keys, the route card, and the commands, from the catalog the command
/// line's help draws on; on the About page, what `vtium-config --help
/// about` prints, from the same source.
pub fn help_rows(app: &App) -> Vec<Row> {
    let cols = app.cols;
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    let mut rows = Vec::new();
    if app.about {
        let mut out = Vec::new();
        crate::banner::about(&mut out);
        for line in String::from_utf8_lossy(&out).lines() {
            if line.contains('\x1b') {
                // the banner, styled by its own module; it was drawn for
                // this terminal's width, and a line wider is left out
                if super::canvas::width(line) <= cols {
                    rows.push(vec![Span {
                        text: line.to_owned(),
                        style: Style::PLAIN,
                        raw: true,
                    }]);
                }
            } else {
                rows.extend(prose(line, cols));
            }
        }
        return rows;
    }
    for (name, does, keys) in words::HELP_VIEWS {
        let mut first = true;
        for line in wrap(
            &format!("{name} {does}"),
            cols.saturating_sub(2),
            usize::MAX,
        ) {
            if first {
                let rest = line.strip_prefix(name).unwrap_or(&line).to_owned();
                rows.push(vec![
                    span(format!(" {name}"), bold),
                    span(rest, Style::PLAIN),
                ]);
                first = false;
            } else {
                rows.push(plain(format!(" {line}")));
            }
        }
        for line in wrap(keys, cols.saturating_sub(6), usize::MAX) {
            rows.push(vec![span(
                format!("     {line}"),
                Style::text(palette::GREY),
            )]);
        }
        rows.push(plain(""));
    }
    rows.extend(prose(words::HELP_ROUTE, cols));
    rows.push(plain(""));
    rows.extend(prose(words::HELP_COMMANDS, cols));
    let usage = crate::help::usage();
    let commands = usage
        .split_once("Commands:\n")
        .map(|(_, rest)| rest.split("\n\n").next().unwrap_or_default())
        .unwrap_or_default();
    // a command's line wider than the screen gives its synopsis a row of
    // its own and goes on under it
    let width = cols.saturating_sub(1);
    for line in commands.lines() {
        if line.chars().count() <= width {
            rows.push(plain(format!(" {line}")));
            continue;
        }
        let (synopsis, does) = line
            .trim_start()
            .split_once("  ")
            .map_or((line.trim(), ""), |(a, b)| (a, b.trim()));
        rows.push(plain(format!("   {synopsis}")));
        for l in wrap(does, cols.saturating_sub(8), usize::MAX) {
            if !l.is_empty() {
                rows.push(plain(format!("       {l}")));
            }
        }
    }
    rows
}

/// Where a picture goes, row and column from 1: in the Looks view beside
/// the list of sizes, below the rule under the tabs; in the Fonts view
/// below the list of fonts, whose notes reach across the screen.
pub fn picture_at(app: &App) -> (usize, usize) {
    match app.view {
        View::Fonts => (
            PICTURE_ROW
                .saturating_add(body(app).list.len())
                .saturating_add(1),
            2,
        ),
        // below the sentence above the sizes and its blank row
        View::Looks => (
            PICTURE_ROW
                .saturating_add(prose(words::LOOKS_SIZES, app.cols).len())
                .saturating_add(1),
            PICTURE_COL,
        ),
        _ => (PICTURE_ROW, PICTURE_COL),
    }
}

const PICTURE_ROW: usize = 4;
const PICTURE_COL: usize = 26;

/// One place in the tab bar: a view's name, or an action's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    View(View),
    Action(usize),
}

/// The tab bar's places, each with its first column and its width: the
/// views on the left, the current one marked, and the actions against the
/// right edge, in their short forms where the long ones do not fit.
fn tab_places(app: &App) -> Vec<(Tab, usize, String)> {
    let names = [
        words::FLOW,
        words::LOOKS,
        words::FONTS,
        words::SETTINGS,
        words::HELP,
    ];
    let mut out = Vec::new();
    let mut x = 0usize;
    for (view, name) in View::TABS.iter().zip(names) {
        let text = format!(" {name} ");
        let w = text.chars().count();
        out.push((Tab::View(*view), x, text));
        x = x.saturating_add(w);
    }
    let tabs_end = x;
    let actions = |long: bool, gap: usize| -> Vec<String> {
        words::ACTIONS
            .iter()
            .map(|(l, s)| {
                let name = if long { l } else { s };
                format!("{}{name}{}", " ".repeat(gap), " ".repeat(gap))
            })
            .collect()
    };
    let fits = |a: &[String]| {
        let w: usize = a.iter().map(|t| t.chars().count()).sum::<usize>();
        tabs_end.saturating_add(1).saturating_add(w) <= app.cols
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
    let mut x = app.cols.saturating_sub(width);
    let tight = shown.first().is_some_and(|t| !t.starts_with(' '));
    for (i, text) in shown.into_iter().enumerate() {
        let w = text.chars().count();
        out.push((Tab::Action(i), x, text));
        x = x.saturating_add(w).saturating_add(usize::from(tight));
    }
    out
}

/// Draws the tab bar on row 0.
fn tab_bar(canvas: &mut Canvas, app: &App) {
    for (tab, x, text) in tab_places(app) {
        let current = match tab {
            Tab::View(v) => app.view.tab() == v,
            Tab::Action(_) => false,
        };
        let style = match (tab, current) {
            (_, true) => Style::PLAIN.reversed(),
            (Tab::View(_), false) => Style::PLAIN,
            (Tab::Action(_), false) => Style::text(palette::BASE),
        };
        canvas.put(x, 0, &text, style);
    }
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

/// The status line, row 1: how the draft stands and, in the Flow view, the
/// tags; at the right, what live preview is doing.
fn status(canvas: &mut Canvas, app: &App) {
    let flow = &app.inputs.flow;
    let mut left = match &flow.draft {
        Some(d) => words::draft_state(&d.name, d.changes, d.problems),
        None => words::NO_DRAFT.to_owned(),
    };
    let waiting = app
        .pending
        .frits
        .len()
        .saturating_add(app.pending.removals.len());
    if waiting > 0 {
        left = format!("{left} · {}", words::frits_waiting(waiting));
    }
    if app.view == View::Flow && !app.tags.is_empty() {
        let names: Vec<String> = app.tags.iter().map(|t| flow.name(*t)).collect();
        left = format!("{left} · {}", words::tagged(&names));
    }
    if app.view == View::Flow && app.draft.is_some() {
        if let Some(came) = &app.came {
            left = format!("{left} · {came}");
        }
        if !(app.inputs.hotplug && app.present.udev) {
            left = format!("{left} · {}", words::NO_HOTPLUG);
        }
    }
    let live = match app.live {
        Live::Asking => words::LIVE_ASKING,
        Live::On => words::LIVE_ON,
        Live::OpsOff | Live::NotVtium => words::LIVE_OFF,
    };
    let room = app
        .cols
        .saturating_sub(live.chars().count())
        .saturating_sub(3);
    let left = super::canvas::fit(&format!(" {left}"), room);
    canvas.put(
        0,
        1,
        &sides(&left, &format!("{live} "), app.cols),
        Style::PLAIN,
    );
}

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;

/// The view's keys in rows of at most the screen's width less its margin,
/// each key and what it does kept whole on one row.
fn key_rows(app: &App) -> Vec<String> {
    let keys = match app.view {
        View::Flow if app.route.is_some() => words::KEYS_ROUTE,
        View::Flow if app.confirm.is_some() => words::KEYS_CONFIRM,
        View::Flow if app.pager.is_some() => words::KEYS_PAGER,
        View::Flow if app.restore.is_some() => words::KEYS_RESTORE,
        View::Flow if app.opened && app.card.typed.is_some() => words::KEYS_TYPING,
        View::Flow if app.opened && !app.fields().is_empty() => words::KEYS_CARD,
        View::Flow => words::KEYS_FLOW,
        View::Looks => words::KEYS_LOOKS,
        View::Fonts => words::KEYS_FONTS,
        View::Settings if app.editing.is_some() => words::KEYS_EDITING,
        View::Settings => words::KEYS_SETTINGS,
        View::Help => words::KEYS_HELP,
        View::New => words::KEYS_NEW,
    };
    let width = app.cols.saturating_sub(2).max(1);
    // the Flow view's key line shows the keys that fit on one row, and
    // always its last, the key that leaves
    if app.view == View::Flow {
        let items: Vec<&str> = keys.split("  ").collect();
        let (last, rest) = items.split_last().unwrap_or((&"", &[]));
        let mut row = String::new();
        for item in rest {
            let len = row
                .chars()
                .count()
                .saturating_add(2)
                .saturating_add(item.chars().count());
            if len.saturating_add(2).saturating_add(last.chars().count()) > width {
                break;
            }
            if !row.is_empty() {
                row.push_str("  ");
            }
            row.push_str(item);
        }
        if !row.is_empty() {
            row.push_str("  ");
        }
        row.push_str(last);
        return vec![row];
    }
    let mut rows: Vec<String> = Vec::new();
    for item in keys.split("  ") {
        match rows.last_mut() {
            Some(row)
                if row
                    .chars()
                    .count()
                    .saturating_add(2)
                    .saturating_add(item.chars().count())
                    <= width =>
            {
                row.push_str("  ");
                row.push_str(item);
            }
            _ => rows.push(item.to_owned()),
        }
    }
    rows
}

/// The cells a picture may cover, columns and rows: from its place to the
/// right edge, and down to the row above the message line, so it never
/// covers the keys.
pub fn picture_area(app: &App) -> (usize, usize) {
    // the rule, the keys, and the message at its longest
    let footer = 1usize
        .saturating_add(key_rows(app).len())
        .saturating_add(MESSAGE_ROWS);
    let (row, col) = picture_at(app);
    (
        app.cols.saturating_sub(col.saturating_sub(1)),
        app.rows
            .saturating_sub(footer)
            .saturating_sub(row.saturating_sub(1)),
    )
}

/// The screen below the minimum size.
fn small(canvas: &mut Canvas, app: &App) {
    let cols = app.cols;
    let mut lines: Vec<String> = Vec::new();
    lines.extend(wrap(
        &words::too_small(app.cols, app.rows),
        cols,
        usize::MAX,
    ));
    lines.push(String::new());
    lines.extend(wrap(words::TOO_SMALL_COMMANDS, cols, usize::MAX));
    for c in crate::help::Command::ALL {
        if matches!(c, crate::help::Command::Serve | crate::help::Command::Tui) {
            continue;
        }
        lines.push(format!("  vtium-config {}", c.name()));
    }
    lines.push(String::new());
    lines.extend(wrap(words::TOO_SMALL_KEYS, cols, usize::MAX));
    // the keys stay on the screen however few its rows
    let keys = wrap(words::TOO_SMALL_KEYS, cols, usize::MAX);
    if lines.len() > app.rows {
        let room = app.rows.saturating_sub(keys.len());
        lines.truncate(room);
        lines.extend(keys);
    }
    for (y, line) in lines.iter().enumerate() {
        canvas.put(0, y, line, Style::PLAIN);
    }
}

/// The Flow view: the base card, the columns with their lines, the card
/// Enter opens, the message and the keys; or the route card in the
/// columns' place.
fn flow_screen(canvas: &mut Canvas, app: &App) -> Option<super::flowdraw::Drawn> {
    let flow = &app.inputs.flow;
    let (cols, rows) = (app.cols, app.rows);
    let keys = key_rows(app);
    let message = app
        .message
        .as_deref()
        .map(|m| wrap(m, cols.saturating_sub(2), MESSAGE_ROWS))
        .unwrap_or_default();
    let bottom = rows
        .saturating_sub(keys.len())
        .saturating_sub(message.len());
    for (i, line) in message.iter().enumerate() {
        canvas.put(
            0,
            bottom.saturating_add(i),
            &format!(" {line}"),
            Style::PLAIN,
        );
    }
    for (i, line) in keys.iter().enumerate() {
        canvas.put(
            0,
            rows.saturating_sub(keys.len()).saturating_add(i),
            &format!(" {line}"),
            Style::text(palette::GREY),
        );
    }
    if let Some(cursor) = app.route {
        route_screen(canvas, app, cursor, bottom);
        return None;
    }
    if let Some(pager) = app.pager {
        pager_screen(canvas, app, pager, bottom);
        return None;
    }
    let base = words::base_row(&flow.base.set, flow.base.defaults);
    let base_style = if app.flow_at == Item::Base {
        Style::block(palette::BASE).reversed()
    } else {
        Style::block(palette::BASE)
    };
    let base_text = if app.inputs.color {
        format!(" {base} ")
    } else {
        format!("[{base}]")
    };
    let shown = super::canvas::fit(&base_text, cols);
    canvas.put(0, 2, &shown, base_style);
    let room = if rows >= 30 { 6 } else { 4 };
    let card = if let Some((kept, at)) = &app.restore {
        restore_rows(kept, *at, room)
    } else if app.opened {
        let mut card = super::flowdraw::card(flow, app.flow_at);
        let fields = app.fields();
        if !fields.is_empty() {
            card.truncate(1);
            card.extend(field_rows(app, &fields, room));
        }
        card
    } else {
        Vec::new()
    };
    let card_rows = if card.is_empty() {
        0
    } else {
        card.len().saturating_add(1)
    };
    let top = 4usize;
    let height = bottom.saturating_sub(card_rows).saturating_sub(top);
    let ctx = super::flowdraw::Ctx {
        flow,
        tags: &app.tags,
        color: app.inputs.color,
    };
    if !flow.read {
        for (i, line) in wrap(words::FLOW_NOT_READ, cols.saturating_sub(2), 2)
            .iter()
            .enumerate()
        {
            canvas.put(1, top.saturating_add(i), line, Style::text(palette::GREY));
        }
    }
    let (top, height) = if flow.read {
        (top, height)
    } else {
        (top.saturating_add(3), height.saturating_sub(3))
    };
    let drawn = super::flowdraw::draw_columns(
        canvas,
        &ctx,
        app.flow_at,
        top.saturating_sub(1),
        top,
        height,
    );
    if card_rows > 0 {
        let y = bottom.saturating_sub(card_rows);
        let color = card
            .first()
            .and_then(|r| r.first())
            .and_then(|(_, s)| s.fg)
            .unwrap_or(palette::GREY);
        canvas.put(0, y, &"─".repeat(cols), Style::text(color));
        for (i, row) in card.iter().enumerate() {
            let mut x = 1usize;
            for (text, style) in row {
                x = canvas.put(x, y.saturating_add(1).saturating_add(i), text, *style);
            }
        }
    }
    Some(drawn)
}

/// The opened card's fields, at most `room` of them around the cursor:
/// each its label, then the values it offers with the current one in its
/// block and the cursor's in reverse video, then `type…` where a value
/// may be typed; the value being typed takes the field's row.
fn field_rows(app: &App, fields: &[super::edit::Field], room: usize) -> Vec<Vec<(String, Style)>> {
    let grey = Style::text(palette::GREY);
    let first = app
        .card
        .field
        .saturating_add(1)
        .saturating_sub(room)
        .min(fields.len().saturating_sub(room));
    fields
        .iter()
        .enumerate()
        .skip(first)
        .take(room)
        .map(|(i, f)| {
            let here = i == app.card.field;
            let mark = if here { "▸ " } else { "  " };
            let mut row = vec![(format!("{mark}{:<18}", f.label), grey)];
            if here && let Some(typed) = &app.card.typed {
                row.push((format!(" {typed}_ "), Style::PLAIN.reversed()));
                return row;
            }
            for (k, o) in f.options.iter().enumerate() {
                let style = if here && k == app.card.pick {
                    Style::PLAIN.reversed()
                } else if *o == f.current {
                    Style::block(palette::LOOKS)
                } else {
                    Style::PLAIN
                };
                row.push((format!(" {o} "), style));
            }
            if f.typed {
                let style = if here && app.card.pick == f.options.len() {
                    Style::PLAIN.reversed()
                } else {
                    grey
                };
                let shown = if f.options.is_empty() && !f.current.is_empty() {
                    format!(" {} · {} ", f.current, words::OR_TYPE_VALUE)
                } else {
                    format!(" {} ", words::OR_TYPE_VALUE)
                };
                row.push((shown, style));
            }
            row
        })
        .collect()
}

/// Restore's list of the installed history, at most `room` files around
/// the highlighted one, under its title.
fn restore_rows(kept: &[super::desk::Kept], at: usize, room: usize) -> Vec<Vec<(String, Style)>> {
    let first = at
        .saturating_add(1)
        .saturating_sub(room)
        .min(kept.len().saturating_sub(room));
    let mut out = vec![vec![(
        words::RESTORE_TITLE.to_owned(),
        Style {
            bold: true,
            ..Style::PLAIN
        },
    )]];
    for (i, k) in kept.iter().enumerate().skip(first).take(room) {
        let style = if i == at {
            Style::PLAIN.reversed()
        } else {
            Style::PLAIN
        };
        out.push(vec![(
            format!(" {:>2}  {} ", i.saturating_add(1), k.name),
            style,
        )]);
    }
    out
}

/// The draft in the pager, from row 2 down to `bottom`: its title, then
/// each line with its number, a line the check complains about marked `!`
/// in bright red, and the line of the card selected in the flow marked
/// with [`SELECTED`].
fn pager_screen(canvas: &mut Canvas, app: &App, pager: super::model::Pager, bottom: usize) {
    let Some(draft) = &app.draft else {
        return;
    };
    canvas.put(
        1,
        2,
        &format!("{} · draft {}", words::PAGER_TITLE, draft.name),
        Style {
            bold: true,
            ..Style::PLAIN
        },
    );
    let flagged: Vec<usize> = draft
        .problems
        .iter()
        .filter_map(|l| l.split(':').nth(1).and_then(|n| n.trim().parse().ok()))
        .collect();
    let width = app.cols.saturating_sub(8);
    for (row, (i, line)) in (4..bottom).zip(draft.text.lines().enumerate().skip(pager.top)) {
        let n = i.saturating_add(1);
        let (mark, style) = if flagged.contains(&n) {
            ("!", Style::text(palette::ERROR))
        } else if n == pager.mark {
            (SELECTED, Style::PLAIN)
        } else {
            (" ", Style::text(palette::GREY))
        };
        let shown: String = line.chars().skip(pager.left).take(width).collect();
        canvas.put(0, row, &format!("{mark}{n:>4} "), style);
        canvas.put(6, row, &shown, Style::PLAIN);
    }
}

/// The mark before the selected item's lines on the route card.
pub const SELECTED: &str = "▸";

/// The route card, from row 2 down to `bottom`: its title, the sentence of
/// what the item does, and each route, the selected item marked with
/// [`SELECTED`] and drawn in its section's block, and the cursor's line in
/// reverse video.
fn route_screen(canvas: &mut Canvas, app: &App, cursor: usize, bottom: usize) {
    let flow = &app.inputs.flow;
    let cols = app.cols;
    let (title, sentence, lines) = super::flowdraw::route_card(flow, app.flow_at);
    canvas.put(
        1,
        2,
        &title,
        Style {
            bold: true,
            ..Style::PLAIN
        },
    );
    let said = wrap(&sentence, cols.saturating_sub(2), 2);
    for (i, line) in said.iter().enumerate() {
        canvas.put(1, 3usize.saturating_add(i), line, Style::PLAIN);
    }
    let top = 4usize.saturating_add(said.len());
    let room = bottom.saturating_sub(top);
    let first = cursor.saturating_add(1).saturating_sub(room);
    for (i, line) in lines.iter().enumerate().skip(first).take(room) {
        let y = top.saturating_add(i).saturating_sub(first);
        let mut style = line.style;
        if line.item == Some(app.flow_at)
            && let Some(color) = line.style.fg
        {
            style = Style::block(color);
        }
        if i == cursor {
            style = style.reversed();
        }
        let indent = if line.item.is_some() { 3 } else { 1 };
        let text = super::canvas::fit(&line.text, cols.saturating_sub(indent).saturating_sub(1));
        if line.item == Some(app.flow_at) {
            // the selected item is marked in words as well as in color: the
            // mark before it shows with no color too
            canvas.put(0, y, SELECTED, Style::PLAIN);
            // its block starts a cell to the left, so its text lines up
            // with the others
            let x = indent.saturating_sub(1);
            canvas.fill(
                x,
                y,
                text.chars().count().saturating_add(2),
                &format!(" {text}"),
                style,
            );
        } else {
            canvas.put(indent, y, &text, style);
        }
    }
}

/// The item at cell (`col`, `row`) of the Flow view, as a click finds it:
/// the base card's row, or an item of the columns.
pub fn item_at(app: &App, col: usize, row: usize) -> Option<Item> {
    if row == 2 {
        return Some(Item::Base);
    }
    let mut canvas = Canvas::new(app.cols, app.rows);
    let drawn = flow_screen(&mut canvas, app)?;
    super::flowdraw::hit(&drawn, col, row)
}

/// The screen for `app`: exactly `app.rows` rows, none wider than
/// `app.cols`.
pub fn screen(app: &App) -> Vec<Row> {
    let (cols, rows) = (app.cols, app.rows);
    let mut canvas = Canvas::new(cols, rows);
    if too_small(app) {
        small(&mut canvas, app);
        return canvas.into_rows();
    }
    tab_bar(&mut canvas, app);
    status(&mut canvas, app);
    if app.view == View::Flow {
        flow_screen(&mut canvas, app);
        return canvas.into_rows();
    }
    canvas.put(0, 2, &"-".repeat(cols), Style::PLAIN);
    let message = wrap(
        app.message.as_deref().unwrap_or(""),
        cols.saturating_sub(2),
        MESSAGE_ROWS,
    );
    let keys = key_rows(app);
    let room = rows
        .saturating_sub(4)
        .saturating_sub(message.len())
        .saturating_sub(keys.len());
    let Body { list, keep, tail } = body(app);
    // the sentences under the list show whole, while the list keeps at
    // least LIST_MIN rows; otherwise they are dropped
    let show_tail = !tail.is_empty()
        && (list.len().saturating_add(tail.len()) <= room
            || room >= LIST_MIN.saturating_add(tail.len()));
    let list_room = if show_tail {
        room.saturating_sub(tail.len())
    } else {
        room
    };
    let first = if app.view == View::Help {
        keep.min(list.len().saturating_sub(list_room))
    } else {
        keep.saturating_add(1).saturating_sub(list_room)
    };
    let mut y = 3usize;
    for row in list.into_iter().skip(first).take(list_room) {
        canvas.row(y, &row);
        y = y.saturating_add(1);
    }
    if show_tail {
        for row in tail {
            canvas.row(y, &row);
            y = y.saturating_add(1);
        }
    }
    let foot = rows
        .saturating_sub(1)
        .saturating_sub(message.len())
        .saturating_sub(keys.len());
    for (i, line) in message.iter().enumerate() {
        canvas.put(0, foot.saturating_add(i), &format!(" {line}"), Style::PLAIN);
    }
    let rule = foot.saturating_add(message.len());
    canvas.put(0, rule, &"-".repeat(cols), Style::PLAIN);
    for (i, line) in keys.iter().enumerate() {
        canvas.put(
            0,
            rule.saturating_add(1).saturating_add(i),
            &format!(" {line}"),
            Style::PLAIN,
        );
    }
    canvas.into_rows()
}

/// The screen as plain text, one line a row, for tests and goldens; a raw
/// span's escapes are left out.
pub fn as_text(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| {
            let line: String = r
                .iter()
                .map(|s| {
                    if s.raw {
                        strip(&s.text)
                    } else {
                        s.text.clone()
                    }
                })
                .collect();
            format!("{}\n", line.trim_end())
        })
        .collect()
}

/// `text` without its CSI sequences.
fn strip(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The bytes that draw `rows` whole: each row placed with CUP and erased
/// in the terminal's own colors, then each span in its style by SGR, color
/// only when `color` is on. The row is erased before it is written: after a
/// character in the last column, the cursor waits there for the wrap, and
/// an erase would take that character with it.
pub fn paint(rows: &[Row], color: bool) -> Vec<u8> {
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!("\x1b[{};1H\x1b[2K", i.saturating_add(1)));
        for s in row {
            if s.raw {
                out.push_str(&s.text);
                out.push_str("\x1b[0m");
                continue;
            }
            // a name read from a file never carries a control onto the
            // screen
            let shown: String = s
                .text
                .chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect();
            let sgr = s.style.sgr(color);
            if sgr.is_empty() {
                out.push_str(&shown);
            } else {
                out.push_str(&format!("{sgr}{shown}\x1b[0m"));
            }
        }
    }
    out.into_bytes()
}

/// The bytes that draw `app`'s screen whole.
pub fn draw(app: &App) -> Vec<u8> {
    paint(&screen(app), app.inputs.color)
}
