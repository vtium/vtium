//! Reflow: a resize that changes the column count rewraps the main
//! screen's lines and its scrollback at the new width (ADR-1003).
//!
//! A logical line is a run of rows each marked wrapped but the last. A
//! rewrap joins each logical line's text and cuts it into rows of the new
//! width, each marked wrapped but the last, which keeps what ended it and
//! the line's fill. A wrapped row's cells are all text, but a pad in its
//! last column; the last row's text runs to its fill (`Line::trim_to`). A
//! wide character that does not fit in the columns left moves whole to the
//! next row and leaves a pad; on a screen one column wide it becomes one
//! empty cell, as printing makes it there. A row written while mode 2028
//! was reset, or of double size, joins nothing, is joined into nothing and
//! is cut at a narrower margin.
//!
//! The logical line's first row keeps its `LineId`, and each other row a
//! rewrap makes takes the next id. The shell-integration marks of its rows
//! go to its first row: a mark records no column, so no finer place is
//! known, and a line rewrapped through several widths keeps its marks
//! where rewrapping it once puts them. Anything anchored to a row (the cursor,
//! the saved cursor, the selection, the view) is carried as a [`Place`]:
//! its cell's offset in the logical line, which returns as the row and
//! column now holding that cell.
//!
//! The screen, the main screen's rows below it, the rows the view and the
//! selection reach, and the newest scrollback rows that could reach the
//! screen rewrap at once; the rest of the scrollback waits, counted, until
//! something reaches it (`scrollback.rs`). After the rewrap the screen's
//! rows below the cursor keep their number, less the rows the screen's text
//! up to the cursor now takes beyond what it took before, as Ghostty's
//! `PageList.resizeCols` keeps them (MIT): blank rows are removed from the
//! bottom or added there, and rows of text that no longer fit below the
//! screen stay below it.

use core::mem;

use super::Screen;
use super::selection::Anchor;
use crate::cell::{Cell, Width};
use crate::line::{IndexMap, Line, LineFlags, LineId, LineSize};
use crate::style::Style;

/// A cell a rewrap carries: its row among the rows rewrapped and its
/// column, and on return the row and column now holding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) row: usize,
    pub(crate) col: usize,
    /// It stands after its cell, as a pending wrap does.
    pub(crate) after: bool,
}

impl Place {
    fn at(row: usize, col: usize) -> Self {
        Self {
            row,
            col,
            after: false,
        }
    }

    /// The cursor at column `x` of `row` of a screen `cols` wide: a pending
    /// wrap stands after the last column's cell.
    fn cursor(x: usize, row: usize, cols: usize) -> Self {
        Self {
            row,
            col: x.min(cols.saturating_sub(1)),
            after: x >= cols,
        }
    }

    /// The cursor's column on a screen `cols` wide: after its cell is the
    /// next column, which is the pending wrap when the cell ends the row.
    fn cursor_x(self, cols: usize) -> usize {
        if self.after {
            self.col.saturating_add(1).min(cols)
        } else {
            self.col
        }
    }
}

/// Whether `line` rewraps: written while mode 2028 was set, at single size.
pub(crate) fn rewraps(line: &Line) -> bool {
    !line.flags().contains(LineFlags::FIXED) && line.flags().size() == LineSize::Single
}

/// Whether the text of `line` goes on in `next`.
pub(crate) fn joins(line: &Line, next: &Line) -> bool {
    line.flags().contains(LineFlags::WRAPPED) && rewraps(line) && rewraps(next)
}

/// The cells of `line`, trimmed, that are text in its logical line: all of
/// them but a pad that ends a wrapped line.
fn text(line: &Line) -> &[Cell] {
    let cells = line.cells();
    match cells.split_last() {
        Some((last, rest))
            if last.width() == Width::Pad && line.flags().contains(LineFlags::WRAPPED) =>
        {
            rest
        }
        _ => cells,
    }
}

/// The cells the cell at `i` of `text` moves with: a wide character and its
/// tail are one unit of two.
fn span(text: &[Cell], i: usize) -> usize {
    let wide = text.get(i).is_some_and(|c| c.width() == Width::Wide)
        && text
            .get(i.saturating_add(1))
            .is_some_and(|c| c.width() == Width::WideTail);
    if wide { 2 } else { 1 }
}

/// The columns a unit of `span` cells takes at `width`: one on a screen
/// one column wide.
fn columns(span: usize, width: usize) -> usize {
    if width < 2 { 1 } else { span }
}

/// The rows the logical line `rows`, trimmed, takes at `width`. A line
/// whose text fits is counted from its length; only a longer one is walked
/// cell by cell, since a wide character may move.
pub(crate) fn rows_at<'a, I>(rows: I, width: usize) -> usize
where
    I: Iterator<Item = &'a Line> + Clone,
{
    let width = width.max(1);
    let len = rows
        .clone()
        .map(|l| text(l).len())
        .fold(0usize, usize::saturating_add);
    if len <= width || rows.clone().any(|l| !rewraps(l)) {
        return 1;
    }
    let (mut count, mut col) = (1usize, 0usize);
    for line in rows {
        let text = text(line);
        let mut i = 0;
        while i < text.len() {
            let span = span(text, i);
            let need = columns(span, width);
            if col.saturating_add(need) > width {
                count = count.saturating_add(1);
                col = 0;
            }
            col = col.saturating_add(need);
            i = i.saturating_add(span);
        }
    }
    count
}

/// Rewraps one logical line, `rows`, trimmed, oldest first, at `width`,
/// appending its rows to `out`. Each row is consumed as its cells move, so
/// the rewrap holds a row or two beyond its result. The first row keeps
/// its id and each other row takes `next_id`, which advances. `places`,
/// rows of the logical line on entry, return as rows of what was appended.
/// Returns how many rows were appended.
pub(crate) fn rewrap_line(
    rows: impl IntoIterator<Item = Line>,
    width: usize,
    next_id: &mut u64,
    places: &mut [Place],
    out: &mut Vec<Line>,
) -> usize {
    let width = width.max(1);
    let last_col = width.saturating_sub(1);
    let start = out.len();
    let mut rows = rows.into_iter().peekable();
    let Some(mut src) = rows.next() else {
        return 0;
    };
    if rows.peek().is_none() {
        // A line that does not rewrap is cut; one that fits stays.
        let fits = !src.flags().contains(LineFlags::WRAPPED) && src.len() <= width;
        if fits || !rewraps(&src) {
            for p in places.iter_mut() {
                p.row = 0;
                p.col = p.col.min(last_col);
            }
            if src.len() > width {
                src.resize(width);
            }
            out.push(src);
            return 1;
        }
    }

    let mut landed: Vec<Option<(usize, usize)>> = vec![None; places.len()];
    let mut map = IndexMap::default();
    let mut cur = Line::with_capacity(src.id(), LineFlags::empty(), width);
    // Places on a pad that a row's end drops land on the next cell.
    let mut on_pad: Vec<usize> = Vec::new();
    let mut k = 0usize;
    loop {
        map.clear();
        let text = text(&src);
        let marks = src.flags().intersection(LineFlags::MARKS);
        match out.get_mut(start) {
            Some(first) => first.flags_mut().insert(marks),
            None => cur.flags_mut().insert(marks),
        }
        let mut i = 0;
        while let Some(&cell) = text.get(i) {
            let span = span(text, i);
            let need = columns(span, width);
            if cur.len().saturating_add(need) > width {
                if need == 2 && cur.len().saturating_add(1) == width {
                    cur.push_pad(&src, cell);
                }
                cur.flags_mut().insert(LineFlags::WRAPPED);
                cur.shrink();
                let id = LineId::new(*next_id);
                *next_id = next_id.wrapping_add(1);
                let next = Line::with_capacity(id, LineFlags::empty(), width);
                out.push(mem::replace(&mut cur, next));
                map.clear();
            }
            let (row, col) = (out.len().saturating_sub(start), cur.len());
            for n in on_pad.drain(..) {
                if let Some(slot) = landed.get_mut(n) {
                    *slot = Some((row, col));
                }
            }
            for (p, slot) in places.iter().zip(landed.iter_mut()) {
                if p.row == k && slot.is_none() && (i..i.saturating_add(span)).contains(&p.col) {
                    let d = p.col.saturating_sub(i).min(need.saturating_sub(1));
                    *slot = Some((row, col.saturating_add(d)));
                }
            }
            if need < span || cell.width() == Width::Pad {
                // A wide character on a screen one column wide, or a pad
                // inside the text, is one empty cell.
                cur.push_from(&src, cell, Width::Narrow, true, &mut map);
            } else {
                for &c in text.iter().skip(i).take(span) {
                    cur.push_from(&src, c, c.width(), false, &mut map);
                }
            }
            i = i.saturating_add(span);
        }
        let Some(next) = rows.next() else {
            break;
        };
        for (n, (p, slot)) in places.iter().zip(landed.iter()).enumerate() {
            if p.row == k && slot.is_none() {
                on_pad.push(n);
            }
        }
        src = next;
        k = k.saturating_add(1);
    }
    // Past the last row's text: as far past it as before, inside the row.
    let row = out.len().saturating_sub(start);
    let len = text(&src).len();
    for (n, (p, slot)) in places.iter().zip(landed.iter_mut()).enumerate() {
        if slot.is_none() {
            let col = if on_pad.contains(&n) {
                cur.len()
            } else {
                cur.len().saturating_add(p.col.saturating_sub(len))
            };
            *slot = Some((row, col.min(last_col)));
        }
    }
    for (p, slot) in places.iter_mut().zip(landed) {
        if let Some((row, col)) = slot {
            p.row = row;
            p.col = col;
        }
    }
    if src.flags().contains(LineFlags::WRAPPED) {
        cur.flags_mut().insert(LineFlags::WRAPPED);
    }
    cur.set_fill_from(&src, &mut map);
    cur.shrink();
    out.push(cur);
    out.len().saturating_sub(start)
}

/// Rewraps `rows`, trimmed, oldest first, at `width`, each logical line
/// through [`rewrap_line`]. `places` are rows of `rows` on entry and of the
/// result on return.
pub(crate) fn rewrap_rows(
    rows: Vec<Line>,
    width: usize,
    next_id: &mut u64,
    places: &mut [Place],
) -> Vec<Line> {
    let mut out = Vec::with_capacity(rows.len());
    let mut moved = places.to_vec();
    let mut rows = rows.into_iter().enumerate().peekable();
    let mut group = Vec::new();
    let mut first = 0;
    while let Some((i, line)) = rows.next() {
        if group.is_empty() {
            first = i;
        }
        let ends = rows.peek().is_none_or(|(_, next)| !joins(&line, next));
        group.push(line);
        if !ends {
            continue;
        }
        let lines = first..i.saturating_add(1);
        let which: Vec<usize> = (0..places.len())
            .filter(|&n| places.get(n).is_some_and(|p| lines.contains(&p.row)))
            .collect();
        let mut mine: Vec<Place> = which
            .iter()
            .filter_map(|&n| places.get(n))
            .map(|p| Place {
                row: p.row.saturating_sub(first),
                ..*p
            })
            .collect();
        let base = out.len();
        rewrap_line(group.drain(..), width, next_id, &mut mine, &mut out);
        for (n, p) in which.into_iter().zip(mine) {
            if let Some(slot) = moved.get_mut(n) {
                *slot = Place {
                    row: p.row.saturating_add(base),
                    ..p
                };
            }
        }
    }
    places.copy_from_slice(&moved);
    out
}

/// Whether `line`, trimmed, is a blank row a rewrap may remove below the
/// cursor: no text, no fill and no flag.
fn blank(line: &Line) -> bool {
    line.is_empty() && line.fill() == Style::DEFAULT && line.flags() == LineFlags::empty()
}

/// What the main screen's rewrap carries besides its text, as places in
/// the rows rewrapped.
struct Carried {
    /// The cursor, or the saved cursor while the alternate screen is shown.
    anchor: Place,
    /// The first cell of the screen's first row.
    top: Place,
    saved: Option<Place>,
    view: Option<Place>,
    selection: Vec<(Anchor, Place)>,
}

impl Carried {
    fn places(&self) -> Vec<Place> {
        [self.anchor, self.top]
            .into_iter()
            .chain(self.saved)
            .chain(self.view)
            .chain(self.selection.iter().map(|&(_, p)| p))
            .collect()
    }

    fn update(&mut self, places: &[Place]) {
        let mut places = places.iter().copied();
        let mut next = |p: &mut Place| {
            if let Some(q) = places.next() {
                *p = q;
            }
        };
        next(&mut self.anchor);
        next(&mut self.top);
        if let Some(p) = self.saved.as_mut() {
            next(p);
        }
        if let Some(p) = self.view.as_mut() {
            next(p);
        }
        for (_, p) in &mut self.selection {
            next(p);
        }
    }
}

impl Screen {
    /// Rewraps the main screen and its scrollback at `cols`, and cuts the
    /// alternate screen's lines there, keeping the row count. The main
    /// screen rewraps around its cursor, or around the main screen's saved
    /// cursor, `saved`, while the alternate screen is shown; `saved` is
    /// carried with it.
    pub(super) fn rewrap(&mut self, cols: usize, saved: &mut (usize, usize)) {
        for line in &mut self.alt {
            line.resize(cols);
        }
        let old = self.cols;
        let rows = self.rows;
        let alternate = self.flags.alternate;
        let (ax, ay) = if alternate {
            *saved
        } else {
            (self.cursor_x, self.cursor_y)
        };
        let ay = ay.min(rows.saturating_sub(1));

        // The scrollback rows the view and the selection reach rewrap at
        // once with the screen and the rows that could reach it.
        let selection = self.selection.anchors();
        let view = self.scrollback.view_index();
        let reach = selection
            .iter()
            .filter_map(|a| self.scrollback.lines().position(|l| l.id() == a.line))
            .chain(view)
            .min();
        let (mut run, split) =
            self.scrollback
                .take_for_reflow(cols, rows, reach, &mut self.next_id);
        let sb = run.len();
        for (y, mut line) in mem::take(&mut self.main).into_iter().enumerate() {
            // The anchor's row runs at least to it, so its place in the
            // text survives a narrowing.
            let keep = match y == ay {
                true if ax >= old => old,
                true => ax.saturating_add(1),
                false => 0,
            };
            line.trim_to(keep);
            run.push(line);
        }

        let mut carried = Carried {
            anchor: Place::cursor(ax, sb.saturating_add(ay), old),
            top: Place::at(sb, 0),
            saved: (!alternate && saved.1 < rows)
                .then(|| Place::cursor(saved.0, sb.saturating_add(saved.1), old)),
            view: view
                .and_then(|v| v.checked_sub(split))
                .filter(|&row| row < sb)
                .map(|row| Place::at(row, 0)),
            selection: selection
                .iter()
                .filter_map(|a| {
                    let row = run.iter().position(|l| l.id() == a.line)?;
                    Some((*a, Place::at(row, a.x)))
                })
                .collect(),
        };
        let mut places = carried.places();
        let mut out = rewrap_rows(run, cols, &mut self.next_id, &mut places);
        carried.update(&places);

        // Ghostty's rule: the rows below the cursor keep their number, less
        // the rows the text up to it now takes beyond what it took.
        let rc = carried.anchor.row;
        let took = ay.saturating_add(1);
        let takes = rc.saturating_sub(carried.top.row).saturating_add(1);
        let below = rows
            .saturating_sub(took)
            .saturating_sub(takes.saturating_sub(took));
        let mut gone = Vec::new();
        while out.len().saturating_sub(rc).saturating_sub(1) > below
            && out.last().is_some_and(blank)
        {
            gone.extend(out.pop());
        }
        let kept = out.len();
        let top = rc.saturating_sub(rows.saturating_sub(1).saturating_sub(below));

        let mut main = out.split_off(top.min(kept));
        let base = self.scrollback.len();
        gone.extend(self.scrollback.extend(out));
        for line in &mut main {
            line.paint(cols);
        }
        while main.len() < rows {
            let line = self.new_line(cols, &Style::DEFAULT);
            main.push(line);
        }
        self.main = main;
        self.cols = cols;
        self.land(&carried, Landing { top, kept, base }, saved);
        self.scrollback.enforce_max();
        self.discard_lines(gone);
    }

    /// Puts what the rewrap carried where its cells landed: rows from `top`
    /// on the screen, rows above it in the scrollback from `base`, and rows
    /// from `kept` removed.
    fn land(&mut self, carried: &Carried, landing: Landing, saved: &mut (usize, usize)) {
        let cols = self.cols;
        let last_row = self.rows.saturating_sub(1);
        let Landing { top, kept, base } = landing;
        let anchor = carried.anchor;
        let y = anchor.row.saturating_sub(top).min(last_row);
        if self.flags.alternate {
            *saved = (anchor.cursor_x(cols), y);
            self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));
        } else {
            self.cursor_x = anchor.cursor_x(cols);
            self.cursor_y = y;
            if let Some(p) = carried.saved {
                // Saved on text that left the screen, it keeps its column
                // on the first row.
                let y = p.row.saturating_sub(top).min(last_row);
                *saved = (p.cursor_x(cols), y);
            }
        }
        let line = |row: usize| -> Option<LineId> {
            if row >= kept {
                return None;
            }
            match row.checked_sub(top) {
                Some(y) => self.main.get(y).map(Line::id),
                None => self.scrollback.get(base.saturating_add(row)).map(Line::id),
            }
        };
        let moved: Vec<(Anchor, Option<Anchor>)> = carried
            .selection
            .iter()
            .map(|&(a, p)| {
                let to = line(p.row).map(|line| Anchor { line, x: p.col });
                (a, to)
            })
            .collect();
        self.selection.carry(&moved);
        let view = carried
            .view
            .filter(|p| p.row < top)
            .map(|p| base.saturating_add(p.row));
        self.scrollback.set_view(view);
    }
}

/// Where a rewrap's rows went: see [`Screen::land`].
#[derive(Clone, Copy)]
struct Landing {
    top: usize,
    kept: usize,
    base: usize,
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::line::Glyph;

    /// A trimmed row of `text`, `W` a wide character, marked wrapped when
    /// `wrapped`, as printing at `cols` leaves it.
    fn row(id: u64, text: &str, wrapped: bool) -> Line {
        let cols = text.chars().map(|c| if c == 'W' { 2 } else { 1 }).sum();
        let mut line = Line::new(LineId::new(id), cols);
        let mut x = 0;
        for c in text.chars() {
            if c == 'W' {
                line.write(x, Glyph::Char('中'), Width::Wide, &Style::DEFAULT, None);
                line.write(x + 1, Glyph::Empty, Width::WideTail, &Style::DEFAULT, None);
                x += 2;
            } else if c == '_' {
                line.write(x, Glyph::Empty, Width::Pad, &Style::DEFAULT, None);
                x += 1;
            } else {
                line.write(x, Glyph::Char(c), Width::Narrow, &Style::DEFAULT, None);
                x += 1;
            }
        }
        if wrapped {
            line.flags_mut().insert(LineFlags::WRAPPED);
        }
        line.trim();
        line
    }

    fn text_of(line: &Line) -> String {
        line.cells()
            .iter()
            .map(|&c| match (line.glyph(c), c.width()) {
                (_, Width::WideTail) => String::new(),
                (_, Width::Pad) => "_".into(),
                (Glyph::Char('中'), _) => "W".into(),
                (Glyph::Char(c), _) => c.into(),
                _ => " ".into(),
            })
            .collect()
    }

    fn rewrap(rows: Vec<Line>, width: usize, places: &mut [Place]) -> Vec<Line> {
        let mut next = 100;
        let mut out = Vec::new();
        rewrap_line(rows, width, &mut next, places, &mut out);
        out
    }

    // The count a resize settles the waiting scrollback with is the count
    // the rewrap makes, wide characters and pads included.
    #[test]
    fn the_count_agrees_with_the_rewrap() {
        let mut seed = 0x1003_u64;
        for _ in 0..3000 {
            let mut rows = Vec::new();
            let n = 1 + (seed % 4) as usize;
            for k in 0..n {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let len = (seed >> 33) as usize % 9;
                let text: String = (0..len)
                    .map(|i| if (seed >> (i + 3)) & 3 == 0 { 'W' } else { 'a' })
                    .collect();
                rows.push(row(k as u64, &text, k + 1 < n));
            }
            for width in 1..12 {
                let counted = rows_at(rows.iter(), width);
                let made = rewrap(rows.clone(), width, &mut []);
                assert_eq!(counted, made.len(), "{:?} at {width}", rows.iter().map(text_of).collect::<Vec<_>>());
                assert!(made.iter().all(|l| l.len() <= width));
            }
        }
    }

    #[test]
    fn a_wide_character_that_does_not_fit_leaves_a_pad() {
        let rows = vec![row(1, "abc", true), row(2, "Wd", false)];
        let out = rewrap(rows, 4, &mut []);
        let texts: Vec<String> = out.iter().map(text_of).collect();
        assert_eq!(texts, ["abc_", "Wd"]);
        assert!(out[0].flags().contains(LineFlags::WRAPPED));
        assert!(!out[1].flags().contains(LineFlags::WRAPPED));
        assert_eq!((out[0].id(), out[1].id()), (LineId::new(1), LineId::new(100)));
        // Rejoined, the pad goes.
        let out = rewrap(out, 8, &mut []);
        assert_eq!(out.iter().map(text_of).collect::<Vec<_>>(), ["abcWd"]);
    }

    // A place on a pad goes to the cell after it; one past the text stays
    // as far past it, inside the row.
    #[test]
    fn places_on_a_pad_or_past_the_text_land_beside_the_text() {
        let rows = vec![row(1, "abc_", true), row(2, "Wd", false)];
        let mut places = [
            Place::at(0, 3),
            Place::at(1, 1),
            Place::at(1, 5),
            Place::cursor(2, 1, 4),
        ];
        let out = rewrap(rows, 8, &mut places);
        assert_eq!(out.len(), 1);
        assert_eq!(places[0], Place::at(0, 3));
        assert_eq!(places[1], Place::at(0, 4));
        assert_eq!(places[2], Place::at(0, 7));
        assert_eq!((places[3].row, places[3].col, places[3].after), (0, 5, false));
    }

    // A row of double size, or one written with mode 2028 reset, joins
    // nothing and is cut; a mark goes to its logical line's first row.
    #[test]
    fn fixed_rows_are_cut_and_marks_go_to_the_first_row() {
        let mut wide = row(1, "abcdef", true);
        wide.flags_mut().set_size(LineSize::DoubleWidth);
        let mut fixed = row(2, "ghij", true);
        fixed.flags_mut().insert(LineFlags::FIXED);
        let mut marked = row(4, "mn", false);
        marked.flags_mut().insert(LineFlags::OUTPUT);
        let rows = vec![wide, fixed, row(3, "kl", true), marked];
        let mut next = 100;
        let mut places = [];
        let out = rewrap_rows(rows, 3, &mut next, &mut places);
        let texts: Vec<String> = out.iter().map(text_of).collect();
        assert_eq!(texts, ["abc", "ghi", "klm", "n"]);
        assert!(out[2].flags().contains(LineFlags::OUTPUT));
        assert!(!out[3].flags().contains(LineFlags::OUTPUT));
        assert!(!out[0].flags().contains(LineFlags::WRAPPED));
    }
}
