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

use super::App;
use crate::head::Head;
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
        let chosen = head.chosen();
        let step = |at: u32, dir: Zoom| {
            chosen
                .zoom(at, dir, &locale, dpi)
                .filter(|&n| if dir == Zoom::In { n > at } else { n < at })
        };
        let mut at = head.font_size;
        let Some(mut fit) = fit_at(head, at, &locale) else {
            return;
        };
        // Down a step at a time until the grid holds what is asked, or to
        // the smallest step, where a side the displays cannot hold becomes
        // what they hold there.
        let mut target = want;
        while !holds(fit, target) {
            match step(at, Zoom::Out).and_then(|n| Some((n, fit_at(head, n, &locale)?))) {
                Some((next, grid)) => (at, fit) = (next, grid),
                None => {
                    target = (
                        target.0.map(|c| c.min(fit.0)),
                        target.1.map(|r| r.min(fit.1)),
                    );
                    break;
                }
            }
        }
        // Then up a step at a time while the next step still holds it.
        while let Some((next, grid)) =
            step(at, Zoom::In).and_then(|n| Some((n, fit_at(head, n, &locale)?)))
        {
            if !holds(grid, target) {
                break;
            }
            (at, fit) = (next, grid);
        }
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
}
