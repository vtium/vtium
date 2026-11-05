//! The TUI's screen (ADR-1201 D3): the state drawn as rows of text, and
//! the bytes that draw them whole, with ECMA-48's cursor position, erase
//! and SGR sequences only (D7).
//!
//! Every view has a title line, the views' tabs, a rule, its body, a
//! message line, a rule and its keys. A body longer than the room scrolls
//! so the highlighted row shows. A grid too small for that, as a large
//! previewed font can leave, draws one line naming the font and the keys
//! that keep it or go back, so the user can always get out.

use super::model::{App, DpiFrom, FontState, Live, NEW_FIELDS, SIZES, Saving, View, setting_keys};
use super::words;
use crate::draft::changes;
use crate::specimen::Sources;

/// The smallest grid the full screen is drawn in: below it, one line. The
/// views fit the 320x240 floor's 40 columns (ADR-1201 D3), and from 25
/// rows, the VGA console's 80x25 among them, every view shows its title,
/// its tabs, its keys and its message whole.
pub const MIN_COLS: usize = 40;
pub const MIN_ROWS: usize = 25;

/// One run of text on a row, shown in reverse video or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub reverse: bool,
}

/// One row of the screen.
pub type Row = Vec<Span>;

fn plain(text: impl Into<String>) -> Row {
    vec![Span {
        text: text.into(),
        reverse: false,
    }]
}

fn marked(text: impl Into<String>) -> Row {
    vec![Span {
        text: text.into(),
        reverse: true,
    }]
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
                    if app.pending.font_name() == Some(&f.name) {
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
        View::Sizes => {
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
                    let note = if app.pending.font_size() == Some(size) {
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
            Body {
                list,
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
                    let value = if app.pending.get(key.name).is_some() {
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
                cut(&what, cols.saturating_sub(2), WHAT_ROWS)
                    .into_iter()
                    .map(|l| plain(format!(" {l}"))),
            );
            Body { list, keep, tail }
        }
        View::Heads => match (app.has_flow(), app.heads()) {
            (false, _) => Body {
                list: prose(words::NO_HEAD_FILE, cols),
                keep: 0,
                tail: Vec::new(),
            },
            (true, None) => Body {
                list: prose(words::HEAD_FILE_ERRORS, cols),
                keep: 0,
                tail: Vec::new(),
            },
            (true, Some(heads)) => {
                let items: Vec<(String, String)> = heads
                    .iter()
                    .map(|(head, profile)| (head.clone(), words::head_profile(profile)))
                    .collect();
                let (list, keep) = list(&items, app.head_at, 16, cols);
                Body {
                    list,
                    keep,
                    tail: tail(words::HEADS_HINT, cols),
                }
            }
        },
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
        View::Save => Body {
            list: save_rows(app),
            keep: 0,
            tail: Vec::new(),
        },
    }
}

/// The save view: each file the save changes, each line that changes,
/// and the one command it runs.
fn save_rows(app: &App) -> Vec<Row> {
    let plan = match app.plan() {
        Ok(plan) => plan,
        Err(why) => return prose(&words::save_failed(why), app.cols),
    };
    if plan.is_empty() {
        return prose(words::NOTHING_TO_SAVE, app.cols);
    }
    let mut rows = Vec::new();
    for file in &plan.files {
        rows.extend(prose(&file.path.display().to_string(), app.cols));
        for c in changes(file.old.as_deref().unwrap_or_default(), &file.new) {
            if let Some(old) = &c.old {
                rows.push(plain(format!("   {:>4} - {old}", c.line)));
            }
            if let Some(new) = &c.new {
                rows.push(plain(format!("   {:>4} + {new}", c.line)));
            }
        }
    }
    for removal in &plan.removals {
        rows.extend(prose(&removal.path.display().to_string(), app.cols));
        let name = removal
            .path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let indent = "   ";
        for line in wrap(
            &words::removed_file(name),
            app.cols.saturating_sub(2 + indent.len()),
            usize::MAX,
        ) {
            rows.push(plain(format!("{indent}{line}")));
        }
    }
    if !app.report.is_empty() {
        rows.push(plain(""));
        rows.extend(prose(words::COMMAND_SAID, app.cols));
        rows.extend(app.report.iter().map(|l| plain(format!("   {l}"))));
    }
    rows.push(plain(""));
    let width = app.cols.saturating_sub(2);
    let mut say = |text: &str, indent: &str| {
        for line in wrap(text, width.saturating_sub(indent.len()), usize::MAX) {
            rows.push(plain(format!("{indent}{line}")));
        }
    };
    match &app.inputs.saving {
        Saving::Sudo(command) | Saving::User(command) => {
            let intro = match &app.inputs.saving {
                Saving::Sudo(_) => words::SAVE_RUNS,
                _ => words::SAVE_AS_USER,
            };
            say(intro, " ");
            say(command, "   ");
            say(&words::draft_under(app.inputs.drafts.display()), " ");
        }
    }
    rows
}

/// Where a picture goes, row and column from 1: in the size view beside
/// the list of sizes, below the rule under the tabs; in the fonts view
/// below the list of fonts, whose notes reach across the screen.
pub fn picture_at(app: &App) -> (usize, usize) {
    match app.view {
        View::Fonts => (
            PICTURE_ROW
                .saturating_add(body(app).list.len())
                .saturating_add(1),
            2,
        ),
        _ => (PICTURE_ROW, PICTURE_COL),
    }
}

const PICTURE_ROW: usize = 4;
const PICTURE_COL: usize = 26;

/// The tabs' row: each view's name, the current one in reverse video, and
/// the save view's after them while it shows. Where the row is wider than
/// the screen, the tabs after the current one go first, then those before
/// it, so the current view's name always shows.
fn tabs(app: &App) -> Row {
    let names = [
        words::FONTS,
        words::SIZES,
        words::SETTINGS,
        words::HEADS,
        words::NEW,
    ];
    let mut all: Vec<(String, bool)> = View::TABS
        .iter()
        .zip(names)
        .map(|(v, name)| (format!(" {name} "), *v == app.view))
        .collect();
    if app.view == View::Save {
        all.push((format!(" {} ", words::SAVE), true));
    }
    let width = |all: &[(String, bool)]| {
        all.iter()
            .map(|(t, _)| t.chars().count())
            .sum::<usize>()
            .saturating_add(1)
    };
    let current = all.iter().position(|(_, on)| *on).unwrap_or(0);
    while width(&all) > app.cols && all.len() > 1 {
        let drop = if all.len().saturating_sub(1) > current {
            all.len().saturating_sub(1)
        } else {
            0
        };
        all.remove(drop);
    }
    std::iter::once(Span {
        text: " ".into(),
        reverse: false,
    })
    .chain(
        all.into_iter()
            .map(|(text, reverse)| Span { text, reverse }),
    )
    .collect()
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

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;

/// The view's keys in rows of at most the screen's width less its margin,
/// each key and what it does kept whole on one row.
fn key_rows(app: &App) -> Vec<String> {
    let keys = match app.view {
        View::Fonts => words::KEYS_FONTS,
        View::Sizes => words::KEYS_SIZES,
        View::Settings if app.editing.is_some() => words::KEYS_EDITING,
        View::Settings => words::KEYS_SETTINGS,
        View::Heads => words::KEYS_HEADS,
        View::New => words::KEYS_NEW,
        View::Save => words::KEYS_SAVE,
    };
    let width = app.cols.saturating_sub(2).max(1);
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

/// The screen for `app`: exactly `app.rows` rows, none wider than
/// `app.cols`.
pub fn screen(app: &App) -> Vec<Row> {
    let (cols, rows) = (app.cols, app.rows);
    let mut out: Vec<Row> = Vec::with_capacity(rows);
    if cols < MIN_COLS || rows < MIN_ROWS {
        let name = app.font().map_or("", |f| f.name.as_str());
        out.push(plain(format!("{name}  {}", words::KEYS_SMALL)));
    } else {
        let live = match app.live {
            Live::Asking => words::LIVE_ASKING,
            Live::On => words::LIVE_ON,
            Live::OpsOff | Live::NotVtium => words::LIVE_OFF,
        };
        out.push(plain(sides(
            &format!(" {}", words::TITLE),
            &format!("{live} "),
            cols,
        )));
        out.push(tabs(app));
        out.push(plain("-".repeat(cols)));
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
        // the sentences under the list show whole, while the list keeps
        // at least LIST_MIN rows; otherwise they are dropped
        let show_tail = !tail.is_empty()
            && (list.len().saturating_add(tail.len()) <= room
                || room >= LIST_MIN.saturating_add(tail.len()));
        let list_room = if show_tail {
            room.saturating_sub(tail.len())
        } else {
            room
        };
        let first = keep.saturating_add(1).saturating_sub(list_room);
        out.extend(list.into_iter().skip(first).take(list_room));
        if show_tail {
            out.extend(tail);
        }
        while out.len()
            < rows
                .saturating_sub(1)
                .saturating_sub(message.len())
                .saturating_sub(keys.len())
        {
            out.push(plain(""));
        }
        out.extend(message.into_iter().map(|l| plain(format!(" {l}"))));
        out.push(plain("-".repeat(cols)));
        out.extend(keys.into_iter().map(|l| plain(format!(" {l}"))));
    }
    out.truncate(rows);
    while out.len() < rows {
        out.push(plain(""));
    }
    out.into_iter().map(|row| clip(row, cols)).collect()
}

/// `text` in lines of at most `width` characters, broken between words,
/// at most `most` of them; at least one line.
fn wrap(text: &str, width: usize, most: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    // a word longer than the line, such as a path, breaks where the line
    // ends
    let pieces: Vec<String> = text
        .split(' ')
        .flat_map(|w| {
            let chars: Vec<char> = w.chars().collect();
            if chars.is_empty() {
                vec![String::new()]
            } else {
                chars.chunks(width).map(|c| c.iter().collect()).collect()
            }
        })
        .collect();
    for word in &pieces {
        let len = line.chars().count();
        if len > 0 && len.saturating_add(1).saturating_add(word.chars().count()) > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines.truncate(most.max(1));
    lines
}

/// `text` wrapped as [`wrap`] wraps it, in at most `most` lines, the last
/// ending in `...` where text was left out.
fn cut(text: &str, width: usize, most: usize) -> Vec<String> {
    let mut lines = wrap(text, width, usize::MAX);
    if lines.len() > most {
        lines.truncate(most);
        if let Some(last) = lines.last_mut() {
            let keep = width.saturating_sub(4);
            while last.chars().count() > keep {
                last.pop();
            }
            last.push_str(" ...");
        }
    }
    lines
}

/// `row` cut to `cols` characters.
fn clip(row: Row, cols: usize) -> Row {
    let mut left = cols;
    row.into_iter()
        .filter_map(|s| {
            let text: String = s.text.chars().take(left).collect();
            left = left.saturating_sub(text.chars().count());
            (!text.is_empty()).then_some(Span {
                text,
                reverse: s.reverse,
            })
        })
        .collect()
}

/// The screen as plain text, one line a row, for tests and goldens.
pub fn as_text(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| format!("{}\n", text(r).trim_end()))
        .collect()
}

/// The bytes that draw `rows` whole: each row placed with CUP, written,
/// and erased to its end; reverse video by SGR 7.
pub fn paint(rows: &[Row]) -> Vec<u8> {
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!("\x1b[{};1H", i.saturating_add(1)));
        for span in row {
            // a name read from a file never carries a control into the screen
            let shown: String = span
                .text
                .chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect();
            if span.reverse {
                out.push_str(&format!("\x1b[7m{shown}\x1b[27m"));
            } else {
                out.push_str(&shown);
            }
        }
        out.push_str("\x1b[K");
    }
    out.into_bytes()
}
