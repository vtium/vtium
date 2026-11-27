//! What changed for the buffer being drawn: the rows and cells that
//! changed, the record of what each cell last drew, and how far each
//! cell's ink reaches past its cells.

use vtium_font::{Face, Width as FontWidth};
use vtium_term::{Attrs, Frame, FrameCell, Glyph, LineSize, Width};

use crate::reach::Reach;

use super::cpu::Back;
use super::jobs::{blink_off, draws_no_text};
#[cfg(feature = "bidi")]
use super::jobs::{drawn_text, joined_syllable};
use super::pass::Pass;
use super::{BlinkStyle, Renderer, double};

impl<F: Face> Renderer<F> {
    /// Stamps every cell of each row whose size changed since the last
    /// frame, so each buffer draws the row again whole, and forgets the
    /// reach of its cells, which stamps the neighbours their ink reached.
    /// A row of double size keeps the reach of its display cells, which
    /// the scan reads again once the size has changed. A frame whose rows
    /// keep their sizes does one comparison for each row.
    pub(super) fn resize_rows(&mut self, p: &Pass<'_, '_>) {
        let (cols, rows) = (p.g.cols, p.g.rows);
        self.sizes.resize(rows, LineSize::Single);
        for row in 0..rows {
            let size = p.size(row);
            let Some(last) = self.sizes.get_mut(row) else {
                break;
            };
            if core::mem::replace(last, size) == size {
                continue;
            }
            for col in 0..cols {
                self.cells.stamp(col, row);
                self.reach_to(col, row, 1, (true, Reach::NONE));
            }
        }
        // every row is scanned after this, so the stamps alone draw the
        // cells the forgotten reaches covered
        self.reached.clear();
    }

    /// Whether a cell of `row` changed for the buffer being drawn. A frame
    /// drawn through the ring, whose lines keep their pixels as they move,
    /// asks whether the row changed in place; every other frame asks
    /// whether it changed at its place on the screen.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn row_changed(&self, p: &Pass<'_, '_>, row: usize) -> bool {
        let frame = p.frame;
        let Some(row) = p.screen_row(row) else {
            return self.above_changed(frame, p.at(row));
        };
        if self.ring.in_place() {
            return frame.row_changed_in_place(row);
        }
        frame.row_changed(row)
    }

    /// Whether a cell of `row` changed for the buffer being drawn, at its
    /// place on the screen.
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn row_changed(&self, frame: &Frame<'_>, row: usize) -> bool {
        frame.row_changed(row)
    }

    /// Whether the row of history at the frame's position `at`, above the
    /// screen, changed for the buffer being drawn. A frame drawn through
    /// the ring keeps the row's pixels as its line moves, so the row
    /// changed only when the frame says so. Every other frame draws each
    /// row at its place on the display, and the rows above the screen
    /// hold other lines whenever the screen moved, or may hold them when
    /// the frame cannot say how it moved.
    ///
    /// References:
    /// - ADR-1310 D2, "What the capture holds under `scroll-pan`": a row beyond the screen reads as changed only in a frame drawn whole.
    #[cfg(feature = "scroll-pan")]
    fn above_changed(&self, frame: &Frame<'_>, at: isize) -> bool {
        frame.row_changed_at(at) || (!self.ring.in_place() && frame.scrolled() != Some(0))
    }

    /// Whether the cell `fc` drawn at `col`, `row` changed for the buffer
    /// being drawn, read in place or at its place on the screen as
    /// [`Renderer::row_changed`] reads a row.
    #[cfg(feature = "scroll-pan")]
    fn cell_changed(
        &self,
        p: &Pass<'_, '_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
    ) -> bool {
        let frame = p.frame;
        let Some(row) = p.screen_row(row) else {
            return self.above_changed(frame, p.at(row));
        };
        if self.ring.in_place() {
            return frame.changed_in_place(row, col);
        }
        fc.changed
    }

    /// Whether the cell `fc` drawn at `col`, `row` changed for the buffer
    /// being drawn, at its place on the screen.
    #[cfg(not(feature = "scroll-pan"))]
    fn cell_changed(
        &self,
        frame: &Frame<'_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
    ) -> bool {
        let _ = (frame, col, row);
        fc.changed
    }

    /// Whether `row` shows a glyph that changed since the last frame
    /// ([`Renderer::shows_changed`]), read from its line's glyphs alone.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn row_shows_changed(&self, p: &Pass<'_, '_>, row: usize, cols: usize) -> bool {
        !self.changed.is_empty()
            && p.line(row).is_some_and(|line| {
                line.cells()
                    .iter()
                    .take(cols)
                    .any(|&c| self.shows_changed(&line.glyph(c)))
            })
    }

    /// Whether `row` shows a glyph that changed since the last frame
    /// ([`Renderer::shows_changed`]), read from its line's glyphs alone.
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn row_shows_changed(&self, frame: &Frame<'_>, row: usize, cols: usize) -> bool {
        !self.changed.is_empty()
            && frame.line(row).is_some_and(|line| {
                line.cells()
                    .iter()
                    .take(cols)
                    .any(|&c| self.shows_changed(&line.glyph(c)))
            })
    }

    /// Whether `glyph` shows a character [`Renderer::glyphs_changed`] named,
    /// or, after an arrival, a character of a text drawn as the font's
    /// missing glyph.
    pub(super) fn shows_changed(&self, glyph: &Glyph<'_>) -> bool {
        let changed = |c: char| self.changed.binary_search(&c).is_ok();
        match *glyph {
            _ if self.changed.is_empty() => false,
            Glyph::Empty => false,
            Glyph::Char(c) => changed(c),
            Glyph::Cluster(text) => text.chars().any(changed),
        }
    }

    /// Updates what the cell `fc` at `col`, `row`, drawing over `ncells`
    /// cells, last drew: a cell that changed is stamped, and a cell whose
    /// ink changed stamps the neighbours its old or new reach covers
    /// (ADR-1306 D5). Returns whether the cell draws: not the trailing half
    /// of a wide character, which the leading half draws.
    pub(super) fn scan_cell<B: Back>(
        &mut self,
        p: &Pass<'_, '_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        (ncells, reads): (usize, bool),
        back: &mut B,
    ) -> bool {
        // a cell that joins or leaves a wide pair draws differently
        let role = u8::try_from(ncells).unwrap_or(1);
        // each cell of a sized run draws a piece of the run's text, which
        // the run's first cell holds, and its ink stays in its cell
        let run = match self.row_runs {
            true => p.run_piece(row, col).map(|(_, first)| first),
            false => None,
        };
        let mut changed = self.cell_changed(changed_arg!(p), (col, row), fc)
            || self.shows_changed(&fc.glyph)
            || run.is_some_and(|first| self.shows_changed(&first.glyph));
        if let Some(s) = self.cells.get_mut(col, row) {
            changed |= s.replace_span(role) != role;
        }
        // a row of double size keeps the reach of each display cell, by
        // its display column, rather than of each of its cells
        let doubled = match self.sizes.get(row) {
            Some(&size) if size != LineSize::Single => Some(size),
            _ => None,
        };
        if ncells == 0 {
            // a trailing half sends no ink of its own
            if reads && doubled.is_none() {
                self.reach_to(col, row, role, (changed, Reach::NONE));
            }
            return false;
        }
        let is_cursor = p.shows_cursor(col, row);
        let off = blink_off(fc, is_cursor, p.state, self.config.blink);
        if ncells == 2 {
            // the pair draws from its leading half alone: its glyph, its
            // colors, its selection and the cursor on it. So a change to
            // the trailing half's own colors or attributes draws the same
            // pixels, and a change that splits the pair changes the span
            // that `replace_span` reads. No frame can show what this read
            // adds; it keeps the pair's two cells drawn together whichever
            // of them the frame marks, in place or at its place on the
            // screen
            let tail = col.saturating_add(1);
            changed |= p
                .cell(row, tail)
                .is_some_and(|t| self.cell_changed(changed_arg!(p), (tail, row), &t));
            if let Some(t) = self.cells.get_mut(tail, row) {
                changed |= t.replace_span(0) != 0;
            }
        }
        // a syllable drawn joined draws again whole when any of its cells
        // changes, and a cell draws again when it joins a syllable, leaves
        // one, or takes another piece (ADR-1203 D4)
        #[cfg(feature = "bidi")]
        {
            let mut scratch = core::mem::take(&mut self.chars);
            let joined = joined_syllable(&self.font, p, (col, row), fc, &mut scratch);
            self.chars = scratch;
            let piece = joined.as_ref().map(|(_, piece)| *piece);
            changed |= self.cells.replace_piece(col, row, piece) != piece;
            if let Some((s, _)) = joined {
                changed |= s.cols().any(|c| {
                    c != col
                        && p.cell(row, c)
                            .is_some_and(|m| self.cell_changed(changed_arg!(p), (c, row), &m))
                });
            }
        }
        // the cursor's look and the blink phase change this cell alone
        let ink_changed = changed;
        let Some(s) = self.cells.get_mut(col, row) else {
            return false;
        };
        let look = is_cursor.then_some((p.cursor.shape, p.cursor.color));
        changed |= s.cursor != look;
        changed |= s.replace_blink_off(off) != off;
        s.cursor = look;
        let held = p.held == Some((col, row));
        if let Some(size) = doubled {
            if changed {
                self.cells.stamp(col, row);
            }
            if reads {
                let piece = run.map_or(double::Piece::of(size, 0), |_| double::Piece::WHOLE);
                self.reach_pieces(
                    p,
                    (col, row),
                    fc,
                    (ncells, piece),
                    (held, ink_changed),
                    back,
                );
            }
            return true;
        }
        let reread = reads && (s.reach_stale(held) || ink_changed);
        if changed {
            self.cells.stamp(col, row);
        }
        if reread {
            #[cfg(not(feature = "bidi"))]
            let reach = match run {
                Some(_) => Reach::NONE,
                None => self.cell_reach(fc, (held, double::Piece::WHOLE), back),
            };
            #[cfg(feature = "bidi")]
            let reach = match run {
                Some(_) => Reach::NONE,
                None => self.cell_reach(
                    fc,
                    (p, (col, row), ncells),
                    (held, double::Piece::WHOLE),
                    back,
                ),
            };
            self.reach_to(col, row, role, (ink_changed, reach));
        }
        true
    }

    /// Records the reach of each display cell that the cell `fc` at `col`,
    /// `row`, drawing over `ncells` cells, draws on a row of double size.
    /// Each display cell draws one piece of the doubled glyph and keeps
    /// that piece's reach by its display column, as a one-cell glyph
    /// ([`double::piece_ink`] says which ink each piece keeps). A display
    /// cell's reach is read again when the cell's ink changed, as
    /// `ink_changed` says, or when the cursor came or went. `first` is the
    /// cell's first piece, or the whole glyph for a cell of a sized run,
    /// whose ink stays in its cell.
    fn reach_pieces<B: Back>(
        &mut self,
        p: &Pass<'_, '_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        (ncells, first): (usize, double::Piece),
        (held, ink_changed): (bool, bool),
        back: &mut B,
    ) {
        let Some(size) = self.sizes.get(row).copied() else {
            return;
        };
        let cols = p.g.cols;
        for column in 0..ncells.saturating_mul(2) {
            let d = col.saturating_mul(2).saturating_add(column);
            if d >= cols {
                break;
            }
            let stale = self
                .cells
                .get_mut(d, row)
                .is_some_and(|s| s.reach_stale(held));
            if !stale && !ink_changed {
                continue;
            }
            let reach = match first.is_whole() {
                true => Reach::NONE,
                false => {
                    let piece = double::Piece::of(size, column);
                    #[cfg(not(feature = "bidi"))]
                    let reach = self.cell_reach(fc, (held, piece), back);
                    #[cfg(feature = "bidi")]
                    let reach = self.cell_reach(fc, (p, (col, row), ncells), (held, piece), back);
                    reach
                }
            };
            self.reach_to(d, row, 1, (ink_changed, reach));
        }
    }

    /// Forgets every cell's reach and marks every cell changed, so the
    /// frame drawn next reads each cell's reach and draws the whole grid
    /// with it.
    pub(super) fn relearn_reaches(&mut self) {
        self.cells.forget_reaches();
        self.reaches_kept = true;
    }

    /// The cell at `col`, `row`, whose glyph spans `span` cells, now sends
    /// ink as far as `reach`: when its ink changed, as `changed` says, or
    /// its reach did, the neighbours the old reach covered and the ones the
    /// new reach covers are stamped. Damage never chains: a neighbour
    /// stamped here has not changed its own ink.
    pub(super) fn reach_to(
        &mut self,
        col: usize,
        row: usize,
        span: u8,
        (changed, reach): (bool, Reach),
    ) {
        let (old, old_span) = self.cells.set_reach(col, row, (reach, span));
        if (old.is_none() && reach.is_none()) || (!changed && (old, old_span) == (reach, span)) {
            return;
        }
        for (r, n) in [(old, old_span), (reach, span)] {
            if !r.is_none() {
                let b = r.block(col, row, usize::from(n));
                self.cells.stamp_block((b.c0, b.r0), (b.c1, b.r1));
                self.reached.push(b);
            }
        }
    }

    /// How far the ink of piece `piece` of `fc`'s glyph reaches past its
    /// cells as drawn (ADR-1302 D8, ADR-1306 D6): cut to the bound; nowhere
    /// for a letter that blinks, one whose ink its own background hides, a
    /// concealed one, or a cell that draws no text; and only up and down
    /// for the letter the visible cursor stands on, `held`, whatever the
    /// cursor's blink phase. The piece is the whole glyph on a line of the
    /// normal size, and one display cell of it on a line of double size.
    fn cell_reach<B: Back>(
        &mut self,
        fc: &FrameCell<'_>,
        #[cfg(feature = "bidi")] (p, at, ncells): (&Pass<'_, '_>, (usize, usize), usize),
        (held, piece): (bool, double::Piece),
        back: &mut B,
    ) -> Reach {
        let blinks = fc.attrs.contains(Attrs::BLINK) || fc.attrs.contains(Attrs::RAPID_BLINK);
        let steady = !blinks || self.config.blink.style == BlinkStyle::Off;
        let concealed = fc.attrs.contains(Attrs::CONCEAL);
        if !steady || fc.fg == fc.bg || concealed || draws_no_text(fc.glyph) {
            return Reach::NONE;
        }
        let style = crate::variant::of(fc.attrs);
        let width = if fc.width == Width::Wide {
            FontWidth::Wide
        } else {
            FontWidth::Narrow
        };
        let compose = &mut self.compose;
        // the text the cell draws, its joining sides and mirroring, and a
        // lam-alef ligature's two cells (ADR-1004 D6, D7)
        #[cfg(feature = "bidi")]
        let reach = {
            self.chars.clear();
            drawn_text(&self.font, p, at, fc, &mut self.chars);
            if self.chars.is_empty() {
                return Reach::NONE;
            }
            let width = if ncells == 2 { FontWidth::Wide } else { width };
            back.reach(compose, &self.font, (&self.chars, width, style), piece)
                .within(self.bound)
        };
        #[cfg(not(feature = "bidi"))]
        let reach = match fc.glyph {
            Glyph::Empty => return Reach::NONE,
            Glyph::Char(c) => back.reach(compose, &self.font, (&[c], width, style), piece),
            Glyph::Cluster(t) => {
                self.chars.clear();
                self.chars.extend(t.chars());
                back.reach(compose, &self.font, (&self.chars, width, style), piece)
            }
        }
        .within(self.bound);
        if held { reach.upright() } else { reach }
    }
}
