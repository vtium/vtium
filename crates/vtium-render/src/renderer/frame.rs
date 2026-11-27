//! A frame from start to end: [`Renderer::draw`] plans the frame, paints
//! the border and the cells the buffer lacks, and finishes with the frame's
//! damage and figures.

use std::time::Instant;

use vtium_font::Face;
use vtium_term::{Cursor, Frame, Glyph, Width};

use crate::arrow::Over;
#[cfg(not(feature = "scroll-pan"))]
use crate::compose::Area;
#[cfg(feature = "images")]
use crate::compose::int;
use crate::link_row::LinkRow;
use crate::mono::{self, Threshold};
use crate::target::{Rect, Target};

use super::cpu::{Back, Cpu};
use super::jobs::span;
use super::layout::Grid;
use super::pass::{Pass, Plan};
#[cfg(feature = "scroll-pan")]
use super::strip;
use super::{DrawState, Drawn, Renderer, Stats};

impl<F: Face> Renderer<F> {
    /// Draws `frame` into `target`, a buffer `age` frames old: 1 when it
    /// holds the last frame this renderer drew damage for, n when it holds
    /// the frame n back, 0 when its contents are unknown. The cells that
    /// changed in the last `age` frames are drawn, or at age 0, or at an age
    /// past this renderer's history, the whole picture with its border;
    /// another buffer's history is never disturbed.
    ///
    /// `frame` is read since the checkpoint taken after this renderer's
    /// previous `draw` (`Age::NEVER` before the first), whatever buffer that
    /// drew into and whether or not it was swapped: the renderer keeps each
    /// buffer's history itself. An older checkpoint draws more than needed;
    /// a newer one loses changes.
    ///
    /// A frame whose damage is empty changed nothing: it is not swapped,
    /// and does not count as a frame for the ages that follow.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    pub fn draw(
        &mut self,
        frame: &Frame<'_>,
        target: &mut impl Target,
        age: u32,
        state: &DrawState,
    ) -> Drawn<'_> {
        let start = Instant::now();
        #[cfg(feature = "gpu")]
        self.gpu.drew_on_cpu();
        #[cfg(feature = "scroll-pan")]
        self.ring.draw_above();
        let plan = self.plan(frame, age, state);
        #[cfg(feature = "scroll-pan")]
        let (counts, missing) = self.paint_through(&plan, frame, state, target, None);
        #[cfg(not(feature = "scroll-pan"))]
        let (counts, missing) = self.paint_through(&plan, frame, state, target);
        self.finish(&plan, counts, missing, start)
    }

    /// Paints the planned frame into `target` through the pointer's
    /// blend, the color slot and the 1-bit cut. Returns how many cells it
    /// drew and how many rows it scanned, and the characters of each text
    /// it drew as the font's missing glyph. `border`, when given, is set
    /// while the border is drawn.
    pub(super) fn paint_through(
        &mut self,
        plan: &Plan,
        frame: &Frame<'_>,
        state: &DrawState,
        target: &mut impl Target,
        #[cfg(feature = "scroll-pan")] border: Option<&core::cell::Cell<bool>>,
    ) -> ((u32, u32), Vec<char>) {
        let arrow = core::mem::take(&mut self.arrow);
        let mut line = core::mem::take(&mut self.pointer_line);
        let mut mono_line = core::mem::take(&mut self.mono_line);
        let mut target = Threshold::new(target, self.config.depth, &mut mono_line);
        // the color slot, between the pointer's blend and a 1-bit cut
        #[cfg(feature = "filters")]
        let (mut color, mut color_line) =
            (self.color.take(), core::mem::take(&mut self.color_line));
        #[cfg(feature = "filters")]
        let mut target = crate::filter::Colored::new(&mut target, color.as_mut(), &mut color_line);
        let mut out = Over::new(&mut target, plan.pointer.map(|p| (&arrow, p.at)), &mut line);
        // the spare lines blanked or faded, under the pointer's blend
        #[cfg(feature = "scroll-pan")]
        let mut strip_line = core::mem::take(&mut self.strip_line);
        #[cfg(feature = "scroll-pan")]
        let mut out = {
            let border = crate::blend::pixel(mono::border(self.config.depth, state.background));
            let screen = self.layout.screen_of(&plan.grid);
            strip::Strip {
                inner: &mut out,
                cover: strip::Cover::of(&self.layout, &screen, border),
                line: &mut strip_line,
            }
        };
        let reaching = self.compose.cache.reaching();
        if reaching && !self.reaches_kept {
            self.relearn_reaches();
        }
        let mut back = Cpu {
            target: &mut out,
            #[cfg(feature = "scroll-pan")]
            border,
        };
        let mut drawn = self.paint(plan, frame, state, &mut back);
        if !reaching && self.compose.cache.reaching() {
            // the font drew its first glyph whose ink passes its cells, and
            // the cells scanned before it read no reach: the frame is drawn
            // again, whole, with every reach read
            self.relearn_reaches();
            drawn = self.paint(plan, frame, state, &mut back);
        }
        #[cfg(feature = "filters")]
        {
            self.color = color;
            self.color_line = color_line;
        }
        #[cfg(feature = "scroll-pan")]
        {
            self.strip_line = strip_line;
        }
        self.arrow = arrow;
        self.pointer_line = line;
        self.mono_line = mono_line;
        (drawn, self.compose.cache.take_missing())
    }

    /// The first half of a frame: where the grid sits, what the pointer
    /// changed, and which parts of the border the buffer lacks.
    pub(super) fn plan(&mut self, frame: &Frame<'_>, age: u32, state: &DrawState) -> Plan {
        self.compose.cache.begin_frame();
        self.cells.begin(age);
        let d = self.config.decorations;
        let palette = frame.palette();
        self.deco = (d.underline.resolve(palette), d.strike.resolve(palette));
        let grid = self.layout.grid_for(frame);
        // the CPU path draws the rows of history above the screen
        #[cfg(feature = "scroll-pan")]
        let (screen, grid) = (grid, self.with_rows_above(frame, grid));
        // a frame drawn through the ring may move the grid's lines, or rest
        // the window and draw whole
        #[cfg(feature = "scroll-pan")]
        let (rest, scrolled) = self.ring_scroll(frame, &screen);
        #[cfg(not(feature = "scroll-pan"))]
        let rest = false;
        let reset = rest || self.grid != Some(grid) || self.background != Some(state.background);
        if reset {
            self.cells.reset(grid.cols, grid.rows);
            self.sizes.clear();
            self.grid = Some(grid);
            self.background = Some(state.background);
            self.cursor_at = None;
            self.held_at = None;
            self.pointer = None;
            #[cfg(feature = "scroll-pan")]
            self.ring_rest();
        }
        #[cfg(feature = "scroll-pan")]
        if !reset && scrolled != 0 {
            self.scroll_rows(&grid, scrolled);
        }
        #[cfg(feature = "scroll-pan")]
        self.rows_above_exist(&grid, reset);
        let pointer = state.pointer.and_then(|p| self.place_pointer(&grid, p));
        if pointer != self.pointer {
            // it appeared, moved or hid: the cells it left
            // and the cells it covers change, and the border where it
            // reaches past the grid
            for p in [self.pointer, pointer].into_iter().flatten() {
                self.stamp_pointer(&p);
            }
            self.pointer = pointer;
        }
        for area in core::mem::take(&mut self.touched) {
            self.stamp_area(&grid, area);
        }
        #[cfg(feature = "images")]
        self.plan_pictures(frame, &grid);
        let full = self.cells.border_stale();
        let border: Vec<Rect> = if full {
            Vec::new()
        } else {
            self.cells.stale_border_areas().collect()
        };
        Plan {
            grid,
            pointer,
            full,
            border,
            #[cfg(feature = "scroll-pan")]
            drawn: self.drawn_area(&grid),
        }
    }

    /// The second half: the border and the cells the buffer lacks, through
    /// `back`. Returns how many cells it drew and how many rows it scanned.
    pub(super) fn paint(
        &mut self,
        plan: &Plan,
        frame: &Frame<'_>,
        state: &DrawState,
        back: &mut impl Back,
    ) -> (u32, u32) {
        let all = Rect {
            x: 0,
            y: 0,
            width: self.layout.width,
            height: self.layout.height,
        };
        let whole_border = [all];
        let bg = mono::border(self.config.depth, state.background);
        let g = &plan.grid;
        #[cfg(not(feature = "scroll-pan"))]
        let grid = (g.cols > 0 && g.rows > 0).then_some(Area {
            x: g.off_x,
            y: g.off_y,
            w: g.max_x.saturating_sub(g.off_x),
            h: g.max_y.saturating_sub(g.off_y),
        });
        #[cfg(feature = "scroll-pan")]
        let grid = plan.drawn;
        for &r in if plan.full {
            &whole_border[..]
        } else {
            &plan.border[..]
        } {
            back.border(&mut self.compose, (all, grid), bg, r);
        }
        self.draw_cells(g, frame, back, state)
    }

    /// The frame's damage and figures, once it is drawn; `missing` holds
    /// the characters of each text it drew as the font's missing glyph.
    pub(super) fn finish(
        &mut self,
        plan: &Plan,
        (cells, rows): (u32, u32),
        missing: Vec<char>,
        start: Instant,
    ) -> Drawn<'_> {
        #[cfg(feature = "images")]
        self.compose.pictures.end_frame();
        self.changed.clear();
        self.owes_missing = false;
        self.shown_missing_drew(missing);
        let layout = self.layout;
        let grid = plan.grid;
        let whole = plan.full.then_some(Rect {
            x: 0,
            y: 0,
            width: layout.width,
            height: layout.height,
        });
        let counts = self.compose.cache.take_counts();
        // a row of history above the screen lies partly past the display's
        // edge, and its damage is the part on the display
        #[cfg(feature = "scroll-pan")]
        let damage = self.cells.collect(whole, &plan.border, |col, row| {
            let a = layout.cells_area(&grid, col, row, 1);
            layout.on_display(a).unwrap_or_default()
        });
        #[cfg(not(feature = "scroll-pan"))]
        let damage = self.cells.collect(whole, &plan.border, |col, row| {
            layout.cell_rect(&grid, col, row)
        });
        let damage_px = damage.iter().fold(0u64, |n, r| n.saturating_add(r.area()));
        Drawn {
            damage,
            stats: Stats {
                cells,
                rows,
                misses: counts.misses,
                uncached: counts.uncached,
                damage_px,
                cpu: start.elapsed(),
            },
        }
    }

    /// The frame's pictures: the cells of each one that moved or changed
    /// since the last frame are stamped, and each one's place on the
    /// display is planned for the rows to draw.
    #[cfg(feature = "images")]
    fn plan_pictures(&mut self, frame: &Frame<'_>, g: &Grid) {
        #[cfg(feature = "scroll-pan")]
        let (placements, above) = Self::placements_above(frame, g);
        #[cfg(not(feature = "scroll-pan"))]
        let (placements, above) = (frame.placements(), 0);
        let cells = &mut self.cells;
        self.compose
            .pictures
            .damage(placements, (g.cols, g.rows), above, |c, r| {
                cells.stamp(c, r)
            });
        let l = self.layout;
        let grid_px = (
            int(g.cols).saturating_mul(l.cw),
            int(g.rows).saturating_mul(l.ch),
        );
        let cell = (
            u32::try_from(l.cw).unwrap_or(1),
            u32::try_from(l.ch).unwrap_or(1),
        );
        let g = *g;
        let pictures = &mut self.compose.pictures;
        pictures.display(l.width, l.height, l.orientation);
        pictures.plan(placements, cell, grid_px, g.rows, above, |a| {
            l.area(&g, a.x, a.y, a.w, a.h)
        });
    }

    /// Where the cursor draws: the leading cell of a wide character whose
    /// trailing half it sits on. The overlay row's cells are narrow.
    fn cursor_cell(frame: &Frame<'_>, link_row: Option<LinkRow<'_>>, c: &Cursor) -> (usize, usize) {
        let width_at = |col: usize| {
            let covered = link_row.is_some_and(|o| o.covers(col, c.row));
            frame
                .cell(c.row, col)
                .map(|fc| if covered { Width::Narrow } else { fc.width })
        };
        let before = c.col.checked_sub(1).and_then(width_at);
        let tail = width_at(c.col).is_some_and(|w| span(w, before, || None) == 0);
        match c.col.checked_sub(1) {
            Some(prev) if tail => (prev, c.row),
            _ => (c.col, c.row),
        }
    }

    /// Draws the cells this buffer lacks; returns how many, and how many
    /// rows it scanned for them.
    fn draw_cells(
        &mut self,
        g: &Grid,
        frame: &Frame<'_>,
        back: &mut impl Back,
        state: &DrawState,
    ) -> (u32, u32) {
        let cursor = frame.cursor();
        // the overlay row and the cursor stand on the screen's rows, which
        // the grid counts from the rows of history above them
        #[cfg(feature = "scroll-pan")]
        let (rows, above) = (g.rows.saturating_sub(g.above), g.above);
        #[cfg(feature = "scroll-pan")]
        let link_row = LinkRow::new(frame, g.cols, rows);
        #[cfg(not(feature = "scroll-pan"))]
        let link_row = LinkRow::new(frame, g.cols, g.rows);
        // the cursor is not drawn under the overlay row
        let held = cursor
            .visible
            .then(|| Self::cursor_cell(frame, link_row, &cursor))
            .filter(|&(c, r)| !link_row.is_some_and(|o| o.covers(c, r)));
        #[cfg(feature = "scroll-pan")]
        let held = held.map(|(c, r)| (c, r.saturating_add(above)));
        let at = held.filter(|_| state.cursor_on);
        let phases = (state.blink_hidden, state.rapid_hidden);
        let blink_toggled = phases != self.blink_hidden;
        self.blink_hidden = phases;
        let last_cursor = core::mem::replace(&mut self.cursor_at, at);
        let last_held = core::mem::replace(&mut self.held_at, held);
        let mut pass = Pass {
            g: *g,
            frame,
            state,
            cursor,
            at,
            at_pair: None,
            at_run: None,
            held,
            plain: core::cell::OnceCell::new(),
            link_row,
            #[cfg(feature = "bidi")]
            ligatures: self.config.ligatures,
            #[cfg(feature = "bidi")]
            runs: core::cell::RefCell::new(core::mem::take(&mut self.runs)),
            #[cfg(feature = "scroll-pan")]
            above,
        };
        pass.at_pair = pass.pair_cells(at);
        pass.at_run = pass.run_block(at);
        // the rows that draw the cursor, or drew it last frame, as the
        // other line of a pair of double-height lines
        let pairs = [at, last_cursor, held, last_held]
            .map(|c| pass.pair_cells(c).map(|(row, cols)| (cols.start, row)));
        // and as the other rows of a sized run
        let run_rows =
            [at, last_cursor, held, last_held].map(|c| pass.run_block(c).map(|(rows, _)| rows));
        self.resize_rows(&pass);
        let on = |c: Option<(usize, usize)>, row: usize| c.is_some_and(|(_, r)| r == row);
        let (mut drawn, mut scanned) = (0u32, 0u32);
        for row in 0..g.rows {
            #[cfg(feature = "scroll-pan")]
            let scan = self.cells.row_stale(row)
                || self.row_shows_changed(&pass, row, g.cols)
                || link_row.is_some_and(|o| {
                    pass.link_on(row) && o.chars().any(|c| self.shows_changed(&Glyph::Char(c)))
                })
                || blink_toggled
                || self.row_changed(&pass, row)
                || [at, last_cursor, held, last_held]
                    .into_iter()
                    .chain(pairs)
                    .any(|c| on(c, row))
                || run_rows.iter().flatten().any(|rows| rows.contains(&row));
            #[cfg(not(feature = "scroll-pan"))]
            let scan = self.cells.row_stale(row)
                || self.row_shows_changed(frame, row, g.cols)
                || link_row.is_some_and(|o| {
                    o.row() == row && o.chars().any(|c| self.shows_changed(&Glyph::Char(c)))
                })
                || blink_toggled
                || self.row_changed(frame, row)
                || [at, last_cursor, held, last_held]
                    .into_iter()
                    .chain(pairs)
                    .any(|c| on(c, row))
                || run_rows.iter().flatten().any(|rows| rows.contains(&row));
            if scan {
                scanned = scanned.saturating_add(1);
                drawn = drawn.saturating_add(self.draw_row(&pass, row, back));
            }
        }
        #[cfg(feature = "bidi")]
        {
            self.runs = pass.runs.into_inner();
            self.runs.clear();
        }
        (drawn, scanned)
    }
}
