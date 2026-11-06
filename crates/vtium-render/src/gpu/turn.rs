//! The turn between the upright frame and a turned display (ADR-1303 D3,
//! D6).
//!
//! The GPU path composes the upright frame, the whole display as the text
//! sees it, and present turns it into the display's buffer. The renderer's
//! front end works in the display's own pixels, as on the CPU path, so the
//! GPU path maps each rectangle the front end gives into the upright frame
//! with [`Turn::up`], and each rectangle present draws back into the
//! display with [`Turn::down`]. Both map a pixel as `Surface` in notice.rs
//! and the CPU path's grid do: a display turned right draws the upright
//! pixel `x`, `y` at `width - 1 - y`, `x`.
//!
//! The grid therefore sits in the upright frame where the CPU path puts it
//! on the display, mapped back, with an odd margin's spare pixel where the
//! CPU path leaves it once the frame is turned, so the two paths draw a
//! turned grid at the same pixels.

use super::gl::Size;
use crate::compose::Area;
use crate::target::{Orientation, Rect};

/// A display's turn and size: how the upright frame maps onto it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Turn {
    pub(crate) orientation: Orientation,
    /// The display's size, in its own pixels.
    pub(crate) display: Size,
}

impl Turn {
    pub(crate) fn new(orientation: Orientation, display: Size) -> Turn {
        Turn {
            orientation,
            display,
        }
    }

    /// The upright frame's size: the display's, its sides swapped for a
    /// quarter turn.
    pub(crate) fn upright(self) -> Size {
        upright(self.orientation, self.display)
    }

    /// The upright frame's area that the display's area `a` draws. The area
    /// may reach past the display, and maps all the same.
    pub(crate) fn up_area(self, a: Area) -> Area {
        let (w, h) = (
            i64::from(self.display.width),
            i64::from(self.display.height),
        );
        let back = |n: i64, at: i64, len: i64| n.saturating_sub(at).saturating_sub(len);
        match self.orientation {
            Orientation::Normal => a,
            Orientation::UpsideDown => Area {
                x: back(w, a.x, a.w),
                y: back(h, a.y, a.h),
                ..a
            },
            Orientation::Right => Area {
                x: a.y,
                y: back(w, a.x, a.w),
                w: a.h,
                h: a.w,
            },
            Orientation::Left => Area {
                x: back(h, a.y, a.h),
                y: a.x,
                w: a.h,
                h: a.w,
            },
        }
    }

    /// The display's area that the upright frame's area `a` draws on:
    /// [`Turn::up_area`] undone.
    pub(crate) fn down_area(self, a: Area) -> Area {
        let up = self.upright();
        let (w, h) = (i64::from(up.width), i64::from(up.height));
        let back = |n: i64, at: i64, len: i64| n.saturating_sub(at).saturating_sub(len);
        match self.orientation {
            Orientation::Normal => a,
            Orientation::UpsideDown => Area {
                x: back(w, a.x, a.w),
                y: back(h, a.y, a.h),
                ..a
            },
            Orientation::Right => Area {
                x: back(h, a.y, a.h),
                y: a.x,
                w: a.h,
                h: a.w,
            },
            Orientation::Left => Area {
                x: a.y,
                y: back(w, a.x, a.w),
                w: a.h,
                h: a.w,
            },
        }
    }

    /// [`Turn::up_area`] for a rectangle inside the display.
    pub(crate) fn up(self, r: Rect) -> Rect {
        rect(self.up_area(area(r)))
    }

    /// [`Turn::down_area`] for a rectangle inside the upright frame.
    pub(crate) fn down(self, r: Rect) -> Rect {
        rect(self.down_area(area(r)))
    }

    /// The code present's shader reads for the turn: 0 upright, 1 a quarter
    /// clockwise, 2 a half, 3 a quarter anticlockwise.
    pub(crate) fn code(self) -> u32 {
        match self.orientation {
            Orientation::Normal => 0,
            Orientation::Right => 1,
            Orientation::UpsideDown => 2,
            Orientation::Left => 3,
        }
    }
}

/// The upright frame's size for a display of `display` turned as
/// `orientation` says.
pub(crate) fn upright(orientation: Orientation, display: Size) -> Size {
    if orientation.turned() {
        Size::new(display.height, display.width)
    } else {
        display
    }
}

fn area(r: Rect) -> Area {
    Area {
        x: i64::from(r.x),
        y: i64::from(r.y),
        w: i64::from(r.width),
        h: i64::from(r.height),
    }
}

/// `a` as a rectangle, the part of it left of or above the origin cut off.
fn rect(a: Area) -> Rect {
    let side = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    let (x, y) = (a.x.max(0), a.y.max(0));
    Rect {
        x: side(x),
        y: side(y),
        width: side(a.right().saturating_sub(x)),
        height: side(a.bottom().saturating_sub(y)),
    }
}

#[cfg(test)]
#[expect(clippy::arithmetic_side_effects, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    const ALL: [Orientation; 4] = [
        Orientation::Normal,
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ];

    /// The display pixel that draws the upright pixel `x`, `y`, as
    /// notice.rs's `Surface::place` and the CPU path's grid turn it.
    fn place(o: Orientation, display: Size, x: u32, y: u32) -> (u32, u32) {
        let (w, h) = (display.width, display.height);
        match o {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
            Orientation::Right => (w - 1 - y, x),
            Orientation::Left => (y, h - 1 - x),
        }
    }

    #[test]
    fn a_rectangle_maps_pixel_for_pixel_as_the_cpu_path_turns_a_pixel() {
        let display = Size::new(13, 7);
        let r = Rect {
            x: 2,
            y: 1,
            width: 4,
            height: 3,
        };
        for o in ALL {
            let t = Turn::new(o, display);
            let up = t.up(r);
            // every upright pixel of the mapped rectangle lands in `r`, and
            // they are as many as its pixels
            assert_eq!(up.area(), r.area(), "{o:?}");
            for y in up.y..up.y + up.height {
                for x in up.x..up.x + up.width {
                    let (dx, dy) = place(o, display, x, y);
                    assert!(
                        dx >= r.x && dx < r.x + r.width && dy >= r.y && dy < r.y + r.height,
                        "{o:?} {x},{y}"
                    );
                }
            }
            assert_eq!(t.down(up), r, "{o:?}");
            assert!(t.upright().holds(up), "{o:?}");
        }
    }

    #[test]
    fn an_area_past_the_display_maps_and_comes_back() {
        let display = Size::new(20, 10);
        let a = Area {
            x: -3,
            y: 8,
            w: 6,
            h: 5,
        };
        for o in ALL {
            let t = Turn::new(o, display);
            assert_eq!(t.down_area(t.up_area(a)), a, "{o:?}");
        }
        let right = Turn::new(Orientation::Right, display).up_area(a);
        assert_eq!(
            right,
            Area {
                x: 8,
                y: 17,
                w: 5,
                h: 6
            }
        );
    }

    #[test]
    fn a_quarter_turn_swaps_the_upright_frames_sides() {
        let display = Size::new(1280, 800);
        assert_eq!(upright(Orientation::Normal, display), display);
        assert_eq!(upright(Orientation::UpsideDown, display), display);
        assert_eq!(upright(Orientation::Right, display), Size::new(800, 1280));
        assert_eq!(upright(Orientation::Left, display), Size::new(800, 1280));
        let codes: Vec<u32> = ALL.iter().map(|&o| Turn::new(o, display).code()).collect();
        assert_eq!(codes, [0, 1, 2, 3]);
    }
}
