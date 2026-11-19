//! The TUI's screen (ADR-1201 D3, ADR-1403 D7): the state drawn on a
//! [`Canvas`], then into ratatui's buffer, whose Backend writes ECMA-48's
//! cursor position, erase and SGR sequences only.
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

use super::canvas::{Canvas, Row, span, wrap};
use super::color::{Colors, Style, palette};
use super::layout::{
    Body, FLOW_TOP, MESSAGE_ROWS, Tab, body, item_rows, key_line, key_rows, list_window,
    message_rows, tab_places, too_small,
};
use super::model::{App, Live, Size, Then, View};
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
    let pictures = app.shared.graphics && !app.shared.text_preview();
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
        .or_else(|| app.preview().preview_line())
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

/// The screen as it would be in a terminal of `size`, with no picture on
/// it and no caption beside one: what a picture of the console shows. It
/// draws a copy of the model at that size, so the model keeps its own size.
pub fn screen_at(app: &App, size: Size) -> Vec<Row> {
    let mut frame = app.clone();
    frame.size = size;
    frame.shared.picture = false;
    frame.shared.shot = None;
    screen(&frame)
}

/// The screen for `app`, read back from the cells the view draws:
/// exactly `app.rows` rows, none wider than `app.cols`. The cells are
/// drawn with [`Colors::Named`], so a style keeps the named color that the
/// pictures of the console draw, where the terminal is sent a 24-bit one.
pub fn screen(app: &App) -> Vec<Row> {
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
fn view(app: &App, colors: Colors, buf: &mut ratatui_core::buffer::Buffer) {
    fill(buf, &canvas(app), colors);
}

/// The screen for `app` drawn on a canvas: exactly `app.rows` rows, none
/// wider than `app.cols`.
fn canvas(app: &App) -> Vec<Row> {
    let (cols, rows) = (app.size.cols, app.size.rows);
    let mut canvas = Canvas::new(cols, rows);
    if too_small(app.size) {
        small(&mut canvas, app);
        return canvas.into_rows();
    }
    tab_bar(&mut canvas, app);
    if super::flowlayout::shown(app) {
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

/// One cell of a raw span: a character and the style its SGR set.
type RawCell = (char, ratatui_core::style::Style);

/// The characters of a raw span, each in the style its SGR sequences set:
/// the About page's banner, which its own module styles in 24-bit or 256
/// colors.
fn raw_cells(text: &str) -> Vec<RawCell> {
    use ratatui_core::style::{Color as C, Modifier as M, Style as S};
    let mut out = Vec::new();
    let mut style = S::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push((c, style));
            continue;
        }
        if chars.peek() != Some(&'[') {
            continue;
        }
        chars.next();
        let mut params = String::new();
        let mut last = ' ';
        for c in chars.by_ref() {
            if ('@'..='~').contains(&c) {
                last = c;
                break;
            }
            params.push(c);
        }
        if last != 'm' {
            continue;
        }
        let ps: Vec<u8> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
        let mut ps = ps.into_iter();
        while let Some(p) = ps.next() {
            // the color after 38 or 48: 5 and an index, or 2 and its red,
            // green and blue
            let extended = |ps: &mut std::vec::IntoIter<u8>| match ps.next() {
                Some(5) => C::Indexed(ps.next().unwrap_or(0)),
                Some(2) => C::Rgb(
                    ps.next().unwrap_or(0),
                    ps.next().unwrap_or(0),
                    ps.next().unwrap_or(0),
                ),
                _ => C::Reset,
            };
            style = match p {
                0 => S::new(),
                1 => style.add_modifier(M::BOLD),
                3 => style.add_modifier(M::ITALIC),
                4 => style.add_modifier(M::UNDERLINED),
                7 => style.add_modifier(M::REVERSED),
                30..=37 => style.fg(C::Indexed(p.saturating_sub(30))),
                90..=97 => style.fg(C::Indexed(p.saturating_sub(82))),
                40..=47 => style.bg(C::Indexed(p.saturating_sub(40))),
                100..=107 => style.bg(C::Indexed(p.saturating_sub(92))),
                38 => style.fg(extended(&mut ps)),
                48 => style.bg(extended(&mut ps)),
                39 => style.fg(C::Reset),
                49 => style.bg(C::Reset),
                _ => style,
            };
        }
    }
    out
}

/// Writes `rows` into `buf` from its top left, their colors read as
/// `colors` says, each character in as many cells as the console's width
/// rule gives it, as the terminal lays it out. A character the console and
/// Unicode's tables measure differently keeps the console's width in the
/// buffer, so ratatui's diff steps over the same cells the terminal's
/// cursor does.
pub fn fill(buf: &mut ratatui_core::buffer::Buffer, rows: &[Row], colors: Colors) {
    use ratatui_core::buffer::{CellDiffOption, CellWidth};
    let area = buf.area;
    for (y, row) in (area.top()..area.bottom()).zip(rows) {
        let cells = row.iter().flat_map(|s| -> Vec<RawCell> {
            if s.raw {
                raw_cells(&s.text)
            } else {
                let style = s.style.in_cells(colors);
                s.text.chars().map(|c| (c, style)).collect()
            }
        });
        let mut x = area.left();
        for (c, style) in cells {
            // a name read from a file never carries a control onto the
            // screen
            let c = if c.is_control() { '?' } else { c };
            let mut symbol = [0u8; 4];
            let symbol: &str = c.encode_utf8(&mut symbol);
            let width = super::backend::console_width(symbol).max(1);
            if x.saturating_add(width) > area.right() {
                break;
            }
            let cell = &mut buf[(x, y)];
            cell.set_symbol(symbol).set_style(style);
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
/// is sent.
pub fn render(app: &App, frame: &mut ratatui_core::terminal::Frame) {
    view(app, Colors::Sent, frame.buffer_mut());
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
        self.terminal.draw(|frame| render(app, frame))?;
        Ok(self.terminal.backend_mut().take())
    }

    /// Clears the screen and forgets what it showed, so the next frame
    /// draws whole: after another program wrote to the terminal, or after
    /// a picture left it.
    pub fn forget(&mut self) -> std::io::Result<()> {
        self.terminal.clear()
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

    #[test]
    fn a_row_is_laid_out_by_the_console_s_width_rule_and_its_diff_steps_the_same_cells() {
        // U+17A4 takes one cell by the console's rule and two by Unicode's
        // tables, and 日 takes two by both
        let rows = vec![vec![span("a\u{17a4}b日c", Style::PLAIN)]];
        let mut next = Buffer::empty(Rect::new(0, 0, 8, 1));
        fill(&mut next, &rows, Colors::Sent);
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
        fill(&mut buf, &rows, Colors::Named);
        assert_eq!(super::rows(&buf), rows);
    }

    #[test]
    fn a_raw_span_s_colors_reach_ratatui_s_cells() {
        use ratatui_core::style::Color as C;
        let cells = raw_cells("\x1b[38;2;1;2;3mA\x1b[48;5;200mB\x1b[0mC\x1b[95mD");
        let got: Vec<(char, C, C)> = cells
            .iter()
            .map(|(c, s)| (*c, s.fg.unwrap_or(C::Reset), s.bg.unwrap_or(C::Reset)))
            .collect();
        assert_eq!(
            got,
            [
                ('A', C::Rgb(1, 2, 3), C::Reset),
                ('B', C::Rgb(1, 2, 3), C::Indexed(200)),
                ('C', C::Reset, C::Reset),
                ('D', C::Indexed(13), C::Reset)
            ]
        );
    }
}
