//! How the Flow view as paths draws ([`super::rows`]): the status line,
//! the policy line and the column titles fixed at the top, the key line
//! fixed at the bottom, and between them a window on the canvas of paths,
//! the editor and the pieces, scrolled to the row [`super::rows::State`]
//! keeps at its top. Enter on a box draws its second layer in the
//! window's place.
//!
//! A box's border says its state: white when it is in a path or free to
//! pick, amber for a device no path holds, grey for what cannot be picked,
//! and bright magenta, a color no state uses, for a part picked into the
//! editor. Its title says its kind: displays cyan, terminals yellow, font
//! sets magenta, inputs green, as the column titles are. The editor's boxes
//! and lines are dashed, and so is the line that says the roaming inputs
//! reach a terminal. The highlighted box has a heavy border in bold, and
//! its own lines are drawn heavy and bright.

use super::canvas::{Canvas, fit, width, wrap};
use super::color::{Color, Style};
use super::flow::{Flow, Kind};
use super::model::App;
use super::paths::{self, Path, Piece};
use super::rows::{self, BOX_ROWS, Boxed, Ctx, Layout, Mood, NoteKind, Obj, Slot, Wire};
use super::words;

/// The rows fixed at the top: the tab bar, the status line, the policy
/// line, the heading of the area at the window's top, and the column
/// titles.
pub const HEAD_ROWS: usize = 5;

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;

/// The column where a layer's values start.
const VALUE_X: usize = 34;

/// The box-drawing character for each mix of lines: the weight of the
/// line up, down, left and right of the cell, each 0 for none, 1 light
/// and 2 heavy, indexed as up × 27 + down × 9 + left × 3 + right. A cell
/// with one line has a whole line through it, so a line's end meets the
/// border beside it.
const JOINTS: [char; 81] = [
    ' ', '─', '━', '─', '─', '╼', '━', '╾', '━', '│', '┌', '┍', '┐', '┬', '┮', '┑', '┭', '┯', '┃',
    '┎', '┏', '┒', '┰', '┲', '┓', '┱', '┳', '│', '└', '┕', '┘', '┴', '┶', '┙', '┵', '┷', '│', '├',
    '┝', '┤', '┼', '┾', '┥', '┽', '┿', '╽', '┟', '┢', '┧', '╁', '╆', '┪', '╅', '╈', '┃', '┖', '┗',
    '┚', '┸', '┺', '┛', '┹', '┻', '╿', '┞', '┡', '┦', '╀', '╄', '┩', '╃', '╇', '┃', '┠', '┣', '┨',
    '╂', '╊', '┫', '╉', '╋',
];

/// A box's border: across, down, and the four corners, light and heavy,
/// whole and dashed.
const LIGHT: [char; 6] = ['─', '│', '┌', '┐', '└', '┘'];
const HEAVY: [char; 6] = ['━', '┃', '┏', '┓', '┗', '┛'];
const LIGHT_DASHED: [char; 6] = ['┄', '┆', '┌', '┐', '└', '┘'];
const HEAVY_DASHED: [char; 6] = ['┅', '┇', '┏', '┓', '┗', '┛'];

/// Whether the rows are on the screen: the Flow view as rows, with no
/// route card, pager or Restore list open over it, which the six-column
/// view's screen draws.
pub fn shown(app: &App) -> bool {
    app.view == super::model::View::Flow
        && app.rows_flow.as_ref().is_some_and(|s| !s.classic)
        && app.route.is_none()
        && app.pager.is_none()
        && app.restore.is_none()
}

/// The font sets the palette offers: every font the TUI lists that reads.
pub fn font_names(app: &App) -> Vec<String> {
    app.inputs
        .fonts
        .iter()
        .filter(|f| !matches!(f.state, super::model::FontState::Refused(_)))
        .map(|f| f.name.clone())
        .collect()
}

/// The document as `app` holds it, laid out at its width.
pub fn layout_of(app: &App) -> Layout {
    let flow = &app.inputs.flow;
    let doc = paths::read(flow);
    let new = app
        .rows_flow
        .as_ref()
        .map(|s| s.new.clone())
        .unwrap_or_default();
    let fonts = font_names(app);
    rows::layout(&Ctx {
        flow,
        doc: &doc,
        new: &new,
        fonts: &fonts,
        cols: app.cols,
        default: paths::is_default(flow),
    })
}

/// The message under the window, wrapped.
fn message_rows(app: &App) -> Vec<String> {
    app.message
        .as_ref()
        .map(|m| wrap(m, app.cols.saturating_sub(2).max(1), MESSAGE_ROWS))
        .unwrap_or_default()
}

/// The window on the canvas: its first screen row, and how many rows it
/// has between the column titles and the message and key lines.
pub fn window(app: &App) -> (usize, usize) {
    let bottom = app
        .rows
        .saturating_sub(1)
        .saturating_sub(message_rows(app).len());
    (HEAD_ROWS, bottom.saturating_sub(HEAD_ROWS))
}

fn bold(color: Color) -> Style {
    Style {
        bold: true,
        ..Style::text(color)
    }
}

/// The color of a box's border, by its state.
pub fn mood_color(mood: Mood) -> Color {
    match mood {
        Mood::Placed => Color::White,
        Mood::Waiting => Color::Yellow,
        Mood::Aside => Color::Grey,
        Mood::Picked => Color::BrightMagenta,
    }
}

/// The color of a box's title and of its column's title, by its kind.
pub fn kind_color(obj: &Obj) -> Color {
    match obj {
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) => match s {
            Slot::Display(_) => Color::Cyan,
            Slot::Terminal => Color::Yellow,
            Slot::Font(_) => Color::Magenta,
            Slot::Input(_) => Color::Green,
        },
        Obj::Roam(_) | Obj::Others => Color::Green,
    }
}

/// Draws the box `b` on `doc`, heavy when it is highlighted.
fn draw_box(doc: &mut Canvas, b: &Boxed, highlighted: bool) {
    let border = mood_color(b.mood);
    let title = kind_color(&b.obj);
    let border = match (highlighted, b.mood) {
        (true, Mood::Placed) => bold(Color::BrightWhite),
        (true, _) | (false, Mood::Picked) => bold(border),
        (false, _) => Style::text(border),
    };
    let [h, v, tl, tr, bl, br] = match (highlighted, b.dashed) {
        (false, false) => LIGHT,
        (true, false) => HEAVY,
        (false, true) => LIGHT_DASHED,
        (true, true) => HEAVY_DASHED,
    };
    let inner = b.w.saturating_sub(2);
    let across = h.to_string().repeat(inner);
    doc.put(b.x, b.y, &format!("{tl}{across}{tr}"), border);
    for i in 1..BOX_ROWS.saturating_sub(1) {
        doc.put(b.x, b.y.saturating_add(i), &v.to_string(), border);
        doc.put(
            b.right().saturating_sub(1),
            b.y.saturating_add(i),
            &v.to_string(),
            border,
        );
    }
    doc.put(
        b.x,
        b.bottom().saturating_sub(1),
        &format!("{bl}{across}{br}"),
        border,
    );
    let room = b.w.saturating_sub(4);
    let title_style = match (highlighted, b.mood) {
        (true, _) => Style {
            underline: true,
            ..bold(title)
        },
        (false, Mood::Picked) => bold(title),
        (false, _) => Style::text(title),
    };
    doc.put(
        b.x.saturating_add(2),
        b.title_row(),
        &fit(&b.title, room),
        title_style,
    );
    doc.put(
        b.x.saturating_add(2),
        b.title_row().saturating_add(1),
        &fit(&b.detail, room),
        Style::text(Color::Grey),
    );
}

/// The cells of a wire's path, in order from its first point to its
/// last.
fn cells(w: &Wire) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for pair in w.points.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let steps = a.0.abs_diff(b.0).max(a.1.abs_diff(b.1));
        for k in 0..=steps {
            let x = if b.0 >= a.0 {
                a.0.saturating_add(k.min(b.0 - a.0))
            } else {
                a.0.saturating_sub(k.min(a.0 - b.0))
            };
            let y = if b.1 >= a.1 {
                a.1.saturating_add(k.min(b.1 - a.1))
            } else {
                a.1.saturating_sub(k.min(a.1 - b.1))
            };
            if out.last() != Some(&(x, y)) {
                out.push((x, y));
            }
        }
    }
    out
}

/// Draws the wires as box-drawing lines that join where they meet, heavy
/// where they belong to the highlighted object, each ending in an arrow
/// at its terminal. A cell that only dashed lines cross is drawn dashed,
/// and one that only a roaming stub crosses is grey.
fn draw_wires(doc: &mut Canvas, wires: &[Wire], at: Option<&Obj>) {
    let (cols, rows) = (doc.cols, doc.rows);
    // each cell's weight up, down, left and right, whether a whole line
    // crosses it, and whether a line other than a stub does
    let mut grid = vec![([0u8; 4], false, false); cols.saturating_mul(rows)];
    let heavy = |w: &Wire| at.is_some_and(|o| w.from == *o || w.to == *o);
    for w in wires {
        let weight = if heavy(w) { 2 } else { 1 };
        let path = cells(w);
        for pair in path.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            // the side of a that b is on, and the side of b that a is on
            let (from_a, from_b) = if b.1 < a.1 {
                (0, 1)
            } else if b.1 > a.1 {
                (1, 0)
            } else if b.0 < a.0 {
                (2, 3)
            } else {
                (3, 2)
            };
            for ((x, y), side) in [(*a, from_a), (*b, from_b)] {
                if let Some(cell) = grid.get_mut(y.saturating_mul(cols).saturating_add(x)) {
                    if let Some(s) = cell.0.get_mut(side) {
                        *s = (*s).max(weight);
                    }
                    cell.1 |= !w.dashed;
                    cell.2 |= !w.stub;
                }
            }
        }
    }
    for y in 0..rows {
        for x in 0..cols {
            let Some((c, whole, other)) = grid.get(y.saturating_mul(cols).saturating_add(x)) else {
                continue;
            };
            if c.iter().all(|w| *w == 0) {
                continue;
            }
            let [u, d, l, r] = c.map(usize::from);
            let strong = c.contains(&2);
            let glyph = match (*whole, u + d == 0, l + r == 0, strong) {
                (false, true, false, false) => '┄',
                (false, true, false, true) => '┅',
                (false, false, true, false) => '┆',
                (false, false, true, true) => '┇',
                _ => JOINTS
                    .get(u * 27 + d * 9 + l * 3 + r)
                    .copied()
                    .unwrap_or(' '),
            };
            let style = if strong {
                bold(Color::BrightWhite)
            } else if !*other {
                Style::text(Color::Grey)
            } else {
                Style::text(Color::White)
            };
            doc.put(x, y, &glyph.to_string(), style);
        }
    }
    // a stub's arrow first, so the arrow of an input whose line it joins
    // is the one that shows
    let mut ordered: Vec<&Wire> = wires.iter().collect();
    ordered.sort_by_key(|w| !w.stub);
    for w in ordered {
        let path = cells(w);
        let (Some(last), Some(before)) = (path.last(), path.iter().rev().nth(1)) else {
            continue;
        };
        let right = last.0 > before.0;
        let (glyph, style) = match (heavy(w), right, w.stub) {
            (true, true, _) => ("▶", bold(Color::BrightWhite)),
            (true, false, _) => ("◀", bold(Color::BrightWhite)),
            (false, true, _) => ("►", Style::text(Color::White)),
            (false, false, false) => ("◄", Style::text(Color::White)),
            (false, false, true) => ("◄", Style::text(Color::Grey)),
        };
        doc.put(last.0, last.1, glyph, style);
    }
}

/// The whole canvas: the paths, the editor and the pieces, with `at`
/// highlighted.
pub fn document(layout: &Layout, cols: usize, at: Option<&Obj>) -> Canvas {
    let mut doc = Canvas::new(cols, layout.height.max(1));
    draw_wires(&mut doc, &layout.wires, at);
    for b in &layout.boxes {
        draw_box(&mut doc, b, Some(&b.obj) == at);
    }
    for n in &layout.notes {
        let style = match n.kind {
            NoteKind::Heading => bold(Color::BrightWhite),
            NoteKind::Needs => Style::text(Color::Yellow),
            NoteKind::Ready => bold(Color::Green),
            NoteKind::Grey => Style::text(Color::Grey),
            NoteKind::Displays => bold(Color::Cyan),
            NoteKind::Inputs => bold(Color::Green),
            NoteKind::Fonts => bold(Color::Magenta),
            NoteKind::Stub => Style {
                italic: true,
                ..Style::text(Color::Grey)
            },
        };
        doc.put(n.x, n.y, &n.text, style);
    }
    for y in &layout.rules {
        doc.put(
            1,
            *y,
            &"─".repeat(cols.saturating_sub(2)),
            Style::text(Color::Grey),
        );
    }
    doc
}

/// The canvas as a window `height` rows tall from row `top` shows it: a
/// box whose title is out of sight is left out, so no stray piece of a
/// border shows at the window's edges.
pub fn window_of(
    layout: &Layout,
    cols: usize,
    at: Option<&Obj>,
    top: usize,
    height: usize,
) -> Canvas {
    let mut doc = document(layout, cols, at);
    let end = top.saturating_add(height);
    for b in &layout.boxes {
        if b.title_row() >= top && b.title_row() < end {
            continue;
        }
        for y in b.y..b.bottom() {
            doc.put(b.x, y, &" ".repeat(b.w), Style::PLAIN);
        }
    }
    doc
}

/// The words a box's object goes by on a wire's label or a layer's
/// breadcrumb.
fn obj_name(flow: &Flow, o: &Obj) -> String {
    match o {
        Obj::Doc(n, Slot::Terminal) => n.clone(),
        Obj::New(Slot::Terminal) => words::ROWS_NEW_PATH.to_lowercase(),
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) => match s {
            Slot::Display(sel) => paths::title(flow, Kind::Display, sel).0,
            Slot::Input(sel) => paths::title(flow, Kind::Input, sel).0,
            Slot::Font(f) => rows::font_words(f.as_deref()),
            Slot::Terminal => String::new(),
        },
        Obj::Roam(sel) => paths::title(flow, Kind::Input, sel).0,
        Obj::Others => words::ROWS_OTHERS_TITLE.to_lowercase(),
    }
}

/// Where a wire whose other end is out of sight meets the window's edge,
/// and what the label there says: `↑ desk`, `↓ Logitech mouse`.
fn edge_labels(
    flow: &Flow,
    layout: &Layout,
    top: usize,
    height: usize,
) -> Vec<(usize, usize, String)> {
    let Some(g) = layout.geometry else {
        return Vec::new();
    };
    let end = top.saturating_add(height);
    let seen = |y: usize| y >= top && y < end;
    let mut out = Vec::new();
    for w in &layout.wires {
        let (Some(first), Some(last)) = (w.points.first(), w.points.last()) else {
            continue;
        };
        let (Some(trunk), true) = (w.points.get(1), w.points.len() > 2) else {
            continue;
        };
        let (hidden, name) = match (seen(first.1), seen(last.1)) {
            (true, false) => (last.1, obj_name(flow, &w.to)),
            (false, true) => (first.1, obj_name(flow, &w.from)),
            _ => continue,
        };
        let (row, arrow) = if hidden < top {
            (top, "↑")
        } else {
            (end.saturating_sub(1), "↓")
        };
        // the label runs from the trunk to the column after it
        let limit = if trunk.0 < g.term_x {
            g.term_x
        } else {
            g.dev_x
        };
        let room = limit.saturating_sub(1).saturating_sub(trunk.0);
        out.push((trunk.0, row, fit(&format!("{arrow} {name}"), room)));
    }
    out
}

/// The status line: the configuration open, or the breadcrumb to the box
/// whose layer is open; then whether every device is in a path; then how
/// the draft stands.
fn status(canvas: &mut Canvas, app: &App, layout: &Layout, crumb: Option<&str>) {
    let flow = &app.inputs.flow;
    let name = flow.draft.as_ref().map_or_else(
        || words::NO_DRAFT.to_owned(),
        |d| format!("draft {}", d.name),
    );
    let left = match crumb {
        Some(c) => format!("{name}  ›  {c}"),
        None => name,
    };
    let mut x = canvas.put(1, 1, &left, bold(Color::BrightWhite));
    let count = |s: rows::Sort| layout.waiting.iter().filter(|w| **w == s).count();
    let waiting = words::rows_waiting(
        count(rows::Sort::Keyboard),
        count(rows::Sort::Pointer),
        count(rows::Sort::Screen),
    );
    let color = if layout.waiting.is_empty() {
        Color::Green
    } else {
        Color::Yellow
    };
    x = canvas.put(x.saturating_add(3), 1, &waiting, Style::text(color));
    let mut rest: Vec<String> = Vec::new();
    if let Some(d) = &flow.draft {
        let state = words::draft_state(&d.name, d.changes, d.problems);
        let prefix = format!("draft {} · ", d.name);
        rest.push(state.strip_prefix(&prefix).unwrap_or(&state).to_owned());
    }
    if app.draft.is_some() {
        if let Some(came) = &app.came {
            rest.push(came.clone());
        }
        if !(app.inputs.hotplug && app.present.udev) {
            rest.push(words::NO_HOTPLUG.to_owned());
        }
    }
    if !rest.is_empty() {
        let room = app.cols.saturating_sub(x).saturating_sub(1);
        canvas.put(
            x,
            1,
            &fit(&format!(" · {}", rest.join(" · ")), room),
            Style::text(Color::Grey),
        );
    }
}

/// The key line on the last row: the keys that fit, and always the last,
/// the key that leaves.
fn key_line(canvas: &mut Canvas, app: &App, keys: &str) {
    let room = app.cols.saturating_sub(2);
    let items: Vec<&str> = keys.split("  ").collect();
    let (last, rest) = items.split_last().unwrap_or((&"", &[]));
    let mut row = String::new();
    for item in rest {
        let len = width(&row)
            .saturating_add(2)
            .saturating_add(width(item))
            .saturating_add(2)
            .saturating_add(width(last));
        if len > room {
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
    canvas.put(
        1,
        app.rows.saturating_sub(1),
        &row,
        Style::text(Color::Grey),
    );
}

/// The message above the key line.
fn message(canvas: &mut Canvas, app: &App) {
    let lines = message_rows(app);
    let y = app.rows.saturating_sub(1).saturating_sub(lines.len());
    for (i, line) in lines.iter().enumerate() {
        canvas.put(1, y.saturating_add(i), line, Style::PLAIN);
    }
}

/// Draws the Flow view as paths on `canvas`, below the tab bar.
pub fn screen(canvas: &mut Canvas, app: &App) {
    let Some(state) = &app.rows_flow else {
        return;
    };
    let flow = &app.inputs.flow;
    let layout = layout_of(app);
    message(canvas, app);
    match &state.layer {
        Some(obj) => {
            status(canvas, app, &layout, Some(&obj_name(flow, obj)));
            let keys = match obj {
                _ if state.typing.is_some() => words::KEYS_TYPING,
                Obj::Doc(_, Slot::Terminal) => words::ROWS_KEYS_CARD,
                Obj::New(Slot::Terminal) => words::ROWS_KEYS_EDITOR_CARD,
                Obj::Doc(..) | Obj::Roam(_) | Obj::Others => words::ROWS_KEYS_INFO,
                _ => words::ROWS_KEYS_CHOOSE,
            };
            key_line(canvas, app, keys);
            match obj {
                Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
                    card_layer(canvas, app, state, obj);
                }
                Obj::Doc(..) | Obj::Roam(_) => info_layer(canvas, app, obj),
                Obj::Others => others_layer(canvas, app),
                _ => choices_layer(canvas, app, state, obj),
            }
        }
        None => {
            status(canvas, app, &layout, None);
            key_line(canvas, app, words::ROWS_KEYS);
            canvas.put(
                1,
                2,
                &fit(
                    &words::rows_policy(&flow.policy),
                    app.cols.saturating_sub(2),
                ),
                Style::text(Color::Grey),
            );
            overview(canvas, app, state, &layout);
        }
    }
}

/// The heading of the area at the window's top above the column titles,
/// which CONFIGURATION and NEW PATH have, and the window on the canvas.
fn overview(canvas: &mut Canvas, app: &App, state: &rows::State, layout: &Layout) {
    let Some(g) = layout.geometry else {
        return;
    };
    let area = layout.area_at(state.top);
    let x = canvas.put(
        1,
        HEAD_ROWS.saturating_sub(2),
        area,
        bold(Color::BrightWhite),
    );
    if area == words::ROWS_NEW_PATH {
        let new = &state.new;
        let color = if new.complete() {
            bold(Color::Green)
        } else {
            Style::text(Color::Yellow)
        };
        let room = app.cols.saturating_sub(16);
        canvas.put(
            x.max(14),
            HEAD_ROWS.saturating_sub(2),
            &fit(new.status(), room),
            color,
        );
    }
    if area != words::ROWS_PIECES {
        for (x, w, title, color) in [
            (g.screen_x, g.screen_w, words::ROWS_DISPLAYS, Color::Cyan),
            (g.term_x, g.term_w, words::ROWS_TERMINALS, Color::Yellow),
            (g.dev_x, g.dev_w, words::ROWS_INPUTS, Color::Green),
        ] {
            canvas.put(x, HEAD_ROWS.saturating_sub(1), &fit(title, w), bold(color));
        }
    }
    let (wtop, wh) = window(app);
    let doc = window_of(layout, app.cols, state.at.as_ref(), state.top, wh);
    for i in 0..wh {
        let y = state.top.saturating_add(i);
        for x in 0..app.cols {
            if let (Some(c), Some(s)) = (doc.at(x, y), doc.style_at(x, y))
                && c != ' '
            {
                canvas.put(x, wtop.saturating_add(i), &c.to_string(), s);
            }
        }
    }
    for (x, y, text) in edge_labels(&app.inputs.flow, layout, state.top, wh) {
        let row = wtop.saturating_add(y.saturating_sub(state.top));
        canvas.put(x, row, &text, Style::text(Color::BrightWhite));
    }
}

/// One choice: `○ text`, or `● text` for what is chosen now, the text in
/// reverse video under the cursor; returns the column after it and its
/// gap.
fn choice(canvas: &mut Canvas, x: usize, y: usize, text: &str, on: bool, cursor: bool) -> usize {
    let (mark, mark_style) = if on {
        ("●", Style::text(Color::Green))
    } else {
        ("○", Style::text(Color::Grey))
    };
    canvas.put(x, y, mark, mark_style);
    let style = match (cursor, on) {
        (true, _) => Style {
            bold: on,
            ..Style::PLAIN.reversed()
        },
        (false, true) => Style {
            underline: true,
            ..bold(Color::BrightWhite)
        },
        (false, false) => Style::text(Color::White),
    };
    let end = canvas.put(x.saturating_add(2), y, text, style);
    end.saturating_add(3)
}

/// An added path's display or input, or a roaming input: what it
/// matches and where it goes, read-only, and the line that says how to
/// change it.
fn info_layer(canvas: &mut Canvas, app: &App, obj: &Obj) {
    let flow = &app.inputs.flow;
    let (kind, sel, heading) = match obj {
        Obj::Doc(_, Slot::Display(s)) => (Kind::Display, s, "DISPLAY"),
        Obj::Doc(_, Slot::Input(s)) | Obj::Roam(s) => (Kind::Input, s, "INPUT"),
        _ => return,
    };
    let (title, detail) = paths::title(flow, kind, sel);
    let x = canvas.put(
        1,
        3,
        &format!("{heading} {title}"),
        bold(Color::BrightWhite),
    );
    canvas.put(x.saturating_add(2), 3, &detail, Style::text(Color::Grey));
    let goes = match obj {
        Obj::Doc(name, _) => words::in_terminal(name),
        _ => words::ROAMS_SENTENCE.to_owned(),
    };
    let vx = VALUE_X.min(app.cols / 3);
    let mut y = 5usize;
    for (label, value) in [("Matched by", sel.clone()), ("Where it goes", goes)] {
        canvas.put(3, y, label, Style::text(Color::Grey));
        canvas.put(
            vx,
            y,
            &fit(&value, app.cols.saturating_sub(vx).saturating_sub(1)),
            Style::text(Color::BrightWhite),
        );
        y = y.saturating_add(2);
    }
    let note = match obj {
        Obj::Roam(_) => words::REVISE_ROAMING_NOTE,
        _ => words::REVISE_NOTE,
    };
    for line in wrap(note, app.cols.saturating_sub(4), 2) {
        canvas.put(3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
}

/// The choices a part's layer offers, each its title, what it covers, and
/// its selector: what a display or an input part can match, from the
/// widest to the narrowest.
pub fn choices_for(app: &App, obj: &Obj) -> Vec<(String, String, Option<String>)> {
    let flow = &app.inputs.flow;
    let (Obj::Pal(slot) | Obj::New(slot)) = obj else {
        return Vec::new();
    };
    let kind = match slot {
        Slot::Display(_) => Kind::Display,
        Slot::Input(_) => Kind::Input,
        _ => return Vec::new(),
    };
    let Some(piece) = slot.piece() else {
        return Vec::new();
    };
    paths::narrow(flow, paths::sort_of(flow, &piece))
        .into_iter()
        .map(|(t, sel)| {
            let detail = paths::title(flow, kind, &sel).1;
            (t, detail, Some(sel))
        })
        .collect()
}

/// One line of a terminal's card: what a path sets that allocates no
/// display, input or font, and in the editor, each part it joins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardLine {
    Name,
    Font,
    Size,
    Setting(&'static str),
    Part(Piece),
}

/// The settings a card offers, without `isolated`, which decides where
/// roaming keyboards go.
const CARD_SETTINGS: [&str; 3] = ["consoles", "login", "when-display-lost"];

/// The path a terminal's card shows: an added path's, or the editor's
/// with the name it will take.
pub fn card_path(app: &App, obj: &Obj) -> Option<Path> {
    let flow = &app.inputs.flow;
    let doc = paths::read(flow);
    match obj {
        Obj::Doc(name, Slot::Terminal) => doc.path(name).cloned(),
        Obj::New(Slot::Terminal) => {
            let mut p = app.rows_flow.as_ref()?.new.clone();
            if p.name.is_none() {
                p.name = Some(paths::new_name(flow, &doc));
            }
            Some(p)
        }
        _ => None,
    }
}

/// The card's lines for `path`; the editor's also list its parts.
pub fn card_lines(path: &Path, editor: bool) -> Vec<CardLine> {
    let mut out = vec![CardLine::Name, CardLine::Font, CardLine::Size];
    out.extend(CARD_SETTINGS.iter().map(|k| CardLine::Setting(k)));
    if editor {
        out.extend(
            path.displays
                .iter()
                .map(|d| CardLine::Part(Piece::Display(d.clone()))),
        );
        out.extend(
            path.inputs
                .iter()
                .map(|i| CardLine::Part(Piece::Input(i.clone()))),
        );
    }
    out
}

/// A line's choices, each its words and its value; none for a value the
/// person types, the name and the login.
pub fn card_choices(app: &App, line: &CardLine) -> Vec<(String, String)> {
    let own = |v: &str| (v.to_owned(), v.to_owned());
    match line {
        CardLine::Name | CardLine::Setting("login") => Vec::new(),
        CardLine::Font => std::iter::once((words::DEFAULT_FONT_SET.to_owned(), String::new()))
            .chain(font_names(app).iter().map(|f| own(f)))
            .collect(),
        CardLine::Size => std::iter::once((words::AUTOMATIC_SIZE.to_owned(), String::new()))
            .chain(super::model::SIZES.iter().map(|s| own(&s.to_string())))
            .collect(),
        CardLine::Setting("consoles") => (1..=6).map(|n| own(&n.to_string())).collect(),
        CardLine::Setting("when-display-lost") => vec![
            (words::KEEP_SESSIONS.to_owned(), String::new()),
            (words::END_SESSIONS.to_owned(), "hangup".to_owned()),
        ],
        CardLine::Setting(_) => Vec::new(),
        CardLine::Part(_) => vec![(words::DISCONNECT.to_owned(), String::new())],
    }
}

/// A line's value as the path holds it, empty for the default.
pub fn card_value(path: &Path, line: &CardLine) -> String {
    match line {
        CardLine::Name => path.name.clone().unwrap_or_default(),
        CardLine::Font => path.font.clone().unwrap_or_default(),
        CardLine::Size => path.size.map(|s| s.to_string()).unwrap_or_default(),
        CardLine::Setting("consoles") => Some(path.setting("consoles"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "1".to_owned()),
        CardLine::Setting(k) => path.setting(k),
        CardLine::Part(_) => String::new(),
    }
}

/// A line's label, in a reader's words.
fn card_label(flow: &Flow, line: &CardLine) -> String {
    match line {
        CardLine::Name => "Name".to_owned(),
        CardLine::Font => "Font set".to_owned(),
        CardLine::Size => "Size".to_owned(),
        CardLine::Setting("consoles") => "Consoles".to_owned(),
        CardLine::Setting("login") => "Login".to_owned(),
        CardLine::Setting("when-display-lost") => "When its screen is unplugged".to_owned(),
        CardLine::Setting(k) => (*k).to_owned(),
        CardLine::Part(Piece::Display(d)) => {
            format!("Display {}", paths::title(flow, Kind::Display, d).0)
        }
        CardLine::Part(Piece::Input(i)) => {
            format!("Input {}", paths::title(flow, Kind::Input, i).0)
        }
        CardLine::Part(Piece::Font(_)) => String::new(),
    }
}

/// A line's value in a reader's words, as it shows off the cursor.
fn card_words(app: &App, path: &Path, line: &CardLine) -> String {
    let value = card_value(path, line);
    match line {
        CardLine::Setting("login") if value.is_empty() => words::SYSTEM_LOGIN.to_owned(),
        CardLine::Part(_) => words::CONNECTED.to_owned(),
        _ => card_choices(app, line)
            .into_iter()
            .find(|(_, v)| *v == value)
            .map_or(value, |(w, _)| w),
    }
}

/// A terminal's card: its name, font set, size and the settings that
/// allocate nothing, each changeable here; in the editor, also the parts
/// it joins, which Enter disconnects. An added path's card says how to
/// change its parts.
fn card_layer(canvas: &mut Canvas, app: &App, state: &rows::State, obj: &Obj) {
    let flow = &app.inputs.flow;
    let Some(path) = card_path(app, obj) else {
        return;
    };
    let editor = matches!(obj, Obj::New(_));
    let heading = if editor {
        words::NEW_TERMINAL.to_owned()
    } else {
        format!("TERMINAL {}", path.name.clone().unwrap_or_default())
    };
    canvas.put(1, 3, &heading, bold(Color::BrightWhite));
    let (wtop, wh) = window(app);
    let bottom = wtop.saturating_add(wh);
    let lines = card_lines(&path, editor);
    let note = wrap(
        if editor {
            words::EDITOR_NOTE
        } else {
            words::REVISE_NOTE
        },
        app.cols.saturating_sub(4),
        2,
    );
    let room = bottom
        .saturating_sub(5)
        .saturating_sub(note.len().saturating_add(1))
        .max(1);
    let first = state.line.saturating_add(1).saturating_sub(room);
    let mut y = 5usize;
    for (i, line) in lines.iter().enumerate().skip(first).take(room) {
        canvas.put(3, y, &card_label(flow, line), Style::text(Color::Grey));
        let here = i == state.line;
        match (&state.typing, here) {
            (Some(t), true) => {
                canvas.put(VALUE_X, y, &format!(" {t}_ "), Style::PLAIN.reversed());
            }
            (None, true) => {
                let choices = card_choices(app, line);
                if choices.is_empty() {
                    let x = canvas.put(
                        VALUE_X,
                        y,
                        &format!(" {} ", card_words(app, &path, line)),
                        Style::PLAIN.reversed(),
                    );
                    canvas.put(
                        x.saturating_add(2),
                        y,
                        words::ENTER_TO_TYPE,
                        Style::text(Color::Grey),
                    );
                } else {
                    let value = card_value(&path, line);
                    // the row starts late enough that the cursor's choice
                    // shows, each choice taking its mark, its words and a gap
                    let room = app.cols.saturating_sub(VALUE_X);
                    let span = |a: usize, b: usize| -> usize {
                        choices
                            .iter()
                            .take(b.saturating_add(1))
                            .skip(a)
                            .map(|(w, _)| width(w).saturating_add(5))
                            .sum()
                    };
                    let mut first = 0usize;
                    while first < state.pick && span(first, state.pick) > room {
                        first = first.saturating_add(1);
                    }
                    let mut x = VALUE_X;
                    for (k, (w, v)) in choices.iter().enumerate().skip(first) {
                        if x.saturating_add(width(w)).saturating_add(3) >= app.cols {
                            break;
                        }
                        let on = *v == value && !matches!(line, CardLine::Part(_));
                        x = choice(canvas, x, y, w, on, k == state.pick);
                    }
                }
            }
            _ => {
                canvas.put(
                    VALUE_X,
                    y,
                    &fit(
                        &card_words(app, &path, line),
                        app.cols.saturating_sub(VALUE_X).saturating_sub(1),
                    ),
                    Style::text(Color::BrightWhite),
                );
            }
        }
        y = y.saturating_add(1);
    }
    y = y.saturating_add(1).min(bottom.saturating_sub(note.len()));
    for line in note {
        canvas.put(3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
}

/// A part's choices, one a row, broad to narrow, each with what it
/// covers; the cursor's in reverse video and the piece's own marked ●.
fn choices_layer(canvas: &mut Canvas, app: &App, state: &rows::State, obj: &Obj) {
    let heading = match obj {
        Obj::Pal(Slot::Display(_)) | Obj::New(Slot::Display(_)) => words::CHOOSE_DISPLAY,
        _ => words::CHOOSE_INPUT,
    };
    canvas.put(1, 3, heading, bold(Color::BrightWhite));
    let about = wrap(words::CHOOSE_ABOUT, app.cols.saturating_sub(4), 2);
    for (k, line) in about.iter().enumerate() {
        canvas.put(3, 4usize.saturating_add(k), line, Style::text(Color::Grey));
    }
    let now = match obj {
        Obj::New(Slot::Display(s)) | Obj::New(Slot::Input(s)) => Some(s.clone()),
        _ => None,
    };
    let (wtop, wh) = window(app);
    let bottom = wtop.saturating_add(wh);
    let top = 5usize.saturating_add(about.len());
    let room = bottom.saturating_sub(top).max(1);
    let first = state.line.saturating_add(1).saturating_sub(room);
    let mut y = top;
    for (i, (title, detail, value)) in choices_for(app, obj).iter().enumerate().skip(first) {
        if y >= bottom {
            break;
        }
        let on = now.is_some() && *value == now;
        let x = choice(canvas, 3, y, title, on, i == state.line);
        let room = app.cols.saturating_sub(x);
        canvas.put(x, y, &fit(detail, room), Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
}

/// The second layer of the box of other inputs: each input it folds, with
/// the rule that catches it.
fn others_layer(canvas: &mut Canvas, app: &App) {
    let flow = &app.inputs.flow;
    canvas.put(1, 3, words::ROWS_OTHERS_TITLE, bold(Color::BrightWhite));
    let about = wrap(words::ROWS_OTHERS_ABOUT, app.cols.saturating_sub(4), 2);
    for (k, l) in about.iter().enumerate() {
        canvas.put(3, 4usize.saturating_add(k), l, Style::text(Color::Grey));
    }
    let vx = (app.cols / 2).min(42);
    let (wtop, wh) = window(app);
    let bottom = wtop.saturating_add(wh);
    let mut y = 5usize.saturating_add(about.len()).saturating_add(1);
    for d in rows::folded(flow) {
        if y >= bottom {
            break;
        }
        let Some(dev) = flow.inputs.get(d) else {
            continue;
        };
        canvas.put(
            3,
            y,
            &fit(&dev.name, vx.saturating_sub(4)),
            Style::text(Color::White),
        );
        let caught = match dev.rule.and_then(|r| flow.rules.get(r).map(|x| (r, x))) {
            Some((r, x)) => format!(
                "rule {}: {} -> {}",
                r.saturating_add(1),
                x.selector,
                x.target
            ),
            None => words::ROWS_NO_RULE.to_owned(),
        };
        canvas.put(
            vx,
            y,
            &fit(&caught, app.cols.saturating_sub(vx).saturating_sub(1)),
            Style::text(Color::Grey),
        );
        y = y.saturating_add(1);
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::super::paths::tests::{EDID_A, KBD_PATH, flow};
    use super::super::paths::{Path, Piece};
    use super::super::rows::tests::lay;
    use super::*;

    /// Whether `c` is a line's glyph, an arrow's or a junction's.
    fn line_glyph(c: char) -> bool {
        JOINTS.contains(&c) && c != ' ' || "►◄▶◀┄┆┅┇".contains(c)
    }

    fn editor() -> Path {
        let mut p = Path::default();
        p.pick(Piece::Input(KBD_PATH.into()));
        p.pick(Piece::Input("id:2dc8:5200".into()));
        p.pick(Piece::Display("*".into()));
        p
    }

    #[test]
    fn every_wire_is_drawn_whole_from_border_to_border() {
        let f = flow();
        for cols in [80, 100, 120] {
            let l = lay(&f, &editor(), cols);
            let at = Obj::Doc("desk".into(), Slot::Terminal);
            let doc = document(&l, cols, Some(&at));
            for w in &l.wires {
                let path = cells(w);
                for (x, y) in &path {
                    let c = doc.at(*x, *y).unwrap_or(' ');
                    assert!(line_glyph(c), "{cols}: {w:?} has {c:?} at {x},{y}");
                }
                if w.stub {
                    // a stub starts beside its words and ends in an arrow
                    let (fx, fy) = path[0];
                    assert_eq!(doc.at(fx + 1, fy), Some('r'), "{cols}: {w:?}");
                    let (tx, ty) = *path.last().expect("a cell");
                    assert!(
                        "◄◀".contains(doc.at(tx, ty).unwrap_or(' ')),
                        "{cols}: {w:?}"
                    );
                    continue;
                }
                // beside each end, the border of the box it joins
                let from = l.find(&w.from).expect("its box");
                let to = l.find(&w.to).expect("its box");
                let (fx, fy) = path[0];
                let (tx, ty) = *path.last().expect("a cell");
                let beside_from = if from.x > fx { from.x } else { fx - 1 };
                let beside_to = if to.x > tx { to.x } else { tx - 1 };
                for (x, y) in [(beside_from, fy), (beside_to, ty)] {
                    let c = doc.at(x, y).unwrap_or(' ');
                    assert!("│┃┆┇".contains(c), "{cols}: {w:?} meets {c:?} at {x},{y}");
                }
                let arrow = doc.at(tx, ty).unwrap_or(' ');
                let arrows = if w.to == at { "▶◀" } else { "►◄" };
                assert!(arrows.contains(arrow), "{cols}: {w:?} ends in {arrow:?}");
            }
        }
    }

    #[test]
    fn the_editor_is_dashed_a_picked_piece_magenta_and_each_title_its_kind_s_color() {
        let f = flow();
        let l = lay(&f, &editor(), 100);
        let doc = document(&l, 100, None);
        let new = l.find(&Obj::New(Slot::Terminal)).expect("the editor");
        assert_eq!(doc.at(new.x, new.y), Some('┌'));
        assert_eq!(doc.at(new.x + 1, new.y), Some('┄'));
        assert_eq!(doc.at(new.x, new.y + 1), Some('┆'));
        let w = l.wires.iter().find(|w| w.dashed).expect("a dashed line");
        let (x, y) = cells(w)[1];
        assert_eq!(doc.at(x, y), Some('┄'));
        let added = l
            .find(&Obj::Doc("desk".into(), Slot::Terminal))
            .expect("desk");
        assert_eq!(doc.at(added.x + 1, added.y), Some('─'));
        let picked = l
            .find(&Obj::Pal(Slot::Input(KBD_PATH.into())))
            .expect("the receiver");
        let s = doc.style_at(picked.x, picked.y).expect("a style");
        assert_eq!((s.fg, s.bold), (Some(Color::BrightMagenta), true));
        // the title says the kind, and the border the state
        let title = |b: &Boxed| doc.style_at(b.x + 2, b.title_row()).and_then(|s| s.fg);
        let border = |b: &Boxed| doc.style_at(b.x, b.y).and_then(|s| s.fg);
        assert_eq!(title(picked), Some(Color::Green));
        assert_eq!(title(added), Some(Color::Yellow));
        assert_eq!(border(added), Some(Color::White));
        let dp = l
            .find(&Obj::Doc("desk".into(), Slot::Display(EDID_A.into())))
            .expect("DP-8");
        assert_eq!(title(dp), Some(Color::Cyan));
        let font = l
            .find(&Obj::Pal(Slot::Font(Some("monoid".into()))))
            .expect("a font set");
        assert_eq!(title(font), Some(Color::Magenta));
        let roams = l
            .find(&Obj::Roam("id:046d:c31c".into()))
            .expect("the roaming receiver");
        assert_eq!(title(roams), Some(Color::Green));
        let taken = l
            .find(&Obj::Pal(Slot::Display(EDID_A.into())))
            .expect("DP-8");
        assert_eq!(
            doc.style_at(taken.x, taken.y).and_then(|s| s.fg),
            Some(Color::Grey)
        );
    }

    #[test]
    fn a_box_whose_title_is_out_of_sight_leaves_no_piece_of_its_border() {
        let f = flow();
        let l = lay(&f, &Path::default(), 100);
        let desk = l
            .find(&Obj::Doc("desk".into(), Slot::Terminal))
            .expect("desk");
        // the window starts at desk's bottom border: desk's title is above it
        let top = desk.bottom() - 1;
        let doc = window_of(&l, 100, None, top, 12);
        for x in desk.x..desk.right() {
            assert_eq!(doc.at(x, top), Some(' '), "at {x},{top}");
        }
        let panel = l
            .find(&Obj::Doc("panel".into(), Slot::Terminal))
            .expect("panel");
        assert_eq!(doc.at(panel.x, panel.y), Some('┌'));
    }

    #[test]
    fn a_wire_to_a_terminal_out_of_sight_ends_at_the_edge_with_its_name() {
        // the receiver sent to desk alone, so desk has two inputs
        let mut f = flow();
        f.rules[3].target = "desk".into();
        let l = lay(&f, &Path::default(), 120);
        let second = l
            .find(&Obj::Doc("desk".into(), Slot::Input("id:046d:c31c".into())))
            .expect("desk's second input");
        // the window starts at the second input's top row: desk is above it
        let labels = edge_labels(&f, &l, second.y, 6);
        assert_eq!(labels.len(), 1, "{labels:?}");
        let (x, y, text) = &labels[0];
        assert_eq!(*y, second.y);
        assert_eq!(*x, l.geometry.expect("columns").dev_join);
        assert_eq!(text, "↑ desk");
    }
}
