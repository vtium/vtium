//! The TUI's screen (ADR-1201 D3): the state drawn as rows of text, and
//! the bytes that draw them whole, with ECMA-48's cursor position, erase
//! and SGR sequences only (D7).
//!
//! Every view has a title line, the views' tabs, a rule, its body, a
//! message line, a rule and its keys. A body longer than the room scrolls
//! so the highlighted row shows. A grid too small for that, as a large
//! previewed font can leave, draws one line naming the font and the keys
//! that keep it or go back, so the user can always get out.

use super::model::{App, DpiFrom, FontState, Live, SETTING_KEYS, SIZES, Saving, View};
use super::words;
use crate::draft::changes;

/// The smallest grid the full screen is drawn in: below it, one line.
pub const MIN_COLS: usize = 40;
pub const MIN_ROWS: usize = 10;

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

/// A list row: the name in a column of `width`, then the note.
fn item(highlight: bool, name: &str, width: usize, note: &str) -> String {
    let mark = if highlight { '>' } else { ' ' };
    format!(" {mark} {name:<width$} {note}")
}

/// The body's rows for `app`'s view, and the row to keep in sight.
fn body(app: &App) -> (Vec<Row>, usize) {
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
            let rows = app
                .inputs
                .fonts
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let mut note = match &f.state {
                        FontState::Reads => words::READS.to_owned(),
                        FontState::BuiltIn => words::BUILT_IN.to_owned(),
                        FontState::Refused(why) => words::does_not_read(why),
                    };
                    if app.refused.contains(&f.name) {
                        note = words::KEPT.to_owned();
                    }
                    if app.pending.font_name.as_deref() == Some(&f.name) {
                        note = format!("{note}, {}", words::TO_SAVE);
                    } else if configured.as_deref() == Some(&f.name) {
                        note = format!("{note}, {}", words::SAVED);
                    }
                    let line = item(i == app.font_at, &f.name, width, &note);
                    if i == app.font_at {
                        marked(line)
                    } else {
                        plain(line)
                    }
                })
                .collect();
            (rows, app.font_at)
        }
        View::Sizes => {
            let configured = app
                .configured("font-size")
                .and_then(|s| s.parse::<u32>().ok());
            let mut rows: Vec<Row> = std::iter::once(0)
                .chain(SIZES)
                .enumerate()
                .map(|(i, size)| {
                    let name = match size {
                        0 => words::AUTOMATIC.to_owned(),
                        n => words::points(n),
                    };
                    let note = if app.pending.font_size == Some(size) {
                        words::TO_SAVE
                    } else if configured.unwrap_or(0) == size {
                        words::SAVED
                    } else {
                        ""
                    };
                    let line = item(i == app.size_at, &name, 10, note);
                    if i == app.size_at {
                        marked(line)
                    } else {
                        plain(line)
                    }
                })
                .collect();
            rows.push(plain(""));
            if !app.graphics {
                rows.push(plain(format!(" {}", words::NO_PICTURES)));
            } else if app.size() == 0 {
                rows.push(plain(format!(" {}", words::AUTOMATIC_PICTURE)));
            } else {
                let (dpi, from) = app.inputs.dpi;
                let from = match from {
                    DpiFrom::Conf => words::DPI_FROM_CONF,
                    DpiFrom::Edid => words::DPI_FROM_EDID,
                    DpiFrom::Assumed => words::DPI_ASSUMED,
                };
                rows.push(plain(format!(" {}", words::density(dpi, from))));
            }
            (rows, app.size_at)
        }
        View::Settings => {
            let rows = SETTING_KEYS
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let value = match *key {
                        "font-name" => app.pending.font_name.clone(),
                        "font-size" => app.pending.font_size.map(|s| s.to_string()),
                        _ => None,
                    }
                    .or_else(|| app.configured(key));
                    let value = match *key {
                        "allow-font-ops" if app.font_ops() => words::ON.to_owned(),
                        "allow-font-ops" => words::OFF.to_owned(),
                        _ => value.unwrap_or_else(|| words::UNSET.to_owned()),
                    };
                    let line = item(i == app.setting_at, key, 18, &value);
                    if i == app.setting_at {
                        marked(line)
                    } else {
                        plain(line)
                    }
                })
                .chain([plain(""), plain(format!(" {}", words::SETTINGS_HINT))])
                .collect();
            (rows, app.setting_at)
        }
        View::Heads => match (app.inputs.heads.is_some(), app.heads()) {
            (false, _) => (vec![plain(format!(" {}", words::NO_HEAD_FILE))], 0),
            (true, None) => (vec![plain(format!(" {}", words::HEAD_FILE_ERRORS))], 0),
            (true, Some(heads)) => {
                let rows = heads
                    .iter()
                    .enumerate()
                    .map(|(i, (head, profile))| {
                        let line = item(i == app.head_at, head, 16, &words::head_profile(profile));
                        if i == app.head_at {
                            marked(line)
                        } else {
                            plain(line)
                        }
                    })
                    .chain([plain(""), plain(format!(" {}", words::HEADS_HINT))])
                    .collect();
                (rows, app.head_at)
            }
        },
        View::Save => (save_rows(app), 0),
    }
}

/// The save view: each file the save changes, each line that changes,
/// and the one command it runs.
fn save_rows(app: &App) -> Vec<Row> {
    let plan = match app.plan() {
        Ok(plan) => plan,
        Err(why) => return vec![plain(format!(" {}", words::save_failed(why)))],
    };
    if plan.is_empty() {
        return vec![plain(format!(" {}", words::NOTHING_TO_SAVE))];
    }
    let mut rows = Vec::new();
    for file in &plan.files {
        rows.push(plain(format!(" {}", file.path.display())));
        for c in changes(file.old.as_deref().unwrap_or_default(), &file.new) {
            if let Some(old) = &c.old {
                rows.push(plain(format!("   {:>4} - {old}", c.line)));
            }
            if let Some(new) = &c.new {
                rows.push(plain(format!("   {:>4} + {new}", c.line)));
            }
        }
    }
    if !app.report.is_empty() {
        rows.push(plain(""));
        rows.push(plain(format!(" {}", words::COMMAND_SAID)));
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
        Saving::Root => say(words::SAVE_COMMAND_ROOT, " "),
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

/// Where the size view's picture goes, row and column from 1: beside the
/// list of sizes, below the rule under the tabs.
pub fn picture_at(_app: &App) -> (usize, usize) {
    (4, 26)
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
        let mut tabs = vec![Span {
            text: " ".into(),
            reverse: false,
        }];
        for (v, name) in
            View::TABS
                .iter()
                .zip([words::FONTS, words::SIZES, words::SETTINGS, words::HEADS])
        {
            tabs.push(Span {
                text: format!(" {name} "),
                reverse: *v == app.view,
            });
            tabs.push(Span {
                text: " ".into(),
                reverse: false,
            });
        }
        if app.view == View::Save {
            tabs.push(Span {
                text: format!(" {} ", words::SAVE),
                reverse: true,
            });
        }
        out.push(tabs);
        out.push(plain("-".repeat(cols)));
        let message = wrap(
            app.message.as_deref().unwrap_or(""),
            cols.saturating_sub(2),
            3,
        );
        let room = rows.saturating_sub(5).saturating_sub(message.len());
        let (lines, keep) = body(app);
        let first = keep.saturating_add(1).saturating_sub(room);
        out.extend(lines.into_iter().skip(first).take(room));
        while out.len() < rows.saturating_sub(2).saturating_sub(message.len()) {
            out.push(plain(""));
        }
        out.extend(message.into_iter().map(|l| plain(format!(" {l}"))));
        out.push(plain("-".repeat(cols)));
        let keys = match app.view {
            View::Fonts => words::KEYS_FONTS,
            View::Sizes => words::KEYS_SIZES,
            View::Settings => words::KEYS_SETTINGS,
            View::Heads => words::KEYS_HEADS,
            View::Save => words::KEYS_SAVE,
        };
        out.push(plain(format!(" {keys}")));
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
