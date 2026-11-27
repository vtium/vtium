//! Where the grid sits on the display: [`Grid`], the grid's place, and
//! [`Layout`], the display's size, the cell's and the orientation, which
//! turn the grid's rows, cells and pixels into display areas.

use vtium_term::Frame;

use crate::bounds::ROW_SCRATCH_MAX;
use crate::compose::{Area, int};
use crate::target::{Orientation, Rect};

use super::Placement;

/// Where the grid sits on the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Grid {
    pub(super) cols: usize,
    pub(super) rows: usize,
    pub(super) off_x: i64,
    pub(super) off_y: i64,
    pub(super) max_x: i64,
    pub(super) max_y: i64,
    /// The rows of history the CPU path draws above the screen's first
    /// row, which `rows` counts with the screen's: the rows the spare lines
    /// above the grid show, and one more above them, which only sends its
    /// ink down. The screen's row `y` is the grid's row `y + above`.
    ///
    /// References:
    /// - ADR-1310 D10: the spare lines show the bottom of the rows above, which take ink from their real neighbours.
    #[cfg(feature = "scroll-pan")]
    pub(super) above: usize,
}

/// The display's size, the cell's and the orientation: where the grid's
/// pixels land on the display.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    pub(super) orientation: Orientation,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) cw: i64,
    pub(super) ch: i64,
    pub(super) max_cols: usize,
    pub(super) max_rows: usize,
    pub(super) placement: Placement,
}

impl Layout {
    /// The row scratch's line length and lines.
    pub(super) fn strip(&self) -> (usize, usize) {
        let (line, lines) = if self.orientation.turned() {
            (self.ch, int(self.height))
        } else {
            (int(self.width), self.ch)
        };
        let line = usize::try_from(line).unwrap_or(0).max(1);
        let lines = usize::try_from(lines).unwrap_or(0);
        let fit = (ROW_SCRATCH_MAX / 4).checked_div(line).unwrap_or(0).max(1);
        (line, lines.min(fit))
    }

    /// Where the grid of `frame` sits on the display.
    pub(super) fn grid_for(&self, frame: &Frame<'_>) -> Grid {
        self.grid_of(frame.cols(), frame.rows())
    }

    /// Where a grid of `cols` by `rows` cells sits on the display.
    ///
    /// Placed at the bottom, the grid's last row touches the text's bottom
    /// edge, so the spare pixel lines, left when the display's length along
    /// the rows is not a whole number of rows, lie above the grid's first
    /// row. The text's bottom is the display's bottom upright, its top
    /// upside down, its left turned right and its right turned left. Across
    /// the rows the grid stays centred in the display's own pixels, and an
    /// odd spare pixel goes to the right or the bottom.
    ///
    /// Centred, the grid is centred on both axes in the display's own
    /// pixels, so at every turn an odd spare pixel goes to the display's
    /// right or bottom edge. Every build places the grid so, and the GPU
    /// path draws the grid where this places it.
    ///
    /// References:
    /// - ADR-1310 D10: the grid flush to the text's bottom edge while smooth scrolling is on, and centred across the rows.
    /// - ADR-1310 D12, "With smooth scrolling off": the grid centred on both axes in the display's own pixels.
    /// - ADR-1303 D3: the GPU path places the grid where the CPU path does.
    pub(super) fn grid_of(&self, cols: usize, rows: usize) -> Grid {
        let cols = cols.min(self.max_cols);
        let rows = rows.min(self.max_rows);
        let gw = int(cols).saturating_mul(self.cw);
        let gh = int(rows).saturating_mul(self.ch);
        let (sw, sh) = (int(self.width), int(self.height));
        let (across, down) = if self.orientation.turned() {
            (gh, gw)
        } else {
            (gw, gh)
        };
        let (spare_x, spare_y) = (sw.saturating_sub(across), sh.saturating_sub(down));
        let (off_x, off_y) = offsets(self.placement, self.orientation, (spare_x, spare_y));
        Grid {
            cols,
            rows,
            off_x,
            off_y,
            max_x: off_x.saturating_add(across),
            max_y: off_y.saturating_add(down),
            #[cfg(feature = "scroll-pan")]
            above: 0,
        }
    }

    /// The grid `s` with the rows of history above the screen that the
    /// CPU path draws in a build with `scroll-pan`: as many as the spare
    /// lines above the grid show, a part of a row included, and one more
    /// above them, whose ink reaches down into the rows below it. The grid
    /// keeps its place on the display: it reaches further up the text by
    /// those rows. A centred grid takes no rows above, so its spare lines
    /// show the border's color and its top row takes no ink from above.
    ///
    /// References:
    /// - ADR-1310 D10, "A strip taller than a row": as many rows of history as fit fill the spare lines.
    /// - ADR-1310 D10, "Ink at the top edge": the screen's top row takes ink from the row above it while smooth scrolling is on, and none while it is off.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn with_rows_above(&self, s: Grid) -> Grid {
        if self.placement == Placement::Centred {
            return s;
        }
        let above = self.strip_rows(&s).saturating_add(1);
        let by = int(above).saturating_mul(self.ch);
        let mut g = Grid {
            rows: s.rows.saturating_add(above),
            above,
            ..s
        };
        match self.orientation {
            Orientation::Normal => g.off_y = s.off_y.saturating_sub(by),
            Orientation::UpsideDown => g.max_y = s.max_y.saturating_add(by),
            Orientation::Right => g.max_x = s.max_x.saturating_add(by),
            Orientation::Left => g.off_x = s.off_x.saturating_sub(by),
        }
        g
    }

    /// The screen's own grid within the grid `g`, without the rows above.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn screen_of(&self, g: &Grid) -> Grid {
        match g.above {
            0 => *g,
            above => self.grid_of(g.cols, g.rows.saturating_sub(above)),
        }
    }

    /// How many rows the spare lines above the screen's grid `s` show,
    /// the topmost perhaps in part: the lines between the text's top edge
    /// and the grid's first row, in rows rounded up.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn strip_rows(&self, s: &Grid) -> usize {
        let rows = self
            .spare_lines(s)
            .saturating_add(self.ch.saturating_sub(1))
            .checked_div(self.ch)
            .unwrap_or(0);
        usize::try_from(rows).unwrap_or(0)
    }

    /// The spare lines above the screen's grid `s`: the pixel lines between
    /// the text's top edge and the grid's first row.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn spare_lines(&self, s: &Grid) -> i64 {
        let (sw, sh) = (int(self.width), int(self.height));
        let spare = match self.orientation {
            Orientation::Normal => s.off_y,
            Orientation::UpsideDown => sh.saturating_sub(s.max_y),
            Orientation::Right => sw.saturating_sub(s.max_x),
            Orientation::Left => s.off_x,
        };
        spare.max(0)
    }

    /// The two bars of spare lines on either side of the grid `g` along
    /// the axis the rows run down, each all across the display: on an
    /// upright display, the lines above the grid's first row and those
    /// below its last. `None` for a bar that holds no line.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn bars(&self, g: &Grid) -> [Option<Rect>; 2] {
        let (w, h) = (self.width, self.height);
        let cut = |v: i64, n: u32| u32::try_from(v.clamp(0, i64::from(n))).unwrap_or(0);
        let (lo, hi, n) = match self.orientation.turned() {
            true => (cut(g.off_x, w), cut(g.max_x, w), w),
            false => (cut(g.off_y, h), cut(g.max_y, h), h),
        };
        [(0, lo), (hi, n)].map(|(a, b)| {
            (a < b).then(|| match self.orientation.turned() {
                true => Rect {
                    x: a,
                    y: 0,
                    width: b.saturating_sub(a),
                    height: h,
                },
                false => Rect {
                    x: 0,
                    y: a,
                    width: w,
                    height: b.saturating_sub(a),
                },
            })
        })
    }

    /// The display area of the grid's rows from `r0` up to `r1`, across
    /// the grid's columns, cut to the display; `None` where it holds no
    /// pixel of the display.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn rows_rect(&self, g: &Grid, (r0, r1): (usize, usize)) -> Option<Rect> {
        let a = self.area(
            g,
            0,
            int(r0).saturating_mul(self.ch),
            int(g.cols).saturating_mul(self.cw),
            int(r1.saturating_sub(r0)).saturating_mul(self.ch),
        );
        self.on_display(a)
    }

    /// The part of the display area `a` that lies on the display, as a
    /// rectangle; `None` when no part does.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn on_display(&self, a: Area) -> Option<Rect> {
        let (w, h) = (int(self.width), int(self.height));
        let (x0, y0) = (a.x.clamp(0, w), a.y.clamp(0, h));
        let x1 = a.x.saturating_add(a.w).clamp(0, w);
        let y1 = a.bottom().clamp(0, h);
        let side = |v: i64| u32::try_from(v).unwrap_or(0);
        let r = Rect {
            x: side(x0),
            y: side(y0),
            width: side(x1.saturating_sub(x0)),
            height: side(y1.saturating_sub(y0)),
        };
        (r.area() > 0).then_some(r)
    }

    /// The display area of the grid's pixels `lx`, `ly`, `lw` by `lh`.
    pub(super) fn area(&self, g: &Grid, lx: i64, ly: i64, lw: i64, lh: i64) -> Area {
        let (x, y, w, h) = match self.orientation {
            Orientation::Normal => (
                g.off_x.saturating_add(lx),
                g.off_y.saturating_add(ly),
                lw,
                lh,
            ),
            Orientation::UpsideDown => (
                g.max_x.saturating_sub(lx.saturating_add(lw)),
                g.max_y.saturating_sub(ly.saturating_add(lh)),
                lw,
                lh,
            ),
            Orientation::Right => (
                g.max_x.saturating_sub(ly.saturating_add(lh)),
                g.off_y.saturating_add(lx),
                lh,
                lw,
            ),
            Orientation::Left => (
                g.off_x.saturating_add(ly),
                g.max_y.saturating_sub(lx.saturating_add(lw)),
                lh,
                lw,
            ),
        };
        Area { x, y, w, h }
    }

    /// The grid's pixels under the display area `a`: [`Layout::area`]
    /// undone.
    pub(super) fn grid_area(&self, g: &Grid, a: Area) -> Area {
        let (lx, ly, lw, lh) = match self.orientation {
            Orientation::Normal => (
                a.x.saturating_sub(g.off_x),
                a.y.saturating_sub(g.off_y),
                a.w,
                a.h,
            ),
            Orientation::UpsideDown => (
                g.max_x.saturating_sub(a.x.saturating_add(a.w)),
                g.max_y.saturating_sub(a.bottom()),
                a.w,
                a.h,
            ),
            Orientation::Right => (
                a.y.saturating_sub(g.off_y),
                g.max_x.saturating_sub(a.x.saturating_add(a.w)),
                a.h,
                a.w,
            ),
            Orientation::Left => (
                g.max_y.saturating_sub(a.bottom()),
                a.x.saturating_sub(g.off_x),
                a.h,
                a.w,
            ),
        };
        Area {
            x: lx,
            y: ly,
            w: lw,
            h: lh,
        }
    }

    /// The display area of a bar cursor's bar `w` pixels wide in the
    /// grid's own pixels, down the whole height of the `n` cells from `col`
    /// in `row`, along their left edge, or along their right edge when
    /// `right`.
    pub(super) fn bar_area(
        &self,
        g: &Grid,
        (col, row, n): (usize, usize, usize),
        w: i64,
        right: bool,
    ) -> Area {
        let span = int(n).saturating_mul(self.cw);
        let w = w.min(span);
        let left = int(col).saturating_mul(self.cw);
        let x = if right {
            left.saturating_add(span).saturating_sub(w)
        } else {
            left
        };
        self.area(g, x, int(row).saturating_mul(self.ch), w, self.ch)
    }

    /// The display area of `n` cells from `col` in `row`.
    pub(super) fn cells_area(&self, g: &Grid, col: usize, row: usize, n: usize) -> Area {
        self.area(
            g,
            int(col).saturating_mul(self.cw),
            int(row).saturating_mul(self.ch),
            int(n).saturating_mul(self.cw),
            self.ch,
        )
    }

    /// The display rectangle of the cell at `col`, `row`.
    pub(super) fn cell_rect(&self, g: &Grid, col: usize, row: usize) -> Rect {
        let a = self.cells_area(g, col, row, 1);
        Rect {
            x: u32::try_from(a.x).unwrap_or(0),
            y: u32::try_from(a.y).unwrap_or(0),
            width: u32::try_from(a.w).unwrap_or(0),
            height: u32::try_from(a.h).unwrap_or(0),
        }
    }
}

/// Where the grid's first display pixel lies, across and down the display,
/// for `spare` (columns, lines) of the display's own pixels left over
/// around it. Placed at the bottom, the grid's last row touches the
/// text's bottom edge: the display's bottom upright, its top upside down,
/// its left turned right and its right turned left. Across the rows it
/// stays centred. Centred, the grid is centred on both axes. Either way an
/// odd spare pixel goes to the display's right or bottom edge.
///
/// References:
/// - ADR-1310 D10: the grid flush to the text's bottom edge while smooth scrolling is on, and centred across the rows.
/// - ADR-1310 D12, "With smooth scrolling off": the grid centred on both axes in the display's own pixels.
fn offsets(
    placement: Placement,
    orientation: Orientation,
    (spare_x, spare_y): (i64, i64),
) -> (i64, i64) {
    match (placement, orientation) {
        (Placement::Centred, _) => (spare_x / 2, spare_y / 2),
        (Placement::Bottom(_), Orientation::Normal) => (spare_x / 2, spare_y),
        (Placement::Bottom(_), Orientation::UpsideDown) => (spare_x / 2, 0),
        (Placement::Bottom(_), Orientation::Right) => (0, spare_y / 2),
        (Placement::Bottom(_), Orientation::Left) => (spare_x, spare_y / 2),
    }
}

/// Where the renderer places a grid `grid` pixels wide and high, as the
/// text reads, on a display `display` pixels wide and high in its own
/// pixels, turned by `orientation`: the pixel, as the text reads, of the
/// grid's top-left corner, counted from the display's top-left corner as
/// the text reads. The login picture is fitted to this origin, so the
/// picture and the grid share one rule. A grid larger than the display
/// starts at 0 on that axis.
///
/// References:
/// - ADR-1310 D10 and D12: where the grid sits with smooth scrolling on and off.
/// - ADR-1100 D17: the login picture fitted to the grid on the display.
pub fn grid_origin(
    placement: Placement,
    orientation: Orientation,
    display: (u32, u32),
    grid: (u32, u32),
) -> (u32, u32) {
    let (across, down) = if orientation.turned() {
        (grid.1, grid.0)
    } else {
        grid
    };
    let spare_x = int(display.0).saturating_sub(int(across)).max(0);
    let spare_y = int(display.1).saturating_sub(int(down)).max(0);
    let (off_x, off_y) = offsets(placement, orientation, (spare_x, spare_y));
    let (x, y) = match orientation {
        Orientation::Normal => (off_x, off_y),
        Orientation::UpsideDown => (spare_x.saturating_sub(off_x), spare_y.saturating_sub(off_y)),
        Orientation::Right => (off_y, spare_x.saturating_sub(off_x)),
        Orientation::Left => (spare_y.saturating_sub(off_y), off_x),
    };
    (u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
}
