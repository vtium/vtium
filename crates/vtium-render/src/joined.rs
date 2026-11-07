//! The text a cell of right-to-left text draws (ADR-1004 D6, D7): what the
//! renderer looks its glyph up by, and what the console asks the glyph
//! helper for, so that the two always name the same glyph.
//!
//! A letter of a joining script, such as Arabic, draws the form its joining
//! sides give it. The frame carries those sides ([`FrameCell::joins`]), and
//! the text keys the letter with U+200D, ZERO WIDTH JOINER, on each side it
//! joins: U+200D before the letter when it joins the letter read before it,
//! and after the letter when it joins the letter read after it. This is the
//! key the glyph helper shapes a joined form under (ADR-1301, the listable
//! clusters).
//!
//! Lam followed by alef joins into one ligature two cells wide, which Arabic
//! writing requires. Where the two letters stand side by side as the row is
//! drawn, alef at the left and lam at the right, the cell on the left draws
//! the ligature across both cells and the cell on the right draws nothing of
//! its own, as the second half of a wide character does. Where the visible
//! cursor stands on either cell, each letter draws its own joined form, so
//! that the cursor marks one letter.
//!
//! A cell with a mirrored character (rule L4) draws the counterpart: the
//! frame gives a single character mirrored already, and a cluster's first
//! character here, its marks drawn on the counterpart.

use vtium_term::{Frame, FrameCell, Glyph};

/// ZERO WIDTH JOINER, which keys a joined form on each side it joins.
pub const ZWJ: char = '\u{200d}';

/// ARABIC LETTER LAM.
const LAM: char = '\u{644}';

/// The alefs that join a lam before them into the lam-alef ligature: alef
/// with madda above, with hamza above, with hamza below, and alef. These
/// are the four whose ligatures Unicode's Arabic Presentation Forms-B hold,
/// U+FEF5 to U+FEFC.
const ALEFS: [char; 4] = ['\u{622}', '\u{623}', '\u{625}', '\u{627}'];

/// How the cell at a column takes part in drawing its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    /// The cell draws on its own.
    One,
    /// The cell is the alef of a lam-alef ligature, and draws the
    /// ligature across itself and the cell after it, the lam.
    Pair,
    /// The cell is the lam of a lam-alef ligature, which the cell before
    /// it draws.
    Tail,
}

/// The first character of a cell's glyph.
fn first(glyph: Glyph<'_>) -> Option<char> {
    match glyph {
        Glyph::Empty => None,
        Glyph::Char(c) => Some(c),
        Glyph::Cluster(s) => s.chars().next(),
    }
}

/// Whether `left` and `right`, two cells side by side as a row is drawn,
/// are an alef and the lam before it that join into the lam-alef ligature:
/// the lam joins the letter read after it and the alef the letter read
/// before it, so the two are read lam then alef, right to left.
fn ligature(left: &FrameCell<'_>, right: &FrameCell<'_>) -> bool {
    let alef = first(left.glyph).is_some_and(|c| ALEFS.contains(&c));
    let lam = first(right.glyph) == Some(LAM);
    alef && lam && left.joins.before && right.joins.after
}

/// How the cell `fc` at column `col` of `row`, counted from the left as
/// drawn, takes part in drawing its row. `held` is where the visible cursor
/// stands, in either phase of its blink: a lam-alef ligature with the
/// cursor on either of its cells draws as two letters. Only an alef or a
/// lam that joins looks at its neighbour.
pub fn span(
    frame: &Frame<'_>,
    (col, row): (usize, usize),
    fc: &FrameCell<'_>,
    held: Option<(usize, usize)>,
) -> Span {
    let free =
        |left: usize, right: usize| held.is_none_or(|(c, r)| r != row || (c != left && c != right));
    let c = first(fc.glyph);
    if fc.joins.before && c.is_some_and(|c| ALEFS.contains(&c)) {
        let right = col.checked_add(1);
        let lam = right.and_then(|r| frame.cell(row, r));
        if let (Some(right), Some(lam)) = (right, lam)
            && free(col, right)
            && ligature(fc, &lam)
        {
            return Span::Pair;
        }
    } else if fc.joins.after && c == Some(LAM) {
        let left = col.checked_sub(1);
        let alef = left.and_then(|l| frame.cell(row, l));
        if let (Some(left), Some(alef)) = (left, alef)
            && free(left, col)
            && ligature(&alef, fc)
        {
            return Span::Tail;
        }
    }
    Span::One
}

/// Appends to `out` the text of `fc`'s own glyph as it draws: its cluster,
/// its first character mirrored where the frame says so, and U+200D on
/// each side it joins. Nothing for an empty cell.
pub fn text(fc: &FrameCell<'_>, out: &mut Vec<char>) {
    let start = out.len();
    match fc.glyph {
        Glyph::Empty => return,
        Glyph::Char(c) => out.push(c),
        Glyph::Cluster(s) => {
            let mut chars = s.chars();
            if let Some(c) = chars.next() {
                out.push(fc.mirror.unwrap_or(c));
            }
            out.extend(chars);
        }
    }
    if fc.joins.before {
        out.insert(start, ZWJ);
    }
    if fc.joins.after {
        out.push(ZWJ);
    }
}

/// Appends to `out` the text of the lam-alef ligature that `alef`, the cell
/// on the left, and `lam`, the cell on the right, draw together: lam, with
/// U+200D before it where it joins the letter read before it, and its
/// marks, then alef and its marks. The two letters are read lam first.
pub fn pair_text(alef: &FrameCell<'_>, lam: &FrameCell<'_>, out: &mut Vec<char>) {
    if lam.joins.before {
        out.push(ZWJ);
    }
    for cell in [lam, alef] {
        match cell.glyph {
            Glyph::Empty => {}
            Glyph::Char(c) => out.push(c),
            Glyph::Cluster(s) => out.extend(s.chars()),
        }
    }
}

/// Appends to `out` the text the cell `fc` at column `col` of `row` draws,
/// as [`span`] places it with the cursor `held` where it is: the cell's own
/// text ([`text`]), the ligature's for the alef of a pair ([`pair_text`]),
/// and nothing for the lam of a pair. Returns the cell's span.
pub fn drawn(
    frame: &Frame<'_>,
    (col, row): (usize, usize),
    fc: &FrameCell<'_>,
    held: Option<(usize, usize)>,
    out: &mut Vec<char>,
) -> Span {
    let span = span(frame, (col, row), fc, held);
    match span {
        Span::One => text(fc, out),
        Span::Pair => {
            if let Some(lam) = col.checked_add(1).and_then(|r| frame.cell(row, r)) {
                pair_text(fc, &lam, out);
            }
        }
        Span::Tail => {}
    }
    span
}
