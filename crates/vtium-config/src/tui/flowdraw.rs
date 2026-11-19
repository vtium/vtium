//! How the Flow view draws ([`super::flowview`]): the status line,
//! the policy line and the column titles fixed at the top, the key line
//! fixed at the bottom, and between them a window on the canvas of paths,
//! the editor and the pieces, scrolled to the row [`super::flowview::State`]
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
//! its own lines are drawn heavy and bright. Where the color file fills a
//! kind's boxes, the fill covers each box's inside and never its border,
//! and the text inside takes the color the fill picks
//! ([`super::flowcolor::text_for`]).

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::text::{Line, Span as RSpan};
use ratatui_core::widgets::Widget;
use ratatui_widgets::paragraph::Paragraph;

use super::canvas::{Canvas, Row, fit, printable, span, width, wrap};
use super::color::{Color, Colors, Style};
use super::flow::{Flow, Kind};
use super::flowcolor::{Palette, Role};
use super::flowlayout::{
    CardLine, card_about, card_choices, card_chosen, card_lines, card_path, card_value,
    choices_for, font_names, label_target, layout_of, message_rows, window,
};
use super::flowview::{self, BOX_ROWS, Boxed, Layout, Mood, NoteKind, Obj, Slot, Wire};
use super::model::{App, Shared};
use super::parts::{self, Stage};
use super::paths::{self, Path, Piece};
use super::{view, words};

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
/// A template's border: double lines, so a template reads as the mould a
/// part is made from, not as a part.
const DOUBLE: [char; 6] = ['═', '║', '╔', '╗', '╚', '╝'];

fn bold(color: Color) -> Style {
    Style {
        bold: true,
        ..Style::text(color)
    }
}

/// The role whose color a box's border takes, by its state.
pub const fn mood_role(mood: Mood) -> Role {
    match mood {
        Mood::Placed => Role::Placed,
        Mood::Waiting => Role::Waiting,
        Mood::Aside => Role::SetAside,
        Mood::Picked => Role::Picked,
    }
}

/// The role whose color a box's title and its column's title take, by
/// its kind.
pub const fn kind_role(obj: &Obj) -> Role {
    match obj {
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) | Obj::Roam(s) | Obj::Nothing(s) => {
            let stage = match s {
                Slot::Display(_) => Stage::Display,
                Slot::Terminal => Stage::Terminal,
                Slot::Font(_) => Stage::Font,
                Slot::Input(_) => Stage::Input,
                Slot::Special(p) => Stage::of(p),
                Slot::Template(st) | Slot::Empty(st) => *st,
            };
            match stage {
                Stage::Display => Role::Display,
                Stage::Terminal => Role::Terminal,
                Stage::Font => Role::FontSet,
                Stage::Input => Role::Input,
            }
        }
        Obj::Others => Role::Input,
    }
}

/// `style` in bold.
const fn emph(style: Style) -> Style {
    Style {
        bold: true,
        ..style
    }
}

/// Draws the box `b` on `doc`, heavy when it is highlighted.
fn draw_box(doc: &mut Canvas, b: &Boxed, highlighted: bool, pal: &Palette) {
    let border = pal.style(mood_role(b.mood));
    // the highlight is not a role: a placed box's is bright white in bold
    let border = match (highlighted, b.mood) {
        (true, Mood::Placed) if border != Style::PLAIN => bold(Color::BrightWhite),
        (true, _) | (false, Mood::Picked) => emph(border),
        (false, _) => border,
    };
    let [h, v, tl, tr, bl, br] = match (highlighted, b.dashed) {
        (false, _) if b.template => DOUBLE,
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
    // inside a filled box, the text takes the color the fill picks, and
    // the title is bold because the fill shows the kind; a set-aside
    // box's title stays light, so its italic reads as set aside. Black
    // text in a basic color is never bold: the console shows bold black
    // as dark grey, which reads poorly on a light fill.
    // a template is never filled: it is the mould of a part, not a part
    let fill = pal.fill(kind_role(&b.obj)).filter(|_| !b.template);
    let (title, detail, bolds) = match fill {
        Some(back) => {
            let text = pal.text_on(back, b.mood == Mood::Aside);
            for i in 1..BOX_ROWS.saturating_sub(1) {
                doc.put(
                    b.x.saturating_add(1),
                    b.y.saturating_add(i),
                    &" ".repeat(inner),
                    text,
                );
            }
            let bolds = text.rgb.is_some() || text.fg != Some(Color::Black);
            let title = if b.mood == Mood::Aside || !bolds {
                text
            } else {
                emph(text)
            };
            (title, text, bolds)
        }
        None => (pal.style(kind_role(&b.obj)), Style::text(Color::Grey), true),
    };
    let strong = |s: Style| if bolds { emph(s) } else { s };
    let room = b.w.saturating_sub(4);
    let title_style = match (highlighted, b.mood) {
        (true, _) => Style {
            underline: true,
            ..strong(title)
        },
        (false, Mood::Picked) => strong(title),
        (false, _) => title,
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
        detail,
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
fn draw_wires(doc: &mut Canvas, wires: &[Wire], at: Option<&Obj>, pal: &Palette) {
    let line = pal.style(Role::Line);
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
                line
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
            (false, true, _) => ("►", line),
            (false, false, false) => ("◄", line),
            (false, false, true) => ("◄", Style::text(Color::Grey)),
        };
        doc.put(last.0, last.1, glyph, style);
    }
}

/// The whole canvas: the paths, the editor and the pieces, with `at`
/// highlighted.
pub fn document(layout: &Layout, cols: usize, at: Option<&Obj>, pal: &Palette) -> Canvas {
    let mut doc = Canvas::new(cols, layout.height.max(1));
    draw_wires(&mut doc, &layout.wires, at, pal);
    for b in &layout.boxes {
        draw_box(&mut doc, b, Some(&b.obj) == at, pal);
    }
    for n in &layout.notes {
        let style = match n.kind {
            NoteKind::Heading => bold(Color::BrightWhite),
            NoteKind::Needs => pal.style(Role::Waiting),
            NoteKind::Ready => bold(Color::Green),
            NoteKind::Grey => Style::text(Color::Grey),
            NoteKind::Displays => emph(pal.style(Role::Display)),
            NoteKind::Inputs => emph(pal.style(Role::Input)),
            NoteKind::Fonts => emph(pal.style(Role::FontSet)),
            NoteKind::Terminals => emph(pal.style(Role::Terminal)),
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
    pal: &Palette,
) -> Canvas {
    let mut doc = document(layout, cols, at, pal);
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
        Obj::New(Slot::Terminal) => words::FLOW_NEW_PATH.to_lowercase(),
        Obj::Nothing(Slot::Terminal) => words::NO_TERMINAL.to_owned(),
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) | Obj::Nothing(s) => match s {
            Slot::Display(sel) => paths::title(flow, Kind::Display, sel).0,
            Slot::Input(sel) => paths::title(flow, Kind::Input, sel).0,
            Slot::Font(f) => flowview::font_words(f.as_deref()),
            Slot::Special(p) => parts::special_title(p),
            Slot::Template(st) | Slot::Empty(st) => st.name().to_owned(),
            Slot::Terminal => String::new(),
        },
        Obj::Roam(Slot::Input(sel)) => paths::title(flow, Kind::Input, sel).0,
        Obj::Roam(_) => words::ACTIVE_TERMINAL.to_owned(),
        Obj::Others => words::FLOW_OTHERS_TITLE.to_lowercase(),
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

/// The status line on row 1: the configuration open, or the breadcrumb
/// to the box whose layer is open; then whether every device is in a
/// path; then how the draft stands. It starts at column 1, and its parts
/// stand three columns apart.
fn status(app: &App, layout: &Layout, crumb: Option<&str>) -> Row {
    let flow = &app.shared.inputs.flow;
    let cols = app.size.cols;
    let name = flow.draft.as_ref().map_or_else(
        || words::NO_DRAFT.to_owned(),
        |d| format!("draft {}", d.name),
    );
    let left = match crumb {
        Some(c) => format!("{name}  ›  {c}"),
        None => name,
    };
    let count = |s: flowview::Sort| layout.waiting.iter().filter(|w| **w == s).count();
    let waiting = words::flow_waiting(
        count(flowview::Sort::Keyboard),
        count(flowview::Sort::Pointer),
        count(flowview::Sort::Screen),
    );
    let style = if layout.waiting.is_empty() {
        Style::text(Color::Green)
    } else {
        app.shared.inputs.flow_colors.style(Role::Waiting)
    };
    // the column after the waiting count, counted in characters as the
    // rest's room is
    let end = 1usize
        .saturating_add(left.chars().count())
        .min(cols)
        .saturating_add(3)
        .saturating_add(waiting.chars().count());
    let mut row = vec![
        span(left, bold(Color::BrightWhite)),
        span("   ", Style::PLAIN),
        span(waiting, style),
    ];
    let mut rest: Vec<String> = Vec::new();
    if let Some(d) = &flow.draft {
        let state = words::draft_state(&d.name, d.changes, app.shared.unsaved, d.problems);
        let prefix = format!("draft {} · ", d.name);
        rest.push(state.strip_prefix(&prefix).unwrap_or(&state).to_owned());
    }
    if app.shared.draft.is_some() {
        if let Some(came) = &app.shared.came {
            rest.push(came.clone());
        }
        if !(app.shared.inputs.hotplug && app.shared.present.udev) {
            rest.push(words::NO_HOTPLUG.to_owned());
        }
    }
    if !rest.is_empty() {
        let room = cols.saturating_sub(end).saturating_sub(1);
        row.push(span(
            fit(&format!(" · {}", rest.join(" · ")), room),
            Style::text(Color::Grey),
        ));
    }
    row
}

/// `height` rows of `area` from its row `y` and its column `x`, as far as
/// its bottom and its right edge.
fn place(area: Rect, x: usize, y: usize, height: usize) -> Rect {
    let n = |v: usize| u16::try_from(v).unwrap_or(u16::MAX);
    Rect::new(n(x), n(y), area.width.saturating_sub(n(x)), n(height)).intersection(area)
}

/// Draws the Flow view below the tab bar into `buf`, its colors read as
/// `colors` says: the status line, the policy line and the window on the
/// document of paths, or the layer open over it, then the message and
/// the key line. The status, the policy and the message can hold any
/// name from vtium.conf or a device, so they are written by the console's
/// width rule.
///
/// References:
/// - ADR-1206 D1, "The console's width rule decides where the cursor
///   goes".
pub fn render(app: &App, colors: Colors, buf: &mut Buffer) {
    let state = app.flow.state();
    let flow = &app.shared.inputs.flow;
    let area = buf.area;
    let (cols, rows) = (app.size.cols, app.size.rows);
    let layout = layout_of(state, &app.shared, cols);
    let keys = super::layout::key_rows(app);
    match &state.layer {
        Some(obj) => {
            let mut canvas = Canvas::new(cols, rows);
            match obj {
                Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
                    card_layer(&mut canvas, app, state, obj);
                }
                Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_) => {
                    info_layer(&mut canvas, app, obj);
                }
                Obj::Others => others_layer(&mut canvas, app),
                Obj::Pal(Slot::Template(st)) | Obj::New(Slot::Empty(st)) => match &state.picker {
                    Some(p) => picker_layer(&mut canvas, app, state, p),
                    None => make_layer(&mut canvas, app, state, *st),
                },
                _ => choices_layer(&mut canvas, app, state, obj),
            }
            let drawn = canvas.into_rows();
            view::fill(
                buf,
                place(area, 0, 2, rows),
                drawn.get(2..).unwrap_or_default(),
                colors,
            );
        }
        None => {
            let policy = fit(&words::flow_policy(&flow.policy), cols.saturating_sub(2));
            view::fill(
                buf,
                place(area, 1, 2, 1),
                &[vec![span(policy, Style::text(Color::Grey))]],
                colors,
            );
            let (top, height) = window(&app.shared, app.size, keys.len());
            Window {
                app,
                layout: &layout,
                height,
                colors,
            }
            .render(place(area, 0, top, height), buf);
        }
    }
    let crumb = state.layer.as_ref().map(|o| obj_name(flow, o));
    view::fill(
        buf,
        place(area, 1, 1, 1),
        &[status(app, &layout, crumb.as_deref())],
        colors,
    );
    let message: Vec<Row> = message_rows(&app.shared, cols)
        .into_iter()
        .map(|line| vec![span(line, Style::PLAIN)])
        .collect();
    let first = rows
        .saturating_sub(keys.len())
        .saturating_sub(message.len());
    view::fill(buf, place(area, 1, first, message.len()), &message, colors);
    let key_lines: Vec<Line> = keys
        .iter()
        .map(|k| {
            let style = Style::text(Color::Grey).in_cells(colors);
            Line::from(RSpan::styled(printable(k), style))
        })
        .collect();
    Paragraph::new(key_lines).render(
        place(area, 1, rows.saturating_sub(keys.len()), keys.len()),
        buf,
    );
}

/// The window on the document of paths, between the policy line and the
/// message and the key line: the rows of the document from the row the
/// Flow tab keeps at the window's top, with a label where a wire leaves
/// the window. The document is a picture made of cells, whose lines join
/// where they cross, so it is drawn on a grid of cells and written into
/// the buffer by the console's width rule.
struct Window<'a> {
    app: &'a App,
    layout: &'a Layout,
    /// The rows the window has, which decide the boxes whose titles show.
    height: usize,
    colors: Colors,
}

impl Widget for Window<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let app = self.app;
        let state = app.flow.state();
        let cells = window_cells(
            &app.shared.inputs.flow,
            self.layout,
            app.size.cols,
            state.at.as_ref(),
            state.top,
            self.height,
            &app.shared.inputs.flow_colors,
        );
        view::fill(buf, area, &cells.into_rows(), self.colors);
    }
}

/// The cells of a window `height` rows tall on the document, from its row
/// `top`, with `at` highlighted: every cell of the document that holds a
/// character, every blank cell inside a filled box, so the fill reaches
/// the screen whole, and a label where a wire leaves the window. Every
/// other cell is blank in the terminal's own colors.
fn window_cells(
    flow: &Flow,
    layout: &Layout,
    cols: usize,
    at: Option<&Obj>,
    top: usize,
    height: usize,
    pal: &Palette,
) -> Canvas {
    let doc = window_of(layout, cols, at, top, height, pal);
    let mut shown = Canvas::new(cols, height);
    for i in 0..height {
        let y = top.saturating_add(i);
        for x in 0..doc.cols {
            if let (Some(c), Some(s)) = (doc.at(x, y), doc.style_at(x, y))
                && (c != ' ' || s.back.is_some())
            {
                shown.put(x, i, &c.to_string(), s);
            }
        }
    }
    for (x, y, text) in edge_labels(flow, layout, top, height) {
        shown.put(
            x,
            y.saturating_sub(top),
            &text,
            Style::text(Color::BrightWhite),
        );
    }
    shown
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
    let flow = &app.shared.inputs.flow;
    let (kind, sel, heading) = match obj {
        Obj::Doc(_, Slot::Display(s)) | Obj::Nothing(Slot::Display(s)) => {
            (Kind::Display, s, "DISPLAY")
        }
        Obj::Doc(_, Slot::Input(s)) | Obj::Roam(Slot::Input(s)) | Obj::Nothing(Slot::Input(s)) => {
            (Kind::Input, s, "INPUT")
        }
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
        Obj::Nothing(Slot::Display(_)) => words::TURNED_OFF.to_owned(),
        Obj::Nothing(_) => words::SET_ASIDE.to_owned(),
        _ => words::ROAMS_SENTENCE.to_owned(),
    };
    let vx = VALUE_X.min(app.size.cols / 3);
    let mut y = 5usize;
    for (label, value) in [("Matched by", sel.clone()), ("Where it goes", goes)] {
        canvas.put(3, y, label, Style::text(Color::Grey));
        canvas.put(
            vx,
            y,
            &fit(&value, app.size.cols.saturating_sub(vx).saturating_sub(1)),
            Style::text(Color::BrightWhite),
        );
        y = y.saturating_add(2);
    }
    let note = match obj {
        Obj::Roam(_) => words::REVISE_ROAMING_NOTE,
        _ => words::REVISE_NOTE,
    };
    for line in wrap(note, app.size.cols.saturating_sub(4), 2) {
        canvas.put(3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
}

/// A line's label, in a reader's words.
fn card_label(flow: &Flow, line: &CardLine) -> String {
    match line {
        CardLine::Name => "Name".to_owned(),
        CardLine::Font => "Font set".to_owned(),
        CardLine::Size => "Size".to_owned(),
        CardLine::Setting("consoles") => "Consoles".to_owned(),
        CardLine::Setting("vt") => words::VTS_LABEL.to_owned(),
        CardLine::Setting("login") => "Login".to_owned(),
        CardLine::Setting("when-display-lost") => "When its screen is unplugged".to_owned(),
        CardLine::Setting("switch-on-enter") => words::SWITCH_LABEL.to_owned(),
        CardLine::Setting(k) => (*k).to_owned(),
        CardLine::Part(Piece::Display(d)) => {
            format!("Display {}", paths::title(flow, Kind::Display, d).0)
        }
        CardLine::Part(Piece::Input(i)) => {
            format!("Input {}", paths::title(flow, Kind::Input, i).0)
        }
        CardLine::Part(Piece::NoInput) => format!("Input {}", words::NO_INPUT),
        CardLine::Part(Piece::Font(_) | Piece::Active | Piece::NoTerminal | Piece::Terminal(_)) => {
            String::new()
        }
    }
}

/// A line's value in a reader's words, as it shows off the cursor.
fn card_words(shared: &Shared, path: &Path, line: &CardLine) -> String {
    let value = card_value(path, line);
    match line {
        CardLine::Setting("login") if value.is_empty() => {
            words::login_default(&paths::login_in_effect(&shared.inputs.flow))
        }
        CardLine::Setting("login") => value,
        CardLine::Part(_) => words::CONNECTED.to_owned(),
        _ => card_choices(shared, line)
            .into_iter()
            .find(|(_, v)| *v == value)
            .map_or(value, |(w, _)| w),
    }
}

/// A terminal's card: its name, font set, size and the settings that
/// allocate nothing, each changeable here; in the editor, also the parts
/// it joins, which Enter disconnects. An added path's card says how to
/// change its parts.
fn card_layer(canvas: &mut Canvas, app: &App, state: &flowview::State, obj: &Obj) {
    let flow = &app.shared.inputs.flow;
    let Some(path) = card_path(app.flow.state(), &app.shared, obj) else {
        return;
    };
    let editor = matches!(obj, Obj::New(_));
    let heading = if editor {
        format!("{} · {}", words::NEW_TERMINAL, words::vt_words(&path.vts()))
    } else {
        format!(
            "TERMINAL {} · {}",
            path.name.clone().unwrap_or_default(),
            words::vt_words(&path.vts())
        )
    };
    canvas.put(1, 3, &heading, bold(Color::BrightWhite));
    let (wtop, wh) = window(&app.shared, app.size, super::layout::key_rows(app).len());
    let bottom = wtop.saturating_add(wh);
    let lines = card_lines(&path, editor);
    let note = wrap(
        if editor {
            words::EDITOR_NOTE
        } else {
            words::REVISE_NOTE
        },
        app.size.cols.saturating_sub(4),
        2,
    );
    let about = lines
        .get(state.line)
        .and_then(|l| card_about(&app.shared, &path, l, editor))
        .map(|a| wrap(&a, app.size.cols.saturating_sub(4), 3))
        .unwrap_or_default();
    let room = bottom
        .saturating_sub(5)
        .saturating_sub(note.len().saturating_add(1))
        .saturating_sub(if about.is_empty() {
            0
        } else {
            about.len().saturating_add(1)
        })
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
                let choices = card_choices(&app.shared, line);
                if choices.is_empty() {
                    let x = canvas.put(
                        VALUE_X,
                        y,
                        &format!(" {} ", card_words(&app.shared, &path, line)),
                        Style::PLAIN.reversed(),
                    );
                    canvas.put(
                        x.saturating_add(2),
                        y,
                        words::ENTER_TO_TYPE,
                        Style::text(Color::Grey),
                    );
                } else {
                    let chosen = card_chosen(&app.shared, &path, line);
                    // the row starts late enough that the cursor's choice
                    // shows, each choice taking its mark, its words and a gap
                    let room = app.size.cols.saturating_sub(VALUE_X);
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
                    for (k, (w, _)) in choices.iter().enumerate().skip(first) {
                        if x.saturating_add(width(w)).saturating_add(3) >= app.size.cols {
                            break;
                        }
                        let on = chosen == Some(k) && !matches!(line, CardLine::Part(_));
                        x = choice(canvas, x, y, w, on, k == state.pick);
                    }
                }
            }
            _ => {
                canvas.put(
                    VALUE_X,
                    y,
                    &fit(
                        &card_words(&app.shared, &path, line),
                        app.size.cols.saturating_sub(VALUE_X).saturating_sub(1),
                    ),
                    Style::text(Color::BrightWhite),
                );
            }
        }
        y = y.saturating_add(1);
    }
    if !about.is_empty() {
        y = y.saturating_add(1);
    }
    for line in about {
        canvas.put(3, y, &line, Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
    y = y.saturating_add(1).min(bottom.saturating_sub(note.len()));
    for line in note {
        canvas.put(3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
    subflow(canvas, app, &path, y.saturating_add(1), bottom);
}

/// The terminal's subflow on its card (ADR-1403 D7): its font set, then
/// what its look draws, then its displays, left to right, and a grey line
/// under them about the look's filters and requirements. vtium-config
/// reads no vtlook file yet (ADR-1304), so the look shows by its name, or
/// as the built-in look, and the grey line says so.
fn subflow(canvas: &mut Canvas, app: &App, path: &Path, y: usize, bottom: usize) {
    if y.saturating_add(3) > bottom || path.active || path.none {
        return;
    }
    let flow = &app.shared.inputs.flow;
    canvas.put(1, y, words::SUBFLOW_TITLE, bold(Color::BrightWhite));
    let font = path
        .font
        .clone()
        .unwrap_or_else(|| words::DEFAULT_FONT_SET.to_owned());
    let look = flow
        .heads
        .iter()
        .find(|h| path.name.as_deref() == Some(h.name.as_str()))
        .map(|h| h.look.clone())
        .filter(|l| l != "default");
    let look = look.map_or_else(
        || words::DEFAULT_LOOK_BOX.to_owned(),
        |l| words::look_box(&l),
    );
    let displays: Vec<String> = path
        .displays
        .iter()
        .map(|d| paths::title(flow, Kind::Display, d).0)
        .collect();
    let displays = if displays.is_empty() {
        words::NO_DISPLAY_BOX.to_owned()
    } else {
        displays.join(", ")
    };
    let mut x = 3usize;
    let room = app.size.cols.saturating_sub(2);
    let arrow = "  \u{2192}  ";
    for (i, (text, color)) in [
        (format!("{} {font}", words::FONT_SET_WORD), Color::Magenta),
        (look, Color::BrightWhite),
        (displays, Color::Cyan),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            x = canvas.put(x, y.saturating_add(1), arrow, Style::text(Color::Grey));
        }
        let shown = fit(&text, room.saturating_sub(x));
        x = canvas.put(x, y.saturating_add(1), &shown, Style::text(color));
    }
    for (i, line) in wrap(words::SUBFLOW_REQUIRES, app.size.cols.saturating_sub(4), 2)
        .iter()
        .enumerate()
    {
        let row = y.saturating_add(2).saturating_add(i);
        if row < bottom {
            canvas.put(3, row, line, Style::text(Color::Grey));
        }
    }
}

/// A part's detail: its name, its label, which the person can type, and
/// the choices of what the part matches, from the widest to the
/// narrowest. The label is the detail's first line.
///
/// A part's choices, one a row, broad to narrow, each with what it
/// covers; the cursor's in reverse video and the piece's own marked ●.
fn choices_layer(canvas: &mut Canvas, app: &App, state: &flowview::State, obj: &Obj) {
    let flow = &app.shared.inputs.flow;
    let Some((kind, sel)) = label_target(obj) else {
        return;
    };
    let (part, heading) = match kind {
        Kind::Display => (words::DISPLAY_PART, words::CHOOSE_DISPLAY),
        Kind::Input => (words::INPUT_PART, words::CHOOSE_INPUT),
    };
    let name = paths::title(flow, kind, &sel).0;
    let x = canvas.put(1, 3, part, bold(Color::BrightWhite));
    let role = match kind {
        Kind::Display => Role::Display,
        Kind::Input => Role::Input,
    };
    canvas.put(
        x.saturating_add(2),
        3,
        &fit(&name, app.size.cols.saturating_sub(x).saturating_sub(3)),
        emph(app.shared.inputs.flow_colors.style(role)),
    );
    // the label's line
    let doc = paths::read(flow);
    let label = doc.label(kind, &sel).unwrap_or_default().to_owned();
    canvas.put(3, 5, words::LABEL_FIELD, Style::text(Color::Grey));
    let vx = 12usize;
    match &state.typing {
        Some(t) => {
            let x = canvas.put(vx, 5, &format!(" {t}_ "), Style::PLAIN.reversed());
            canvas.put(
                x.saturating_add(2),
                5,
                &fit(
                    words::LABEL_TYPING,
                    app.size.cols.saturating_sub(x).saturating_sub(3),
                ),
                Style::text(Color::Grey),
            );
        }
        None => {
            let shown = if label.is_empty() {
                words::NO_LABEL.to_owned()
            } else {
                label
            };
            let style = if state.line == 0 {
                Style::PLAIN.reversed()
            } else {
                Style::text(Color::BrightWhite)
            };
            let x = canvas.put(vx, 5, &format!(" {shown} "), style);
            canvas.put(
                x.saturating_add(3),
                5,
                &fit(
                    words::LABEL_ABOUT,
                    app.size.cols.saturating_sub(x).saturating_sub(4),
                ),
                Style::text(Color::Grey),
            );
        }
    }
    let y0 = 7usize;
    canvas.put(1, y0, heading, bold(Color::BrightWhite));
    let about = wrap(words::CHOOSE_ABOUT, app.size.cols.saturating_sub(4), 2);
    for (k, line) in about.iter().enumerate() {
        canvas.put(
            3,
            y0.saturating_add(1).saturating_add(k),
            line,
            Style::text(Color::Grey),
        );
    }
    let now = match obj {
        Obj::New(Slot::Display(s)) | Obj::New(Slot::Input(s)) => Some(s.clone()),
        _ => None,
    };
    let (wtop, wh) = window(&app.shared, app.size, super::layout::key_rows(app).len());
    let bottom = wtop.saturating_add(wh);
    let top = y0.saturating_add(2).saturating_add(about.len());
    let room = bottom.saturating_sub(top).max(1);
    // the choices' lines come after the label's
    let line = state.line.saturating_sub(1);
    let first = line.saturating_add(1).saturating_sub(room);
    let mut y = top;
    for (i, (title, detail, value)) in choices_for(&app.shared, obj).iter().enumerate().skip(first)
    {
        if y >= bottom {
            break;
        }
        let on = now.is_some() && *value == now;
        let x = choice(canvas, 3, y, title, on, state.line > 0 && i == line);
        let room = app.size.cols.saturating_sub(x);
        canvas.put(x, y, &fit(detail, room), Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
}

/// A template's layer: the ways to make a part of `stage`, one a row,
/// each with what it covers.
fn make_layer(canvas: &mut Canvas, app: &App, state: &flowview::State, stage: Stage) {
    let fonts = font_names(&app.shared);
    let makes = parts::makes(stage, &fonts);
    // the whole-kind choices first, then under MATCH BY the choices that
    // match by one key, and a sentence about them
    let filters = makes
        .iter()
        .position(|(_, _, m)| matches!(m, parts::Make::Filter(_)));
    let list: Vec<(String, String)> = makes.into_iter().map(|(w, d, _)| (w, d)).collect();
    let marks: Vec<(usize, &str)> = filters
        .map(|at| (at, words::MATCH_BY))
        .into_iter()
        .collect();
    list_layer(
        canvas,
        app,
        state.line,
        &words::template_heading(stage.name()),
        words::template_about(stage.name()),
        &list,
        None,
        &marks,
        None,
    );
}

/// A filter's picker: each value the connected devices give for its key,
/// with the devices it matches, and last the entry to type a value.
fn picker_layer(canvas: &mut Canvas, app: &App, state: &flowview::State, p: &flowview::Picker) {
    let list: Vec<(String, String)> = parts::picker(&app.shared.inputs.flow, p.stage, p.key)
        .into_iter()
        .map(|pick| match pick {
            parts::Pick::Value { sel, devices } => (sel, devices.join(", ")),
            parts::Pick::Typed => (
                words::PICKER_TYPE.to_owned(),
                words::PICKER_TYPE_DETAIL.to_owned(),
            ),
        })
        .collect();
    let typing = state.typing.as_deref();
    list_layer(
        canvas,
        app,
        state.line,
        &words::picker_heading(parts::filter_words(p.key)),
        &words::picker_about(p.key),
        &list,
        typing,
        &[],
        None,
    );
}

/// A layer's list: its heading, a grey sentence about it, and one choice
/// a row with its detail in grey; the cursor's in reverse video. While a
/// value is typed, the last row shows it. Each of `marks` puts a small
/// heading, after a blank row, before the choice at its index, and `note`
/// is a grey sentence under the list.
#[expect(clippy::too_many_arguments, reason = "a layer's parts, each its own")]
fn list_layer(
    canvas: &mut Canvas,
    app: &App,
    line: usize,
    heading: &str,
    about: &str,
    list: &[(String, String)],
    typing: Option<&str>,
    marks: &[(usize, &str)],
    note: Option<&str>,
) {
    canvas.put(1, 3, heading, bold(Color::BrightWhite));
    let about = wrap(about, app.size.cols.saturating_sub(4), 2);
    for (k, l) in about.iter().enumerate() {
        canvas.put(3, 4usize.saturating_add(k), l, Style::text(Color::Grey));
    }
    let (wtop, wh) = window(&app.shared, app.size, super::layout::key_rows(app).len());
    let bottom = wtop.saturating_add(wh);
    let top = 5usize.saturating_add(about.len());
    let room = bottom.saturating_sub(top).max(1);
    let first = line.saturating_add(1).saturating_sub(room);
    let mut y = top;
    let last = list.len().saturating_sub(1);
    // the details line up in a column after the longest choice, as far as
    // half the screen
    let column = list
        .iter()
        .map(|(t, _)| width(t))
        .max()
        .unwrap_or(0)
        .saturating_add(9)
        .min(app.size.cols / 2);
    for (i, (title, detail)) in list.iter().enumerate().skip(first) {
        if let Some((_, mark)) = marks.iter().find(|(at, _)| *at == i) {
            y = y.saturating_add(1);
            if y < bottom {
                canvas.put(3, y, mark, bold(Color::BrightWhite));
            }
            y = y.saturating_add(1);
        }
        if y >= bottom {
            break;
        }
        if let (Some(t), true) = (typing, i == last) {
            canvas.put(3, y, &format!(" {t}_ "), Style::PLAIN.reversed());
            y = y.saturating_add(1);
            continue;
        }
        let x = choice(
            canvas,
            3,
            y,
            &fit(title, app.size.cols.saturating_sub(10)),
            false,
            i == line,
        );
        let x = x.max(column);
        let room = app.size.cols.saturating_sub(x).saturating_sub(1);
        canvas.put(x, y, &fit(detail, room), Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
    if let Some(note) = note {
        for l in wrap(note, app.size.cols.saturating_sub(6), 2) {
            if y >= bottom {
                break;
            }
            canvas.put(5, y, &l, Style::text(Color::Grey));
            y = y.saturating_add(1);
        }
    }
}

/// The second layer of the box of other inputs: each input it folds, with
/// the rule that catches it.
fn others_layer(canvas: &mut Canvas, app: &App) {
    let flow = &app.shared.inputs.flow;
    canvas.put(1, 3, words::FLOW_OTHERS_TITLE, bold(Color::BrightWhite));
    let about = wrap(words::FLOW_OTHERS_ABOUT, app.size.cols.saturating_sub(4), 2);
    for (k, l) in about.iter().enumerate() {
        canvas.put(3, 4usize.saturating_add(k), l, Style::text(Color::Grey));
    }
    let vx = (app.size.cols / 2).min(42);
    let (wtop, wh) = window(&app.shared, app.size, super::layout::key_rows(app).len());
    let bottom = wtop.saturating_add(wh);
    let mut y = 5usize.saturating_add(about.len()).saturating_add(1);
    for d in flowview::folded(flow) {
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
            None => words::FLOW_NO_RULE.to_owned(),
        };
        canvas.put(
            vx,
            y,
            &fit(&caught, app.size.cols.saturating_sub(vx).saturating_sub(1)),
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
    use super::super::flowview::tests::lay;
    use super::super::paths::tests::{EDID_A, KBD_PATH, flow};
    use super::super::paths::{Path, Piece};
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
            let doc = document(&l, cols, Some(&at), &Palette::console());
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
                    assert!("│┃┆┇║".contains(c), "{cols}: {w:?} meets {c:?} at {x},{y}");
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
        let mut named = editor();
        named.name = Some("kiosk".into());
        let l = lay(&f, &named, 100);
        let doc = document(&l, 100, None, &Palette::console());
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
            .find(&Obj::Roam(Slot::Input("id:046d:c31c".into())))
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
    fn the_color_file_s_roles_reach_the_titles_borders_and_lines() {
        let f = flow();
        let l = lay(&f, &editor(), 100);
        let (mut pal, _) = crate::tui::flowcolor::parse(
            "[colors]\ndisplay = #7fb3c8 / cyan\nplaced = none\nline = bright-red\n",
        );
        pal.truecolor = true;
        let doc = document(&l, 100, None, &pal);
        let dp = l
            .find(&Obj::Doc("desk".into(), Slot::Display(EDID_A.into())))
            .expect("DP-8");
        let title = doc.style_at(dp.x + 2, dp.title_row()).expect("a style");
        assert_eq!(title.rgb, Some([0x7f, 0xb3, 0xc8]));
        assert_eq!(doc.style_at(dp.x, dp.y), Some(Style::PLAIN));
        let w = l.wires.iter().find(|w| !w.dashed).expect("a whole line");
        let (x, y) = cells(w)[1];
        assert_eq!(
            doc.style_at(x, y).and_then(|s| s.fg),
            Some(Color::BrightRed)
        );
    }

    #[test]
    fn a_fill_covers_the_inside_of_a_box_and_never_its_border() {
        use crate::tui::color::{Back, Normal};
        use crate::tui::flowcolor::LIGHT_TEXT;
        let f = flow();
        let l = lay(&f, &editor(), 100);
        let (mut pal, skipped) = crate::tui::flowcolor::parse(
            "[colors]\nset-aside = #969696 / bright-black\ndisplay-fill = #0f4c57 / cyan\nterminal-fill = #5a4012 / yellow\nfont-set-fill = #4a2a60 / magenta\ninput-fill = #1b4a24 / green\n",
        );
        assert!(skipped.is_empty(), "{skipped:?}");
        pal.truecolor = true;
        let desk = Obj::Doc("desk".into(), Slot::Terminal);
        let doc = document(&l, 100, Some(&desk), &pal);
        let plain = document(&l, 100, Some(&desk), &Palette::console());
        // a fill moves nothing: every character is where it was
        for y in 0..doc.rows {
            for x in 0..doc.cols {
                assert_eq!(doc.at(x, y), plain.at(x, y), "at {x},{y}");
            }
        }
        let back = |x: usize, y: usize| doc.style_at(x, y).and_then(|s| s.back);
        for b in &l.boxes {
            let fill = pal.fill(kind_role(&b.obj)).expect("every kind is filled");
            for y in b.y..b.bottom() {
                for x in b.x..b.right() {
                    // a template, or an empty place, is never filled
                    let inside = y > b.y && y + 1 < b.bottom() && x > b.x && x + 1 < b.right();
                    let want = (inside && !b.template).then_some(fill);
                    assert_eq!(back(x, y), want, "{:?} at {x},{y}", b.obj);
                }
            }
            // nothing around the box is filled
            if b.x > 0 {
                assert_eq!(back(b.x - 1, b.title_row()), None, "{:?}", b.obj);
            }
            assert_eq!(back(b.right(), b.title_row()), None, "{:?}", b.obj);
        }
        // the title is bold in the fill's text color; the highlight keeps
        // its heavy border and underlined title
        let d = l.find(&desk).expect("desk");
        let title = doc.style_at(d.x + 2, d.title_row()).expect("a style");
        assert_eq!(
            (title.rgb, title.bold, title.underline),
            (Some(LIGHT_TEXT), true, true)
        );
        assert_eq!(doc.at(d.x, d.y), Some('┏'));
        // a set-aside box keeps its fill, its text italic in grey
        let taken = l
            .find(&Obj::Pal(Slot::Display(EDID_A.into())))
            .expect("DP-8");
        let t = doc
            .style_at(taken.x + 2, taken.title_row())
            .expect("a style");
        assert_eq!(
            (t.rgb, t.italic, t.back),
            (
                Some([0x96; 3]),
                true,
                Some(Back {
                    basic: Normal::Cyan,
                    rgb: Some([0x0f, 0x4c, 0x57])
                })
            )
        );
        // the border keeps its state's color
        assert_eq!(
            doc.style_at(taken.x, taken.y).and_then(|s| s.fg),
            Some(Color::Grey)
        );
    }

    #[test]
    fn black_text_on_a_basic_fill_is_never_bold() {
        // bold black shows as dark grey on the console, which reads
        // poorly on a light fill
        let f = flow();
        let l = lay(&f, &editor(), 100);
        let (pal, _) =
            crate::tui::flowcolor::parse("[colors]\ndisplay-fill = cyan\nterminal-fill = blue\n");
        let desk = Obj::Doc("desk".into(), Slot::Terminal);
        let dp8 = Obj::Doc("desk".into(), Slot::Display(EDID_A.into()));
        for at in [None, Some(&desk), Some(&dp8)] {
            let doc = document(&l, 100, at, &pal);
            let d = l.find(&dp8).expect("DP-8");
            let s = doc.style_at(d.x + 2, d.title_row()).expect("a style");
            assert_eq!(
                (s.fg, s.bold, s.underline),
                (Some(Color::Black), false, at == Some(&dp8)),
                "{at:?}"
            );
            let t = l.find(&desk).expect("desk");
            let s = doc.style_at(t.x + 2, t.title_row()).expect("a style");
            assert_eq!((s.fg, s.bold), (Some(Color::BrightWhite), true), "{at:?}");
        }
    }

    #[test]
    fn the_whole_inside_of_a_filled_box_reaches_the_screen() {
        let f = flow();
        let l = lay(&f, &editor(), 100);
        let (pal, _) = crate::tui::flowcolor::parse("[colors]\nterminal-fill = blue\n");
        let screen = window_cells(&f, &l, 100, None, 0, l.height, &pal);
        let desk = l
            .find(&Obj::Doc("desk".into(), Slot::Terminal))
            .expect("desk");
        let fill = pal.fill(Role::Terminal);
        assert!(fill.is_some());
        for y in desk.y + 1..desk.bottom() - 1 {
            for x in desk.x + 1..desk.right() - 1 {
                let s = screen.style_at(x, y).expect("a cell");
                assert_eq!(s.back, fill, "at {x},{y}");
            }
        }
        // a blank cell outside every box stays the terminal's own
        assert_eq!(screen.style_at(0, 0), Some(Style::PLAIN));
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
        let doc = window_of(&l, 100, None, top, 12, &Palette::console());
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
