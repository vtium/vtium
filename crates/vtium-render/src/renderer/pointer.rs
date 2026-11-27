//! The pointer's arrow on the grid: where it draws for the pointer's tip,
//! and the cells and border it changes.

use vtium_font::Face;

use crate::compose::{Area, int};
use crate::target::Rect;

use super::layout::Grid;
use super::{PointerShape, Renderer};

/// Where the pointer's arrow draws and the cells it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pointer {
    pub(super) at: Area,
    /// The first column and row of the cells it changes, and the column
    /// and row past the last.
    cells: (usize, usize, usize, usize),
    /// It reaches past the grid into the border.
    outside: bool,
    /// The shape its image shows, so that a new shape in the same place
    /// draws the cells again.
    shape: vtium_term::PointerShape,
}

impl<F: Face> Renderer<F> {
    /// The grid's pixel the pointer's tip at `x`, `y` names, kept inside
    /// the grid; for a glyph pointer, the corner of the cell it is in.
    pub(super) fn pointer_pixel(&self, g: &Grid, (x, y): (u32, u32)) -> Option<(i64, i64)> {
        if g.cols == 0 || g.rows == 0 {
            return None;
        }
        let l = self.layout;
        let last = |n: usize, side: i64| int(n).saturating_mul(side).saturating_sub(1).max(0);
        let (px, py) = (
            int(x).min(last(g.cols, l.cw)),
            int(y).min(last(g.rows, l.ch)),
        );
        let corner = |p: i64, side: i64| p.saturating_sub(p.checked_rem(side).unwrap_or(0));
        let glyph = matches!(self.config.pointer, PointerShape::Glyph(_));
        Some(
            if glyph && self.shape == vtium_term::PointerShape::Default {
                (corner(px, l.cw), corner(py, l.ch))
            } else {
                (px, py)
            },
        )
    }

    /// Where the arrow draws for the pointer's tip at `x`, `y` in the
    /// grid's pixels, and the cells it covers. Its hotspot lands on the
    /// grid's pixel, kept inside the grid; the rest may reach into the
    /// border and past the display, where nothing is drawn.
    pub(super) fn place_pointer(&self, g: &Grid, at: (u32, u32)) -> Option<Pointer> {
        // the pointer's tip stays on the screen's grid, and the arrow covers
        // the cells of the rows above it too
        #[cfg(feature = "scroll-pan")]
        let s = self.layout.screen_of(g);
        #[cfg(feature = "scroll-pan")]
        let (px, py) = self.pointer_pixel(&s, at)?;
        #[cfg(not(feature = "scroll-pan"))]
        let (px, py) = self.pointer_pixel(g, at)?;
        let l = self.layout;
        #[cfg(feature = "scroll-pan")]
        let tip = l.area(&s, px, py, 1, 1);
        #[cfg(not(feature = "scroll-pan"))]
        let tip = l.area(g, px, py, 1, 1);
        let (hx, hy) = self.arrow.hot();
        let at = Area {
            x: tip.x.saturating_sub(int(hx)),
            y: tip.y.saturating_sub(int(hy)),
            w: int(self.arrow.width()),
            h: int(self.arrow.height()),
        };
        let (gw, gh) = (
            int(g.cols).saturating_mul(l.cw),
            int(g.rows).saturating_mul(l.ch),
        );
        let b = l.grid_area(g, at);
        let first = |v: i64, side: i64, n: usize| {
            usize::try_from(v.max(0).checked_div(side).unwrap_or(0))
                .unwrap_or(0)
                .min(n)
        };
        let past = |v: i64, side: i64, n: usize| {
            let end = v.saturating_add(side).saturating_sub(1).max(0);
            usize::try_from(end.checked_div(side).unwrap_or(0))
                .unwrap_or(0)
                .min(n)
        };
        let cells = (
            first(b.x, l.cw, g.cols),
            first(b.y, l.ch, g.rows),
            past(b.x.saturating_add(b.w), l.cw, g.cols),
            past(b.bottom(), l.ch, g.rows),
        );
        let outside = b.x < 0 || b.y < 0 || b.x.saturating_add(b.w) > gw || b.bottom() > gh;
        // where no row of history exists above the screen the spare lines
        // show the border, so an arrow that reaches past the screen's grid
        // draws the border again there
        #[cfg(feature = "scroll-pan")]
        let outside = outside || {
            let up = int(g.above).saturating_mul(l.ch);
            b.y < up
        };
        Some(Pointer {
            at,
            cells,
            outside,
            shape: self.shape,
        })
    }

    /// The cells `area` touches changed on this frame, and the border
    /// inside it.
    pub(super) fn stamp_area(&mut self, g: &Grid, area: Rect) {
        let meets = |r: Rect| {
            r.x < area.x.saturating_add(area.width)
                && area.x < r.x.saturating_add(r.width)
                && r.y < area.y.saturating_add(area.height)
                && area.y < r.y.saturating_add(r.height)
        };
        for row in 0..g.rows {
            for col in 0..g.cols {
                if meets(self.layout.cell_rect(g, col, row)) {
                    self.cells.stamp(col, row);
                }
            }
        }
        self.cells.stamp_border_area(area);
    }

    /// The cells under `p` changed on this frame, and the border inside
    /// its area, clipped to the display, when it reaches into it.
    pub(super) fn stamp_pointer(&mut self, p: &Pointer) {
        let (c0, r0, c1, r1) = p.cells;
        for r in r0..r1 {
            for c in c0..c1 {
                self.cells.stamp(c, r);
            }
        }
        if p.outside {
            let (w, h) = (int(self.layout.width), int(self.layout.height));
            let (x0, y0) = (p.at.x.clamp(0, w), p.at.y.clamp(0, h));
            let x1 = p.at.x.saturating_add(p.at.w).clamp(0, w);
            let side = |v: i64| u32::try_from(v).unwrap_or(0);
            self.cells.stamp_border_area(Rect {
                x: side(x0),
                y: side(y0),
                width: side(x1.saturating_sub(x0)),
                height: side(p.at.bottom().clamp(0, h).saturating_sub(y0)),
            });
        }
    }
}
