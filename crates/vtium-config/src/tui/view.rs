//! The TUI's screen (ADR-1201 D3, ADR-1403 D7): the state drawn on a
//! [`Canvas`], and the bytes that draw it whole, with ECMA-48's cursor
//! position, erase and SGR sequences only (D7).
//!
//! Every view has the tab bar, with the views and the actions, and the
//! status line. The Flow view then draws its document of paths and parts,
//! or the file view or Restore's list over it ([`super::flowdraw`]). The
//! other views
//! draw a rule, their body, a message line, a rule and their keys; a body
//! longer than the room scrolls so the highlighted row shows. Below 80 by
//! 25 cells no view is drawn: one screen says the size the TUI needs, the
//! commands that work at any size, and the keys that leave and that undo a
//! font preview, so the user can always get out.

use super::canvas::{Canvas, Row, Span, span, wrap};
use super::color::{Style, palette};
use super::listbox::{self, Item, Marks};
use super::model::{App, DpiFrom, FontState, Live, SIZES, Then, View};
use super::tabs::fonts::NEW_FIELDS;
use super::words;
use crate::specimen::Sources;
use vtium_settings::keys::Kind as KeyKind;

/// The smallest grid a view is drawn in (ADR-1201 D3, ADR-1403 D7): below
/// it, the one screen that says so.
pub const MIN_COLS: usize = 80;
pub const MIN_ROWS: usize = 25;

/// Whether `app`'s grid is below the minimum.
pub fn too_small(app: &App) -> bool {
    app.size.cols < MIN_COLS || app.size.rows < MIN_ROWS
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
            let sentence = if app.text_preview() || !app.shared.graphics {
                app.preview_line()
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
                match app.preview_line() {
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
        let (does, keys) = if *name == words::FONTS && app.text_preview() {
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
const FLOW_TOP: usize = 4;
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

/// Draws the tab bar on row 0.
fn tab_bar(canvas: &mut Canvas, app: &App) {
    for (tab, x, text) in tab_places(app) {
        let current = match tab {
            Tab::View(v) => app.nav.view.tab() == v,
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
/// tags; at the right, how a change is previewed and whether `t` tries
/// a font live.
fn status(canvas: &mut Canvas, app: &App) {
    let flow = &app.shared.inputs.flow;
    // the left side with `waiting`, the words for the font sets waiting
    // to be installed
    let left_with = |waiting: &dyn Fn(usize) -> String| {
        let mut left = match &flow.draft {
            Some(d) => words::draft_state(&d.name, d.changes, app.shared.unsaved, d.problems),
            None => words::NO_DRAFT.to_owned(),
        };
        let pending = app
            .shared
            .pending
            .frits
            .len()
            .saturating_add(app.shared.pending.removals.len());
        if pending > 0 {
            left = format!("{left} · {}", waiting(pending));
        }
        if app.nav.view == View::Flow && app.shared.draft.is_some() {
            if let Some(came) = &app.shared.came {
                left = format!("{left} · {came}");
            }
            if !(app.shared.inputs.hotplug && app.shared.present.udev) {
                left = format!("{left} · {}", words::NO_HOTPLUG);
            }
        }
        left
    };
    // the right side, in its long or its short words
    // a vtium console draws the preview as text, pictures or not
    let pictures = app.shared.graphics && !app.text_preview();
    let live_with = |short: bool| match (&app.shared.live, pictures) {
        (Live::Asking, _) => words::PREVIEW_ASKING.to_owned(),
        (live, graphics) => format!(
            "{} · {}",
            match (graphics, short) {
                (true, false) => words::PREVIEW_PICTURE,
                (true, true) => words::PREVIEW_PICTURE_SHORT,
                (false, false) => words::PREVIEW_TEXT,
                (false, true) => words::PREVIEW_TEXT_SHORT,
            },
            if *live == Live::On {
                words::TRIAL_KEY
            } else {
                words::TRIAL_NONE
            }
        ),
    };
    // the room the left side has beside the right side, with the gap
    // between them and the margin after
    let room = |live: &str| {
        app.size
            .cols
            .saturating_sub(live.chars().count())
            .saturating_sub(3)
    };
    // the long words where they fit, else the short words for the font
    // sets waiting, else the short words on the right as well; only then
    // is the left side cut
    let long = format!(" {}", left_with(&words::frits_waiting));
    let short = format!(" {}", left_with(&words::frits_waiting_short));
    let fits = |left: &str, live: &str| left.chars().count() <= room(live);
    let (live_long, live_short) = (live_with(false), live_with(true));
    let (left, live) = if fits(&long, &live_long) {
        (long, live_long)
    } else if fits(&short, &live_long) {
        (short, live_long)
    } else {
        (short, live_short)
    };
    let room = room(&live);
    let left = super::canvas::fit(&left, room);
    canvas.put(
        0,
        1,
        &sides(&left, &format!("{live} "), app.size.cols),
        Style::PLAIN,
    );
}

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;

/// The hand-written keys of a view whose node of the interface tree is
/// still planned: Looks, Fonts and the new-frit form. Their work
/// rebuilds them and moves their nodes to supported, and the key line then
/// comes from the contract, as everywhere else.
fn planned_keys(app: &App) -> &'static str {
    match app.nav.view {
        View::Looks => words::KEYS_LOOKS,
        View::Fonts if app.text_preview() => words::KEYS_FONTS_LIVE,
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
fn item_rows(items: &str, width: usize) -> Vec<String> {
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
fn message_rows(app: &App) -> Vec<String> {
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
fn list_window(app: &App, list: usize, tail: usize) -> (usize, bool) {
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

/// The screen below the minimum size.
fn small(canvas: &mut Canvas, app: &App) {
    let cols = app.size.cols;
    let mut lines: Vec<String> = Vec::new();
    // the live trial's countdown, and its keys, first
    if app.nav.trial.is_some()
        && let Some(message) = &app.shared.message
    {
        lines.extend(wrap(message, cols, usize::MAX));
        lines.push(String::new());
    }
    lines.extend(wrap(
        &words::too_small(app.size.cols, app.size.rows),
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
    // a question asked here comes first, what it says and its keys
    // wrapped to the width, so it shows whole however narrow the screen;
    // while it is open the screen's own keys give way to the question's,
    // since q does nothing under a question
    let question: Vec<String> = match app.nav.confirm.as_ref().map(|c| &c.then) {
        None => Vec::new(),
        Some(Then::Quit(_)) => item_rows(
            &format!(
                "{}  {}",
                words::small_question(app.shared.unsaved),
                key_line(app)
            ),
            cols,
        ),
        // the leave question's keys fill a row, so what it says goes on
        // the row above them
        Some(Then::Leave(_)) => wrap(&words::small_leave(app.shared.unsaved), cols, usize::MAX)
            .into_iter()
            .chain(item_rows(key_line(app), cols))
            .collect(),
        Some(_) => item_rows(key_line(app), cols),
    };
    // the keys that stay on the screen however few its rows: the
    // question's, at the top, or the screen's own, at the bottom
    // the screen's own keys are its generated key line, and a sentence
    // about u while the key line leaves u out
    let keys = if question.is_empty() {
        lines.push(String::new());
        let mut keys = item_rows(key_line(app), cols);
        keys.extend(wrap(words::TOO_SMALL_UNDO, cols, usize::MAX));
        lines.extend(keys.clone());
        keys
    } else {
        Vec::new()
    };
    let mut lines: Vec<String> = question.into_iter().chain(lines).collect();
    if lines.len() > app.size.rows {
        let room = app.size.rows.saturating_sub(keys.len());
        lines.truncate(room);
        lines.extend(keys);
    }
    for (y, line) in lines.iter().enumerate() {
        canvas.put(0, y, line, Style::PLAIN);
    }
}

/// The Flow view's screens that take the paths' place: the file view
/// and Restore's list of the installed history.
fn flow_screen(canvas: &mut Canvas, app: &App) {
    let (cols, rows) = (app.size.cols, app.size.rows);
    let keys = key_rows(app);
    let message = app
        .shared
        .message
        .clone()
        .or_else(|| app.preview_line())
        .map(|m| wrap(&m, cols.saturating_sub(2), MESSAGE_ROWS))
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
    if let Some(pager) = app.pager {
        pager_screen(canvas, app, pager, bottom);
        return;
    }
    if let Some((kept, at)) = &app.restore {
        let room = bottom.saturating_sub(FLOW_TOP).max(1);
        for (i, row) in restore_rows(kept, *at, room).iter().enumerate() {
            let mut x = 1usize;
            for (text, style) in row {
                x = canvas.put(x, FLOW_TOP.saturating_add(i), text, *style);
            }
        }
    }
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
    let Some(draft) = &app.shared.draft else {
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
    let width = app.size.cols.saturating_sub(8);
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

/// The mark the file view puts before the line of the highlighted box.
pub const SELECTED: &str = "▸";

/// The screen for `app`: exactly `app.rows` rows, none wider than
/// `app.cols`.
pub fn screen(app: &App) -> Vec<Row> {
    let (cols, rows) = (app.size.cols, app.size.rows);
    let mut canvas = Canvas::new(cols, rows);
    if too_small(app) {
        small(&mut canvas, app);
        return canvas.into_rows();
    }
    tab_bar(&mut canvas, app);
    if super::flowdraw::shown(app) {
        super::flowdraw::screen(&mut canvas, app);
        return canvas.into_rows();
    }
    status(&mut canvas, app);
    if app.nav.view == View::Flow {
        flow_screen(&mut canvas, app);
        return canvas.into_rows();
    }
    canvas.put(0, 2, &"-".repeat(cols), Style::PLAIN);
    let message = message_rows(app);
    let keys = key_rows(app);
    let Body { list, keep, tail } = body(app);
    let (list_room, show_tail) = list_window(app, list.len(), tail.len());
    let first = if app.nav.view == View::Help {
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
    // in a vtium console, the preview's text where a picture would go
    super::console::draw(&mut canvas, app);
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
    paint(&screen(app), app.shared.inputs.color)
}
