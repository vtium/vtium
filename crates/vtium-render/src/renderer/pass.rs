//! What one frame draws: [`Pass`], the frame's inputs as each row reads
//! them, [`Place`] and [`Paint`], a cell job's place and what it paints,
//! and [`Plan`], what a frame draws, decided before anything is drawn.

use vtium_font::{Variant, Width as FontWidth};
use vtium_term::{Cursor, Frame, FrameCell, LineSize, Rgb, Width};

#[cfg(feature = "scroll-pan")]
use crate::compose::Area;
use crate::compose::Bar;
use crate::deco::Line;
use crate::link_row::LinkRow;
#[cfg(feature = "bidi")]
use crate::syllable::Runs;
use crate::target::Rect;

use super::DrawState;
use super::layout::Grid;
use super::pointer::Pointer;
use super::sized::RunPiece;

/// One frame's inputs, as each row reads them.
pub(super) struct Pass<'p, 'f> {
    pub(super) g: Grid,
    pub(super) frame: &'p Frame<'f>,
    pub(super) state: &'p DrawState,
    pub(super) cursor: Cursor,
    /// Where the cursor draws, when it shows.
    pub(super) at: Option<(usize, usize)>,
    /// The other row that draws the cursor, and the columns it covers
    /// there, when the cursor shows on a pair of double-height lines
    /// ([`Pass::pair_cells`]).
    pub(super) at_pair: Option<(usize, core::ops::Range<usize>)>,
    /// The rows and columns of the sized run the cursor stands on, when it
    /// shows on one: the cursor covers the whole block ([`Pass::run_block`]).
    pub(super) at_run: Option<(core::ops::Range<usize>, core::ops::Range<usize>)>,
    /// Where the visible cursor stands, in either phase of its blink: the
    /// letter there keeps its ink beside it inside its cells (ADR-1302 D8).
    pub(super) held: Option<(usize, usize)>,
    /// The frame without its selection, whose cells say what a selected
    /// cell draws unselected: read at 1 bit, and for a selected glyph whose
    /// ink lands outside the selection (ADR-1306 D6). Made the first time
    /// it is read.
    pub(super) plain: core::cell::OnceCell<Frame<'f>>,
    /// The overlay row showing the hovered link's address (ADR-1002,
    /// "Drawing"), whose cells take the place of the screen's.
    pub(super) link_row: Option<LinkRow<'f>>,
    /// [`Config::ligatures`]. Only a build with the bidi feature joins
    /// cells into syllables and ligatures, so only that build reads it.
    #[cfg(feature = "bidi")]
    pub(super) ligatures: bool,
    /// The syllables and ligature runs found on the row read last, so that
    /// the scan and the drawing of a row walk each run once and decide once
    /// whether it draws joined.
    #[cfg(feature = "bidi")]
    pub(super) runs: core::cell::RefCell<Runs>,
    /// The grid's rows above the screen's first row ([`Grid::above`]).
    #[cfg(feature = "scroll-pan")]
    pub(super) above: usize,
}

/// A grid's rows as the frame counts them: the screen's row `y` is the
/// grid's row `y + above`, and the rows of history above it are negative.
#[cfg(feature = "scroll-pan")]
impl<'f> Pass<'_, 'f> {
    /// The frame's position of the grid's row `row`: negative above the
    /// screen.
    pub(super) fn at(&self, row: usize) -> isize {
        let row = isize::try_from(row).unwrap_or(isize::MAX);
        row.saturating_sub(isize::try_from(self.above).unwrap_or(isize::MAX))
    }

    /// The screen's row that the grid's row `row` is, or `None` for a row
    /// of history above the screen.
    pub(super) fn screen_row(&self, row: usize) -> Option<usize> {
        row.checked_sub(self.above)
    }

    /// Whether the grid's row `row` lies wholly past the spare lines: the
    /// topmost row of history, which only sends its ink down into the row
    /// below it and draws nothing.
    pub(super) fn hidden(&self, row: usize) -> bool {
        self.above > 0 && row == 0
    }
}

/// A grid's cells at the frame's positions, which the ligatures and the
/// syllables read the rows through, so a row above the screen joins its
/// letters as a row of the screen does.
#[cfg(feature = "bidi")]
impl Pass<'_, '_> {
    /// The frame's position of the grid's cell `at`: its column, and its
    /// row counted from the screen's first, negative above the screen.
    pub(super) fn frame_at(&self, (col, row): (usize, usize)) -> (usize, isize) {
        #[cfg(feature = "scroll-pan")]
        return (col, self.at(row));
        #[cfg(not(feature = "scroll-pan"))]
        (col, isize::try_from(row).unwrap_or(isize::MAX))
    }

    /// Where the visible cursor stands, at the frame's position.
    pub(super) fn held_at(&self) -> Option<(usize, isize)> {
        self.held.map(|at| self.frame_at(at))
    }

    /// Whether the overlay row showing the hovered link covers the cell at
    /// `col` of the grid's row `row`.
    pub(super) fn covered(&self, col: usize, row: usize) -> bool {
        #[cfg(feature = "scroll-pan")]
        return self.link_covers(col, row);
        #[cfg(not(feature = "scroll-pan"))]
        self.link_row.is_some_and(|o| o.covers(col, row))
    }
}

/// The frame's rows through the pass's count of the rows above the screen,
/// the overlay row's included.
#[cfg(feature = "scroll-pan")]
impl<'f> Pass<'_, 'f> {
    /// The sized runs of the grid's row `row`.
    pub(super) fn sized_runs(&self, row: usize) -> &'f [vtium_term::SizedRun] {
        self.frame.line_at(self.at(row)).map_or(&[], |l| l.runs())
    }

    /// The line of the grid's row `row`.
    pub(super) fn line(&self, row: usize) -> Option<&'f vtium_term::Line> {
        self.frame.line_at(self.at(row))
    }

    /// The frame's own cell at `row`, column `col`, which the overlay row
    /// does not cover.
    fn frame_cell(&self, row: usize, col: usize) -> Option<FrameCell<'f>> {
        self.frame.cell_at(self.at(row), col)
    }

    /// The cell at `row`, column `col` of the frame without its selection.
    pub(super) fn plain_cell(&self, row: usize, col: usize) -> Option<FrameCell<'f>> {
        self.plain().cell_at(self.at(row), col)
    }

    /// Whether the overlay row showing the hovered link covers the cell at
    /// `col` of the grid's row `row`.
    pub(super) fn link_covers(&self, col: usize, row: usize) -> bool {
        let Some(row) = self.screen_row(row) else {
            return false;
        };
        self.link_row.is_some_and(|o| o.covers(col, row))
    }

    /// Whether the grid's row `row` is the overlay row's.
    pub(super) fn link_on(&self, row: usize) -> bool {
        let Some(row) = self.screen_row(row) else {
            return false;
        };
        self.link_row.is_some_and(|o| o.row() == row)
    }
}

impl<'f> Pass<'_, 'f> {
    /// The share of the sized run that the cell at `col`, `row` belongs
    /// to, if any: the block of cells that one OSC 66 sequence writes.
    pub(super) fn run_at(&self, row: usize, col: usize) -> Option<vtium_term::SizedRun> {
        #[cfg(feature = "scroll-pan")]
        return self
            .sized_runs(row)
            .iter()
            .copied()
            .find(|r| r.span().contains(&col));
        #[cfg(not(feature = "scroll-pan"))]
        self.frame
            .sized_runs(row)
            .iter()
            .copied()
            .find(|r| r.span().contains(&col))
    }

    /// The piece of its sized run that the cell at `col`, `row` draws,
    /// and the run's first cell on that row, which holds the run's text;
    /// `None` for a cell of no run.
    pub(super) fn run_piece(&self, row: usize, col: usize) -> Option<(RunPiece, FrameCell<'f>)> {
        let run = self.run_at(row, col)?;
        #[cfg(feature = "scroll-pan")]
        let first = self.frame_cell(row, run.col())?;
        #[cfg(not(feature = "scroll-pan"))]
        let first = self.frame.cell(row, run.col())?;
        Some((RunPiece::at(run, col)?, first))
    }

    /// Whether the cell at `col`, `row` draws the cursor: the cursor's own
    /// cell, and on a pair of double-height lines the cells of the pair's
    /// other line under the columns the cursor covers.
    pub(super) fn shows_cursor(&self, col: usize, row: usize) -> bool {
        self.at == Some((col, row))
            || self
                .at_pair
                .as_ref()
                .is_some_and(|(r, cols)| *r == row && cols.contains(&col))
            || self
                .at_run
                .as_ref()
                .is_some_and(|(rows, cols)| rows.contains(&row) && cols.contains(&col))
    }

    /// The rows and columns of the sized run whose cell `at` is, as the
    /// screen shows them: the cursor that stands on any cell of a run
    /// covers the whole block. Rows of the run that have scrolled above
    /// the screen are left out. `None` off a run.
    ///
    /// References:
    /// - ADR-1308 D6, "The cursor's drawing": the cursor covers every cell
    ///   of the run.
    pub(super) fn run_block(
        &self,
        at: Option<(usize, usize)>,
    ) -> Option<(core::ops::Range<usize>, core::ops::Range<usize>)> {
        let (col, row) = at?;
        let run = self.run_at(row, col)?;
        // the run's rows below this one; its top may lie above the screen
        let below = run.rows().saturating_sub(run.row());
        let top = row.saturating_sub(run.row());
        // the cursor stays on the screen's rows
        #[cfg(feature = "scroll-pan")]
        let top = top.max(self.above);
        let bottom = row.saturating_add(below).min(self.g.rows);
        Some((top..bottom, run.span()))
    }

    /// The other row of a pair of double-height lines that draws the
    /// cursor standing at `at`, and the columns it covers there: the
    /// columns of the character the cursor stands on, from its left cell.
    /// So the cursor covers the whole doubled character, whichever line of
    /// the pair it stands on. Each line draws its half of the cursor, cut
    /// from the cursor doubled as its glyph is: a block covers both halves,
    /// a bar runs down both lines, and an underline lies under the bottom
    /// half. `None` on a line that belongs to no pair, where the cursor
    /// covers its own line alone. The terminal's cursor stays on its own
    /// line. vtium-term's [`Frame::cursor_extent`] decides which rows the
    /// cursor covers.
    pub(super) fn pair_cells(
        &self,
        at: Option<(usize, usize)>,
    ) -> Option<(usize, core::ops::Range<usize>)> {
        let (col, row) = at?;
        #[cfg(feature = "scroll-pan")]
        let cells = match self.frame_cell(row, col).map(|c| c.width) {
            Some(Width::Wide) => 2,
            _ => 1,
        };
        #[cfg(not(feature = "scroll-pan"))]
        let cells = match self.frame.cell(row, col).map(|c| c.width) {
            Some(Width::Wide) => 2,
            _ => 1,
        };
        // the cursor stands on the screen's rows, which the extent counts
        #[cfg(feature = "scroll-pan")]
        let (row, above) = (self.screen_row(row)?, self.above);
        let other = self
            .frame
            .cursor_extent(row, cells)
            .rows
            .find(|&r| r != row)?;
        #[cfg(feature = "scroll-pan")]
        let other = other.saturating_add(above);
        Some((other, col..col.saturating_add(cells)))
    }

    /// The frame without its selection.
    pub(super) fn plain(&self) -> &Frame<'f> {
        #[cfg(feature = "scroll-pan")]
        return self.plain.get_or_init(|| {
            self.frame
                .clone()
                .with_selection(Vec::new())
                .with_selection_beyond(Vec::new(), Vec::new())
        });
        #[cfg(not(feature = "scroll-pan"))]
        self.plain
            .get_or_init(|| self.frame.clone().with_selection(Vec::new()))
    }

    /// The cell drawn at `row`, column `col`, the overlay row's where it
    /// covers the cell ([`Frame::cell`]).
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn cell(&self, row: usize, col: usize) -> Option<FrameCell<'f>> {
        let fc = self.frame.cell(row, col)?;
        Some(self.link_row.map_or(fc, |o| o.cell(col, row, fc)))
    }

    /// The cell drawn at `row`, column `col`, the overlay row's where it
    /// covers the cell ([`Frame::cell_at`]).
    #[cfg(feature = "scroll-pan")]
    pub(super) fn cell(&self, row: usize, col: usize) -> Option<FrameCell<'f>> {
        let fc = self.frame_cell(row, col)?;
        Some(match (self.link_row, self.screen_row(row)) {
            (Some(o), Some(y)) => o.cell(col, y, fc),
            _ => fc,
        })
    }

    /// The size `row` draws at: its line's, or the normal size under the
    /// overlay row, which draws over the row at the normal size.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn size(&self, row: usize) -> LineSize {
        if self.link_on(row) {
            return LineSize::Single;
        }
        self.frame.size_at(self.at(row))
    }

    /// The size `row` draws at: its line's, or the normal size under the
    /// overlay row, which draws over the row at the normal size.
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn size(&self, row: usize) -> LineSize {
        if self.link_row.is_some_and(|o| o.row() == row) {
            return LineSize::Single;
        }
        self.frame.size(row)
    }

    /// The cells of `row` as drawn, left to right, the overlay row's where
    /// it covers them ([`Frame::cells`]).
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn cells(&self, row: usize) -> impl Iterator<Item = FrameCell<'f>> + '_ {
        let link_row = self.link_row;
        self.frame
            .cells(row)
            .enumerate()
            .map(move |(col, fc)| link_row.map_or(fc, |o| o.cell(col, row, fc)))
    }

    /// The cells of `row` as drawn, left to right, the overlay row's where
    /// it covers them ([`Frame::cells_at`]).
    #[cfg(feature = "scroll-pan")]
    pub(super) fn cells(&self, row: usize) -> impl Iterator<Item = FrameCell<'f>> + '_ {
        let link_row = self.link_row.zip(self.screen_row(row));
        self.frame
            .cells_at(self.at(row))
            .enumerate()
            .map(move |(col, fc)| link_row.map_or(fc, |(o, y)| o.cell(col, y, fc)))
    }
}

/// A cell's place: its column, row and the cells it draws over.
#[derive(Clone, Copy, Debug)]
pub(super) struct Place {
    pub(super) col: usize,
    pub(super) row: usize,
    pub(super) ncells: usize,
}

/// What a job paints: its colors, decorations and text.
#[derive(Clone, Copy, Debug)]
pub(super) struct Paint {
    pub(super) fg: Rgb,
    pub(super) bg: Rgb,
    pub(super) underline: Option<(Line, Rgb)>,
    /// The strike-through and the overline, drawn over the glyph.
    pub(super) over: Option<(Line, Rgb)>,
    pub(super) text: Option<(usize, usize, FontWidth, Variant)>,
    /// At 1 bit, whether the background is lit, where vtium marks the cell
    /// (`mono::marked`).
    pub(super) lit: Option<bool>,
    /// A bar cursor's bar, in the color it draws before the display's
    /// depth changes it.
    pub(super) bar: Option<Bar>,
}

/// What a frame draws, decided before anything is drawn.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) grid: Grid,
    pub(super) pointer: Option<Pointer>,
    /// The buffer lacks the whole border.
    pub(super) full: bool,
    /// The areas of the border it lacks, when not the whole.
    pub(super) border: Vec<Rect>,
    /// The display area the frame's rows cover, which the border leaves:
    /// the screen's grid and the rows of history above it that exist.
    /// `None` for an empty grid.
    #[cfg(feature = "scroll-pan")]
    pub(super) drawn: Option<Area>,
}
