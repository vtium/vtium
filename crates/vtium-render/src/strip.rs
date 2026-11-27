//! The spare lines above a grid placed at the bottom, blanked or faded as
//! `partial-row` asks.
//!
//! The renderer draws the rows of history above the screen into the spare
//! lines, as under `partial-row = history`. Under `blank` and `fade`, every
//! pixel the frame writes into the spare lines passes through [`Strip`]
//! first, which replaces it with the border's color, or mixes it toward the
//! border's color the farther its line lies from the grid's first row.
//! The strip works in the display's own coordinates, before the pointer's
//! blend, the color slot and the 1-bit cut, so every way of drawing a
//! scroll, the redraw, the pan and the copy, writes the same pixels. The
//! ring's slots under the spare lines then hold covered pixels, so after a
//! scroll the renderer draws again each row that moved into the spare lines
//! or out of them.
//!
//! References:
//! - ADR-1310 D12, "The key `partial-row`": the blank and the fade drawn over the spare lines after the rows, in linear light, and a covered row drawn again when a later window shows it.

use super::{Grid, Layout, PartialRow, Placement};
use crate::blend;
use crate::target::{Orientation, Target};

/// Where the spare lines lie on the display, along the axis the rows run
/// down, and how they are covered.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cover {
    /// The rows run along the display's x axis.
    turned: bool,
    /// The spare lines' coordinates along the axis, from `from` up to `to`.
    from: u32,
    to: u32,
    /// The coordinate next to the grid's first row grows toward the
    /// screen's edge as it falls: the spare lines lie before the grid.
    before: bool,
    fade: bool,
    border: u32,
}

impl Cover {
    /// The cover of the spare lines above the screen's grid `s` under the
    /// layout `l`, whose border is the display pixel `border`; `None`
    /// when the placement draws them as they are, or there are none.
    pub(super) fn of(l: &Layout, s: &Grid, border: u32) -> Option<Cover> {
        let fade = match l.placement {
            Placement::Bottom(PartialRow::Blank) => false,
            Placement::Bottom(PartialRow::Fade) => true,
            Placement::Bottom(PartialRow::History) | Placement::Centred => return None,
        };
        let to_u = |v: i64| u32::try_from(v.max(0)).unwrap_or(0);
        let (from, to, before) = match l.orientation {
            Orientation::Normal => (0, to_u(s.off_y).min(l.height), true),
            Orientation::UpsideDown => (to_u(s.max_y), l.height, false),
            Orientation::Right => (to_u(s.max_x), l.width, false),
            Orientation::Left => (0, to_u(s.off_x).min(l.width), true),
        };
        (from < to).then_some(Cover {
            turned: l.orientation.turned(),
            from,
            to,
            before,
            fade,
            border,
        })
    }

    /// The pixel `p` written at coordinate `at` along the axis.
    fn cover(&self, at: u32, p: u32) -> u32 {
        if p == self.border || !self.fade {
            return self.border;
        }
        let lines = self.to.saturating_sub(self.from);
        // how many lines lie between this one and the grid's first row
        let gap = match self.before {
            true => self.to.saturating_sub(1).saturating_sub(at),
            false => at.saturating_sub(self.from),
        };
        // the row's own pixel next to the grid, the border's color at the
        // screen's edge, and a line of its own takes the border's color
        let span = lines.saturating_sub(1);
        let keep = match span {
            0 => 0,
            _ => {
                let left = span.saturating_sub(gap);
                left.saturating_mul(255)
                    .saturating_add(span / 2)
                    .checked_div(span)
                    .unwrap_or(0)
            }
        };
        blend::mix_linear(p, self.border, u8::try_from(keep.min(255)).unwrap_or(255))
    }
}

/// A target that covers each pixel written into the spare lines, as
/// [`Cover`] says, and passes every other pixel through as it is.
pub(super) struct Strip<'a, T: Target + ?Sized> {
    pub(super) inner: &'a mut T,
    pub(super) cover: Option<Cover>,
    pub(super) line: &'a mut Vec<u32>,
}

impl<T: Target + ?Sized> Target for Strip<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let Some(c) = self.cover else {
            return self.inner.write(x, y, pixels);
        };
        if !c.turned {
            if y < c.from || y >= c.to {
                return self.inner.write(x, y, pixels);
            }
            self.line.clear();
            self.line.extend(pixels.iter().map(|&p| c.cover(y, p)));
            return self.inner.write(x, y, self.line);
        }
        let len = u32::try_from(pixels.len()).unwrap_or(u32::MAX);
        let end = x.saturating_add(len);
        if end <= c.from || x >= c.to {
            return self.inner.write(x, y, pixels);
        }
        self.line.clear();
        self.line.extend(pixels.iter().zip(x..).map(
            |(&p, at)| match (c.from..c.to).contains(&at) {
                true => c.cover(at, p),
                false => p,
            },
        ));
        self.inner.write(x, y, self.line);
    }
}
