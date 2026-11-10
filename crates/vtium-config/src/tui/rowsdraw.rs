//! How the Flow view as rows draws ([`super::rows`]): the status line,
//! the policy line and the column titles fixed at the top, the key line
//! fixed at the bottom, and between them a window on the canvas of rows
//! and trays, scrolled to the row [`super::rows::State`] keeps at its top.
//! Enter on an object draws its second layer in the window's place.
//!
//! Color says state and position says kind: a box's border is white when
//! its object is in a row, amber when it waits, and grey when it is set
//! aside. The highlighted box has a heavy border in bold, and its own
//! lines are drawn heavy and bright.

use super::canvas::{Canvas, fit, width, wrap};
use super::color::{Color, Style};
use super::edit::{Field, What};
use super::flow::{Flow, Item};
use super::model::App;
use super::rows::{
    self, BOX_ROWS, Boxed, Layout, Line, Mood, NoteKind, Obj, Sort, Wire, device_lines,
    match_words, reaching,
};
use super::words;

/// The rows fixed at the top: the tab bar, the status line, the policy
/// line, a blank row and the column titles.
pub const HEAD_ROWS: usize = 5;

/// The most rows the message line wraps to.
const MESSAGE_ROWS: usize = 3;

/// The column where a layer's values start, at most.
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

const LIGHT: [char; 6] = ['─', '│', '┌', '┐', '└', '┘'];
const HEAVY: [char; 6] = ['━', '┃', '┏', '┓', '┗', '┛'];

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

/// The color of a box's border and of its title, by its state.
fn mood_color(mood: Mood) -> (Color, Color) {
    match mood {
        Mood::Placed => (Color::White, Color::BrightWhite),
        Mood::Waiting => (Color::Yellow, Color::Yellow),
        Mood::Aside => (Color::Grey, Color::Grey),
    }
}

/// Draws the box `b` on `doc`, heavy when it is highlighted.
fn draw_box(doc: &mut Canvas, b: &Boxed, highlighted: bool) {
    let (border, title) = mood_color(b.mood);
    let border = match (highlighted, b.mood) {
        (true, Mood::Placed) => bold(Color::BrightWhite),
        (true, _) => bold(border),
        (false, _) => Style::text(border),
    };
    let [h, v, tl, tr, bl, br] = if highlighted { HEAVY } else { LIGHT };
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
    let title_style = if highlighted {
        Style {
            underline: true,
            ..bold(title)
        }
    } else {
        Style::text(title)
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

/// Draws the wires on `doc`: the solid ones as box-drawing lines that join
/// where they meet, heavy where they belong to the highlighted object, and
/// the dotted ones in grey.
fn draw_wires(doc: &mut Canvas, layout: &Layout, at: Option<&Obj>) {
    let (cols, rows) = (doc.cols, doc.rows);
    // each cell's weight up, down, left and right
    let mut grid = vec![[0u8; 4]; cols.saturating_mul(rows)];
    let heavy = |w: &Wire| at.is_some_and(|o| w.from == *o || w.to == *o);
    for w in layout.wires.iter().filter(|w| !w.dashed) {
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
                if let Some(cell) = grid
                    .get_mut(y.saturating_mul(cols).saturating_add(x))
                    .and_then(|c| c.get_mut(side))
                {
                    *cell = (*cell).max(weight);
                }
            }
        }
    }
    for y in 0..rows {
        for x in 0..cols {
            let Some(c) = grid.get(y.saturating_mul(cols).saturating_add(x)) else {
                continue;
            };
            if c.iter().all(|w| *w == 0) {
                continue;
            }
            let [u, d, l, r] = c.map(usize::from);
            let i = u * 27 + d * 9 + l * 3 + r;
            let glyph = JOINTS.get(i).copied().unwrap_or(' ');
            let style = if c.contains(&2) {
                bold(Color::BrightWhite)
            } else {
                Style::text(Color::White)
            };
            doc.put(x, y, &glyph.to_string(), style);
        }
    }
    // the dotted lines, on the cells no solid line holds; one that ends
    // under a solid line branches off it
    for w in layout.wires.iter().filter(|w| w.dashed) {
        let path = cells(w);
        for (k, (x, y)) in path.iter().enumerate() {
            let at = y.saturating_mul(cols).saturating_add(*x);
            if grid.get(at).is_some_and(|c| c.iter().any(|w| *w > 0)) {
                continue;
            }
            let across = [path.get(k.wrapping_sub(1)), path.get(k.saturating_add(1))]
                .into_iter()
                .flatten()
                .all(|n| n.1 == *y);
            let glyph = match (across, k == 0 || k + 1 == path.len()) {
                (true, _) => "┄",
                (false, true) => "┆",
                (false, false) => {
                    // the turn: from the line across to the line up
                    "┘"
                }
            };
            doc.put(*x, *y, glyph, Style::text(Color::Grey));
        }
        let (Some(last), Some(before)) = (path.last(), path.iter().rev().nth(1)) else {
            continue;
        };
        let above = last.1.checked_sub(1).and_then(|up| {
            let i = up.saturating_mul(cols).saturating_add(last.0);
            grid.get(i)
                .filter(|c| c.iter().any(|w| *w > 0))
                .map(|c| (up, *c))
        });
        match above {
            // the branch: the solid line's cell above it gains a line down
            Some((up, [u, _, l, r])) if before.0 == last.0 => {
                let i = usize::from(u) * 27 + 9 + usize::from(l) * 3 + usize::from(r);
                let glyph = JOINTS.get(i).copied().unwrap_or(' ');
                let style = doc.style_at(last.0, up).unwrap_or(Style::PLAIN);
                doc.put(last.0, up, &glyph.to_string(), style);
            }
            _ => {
                doc.put(last.0, last.1, "◄", Style::text(Color::Grey));
            }
        }
    }
    // the arrows of the solid lines, at the end beside the terminal
    for w in layout.wires.iter().filter(|w| !w.dashed) {
        let path = cells(w);
        let (Some(last), Some(before)) = (path.last(), path.iter().rev().nth(1)) else {
            continue;
        };
        let right = last.0 > before.0;
        let (glyph, style) = match (heavy(w), right) {
            (true, true) => ("▶", bold(Color::BrightWhite)),
            (true, false) => ("◀", bold(Color::BrightWhite)),
            (false, true) => ("►", Style::text(Color::White)),
            (false, false) => ("◄", Style::text(Color::White)),
        };
        doc.put(last.0, last.1, glyph, style);
    }
}

/// The whole canvas: the rows, the wires, the notes and the trays, with
/// `at` highlighted.
pub fn document(layout: &Layout, cols: usize, at: Option<&Obj>) -> Canvas {
    let mut doc = Canvas::new(cols, layout.height.max(1));
    draw_wires(&mut doc, layout, at);
    for b in &layout.boxes {
        draw_box(&mut doc, b, Some(&b.obj) == at);
    }
    for n in &layout.notes {
        let style = match n.kind {
            NoteKind::Waiting => bold(Color::Yellow),
            NoteKind::Aside => bold(Color::Grey),
            NoteKind::Hint => Style {
                italic: true,
                ..Style::text(Color::Grey)
            },
            NoteKind::Add => Style::text(Color::Grey),
        };
        doc.put(n.x, n.y, &n.text, style);
    }
    doc.put(
        1,
        layout.rule,
        &"─".repeat(cols.saturating_sub(2)),
        Style::text(Color::Grey),
    );
    doc
}

/// The name an object goes by on a wire's label.
fn obj_name(o: &Obj) -> String {
    match o {
        Obj::Screen(n, _) | Obj::Terminal(n) | Obj::Input(n, _) => n.clone(),
        Obj::Others => String::new(),
    }
}

/// Where a wire whose other end is out of sight meets the window's edge,
/// and what the label there says: `↑ desk`, `↓ Logitech mouse`.
fn edge_labels(layout: &Layout, top: usize, height: usize) -> Vec<(usize, usize, String)> {
    let Some(g) = layout.geometry else {
        return Vec::new();
    };
    let end = top.saturating_add(height);
    let seen = |y: usize| y >= top && y < end;
    let mut out = Vec::new();
    for w in layout.wires.iter().filter(|w| !w.dashed) {
        let (Some(first), Some(last)) = (w.points.first(), w.points.last()) else {
            continue;
        };
        let (Some(trunk), true) = (w.points.get(1), w.points.len() > 2) else {
            continue;
        };
        let (hidden, name) = match (seen(first.1), seen(last.1)) {
            (true, false) => (last.1, obj_name(&w.to)),
            (false, true) => (first.1, obj_name(&w.from)),
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

/// The status line: the draft's name, or the breadcrumb to the object
/// whose layer is open, then whether every device is in a path, then how
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
    let count = |s: Sort| layout.waiting.iter().filter(|w| **w == s).count();
    let waiting = words::rows_waiting(
        count(Sort::Keyboard),
        count(Sort::Pointer),
        count(Sort::Screen),
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

/// Draws the Flow view as rows on `canvas`, below the tab bar.
pub fn screen(canvas: &mut Canvas, app: &App) {
    let Some(state) = &app.rows_flow else {
        return;
    };
    let flow = &app.inputs.flow;
    let layout = rows::layout(flow, app.cols);
    message(canvas, app);
    match &state.layer {
        Some(obj) => {
            let crumb = match obj {
                Obj::Others => words::ROWS_OTHERS_TITLE.to_lowercase(),
                o => obj_name(o),
            };
            status(canvas, app, &layout, Some(&crumb));
            let keys = match obj {
                Obj::Terminal(_) if app.card.typed.is_some() => words::KEYS_TYPING,
                Obj::Terminal(_) => words::ROWS_KEYS_TERMINAL,
                Obj::Others => words::ROWS_KEYS_OTHERS,
                _ => words::ROWS_KEYS_DEVICE,
            };
            key_line(canvas, app, keys);
            match obj {
                Obj::Terminal(_) => terminal_layer(canvas, app),
                Obj::Others => others_layer(canvas, app),
                o => device_layer(canvas, app, o),
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

/// The column titles and the window on the canvas.
fn overview(canvas: &mut Canvas, app: &App, state: &rows::State, layout: &Layout) {
    let Some(g) = layout.geometry else {
        return;
    };
    for (x, w, title, color) in [
        (g.screen_x, g.screen_w, words::ROWS_SCREENS, Color::Cyan),
        (
            g.term_x,
            g.term_w,
            words::ROWS_TERMINALS,
            Color::BrightWhite,
        ),
        (g.dev_x, g.dev_w, words::ROWS_KEYBOARDS, Color::Green),
    ] {
        canvas.put(x, HEAD_ROWS.saturating_sub(1), &fit(title, w), bold(color));
    }
    let doc = document(layout, app.cols, state.at.as_ref());
    let (wtop, wh) = window(app);
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
    for (x, y, text) in edge_labels(layout, state.top, wh) {
        let row = wtop.saturating_add(y.saturating_sub(state.top));
        canvas.put(x, row, &text, Style::text(Color::BrightWhite));
    }
}

/// A field's label on a terminal's layer, in a reader's words.
fn field_label(f: &Field) -> String {
    match &f.what {
        What::HeadLook(_) => "Look".to_owned(),
        What::HeadName(_) => "Name".to_owned(),
        What::HeadKey(_, "consoles") => "Consoles".to_owned(),
        What::HeadKey(_, "login") => "Login".to_owned(),
        What::HeadKey(_, "when-display-lost") => "When its screen is unplugged".to_owned(),
        What::HeadKey(_, "isolated") => "Isolated".to_owned(),
        What::Actions => "Remove".to_owned(),
        _ => f.label.clone(),
    }
}

/// A head key's value in a reader's words, and a note after it.
fn field_value(flow: &Flow, h: usize, f: &Field) -> (String, String) {
    let name = flow
        .heads
        .get(h)
        .map(|x| x.name.clone())
        .unwrap_or_default();
    match &f.what {
        What::HeadLook(_) => {
            let note = flow
                .look_of(h)
                .and_then(|l| flow.looks.get(l))
                .map(|l| match (l.frit.is_empty(), l.points) {
                    (true, _) => String::new(),
                    (false, 0) => format!("{} at the automatic size", l.frit),
                    (false, p) => format!("{} at {p} pt", l.frit),
                })
                .unwrap_or_default();
            (f.current.clone(), note)
        }
        What::HeadKey(_, "consoles") => (
            f.current.clone(),
            "Ctrl+Super+Return opens another while it runs".to_owned(),
        ),
        What::HeadKey(_, "login") if f.current.is_empty() => {
            ("the system's login".to_owned(), String::new())
        }
        What::HeadKey(_, "when-display-lost") => {
            let v = if f.current.is_empty() || f.current == "keep" {
                "keep the sessions running".to_owned()
            } else {
                format!("end the sessions ({})", f.current)
            };
            (v, String::new())
        }
        What::HeadKey(_, "isolated") => {
            let yes = match f.current.as_str() {
                "yes" => true,
                "no" => false,
                _ => flow.policy == "closed",
            };
            if yes {
                ("yes".to_owned(), "no keyboard roams here".to_owned())
            } else {
                (
                    "no".to_owned(),
                    format!("roaming keyboards type here while {name} is active"),
                )
            }
        }
        What::Actions => (String::new(), String::new()),
        _ => (f.current.clone(), String::new()),
    }
}

/// The value column of a layer at this width.
fn value_x(cols: usize) -> usize {
    VALUE_X.min(cols / 3)
}

/// A terminal's second layer: its look, consoles, login, what happens when
/// its screen is unplugged and whether it is isolated, each a field of
/// the card Enter opens in the six-column view; then what reaches it.
fn terminal_layer(canvas: &mut Canvas, app: &App) {
    let flow = &app.inputs.flow;
    let Item::Head(h) = app.flow_at else {
        return;
    };
    let Some(head) = flow.heads.get(h) else {
        return;
    };
    let vx = value_x(app.cols);
    let active = rows::active(flow) == Some(h);
    let x = canvas.put(
        1,
        3,
        &format!("TERMINAL {}", head.name),
        bold(Color::BrightWhite),
    );
    canvas.put(
        x.saturating_add(2),
        3,
        &words::rows_terminal_about(active, head.consoles),
        Style::text(Color::Grey),
    );
    let fields = app.fields();
    let spacing = if app.rows >= 30 { 2 } else { 1 };
    let mut y = 5usize;
    for (i, f) in fields.iter().enumerate() {
        let here = i == app.card.field;
        canvas.put(3, y, &field_label(f), Style::text(Color::Grey));
        if here {
            field_choices(canvas, app, f, vx, y);
        } else {
            let (value, note) = field_value(flow, h, f);
            let x = canvas.put(vx, y, &value, Style::text(Color::BrightWhite));
            if !note.is_empty() {
                let room = app.cols.saturating_sub(x.saturating_add(3));
                canvas.put(
                    x.saturating_add(3),
                    y,
                    &fit(&note, room),
                    Style::text(Color::Grey),
                );
            }
        }
        y = y.saturating_add(spacing);
    }
    y = y.saturating_add(1);
    canvas.put(1, y, words::ROWS_CONNECTED, bold(Color::BrightWhite));
    let (screens, inputs) = reaching(flow, h);
    let screen_lines: Vec<(Vec<String>, String)> = group(
        screens
            .iter()
            .filter_map(|d| flow.displays.get(*d))
            .map(|d| {
                let how = d
                    .rule
                    .and_then(|r| flow.rules.get(r).map(|x| (r, x)))
                    .map(|(r, x)| (match_words(&x.selector, Sort::Screen), r))
                    .unwrap_or_default();
                (d.name.clone(), how)
            })
            .collect(),
    );
    let input_lines: Vec<(Vec<String>, String)> = group(
        inputs
            .iter()
            .filter_map(|(d, roams)| flow.inputs.get(*d).map(|x| (x, *roams)))
            .map(|(d, roams)| {
                let how = d
                    .rule
                    .and_then(|r| flow.rules.get(r).map(|x| (r, x)))
                    .map(|(r, x)| {
                        let w = if roams {
                            words::ROWS_ROAMS.to_owned()
                        } else {
                            match_words(&x.selector, rows::sort(d))
                        };
                        (w, r)
                    })
                    .unwrap_or_default();
                (d.name.clone(), how)
            })
            .collect(),
    );
    let bottom = window(app).0.saturating_add(window(app).1);
    for (label, lines, color) in [
        ("Screens", screen_lines, Color::Cyan),
        ("Keyboards and pointers", input_lines, Color::Green),
    ] {
        y = y.saturating_add(1);
        if y >= bottom {
            break;
        }
        canvas.put(3, y, label, Style::text(Color::Grey));
        if lines.is_empty() {
            canvas.put(vx, y, words::ROWS_NOTHING, Style::text(Color::Grey));
        }
        for (k, (names, how)) in lines.iter().enumerate() {
            if k > 0 {
                y = y.saturating_add(1);
            }
            if y >= bottom {
                break;
            }
            let room = app.cols.saturating_sub(vx).saturating_sub(1);
            let x = canvas.put(vx, y, &fit(&names.join(", "), room), Style::text(color));
            let room = app.cols.saturating_sub(x.saturating_add(3));
            canvas.put(
                x.saturating_add(2),
                y,
                &fit(how, room),
                Style::text(Color::Grey),
            );
        }
    }
}

/// Devices that reach a terminal the same way, on one line: their names,
/// and how a rule matches them, with the rules' numbers.
fn group(devices: Vec<(String, (String, usize))>) -> Vec<(Vec<String>, String)> {
    let mut out: Vec<(Vec<String>, String, Vec<usize>)> = Vec::new();
    for (name, (how, rule)) in devices {
        match out.iter_mut().find(|(_, h, _)| *h == how) {
            Some((names, _, rules)) => {
                names.push(name);
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
            None => out.push((vec![name], how, vec![rule])),
        }
    }
    out.into_iter()
        .map(|(names, how, rules)| {
            let numbers: Vec<String> = rules
                .iter()
                .map(|r| r.saturating_add(1).to_string())
                .collect();
            let rules = match numbers.as_slice() {
                [one] => format!("rule {one}"),
                [rest @ .., last] => format!("rules {} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            let how = match (how.as_str(), names.len()) {
                (words::ROWS_ROAMS, 1) => "it roams".to_owned(),
                (words::ROWS_ROAMS, _) => "they roam".to_owned(),
                _ => how,
            };
            (names, format!("{how} ({rules})"))
        })
        .collect()
}

/// The highlighted field's choices across its row: each offered value,
/// the one the draft has now marked ● and the cursor's in reverse video,
/// then `type…` where a value may be typed.
fn field_choices(canvas: &mut Canvas, app: &App, f: &Field, vx: usize, y: usize) {
    if let Some(typed) = &app.card.typed {
        canvas.put(vx, y, &format!(" {typed}_ "), Style::PLAIN.reversed());
        return;
    }
    let mut x = vx;
    for (k, o) in f.options.iter().enumerate() {
        let on = *o == f.current;
        x = choice(canvas, x, y, o, on, k == app.card.pick);
    }
    if f.typed {
        let text = if f.options.is_empty() && !f.current.is_empty() {
            format!("{} · {}", f.current, words::OR_TYPE_VALUE)
        } else {
            words::OR_TYPE_VALUE.to_owned()
        };
        let style = if app.card.pick == f.options.len() {
            Style::PLAIN.reversed()
        } else {
            Style::text(Color::Grey)
        };
        canvas.put(x, y, &text, style);
    }
}

/// One choice: `○ text`, or `● text` for what the draft has now, the
/// text in reverse video under the cursor; returns the column after it
/// and its gap.
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

/// A device's or a screen's second layer: where it goes, how its rule
/// matches it, and what the choice under the cursor changes.
fn device_layer(canvas: &mut Canvas, app: &App, obj: &Obj) {
    let flow = &app.inputs.flow;
    let Some(state) = &app.rows_flow else {
        return;
    };
    let Some(item) = rows::item_of(flow, obj) else {
        return;
    };
    let (kind_word, dev) = match item {
        Item::Display(d) => ("SCREEN", flow.displays.get(d)),
        Item::Input(d) => {
            let dev = flow.inputs.get(d);
            let word = match dev.map(rows::sort) {
                Some(Sort::Keyboard) => "KEYBOARD",
                Some(Sort::Pointer) => "POINTER",
                _ => "INPUT",
            };
            (word, dev)
        }
        _ => return,
    };
    let Some(dev) = dev else {
        return;
    };
    let s = match item {
        Item::Display(_) => Sort::Screen,
        _ => rows::sort(dev),
    };
    let x = canvas.put(
        1,
        3,
        &format!("{kind_word} {}", dev.name),
        bold(Color::BrightWhite),
    );
    let mut about: Vec<String> = Vec::new();
    let port = rows::port_words(&dev.detail);
    if !port.is_empty() {
        about.push(port);
    }
    if let Some(id) = dev.keys.iter().find_map(|k| k.strip_prefix("id:")) {
        about.push(format!("id {id}"));
    }
    about.push(
        if dev.found {
            "connected"
        } else {
            "not plugged in"
        }
        .to_owned(),
    );
    let room = app.cols.saturating_sub(x.saturating_add(3));
    canvas.put(
        x.saturating_add(2),
        3,
        &fit(&about.join(" · "), room),
        Style::text(Color::Grey),
    );
    let vx = value_x(app.cols).min(16);
    let lines: Vec<Line> = device_lines(flow, item);
    let mut y = 5usize;
    for (i, line) in lines.iter().enumerate() {
        canvas.put(3, y, &line.label, Style::text(Color::Grey));
        let mut x = vx;
        for (k, c) in line.choices.iter().enumerate() {
            let cursor = i == state.line && k == state.pick;
            if line.down && k > 0 {
                y = y.saturating_add(1);
                x = vx;
            }
            x = choice(canvas, x, y, &c.text, c.on, cursor);
            if !c.note.is_empty() {
                x = canvas
                    .put(x.saturating_sub(1), y, &c.note, Style::text(Color::Grey))
                    .saturating_add(3);
            }
            // a row of choices too wide for the screen goes on below
            if !line.down
                && let Some(next) = line.choices.get(k.saturating_add(1))
                && x.saturating_add(width(&next.text)).saturating_add(2) >= app.cols
            {
                y = y.saturating_add(1);
                x = vx;
            }
        }
        y = y.saturating_add(2);
    }
    // what the choice under the cursor changes, else the choice made now
    let goes = lines.first();
    let picked = goes.and_then(|l| {
        if state.line == 0 {
            l.choices.get(state.pick)
        } else {
            l.choices.iter().find(|c| c.on)
        }
    });
    if let Some(c) = picked
        && dev.found
    {
        y = y.saturating_add(1);
        canvas.put(1, y, words::ROWS_WHAT_CHANGES, bold(Color::BrightWhite));
        let sentence = rows::what_changes(flow, s, &c.text);
        for (k, l) in wrap(&sentence, app.cols.saturating_sub(4), 3)
            .iter()
            .enumerate()
        {
            canvas.put(
                3,
                y.saturating_add(1).saturating_add(k),
                l,
                Style::text(Color::Grey),
            );
        }
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
    let vx = value_x(app.cols);
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
    use super::super::rows::tests::flow;
    use super::*;

    /// Whether `c` is a line's glyph, an arrow's or a junction's.
    fn line_glyph(c: char) -> bool {
        JOINTS.contains(&c) && c != ' ' || "►◄▶◀┄".contains(c)
    }

    #[test]
    fn every_wire_is_drawn_whole_from_border_to_border() {
        let mut f = flow();
        f.inputs[5].rule = Some(3);
        for cols in [80, 100, 120] {
            let l = rows::layout(&f, cols);
            let at = Obj::Terminal("desk".into());
            let doc = document(&l, cols, Some(&at));
            for w in l.wires.iter().filter(|w| !w.dashed) {
                let path = cells(w);
                for (x, y) in &path {
                    let c = doc.at(*x, *y).unwrap_or(' ');
                    assert!(line_glyph(c), "{cols}: {w:?} has {c:?} at {x},{y}");
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
                    assert!("│┃".contains(c), "{cols}: {w:?} meets {c:?} at {x},{y}");
                }
                // the highlighted terminal's wires are heavy, ending in a
                // heavy arrow, and the others light
                let arrow = doc.at(tx, ty).unwrap_or(' ');
                let arrows = if w.to == at { "▶◀" } else { "►◄" };
                assert!(arrows.contains(arrow), "{cols}: {w:?} ends in {arrow:?}");
            }
        }
    }

    #[test]
    fn the_highlighted_box_is_heavy_and_the_others_light() {
        let f = flow();
        let l = rows::layout(&f, 100);
        let at = Obj::Terminal("desk".into());
        let doc = document(&l, 100, Some(&at));
        let desk = l.find(&at).expect("desk");
        assert_eq!(doc.at(desk.x, desk.y), Some('┏'));
        assert!(doc.style_at(desk.x, desk.y).expect("a style").bold);
        let title = doc.style_at(desk.x + 2, desk.title_row()).expect("a style");
        assert!(title.bold && title.underline);
        let panel = l.find(&Obj::Terminal("panel".into())).expect("panel");
        assert_eq!(doc.at(panel.x, panel.y), Some('┌'));
        let wait = l
            .boxes
            .iter()
            .find(|b| b.mood == Mood::Waiting)
            .expect("one waits");
        assert_eq!(
            doc.style_at(wait.x, wait.y).and_then(|s| s.fg),
            Some(Color::Yellow)
        );
        let others = l.find(&Obj::Others).expect("the other inputs");
        assert_eq!(
            doc.style_at(others.x, others.y).and_then(|s| s.fg),
            Some(Color::Grey)
        );
    }

    #[test]
    fn a_wire_to_a_terminal_out_of_sight_ends_at_the_edge_with_its_name() {
        let f = flow();
        let l = rows::layout(&f, 120);
        let mouse = l
            .find(&Obj::Input(
                f.inputs[2].name.clone(),
                f.inputs[2].detail.clone(),
            ))
            .expect("the mouse");
        // the window starts at the mouse's top row: desk is above it
        let labels = edge_labels(&l, mouse.y, 6);
        assert_eq!(labels.len(), 1, "{labels:?}");
        let (x, y, text) = &labels[0];
        assert_eq!(*y, mouse.y);
        assert_eq!(*x, l.geometry.expect("columns").dev_join);
        assert_eq!(text, "↑ desk");
    }
}
