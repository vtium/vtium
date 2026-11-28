//! How the Flow page draws: a window on the document of
//! [`super::layout`], scrolled to the row the tab keeps at the viewer's
//! top, with a label where a line leaves the window, such as `↑ desk`; and
//! [`Sheet`], the writer the tab's other screens draw their lines with.
//!
//! The document is a picture made of cells: a line's character depends on
//! the lines that meet at its cell, so it is drawn cell by cell on a grid
//! and then written into the buffer by the console's width rule.
//! A box's border says its state and its kind of border says what kind of
//! box it is: double for a template, dashed in NEW PATH and for the active
//! terminal, heavy for the highlighted box. A solid line is a connection
//! in force, a dotted line one still being composed or a roaming input's,
//! and a heavy line a connection of the highlighted box.
//!
//! References:
//! - ADR-1403 D7, "Colors, borders and lines" and "One document with
//!   three sections": the borders, the lines, and the label where a line
//!   leaves the window.
//! - ADR-1206 D1, "The console's width rule decides where the cursor
//!   goes": each character takes the cells the console gives it.

use ratatui_core::buffer::{Buffer, CellDiffOption, CellWidth};
use ratatui_core::layout::Rect;
use ratatui_core::widgets::Widget;
use ratatui_widgets::clear::Clear;
use vtium_config_core::flow::{Flow, Kind};
use vtium_config_widgets::canvas::{clusters, fit, printable, shown};
use vtium_config_widgets::color::Style;
use vtium_config_widgets::roles::Role;

use super::colors::{
    bold, fill_role, marked, mood_role, note_bold, note_role, obj_stage, stage_role,
};
use super::layout::{BOX_ROWS, Boxed, Layout, Mood, Obj, Slot, Wire, look_words};
use crate::parts;
use crate::paths;
use crate::words;

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
/// whole and dashed, and a template's double border.
const LIGHT: [char; 6] = ['─', '│', '┌', '┐', '└', '┘'];
const HEAVY: [char; 6] = ['━', '┃', '┏', '┓', '┗', '┛'];
const LIGHT_DASHED: [char; 6] = ['┄', '┆', '┌', '┐', '└', '┘'];
const HEAVY_DASHED: [char; 6] = ['┅', '┇', '┏', '┓', '┗', '┛'];
const DOUBLE: [char; 6] = ['═', '║', '╔', '╗', '╚', '╝'];

/// The document as a grid of the console's cells, each holding the
/// character the console shows there and its style. A cell holds a whole
/// character, a base character with the marks that join it, and a
/// character two cells wide is held in its first cell while the cell it
/// covers holds an empty string. So a box's right border stays in its
/// column whatever its title holds.
///
/// A grid holds only the rows a window shows, from the document's row
/// `top`, so a page of hundreds of rows costs no more to draw than the
/// viewer it fills. Each method takes the document's rows, and leaves out
/// what falls outside the window.
#[derive(Clone, Debug)]
struct Grid {
    cols: usize,
    top: usize,
    rows: usize,
    cells: Vec<Vec<(String, Style)>>,
}

impl Grid {
    fn new(cols: usize, top: usize, rows: usize) -> Grid {
        Grid {
            cols,
            top,
            rows,
            cells: vec![vec![(" ".to_owned(), Style::PLAIN); cols]; rows],
        }
    }

    /// The grid's row of the document's row `y`, while the window shows it.
    fn row(&self, y: usize) -> Option<usize> {
        y.checked_sub(self.top).filter(|r| *r < self.rows)
    }

    /// Writes `text` from cell `x` of row `y` in `style`, each character in
    /// the cells the console gives it, as far as the grid's edge. A wide
    /// character that would cross the edge is left out, and one that the
    /// text covers half of becomes a blank, as the console erases it.
    fn put(&mut self, x: usize, y: usize, text: &str, style: Style) {
        let Some(row) = self.row(y).and_then(|r| self.cells.get_mut(r)) else {
            return;
        };
        let text = printable(text);
        let mut x = x;
        for cluster in clusters(&text) {
            let (symbol, width) = shown(&cluster);
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

    /// The cell at `x` of the document's row `y`: its symbol and its
    /// style.
    fn cell(&self, x: usize, y: usize) -> Option<&(String, Style)> {
        self.cells.get(self.row(y)?)?.get(x)
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

/// Draws the box `b` on `doc`, heavy when it is highlighted, its roles
/// drawn in color where `color` holds.
fn draw_box(doc: &mut Grid, b: &Boxed, highlighted: bool, color: bool) {
    let border = mood_role(b.mood).style(color);
    let border = if highlighted || b.mood == Mood::Picked {
        bold(border)
    } else {
        border
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
    let stage = obj_stage(&b.obj);
    // a template is never filled: it is the mould of a part, not a part
    let fill = if b.template {
        Style::PLAIN
    } else {
        fill_role(stage).style(color)
    };
    for i in 1..BOX_ROWS.saturating_sub(1) {
        let y = b.y.saturating_add(i);
        doc.put(b.x, y, &v.to_string(), border);
        doc.put(b.x.saturating_add(1), y, &" ".repeat(inner), fill);
        doc.put(b.right().saturating_sub(1), y, &v.to_string(), border);
    }
    doc.put(
        b.x,
        b.bottom().saturating_sub(1),
        &format!("{bl}{across}{br}"),
        border,
    );
    let room = b.w.saturating_sub(4);
    let on_fill = |s: Style| Style {
        bg: fill.bg,
        back: fill.back,
        ..s
    };
    let title = on_fill(stage_role(stage).style(color));
    let title = match (highlighted, b.mood) {
        (true, _) => marked(title),
        (false, Mood::Picked) => bold(title),
        (false, _) => title,
    };
    doc.put(
        b.x.saturating_add(2),
        b.title_row(),
        &fit(&b.title, room),
        title,
    );
    doc.put(
        b.x.saturating_add(2),
        b.title_row().saturating_add(1),
        &fit(&b.detail, room),
        on_fill(Role::Dim.style(color)),
    );
}

/// The cells of a wire's path, in order from its first point to its last.
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
/// where they belong to the highlighted box, each ending in an arrow at
/// its terminal. A cell that only dotted lines cross is drawn dotted, and
/// one that only a roaming line crosses is dim.
fn draw_wires(doc: &mut Grid, wires: &[Wire], at: Option<&Obj>, color: bool) {
    let line = Role::Line.style(color);
    let heavy_style = bold(line);
    let dim = Role::Dim.style(color);
    let (cols, top, rows) = (doc.cols, doc.top, doc.rows);
    // each cell's weight up, down, left and right, whether a whole line
    // crosses it, and whether a line other than a roaming stub does, for
    // the rows the window shows
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
                let Some(r) = y.checked_sub(top).filter(|r| *r < rows && x < cols) else {
                    continue;
                };
                if let Some(cell) = grid.get_mut(r.saturating_mul(cols).saturating_add(x)) {
                    if let Some(s) = cell.0.get_mut(side) {
                        *s = (*s).max(weight);
                    }
                    cell.1 |= !w.dashed;
                    cell.2 |= !w.stub;
                }
            }
        }
    }
    for r in 0..rows {
        let y = top.saturating_add(r);
        for x in 0..cols {
            let Some((c, whole, other)) = grid.get(r.saturating_mul(cols).saturating_add(x)) else {
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
                heavy_style
            } else if !*other {
                dim
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
            (true, true, _) => ("▶", heavy_style),
            (true, false, _) => ("◀", heavy_style),
            (false, true, _) => ("►", line),
            (false, false, false) => ("◄", line),
            (false, false, true) => ("◄", dim),
        };
        doc.put(last.0, last.1, glyph, style);
    }
}

/// The rows `top` to `top + height` of the document, with `at`
/// highlighted. A box whose title row is out of sight is left out, so no
/// stray piece of a border shows at the window's edges.
fn window(
    layout: &Layout,
    cols: usize,
    at: Option<&Obj>,
    top: usize,
    height: usize,
    color: bool,
) -> Grid {
    let mut doc = Grid::new(cols, top, height);
    draw_wires(&mut doc, &layout.wires, at, color);
    let end = top.saturating_add(height);
    for b in &layout.boxes {
        if b.bottom() <= top || b.y >= end {
            continue;
        }
        let seen = b.title_row() >= top && b.title_row() < end;
        if seen {
            draw_box(&mut doc, b, Some(&b.obj) == at, color);
        } else {
            for y in b.y..b.bottom() {
                doc.put(b.x, y, &" ".repeat(b.w), Style::PLAIN);
            }
        }
    }
    for n in &layout.notes {
        let style = note_role(n.kind).style(color);
        let style = if note_bold(n.kind) {
            bold(style)
        } else {
            style
        };
        doc.put(n.x, n.y, &n.text, style);
    }
    for y in &layout.rules {
        doc.put(
            1,
            *y,
            &"─".repeat(cols.saturating_sub(2)),
            Role::Dim.style(color),
        );
    }
    doc
}

/// The words a box goes by on a line's label at the window's edge.
fn obj_name(flow: &Flow, o: &Obj) -> String {
    match o {
        Obj::Doc(n, Slot::Terminal) => n.clone(),
        Obj::New(Slot::Terminal) => words::NEW_PATH.to_lowercase(),
        Obj::Nothing(Slot::Terminal) => words::NO_TERMINAL.to_owned(),
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) | Obj::Nothing(s) => match s {
            Slot::Display(sel) => paths::title(flow, Kind::Display, sel).0,
            Slot::Input(sel) => paths::title(flow, Kind::Input, sel).0,
            Slot::Look(l) => look_words(l.as_deref()),
            Slot::Special(p) => parts::special_title(p),
            Slot::Template(st) | Slot::Empty(st) => st.name().to_owned(),
            Slot::Terminal => String::new(),
        },
        Obj::Roam(Slot::Input(sel)) => paths::title(flow, Kind::Input, sel).0,
        Obj::Roam(_) => words::ACTIVE_TERMINAL.to_owned(),
        Obj::Others => words::OTHERS_CRUMB.to_owned(),
    }
}

/// Where a line whose other end is out of sight meets the window's edge,
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

/// Draws the window on the document into `area`: the rows of the document
/// from `top`, with `at` highlighted and a label where a line leaves the
/// window.
pub fn draw_page(
    flow: &Flow,
    layout: &Layout,
    at: Option<&Obj>,
    top: usize,
    buf: &mut Buffer,
    area: Rect,
    color: bool,
) {
    let (cols, height) = (usize::from(area.width), usize::from(area.height));
    let mut doc = window(layout, cols, at, top, height, color);
    for (x, y, text) in edge_labels(flow, layout, top, height) {
        doc.put(x, y, &text, Role::Heading.style(color));
    }
    Clear.render(area, buf);
    for i in 0..height {
        let y = top.saturating_add(i);
        let row = area.y.saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
        let mut x = 0usize;
        while x < cols {
            let Some((symbol, style)) = doc.cell(x, y) else {
                break;
            };
            if !symbol.is_empty() {
                let at = area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX));
                write(buf, at, row, area.right(), symbol, *style);
            }
            x = x.saturating_add(1);
        }
    }
}

/// Writes the one character `symbol` at `x`, `y` in `style`, in the cells
/// the console gives it, and returns the cell after it. A character that
/// would cross `right` is left out.
fn write(buf: &mut Buffer, x: u16, y: u16, right: u16, symbol: &str, style: Style) -> u16 {
    let width = vtium_config_widgets::backend::console_width(symbol).max(1);
    if x.saturating_add(width) > right {
        return right;
    }
    let cell = &mut buf[(x, y)];
    cell.set_symbol(symbol).set_style(style.cell());
    if symbol.cell_width() != width
        && let Some(w) = std::num::NonZeroU16::new(width)
    {
        cell.set_diff_option(CellDiffOption::ForcedWidth(w));
    }
    // the cell that a wide character covers holds nothing of its own
    for covered in 1..width {
        buf[(x.saturating_add(covered), y)].reset();
    }
    x.saturating_add(width)
}

/// A writer of lines of text into an area of the screen, for the Flow
/// tab's screens other than the page: each text at a cell of a row, in a
/// role, by the console's width rule, cut at the area's right edge.
pub struct Sheet<'a> {
    buf: &'a mut Buffer,
    area: Rect,
    color: bool,
}

impl<'a> Sheet<'a> {
    /// A sheet over `area` of `buf`, cleared, drawn in color where `color`
    /// holds.
    pub fn new(buf: &'a mut Buffer, area: Rect, color: bool) -> Sheet<'a> {
        Clear.render(area, buf);
        Sheet { buf, area, color }
    }

    /// Writes `text` in `style` from the area's cell `x` of its row `y`,
    /// and returns the cell after it. A row below the area takes nothing.
    pub fn put_style(&mut self, x: usize, y: usize, text: &str, style: Style) -> usize {
        let Ok(dy) = u16::try_from(y) else {
            return x;
        };
        if dy >= self.area.height {
            return x;
        }
        let row = self.area.y.saturating_add(dy);
        let right = self.area.right();
        let mut at = self
            .area
            .x
            .saturating_add(u16::try_from(x).unwrap_or(u16::MAX));
        for cluster in clusters(&printable(text)) {
            if at >= right {
                break;
            }
            let (symbol, _) = shown(&cluster);
            at = write(self.buf, at, row, right, &symbol, style);
        }
        usize::from(at.saturating_sub(self.area.x))
    }

    /// Writes `text` in the role `role`.
    pub fn put(&mut self, x: usize, y: usize, text: &str, role: Role) -> usize {
        let style = role.style(self.color);
        self.put_style(x, y, text, style)
    }

    /// Writes `text` in the role `role`, in bold.
    pub fn put_bold(&mut self, x: usize, y: usize, text: &str, role: Role) -> usize {
        let style = bold(role.style(self.color));
        self.put_style(x, y, text, style)
    }

    /// Paints row `y` across the area in the highlighted row's role, under
    /// what is written on it next.
    pub fn highlight(&mut self, y: usize) {
        let w = usize::from(self.area.width);
        self.put(0, y, &" ".repeat(w), Role::RowHighlight);
    }

    /// The breadcrumb on the area's first row: `Flow ›` and the name of
    /// what is open.
    pub fn crumb(&mut self, name: &str) {
        let x = self.put(1, 0, &format!("{} ›", words::FLOW), Role::Dim);
        let room = usize::from(self.area.width)
            .saturating_sub(x)
            .saturating_sub(2);
        self.put_bold(x.saturating_add(1), 0, &fit(name, room), Role::Heading);
    }
}
