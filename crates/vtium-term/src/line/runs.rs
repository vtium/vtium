//! Sized runs on a line: the keys one OSC 66 sequence sizes its text with,
//! each line's share of the run, and the line's edits that keep its run
//! table and its cells' run marks in step.

use core::ops::Range;

#[cfg(doc)]
use super::memory::run_fit;
use super::{Glyph, Line, LinkId, is_placeholder};
use crate::cell::{Cell, ClusterOffset, Content, Width};
use crate::style::Style;

/// How one OSC 66 sequence sizes its text, with each of kitty's text
/// sizing protocol's keys inside its range.
///
/// References:
/// - ADR-1308 D2: the six keys vtium takes, their ranges and their
///   defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSize {
    /// `s`, 1 to 7: the run is this many rows high, and this many times as
    /// wide as its text's own cells.
    pub scale: u8,
    /// `w`, 0 to 7: the width in cells that the program gave the text, or
    /// 0 when vtium measured the text itself.
    pub width: u8,
    /// `n`, 0 to 15: the numerator of the fraction the text draws at.
    pub numerator: u8,
    /// `d`, 0 to 15: the denominator of that fraction, 0 when the text
    /// fills its block, and otherwise above the numerator.
    pub denominator: u8,
    /// `v`, 0 to 2: where text drawn at a fraction sits in its block, at
    /// the top (0), the bottom (1) or the middle (2).
    pub vertical: u8,
    /// `h`, 0 to 2: where text drawn at a fraction sits in its block, at
    /// the left (0), the right (1) or the middle (2).
    pub horizontal: u8,
}

impl TextSize {
    /// Each key at its default: text of the normal size.
    pub const NORMAL: Self = Self {
        scale: 1,
        width: 0,
        numerator: 0,
        denominator: 0,
        vertical: 0,
        horizontal: 0,
    };

    /// Whether the program set the text's width with `w`, so that no
    /// character that follows changes it.
    pub const fn width_set(self) -> bool {
        self.width != 0
    }

    /// Whether this size draws text just as printing it does: scale 1, no
    /// width given and no fraction. OSC 66 then makes no run.
    pub const fn is_normal(self) -> bool {
        self.scale <= 1 && self.width == 0 && self.denominator == 0
    }
}

/// One line's share of a sized run: the block of cells that one OSC 66
/// sequence writes, which every edit treats as one unit. Each line that a
/// row of the run lies on holds the row it holds, so no line refers to
/// another one.
///
/// References:
/// - ADR-1308 D3: the line's run table and what each entry holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SizedRun {
    col: u16,
    /// The rest of the entry in 27 of 32 bits, kept as two 16-bit halves
    /// so the entry aligns to 2 bytes and takes 6: the run's width in
    /// columns, 1 to 49 in 6 bits, which row of the run the line holds, 0
    /// to 6 in 3 bits, then the keys s, w, n, d, v and h in 3, 3, 4, 4, 2
    /// and 2 bits ([`SizedRun::FIELDS`]).
    packed: [u16; 2],
}

// A run's entry takes 6 bytes in its line's run table, on every target.
const _: () = assert!(size_of::<SizedRun>() == 6 && align_of::<SizedRun>() == 2);

impl SizedRun {
    /// The bit each field of the packed entry starts at, and its width in
    /// bits, in the order of [`SizedRun::fields`].
    const FIELDS: [(u32, u32); 8] = [
        (0, 6),
        (6, 3),
        (9, 3),
        (12, 3),
        (15, 4),
        (19, 4),
        (23, 2),
        (25, 2),
    ];

    /// The share of a run that starts at column `col` and is `cols`
    /// columns wide, for the run's row `row`, counted from 0 at its top.
    /// Each value keeps the bits its field has: OSC 66's keys and the run
    /// sizes they make fit them.
    ///
    /// References:
    /// - ADR-1009 D2: the 6-byte entry and the width of each field.
    pub(crate) fn new(col: usize, cols: usize, row: usize, size: TextSize) -> Self {
        let values = [
            u32::try_from(cols).unwrap_or(u32::MAX),
            u32::try_from(row).unwrap_or(u32::MAX),
            u32::from(size.scale),
            u32::from(size.width),
            u32::from(size.numerator),
            u32::from(size.denominator),
            u32::from(size.vertical),
            u32::from(size.horizontal),
        ];
        let mut bits = 0u32;
        for ((shift, width), value) in Self::FIELDS.into_iter().zip(values) {
            let max = (1u32 << width).saturating_sub(1);
            bits |= value.min(max) << shift;
        }
        let [a, b, c, d] = bits.to_le_bytes();
        Self {
            col: u16::try_from(col).unwrap_or(u16::MAX),
            packed: [u16::from_le_bytes([a, b]), u16::from_le_bytes([c, d])],
        }
    }

    /// The packed fields' values, in the order [`SizedRun::FIELDS`] lists
    /// them.
    fn fields(self) -> [u8; 8] {
        let [a, b] = self.packed[0].to_le_bytes();
        let [c, d] = self.packed[1].to_le_bytes();
        let bits = u32::from_le_bytes([a, b, c, d]);
        Self::FIELDS.map(|(shift, width)| {
            let max = (1u32 << width).saturating_sub(1);
            u8::try_from((bits >> shift) & max).unwrap_or(u8::MAX)
        })
    }

    /// The run's first column.
    pub fn col(self) -> usize {
        usize::from(self.col)
    }

    /// How many columns the run takes.
    pub fn cols(self) -> usize {
        usize::from(self.fields()[0])
    }

    /// The columns the run takes on its line.
    pub fn span(self) -> Range<usize> {
        self.col()..self.col().saturating_add(self.cols())
    }

    /// Which row of the run this line holds, 0 for its top row.
    pub fn row(self) -> usize {
        usize::from(self.fields()[1])
    }

    /// How many rows the run takes: its scale.
    pub fn rows(self) -> usize {
        usize::from(self.fields()[2].max(1))
    }

    /// The keys the run was written with.
    pub fn size(self) -> TextSize {
        let [
            _,
            _,
            scale,
            width,
            numerator,
            denominator,
            vertical,
            horizontal,
        ] = self.fields();
        TextSize {
            scale,
            width,
            numerator,
            denominator,
            vertical,
            horizontal,
        }
    }

    /// The same share, moved to column `col`.
    pub(crate) fn at(self, col: usize) -> Self {
        Self {
            col: u16::try_from(col).unwrap_or(u16::MAX),
            ..self
        }
    }
}

impl core::fmt::Debug for SizedRun {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SizedRun")
            .field("col", &self.col())
            .field("cols", &self.cols())
            .field("row", &self.row())
            .field("size", &self.size())
            .finish()
    }
}

impl Line {
    /// The share of each sized run that crosses the line, ordered by
    /// column. A line with no run has an empty table.
    pub fn runs(&self) -> &[SizedRun] {
        self.tables.as_ref().map_or(&[], |t| t.runs.as_slice())
    }

    /// Whether the line holds a share of any sized run. Every run check on
    /// the print and edit paths asks this first. A run marks its line as
    /// holding wide cells, so a line with no wide cell answers from that
    /// mark alone, and a line with one reads whether its run table is
    /// empty. A line without runs pays those one or two reads and nothing
    /// more.
    ///
    /// References:
    /// - ADR-1009 D9: a line with no run pays no time for runs.
    #[inline]
    pub(crate) fn has_runs(&self) -> bool {
        self.wide && self.tables.as_ref().is_some_and(|t| !t.runs.is_empty())
    }

    /// The share of the sized run that holds column `col`, if a run does.
    pub fn run_at(&self, col: usize) -> Option<SizedRun> {
        if !self.has_runs() || !self.cells.get(col).is_some_and(|c| c.in_run()) {
            return None;
        }
        self.runs()
            .iter()
            .copied()
            .find(|r| r.span().contains(&col))
    }

    /// Whether the line holds a row of a sized run taller than one row.
    /// Such a line keeps its columns at a width change.
    ///
    /// References:
    /// - ADR-1308 D6: a line that holds any row of a run taller than one
    ///   row does not rewrap.
    pub(crate) fn holds_tall_run(&self) -> bool {
        self.runs().iter().any(|r| r.rows() > 1)
    }

    /// Writes the share `run` of a sized run: `glyph`, the run's text, in
    /// its first cell, and its other cells empty, each in `style` and
    /// `link` and marked as part of the run. A run already in those cells
    /// goes from this line first. Returns false, changing nothing, when
    /// the run does not fit inside the line.
    pub(crate) fn put_run(
        &mut self,
        run: SizedRun,
        glyph: Glyph<'_>,
        style: &Style,
        link: Option<LinkId>,
    ) -> bool {
        let span = run.span();
        if run.cols() == 0 || span.end > self.cells.len() {
            return false;
        }
        self.drop_runs(span.clone());
        // The other cells give back their clusters before the first cell
        // stores the text, so the text has the room that
        // [`run_room`](Self::run_room) reports.
        for col in span.clone().skip(1) {
            self.write(col, Glyph::Empty, Width::Narrow, style, link);
        }
        self.write(span.start, glyph, Width::Narrow, style, link);
        self.mark_run(run);
        true
    }

    /// The bytes of the line's cluster budget that a sized run written over
    /// the cells `cols` has for its text: the budget less what the cells
    /// outside `cols` name. A run's text keeps to this room, so the line
    /// never cuts it in the middle of a grapheme cluster ([`run_fit`]).
    pub(crate) fn run_room(&self, cols: Range<usize>) -> usize {
        let Some(t) = self.tables.as_ref() else {
            return self.cluster_budget();
        };
        let freed: usize = self
            .cells
            .get(cols)
            .unwrap_or_default()
            .iter()
            .filter_map(|c| match c.content() {
                Content::Cluster(ClusterOffset(o)) if !t.cluster(o).is_some_and(is_placeholder) => {
                    Some(t.cluster_size(o))
                }
                _ => None,
            })
            .fold(0, usize::saturating_add);
        self.cluster_budget()
            .saturating_sub(t.live().saturating_sub(freed))
    }

    /// Replaces the text of the share `run` of a sized run with `glyph`,
    /// keeping the run: a codepoint that joins its text.
    pub(crate) fn set_run_text(&mut self, run: SizedRun, glyph: Glyph<'_>) {
        if !self.runs().contains(&run) {
            return;
        }
        let cell = self.cell(run.col());
        let (style, link) = (self.style(cell), self.link(cell));
        self.drop_runs(run.span());
        self.write(run.col(), glyph, Width::Narrow, &style, link);
        self.mark_run(run);
    }

    /// Marks the cells of `run` as part of it and adds it to the run
    /// table, for cells that already hold the run's text: a rewrap or a
    /// scroll inside the margins moves a run's cells, then marks them.
    pub(crate) fn mark_run(&mut self, run: SizedRun) {
        let span = run.span();
        if run.cols() == 0 || span.end > self.cells.len() {
            return;
        }
        for cell in self.cells.get_mut(span).unwrap_or_default() {
            *cell = cell.with_run(true);
        }
        self.wide = true;
        let cells = self.cells.len();
        self.tables.get_or_insert_default().runs.insert(run, cells);
    }

    /// Takes every run that holds a column of `cols` off the line: its
    /// cells keep their style and link, lose their text and their mark,
    /// and become narrow cells. A run whose cells an edit would separate,
    /// or partly overwrite, goes whole, as a wide character does. On this
    /// line that is this row of the run. The screen takes the run's other
    /// rows off their lines.
    #[inline]
    pub(super) fn drop_runs(&mut self, cols: Range<usize>) {
        if self.has_runs() {
            self.drop_runs_where(|r| r.col() < cols.end && r.span().end > cols.start);
        }
    }

    /// Takes each run that `gone` holds for off the line, as
    /// [`drop_runs`](Self::drop_runs) does.
    #[cold]
    #[inline(never)]
    pub(super) fn drop_runs_where(&mut self, gone: impl Fn(SizedRun) -> bool) {
        let Some(t) = self.tables.as_mut() else {
            return;
        };
        if !t.runs.as_slice().iter().any(|&r| gone(r)) {
            return;
        }
        let mut dropped = Vec::new();
        t.runs.retain(|&r| {
            let keep = !gone(r);
            if !keep {
                dropped.push(r.span());
            }
            keep
        });
        for span in dropped {
            for col in span {
                let Some(&old) = self.cells.get(col) else {
                    continue;
                };
                self.release(old);
                if let Some(cell) = self.cells.get_mut(col) {
                    *cell = Cell::new(Content::Empty, Width::Narrow, old.style(), old.link());
                }
            }
        }
    }

    /// Moves the runs of `at..end` as `insert_cells` (`insert` true) or
    /// `delete_cells` moves their cells by `n`. A run the move would cut,
    /// one across `at`, across `end`, across the deleted cells, or pushed
    /// partly past `end`, goes from the line first.
    pub(super) fn shift_runs(&mut self, at: usize, n: usize, end: usize, insert: bool) {
        if !self.has_runs() {
            return;
        }
        let cut = |r: SizedRun| {
            let span = r.span();
            if span.end <= at || span.start >= end {
                return false;
            }
            if span.start < at || span.end > end {
                return true;
            }
            if insert {
                span.end.saturating_add(n) > end
            } else {
                span.start < at.saturating_add(n)
            }
        };
        self.drop_runs_where(cut);
        if let Some(t) = self.tables.as_mut() {
            for run in t.runs.iter_mut() {
                if run.col() >= at && run.col() < end {
                    let col = if insert {
                        run.col().saturating_add(n)
                    } else {
                        run.col().saturating_sub(n)
                    };
                    *run = run.at(col);
                }
            }
        }
    }
}
