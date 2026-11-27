//! Each row's drawing: the scan of a row, its cells' jobs, the rows of
//! double size, the jobs cut to the display, and the neighbours whose ink
//! each job receives.

use vtium_font::{Face, Width as FontWidth};
use vtium_term::{CursorShape, FrameCell, Glyph, LineSize, Width};

#[cfg(feature = "scroll-pan")]
use crate::compose::Area;
use crate::compose::{Bar, Job, Source, int};
use crate::mono::{self, Depth};
use crate::reach::Reach;

use super::cpu::Back;
use super::jobs::{blink_off, next_width, span};
#[cfg(feature = "bidi")]
use super::jobs::{drawn_text, paired};
use super::layout::Grid;
use super::pass::{Pass, Place};
use super::{Renderer, double};

impl<F: Face> Renderer<F> {
    /// Scans one row, updates what each cell last drew, and draws the cells
    /// some buffer lacks; returns how many.
    ///
    /// A cell's ink can damage cells the scan has passed: the cells before
    /// it in its row, and the cells of the rows above, which are already
    /// drawn (ADR-1306 D5). Those cells are drawn once the row's scan is
    /// done: the ones in this row with the row, and the ones above in a
    /// second pass over each of those rows, so a screen whose ink stays in
    /// its cells is scanned and drawn in one pass, as before.
    pub(super) fn draw_row<B: Back>(&mut self, p: &Pass<'_, '_>, row: usize, back: &mut B) -> u32 {
        match self.sizes.get(row) {
            Some(&size) if size != LineSize::Single => {
                return self.draw_doubled_row(p, (row, size), back);
            }
            _ => {}
        }
        let g = &p.g;
        self.compose.jobs.clear();
        self.compose.text.clear();
        self.compose.sources.clear();
        self.compose.received.clear();
        self.reached.clear();
        // the reaches are read only where received ink is drawn, and once
        // the font has drawn a glyph whose ink passes its cells
        let reads = B::RECEIVES && back.reaching(&self.compose);
        self.reaches_kept &= reads;
        let mut count = 0u32;
        let mut before = None;
        #[cfg(feature = "scroll-pan")]
        let runs = !p.sized_runs(row).is_empty();
        #[cfg(not(feature = "scroll-pan"))]
        let runs = !p.frame.sized_runs(row).is_empty();
        self.row_runs = runs;
        for (here, fc) in p.cells(row).take(g.cols).enumerate() {
            let ncells = span(fc.width, before, || next_width(p, row, here, g.cols));
            #[cfg(feature = "bidi")]
            let ncells = paired(p, (here, row), &fc).unwrap_or(ncells);
            // each cell of a sized run draws its own piece of the run
            let ncells = match runs && p.run_at(row, here).is_some() {
                true => 1,
                false => ncells,
            };
            before = Some(fc.width);
            if !self.scan_cell(p, (here, row), &fc, (ncells, reads), back) {
                continue;
            }
            #[cfg(feature = "scroll-pan")]
            if p.hidden(row) {
                continue;
            }
            count = count.saturating_add(self.push_cell(p, (here, row), &fc, ncells));
        }
        // no row lies above the hidden row for its ink to reach
        #[cfg(feature = "scroll-pan")]
        if p.hidden(row) {
            return count;
        }
        // the cells this row's ink reached once the scan had passed them
        let reached = core::mem::take(&mut self.reached);
        let meets = |b: &crate::reach::Block, r: usize| b.r0 <= int(r) && b.r1 >= int(r);
        for b in reached.iter().filter(|b| meets(b, row)) {
            count = count.saturating_add(self.push_block(p, row, (b.c0, b.c1)));
        }
        self.finish_row(p, row, back);
        count = count.saturating_add(self.draw_reached_above(p, row, &reached, back));
        self.reached = reached;
        count
    }

    /// Draws again the cells of the rows above `row`, already drawn, that
    /// the blocks `reached` cover: the cells that ink from `row` reached
    /// once the scan had passed them. The rows are drawn top to bottom. A
    /// row of double size receives no ink, so it is not drawn again.
    /// Returns how many cells it drew that were not drawn before on this
    /// frame.
    fn draw_reached_above<B: Back>(
        &mut self,
        p: &Pass<'_, '_>,
        row: usize,
        reached: &[crate::reach::Block],
        back: &mut B,
    ) -> u32 {
        let meets = |b: &crate::reach::Block, r: usize| b.r0 <= int(r) && b.r1 >= int(r);
        let mut count = 0u32;
        for above in row.saturating_sub(usize::from(self.bound.rows))..row {
            if self
                .sizes
                .get(above)
                .is_some_and(|&s| s != LineSize::Single)
            {
                continue;
            }
            #[cfg(feature = "scroll-pan")]
            if p.hidden(above) {
                continue;
            }
            let mut first = true;
            #[cfg(feature = "scroll-pan")]
            {
                self.row_runs = !p.sized_runs(above).is_empty();
            }
            #[cfg(not(feature = "scroll-pan"))]
            {
                self.row_runs = !p.frame.sized_runs(above).is_empty();
            }
            for b in reached.iter().filter(|b| meets(b, above)) {
                if core::mem::take(&mut first) {
                    self.compose.jobs.clear();
                    self.compose.text.clear();
                    self.compose.sources.clear();
                    self.compose.received.clear();
                }
                count = count.saturating_add(self.push_block(p, above, (b.c0, b.c1)));
            }
            if !first {
                self.finish_row(p, above, back);
            }
        }
        count
    }

    /// Pushes the jobs of the cell `fc` at `col`, `row`, drawing over
    /// `ncells` cells, when this buffer lacks it; returns whether it pushed
    /// a cell not drawn before on this frame.
    #[inline]
    fn push_cell(
        &mut self,
        p: &Pass<'_, '_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        ncells: usize,
    ) -> u32 {
        if !self.cells.stale_cells(col, row, ncells) {
            return 0;
        }
        let new = self.cells.mark_drawn(col, row, ncells);
        self.push_cell_jobs(p, (col, row), fc, ncells);
        u32::from(new)
    }

    /// Pushes the jobs of the cell `fc` at `col`, `row`, drawing over
    /// `ncells` cells.
    pub(super) fn push_cell_jobs(
        &mut self,
        p: &Pass<'_, '_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        ncells: usize,
    ) {
        let is_cursor = p.shows_cursor(col, row);
        let off = blink_off(fc, is_cursor, p.state, self.config.blink);
        // a cell of a sized run draws as a narrow cell that holds the
        // run's text, and its job draws its piece of the run's block
        let run = match self.row_runs {
            true => p.run_piece(row, col),
            false => None,
        };
        let own = fc;
        let in_run;
        let fc = match run {
            Some((_, first)) => {
                in_run = FrameCell {
                    glyph: first.glyph,
                    width: Width::Narrow,
                    ..*fc
                };
                &in_run
            }
            None => fc,
        };
        #[cfg(feature = "bidi")]
        let mut drawn = core::mem::take(&mut self.drawn);
        #[cfg(feature = "bidi")]
        {
            drawn.clear();
            match (run, fc.glyph) {
                (None, _) => drawn_text(&self.font, p, (col, row), fc, &mut drawn),
                (Some(_), Glyph::Char(c)) => drawn.push(c),
                (Some(_), Glyph::Cluster(t)) => drawn.extend(t.chars()),
                (Some(_), Glyph::Empty) => {}
            }
        }
        let lit = self.one_bit_lit(p, own, row, col);
        let jobs = self.compose.jobs.len();
        // on a run, a block cursor covers every cell and a bar stands along
        // the block's left edge. An underline cursor is the cell's
        // underline, which scales with the run, so every cell of the run
        // draws its piece of it, and the scaled line lies under the block's
        // bottom row whichever of the block's rows its pixels fall in.
        let draws_cursor = is_cursor
            && run.is_none_or(|(piece, _)| match p.cursor.shape {
                CursorShape::Block | CursorShape::Underline => true,
                CursorShape::Bar => piece.column == 0,
            });
        self.push_jobs(
            &p.g,
            fc,
            Place { col, row, ncells },
            draws_cursor.then_some(&p.cursor),
            off,
            lit,
            #[cfg(feature = "bidi")]
            &drawn,
        );
        if let Some((piece, _)) = run {
            let scale = i64::from(piece.size.scale);
            for job in self.compose.jobs.get_mut(jobs..).unwrap_or_default() {
                job.piece = double::Piece::of_run(piece);
                // the decorations scale with the run's text
                for (line, _) in job.underline.into_iter().chain(job.over) {
                    self.compose.sprites.prepare(line, job.piece);
                }
                // the bar scales with the run's glyph
                job.bar = job.bar.map(|b| {
                    let w = b.w.saturating_mul(scale);
                    Bar {
                        area: self.layout.bar_area(&p.g, (col, row, 1), w, b.right),
                        w,
                        ..b
                    }
                });
            }
        }
        #[cfg(feature = "bidi")]
        {
            self.drawn = drawn;
        }
    }

    /// Pushes the jobs of the cells of `row` from column `c0` to `c1` that
    /// this buffer lacks and that no job holds yet; returns how many it
    /// pushed that were not drawn before on this frame.
    fn push_block(&mut self, p: &Pass<'_, '_>, row: usize, (c0, c1): (i64, i64)) -> u32 {
        let cols = p.g.cols;
        let first = usize::try_from(c0.max(0)).unwrap_or(0);
        let last = usize::try_from(c1.max(0))
            .unwrap_or(0)
            .min(cols.saturating_sub(1));
        #[cfg(feature = "scroll-pan")]
        let line = p.line(row);
        #[cfg(feature = "scroll-pan")]
        let covered = |c: usize| p.link_covers(c, row);
        #[cfg(not(feature = "scroll-pan"))]
        let line = p.frame.line(row);
        #[cfg(not(feature = "scroll-pan"))]
        let covered = |c: usize| p.link_row.is_some_and(|o| o.covers(c, row));
        let width = |c: usize| {
            line.filter(|_| c < cols).map(|l| {
                if covered(c) {
                    Width::Narrow
                } else {
                    l.cell(c).width()
                }
            })
        };
        let mut count = 0u32;
        let mut col = first;
        while col <= last && col < cols {
            let before = col.checked_sub(1).and_then(width);
            let Some(w) = width(col) else { break };
            let (lead, n) = match span(w, before, || width(col.saturating_add(1))) {
                0 => (col.saturating_sub(1), 2),
                n => (col, n),
            };
            // a lam-alef ligature is drawn whole by its alef, on the left
            #[cfg(feature = "bidi")]
            let (lead, n) = match p.cell(row, col).and_then(|fc| paired(p, (col, row), &fc)) {
                Some(0) => (col.saturating_sub(1), 2),
                Some(n) => (col, n),
                None => (lead, n),
            };
            let end = lead.saturating_add(n);
            let held = self
                .compose
                .jobs
                .iter()
                .any(|j| lead < j.posx.saturating_add(j.ncells) && j.posx < end);
            if let Some(fc) = p.cell(row, lead).filter(|_| !held) {
                count = count.saturating_add(self.push_cell(p, (lead, row), &fc, n));
            }
            col = lead.saturating_add(n);
        }
        count
    }

    /// Scans and draws `row`, a row of double size `size`: each of its
    /// cells draws across two cells of the display, and a wide character
    /// across four. Returns how many cells it drew.
    ///
    /// The row is drawn whole whenever the buffer lacks any of its cells.
    /// The cells' own stamps, the pointer's and the pictures' count in the
    /// grid's columns, which on this row are not the display's, so drawing
    /// the whole row gives every buffer what it lacks without mapping one
    /// onto the other. Only the cells whose left half lies in the grid
    /// draw; the rest of the line lies past the grid's right edge.
    ///
    /// The row sends its glyphs' ink past the line into the rows above and
    /// below, and receives none. Each display cell keeps the reach of the
    /// piece it draws, by its display column ([`Renderer::reach_pieces`]),
    /// so a row that receives the ink finds each piece as a one-cell glyph
    /// ([`Renderer::sent_reach`]). A piece whose ink changed stamps the
    /// cells its old and new reach cover, and the cells of the rows above,
    /// already drawn, are drawn again once the row is drawn.
    fn draw_doubled_row<B: Back>(
        &mut self,
        p: &Pass<'_, '_>,
        (row, size): (usize, LineSize),
        back: &mut B,
    ) -> u32 {
        let g = &p.g;
        let mut doubled = core::mem::take(&mut self.doubled);
        doubled.clear();
        self.reached.clear();
        // the reaches are read only where received ink is drawn, and once
        // the font has drawn a glyph whose ink passes its cells
        let reads = B::RECEIVES && back.reaching(&self.compose);
        self.reaches_kept &= reads;
        let mut before = None;
        #[cfg(feature = "scroll-pan")]
        let runs = !p.sized_runs(row).is_empty();
        #[cfg(not(feature = "scroll-pan"))]
        let runs = !p.frame.sized_runs(row).is_empty();
        self.row_runs = runs;
        for (here, fc) in p.cells(row).take(g.cols).enumerate() {
            let ncells = span(fc.width, before, || next_width(p, row, here, g.cols));
            #[cfg(feature = "bidi")]
            let ncells = paired(p, (here, row), &fc).unwrap_or(ncells);
            // each cell of a sized run draws its own piece of the run
            let ncells = match runs && p.run_at(row, here).is_some() {
                true => 1,
                false => ncells,
            };
            before = Some(fc.width);
            if self.scan_cell(p, (here, row), &fc, (ncells, reads), back)
                && here.saturating_mul(2) < g.cols
            {
                doubled.push((here, ncells));
            }
        }
        let mut count = 0u32;
        // the hidden row above the spare lines draws nothing
        #[cfg(feature = "scroll-pan")]
        let shown = !p.hidden(row);
        #[cfg(not(feature = "scroll-pan"))]
        let shown = true;
        if shown && self.cells.stale_cells(0, row, g.cols) {
            self.compose.jobs.clear();
            self.compose.text.clear();
            self.compose.sources.clear();
            self.compose.received.clear();
            for &(col, ncells) in &doubled {
                let Some(fc) = p.cell(row, col) else { continue };
                let new = self.cells.mark_drawn(col, row, ncells);
                count = count.saturating_add(u32::from(new));
                self.push_cell_jobs(p, (col, row), &fc, ncells);
            }
            // the display cells of the cells past the middle of the line
            // are drawn by the cells before it
            self.cells.mark_drawn(0, row, g.cols);
            self.double_jobs(g, row, size);
            #[cfg(feature = "scroll-pan")]
            self.cut_to_display(p, row);
            #[cfg(feature = "images")]
            {
                self.compose.pictures.row = row;
            }
            back.row(&mut self.compose, &self.font, self.layout.strip(), row);
        }
        self.doubled = doubled;
        let reached = core::mem::take(&mut self.reached);
        count = count.saturating_add(self.draw_reached_above(p, row, &reached, back));
        self.reached = reached;
        count
    }

    /// Turns the jobs of `row`, pushed in the grid's columns, into the jobs
    /// of its doubled cells: one job for each display cell, each drawing
    /// its column's piece of the doubled glyph and decorations
    /// ([`double`]). A job's colors, text and lines carry over, so the
    /// cursor, the selection, colors and decorations follow the line's
    /// size. Display cells past the grid's right edge are cut.
    fn double_jobs(&mut self, g: &Grid, row: usize, size: LineSize) {
        let jobs = core::mem::take(&mut self.compose.jobs);
        let l = &self.layout;
        for job in &jobs {
            let first = job.posx.saturating_mul(2);
            // the bar doubles with its character: twice as thick, at the
            // same edge of the doubled character, so only the display cell
            // at that edge draws it
            let bar = job.bar.map(|b| {
                let w = b.w.saturating_mul(2);
                let cells = (first, row, job.ncells.saturating_mul(2));
                Bar {
                    area: l.bar_area(g, cells, w, b.right),
                    w,
                    ..b
                }
            });
            for column in 0..job.ncells.saturating_mul(2) {
                let col = first.saturating_add(column);
                if col >= g.cols {
                    break;
                }
                let r = l.cells_area(g, col, row, 1);
                self.compose.jobs.push(Job {
                    posx: col,
                    ncells: 1,
                    r,
                    g: r,
                    tiles: [r, l.cells_area(g, col.saturating_add(1), row, 1)],
                    piece: job.piece.on_line(size, column),
                    bar,
                    ..*job
                });
            }
        }
        self.compose.jobs.sort_unstable_by_key(|j| j.posx);
    }

    /// Draws the row's jobs: the cells between its layered jobs join them,
    /// each job gathers the ink its neighbours send it, and `back` draws
    /// them.
    fn finish_row<B: Back>(&mut self, p: &Pass<'_, '_>, row: usize, back: &mut B) {
        self.widen_to_span(p, row);
        // the ranges of received ink follow the jobs' places, so the jobs
        // take their order before they are gathered
        let jobs = &mut self.compose.jobs;
        if !jobs.is_sorted_by_key(|j| j.posx) {
            jobs.sort_unstable_by_key(|j| j.posx);
        }
        if B::RECEIVES {
            self.gather(p, row);
        }
        #[cfg(feature = "scroll-pan")]
        self.cut_to_display(p, row);
        #[cfg(feature = "images")]
        {
            self.compose.pictures.row = row;
        }
        back.row(&mut self.compose, &self.font, self.layout.strip(), row);
    }

    /// Cuts the jobs of the grid's row `row` to the display, when it is a
    /// row of history above the screen: such a row shows only its part in
    /// the spare lines, and the rest lies past the display's edge. Every
    /// row the spare lines show keeps a part on the display, so no job is
    /// left empty.
    #[cfg(feature = "scroll-pan")]
    fn cut_to_display(&mut self, p: &Pass<'_, '_>, row: usize) {
        if p.screen_row(row).is_some() {
            return;
        }
        let l = self.layout;
        for job in &mut self.compose.jobs {
            if let Some(r) = l.on_display(job.r) {
                job.r = Area {
                    x: int(r.x),
                    y: int(r.y),
                    w: int(r.width),
                    h: int(r.height),
                };
            }
        }
    }

    /// Finds, for each of the row's jobs, the neighbours whose glyphs send
    /// ink into its cells, from their reach, and lists them in the order
    /// ADR-1306 D4 draws them: the row above, the row below, then the cells
    /// beside it, farthest first.
    fn gather(&mut self, p: &Pass<'_, '_>, row: usize) {
        let rows = usize::from(self.bound.rows);
        let side = usize::from(self.bound.side);
        let (first, last) = (row.saturating_sub(rows), row.saturating_add(rows));
        if !self.cells.rows_reach(first, last) {
            return;
        }
        let (cols, grid_rows) = self.cells.size();
        let last = last.min(grid_rows.saturating_sub(1));
        let mut found: Vec<(u8, i64, usize, usize, usize)> = Vec::new();
        for j in 0..self.compose.jobs.len() {
            let Some(job) = self.compose.jobs.get(j).copied() else {
                continue;
            };
            let (a, n) = (job.posx, job.ncells.max(1));
            let end = a.saturating_add(n);
            found.clear();
            for r in first..=last {
                // a wide glyph's first cell may lie one further left
                let k0 = a.saturating_sub(side.saturating_add(1));
                let k1 = end.saturating_add(side).min(cols);
                for k in k0..k1 {
                    let (reach, span) = self.sent_reach(k, r);
                    let span = usize::from(span);
                    if reach.is_none() || span == 0 {
                        continue;
                    }
                    // a glyph's own cells take its ink as its own glyph
                    if r == row && k < end && k.saturating_add(span) > a {
                        continue;
                    }
                    if !reach.block(k, r, span).meets(a, row, n) {
                        continue;
                    }
                    // ADR-1306 D4: the row above, the row below, then beside
                    // the cell, farthest first and the cell before it first
                    let key = match r.cmp(&row) {
                        core::cmp::Ordering::Less => (0, int(k)),
                        core::cmp::Ordering::Greater => (1, int(k)),
                        core::cmp::Ordering::Equal => {
                            let before = k < a;
                            let gap = if before {
                                a.saturating_sub(k.saturating_add(span))
                            } else {
                                k.saturating_sub(end)
                            };
                            (
                                2,
                                int(gap)
                                    .saturating_neg()
                                    .saturating_mul(2)
                                    .saturating_add(i64::from(!before)),
                            )
                        }
                    };
                    found.push((key.0, key.1, k, r, span));
                }
            }
            if found.is_empty() {
                continue;
            }
            found.sort_unstable();
            // a block cursor or the selection marks the job's cell
            let block = matches!(p.cursor.shape, CursorShape::Block);
            let marked =
                (block && p.shows_cursor(a, row)) || p.cell(row, a).is_some_and(|c| c.selected);
            let from = self.compose.sources.len();
            for &(_, _, k, r, span) in &found {
                if let Some(src) = self.source(p, (&job, marked), (k, r, span)) {
                    self.compose.sources.push(src);
                }
            }
            let to = self.compose.sources.len();
            let jobs = self.compose.jobs.len();
            let received = &mut self.compose.received;
            if received.len() < jobs {
                received.resize(jobs, (0, 0));
            }
            if let Some(range) = received.get_mut(j) {
                *range = (from, to);
            }
        }
    }

    /// The reach of the cell at `k`, `r` and how many cells the glyph it
    /// counts from spans. On a row of double size, the cell is a display
    /// cell that sends the ink of the one-cell piece it draws.
    pub(crate) fn sent_reach(&self, k: usize, r: usize) -> (Reach, u8) {
        let Some(s) = self.cells.get(k, r) else {
            return (Reach::NONE, 0);
        };
        match self.sizes.get(r) {
            Some(&size) if size != LineSize::Single => (s.reach(), 1),
            _ => (s.reach(), s.span()),
        }
    }

    /// The cell of row `r` that draws display cell `k` of a row of double
    /// size `size`, and the piece of its glyph that the display cell
    /// draws: the cell at column `k / 2`, or the wide character it is the
    /// trailing half of.
    fn doubled_source(&self, (k, r): (usize, usize), size: LineSize) -> (usize, double::Piece) {
        let half = k / 2;
        let lead = match self.cells.get(half, r).map(|s| s.span()) {
            Some(0) => half.saturating_sub(1),
            _ => half,
        };
        let column = k.saturating_sub(lead.saturating_mul(2));
        (lead, double::Piece::of(size, column))
    }

    /// The glyph of the cell at `k`, `r`, spanning `span` cells, as a source
    /// of ink for `job` (ADR-1306 D6): in `job`'s mark color when a block
    /// cursor or the selection marks `job`'s cell, as `marked` says; else, at 1 bit, the
    /// opposite of `job`'s background; else in its own foreground, as it
    /// draws unselected. On a row of double size, `k` is a display cell,
    /// and the source is the one-cell piece of the doubled glyph it draws.
    fn source(
        &mut self,
        p: &Pass<'_, '_>,
        (job, marked): (&Job, bool),
        (k, r, span): (usize, usize, usize),
    ) -> Option<Source> {
        let (lead, piece) = match self.sizes.get(r) {
            Some(&size) if size != LineSize::Single => self.doubled_source((k, r), size),
            _ => (k, double::Piece::WHOLE),
        };
        let fc = p.cell(r, lead)?;
        let own = if self.config.depth == Depth::One {
            mono::opposite(job.bg)
        } else if fc.selected {
            plain_cell!(p, r, k).map_or(fc.fg, |c| c.fg)
        } else {
            fc.fg
        };
        let color = if marked { job.fg } else { own };
        let text = &mut self.compose.text;
        let start = text.len();
        match fc.glyph {
            Glyph::Empty => return None,
            Glyph::Char(c) => text.push(c),
            Glyph::Cluster(t) => text.extend(t.chars()),
        }
        let style = crate::variant::of(fc.attrs);
        let (width, cells) = match (fc.width, piece.is_whole()) {
            (Width::Wide, true) => (FontWidth::Wide, 2),
            (Width::Wide, false) => (FontWidth::Wide, 1),
            _ => (FontWidth::Narrow, 1),
        };
        let reach = self.cells.get(k, r)?.reach();
        let b = reach.block(k, r, span);
        let l = &self.layout;
        let (cw, ch) = (l.cw, l.ch);
        let g = &p.g;
        Some(Source {
            text: (start, text.len().saturating_sub(start), width, style),
            g: l.cells_area(g, k, r, cells),
            clip: l.area(
                g,
                b.c0.saturating_mul(cw),
                b.r0.saturating_mul(ch),
                b.c1.saturating_add(1)
                    .saturating_sub(b.c0)
                    .saturating_mul(cw),
                b.r1.saturating_add(1)
                    .saturating_sub(b.r0)
                    .saturating_mul(ch),
            ),
            color,
            cell: (k, r),
            own,
            marked,
            piece,
        })
    }
}
