//! Kitty's Unicode placeholders (ADR-1100, D13): vtium-term reads the
//! cells a program writes U+10EEEE into and groups them into the runs the
//! image store draws a virtual placement through.
//!
//! A placeholder cell is a cell whose text is U+10EEEE followed by up to
//! three combining marks from [`DIACRITICS`], the row and column
//! diacritics. A mark's value is its place in that table, counted from 0:
//! the first mark gives the cell's row in the picture, the second its
//! column, and the third the top byte of the image id. The cell's
//! foreground color gives the image id's low 24 bits: a palette index, or a
//! direct color's 24-bit value. Its underline color gives the placement id
//! the same way. The default foreground color, or an image id of 0, names
//! no image, and the default underline color names no placement.
//!
//! A cell that leaves marks out takes them from the placeholder cell to its
//! left, when that cell has the same foreground and underline colors, by
//! the kitty spec's three rules ([`Reader`]).
//!
//! A *run* is a stretch of side-by-side placeholder cells on one row that
//! name the same image id and placement id, lie on the same row of the
//! picture and show consecutive columns of it. vtium-term keeps no runs: it
//! reads them from the cells each time they are needed, so a write, an
//! erase, a scroll or a rewrap that moves the cells moves the runs with
//! them.
//!
//! The table of 297 diacritics is taken from Ghostty's
//! `src/terminal/kitty/graphics_unicode.zig` at commit 0b1729e945, which
//! Ghostty derived from the list the kitty spec publishes and released
//! under the MIT license:
//!
//! > Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining
//! > a copy of this software and associated documentation files (the
//! > "Software"), to deal in the Software without restriction, including
//! > without limitation the rights to use, copy, modify, merge, publish,
//! > distribute, sublicense, and/or sell copies of the Software, and to
//! > permit persons to whom the Software is furnished to do so, subject to
//! > the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be
//! > included in all copies or substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
//! > EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! > MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! > NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
//! > LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
//! > OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
//! > WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//!
//! A test checks every entry against the Unicode Character Database that
//! vtium-term pins: `unicode/above_marks.txt`, which `gen-unicode` writes
//! from UnicodeData.txt, lists every nonspacing mark with canonical
//! combining class 230 and no decomposition mapping.

use vtium_image::{LineKey, PlaceholderRun};

use crate::cell::Cell;
use crate::line::{Glyph, Line, LineFlags, PLACEHOLDER};
use crate::style::Color;

/// The row and column diacritics, in increasing code point order: a
/// mark's value is its index.
pub(crate) const DIACRITICS: [char; 297] = [
    '\u{0305}',
    '\u{030D}',
    '\u{030E}',
    '\u{0310}',
    '\u{0312}',
    '\u{033D}',
    '\u{033E}',
    '\u{033F}',
    '\u{0346}',
    '\u{034A}',
    '\u{034B}',
    '\u{034C}',
    '\u{0350}',
    '\u{0351}',
    '\u{0352}',
    '\u{0357}',
    '\u{035B}',
    '\u{0363}',
    '\u{0364}',
    '\u{0365}',
    '\u{0366}',
    '\u{0367}',
    '\u{0368}',
    '\u{0369}',
    '\u{036A}',
    '\u{036B}',
    '\u{036C}',
    '\u{036D}',
    '\u{036E}',
    '\u{036F}',
    '\u{0483}',
    '\u{0484}',
    '\u{0485}',
    '\u{0486}',
    '\u{0487}',
    '\u{0592}',
    '\u{0593}',
    '\u{0594}',
    '\u{0595}',
    '\u{0597}',
    '\u{0598}',
    '\u{0599}',
    '\u{059C}',
    '\u{059D}',
    '\u{059E}',
    '\u{059F}',
    '\u{05A0}',
    '\u{05A1}',
    '\u{05A8}',
    '\u{05A9}',
    '\u{05AB}',
    '\u{05AC}',
    '\u{05AF}',
    '\u{05C4}',
    '\u{0610}',
    '\u{0611}',
    '\u{0612}',
    '\u{0613}',
    '\u{0614}',
    '\u{0615}',
    '\u{0616}',
    '\u{0617}',
    '\u{0657}',
    '\u{0658}',
    '\u{0659}',
    '\u{065A}',
    '\u{065B}',
    '\u{065D}',
    '\u{065E}',
    '\u{06D6}',
    '\u{06D7}',
    '\u{06D8}',
    '\u{06D9}',
    '\u{06DA}',
    '\u{06DB}',
    '\u{06DC}',
    '\u{06DF}',
    '\u{06E0}',
    '\u{06E1}',
    '\u{06E2}',
    '\u{06E4}',
    '\u{06E7}',
    '\u{06E8}',
    '\u{06EB}',
    '\u{06EC}',
    '\u{0730}',
    '\u{0732}',
    '\u{0733}',
    '\u{0735}',
    '\u{0736}',
    '\u{073A}',
    '\u{073D}',
    '\u{073F}',
    '\u{0740}',
    '\u{0741}',
    '\u{0743}',
    '\u{0745}',
    '\u{0747}',
    '\u{0749}',
    '\u{074A}',
    '\u{07EB}',
    '\u{07EC}',
    '\u{07ED}',
    '\u{07EE}',
    '\u{07EF}',
    '\u{07F0}',
    '\u{07F1}',
    '\u{07F3}',
    '\u{0816}',
    '\u{0817}',
    '\u{0818}',
    '\u{0819}',
    '\u{081B}',
    '\u{081C}',
    '\u{081D}',
    '\u{081E}',
    '\u{081F}',
    '\u{0820}',
    '\u{0821}',
    '\u{0822}',
    '\u{0823}',
    '\u{0825}',
    '\u{0826}',
    '\u{0827}',
    '\u{0829}',
    '\u{082A}',
    '\u{082B}',
    '\u{082C}',
    '\u{082D}',
    '\u{0951}',
    '\u{0953}',
    '\u{0954}',
    '\u{0F82}',
    '\u{0F83}',
    '\u{0F86}',
    '\u{0F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

/// The most columns a row of a picture spans: a cell's column comes from
/// a diacritic's value, so no picture row passes [`DIACRITICS`]'s length.
const PICTURE_COLUMNS_MAX: usize = DIACRITICS.len();

/// The value of the diacritic `c`, its index in [`DIACRITICS`].
fn value(c: char) -> Option<u32> {
    DIACRITICS
        .binary_search(&c)
        .ok()
        .and_then(|i| u32::try_from(i).ok())
}

/// What a placeholder cell says on its own: its colors, and the values of
/// the marks it carries, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Said {
    fg: Color,
    underline: Color,
    row: Option<u32>,
    column: Option<u32>,
    high: Option<u32>,
}

/// A placeholder cell with its marks filled in by the rules for marks left
/// out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Read {
    /// The column after the cell's: where a cell that inherits from it
    /// stands. 0 for a cell carried over a soft wrap from the row before.
    next: usize,
    fg: Color,
    underline: Color,
    row: u32,
    column: u32,
    high: u32,
}

/// The low 24 bits an image id or a placement id takes from `color`:
/// `None` for the default color.
fn id_bits(color: Color) -> Option<u32> {
    match color {
        Color::Default => None,
        Color::Indexed(i) => Some(u32::from(i)),
        Color::Rgb(c) => Some(u32::from_be_bytes([0, c.r, c.g, c.b])),
    }
}

/// Whether the cell `cell` of `line` holds a placeholder.
pub(crate) fn is_placeholder_cell(line: &Line, cell: Cell) -> bool {
    match line.glyph(cell) {
        Glyph::Char(c) => c == PLACEHOLDER,
        Glyph::Cluster(s) => s.starts_with(PLACEHOLDER),
        Glyph::Empty => false,
    }
}

/// What the cell `cell` of `line` says, or `None` when it holds no
/// placeholder. The marks are read from the first; a mark that is not a
/// diacritic ends them, and a third mark past 255 gives no top byte.
fn said(line: &Line, cell: Cell) -> Option<Said> {
    let marks = match line.glyph(cell) {
        Glyph::Char(c) if c == PLACEHOLDER => "",
        Glyph::Cluster(s) => s.strip_prefix(PLACEHOLDER)?,
        _ => return None,
    };
    let mut values = marks.chars().map_while(value);
    let style = line.style(cell);
    Some(Said {
        fg: style.fg,
        underline: style.underline_color,
        row: values.next(),
        column: values.next(),
        high: values.next().filter(|&v| v <= 0xff),
    })
}

/// Reads one row's placeholder cells from left to right, filling in the
/// marks a cell leaves out from the placeholder cell to its left by the
/// kitty spec's three rules, each when that cell has the same foreground
/// and underline colors:
///
/// - a cell with no mark takes its left neighbor's row, its column plus
///   one and its top byte;
/// - a cell with only a row takes its column plus one and its top byte,
///   when the neighbor is on the same row of the picture;
/// - a cell with a row and a column takes its top byte, when the neighbor
///   is on the same row of the picture and in the column before.
///
/// A row, column or top byte that no rule gives is 0. When the row before
/// soft-wrapped into this one and its last cell is a placeholder, that cell
/// is the left neighbor of this row's first cell, so a picture row that a
/// wrap splits keeps counting its columns (the maintainer, 2026-11-02); a
/// hard line break restarts them.
#[derive(Default)]
struct Reader {
    left: Option<Read>,
}

/// The last placeholder cell of a row that soft-wrapped, which the next
/// row's first cell may inherit from ([`Reader`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Carry(Option<Read>);

impl Reader {
    fn read(&mut self, col: usize, said: Option<Said>) -> Option<Read> {
        let Some(s) = said else {
            self.left = None;
            return None;
        };
        let left = self
            .left
            .filter(|l| l.next == col)
            .filter(|l| l.fg == s.fg && l.underline == s.underline);
        let next = |l: Read| l.column.saturating_add(1);
        let (row, column, high) = match (s.row, s.column, s.high, left) {
            (Some(r), Some(c), Some(h), _) => (r, c, h),
            (Some(r), Some(c), None, Some(l)) if l.row == r && next(l) == c => (r, c, l.high),
            (Some(r), Some(c), None, _) => (r, c, 0),
            (Some(r), None, _, Some(l)) if l.row == r => (r, next(l), l.high),
            (Some(r), None, _, _) => (r, 0, 0),
            (None, _, _, Some(l)) => (l.row, next(l), l.high),
            (None, _, _, None) => (0, 0, 0),
        };
        let read = Read {
            next: col.saturating_add(1),
            fg: s.fg,
            underline: s.underline,
            row,
            column,
            high,
        };
        self.left = Some(read);
        Some(read)
    }
}

/// What the rows above the first row a read starts at hand on to it across
/// soft wraps: `above` gives them nearest first. The walk goes back while
/// each row soft-wrapped into the one below it and ends in a placeholder
/// cell, at most as many rows as a picture row of [`PICTURE_COLUMNS_MAX`]
/// cells fills at `cols` columns, and reads them from the farthest down.
/// Returns the carry and the cells it read.
pub(crate) fn carry_from<'a>(above: impl Iterator<Item = &'a Line>, cols: usize) -> (Carry, usize) {
    let bound = PICTURE_COLUMNS_MAX.div_ceil(cols.max(1));
    let rows: Vec<&Line> = above
        .take(bound)
        .take_while(|line| {
            let last = cols.min(line.len()).checked_sub(1);
            line.flags().contains(LineFlags::WRAPPED)
                && last.is_some_and(|x| is_placeholder_cell(line, line.cell(x)))
        })
        .collect();
    let mut carry = Carry::default();
    let mut read = 0usize;
    for line in rows.into_iter().rev() {
        let cells = runs(
            line,
            LineKey::new(line.id().get()),
            cols,
            &mut carry,
            &mut |_| {},
        );
        read = read.saturating_add(cells);
    }
    (carry, read)
}

/// Reads the runs of placeholder cells on `line`, the line `key` names, in
/// its first `cols` columns, and hands each to `out`, left to right. A cell
/// that names no image belongs to no run. `carry` holds what the row read
/// before this one hands on across a soft wrap, and this row leaves in it
/// what it hands to the next. Returns the cells it visited.
pub(crate) fn runs(
    line: &Line,
    key: LineKey,
    cols: usize,
    carry: &mut Carry,
    out: &mut impl FnMut(PlaceholderRun),
) -> usize {
    let cols = cols.min(line.len());
    let mut reader = Reader {
        left: carry.0.take(),
    };
    let mut open: Option<PlaceholderRun> = None;
    for col in 0..cols {
        let read = reader.read(col, said(line, line.cell(col)));
        let named = read.and_then(|r| {
            let low = id_bits(r.fg)?;
            let image = low | (r.high << 24);
            (image != 0).then_some((r, image))
        });
        let Some((r, image)) = named else {
            if let Some(run) = open.take() {
                out(run);
            }
            continue;
        };
        let placement = id_bits(r.underline).filter(|&p| p != 0);
        let at = u32::try_from(col).unwrap_or(u32::MAX);
        if let Some(run) = open.as_mut() {
            let follows = run.col.saturating_add(run.cells) == at
                && run.image == image
                && run.placement == placement
                && run.row == r.row
                && run.column.saturating_add(run.cells) == r.column;
            if follows {
                run.cells = run.cells.saturating_add(1);
                continue;
            }
        }
        if let Some(run) = open.replace(PlaceholderRun {
            line: key,
            col: at,
            cells: 1,
            image,
            placement,
            row: r.row,
            column: r.column,
        }) {
            out(run);
        }
    }
    if let Some(run) = open {
        out(run);
    }
    if line.flags().contains(LineFlags::WRAPPED) {
        carry.0 = reader
            .left
            .filter(|l| l.next == cols)
            .map(|l| Read { next: 0, ..l });
    }
    cols
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    // ADR-1100, D13: each of the 297 diacritics is a nonspacing mark
    // (General_Category Mn) with canonical combining class 230 and no
    // decomposition mapping of its own, in the Unicode Character Database
    // vtium-term pins, and the table runs in increasing code point order
    // with no code point twice. `unicode/above_marks.txt` lists every such
    // mark, from UnicodeData.txt by `gen-unicode`.
    // covers: image.kitty-placeholders
    #[test]
    fn every_diacritic_is_an_above_mark_in_the_pinned_ucd() {
        let text = include_str!("unicode/above_marks.txt");
        assert!(
            text.contains(&format!("Unicode {}.", crate::UNICODE_VERSION)),
            "above_marks.txt is not from Unicode {}",
            crate::UNICODE_VERSION
        );
        let marks: Vec<char> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(|l| char::from_u32(u32::from_str_radix(l, 16).unwrap()).unwrap())
            .collect();
        assert!(marks.len() > DIACRITICS.len());
        for (i, &c) in DIACRITICS.iter().enumerate() {
            assert!(
                marks.binary_search(&c).is_ok(),
                "entry {i}, U+{:04X}, is not an above mark without a decomposition",
                u32::from(c)
            );
        }
        for (i, pair) in DIACRITICS.windows(2).enumerate() {
            assert!(
                pair.first() < pair.get(1),
                "entries {i} and {} are out of order or repeat",
                i + 1
            );
        }
        assert_eq!(DIACRITICS.first(), Some(&'\u{0305}'));
        assert_eq!(DIACRITICS.last(), Some(&'\u{1D244}'));
    }

    // ADR-1100, D13: the table is Ghostty's list, entry for entry. The
    // check against the Unicode Character Database passes any above mark
    // in order, so a wrong entry that is still such a mark, U+1DE7 for
    // U+1DE6 among them, needs this one. The checksum is FNV-1a 64 over
    // each code point's four little-endian bytes, worked out from the 297
    // entries of `diacritics` in `src/terminal/kitty/graphics_unicode.zig`
    // at Ghostty commit 0b1729e945 (the file's SHA-256 is
    // 9d49ead0774cdb4c43eaff32955a39948cee770409f7fe7dddc7899a0e4819c5).
    // covers: image.kitty-placeholders
    #[test]
    fn the_table_is_ghosttys_list_entry_for_entry() {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for c in DIACRITICS {
            for b in u32::from(c).to_le_bytes() {
                hash ^= u64::from(b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        assert_eq!(DIACRITICS.len(), 297);
        assert_eq!(hash, 0x0feb_8357_288a_293a);
    }
}
