//! A head's grid and each of its displays' font height, with several
//! displays at different sizes, the zoom's direction and the rotation step
//! (`src/terminal.c:961-1138, 1166-1203, 1325-1355` at kmscon b1f7a7c5).
//! The heights a zoom steps through are the font's ([`crate::font::zoom`]).
//!
//! A head's grid is the smallest every display of the head holds at the
//! base font, so displays mirror one grid within a head (ADR-1401 D1). A
//! larger display then draws at the largest height that still holds that
//! grid, tried from the height that would fill it and down a size step at
//! a time, and at the base height when no larger one fits: the C
//! reference's `multi-monitor=scaled`, the operator's choice on
//! vtium/vtium#21.

use vtium_render::Orientation;

/// The grid and the height each display draws at, in the order given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub grid: (usize, usize),
    pub heights: Vec<u32>,
}

/// The layout for `displays` (each width by height in pixels), turned a
/// quarter when `turned`, at the `base` height, whose size step is `step`.
/// `cell` gives a font's cell at a height, `None` when there is no font at
/// it. `None` when no display holds a cell.
pub fn layout(
    displays: &[(u32, u32)],
    turned: bool,
    base: u32,
    step: u32,
    cell: impl Fn(u32) -> Option<(u32, u32)>,
) -> Option<Layout> {
    let (bw, bh) = cell(base).filter(|&(w, h)| w > 0 && h > 0)?;
    let sides = |&(w, h): &(u32, u32)| if turned { (h, w) } else { (w, h) };
    let fits = |(w, h): (u32, u32), (cw, ch): (u32, u32)| {
        (
            to_usize(w.checked_div(cw).unwrap_or(0)),
            to_usize(h.checked_div(ch).unwrap_or(0)),
        )
    };
    let grid = displays
        .iter()
        .map(|d| fits(sides(d), (bw, bh)))
        .filter(|&(c, r)| c > 0 && r > 0)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)))?;
    let heights = displays
        .iter()
        .map(|d| {
            let size = sides(d);
            let (cols, rows) = fits(size, (bw, bh));
            let scaled = |n: usize, of: usize| {
                u64::from(base)
                    .saturating_mul(u64::try_from(n).unwrap_or(0))
                    .checked_div(u64::try_from(of).unwrap_or(0))
                    .map_or(0, |v| u32::try_from(v).unwrap_or(u32::MAX))
            };
            let mut h = scaled(cols, grid.0).min(scaled(rows, grid.1));
            while h > base {
                match cell(h) {
                    Some(c) if c.1 == bh => return base,
                    Some(c) if fits(size, c).0 >= grid.0 && fits(size, c).1 >= grid.1 => return h,
                    _ => h = h.saturating_sub(step.max(1)),
                }
            }
            base
        })
        .collect();
    Some(Layout { grid, heights })
}

fn to_usize(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// Which way a zoom goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
}

/// The orientation a quarter turn gives, clockwise when `cw`.
pub fn rotate(o: Orientation, cw: bool) -> Orientation {
    use Orientation::*;
    match (o, cw) {
        (Normal, true) | (UpsideDown, false) => Right,
        (Right, true) | (Left, false) => UpsideDown,
        (UpsideDown, true) | (Normal, false) => Left,
        (Left, true) | (Right, false) => Normal,
    }
}

#[cfg(test)]
#[expect(clippy::arithmetic_side_effects, reason = "test fonts' small cells")]
mod tests {
    use super::*;

    /// A bitmap font of 8x16 that scales by whole factors, as unifont.
    fn bitmap(h: u32) -> Option<(u32, u32)> {
        let scale = (h / 16).max(1);
        Some((8 * scale, 16 * scale))
    }

    #[test]
    fn one_display() {
        let l = layout(&[(1366, 768)], false, 16, 16, bitmap);
        assert_eq!(
            l,
            Some(Layout {
                grid: (170, 48),
                heights: vec![16]
            })
        );
    }

    #[test]
    fn a_larger_display_scales_up_to_the_smaller_ones_grid() {
        // 1366x768 holds 170x48. 3840x2160 holds 480x135 at 16, so the
        // height that fills it is min(480 * 16 / 170, 135 * 16 / 48) = 45,
        // as the C reference asked for; the bitmap draws 45 at scale 2, a
        // 16x32 cell, 240x67, which holds 170x48.
        let l = layout(&[(1366, 768), (3840, 2160)], false, 16, 16, bitmap);
        assert_eq!(
            l,
            Some(Layout {
                grid: (170, 48),
                heights: vec![16, 45]
            })
        );
        // An outline font at every height: 45 gives a 22x45 cell, 174x48,
        // which holds 170x48.
        let outline = |h: u32| Some((h / 2, h));
        let l = layout(&[(1366, 768), (3840, 2160)], false, 16, 16, outline);
        assert_eq!(l.map(|l| l.heights), Some(vec![16, 45]));
        // Where 45 does not fit, a size step down is tried until one does:
        // with cells h*h/32 wide, 28 gives 24, 160 columns, too few; 27
        // gives 22, 174 columns.
        let l = layout(&[(1366, 768), (3840, 2160)], false, 16, 1, |h| {
            Some((h * h / 32, h))
        });
        assert_eq!(l.map(|l| l.heights), Some(vec![16, 27]));
    }

    #[test]
    fn no_larger_font_that_fits_keeps_the_base() {
        // Twice as wide but no taller: the height that fills it is the base.
        let l = layout(&[(1024, 768), (2048, 768)], false, 16, 16, bitmap);
        assert_eq!(l.map(|l| l.heights), Some(vec![16, 16]));
        // A height with no font falls back to the base.
        let l = layout(&[(640, 480), (1920, 1440)], false, 16, 16, |h| {
            (h == 16).then_some((8, 16))
        });
        assert_eq!(l.map(|l| l.heights), Some(vec![16, 16]));
    }

    #[test]
    fn a_turned_display_swaps_its_sides() {
        let l = layout(&[(1366, 768)], true, 16, 16, bitmap);
        assert_eq!(l.map(|l| l.grid), Some((96, 85)));
    }

    #[test]
    fn no_display_holds_a_cell() {
        assert_eq!(layout(&[], false, 16, 16, bitmap), None);
        assert_eq!(layout(&[(4, 4)], false, 16, 16, bitmap), None);
    }

    #[test]
    fn rotation_turns_a_quarter_each_way() {
        use Orientation::*;
        assert_eq!(rotate(Normal, true), Right);
        assert_eq!(rotate(Right, true), UpsideDown);
        assert_eq!(rotate(UpsideDown, true), Left);
        assert_eq!(rotate(Left, true), Normal);
        assert_eq!(rotate(Normal, false), Left);
        assert_eq!(rotate(Left, false), UpsideDown);
    }
}
