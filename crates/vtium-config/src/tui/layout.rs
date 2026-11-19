//! Where things stand on the TUI's screen, as pure functions of the model
//! that update and the view both call: whether the grid is too small, the
//! places of the tab bar, the key line's rows, each view's body and the
//! window it scrolls in, the Help page, and where a picture goes. Each
//! function reads the model and the terminal's size and changes nothing.
//! Update reads these facts here, so it never calls the view to learn
//! them, and the view draws what these functions place.
//!
//! References:
//! - ADR-1206 D4, "Layout is a pure function": update calls layout
//!   functions and never a view.

use super::canvas::{Row, Span, span, wrap};
use super::color::{Style, palette};
use super::listbox::{self, Item, Marks};
use super::model::{App, DpiFrom, FontState, SIZES, Size, View};
use super::tabs::fonts::NEW_FIELDS;
use super::words;
use crate::specimen::Sources;
use vtium_settings::keys::Kind as KeyKind;

/// The smallest grid a view is drawn in (ADR-1201 D3, ADR-1403 D7): below
/// it, the one screen that says so.
pub const MIN_COLS: usize = 80;
pub const MIN_ROWS: usize = 25;

/// Whether a grid of `size` is below the minimum.
pub fn too_small(size: Size) -> bool {
    size.cols < MIN_COLS || size.rows < MIN_ROWS
}

fn plain(text: impl Into<String>) -> Row {
    vec![span(text, Style::PLAIN)]
}

fn marked(text: impl Into<String>) -> Row {
    vec![span(text, Style::PLAIN.reversed())]
}

/// A view's body: its list, the row of the list to keep in sight, and the
/// sentences under it, which show whole or not at all.
pub struct Body {
    pub list: Vec<Row>,
    pub keep: usize,
    pub tail: Vec<Row>,
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
pub fn body(app: &App) -> Body {
    let cols = app.size.cols;
    match app.nav.view {
        View::Fonts => {
            let width = app
                .shared
                .inputs
                .fonts
                .iter()
                .map(|f| f.name.chars().count())
                .max()
                .unwrap_or(0)
                .clamp(8, 24);
            let configured = app.configured("font-name");
            let items: Vec<(String, String)> = app
                .shared
                .inputs
                .fonts
                .iter()
                .map(|f| {
                    let mut note = match (&f.state, app.shared.sources.get(&f.name)) {
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
                    if app.shared.refused.contains(&f.name) {
                        note = words::KEPT.to_owned();
                    }
                    if app.shared.pending.removals.contains(&f.name) {
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
            let (list, keep) = list(&items, app.fonts.at(), width, cols);
            Body {
                list,
                keep,
                tail: app
                    .preview()
                    .preview_line()
                    .map(|line| tail(&line, cols))
                    .unwrap_or_default(),
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
            let (list, keep) = list(&items, app.looks.at(), 10, cols);
            let sentence = if app.shared.text_preview() || !app.shared.graphics {
                app.preview()
                    .preview_line()
                    .unwrap_or_else(|| words::NO_PICTURES.to_owned())
            } else if app.size() == 0 {
                words::AUTOMATIC_PICTURE.to_owned()
            } else {
                let (dpi, from) = app.shared.inputs.dpi;
                let from = match from {
                    DpiFrom::Conf => words::DPI_FROM_CONF,
                    DpiFrom::Edid => words::DPI_FROM_EDID,
                    DpiFrom::Assumed => words::DPI_ASSUMED,
                };
                match app.preview().preview_line() {
                    Some(line) => format!("{} {line}", words::density(dpi, from)),
                    None => words::density(dpi, from),
                }
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
        View::Settings => settings_body(app),
        View::New => {
            let mut items: Vec<(String, String)> = words::NEW_FIELD_NAMES
                .iter()
                .zip(&app.fonts.form().fields)
                .enumerate()
                .map(|(i, (name, text))| {
                    let cursor = if i == app.fonts.form().at { "_" } else { "" };
                    ((*name).to_owned(), format!("{text}{cursor}"))
                })
                .collect();
            items.push((words::NEW_ADD.to_owned(), String::new()));
            let (list, keep) = list(&items, app.fonts.form().at.min(NEW_FIELDS), 8, cols);
            Body {
                list,
                keep,
                tail: tail(words::NEW_HINT, cols),
            }
        }
        View::Help => Body {
            keep: app.help.top(),
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

/// The Settings view's body, in the shared list: the sections, with the
/// highlighted section's description under them; or the open section's
/// keys in their titled groups, each with its value, and the highlighted
/// key's description under them; or the highlighted key's allowed values.
///
/// References:
/// - ADR-1201 D3, the Settings bullet: sections, then groups of keys.
/// - ADR-1403 D7, "One list for every set of items": the list's look.
fn settings_body(app: &App) -> Body {
    let cols = app.size.cols;
    let sections = super::tabs::settings::sections();
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    let Some((at, keys_list)) = app.settings.open() else {
        let items: Vec<Item> = sections
            .iter()
            .map(|s| Item::new(s.title, words::settings_count(s.keys().len())))
            .collect();
        let (list, keep) = listbox::rows(&items, app.settings.section_at(), Marks::None, cols);
        let about = sections
            .get(app.settings.section_at())
            .map_or("", |s| s.about);
        let mut tail = tail(words::SECTIONS_HINT, cols);
        tail.extend(
            wrap(about, cols.saturating_sub(2), WHAT_ROWS)
                .into_iter()
                .map(|l| plain(format!(" {l}"))),
        );
        return Body { list, keep, tail };
    };
    let Some(section) = sections.get(at) else {
        return Body {
            list: Vec::new(),
            keep: 0,
            tail: Vec::new(),
        };
    };
    let keys = section.keys();
    let key = keys.get(keys_list.at).copied();
    let what = key.map_or(String::new(), |k| k.what_line());
    let tail_of = |hint: &str| {
        let mut rows = tail(hint, cols);
        rows.extend(
            wrap(&what, cols.saturating_sub(2), WHAT_ROWS)
                .into_iter()
                .map(|l| plain(format!(" {l}"))),
        );
        rows
    };
    // a key's allowed values, open over the section
    if let (Some(values), Some(k)) = (app.settings.values(), key) {
        let now = app.value(k).0;
        let default = crate::commands::value_text(k, None).0;
        let items: Vec<Item> = super::tabs::settings::fixed_values(k)
            .unwrap_or_default()
            .into_iter()
            .map(|w| Item {
                on: w == now,
                ..Item::new(
                    w,
                    if w == default {
                        words::VALUE_DEFAULT
                    } else {
                        ""
                    },
                )
            })
            .collect();
        let title = words::settings_title(&[section.title, k.name]);
        let mut list = vec![vec![span(format!(" {title}"), bold)], plain("")];
        let (rows, keep) = listbox::rows(&items, values.at, Marks::Choice, cols);
        let keep = keep.saturating_add(list.len());
        list.extend(rows);
        return Body {
            list,
            keep,
            tail: tail_of(words::VALUES_HINT),
        };
    }
    let mut items = Vec::new();
    for (group, group_keys) in &section.groups {
        for (n, k) in group_keys.iter().enumerate() {
            let typed = app
                .settings
                .editing()
                .filter(|_| key.is_some_and(|h| h.name == k.name));
            let value = match typed {
                Some(t) => format!("{t}_"),
                None => match app.value(k) {
                    (v, true) => words::default_value(v),
                    (v, false) if v.is_empty() && k.kind == KeyKind::KeyBinding => {
                        words::NO_KEY.to_owned()
                    }
                    (v, false) => v,
                },
            };
            let value = if app.changed(k.name) {
                format!("{value}, {}", words::TO_SAVE)
            } else {
                value
            };
            items.push(Item {
                heading: (n == 0).then(|| (*group).to_owned()),
                ..Item::new(k.name, value)
            });
        }
    }
    let title = words::settings_title(&[section.title]);
    let mut list = vec![vec![span(format!(" {title}"), bold)], plain("")];
    let (rows, keep) = listbox::rows(&items, keys_list.at, Marks::None, cols);
    let keep = keep.saturating_add(list.len());
    list.extend(rows);
    let hint = match (app.settings.editing(), key) {
        (Some(_), Some(k)) if k.kind == KeyKind::KeyBinding => words::EDITING_BINDING,
        (Some(_), _) => words::EDITING,
        (None, _) => words::SETTINGS_HINT,
    };
    Body {
        list,
        keep,
        tail: tail_of(hint),
    }
}

/// The widest a key column of the Help page grows; keys wider than this
/// push their meaning onto the rows below.
const HELP_KEYS_MAX: usize = 16;

/// One node's section of the Help page, from the contract: what the node
/// is, in bold, then each key that acts there and what it does, the
/// meaning wrapped under its own column.
fn help_section(node: &super::contract::Node, cols: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    if node.help.is_empty() {
        return rows;
    }
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    for line in wrap(node.what, cols.saturating_sub(2), usize::MAX) {
        rows.push(vec![span(format!(" {line}"), bold)]);
    }
    let keys_width = node
        .help
        .iter()
        .map(|(keys, _)| keys.chars().count())
        .filter(|w| *w <= HELP_KEYS_MAX)
        .max()
        .unwrap_or(0);
    let indent = keys_width.saturating_add(5);
    for (keys, does) in node.help {
        let lines = wrap(does, cols.saturating_sub(indent).max(1), usize::MAX);
        let mut lines = lines.into_iter();
        if keys.chars().count() > keys_width {
            rows.push(plain(format!("   {keys}")));
        } else {
            let first = lines.next().unwrap_or_default();
            rows.push(vec![
                span(
                    format!("   {keys:<keys_width$}  "),
                    Style::text(palette::GREY),
                ),
                span(first, Style::PLAIN),
            ]);
        }
        for line in lines {
            rows.push(plain(format!("{:indent$}{line}", "")));
        }
    }
    rows.push(plain(""));
    rows
}

/// The Help view's rows: on the Help page, what each view does and its
/// keys, the route card, and the commands, from the catalog the command
/// line's help draws on; on the About page, what `vtium-config --help
/// about` prints, from the same source.
pub fn help_rows(app: &App) -> Vec<Row> {
    let cols = app.size.cols;
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    let mut rows = Vec::new();
    if app.help.about() {
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
    for node in super::contract::NODES {
        if node.supported {
            rows.extend(help_section(node, cols));
            continue;
        }
        // a view whose node is still planned keeps its hand-written words
        let Some((_, name, does, keys)) = words::HELP_PLANNED.iter().find(|p| p.0 == node.id)
        else {
            continue;
        };
        // the Fonts view's keys in a vtium console, where `s` saves
        let (does, keys) = if *name == words::FONTS && app.shared.text_preview() {
            (words::HELP_FONTS_LIVE, words::KEYS_FONTS_LIVE)
        } else {
            (*does, *keys)
        };
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
    match app.nav.view {
        View::Fonts => {
            let body = body(app);
            (
                PICTURE_ROW
                    .saturating_add(body.list.len())
                    .saturating_add(body.tail.len())
                    .saturating_add(1),
                2,
            )
        }
        // in the document's place, below the status lines
        View::Flow => (FLOW_TOP.saturating_add(1), 2),
        // below the sentence above the sizes and its blank row
        View::Looks => (
            PICTURE_ROW
                .saturating_add(prose(words::LOOKS_SIZES, app.size.cols).len())
                .saturating_add(1),
            PICTURE_COL,
        ),
        _ => (PICTURE_ROW, PICTURE_COL),
    }
}

const PICTURE_ROW: usize = 4;

/// The Flow view's first row of columns, from 0.
pub const FLOW_TOP: usize = 4;
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
pub fn tab_places(app: &App) -> Vec<(Tab, usize, String)> {
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
        let list: &[(&str, &str)] = match app.nav.view.tab() {
            View::Flow => &words::FLOW_ACTIONS,
            View::Help => &[],
            _ => &words::ACTIONS,
        };
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

/// The hand-written keys of a view whose node of the interface tree is
/// still planned: Looks, Fonts and the new-frit form. Their work
/// rebuilds them and moves their nodes to supported, and the key line then
/// comes from the contract, as everywhere else.
fn planned_keys(app: &App) -> &'static str {
    match app.nav.view {
        View::Looks => words::KEYS_LOOKS,
        View::Fonts if app.shared.text_preview() => words::KEYS_FONTS_LIVE,
        View::Fonts => words::KEYS_FONTS,
        View::New => words::KEYS_NEW,
        View::Flow | View::Settings | View::Help => "",
    }
}

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
        None => planned_keys(app),
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

/// The Help view's page: how many rows it has, and how many of them the
/// screen shows at once. The page's top row goes as far as the row that
/// puts the page's last row at the bottom of the screen.
pub fn help_window(app: &App) -> (usize, usize) {
    let len = help_rows(app).len();
    (len, list_window(app, len, 0).0)
}
