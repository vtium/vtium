//! How the Flow view draws ([`super::flowview`]): the status line,
//! the policy line and the column titles fixed at the top, the key line
//! fixed at the bottom, and between them a window on the document of paths,
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

use super::canvas::{Row, column, field, fit, printable, put, span, wrap};
use super::color::{Color, Colors, Style};
use super::flow::{Flow, Kind};
use super::flowcolor::{Palette, Role};
use super::flowlayout::{
    CardLine, card_about, card_chosen, card_lines, card_path, card_value, choices_for,
    label_target, layout_of, look_names, look_section, message_rows, path_choices, window,
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
                Slot::Look(_) => Stage::Look,
                Slot::Input(_) => Stage::Input,
                Slot::Special(p) => Stage::of(p),
                Slot::Template(st) | Slot::Empty(st) => *st,
            };
            match stage {
                Stage::Display => Role::Display,
                Stage::Terminal => Role::Terminal,
                Stage::Look => Role::FontSet,
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

/// The document of paths as a grid of the console's cells, each holding
/// the character the console shows there and its style. The document is a
/// picture: a line's character depends on the lines that meet at its
/// cell, and a box's fill covers cells that hold no text. So it is drawn
/// cell by cell, and its rows go to the screen through [`view::fill`].
///
/// A cell holds a whole character as the console shows it, a base
/// character with the marks that join it, so a title with an accent or a
/// vowel sign takes the cells the console gives it. A character two cells
/// wide is held in its first cell, and the cell it covers holds an empty
/// string. So a box's right border stays in its column whatever its title
/// holds.
#[derive(Clone, Debug)]
struct Grid {
    cols: usize,
    rows: usize,
    cells: Vec<Vec<(String, Style)>>,
}

impl Grid {
    fn new(cols: usize, rows: usize) -> Grid {
        Grid {
            cols,
            rows,
            cells: vec![vec![(" ".to_owned(), Style::PLAIN); cols]; rows],
        }
    }

    /// Writes `text` from cell `x` of row `y` in `style`, each character
    /// the console shows in the cells the console gives it, as far as the
    /// grid's edge. A wide character that would cross the edge is left
    /// out. A wide character that the text covers only half of becomes a
    /// blank, as the console erases it.
    fn put(&mut self, x: usize, y: usize, text: &str, style: Style) {
        let Some(row) = self.cells.get_mut(y) else {
            return;
        };
        let text = printable(text);
        let mut x = x;
        for cluster in super::canvas::clusters(&text) {
            let (symbol, width) = super::canvas::shown(&cluster);
            let end = x.saturating_add(usize::from(width));
            if end > row.len() {
                break;
            }
            split(row, x);
            split(row, end.saturating_sub(1));
            for (k, cell) in row.iter_mut().enumerate().take(end).skip(x) {
                let text = if k == x {
                    symbol.clone()
                } else {
                    String::new()
                };
                *cell = (text, style);
            }
            x = end;
        }
    }

    /// The first character of the cell, for the tests of the lines that
    /// join at it; `None` for the cell a wide character covers.
    #[cfg(test)]
    fn at(&self, x: usize, y: usize) -> Option<char> {
        self.cells.get(y)?.get(x)?.0.chars().next()
    }

    /// The whole character the cell shows: its base and its marks, or an
    /// empty string for the cell a wide character covers.
    fn symbol(&self, x: usize, y: usize) -> Option<&str> {
        self.cells.get(y)?.get(x).map(|(c, _)| c.as_str())
    }

    /// The style a cell is drawn in.
    fn style_at(&self, x: usize, y: usize) -> Option<Style> {
        self.cells.get(y)?.get(x).map(|(_, s)| *s)
    }

    /// The grid as rows of text, each run of one style a span. A cell that
    /// a wide character covers adds nothing.
    fn into_rows(self) -> Vec<Row> {
        self.cells
            .into_iter()
            .map(|cells| {
                let mut row: Row = Vec::new();
                for (c, style) in cells {
                    match row.last_mut() {
                        Some(s) if s.style == style => s.text.push_str(&c),
                        _ => row.push(span(c, style)),
                    }
                }
                row
            })
            .collect()
    }
}

/// Makes cell `x` of `row` a cell of its own before something is written
/// there: the other half of a wide character that `x` is part of becomes a
/// blank in the wide character's style.
fn split(row: &mut [(String, Style)], x: usize) {
    if row.get(x).is_some_and(|(c, _)| c.is_empty())
        && let Some(lead) = x.checked_sub(1).and_then(|k| row.get_mut(k))
    {
        lead.0 = " ".to_owned();
    }
    if let Some(cover) = row.get_mut(x.saturating_add(1))
        && cover.0.is_empty()
    {
        cover.0 = " ".to_owned();
    }
}

/// Draws the box `b` on `doc`, heavy when it is highlighted.
fn draw_box(doc: &mut Grid, b: &Boxed, highlighted: bool, pal: &Palette) {
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
fn draw_wires(doc: &mut Grid, wires: &[Wire], at: Option<&Obj>, pal: &Palette) {
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

/// The whole document: the paths, the editor and the pieces, with `at`
/// highlighted.
fn document(layout: &Layout, cols: usize, at: Option<&Obj>, pal: &Palette) -> Grid {
    let mut doc = Grid::new(cols, layout.height.max(1));
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
            NoteKind::Looks => emph(pal.style(Role::FontSet)),
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

/// The document as a window `height` rows tall from row `top` shows it:
/// a box whose title is out of sight is left out, so no stray piece of a
/// border shows at the window's edges.
fn window_of(
    layout: &Layout,
    cols: usize,
    at: Option<&Obj>,
    top: usize,
    height: usize,
    pal: &Palette,
) -> Grid {
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
            Slot::Look(l) => flowview::look_words(l.as_deref()),
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
    // the cell after the waiting count, where the rest starts
    let end = 1usize
        .saturating_add(super::canvas::cells(&left))
        .min(cols)
        .saturating_add(3)
        .saturating_add(super::canvas::cells(&waiting));
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
            let drawn = layer(app, state, obj);
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
) -> Grid {
    let doc = window_of(layout, cols, at, top, height, pal);
    let mut shown = Grid::new(cols, height);
    for i in 0..height {
        let y = top.saturating_add(i);
        for x in 0..doc.cols {
            if let (Some(c), Some(s)) = (doc.symbol(x, y), doc.style_at(x, y))
                && !c.is_empty()
                && (c != " " || s.back.is_some())
            {
                shown.put(x, i, c, s);
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

/// The rows of the layer open on `obj`, one for each row of the screen,
/// with the text each row holds from column 0. A layer writes from row 3
/// down, and the window's bottom ends what it writes, so the message and
/// the key line below the window stay clear of it. The status line, the
/// message and the key line are drawn after the layer all the same.
fn layer(app: &App, state: &flowview::State, obj: &Obj) -> Vec<Row> {
    let mut rows: Vec<Row> = vec![Vec::new(); app.size.rows];
    match obj {
        Obj::Doc(_, Slot::Terminal) | Obj::New(Slot::Terminal) => {
            card_layer(&mut rows, app, state, obj);
        }
        Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_) => info_layer(&mut rows, app, obj),
        Obj::Others => others_layer(&mut rows, app),
        Obj::Pal(Slot::Template(st)) | Obj::New(Slot::Empty(st)) => match &state.picker {
            Some(p) => picker_layer(&mut rows, app, state, p),
            None => make_layer(&mut rows, app, state, *st),
        },
        _ => choices_layer(&mut rows, app, state, obj),
    }
    rows
}

/// Writes `text` in `style` on row `y` of a layer from its column `x`, over
/// what the row holds there, and returns the column after the text. A row
/// below the screen takes nothing.
fn at(rows: &mut [Row], x: usize, y: usize, text: &str, style: Style) -> usize {
    match rows.get_mut(y) {
        Some(row) => put(row, x, text, style),
        None => x,
    }
}

/// One choice: `○ text`, or `● text` for what is chosen now, the text in
/// reverse video under the cursor; returns the column after it and its
/// gap.
fn choice(rows: &mut [Row], x: usize, y: usize, text: &str, on: bool, cursor: bool) -> usize {
    let (mark, mark_style) = if on {
        ("●", Style::text(Color::Green))
    } else {
        ("○", Style::text(Color::Grey))
    };
    at(rows, x, y, mark, mark_style);
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
    let end = at(rows, x.saturating_add(2), y, text, style);
    end.saturating_add(3)
}

/// An added path's display or input, or a roaming input: what it
/// matches and where it goes, read-only, and the line that says how to
/// change it.
fn info_layer(rows: &mut [Row], app: &App, obj: &Obj) {
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
    let x = at(
        rows,
        1,
        3,
        &format!("{heading} {title}"),
        bold(Color::BrightWhite),
    );
    at(
        rows,
        x.saturating_add(2),
        3,
        &detail,
        Style::text(Color::Grey),
    );
    let goes = match obj {
        Obj::Doc(name, _) => words::in_terminal(name),
        Obj::Nothing(Slot::Display(_)) => words::TURNED_OFF.to_owned(),
        Obj::Nothing(_) => words::SET_ASIDE.to_owned(),
        _ => words::ROAMS_SENTENCE.to_owned(),
    };
    let vx = VALUE_X.min(app.size.cols / 3);
    let mut y = 5usize;
    for (label, value) in [("Matched by", sel.clone()), ("Where it goes", goes)] {
        at(rows, 3, y, label, Style::text(Color::Grey));
        at(
            rows,
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
        at(rows, 3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
}

/// A line's label, in a reader's words.
fn card_label(flow: &Flow, line: &CardLine) -> String {
    match line {
        CardLine::Name => "Name".to_owned(),
        CardLine::Look => "Look".to_owned(),
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
        CardLine::Part(Piece::Look(_) | Piece::Active | Piece::NoTerminal | Piece::Terminal(_)) => {
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
        _ => path_choices(shared, path, line)
            .into_iter()
            .find(|(_, v)| *v == value)
            .map_or(value, |(w, _)| w),
    }
}

/// A terminal's card: its name, font set, size and the settings that
/// allocate nothing, each changeable here; in the editor, also the parts
/// it joins, which Enter disconnects. An added path's card says how to
/// change its parts.
fn card_layer(rows: &mut [Row], app: &App, state: &flowview::State, obj: &Obj) {
    let flow = &app.shared.inputs.flow;
    let Some(path) = card_path(state, &app.shared, obj) else {
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
    at(rows, 1, 3, &heading, bold(Color::BrightWhite));
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
        // the label is cut with `…` to end a blank cell before the value
        // column, counted in the console's cells, so a long device name
        // never runs under the value. The value starts at the character
        // column that the console shows at VALUE_X, so a wide character in
        // the label does not push the value right.
        at(
            rows,
            3,
            y,
            &fit(&card_label(flow, line), VALUE_X.saturating_sub(4)),
            Style::text(Color::Grey),
        );
        let vx = rows.get(y).map_or(VALUE_X, |r| column(r, VALUE_X));
        let here = i == state.line;
        match (&state.typing, here) {
            (Some(t), true) => {
                let room = app.size.cols.saturating_sub(VALUE_X);
                at(rows, vx, y, &field(t, room), Style::PLAIN.reversed());
            }
            (None, true) => {
                let choices = path_choices(&app.shared, &path, line);
                if choices.is_empty() {
                    let x = at(
                        rows,
                        vx,
                        y,
                        &format!(" {} ", card_words(&app.shared, &path, line)),
                        Style::PLAIN.reversed(),
                    );
                    at(
                        rows,
                        x.saturating_add(2),
                        y,
                        words::ENTER_TO_TYPE,
                        Style::text(Color::Grey),
                    );
                } else {
                    let chosen = card_chosen(&app.shared, &path, line);
                    // the row starts late enough that the cursor's choice
                    // shows, each choice taking its mark, its words and a
                    // gap, counted in the console's cells
                    let room = app.size.cols.saturating_sub(VALUE_X);
                    let span = |a: usize, b: usize| -> usize {
                        choices
                            .iter()
                            .take(b.saturating_add(1))
                            .skip(a)
                            .map(|(w, _)| super::canvas::cells(w).saturating_add(5))
                            .sum()
                    };
                    let mut first = 0usize;
                    while first < state.pick && span(first, state.pick) > room {
                        first = first.saturating_add(1);
                    }
                    let mut x = vx;
                    let mut cell = VALUE_X;
                    for (k, (w, _)) in choices.iter().enumerate().skip(first) {
                        if cell
                            .saturating_add(super::canvas::cells(w))
                            .saturating_add(3)
                            >= app.size.cols
                        {
                            break;
                        }
                        let on = chosen == Some(k) && !matches!(line, CardLine::Part(_));
                        x = choice(rows, x, y, w, on, k == state.pick);
                        cell = cell
                            .saturating_add(super::canvas::cells(w))
                            .saturating_add(5);
                    }
                }
            }
            _ => {
                at(
                    rows,
                    vx,
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
        at(rows, 3, y, &line, Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
    y = y.saturating_add(1).min(bottom.saturating_sub(note.len()));
    for line in note {
        at(rows, 3, y, &line, Style::text(Color::Yellow));
        y = y.saturating_add(1);
    }
    subflow(rows, app, &path, y.saturating_add(1), bottom);
}

/// The terminal's subflow on its card: its look's font set, then the
/// look, then its displays, left to right, and a grey line under them
/// that names the vtlook the look draws, as the Effects tab found it. The
/// Effects tab lists the vtlook's filters, so the line points there.
///
/// References:
/// - ADR-1403 D7, "The terminal's subflow: font set and effects": what
///   the subflow shows.
/// - ADR-1304 D5: the look's vtlook, then the top level's, then the
///   built-in one.
/// - ADR-1304 D6: a vtlook that fails puts the terminal on the built-in
///   vtlook.
fn subflow(rows: &mut [Row], app: &App, path: &Path, y: usize, bottom: usize) {
    if y.saturating_add(3) > bottom || path.active || path.none {
        return;
    }
    let flow = &app.shared.inputs.flow;
    at(rows, 1, y, words::SUBFLOW_TITLE, bold(Color::BrightWhite));
    // a terminal with no look draws with `[look default]` when the draft
    // holds that section, and with the top of vtium.conf when it does not
    let section = look_section(flow, path.look.as_deref());
    let font = section
        .map(|l| l.frit.clone())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| words::FONT_SET_FROM_CONF.to_owned());
    let look = match (&path.look, section) {
        (None, Some(_)) => words::DEFAULT_LOOK_PART.to_owned(),
        (None, None) => words::DEFAULT_LOOK_BOX.to_owned(),
        (Some(l), Some(_)) => words::look_box(l),
        (Some(l), None) => words::look_box(&words::missing_look(l)),
    };
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
            x = at(
                rows,
                x,
                y.saturating_add(1),
                arrow,
                Style::text(Color::Grey),
            );
        }
        let shown = fit(&text, room.saturating_sub(x));
        x = at(rows, x, y.saturating_add(1), &shown, Style::text(color));
    }
    let effects = subflow_effects(&app.shared, section);
    for (i, line) in wrap(&effects, app.size.cols.saturating_sub(4), 2)
        .iter()
        .enumerate()
    {
        let row = y.saturating_add(2).saturating_add(i);
        if row < bottom {
            at(rows, 3, row, line, Style::text(Color::Grey));
        }
    }
}

/// The line under a terminal's subflow: the vtlook that the look
/// `section` draws, or the top of vtium.conf where the terminal has no
/// look section, and whether the Effects tab found it and read it. A look
/// with no `vtlook` line draws the top level's vtlook, and `default`
/// names the built-in one.
///
/// References:
/// - ADR-1304 D5: the look's vtlook, then the top level's, then the
///   built-in one.
fn subflow_effects(shared: &Shared, section: Option<&super::flow::Look>) -> String {
    let name = section
        .and_then(|l| l.vtlook.clone())
        .or_else(|| shared.drafted("vtlook"))
        .filter(|v| v != vtium_settings::vtlook::DEFAULT);
    let Some(name) = name else {
        return words::SUBFLOW_NO_VTLOOK.to_owned();
    };
    match shared.inputs.effects.vtlook(&name).map(|v| &v.read) {
        Some(Ok(_)) => words::subflow_vtlook(&name),
        Some(Err(_)) => words::subflow_vtlook_broken(&name),
        None => words::subflow_vtlook_missing(&name),
    }
}

/// A part's detail: its name, its label, which the person can type, and
/// the choices of what the part matches, from the widest to the
/// narrowest. The label is the detail's first line.
///
/// A part's choices, one a row, broad to narrow, each with what it
/// covers; the cursor's in reverse video and the piece's own marked ●.
fn choices_layer(rows: &mut [Row], app: &App, state: &flowview::State, obj: &Obj) {
    let flow = &app.shared.inputs.flow;
    let Some((kind, sel)) = label_target(obj) else {
        return;
    };
    let (part, heading) = match kind {
        Kind::Display => (words::DISPLAY_PART, words::CHOOSE_DISPLAY),
        Kind::Input => (words::INPUT_PART, words::CHOOSE_INPUT),
    };
    let name = paths::title(flow, kind, &sel).0;
    let x = at(rows, 1, 3, part, bold(Color::BrightWhite));
    let role = match kind {
        Kind::Display => Role::Display,
        Kind::Input => Role::Input,
    };
    at(
        rows,
        x.saturating_add(2),
        3,
        &fit(&name, app.size.cols.saturating_sub(x).saturating_sub(3)),
        emph(app.shared.inputs.flow_colors.style(role)),
    );
    // the label's line
    let doc = paths::read(flow);
    let label = doc.label(kind, &sel).unwrap_or_default().to_owned();
    at(rows, 3, 5, words::LABEL_FIELD, Style::text(Color::Grey));
    let vx = 12usize;
    match &state.typing {
        Some(t) => {
            let x = at(rows, vx, 5, &format!(" {t}_ "), Style::PLAIN.reversed());
            at(
                rows,
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
            let x = at(rows, vx, 5, &format!(" {shown} "), style);
            at(
                rows,
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
    at(rows, 1, y0, heading, bold(Color::BrightWhite));
    let about = wrap(words::CHOOSE_ABOUT, app.size.cols.saturating_sub(4), 2);
    for (k, line) in about.iter().enumerate() {
        at(
            rows,
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
        let x = choice(rows, 3, y, title, on, state.line > 0 && i == line);
        let room = app.size.cols.saturating_sub(x);
        at(rows, x, y, &fit(detail, room), Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
}

/// A template's layer: the ways to make a part of `stage`, one a row,
/// each with what it covers.
fn make_layer(rows: &mut [Row], app: &App, state: &flowview::State, stage: Stage) {
    let looks = look_names(&app.shared);
    let makes = parts::makes(stage, &looks);
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
        rows,
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
fn picker_layer(rows: &mut [Row], app: &App, state: &flowview::State, p: &flowview::Picker) {
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
        rows,
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
    rows: &mut [Row],
    app: &App,
    line: usize,
    heading: &str,
    about: &str,
    list: &[(String, String)],
    typing: Option<&str>,
    marks: &[(usize, &str)],
    note: Option<&str>,
) {
    at(rows, 1, 3, heading, bold(Color::BrightWhite));
    let about = wrap(about, app.size.cols.saturating_sub(4), 2);
    for (k, l) in about.iter().enumerate() {
        at(
            rows,
            3,
            4usize.saturating_add(k),
            l,
            Style::text(Color::Grey),
        );
    }
    let (wtop, wh) = window(&app.shared, app.size, super::layout::key_rows(app).len());
    let bottom = wtop.saturating_add(wh);
    let top = 5usize.saturating_add(about.len());
    let room = bottom.saturating_sub(top).max(1);
    let first = line.saturating_add(1).saturating_sub(room);
    let mut y = top;
    let last = list.len().saturating_sub(1);
    // the details line up in a column after the longest choice, as far as
    // half the screen, counted in the console's cells so a wide character
    // in a name moves no detail out of its column
    let detail_x = list
        .iter()
        .map(|(t, _)| super::canvas::cells(t))
        .max()
        .unwrap_or(0)
        .saturating_add(9)
        .min(app.size.cols / 2);
    for (i, (title, detail)) in list.iter().enumerate().skip(first) {
        if let Some((_, mark)) = marks.iter().find(|(at, _)| *at == i) {
            y = y.saturating_add(1);
            if y < bottom {
                at(rows, 3, y, mark, bold(Color::BrightWhite));
            }
            y = y.saturating_add(1);
        }
        if y >= bottom {
            break;
        }
        if let (Some(t), true) = (typing, i == last) {
            let room = app.size.cols.saturating_sub(3);
            at(rows, 3, y, &field(t, room), Style::PLAIN.reversed());
            y = y.saturating_add(1);
            continue;
        }
        let title = fit(title, app.size.cols.saturating_sub(10));
        choice(rows, 3, y, &title, false, i == line);
        // the choice takes its mark, a blank, its words and a gap of three
        let cell = super::canvas::cells(&title).saturating_add(8).max(detail_x);
        let x = rows.get(y).map_or(cell, |r| column(r, cell));
        let room = app.size.cols.saturating_sub(cell).saturating_sub(1);
        at(rows, x, y, &fit(detail, room), Style::text(Color::Grey));
        y = y.saturating_add(1);
    }
    if let Some(note) = note {
        for l in wrap(note, app.size.cols.saturating_sub(6), 2) {
            if y >= bottom {
                break;
            }
            at(rows, 5, y, &l, Style::text(Color::Grey));
            y = y.saturating_add(1);
        }
    }
}

/// The second layer of the box of other inputs: each input it folds, with
/// the rule that catches it.
fn others_layer(rows: &mut [Row], app: &App) {
    let flow = &app.shared.inputs.flow;
    at(
        rows,
        1,
        3,
        words::FLOW_OTHERS_TITLE,
        bold(Color::BrightWhite),
    );
    let about = wrap(words::FLOW_OTHERS_ABOUT, app.size.cols.saturating_sub(4), 2);
    for (k, l) in about.iter().enumerate() {
        at(
            rows,
            3,
            4usize.saturating_add(k),
            l,
            Style::text(Color::Grey),
        );
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
        // the name is cut in the console's cells to end before the rule's
        // column, and the rule starts at the character column the console
        // shows there
        at(
            rows,
            3,
            y,
            &fit(&dev.name, vx.saturating_sub(4)),
            Style::text(Color::White),
        );
        let x = rows.get(y).map_or(vx, |r| column(r, vx));
        let caught = match dev.rule.and_then(|r| flow.rules.get(r).map(|x| (r, x))) {
            Some((r, x)) => format!(
                "rule {}: {} -> {}",
                r.saturating_add(1),
                x.selector,
                x.target
            ),
            None => words::FLOW_NO_RULE.to_owned(),
        };
        at(
            rows,
            x,
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
        let look = l
            .find(&Obj::Pal(Slot::Look(Some("big".into()))))
            .expect("a look");
        assert_eq!(title(look), Some(Color::Magenta));
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

    /// The Flow view at 80×25 on the fixture, with `more` inputs that no
    /// rule names and that fold into the box of other inputs, one of them
    /// with a name longer than a card's label column.
    fn flow_app(more: usize) -> App {
        let mut i = crate::tui::fixture::inputs();
        i.view = crate::tui::model::View::Flow;
        let first = i.flow.inputs.first().cloned().expect("an input");
        for k in 0..more {
            i.flow.inputs.push(crate::tui::flow::Device {
                name: format!("A rather long input device name number {k}"),
                keys: vec![format!("id:aaaa:{k:04x}")],
                found: true,
                rule: None,
                ..first.clone()
            });
        }
        App::new(i, 80, 25).0
    }

    /// The line under a terminal's subflow names the vtlook its look
    /// draws: the look's own, else the top level's, else the built-in
    /// one. It says when the vtlook has an error or this machine has no
    /// vtlook of that name, because vtium then renders the built-in one.
    ///
    /// References:
    /// - ADR-1304 D5, D6: which vtlook a look draws, and the built-in
    ///   vtlook in place of one that fails.
    #[test]
    fn the_subflow_names_the_vtlook_its_look_draws() {
        use super::super::effects_found::{From, Vtlook};
        let mut i = super::super::fixture::inputs();
        let vtlook = |name: &str, read: Result<Vec<String>, String>| Vtlook {
            name: name.to_owned(),
            file: Some(format!("/etc/vtium/looks/{name}.vtlook").into()),
            from: From::Administrator,
            hidden: false,
            read,
            files: Vec::new(),
        };
        i.effects.vtlooks = vec![
            vtlook("amber", Ok(vec!["tint".to_owned()])),
            vtlook("cracked", Err("line 2: no such filter".to_owned())),
        ];
        i.conf = Some("version = 1\nvtlook = amber\n".to_owned());
        let app = App::new(i, 80, 25).0;
        let look = |vtlook: Option<&str>| super::super::flow::Look {
            name: "warm".to_owned(),
            frit: String::new(),
            points: 0,
            keys: Vec::new(),
            vtlook: vtlook.map(str::to_owned),
            fonts: Vec::new(),
            problem: None,
        };
        let said = |l: Option<super::super::flow::Look>| subflow_effects(&app.shared, l.as_ref());
        // the look's own vtlook, then the top level's
        assert_eq!(
            said(Some(look(Some("amber")))),
            words::subflow_vtlook("amber")
        );
        assert_eq!(said(Some(look(None))), words::subflow_vtlook("amber"));
        assert_eq!(said(None), words::subflow_vtlook("amber"));
        assert_eq!(said(Some(look(Some("default")))), words::SUBFLOW_NO_VTLOOK);
        assert_eq!(
            said(Some(look(Some("cracked")))),
            words::subflow_vtlook_broken("cracked")
        );
        assert_eq!(
            said(Some(look(Some("gone")))),
            words::subflow_vtlook_missing("gone")
        );
    }

    /// The text of a layer's row, in characters from column 0.
    fn text(row: &Row) -> Vec<char> {
        row.iter().flat_map(|s| s.text.chars()).collect()
    }

    #[test]
    fn a_layer_writes_nothing_on_the_message_or_the_key_line() {
        let mut app = flow_app(40);
        // the longest message makes the window as short as it gets
        app.shared.message = Some("word ".repeat(200));
        let (top, height) = window(
            &app.shared,
            app.size,
            super::super::layout::key_rows(&app).len(),
        );
        let bottom = top + height;
        let mut editor = editor();
        for k in 0..30 {
            editor.pick(Piece::Input(format!("id:aaaa:{k:04x}")));
        }
        let state = |line: usize| flowview::State {
            line,
            new: editor.clone(),
            ..flowview::State::default()
        };
        let picker = |line: usize| flowview::State {
            picker: Some(flowview::Picker {
                stage: Stage::Input,
                key: "name",
                from_new: false,
            }),
            ..state(line)
        };
        let input = app
            .shared
            .inputs
            .flow
            .inputs
            .iter()
            .find_map(|d| d.keys.first().cloned())
            .expect("an input with a key");
        // each layer, and whether what it shows is long enough to fill
        // the window: the editor's card with its 33 parts, the 40 inputs
        // of the box of other inputs, and the picker over their names
        let template = Obj::Pal(Slot::Template(Stage::Input));
        let cases = [
            (Obj::New(Slot::Terminal), state(0), true),
            (Obj::Doc("desk".into(), Slot::Terminal), state(0), false),
            (Obj::Others, state(0), true),
            (template.clone(), state(0), false),
            (template, picker(0), true),
            (Obj::Pal(Slot::Input(input)), state(0), false),
        ];
        for (obj, state, fills) in cases {
            let mut reached = 0;
            for line in [0, 1, 5, 20, 39, 60] {
                let state = flowview::State {
                    line,
                    ..state.clone()
                };
                let rows = layer(&app, &state, &obj);
                assert_eq!(rows.len(), 25);
                for (y, row) in rows.iter().enumerate().skip(bottom) {
                    assert!(
                        row.is_empty(),
                        "{obj:?} line {line} writes row {y}: {row:?}"
                    );
                }
                reached = reached.max(rows.iter().rposition(|r| !r.is_empty()).unwrap_or(0));
            }
            // a layer that fills the window reaches its last row, so the
            // check above has something to see
            if fills {
                assert_eq!(reached, bottom - 1, "{obj:?}");
            }
        }
    }

    /// A row as the screen shows it, one entry for each cell: each
    /// character takes as many cells as the console's width rule gives it,
    /// and the cell a wide character covers holds `\0`.
    fn on_screen(row: &Row) -> Vec<char> {
        let mut out = Vec::new();
        for c in text(row) {
            let width = super::super::backend::console_width(&c.to_string()).max(1);
            out.push(c);
            out.extend(std::iter::repeat_n('\0', usize::from(width) - 1));
        }
        out
    }

    /// The Flow view at 80×25 with one more input, called `name`, whose
    /// key is `id:aaaa:0000`.
    fn app_with_input(name: &str) -> App {
        let mut i = crate::tui::fixture::inputs();
        i.view = crate::tui::model::View::Flow;
        let first = i.flow.inputs.first().cloned().expect("an input");
        i.flow.inputs.push(crate::tui::flow::Device {
            name: name.to_owned(),
            keys: vec!["id:aaaa:0000".to_owned()],
            found: true,
            rule: None,
            ..first
        });
        App::new(i, 80, 25).0
    }

    /// The layer of a card in the editor that holds the input called
    /// `name`, with the cursor on `line`, or on the input's own line.
    fn card_with_input(name: &str, line: Option<usize>) -> (App, Vec<Row>) {
        let app = app_with_input(name);
        let mut editor = Path::default();
        editor.pick(Piece::Input("id:aaaa:0000".into()));
        let part = CardLine::Part(Piece::Input("id:aaaa:0000".into()));
        let own = card_lines(&editor, true)
            .iter()
            .position(|l| *l == part)
            .expect("the part's line");
        let state = flowview::State {
            new: editor,
            line: line.unwrap_or(own),
            ..flowview::State::default()
        };
        let rows = layer(&app, &state, &Obj::New(Slot::Terminal));
        (app, rows)
    }

    /// The screen row of the card's line that starts with `Input`.
    fn input_line(rows: &[Row]) -> Vec<char> {
        rows.iter()
            .map(on_screen)
            .find(|t| t.len() > 3 && t[3..].starts_with(&['I', 'n', 'p', 'u', 't']))
            .expect("the part's line")
    }

    #[test]
    fn a_long_part_label_is_cut_before_the_value_column_and_the_value_shows_whole() {
        let name = "A rather long input device name number 0";
        // the cursor on the card's first line, then on the part's own
        for line in [Some(0), None] {
            let (_, rows) = card_with_input(name, line);
            let row = input_line(&rows);
            // the label fills the room before the value column, less one
            // blank cell, and ends in `…`
            let label: String = format!("Input {name}").chars().take(VALUE_X - 5).collect();
            let shown: String = row[3..VALUE_X].iter().collect();
            assert_eq!(shown, format!("{label}… "), "{line:?}");
            let after: String = row[VALUE_X..].iter().collect();
            if line.is_some() {
                assert_eq!(after.trim_end(), words::CONNECTED);
            } else {
                // the part's choices start at the value column with a mark
                assert!(after.starts_with(['○', '●']), "{after}");
            }
            assert!(!after.contains("number"), "{line:?}: {after}");
        }
    }

    #[test]
    fn a_long_typed_name_scrolls_so_its_end_and_the_cursor_stay_on_the_screen() {
        // a card's name may grow to 64 characters, and at 80 columns the
        // value column has 46 cells
        let app = app_with_input("kbd");
        let typed = format!("desk{}END", "x".repeat(57));
        let state = flowview::State {
            line: 0,
            typing: Some(typed),
            ..flowview::State::default()
        };
        let rows = layer(&app, &state, &Obj::New(Slot::Terminal));
        let row = on_screen(&rows[5]);
        assert!(row.len() <= 80, "{} cells", row.len());
        let shown: String = row[VALUE_X..].iter().collect();
        assert!(shown.starts_with(" …x"), "{shown:?}");
        assert!(shown.trim_end().ends_with("xEND_"), "{shown:?}");
    }

    #[test]
    fn a_long_typed_filter_value_scrolls_so_its_end_and_the_cursor_stay_on_the_screen() {
        let app = app_with_input("kbd");
        let typed = format!("{}键END", "y".repeat(190));
        let mut rows: Vec<Row> = vec![Vec::new(); 25];
        let list = [
            ("a".to_owned(), "b".to_owned()),
            ("type".to_owned(), String::new()),
        ];
        list_layer(
            &mut rows,
            &app,
            1,
            "H",
            "about",
            &list,
            Some(&typed),
            &[],
            None,
        );
        let row = rows
            .iter()
            .map(on_screen)
            .find(|r| r.contains(&'_'))
            .expect("the typed value's row");
        assert!(row.len() <= 80, "{} cells", row.len());
        let shown: String = row.iter().filter(|c| **c != '\0').collect();
        assert!(shown.trim_end().ends_with("y键END_"), "{shown:?}");
    }

    #[test]
    fn a_wide_part_label_is_cut_by_the_console_s_cells_before_the_value_column() {
        // twenty characters two cells wide: forty cells, under the thirty
        // characters the label has room for, so cutting by characters
        // keeps them all
        let name = "键盘".repeat(10);
        let (_, rows) = card_with_input(&name, Some(0));
        let row = input_line(&rows);
        // `Input ` takes 6 cells, eleven wide characters 22, `…` one, so
        // the label takes 29 of its 30 cells, and two blank cells part it
        // from the value
        let wide: String = name.chars().take(11).flat_map(|c| [c, '\0']).collect();
        let shown: String = row[3..VALUE_X].iter().collect();
        assert_eq!(shown, format!("Input {wide}…  "));
        let after: String = row[VALUE_X..].iter().collect();
        assert_eq!(after.trim_end(), words::CONNECTED);
    }

    #[test]
    fn a_picker_s_details_share_one_column_of_cells_whatever_the_names() {
        // ten characters two cells wide in one name
        let name = "键盘".repeat(5);
        let app = app_with_input(&name);
        let state = flowview::State {
            picker: Some(flowview::Picker {
                stage: Stage::Input,
                key: "name",
                from_new: false,
            }),
            ..flowview::State::default()
        };
        let rows = layer(&app, &state, &Obj::Pal(Slot::Template(Stage::Input)));
        // a value's row: its mark, `name:` and the name without blanks,
        // then blanks, then the names of the devices it matches
        let starts: Vec<(usize, Vec<char>)> = rows
            .iter()
            .map(on_screen)
            .filter(|r| r.len() > 10 && r[5..].starts_with(&['n', 'a', 'm', 'e', ':']))
            .map(|r| {
                let gap = r.iter().skip(5).position(|c| *c == ' ').expect("a gap") + 5;
                let at = r
                    .iter()
                    .skip(gap)
                    .position(|c| *c != ' ')
                    .expect("a detail")
                    + gap;
                (at, r)
            })
            .collect();
        assert!(starts.len() > 1, "{starts:?}");
        let column = starts[0].0;
        assert!(starts.iter().all(|(at, _)| *at == column), "{starts:?}");
        let wide = starts
            .iter()
            .find(|(_, r)| r.contains(&'键'))
            .expect("the wide name's row");
        let detail: String = wide.1[column..].iter().filter(|c| **c != '\0').collect();
        assert_eq!(detail.trim_end(), name);
    }

    #[test]
    fn the_box_of_other_inputs_cuts_a_wide_name_before_the_rule_s_column() {
        // twenty-five characters two cells wide: fifty cells, more than
        // the 36 the name has room for at 80 columns
        let name = "键盘".repeat(12) + "键";
        let app = app_with_input(&name);
        let rows = layer(&app, &flowview::State::default(), &Obj::Others);
        let row = rows
            .iter()
            .map(on_screen)
            .find(|r| r.contains(&'键'))
            .expect("the wide name's row");
        // the rule's column is half of 80 columns, so the name has 36
        // cells: seventeen wide characters, `…` and two blank cells
        let wide: String = name.chars().take(17).flat_map(|c| [c, '\0']).collect();
        let shown: String = row[3..40].iter().collect();
        assert_eq!(shown, format!("{wide}…  "));
        let rule: String = row[40..].iter().collect();
        assert!(rule.starts_with(words::FLOW_NO_RULE), "{rule}");
    }

    /// The character the console shows at screen cell `cell` of `row`, by
    /// the console's clusters and widths.
    fn shown_at(row: &Row, cell: usize) -> String {
        let text: String = row.iter().map(|s| s.text.as_str()).collect();
        let mut used = 0usize;
        for c in super::super::canvas::clusters(&text) {
            let (symbol, width) = super::super::canvas::shown(&c);
            used += usize::from(width);
            if used > cell {
                return symbol;
            }
        }
        String::new()
    }

    #[test]
    fn a_title_with_marks_or_wide_characters_leaves_the_box_s_border_in_place() {
        let f = flow();
        for title in [
            "e\u{301}e\u{301}e\u{301}",
            "键盘键盘",
            "க\u{BCA}a",
            "\u{301}x",
        ] {
            let mut l = lay(&f, &Path::default(), 100);
            let at = l
                .boxes
                .iter()
                .position(|b| b.obj == Obj::Doc("desk".into(), Slot::Terminal))
                .expect("desk");
            l.boxes[at].title = title.to_owned();
            let b = l.boxes[at].clone();
            let rows = document(&l, 100, None, &Palette::console()).into_rows();
            let row = &rows[b.title_row()];
            for (cell, want) in [(b.x, "│"), (b.right() - 1, "│")] {
                assert_eq!(shown_at(row, cell), want, "{title:?} at {cell}: {row:?}");
            }
            let text: String = row.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(
                super::super::canvas::cells(&text),
                100,
                "{title:?}: {text:?}"
            );
        }
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
