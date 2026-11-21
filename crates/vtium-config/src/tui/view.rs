//! The TUI's screen: the model drawn into ratatui's buffer, whose Backend
//! writes ECMA-48's cursor position, erase and SGR sequences only.
//!
//! Every view has the tab bar, with the views and the actions, and the
//! status line. The Flow view then draws its document of paths and parts,
//! or the file view or Restore's list over it ([`super::flowdraw`]). The
//! other views draw a rule, their body, a message line, a rule and their
//! keys; a list longer than the room scrolls so the highlighted row shows.
//! Below 80 by 25 cells no view is drawn: one screen says the size the TUI
//! needs, the commands that work at any size, and the keys that leave and
//! that undo a font preview, so the user can always get out. Where each
//! thing stands comes from [`super::layout`], which update reads too.
//!
//! ratatui's widgets draw the tab bar, the rules, the key line, the Help
//! page, the tabs' lists and Restore's list. Text that can hold any
//! character, such as a font's name, its specimen, a line of the draft,
//! the status line, the message and the screen below the minimum, is
//! written by the console's width rule ([`fill`]), and so is each row of a
//! list that holds anything but ASCII. The Flow view's
//! layers are rows of such text, and its document of paths is a picture of
//! box-drawing lines that [`super::flowdraw`] draws cell by cell, so both
//! go through [`fill`] too.
//!
//! References:
//! - ADR-1201 D3 and ADR-1403 D7: the views, the minimum size and the
//!   screen below it.
//! - ADR-1206 D1: ratatui's core and widgets draw the TUI.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::text::{Line, Span};
use ratatui_core::widgets::Widget as _;
use ratatui_widgets::paragraph::Paragraph;

use super::canvas::{Row, cells, printable, span, wrap};
use super::color::{Colors, Style, palette};
use super::layout::{
    FLOW_TOP, Tab, body, item_rows, key_line, key_rows, list_window, message_rows, tab_places,
    too_small,
};
use super::listbox::Body;
use super::model::{App, Live, Size, Then, View};
use super::words;

/// The row's text, without its styles.
pub fn text(row: &Row) -> String {
    row.iter().map(|s| s.text.as_str()).collect()
}

/// `left` and `right` on one line of `cols` cells, `right` against the
/// edge.
fn sides(left: &str, right: &str, cols: usize) -> String {
    let (l, r) = (cells(left), cells(right));
    let gap = cols.saturating_sub(l.saturating_add(r)).max(1);
    format!("{left}{}{right}", " ".repeat(gap))
}

/// A span of `text` in `style`, its colors read as `colors` says. A
/// control character shows as `?`.
fn styled(text: impl AsRef<str>, style: Style, colors: Colors) -> Span<'static> {
    Span::styled(printable(text.as_ref()), style.in_cells(colors))
}

/// A line of plain `text`.
fn plain(text: impl AsRef<str>) -> Line<'static> {
    Line::from(styled(text, Style::PLAIN, Colors::Sent))
}

/// The `height` rows of `area` from its row `y`, as far as its bottom.
fn band(area: Rect, y: usize, height: usize) -> Rect {
    let y = u16::try_from(y).unwrap_or(u16::MAX).min(area.height);
    let height = u16::try_from(height)
        .unwrap_or(u16::MAX)
        .min(area.height.saturating_sub(y));
    Rect::new(area.x, area.y.saturating_add(y), area.width, height)
}

/// The tab bar, row 0: the views on the left, the current one in reverse
/// video, and the actions against the right edge.
fn tab_bar(app: &App, colors: Colors) -> Line<'static> {
    let mut spans = Vec::new();
    let mut at = 0usize;
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
        if x > at {
            spans.push(Span::raw(" ".repeat(x.saturating_sub(at))));
        }
        at = x.saturating_add(text.chars().count());
        spans.push(styled(text, style, colors));
    }
    Line::from(spans)
}

/// The status line, row 1: how the draft stands and, in the Flow view, the
/// tags; at the right, how a change is previewed and whether `t` tries
/// a font live.
fn status(app: &App) -> String {
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
            .draft
            .as_ref()
            .map_or(0, |d| d.frit_changes.len());
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
    let pictures = app.shared.graphics && !app.shared.text_preview();
    // the open tab's own node, under any question over it, since the
    // status line says what t does in the tab
    let trial_node = app.tab().node();
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
            // t tries a font set on a look or a font set of the Looks tab
            // and in a look's card; elsewhere in the Looks tab t does
            // nothing, and the other tabs say where the trial is
            match (*live == Live::On, app.nav.view) {
                (true, View::Looks) if matches!(trial_node, "looks" | "looks.look") => {
                    words::TRIAL_KEY
                }
                (true, View::Looks) => words::TRIAL_ON_FONT_SETS,
                (true, _) => words::TRIAL_IN_LOOKS,
                (false, _) => words::TRIAL_NONE,
            }
        ),
    };
    // the room the left side has beside the right side, with the gap
    // between them and the margin after
    let room = |live: &str| app.size.cols.saturating_sub(cells(live)).saturating_sub(3);
    // the long words where they fit, else the short words for the font
    // sets waiting, else the short words on the right as well; only then
    // is the left side cut
    let long = format!(" {}", left_with(&words::frits_waiting));
    let short = format!(" {}", left_with(&words::frits_waiting_short));
    let fits = |left: &str, live: &str| cells(left) <= room(live);
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
    sides(&left, &format!("{live} "), app.size.cols)
}

/// The screen below the minimum size, a line a row.
fn small(app: &App) -> Vec<String> {
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
    lines
}

/// The Flow view's screens that take the paths' place: the file view
/// and Restore's list of the installed history, over the message and the
/// key line. The message can hold a font's name, so it is written by the
/// console's width rule.
fn flow_screen(app: &App, colors: Colors, buf: &mut Buffer) {
    let area = buf.area;
    let rows = app.size.rows;
    let keys = key_rows(app);
    let message: Vec<Row> = super::layout::flow_message_rows(app)
        .into_iter()
        .map(|line| vec![span(format!(" {line}"), Style::PLAIN)])
        .collect();
    let bottom = super::layout::flow_bottom(app);
    fill(buf, band(area, bottom, message.len()), &message, colors);
    let key_lines: Vec<Line> = keys
        .iter()
        .map(|l| Line::from(styled(format!(" {l}"), Style::text(palette::GREY), colors)))
        .collect();
    Paragraph::new(key_lines).render(band(area, rows.saturating_sub(keys.len()), rows), buf);
    match app.flow.pager() {
        Some(pager) => pager_screen(app, pager, bottom, colors, buf),
        None => restore_list(app, colors, buf),
    }
}

/// Restore's list of the installed history under its title, drawn by
/// ratatui's List from a copy of the Flow tab's list state. The window
/// ends on the row above the message, so the entries never cover the
/// message or the key line. A row whose file name holds anything but
/// ASCII is written again by the console's width rule.
///
/// References:
/// - ADR-1206 D4, "A list's selection lives in the tab's state": the view
///   draws from a copy, so drawing never changes the model.
fn restore_list(app: &App, colors: Colors, buf: &mut Buffer) {
    use ratatui_core::widgets::StatefulWidget;
    use ratatui_widgets::list::{List, ListItem, ListState};
    let Some((kept, list)) = app.flow.restore() else {
        return;
    };
    let (room, _) = super::layout::restore_window(app);
    let inside = Rect {
        x: 1,
        width: buf.area.width.saturating_sub(1),
        ..buf.area
    };
    let title = Style {
        bold: true,
        ..Style::PLAIN
    };
    Paragraph::new(Line::from(styled(words::RESTORE_TITLE, title, colors)))
        .render(band(inside, FLOW_TOP, 1), buf);
    let rows: Vec<Row> = kept
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let style = if i == list.at {
                Style::PLAIN.reversed()
            } else {
                Style::PLAIN
            };
            vec![span(
                format!(" {:>2}  {} ", i.saturating_add(1), k.name),
                style,
            )]
        })
        .collect();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .iter()
                .map(|s| styled(&s.text, s.style, colors))
                .collect();
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default()
        .with_offset(list.offset)
        .with_selected(Some(list.at));
    let area = band(inside, FLOW_TOP.saturating_add(1), room);
    StatefulWidget::render(List::new(items), area, buf, &mut state);
    // an installed file's name can hold any character
    rewrite_apart(buf, area, &rows, state.offset(), colors);
}

/// The draft in the pager, from row 2 down to `bottom`: its title, then
/// each line with its number, a line the check complains about marked `!`
/// in bright red, and the line of the card selected in the flow marked
/// with [`SELECTED`]. The title and the lines are the draft's own text,
/// so they are written by the console's width rule.
fn pager_screen(
    app: &App,
    pager: super::model::Pager,
    bottom: usize,
    colors: Colors,
    buf: &mut Buffer,
) {
    let Some(draft) = &app.shared.draft else {
        return;
    };
    let area = buf.area;
    let inside = |x: u16, y: usize| {
        let y = u16::try_from(y).unwrap_or(u16::MAX);
        Rect::new(x, y, area.width.saturating_sub(x), 1)
    };
    let title = format!("{} · draft {}", words::PAGER_TITLE, draft.name);
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    fill(buf, inside(1, 2), &[vec![span(title, bold)]], colors);
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
        // the number takes the six columns before the line, and a number
        // wider than that is cut where the line starts
        let number = inside(0, row).intersection(Rect::new(0, 0, 6, area.height));
        fill(
            buf,
            number,
            &[vec![span(format!("{mark}{n:>4} "), style)]],
            colors,
        );
        fill(
            buf,
            inside(6, row),
            &[vec![span(shown, Style::PLAIN)]],
            colors,
        );
    }
}

/// The mark the file view puts before the line of the highlighted box.
pub const SELECTED: &str = "▸";

/// The screen as it would be in a terminal of `size`, with no picture on
/// it and no caption beside one: what a picture of the console shows. It
/// draws a copy of the model at that size, so the model keeps its own size.
pub fn screen_at(app: &App, size: Size) -> Vec<Row> {
    let _rule = app.shared.measure();
    let mut frame = app.clone();
    frame.size = size;
    frame.shared.picture = None;
    frame.shared.shot = None;
    // the copy's lists keep their highlight in view at the copy's size
    frame.lists_settle();
    screen(&frame)
}

/// The screen for `app`, read back from the cells the view draws:
/// exactly `app.rows` rows, none wider than `app.cols`. The cells are
/// drawn with [`Colors::Named`], so a style keeps the named color that the
/// pictures of the console draw, where the terminal is sent a 24-bit one.
pub fn screen(app: &App) -> Vec<Row> {
    let _rule = app.shared.measure();
    let area = ratatui_core::layout::Rect::new(
        0,
        0,
        u16::try_from(app.size.cols).unwrap_or(u16::MAX),
        u16::try_from(app.size.rows).unwrap_or(u16::MAX),
    );
    let mut buf = ratatui_core::buffer::Buffer::empty(area);
    view(app, Colors::Named, &mut buf);
    rows(&buf)
}

/// The rows of `buf`, each run of cells in one style a span. A cell that
/// a wide character covers, by the console's width rule, is the
/// character's and adds nothing to the row.
pub fn rows(buf: &ratatui_core::buffer::Buffer) -> Vec<Row> {
    let area = buf.area;
    (area.top()..area.bottom())
        .map(|y| {
            let mut row: Row = Vec::new();
            let mut x = area.left();
            while x < area.right() {
                let cell = &buf[(x, y)];
                let style = Style::of_cell(cell.style());
                match row.last_mut() {
                    Some(s) if s.style == style => s.text.push_str(cell.symbol()),
                    _ => row.push(span(cell.symbol(), style)),
                }
                x = x.saturating_add(super::backend::console_width(cell.symbol()).max(1));
            }
            row
        })
        .collect()
}

/// Draws `app`'s screen into `buf`, its colors read as `colors` says.
/// ratatui draws the tab bar, the rules, the key line, the Help page and
/// the lists. The status line, the message, the screen below the minimum,
/// and the Flow view's layers and its document of paths are written by the
/// console's width rule.
fn view(app: &App, colors: Colors, buf: &mut Buffer) {
    let area = buf.area;
    let (cols, rows) = (app.size.cols, app.size.rows);
    if too_small(app.size) {
        let lines: Vec<Row> = small(app)
            .into_iter()
            .map(|l| vec![span(l, Style::PLAIN)])
            .collect();
        fill(buf, area, &lines, colors);
        return;
    }
    Paragraph::new(tab_bar(app, colors)).render(band(area, 0, 1), buf);
    if super::flowlayout::shown(app) {
        super::flowdraw::render(app, colors, buf);
        return;
    }
    let status = vec![span(status(app), Style::PLAIN)];
    fill(buf, band(area, 1, 1), &[status], colors);
    if app.nav.view == View::Flow {
        flow_screen(app, colors, buf);
        return;
    }
    let rule = || plain("-".repeat(cols));
    Paragraph::new(rule()).render(band(area, 2, 1), buf);
    let message = message_rows(app);
    let keys = key_rows(app);
    let foot = rows
        .saturating_sub(1)
        .saturating_sub(message.len())
        .saturating_sub(keys.len());
    let body_area = band(area, 3, foot.saturating_sub(3));
    if let Some((page, top)) = app.tab().page(&app.shared, app.size) {
        let (room, _) = list_window(app, page.len(), 0);
        let first = top.min(page.len().saturating_sub(room));
        Paragraph::new(page)
            .scroll((u16::try_from(first).unwrap_or(u16::MAX), 0))
            .render(band(area, 3, room), buf);
    } else {
        list_body(app, colors, buf, body_area);
        preview_text(app, colors, buf, body_area);
    }
    // the message can hold a font's name or a device's, so it is written
    // by the console's width rule
    let message_lines: Vec<Row> = message
        .iter()
        .map(|l| vec![span(format!(" {l}"), Style::PLAIN)])
        .collect();
    fill(buf, band(area, foot, message.len()), &message_lines, colors);
    let foot_lines: Vec<Line> = std::iter::once(rule())
        .chain(keys.iter().map(|l| plain(format!(" {l}"))))
        .collect();
    Paragraph::new(foot_lines).render(band(area, foot.saturating_add(message.len()), rows), buf);
}

/// A tab's body: its list, drawn by ratatui's List from a copy of the
/// tab's list state, and the sentences under the list where they fit.
///
/// Each row of the list is an item of the List, so the window can start
/// partway through an item that takes several rows, as the layout's
/// offset says. Each row is drawn in the styles its spans carry, so the
/// highlight covers what the layout marks and no more. ratatui lays a
/// row's text out by Unicode's widths and clusters, so a row holding
/// anything but ASCII is written again by the console's rule.
/// The sentences, which can hold a font's specimen, are written by the
/// console's rule too.
///
/// References:
/// - ADR-1206 D4, "A list's selection lives in the tab's state": the view
///   draws from a copy, so drawing never changes the model.
/// - ADR-1206 D1, "The console's width rule decides where the cursor
///   goes".
fn list_body(app: &App, colors: Colors, buf: &mut Buffer, area: Rect) {
    use ratatui_core::widgets::StatefulWidget;
    use ratatui_widgets::list::{List, ListItem, ListState};
    let Body { list, keep, tail } = body(app);
    let (room, show_tail) = list_window(app, list.len(), tail.len());
    let list_area = band(area, 0, room);
    let items: Vec<ListItem> = list
        .iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .iter()
                .map(|s| styled(&s.text, s.style, colors))
                .collect();
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default()
        .with_offset(app.list_offset())
        .with_selected(Some(keep));
    StatefulWidget::render(List::new(items), list_area, buf, &mut state);
    let first = state.offset();
    let shown = list.len().saturating_sub(first).min(room);
    rewrite_apart(buf, list_area, &list, first, colors);
    if show_tail {
        fill(buf, band(area, shown, tail.len()), &tail, colors);
    }
}

/// In a vtium console, which previews a font by drawing the console in
/// it, the preview's text where a picture would go: as many lines as the
/// picture's area holds, each cut at the screen's edge, over the body in
/// `area`. The text can hold any character of a font's specimen, so it is
/// written by the console's width rule.
fn preview_text(app: &App, colors: Colors, buf: &mut Buffer, area: Rect) {
    if !app.shared.text_preview() {
        return;
    }
    let (row, col) = super::layout::picture_at(app);
    let (_, rows) = super::layout::picture_area(app);
    let top = u16::try_from(row.saturating_sub(1)).unwrap_or(u16::MAX);
    let left = u16::try_from(col.saturating_sub(1)).unwrap_or(u16::MAX);
    for (y, line) in (top..).zip(super::console::lines(app).into_iter().take(rows)) {
        let row: Row = line
            .into_iter()
            .map(|(text, style)| span(text, style))
            .collect();
        let place = Rect::new(left, y, buf.area.width.saturating_sub(left), 1).intersection(area);
        fill(buf, place, &[row], colors);
    }
}

/// Writes again, by the console's width rule, each row of `list` from its
/// row `first` that ratatui's List drew in `area` and might have laid out
/// in other cells than the console does.
fn rewrite_apart(buf: &mut Buffer, area: Rect, list: &[Row], first: usize, colors: Colors) {
    for (i, row) in list.iter().enumerate().skip(first) {
        let line = band(area, i.saturating_sub(first), 1);
        if line.height == 0 {
            break;
        }
        if measured_apart(row) {
            buf.set_style(line, ratatui_core::style::Style::reset());
            for x in line.left()..line.right() {
                buf[(x, line.y)].reset();
            }
            fill(buf, line, std::slice::from_ref(row), colors);
        }
    }
}

/// Whether ratatui might lay `row` out in other cells than the console
/// does: whether the row holds anything but ASCII. ratatui measures a
/// character by Unicode's tables and joins marks by Unicode's clusters,
/// and the console has its own width rule and its own clusters, so only
/// plain ASCII text is sure to take the same cells under both.
fn measured_apart(row: &Row) -> bool {
    row.iter().any(|s| !s.text.is_ascii())
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

/// Writes `rows` into `area` of `buf` from its top left, their colors read
/// as `colors` says, as the terminal lays them out. Each character the
/// console shows, a base character with the marks that join it, goes in
/// one cell of the buffer, in the style of its base, and takes as many
/// cells as the console's width rule gives it. A character that ratatui
/// measures differently keeps the console's width in the buffer, so
/// ratatui's diff steps over the same cells the terminal's cursor does.
/// A frame that changes a mark writes the whole character again, since
/// the mark is in the character's cell.
///
/// References:
/// - ADR-1206 D1, "The console's width rule decides where the cursor
///   goes".
pub fn fill(buf: &mut Buffer, area: Rect, rows: &[Row], colors: Colors) {
    use ratatui_core::buffer::{CellDiffOption, CellWidth};
    let area = area.intersection(buf.area);
    for (y, row) in (area.top()..area.bottom()).zip(rows) {
        // a name read from a file never carries a control onto the screen
        let cells: Vec<super::color::SgrCell> = row
            .iter()
            .flat_map(|s| -> Vec<super::color::SgrCell> {
                if s.raw {
                    super::color::sgr_cells(&s.text)
                } else {
                    let style = s.style.in_cells(colors);
                    s.text.chars().map(|c| (c, style)).collect()
                }
            })
            .map(|(c, style)| (if c.is_control() { '?' } else { c }, style))
            .collect();
        let text: String = cells.iter().map(|(c, _)| *c).collect();
        let mut x = area.left();
        let mut at = 0usize;
        for cluster in super::canvas::clusters(&text) {
            let Some(&(_, style)) = cells.get(at) else {
                break;
            };
            at = at.saturating_add(cluster.chars);
            let (symbol, width) = super::canvas::shown(&cluster);
            if x.saturating_add(width) > area.right() {
                break;
            }
            let cell = &mut buf[(x, y)];
            cell.set_symbol(&symbol).set_style(style);
            if symbol.cell_width() != width
                && let Some(w) = std::num::NonZeroU16::new(width)
            {
                cell.set_diff_option(CellDiffOption::ForcedWidth(w));
            }
            // the cell that a wide character covers holds nothing of its
            // own
            for covered in 1..width {
                buf[(x.saturating_add(covered), y)].reset();
            }
            x = x.saturating_add(width);
        }
    }
}

/// Draws `app`'s screen into ratatui's frame, in the colors the terminal
/// is sent. The cells under the picture on the screen, if one shows, are
/// marked to skip, so the frame writes nothing over the picture: a
/// terminal that draws a picture on the text's cells loses the part of the
/// picture that text is written over.
///
/// References:
/// - ADR-1206 D5, "ratatui draws what changed": the cells under a kitty
///   graphics picture are marked to skip.
pub fn render(app: &App, frame: &mut ratatui_core::terminal::Frame) {
    let buf = frame.buffer_mut();
    view(app, Colors::Sent, buf);
    if let Some(covered) = app.shared.picture {
        let rect = Rect::new(
            u16::try_from(covered.col).unwrap_or(u16::MAX),
            u16::try_from(covered.row).unwrap_or(u16::MAX),
            u16::try_from(covered.cols).unwrap_or(u16::MAX),
            u16::try_from(covered.rows).unwrap_or(u16::MAX),
        )
        .intersection(buf.area);
        for position in rect.positions() {
            buf[position].set_diff_option(ratatui_core::buffer::CellDiffOption::Skip);
        }
    }
}

/// The terminal as ratatui last drew it, so that each frame writes only
/// the cells that changed. The loop keeps one for the whole run.
pub struct Painter {
    terminal: ratatui_core::terminal::Terminal<super::backend::Vt>,
}

impl Painter {
    pub fn new() -> std::io::Result<Painter> {
        Ok(Painter {
            terminal: ratatui_core::terminal::Terminal::new(super::backend::Vt::new(0, 0, true))?,
        })
    }

    /// The bytes that bring the terminal from the last frame to `app`'s
    /// screen. A new size clears the screen and draws the frame whole.
    pub fn frame(&mut self, app: &App) -> std::io::Result<Vec<u8>> {
        let cols = u16::try_from(app.size.cols).unwrap_or(u16::MAX);
        let rows = u16::try_from(app.size.rows).unwrap_or(u16::MAX);
        let backend = self.terminal.backend_mut();
        backend.set_size(cols, rows);
        backend.set_color(app.shared.inputs.color);
        let _rule = app.shared.measure();
        self.terminal.draw(|frame| render(app, frame))?;
        Ok(self.terminal.backend_mut().take())
    }

    /// Forgets what the screen showed, so the next frame draws whole, and
    /// returns the bytes that clear the screen: after another program
    /// wrote to the terminal, or after a picture left it. The loop writes
    /// them at once, before anything else it writes, because a clear that
    /// waited for the next frame would erase a picture shown meanwhile.
    pub fn forget(&mut self) -> std::io::Result<Vec<u8>> {
        self.terminal.clear()?;
        Ok(self.terminal.backend_mut().take())
    }
}

/// The bytes that draw `app`'s screen whole, on a terminal that shows
/// nothing yet.
pub fn draw(app: &App) -> Vec<u8> {
    Painter::new()
        .and_then(|mut p| p.frame(app))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;

    use super::super::canvas::span;
    use super::*;

    /// The rows a vtium console shows after `bytes`, on a screen of
    /// `cols` by `rows`, the second cell of a wide character as a blank.
    fn console(bytes: &[u8], cols: u16, rows: u16) -> Vec<String> {
        console_by(bytes, cols, rows, vtium_term::ClusterWidth::Double)
    }

    /// [`console`] on a console whose `cluster-width` is `rule`.
    fn console_by(
        bytes: &[u8],
        cols: u16,
        rows: u16,
        rule: vtium_term::ClusterWidth,
    ) -> Vec<String> {
        let mut t = vtium_term::Terminal::new(usize::from(cols), usize::from(rows), 0);
        t.set_cluster_width(rule);
        t.input(bytes);
        let s = t.screen();
        (0..s.rows())
            .map(|r| {
                let Some(line) = s.line(r) else {
                    return String::new();
                };
                line.cells()
                    .iter()
                    .map(|&c| match line.glyph(c) {
                        vtium_term::Glyph::Char(ch) => ch.to_string(),
                        vtium_term::Glyph::Cluster(s) => s.to_owned(),
                        vtium_term::Glyph::Empty => " ".to_owned(),
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// The bytes ratatui's Terminal writes through the Backend for each of
    /// `texts` in turn, each written on the first row of a frame of `cols`
    /// by `rows` cells by [`fill`].
    fn frames(cols: u16, rows: u16, texts: &[&str]) -> Vec<Vec<u8>> {
        let backend = super::super::backend::Vt::new(cols, rows, true);
        let Ok(mut t) = ratatui_core::terminal::Terminal::new(backend) else {
            return Vec::new();
        };
        texts
            .iter()
            .map(|text| {
                let row = vec![span(*text, Style::PLAIN)];
                let _ = t.draw(|f| {
                    let buf = f.buffer_mut();
                    let area = Rect::new(0, 0, buf.area.width, 1);
                    fill(buf, area, &[row], Colors::Sent);
                });
                t.backend_mut().take()
            })
            .collect()
    }

    #[test]
    fn text_written_by_fill_shows_on_the_console_as_the_same_text_written_plainly() {
        // é as e and a combining accent, a Tamil consonant with its
        // virama, a vowel sign after a space, which the console joins to
        // the space in a character two cells wide, and a vowel sign on its
        // consonant
        for text in ["e\u{301}x", "க்ஷa", "ab ொ.", "ab கொ."] {
            let drawn = frames(12, 1, &[text]);
            let first = drawn.first().cloned().unwrap_or_default();
            assert_eq!(
                console(&first, 12, 1),
                console(text.as_bytes(), 12, 1),
                "{text}: {:?}",
                String::from_utf8_lossy(&first)
            );
        }
    }

    #[test]
    fn the_status_line_and_the_message_are_laid_out_by_the_console_s_width_rule() {
        // U+17A4 takes two cells by Unicode's tables and one by the
        // console's rule; a draft's name and a message can hold any
        // character
        let mut inputs = super::super::fixture::inputs();
        inputs.flow.draft = Some(super::super::flow::Draft {
            name: "n\u{17a4}m".to_owned(),
            changes: 0,
            problems: 0,
        });
        let (mut app, _) = App::new(inputs, 80, 25);
        app.shared.message = Some("say a\u{17a4}b".to_owned());
        let shown = console(&draw(&app), 80, 25);
        let status = shown.get(1).cloned().unwrap_or_default();
        assert!(status.starts_with(" draft n\u{17a4}m · "), "{status:?}");
        assert!(
            shown.iter().any(|row| row == " say a\u{17a4}b"),
            "{shown:#?}"
        );
    }

    #[test]
    fn under_cluster_width_single_a_frame_after_a_vowel_sign_lands_where_the_console_shows_it() {
        // a vowel sign after a space joins the space: two cells under
        // `double`, one under `single`, which the installed vtium.conf
        // sets. The second frame writes only the changed `Y`, at the cell
        // where the console shows it.
        let mut inputs = super::super::fixture::inputs();
        inputs.conf = Some("cluster-width = single\n".to_owned());
        let (mut app, _) = App::new(inputs, 80, 25);
        let mut painter = Painter::new().ok();
        let mut frame = |app: &App| {
            painter
                .as_mut()
                .and_then(|p| p.frame(app).ok())
                .unwrap_or_default()
        };
        app.shared.message = Some("ab \u{BCA}.xyz".to_owned());
        let mut bytes = frame(&app);
        app.shared.message = Some("ab \u{BCA}.xYz".to_owned());
        bytes.extend(frame(&app));
        let shown = console_by(&bytes, 80, 25, vtium_term::ClusterWidth::Single);
        assert!(
            shown.iter().any(|row| row == " ab \u{BCA}.xYz"),
            "{shown:#?}"
        );
        // the screen the tests read lays the row out by the same rule: the
        // vowel sign's character takes one cell, so the row's text, read
        // back cell by cell, fills the 80 cells under `single`
        let rows: Vec<String> = screen(&app)
            .iter()
            .map(|r| r.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect();
        let row = rows
            .iter()
            .find(|r| r.contains('Y'))
            .cloned()
            .unwrap_or_default();
        let _single = super::super::canvas::measure_by(vtium_term::ClusterWidth::Single);
        assert_eq!(super::super::canvas::cells(&row), 80, "{row:?}");
    }

    #[test]
    fn a_frame_that_changes_only_a_vowel_sign_writes_its_whole_character_again() {
        // the console draws கா and கொ each as one character two cells
        // wide; a frame that wrote the new sign alone into the second cell
        // would erase the consonant
        let drawn = frames(10, 1, &["ab கா.", "ab கொ."]);
        let both: Vec<u8> = drawn.concat();
        assert_eq!(
            console(&both, 10, 1),
            console("ab கொ.".as_bytes(), 10, 1),
            "{:?}",
            drawn
                .iter()
                .map(|d| String::from_utf8_lossy(d))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_row_is_laid_out_by_the_console_s_width_rule_and_its_diff_steps_the_same_cells() {
        // U+17A4 takes one cell by the console's rule and two by Unicode's
        // tables, and 日 takes two by both
        let rows = vec![vec![span("a\u{17a4}b日c", Style::PLAIN)]];
        let mut next = Buffer::empty(Rect::new(0, 0, 8, 1));
        let area = next.area;
        fill(&mut next, area, &rows, Colors::Sent);
        let at = |b: &Buffer, x: u16| b[(x, 0)].symbol().to_owned();
        assert_eq!(
            (0..6).map(|x| at(&next, x)).collect::<Vec<_>>(),
            ["a", "\u{17a4}", "b", "日", " ", "c"]
        );
        // a diff against a blank screen writes `b`, which a width of two
        // for the vowel would step over, and steps over the cell 日 covers
        let blank = Buffer::empty(next.area);
        let written: Vec<(u16, String)> = blank
            .diff(&next)
            .into_iter()
            .map(|(x, _, c)| (x, c.symbol().to_owned()))
            .collect();
        assert_eq!(
            written,
            [
                (0, "a".to_owned()),
                (1, "\u{17a4}".to_owned()),
                (2, "b".to_owned()),
                (3, "日".to_owned()),
                (5, "c".to_owned())
            ]
        );
    }

    #[test]
    fn a_wide_character_takes_two_cells_and_never_crosses_the_edge() {
        // 日 takes two cells by the console's width rule, so the second
        // one would end past the fifth cell and is left out
        let rows = vec![vec![span("a日b日", Style::PLAIN)]];
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
        let area = buf.area;
        fill(&mut buf, area, &rows, Colors::Named);
        let at = |x: u16| buf[(x, 0)].symbol().to_owned();
        assert_eq!(
            (0..5).map(at).collect::<Vec<_>>(),
            ["a", "日", " ", "b", " "]
        );
        assert_eq!(super::rows(&buf), [vec![span("a日b ", Style::PLAIN)]]);
    }

    #[test]
    fn rows_read_back_from_the_cells_are_the_rows_drawn_into_them() {
        // U+17A4 takes one cell by the console's rule, and 日 takes two
        let grey = Style::text(palette::GREY);
        let rows = vec![
            vec![
                span("a\u{17a4}", Style::PLAIN.reversed()),
                span("日b", grey),
                span(" ", Style::PLAIN),
            ],
            vec![
                span("cd", Style::block(super::super::color::Color::Cyan)),
                span("    ", Style::PLAIN),
            ],
        ];
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 2));
        let area = buf.area;
        fill(&mut buf, area, &rows, Colors::Named);
        assert_eq!(super::rows(&buf), rows);
    }

    /// The text of `app`'s screen, a string a row.
    fn texts(app: &App) -> Vec<String> {
        screen(app)
            .iter()
            .map(|r| r.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn under_cluster_width_single_update_keeps_the_list_window_the_view_draws() {
        // each font file's name holds a space with a vowel sign after it,
        // which takes one cell more under `double` than under `single`;
        // the five fonts fill the Fonts tab's window, and under `single`
        // the list's window starts at the filter line wherever the TUI
        // lays the list out: at start, after a key, and in a picture of
        // the TUI
        let name = |c: char| format!("{c}{} \u{BCA}.ttf", "x".repeat(18));
        let mut inputs = super::super::fixture::inputs();
        inputs.conf = Some("cluster-width = single\n".to_owned());
        inputs.font_files = super::super::fonts_found::Fonts::new(
            ('a'..='e')
                .map(|c| crate::discover::Found {
                    path: std::path::PathBuf::from(format!("/usr/share/fonts/{}", name(c))),
                    folder: crate::discover::Folder {
                        path: "/usr/share/fonts".into(),
                        place: crate::discover::Place::System,
                    },
                    trust: Ok(()),
                    state: crate::batch::State::Inactive,
                    duplicate: None,
                })
                .collect(),
        );
        let all_shown = |shown: &[String], when: &str| {
            for c in 'a'..='e' {
                assert!(
                    shown.iter().any(|r| r.contains(&name(c))),
                    "{when}: {}: {shown:#?}",
                    name(c)
                );
            }
        };
        let (mut app, _) = App::new(inputs, 80, 25);
        all_shown(&texts(&app), "at start");
        app.event(super::super::model::Event::Input(
            super::super::input::Input::Key(super::super::input::Key::End),
        ));
        assert_eq!(super::super::tabs::Tab::offset(&app.fonts), 0);
        all_shown(&texts(&app), "after End");
        let shot: Vec<String> = screen_at(&app, app.size)
            .iter()
            .map(|r| r.iter().map(|s| s.text.as_str()).collect())
            .collect();
        all_shown(&shot, "in a picture of the TUI");
    }

    #[test]
    fn after_an_install_the_tui_measures_by_the_rule_the_console_started_with() {
        // the console reads `cluster-width` when vtium starts, so an
        // install that sets it to `double` leaves the console on `single`
        // until vtium restarts, and the TUI keeps measuring by `single`
        let mut inputs = super::super::fixture::inputs();
        inputs.conf = Some("cluster-width = single\n".to_owned());
        let (mut app, _) = App::new(inputs, 80, 25);
        let installed = super::super::desk::Opened {
            name: "vtium.conf".into(),
            text: "cluster-width = double\n".into(),
            ..Default::default()
        };
        app.event(super::super::model::Event::Installed(Ok(
            super::super::desk::Done::Draft(installed, None),
        )));
        assert_eq!(app.configured("cluster-width").as_deref(), Some("double"));
        app.shared.message = Some("ab \u{BCA}.xYz".to_owned());
        let row = texts(&app)
            .into_iter()
            .find(|r| r.contains('Y'))
            .unwrap_or_default();
        // read back cell by cell, the row fills the 80 cells only when the
        // vowel sign's character took one cell, as it does under `single`
        let _single = super::super::canvas::measure_by(vtium_term::ClusterWidth::Single);
        assert_eq!(super::super::canvas::cells(&row), 80, "{row:?}");
    }
}
