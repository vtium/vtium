//! The order vtium draws right-to-left text in, in a build with the bidi
//! feature (ADR-1004 D3 to D8).
//!
//! The screen keeps every row in logical order, the order the program
//! wrote it. When the screen builds a frame, it hands each paragraph (the
//! rows a wrap joins) that can draw in another order to [`paragraph`],
//! which gives each of the paragraph's rows a [`RowOrder`]: the logical
//! column drawn at each visual column, the visual column each logical
//! column is drawn at, whether each cell resolves right to left, and which
//! sides of each cell join its neighbours. The frame reads cells through
//! it, so every renderer draws the same cells in the same places. A row
//! with no [`RowOrder`] draws in logical order, which is the order of every
//! row of a left-to-right paragraph that holds no right-to-left or joining
//! character.
//!
//! In implicit mode, BDSM's default, the bidirectional algorithm resolves
//! the whole paragraph, one character a cell, and each row is reordered on
//! its own, so text never moves from one row to another. Each cell gives
//! the algorithm the first codepoint of its cluster; a wide character's
//! second cell moves with it. An erased cell inside a row reads as a
//! space, and the erased cells after a row's last written cell take no part
//! in the algorithm and are drawn at the row's end side: at the right in a
//! left-to-right paragraph, at the left in a right-to-left one. In explicit
//! mode, and for a paragraph of more than [`BIDI_PARAGRAPH_ROWS_MAX`](crate::bounds::BIDI_PARAGRAPH_ROWS_MAX) rows,
//! no algorithm runs: a left-to-right row draws as written, and a
//! right-to-left row draws reversed, every character resolving right to
//! left.
//!
//! Arabic and the other joining scripts join by their neighbours in the
//! order they are read, before any reordering: logical order across the
//! whole paragraph in implicit mode, so a word joins across a wrap, and the
//! order drawn from right to left on each row in explicit mode, since the
//! program has reordered the row itself (ADR-1004 D6).

use core::ops::Range;

use crate::cell::Width;
use crate::line::{Glyph, Line, LineFlags};
use crate::unicode::bidi::{self, Class, Direction, levels};

/// Which sides of a cell join its neighbours, in the order its script is
/// read: `before` is the letter read just before it, which an Arabic word
/// draws at its right, and `after` the letter read just after it (ADR-1004
/// D6). A renderer that asks for a letter's joined form keys it with U+200D
/// on each side it joins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Joins {
    /// It joins the letter read before it.
    pub before: bool,
    /// It joins the letter read after it.
    pub after: bool,
}

/// How each row of a frame is drawn: in logical order, or in the order its
/// paragraph's [`RowOrder`] gives. The screen builds it with each frame, and
/// a holder of a frame's parts, such as a snapshot a render thread reads,
/// hands it back with [`Frame::with_order`](crate::Frame::with_order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Order {
    /// One entry for each view row from the top; a row past the list, and a
    /// `None`, draws in logical order.
    rows: Vec<Option<RowOrder>>,
}

impl Order {
    pub(crate) fn new(rows: Vec<Option<RowOrder>>) -> Self {
        Self { rows }
    }

    /// The order of view row `row`, or `None` for a row drawn in logical
    /// order.
    pub(crate) fn row(&self, row: usize) -> Option<&RowOrder> {
        self.rows.get(row).and_then(Option::as_ref)
    }

    /// The rows from the top that have an order.
    pub(crate) fn rows(&self) -> impl Iterator<Item = (usize, &RowOrder)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(y, r)| r.as_ref().map(|r| (y, r)))
    }
}

/// A cell resolves right to left: it is drawn mirrored where its character
/// has a mirrored glyph, and a bar cursor stands at its right side.
const RTL: u8 = 1 << 0;
/// A cell joins the letter read before it.
const JOINS_BEFORE: u8 = 1 << 1;
/// A cell joins the letter read after it.
const JOINS_AFTER: u8 = 1 << 2;

/// The drawing order of one row of a paragraph that can draw in another
/// order than it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowOrder {
    /// The logical column drawn at each visual column, from the left.
    visual: Vec<u16>,
    /// The visual column each logical column is drawn at.
    logical: Vec<u16>,
    /// What each logical column resolves to: [`RTL`], [`JOINS_BEFORE`] and
    /// [`JOINS_AFTER`].
    marks: Vec<u8>,
    /// The paragraph runs right to left: its erased cells sit at the left,
    /// and a picture anchored on it is mirrored.
    pub(crate) rtl: bool,
    /// Mode 2500 is set on the paragraph: in a right-to-left paragraph a
    /// box-drawing character draws its left-right counterpart.
    box_mirror: bool,
    /// The view rows of the paragraph, which change together; the screen
    /// sets it once it knows where the paragraph sits in the view.
    pub(crate) paragraph: Range<usize>,
}

impl RowOrder {
    /// The logical column drawn at visual column `col`; a column past the
    /// row is its own.
    pub(crate) fn logical_at(&self, col: usize) -> usize {
        self.visual.get(col).map_or(col, |&c| usize::from(c))
    }

    /// The visual column logical column `col` is drawn at; a column past
    /// the row is its own.
    pub(crate) fn visual_at(&self, col: usize) -> usize {
        self.logical.get(col).map_or(col, |&c| usize::from(c))
    }

    /// Whether logical column `col` resolves right to left.
    pub(crate) fn rtl_at(&self, col: usize) -> bool {
        self.marks.get(col).is_some_and(|m| m & RTL != 0)
    }

    /// The joining sides of logical column `col`.
    pub(crate) fn joins_at(&self, col: usize) -> Joins {
        let m = self.marks.get(col).copied().unwrap_or(0);
        Joins {
            before: m & JOINS_BEFORE != 0,
            after: m & JOINS_AFTER != 0,
        }
    }

    /// The character logical column `col` draws in place of the first
    /// character of `glyph`, the cell the program wrote: by rule L4 a
    /// character with a mirrored glyph draws it when the cell resolves
    /// right to left, and under mode 2500 a box-drawing character in a
    /// right-to-left paragraph draws its left-right counterpart (ADR-1004
    /// D7). A character's marks stay with it, drawn on the counterpart.
    /// `None` when the cell draws as written.
    pub(crate) fn mirror(&self, col: usize, glyph: Glyph<'_>) -> Option<char> {
        let c = match glyph {
            Glyph::Empty => return None,
            Glyph::Char(c) => c,
            Glyph::Cluster(s) => s.chars().next()?,
        };
        let mirrored = self.rtl_at(col).then(|| bidi::mirrored(c)).flatten();
        let boxed = (self.box_mirror && self.rtl)
            .then(|| bidi::box_mirrored(c))
            .flatten();
        mirrored.or(boxed)
    }

    /// The glyph logical column `col` draws for `glyph`: a single character
    /// drawn mirrored is its counterpart, as [`mirror`](Self::mirror) says.
    /// A cluster keeps its own text, and the frame carries its mirrored
    /// first character beside it.
    pub(crate) fn drawn<'a>(&self, col: usize, glyph: Glyph<'a>) -> Glyph<'a> {
        match (glyph, self.mirror(col, glyph)) {
            (Glyph::Char(_), Some(m)) => Glyph::Char(m),
            _ => glyph,
        }
    }
}

/// One character the algorithm reads: a cell, or a wide character's two.
#[derive(Clone, Copy)]
struct Unit {
    col: usize,
    cells: usize,
    c: char,
}

/// The characters of `line` up to its last written cell, read `cols` wide,
/// and the column after that cell, where its trailing erased cells start.
fn units(line: &Line, cols: usize) -> (Vec<Unit>, usize) {
    let len = line.len().min(cols);
    let mut end = (0..len)
        .rev()
        .find(|&col| line.glyph(line.cell(col)) != Glyph::Empty)
        .map_or(0, |col| col.saturating_add(1));
    // a wide character's second cell holds nothing, and goes with it
    if end > 0
        && end < len
        && line.cell(end.saturating_sub(1)).width() == Width::Wide
        && line.cell(end).width() == Width::WideTail
    {
        end = end.saturating_add(1);
    }
    let mut out: Vec<Unit> = Vec::with_capacity(end);
    for col in 0..end {
        let cell = line.cell(col);
        if cell.width() == Width::WideTail
            && let Some(last) = out.last_mut()
            && last.cells == 1
            && last.col.saturating_add(1) == col
            && line.cell(last.col).width() == Width::Wide
        {
            last.cells = 2;
            continue;
        }
        let c = match line.glyph(cell) {
            Glyph::Empty => ' ',
            Glyph::Char(c) => c,
            Glyph::Cluster(s) => s.chars().next().unwrap_or(' '),
        };
        out.push(Unit { col, cells: 1, c });
    }
    (out, end)
}

/// One row's units and where its trailing erased cells start.
struct RowText {
    units: Vec<Unit>,
    end: usize,
}

/// The drawing order of the rows of one paragraph that the caller draws,
/// `wanted`, each with its index in `lines`: `lines` is the whole
/// paragraph in reading order, each row read `cols` wide. The paragraph's
/// properties are those of its first row (ADR-1004 D2). `too_long` draws
/// it as in explicit mode, as a paragraph of more than
/// [`BIDI_PARAGRAPH_ROWS_MAX`](crate::bounds::BIDI_PARAGRAPH_ROWS_MAX) rows
/// is. The algorithm reads the whole paragraph, and the rows outside
/// `wanted` cost it nothing more. The rows' `paragraph` ranges are left
/// empty for the caller.
pub(crate) fn paragraph(
    lines: &[&Line],
    cols: usize,
    too_long: bool,
    wanted: Range<usize>,
) -> Vec<(usize, RowOrder)> {
    let flags = lines.first().map_or(LineFlags::empty(), |l| l.flags());
    let text = |l: &&Line| {
        let (units, end) = units(l, cols);
        RowText { units, end }
    };
    let box_mirror = flags.contains(LineFlags::BOX_MIRROR);
    let explicit = flags.contains(LineFlags::EXPLICIT) || too_long;
    if explicit {
        let rtl = flags.contains(LineFlags::RTL);
        return wanted
            .filter_map(|i| lines.get(i).map(|l| (i, text(l))))
            .map(|(i, row)| (i, explicit_row(&row, cols, rtl, box_mirror)))
            .collect();
    }
    let rows: Vec<RowText> = lines.iter().map(text).collect();
    implicit(&rows, cols, flags, box_mirror, wanted)
}

/// A row in explicit mode: drawn as written in a left-to-right paragraph
/// and reversed in a right-to-left one, its letters joined in the order
/// they are drawn from right to left (ADR-1004 D4, D6).
fn explicit_row(row: &RowText, cols: usize, rtl: bool, box_mirror: bool) -> RowOrder {
    let mut order: Vec<usize> = (0..row.units.len()).collect();
    if rtl {
        order.reverse();
    }
    let read: Vec<char> = order
        .iter()
        .rev()
        .filter_map(|&u| row.units.get(u).map(|u| u.c))
        .collect();
    let joins = bidi::joins(&read);
    let mut marks = vec![if rtl { RTL } else { 0 }; cols];
    // the read order runs from the rightmost unit leftwards
    for (j, &u) in joins.iter().zip(order.iter().rev()) {
        if let Some(unit) = row.units.get(u) {
            mark_joins(&mut marks, unit.col, *j);
        }
    }
    build(row, &order, cols, rtl, box_mirror, marks)
}

/// The rows of a paragraph in implicit mode: the bidirectional algorithm
/// over the paragraph's characters, each row reordered on its own, and
/// the letters joined in logical order across the whole paragraph
/// (ADR-1004 D3, D6).
fn implicit(
    rows: &[RowText],
    cols: usize,
    flags: LineFlags,
    box_mirror: bool,
    wanted: Range<usize>,
) -> Vec<(usize, RowOrder)> {
    let text: Vec<char> = rows
        .iter()
        .flat_map(|r| r.units.iter().map(|u| u.c))
        .collect();
    let lens: Vec<usize> = rows.iter().map(|r| r.units.len()).collect();
    let mut direction = if flags.contains(LineFlags::RTL) {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    };
    let detect = flags.contains(LineFlags::DETECT);
    if detect {
        direction = bidi::first_strong(&text).unwrap_or(direction);
    }
    let moves = text
        .iter()
        .any(|&c| matches!(bidi::class(c), Class::R | Class::Al | Class::An));
    // A left-to-right paragraph with no right-to-left character or
    // Arabic digit resolves every character to level 0 (rules W7 and N1),
    // so the crate is not called for it.
    let (rtl, levels) = if direction == Direction::LeftToRight && !moves {
        (false, vec![0; text.len()])
    } else {
        let resolved = levels::resolve(&text, direction, false, &lens, wanted.clone());
        (resolved.level % 2 == 1, resolved.levels)
    };
    let joins = bidi::joins(&text);
    let mut at: usize = 0;
    rows.iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let n = row.units.len();
            let span = at..at.saturating_add(n);
            at = span.end;
            if !wanted.contains(&i) {
                return None;
            }
            let row_levels = levels.get(span.clone()).unwrap_or_default();
            let order = levels::visual_order(row_levels);
            let mut marks = vec![if rtl { RTL } else { 0 }; cols];
            for (k, unit) in row.units.iter().enumerate() {
                let odd = row_levels.get(k).is_some_and(|l| l % 2 == 1);
                for col in unit.col..unit.col.saturating_add(unit.cells) {
                    if let Some(m) = marks.get_mut(col) {
                        *m = if odd { RTL } else { 0 };
                    }
                }
                let j = joins
                    .get(span.start.saturating_add(k))
                    .copied()
                    .unwrap_or_default();
                mark_joins(&mut marks, unit.col, j);
            }
            Some((i, build(row, &order, cols, rtl, box_mirror, marks)))
        })
        .collect()
}

fn mark_joins(marks: &mut [u8], col: usize, j: bidi::Joins) {
    if let Some(m) = marks.get_mut(col) {
        if j.before {
            *m |= JOINS_BEFORE;
        }
        if j.after {
            *m |= JOINS_AFTER;
        }
    }
}

/// A row's order from `order`, the row's units from the left: each unit's
/// cells keep their own order, so a wide character's second cell follows
/// its first, and the trailing erased cells go to the paragraph's end side,
/// reversed in a right-to-left paragraph.
fn build(
    row: &RowText,
    order: &[usize],
    cols: usize,
    rtl: bool,
    box_mirror: bool,
    marks: Vec<u8>,
) -> RowOrder {
    #[cfg(test)]
    BUILT.with(|n| n.set(n.get().saturating_add(1)));
    let trailing = row.end..cols.max(row.end);
    let mut visual: Vec<usize> = Vec::with_capacity(cols);
    if rtl {
        visual.extend(trailing.clone().rev());
    }
    for unit in order.iter().filter_map(|&u| row.units.get(u)) {
        visual.extend(unit.col..unit.col.saturating_add(unit.cells));
    }
    if !rtl {
        visual.extend(trailing);
    }
    visual.truncate(cols);
    let to16 = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
    let mut logical = vec![0u16; cols];
    for (v, &l) in visual.iter().enumerate() {
        if let Some(slot) = logical.get_mut(l) {
            *slot = to16(v);
        }
    }
    RowOrder {
        visual: visual.into_iter().map(to16).collect(),
        logical,
        marks,
        rtl,
        box_mirror,
        paragraph: 0..0,
    }
}

#[cfg(test)]
std::thread_local! {
    /// The rows [`build`] has ordered on this thread, which a test counts
    /// to check that a frame orders only the rows it draws.
    static BUILT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// The rows [`build`] has ordered on this thread so far.
#[cfg(test)]
pub(crate) fn rows_built() -> usize {
    BUILT.with(core::cell::Cell::get)
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::cell::Width;
    use crate::line::LineId;
    use crate::style::Style;

    fn line(text: &str, cols: usize, flags: LineFlags) -> Line {
        let mut l = Line::new(LineId::new(0), cols);
        let mut col = 0;
        for c in text.chars() {
            let wide = crate::unicode::cluster_width(&[c]) == 2;
            l.write(
                col,
                Glyph::Char(c),
                if wide { Width::Wide } else { Width::Narrow },
                &Style::DEFAULT,
                None,
            );
            if wide {
                l.write(
                    col + 1,
                    Glyph::Empty,
                    Width::WideTail,
                    &Style::DEFAULT,
                    None,
                );
                col += 1;
            }
            col += 1;
        }
        l.flags_mut().insert(flags);
        l
    }

    /// The row as drawn, a character a visual column, mirrored glyphs
    /// drawn, erased cells as `.`.
    fn drawn(l: &Line, r: &RowOrder, cols: usize) -> String {
        (0..cols)
            .map(|v| {
                let col = r.logical_at(v);
                match r.drawn(col, l.glyph(l.cell(col))) {
                    Glyph::Char(c) => c,
                    Glyph::Empty if l.cell(col).width() == Width::WideTail => '_',
                    _ => '.',
                }
            })
            .collect()
    }

    fn one(text: &str, cols: usize, flags: LineFlags) -> String {
        let l = line(text, cols, flags);
        let r: Vec<RowOrder> = paragraph(&[&l], cols, false, 0..1)
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        drawn(&l, &r[0], cols)
    }

    #[test]
    fn a_left_to_right_paragraph_reverses_its_hebrew_and_mirrors_its_brackets() {
        // ADR-1004's figure: abc, shalom, (1), def
        assert_eq!(
            one("abc שלום (1) def", 20, LineFlags::empty()),
            "abc (1) םולש def...."
        );
    }

    #[test]
    fn a_right_to_left_paragraph_starts_at_the_right_edge() {
        assert_eq!(
            one("abc שלום (1) def", 20, LineFlags::RTL),
            "....def (1) םולש abc"
        );
    }

    #[test]
    fn explicit_mode_draws_as_written_or_reversed_and_mirrors_every_character() {
        assert_eq!(one("abc (1)", 10, LineFlags::EXPLICIT), "abc (1)...");
        assert_eq!(
            one("abc (1)", 10, LineFlags::EXPLICIT | LineFlags::RTL),
            "...(1) cba"
        );
    }

    #[test]
    fn a_wide_characters_second_cell_follows_it() {
        // the emoji, a neutral, resolves right to left between Hebrew
        // letters; the CJK character is left to right and stays in place
        assert_eq!(one("א😀ב", 6, LineFlags::empty()), "ב😀_א..");
        assert_eq!(one("א世ב", 6, LineFlags::empty()), "א世_ב..");
        assert_eq!(one("a世", 5, LineFlags::RTL), "..a世_");
    }

    #[test]
    fn the_maps_are_inverse() {
        let l = line("abc שלום (1) def", 20, LineFlags::RTL);
        let r = &paragraph(&[&l], 20, false, 0..1)[0].1;
        for v in 0..20 {
            assert_eq!(r.visual_at(r.logical_at(v)), v);
        }
    }

    #[test]
    fn box_drawing_mirrors_under_2500_in_a_right_to_left_paragraph_only() {
        let rtl = LineFlags::RTL | LineFlags::BOX_MIRROR;
        assert_eq!(one("┌─┐", 3, rtl), "┌─┐");
        assert_eq!(one("┌─┐", 3, LineFlags::RTL), "┐─┌");
        assert_eq!(one("┌─┐", 3, LineFlags::BOX_MIRROR), "┌─┐");
    }
}
