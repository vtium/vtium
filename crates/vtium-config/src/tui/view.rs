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
//! font preview, so the user can always get out. Where each thing stands
//! comes from [`super::layout`], which update reads too.

use super::canvas::{Canvas, Row, wrap};
use super::color::{Style, palette};
use super::layout::{
    Body, FLOW_TOP, MESSAGE_ROWS, Tab, body, item_rows, key_line, key_rows, list_window,
    message_rows, tab_places, too_small,
};
use super::model::{App, Live, Then, View};
use super::words;

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
    if let Some(pager) = app.flow.pager() {
        pager_screen(canvas, app, pager, bottom);
        return;
    }
    if let Some((kept, at)) = app.flow.restore() {
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
    if too_small(app.size) {
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
