//! A program's request for a text area of so many rows and columns, xterm's
//! XTWINOPS 8, `CSI 8 ; height ; width t` (ctlseqs, "Window manipulation
//! (XTWINOPS)"), which the terminal passes on under `allow-window-ops`.
//!
//! The console has no window to resize, so vtium meets the request through
//! the zoom path ([`App::take_size`]). It draws at the largest of the
//! font's zoom steps at which every display of the head holds the grid
//! asked for, and the grid becomes exactly that grid. The pty is resized
//! with it, and the terminal last, so a size report that follows the
//! request (`CSI 18 t`) answers the new size.
//!
//! The zoom steps are the ones a zoom key walks. For a font set whose first
//! source is a bitmap font with strikes, they are the strikes' heights and
//! the whole multiples of the tallest strike. vtium asks the font helper
//! which strike draws at the heights it tries, so a request lands on a
//! strike. All the asks of one request share one wait for the helper's
//! answers ([`App::strike_step`]). When the helper has not answered a
//! height the search needs by then, vtium resizes nothing yet: it carries
//! the search on at each turn that takes the helper's late answers
//! ([`App::resume_climbs`]), and the console that asked takes none of its
//! program's output until the search ends. So a slow helper delays the
//! request, and the grid is the one a fast helper gives.
//!
//! - An omitted side keeps the length it has, and a side of 0 takes the
//!   length the displays hold at the height chosen for the other side, as
//!   ctlseqs says. With both sides 0, the grid fills the displays at the
//!   height in use.
//! - When the displays cannot hold a side even at the font's smallest
//!   height, the grid takes, on that side, the most they hold there, and
//!   vtium draws at the largest step that still holds that grid. ctlseqs
//!   defines 0 as the display's length, so a length past it is met with
//!   the most the display holds. vtium logs a line saying so.
//! - The grid lasts until a zoom, which fills the displays again, or until
//!   the session of the console that asked ends, when the console returns
//!   to the configured size (ADR-1201 D11).

use vtium_render::Orientation;
use vtium_term::{Extent, GridRequest};

use std::time::{Duration, Instant};

use super::{App, after};
use crate::font;
use crate::head::Head;
use crate::helper;
use crate::layout::{self, Zoom};

/// The grid, in columns and rows, that every display of `head` holds at
/// the base height `at`, or `None` when no display holds a cell there.
fn fit_at(head: &Head, at: u32, locale: &str) -> Option<(usize, usize)> {
    let sizes: Vec<(u32, u32)> = head.views.values().map(|v| v.size).collect();
    let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
    let cell = |x: u32| {
        head.face(x, locale).ok().map(|f| {
            let c = f.cell();
            (c.width, c.height)
        })
    };
    layout::layout(&sizes, turned, None, at, head.step, cell).map(|l| l.grid)
}

impl App {
    /// XTWINOPS 8 from head `h`'s shown console asked for the text area
    /// `request`: the head draws at the largest zoom step whose grid holds
    /// it, on exactly that grid where the displays hold it.
    pub(super) fn request_grid(&mut self, h: usize, request: GridRequest) {
        self.drop_climb(h);
        self.choose_font(h);
        let dpi = self.dpi(h);
        let locale = self.locale.clone();
        let Some(head) = self.heads.get(h) else {
            return;
        };
        if head.views.is_empty() {
            log::info!(
                "head {}: a program asked for a text area while the head has no display, so vtium ignores the request",
                head.name()
            );
            return;
        }
        // The sides asked for, in columns and rows; `None` fills.
        let side = |e: Extent, now: usize| match e {
            Extent::Keep => Some(now),
            Extent::Display => None,
            Extent::Cells(n) => Some(n),
        };
        let want = (
            side(request.cols, head.grid.0),
            side(request.rows, head.grid.1),
        );
        if want == (None, None) {
            if head.asked.is_some() {
                let size = head.font_size;
                self.take_size(h, size, None);
            }
            return;
        }
        if want == (Some(head.grid.0), Some(head.grid.1)) {
            return;
        }
        let holds = |grid: (usize, usize), want: (Option<usize>, Option<usize>)| {
            want.0.is_none_or(|c| grid.0 >= c) && want.1.is_none_or(|r| grid.1 >= r)
        };
        let mut at = head.font_size;
        let Some(mut fit) = fit_at(head, at, &locale) else {
            return;
        };
        // Every ask of the font helper that the request makes shares one
        // wait for the answers.
        let end = after((self.clock)(), Duration::from_millis(helper::OWN_CELL_MS));
        // The next height in a direction, and the grid the displays hold
        // there.
        let next = |app: &mut App, at: u32, dir: Zoom| {
            let n = app.size_step(h, at, dir, dpi, end)?;
            let grid = app.heads.get(h).and_then(|head| fit_at(head, n, &locale));
            grid.map(|grid| (n, grid)).ok_or(NoStep::End)
        };
        // Down a step at a time until the grid holds what is asked, or to
        // the smallest step, where a side the displays cannot hold becomes
        // what they hold there.
        let mut target = want;
        while !holds(fit, target) {
            match next(self, at, Zoom::Out) {
                Ok((n, grid)) => (at, fit) = (n, grid),
                Err(NoStep::Owed) => return self.climb_waits(h, request),
                Err(NoStep::End) => {
                    target = (
                        target.0.map(|c| c.min(fit.0)),
                        target.1.map(|r| r.min(fit.1)),
                    );
                    break;
                }
            }
        }
        // Then up a step at a time while the next step still holds it.
        loop {
            match next(self, at, Zoom::In) {
                Ok((n, grid)) if holds(grid, target) => (at, fit) = (n, grid),
                Ok(_) | Err(NoStep::End) => break,
                Err(NoStep::Owed) => return self.climb_waits(h, request),
            }
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let grid = (
            target.0.map_or(fit.0, |c| c.min(fit.0)),
            target.1.map_or(fit.1, |r| r.min(fit.1)),
        );
        let name = head.name().to_owned();
        if target != want {
            let asked = |n: Option<usize>, now: usize| n.unwrap_or(now);
            log::info!(
                "head {name}: a program asked for a text area of {} rows and {} columns, which the displays cannot hold; the text area is {} rows and {} columns",
                asked(want.1, grid.1),
                asked(want.0, grid.0),
                grid.1,
                grid.0
            );
        }
        if self.take_size(h, at, Some(grid)) {
            log::debug!(
                "head {name}: a program asked for a text area of {} rows and {} columns, which vtium draws at {at} pixels high",
                grid.1,
                grid.0
            );
        }
    }

    /// Head `h`'s search for the text area `request` waits for a late
    /// answer from the font helper. The head keeps its size and grid, and
    /// the shown console, which asked, keeps the request and takes none of
    /// its program's output, until [`App::resume_climbs`] ends the search.
    fn climb_waits(&mut self, h: usize, request: GridRequest) {
        if let Some(head) = self.heads.get_mut(h) {
            head.console.term.sizing = Some(request);
        }
    }

    /// Head `h` stops waiting to carry on a search through its font set's
    /// strikes, because a new request, a zoom or a new size takes its
    /// place. The shown console, if its text area request waited, takes
    /// its program's output again. A hidden console keeps the request it
    /// waits on, since the new size is not its own until it is shown.
    pub(super) fn drop_climb(&mut self, h: usize) {
        if let Some(head) = self.heads.get_mut(h) {
            head.zoom_waits = None;
            head.console.term.sizing = None;
        }
    }

    /// Carries on each search through a font set's strikes that waits for
    /// a late answer from the font helper, at each turn that takes the
    /// helper's answers. The search starts again from the head's size, so
    /// it reuses every answer it has and asks only at the heights it has
    /// yet to ask at. It ends once every height it needs has an answer, or
    /// once the helper has failed and cannot answer.
    ///
    /// Only the shown console's text area request goes on. A console that
    /// was hidden while its request waited goes on holding the request
    /// and its program's output, and vtium carries the request on at the
    /// first turn after the console is shown again, as it carries out a
    /// request that a hidden console holds once that console is shown. That
    /// request takes the place of a zoom that still waits.
    ///
    /// References:
    /// - ADR-1401 D1: a hidden console keeps its own grid and cell.
    pub(super) fn resume_climbs(&mut self) {
        for h in 0..self.heads.len() {
            let Some(head) = self.heads.get(h) else {
                continue;
            };
            if let Some(request) = head.console.term.sizing {
                self.request_grid(h, request);
            } else if let Some(dir) = head.zoom_waits {
                self.zoom(h, dir);
            }
        }
    }

    /// The height that head `h` draws at after one zoom step from `at` in
    /// the direction `dir`. [`NoStep::End`] says the font has no step that
    /// way, and [`NoStep::Owed`] that the font helper has yet to answer at
    /// a height the step needs. An XTWINOPS 8 request walks the same steps
    /// as a zoom, so the grid it settles on is one a zoom also reaches.
    /// `end` bounds the wait for the font helper's answers
    /// ([`App::strike_step`]).
    fn size_step(
        &mut self,
        h: usize,
        at: u32,
        dir: Zoom,
        dpi: f64,
        end: Instant,
    ) -> Result<u32, NoStep> {
        let chosen = self.heads.get(h).ok_or(NoStep::End)?.chosen();
        if chosen.struck() {
            return self.strike_step(h, at, dir, end);
        }
        chosen
            .zoom(at, dir, &self.locale, dpi)
            .filter(|&n| if dir == Zoom::In { n > at } else { n < at })
            .ok_or(NoStep::End)
    }

    /// The next height from `from` in the direction `dir` at which head
    /// `h`'s font set draws another cell, for a set whose first source is
    /// a bitmap font with strikes. That height is a strike's own height,
    /// or a whole multiple of the tallest strike above it. The console
    /// reads no font, so it does not know the strikes' heights, and it asks
    /// the font helper which strike draws at a height. Each answer is kept,
    /// so each height is asked once.
    ///
    /// The helper names the tallest strike no taller than the height asked,
    /// so its answers never go down as the height goes up. A step down asks
    /// once, one pixel below `from`. A step up first asks at the next whole
    /// multiple of the strike in use: when the same strike answers there,
    /// no strike lies between, and that multiple is the step. Otherwise a
    /// taller strike lies between, and vtium finds the first height where
    /// the answer changes by asking at 1, 2, 4 and more pixels above
    /// `from`, then halving the gap. Below the shortest strike the helper
    /// refuses, and the search climbs no higher than [`font::ZOOM_IN_MAX`].
    /// A step up from 32 pixels on Terminus asks once, at 64, and a step
    /// from 16 asks three times.
    ///
    /// Every ask of one step, or of one text area request, shares one wait
    /// of [`helper::OWN_CELL_MS`] for the helper's answers, which `end`
    /// marks, so a slow helper holds the loop up for at most that wait. An
    /// ask with no answer by then stops the step with [`NoStep::Owed`]
    /// while the helper still runs, and the caller carries the step on once
    /// the late answer comes ([`App::resume_climbs`]). A helper that could
    /// not answer at all ends the step with [`NoStep::End`].
    ///
    /// References:
    /// - ADR-1204 D2: the cell of the tallest strike no taller than each
    ///   height, and a whole multiple of the tallest strike's cell above
    ///   it.
    pub(super) fn strike_step(
        &mut self,
        h: usize,
        from: u32,
        dir: Zoom,
        end: Instant,
    ) -> Result<u32, NoStep> {
        if dir == Zoom::Out {
            let at = from.checked_sub(1).filter(|&a| a > 0).ok_or(NoStep::End)?;
            return match self.strike_answer(h, at, end) {
                Strike::At(cell) => Ok(font::whole_height(cell, at)),
                Strike::Owed => Err(NoStep::Owed),
                Strike::Short | Strike::Unknown => Err(NoStep::End),
            };
        }
        if from > font::ZOOM_IN_MAX {
            return Err(NoStep::End);
        }
        let base = self.strike_answer(h, from, end);
        // The highest height the search asks at: the next whole multiple
        // of the strike in use, which draws another cell whatever lies
        // between, or with no strike yet the largest height a zoom in
        // starts from.
        let limit = match base {
            Strike::At(cell) => from
                .checked_div(cell.height)
                .and_then(|n| n.checked_add(1))
                .and_then(|n| n.checked_mul(cell.height))
                .ok_or(NoStep::End)?,
            Strike::Short => font::ZOOM_IN_MAX,
            Strike::Owed => return Err(NoStep::Owed),
            Strike::Unknown => return Err(NoStep::End),
        }
        .min(vtium_font::CELL_SIDE_MAX);
        if limit <= from {
            return Err(NoStep::End);
        }
        // Whether the answer at a height differs from the one at `from`.
        // An error stops the step: an answer that has yet to come, a helper
        // that could not answer, or a refusal above a strike, which a
        // helper that names the tallest strike no taller than the height
        // never sends.
        let differs = |app: &mut App, at: u32| match (base, app.strike_answer(h, at, end)) {
            (_, Strike::Owed) => Err(NoStep::Owed),
            (_, Strike::Unknown) | (Strike::At(_), Strike::Short) => Err(NoStep::End),
            (Strike::At(a), Strike::At(b)) => Ok(a != b),
            (Strike::Short, Strike::At(_)) => Ok(true),
            (Strike::Short, Strike::Short) => Ok(false),
            (Strike::Owed | Strike::Unknown, _) => Err(NoStep::End),
        };
        let mut hi = limit;
        if matches!(base, Strike::At(_)) && !differs(self, limit)? {
            return Ok(limit);
        }
        // The first height above `from` whose answer differs lies in
        // (lo, hi]: double the gap from `from` until an answer differs,
        // then halve it.
        let mut lo = from;
        let mut gap = 1u32;
        loop {
            let at = from.saturating_add(gap).min(limit);
            if at >= hi {
                break;
            }
            if differs(self, at)? {
                hi = at;
                break;
            }
            lo = at;
            gap = gap.saturating_mul(2);
        }
        if hi == limit && matches!(base, Strike::Short) && !differs(self, limit)? {
            return Err(NoStep::End);
        }
        // lo < hi throughout, so the gap and the midpoint stay in range.
        while hi.saturating_sub(lo) > 1 {
            let mid = lo.saturating_add(hi.saturating_sub(lo) / 2);
            if differs(self, mid)? {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        match self.strike_answer(h, hi, end) {
            Strike::At(cell) => Some(font::whole_height(cell, hi))
                .filter(|&n| n > from)
                .ok_or(NoStep::End),
            Strike::Owed => Err(NoStep::Owed),
            Strike::Short | Strike::Unknown => Err(NoStep::End),
        }
    }

    /// The strike that head `h`'s font set draws at `at`, asked of the
    /// helper unless an answer is kept, waiting for it until `end`.
    fn strike_answer(&mut self, h: usize, at: u32, end: Instant) -> Strike {
        let wait = end.saturating_duration_since((self.clock)());
        self.own_cell_within(h, at, wait);
        let Some(head) = self.heads.get(h) else {
            return Strike::Unknown;
        };
        if let Some(cell) = head.chosen().strike_at(at) {
            return Strike::At(cell);
        }
        let Some(frit) = head.frit().and_then(|name| self.frits.get(name)) else {
            return Strike::Unknown;
        };
        let running = frit
            .helper
            .as_ref()
            .is_some_and(|helper| helper.failed().is_none());
        match frit.own.get(&at) {
            Some(Err(no)) if no.from_frit => Strike::Short,
            Some(Err(no)) if no.late && running => Strike::Owed,
            _ => Strike::Unknown,
        }
    }
}

/// Why a zoom step from a height finds no next height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoStep {
    /// The font has no step that way, or the helper could not answer.
    End,
    /// The font helper has yet to answer at a height the step needs.
    Owed,
}

/// What the helper says of a height, for a font set whose first source is
/// a bitmap font with strikes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strike {
    /// The cell of the tallest strike no taller than the height.
    At(vtium_font::Cell),
    /// No strike is that short.
    Short,
    /// No answer yet: none came in time, and the helper still runs, so
    /// its late answer is on the way.
    Owed,
    /// No answer, because the helper could not answer.
    Unknown,
}
