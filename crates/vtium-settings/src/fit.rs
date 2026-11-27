//! How many character cells fit on a display, and the pixels left over.
//!
//! vtium applies one rule to each axis of a display. Along the text's
//! height, the rows that fit are the display's pixels divided by a row's
//! height, rounded down. Across the text, the columns that fit are the
//! pixels divided by a cell's width, rounded down. What remains on each
//! axis is left over. The axes are the text's, so on a display turned on
//! its side the rows run along the display's width.
//!
//! vtium logs [`Fit::line`] for each display whose layout leaves pixels
//! over, and vtium-config reads the same [`Fit`] for its note beside a
//! font set, a size or a look. [`Axis::of`] gives one axis alone, which
//! vtium-config can try for each size it offers to find the sizes that
//! fit exactly.
//!
//! References:
//! - ADR-1310 D12, "What a partial row is": one rule applied to each axis.
//! - ADR-1310 D12, "vtium logs it": the line's words and when it is logged.
//! - ADR-1310 D12, "vtium-config notes it": the note that reads the same arithmetic.

use core::fmt::Write as _;

/// One axis of a display: the cells that fit along it, a cell's size along
/// it, and the pixels left over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Axis {
    /// The cells that fit, whole.
    pub cells: u32,
    /// A cell's size along the axis, in pixels.
    pub size: u32,
    /// The pixels left over after the cells that fit.
    pub left: u32,
}

impl Axis {
    /// The axis `pixels` long, in cells `size` pixels long. A cell of no
    /// size fits nowhere, and leaves every pixel over.
    pub fn of(pixels: u32, size: u32) -> Axis {
        Axis {
            cells: pixels.checked_div(size).unwrap_or(0),
            size,
            left: pixels.checked_rem(size).unwrap_or(pixels),
        }
    }

    /// The cells fill the axis exactly.
    pub fn exact(&self) -> bool {
        self.left == 0
    }
}

/// A display's columns and rows of character cells, and the pixels left
/// over on each axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fit {
    /// The display's size as its mode states it, width by height.
    pub display: (u32, u32),
    /// The columns, across the text.
    pub columns: Axis,
    /// The rows, along the text's height.
    pub rows: Axis,
}

impl Fit {
    /// The cells of `cell`, width by height, that fit on a display of
    /// `display` pixels, width by height as its mode states it. On a
    /// display `turned` on its side, the rows run along its width.
    pub fn of(display: (u32, u32), cell: (u32, u32), turned: bool) -> Fit {
        let (across, along) = match turned {
            true => (display.1, display.0),
            false => display,
        };
        Fit {
            display,
            columns: Axis::of(across, cell.0),
            rows: Axis::of(along, cell.1),
        }
    }

    /// Either axis leaves pixels over.
    pub fn leaves_pixels(&self) -> bool {
        !self.columns.exact() || !self.rows.exact()
    }

    /// The line vtium logs for the display named `name`, as information:
    /// its size, the cells that fit along each axis that leaves pixels
    /// over, and those pixels. `None` when both axes divide evenly.
    ///
    /// For example: "Display DP-3 is 5120×1440 and holds 465 columns of 11
    /// pixels and 65 rows of 22 pixels, so 5 pixel columns and 10 pixel
    /// lines are left over."
    pub fn line(&self, name: &str) -> Option<String> {
        let (c, r) = (self.columns, self.rows);
        let held = |a: Axis, one: &str, many: &str| {
            format!(
                "{} {} of {} pixel{}",
                a.cells,
                if a.cells == 1 { one } else { many },
                a.size,
                if a.size == 1 { "" } else { "s" },
            )
        };
        let over =
            |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let (cols_held, rows_held) = (held(c, "column", "columns"), held(r, "row", "rows"));
        let (cols_over, rows_over) = (
            over(c.left, "pixel column", "pixel columns"),
            over(r.left, "pixel line", "pixel lines"),
        );
        let (holds, left, many) = match (c.exact(), r.exact()) {
            (true, true) => return None,
            (false, false) => (
                format!("{cols_held} and {rows_held}"),
                format!("{cols_over} and {rows_over}"),
                true,
            ),
            (false, true) => (cols_held, cols_over, c.left != 1),
            (true, false) => (rows_held, rows_over, r.left != 1),
        };
        let mut s = String::new();
        let _ = write!(
            s,
            "Display {name} is {}×{} and holds {holds}, so {left} {} left over.",
            self.display.0,
            self.display.1,
            if many { "are" } else { "is" },
        );
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Fit};

    // The record's examples: a display that leaves pixels over on both
    // axes names both, one that leaves them on one axis names only that
    // axis, and one that divides evenly on both says nothing.
    //
    // References:
    // - ADR-1310 D12, "What a partial row is" and "vtium logs it": the examples.
    #[test]
    fn the_line_names_each_axis_that_leaves_pixels_over() {
        assert_eq!(
            Fit::of((5120, 1440), (11, 22), false)
                .line("DP-3")
                .as_deref(),
            Some(
                "Display DP-3 is 5120×1440 and holds 465 columns of 11 pixels and 65 rows of 22 pixels, so 5 pixel columns and 10 pixel lines are left over."
            )
        );
        assert_eq!(
            Fit::of((1366, 768), (8, 16), false)
                .line("HDMI-A-1")
                .as_deref(),
            Some(
                "Display HDMI-A-1 is 1366×768 and holds 170 columns of 8 pixels, so 6 pixel columns are left over."
            )
        );
        assert_eq!(
            Fit::of((7680, 4320), (20, 41), false)
                .line("DP-1")
                .as_deref(),
            Some(
                "Display DP-1 is 7680×4320 and holds 105 rows of 41 pixels, so 15 pixel lines are left over."
            )
        );
        assert_eq!(Fit::of((7680, 4320), (24, 48), false).line("DP-1"), None);
        assert!(!Fit::of((7680, 4320), (24, 48), false).leaves_pixels());
    }

    // On a display turned on its side, the rows run along its width and
    // the columns along its height, and the line still gives the size as
    // the display's mode states it.
    #[test]
    fn a_turned_display_counts_its_rows_along_its_width() {
        let f = Fit::of((1440, 5120), (11, 22), true);
        assert_eq!(f.columns, Axis::of(5120, 11));
        assert_eq!(f.rows, Axis::of(1440, 22));
        assert_eq!(
            f.line("DP-3").as_deref(),
            Some(
                "Display DP-3 is 1440×5120 and holds 465 columns of 11 pixels and 65 rows of 22 pixels, so 5 pixel columns and 10 pixel lines are left over."
            )
        );
    }

    // One pixel left over takes the singular.
    #[test]
    fn one_pixel_left_over_is_one_line() {
        assert_eq!(
            Fit::of((64, 49), (8, 16), false).line("X").as_deref(),
            Some("Display X is 64×49 and holds 3 rows of 16 pixels, so 1 pixel line is left over.")
        );
    }

    // The nearest sizes that fit an axis exactly, which vtium-config
    // offers, are those whose axis leaves nothing over: on 5120 pixel
    // columns, 10 and 16 pixels, and none from 12 to 15.
    #[test]
    fn an_axis_fits_exactly_when_it_leaves_nothing_over() {
        let exact: Vec<u32> = (10..=16).filter(|&w| Axis::of(5120, w).exact()).collect();
        assert_eq!(exact, [10, 16]);
        assert_eq!(Axis::of(1440, 20).cells, 72);
        assert_eq!(
            Axis::of(100, 0),
            Axis {
                cells: 0,
                size: 0,
                left: 100
            }
        );
    }
}
