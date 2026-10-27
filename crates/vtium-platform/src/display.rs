//! Displays: DRM/KMS through dumb buffers (drm2d), and fbdev.
//!
//! A video device is one DRM card ([`Drm2d`]) or one fbdev node
//! ([`Fbdev`]), opened by the seat and handed over as a descriptor. Each
//! offers its displays the same way:
//!
//! - [`Drm2d::canvas`] lends the back buffer as a [`Canvas`] in XRGB8888,
//!   with its age, so the compositor redraws only what the buffer lacks;
//! - [`Drm2d::swap`] presents it with the frame's damage, and a
//!   [`Event::Flipped`] tells when the frame is on screen and the next may
//!   be drawn, so frames are paced by the display. Damage is rectangles,
//!   the whole screen one rectangle covering it; a swap with nothing to
//!   send commits nothing and says so ([`Swapped::Nothing`]);
//! - [`Drm2d::sleep`] and [`Drm2d::wake_up`] give the display up and take
//!   it back across a VT switch, and [`Event::Asleep`] tells when the
//!   release may be acknowledged.
//!
//! Each device collects its [`Event`]s, and the owner drains them once a
//! turn of the loop, where the terminal draws (`Loop::run`'s callback).
//! Nothing here blocks: a modeset or a sleep that finds a page flip in
//! flight waits for the flip's completion event, and each flip in flight
//! is taken as done after [`crate::FLIP_WAIT_MS`] without one.

mod blank;
mod card;
mod drm2d;
#[cfg(test)]
mod fake;
mod fbdev;
mod kms;
mod mode;

pub use blank::{Blank, DPMS_TIMEOUT};
pub use card::Card;
pub use drm2d::Drm2d;
pub use fbdev::{Fb, Fbdev};
pub use mode::{Mode, ModeLookup, ModePolicy, Wanted};

use std::io;

/// A display: the connector's id on a DRM card, 0 on fbdev.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayId(pub u32);

/// A rectangle of pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The part of `self` inside a `width` by `height` screen.
    fn clip(self, width: u32, height: u32) -> Option<Rect> {
        let x = self.x.min(width);
        let y = self.y.min(height);
        let right = self.x.saturating_add(self.width).min(width);
        let bottom = self.y.saturating_add(self.height).min(height);
        (right > x && bottom > y).then(|| Rect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        })
    }

    /// The smallest rectangle holding `self` and `other`.
    fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Rect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}

/// A display's power state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dpms {
    On,
    Standby,
    Suspend,
    Off,
}

/// The back buffer, lent for one frame: XRGB8888, `stride` bytes a line.
pub struct Canvas<'a> {
    pub pixels: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    /// How many frames ago the buffer was last drawn: 1 holds the frame
    /// before this one, 2 the frame before that, and 0 means its contents
    /// are unknown, so the frame is drawn whole.
    pub age: u32,
}

/// What a video device tells its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A display is online: draw it whole.
    Added(DisplayId),
    /// A display is gone.
    Removed(DisplayId),
    /// The display was set up again, as after a modeset or a wake-up, and
    /// shows nothing drawn yet: draw it whole.
    Redraw(DisplayId),
    /// The last frame swapped is on screen, and the next may be drawn.
    Flipped(DisplayId),
    /// The device holds its displays again after a wake-up.
    Awake,
    /// The device has let its displays go, and the VT release may be
    /// acknowledged.
    Asleep,
    /// Waking up failed, and the retries ran out.
    WakeFailed,
}

/// What a swap sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swapped {
    /// The frame drawn into the canvas, with its damage and the cursor's
    /// state. [`Event::Flipped`] follows.
    Frame,
    /// The cursor plane's state alone; the front buffer stays on screen.
    /// [`Event::Flipped`] follows.
    Cursor,
    /// Nothing: no frame drawn with damage, none kept from a failed swap,
    /// and the cursor unchanged. No [`Event::Flipped`] follows.
    Nothing,
}

/// Why a swap did not present the frame. The frame's damage is kept, so
/// the next swap that succeeds carries it, and the buffer it was drawn
/// into holds it: lent again, its age is 1.
#[derive(Debug)]
pub enum SwapError {
    /// A flip is in flight, or a modeset or a sleep waits for one.
    Busy,
    /// The display is not on: offline, asleep, or powered down.
    Off,
    /// The device refused the frame.
    Failed(io::Error),
}

impl From<io::Error> for SwapError {
    fn from(e: io::Error) -> SwapError {
        if e.raw_os_error() == Some(rustix::io::Errno::BUSY.raw_os_error()) {
            SwapError::Busy
        } else {
            SwapError::Failed(e)
        }
    }
}

/// The damage a swap carries: the frame's, and that of earlier frames
/// whose swaps failed, bounded by [`crate::DAMAGE_CLIPS_MAX`] rectangles.
#[derive(Debug, Default)]
struct Damage {
    rects: Vec<Rect>,
    /// The whole screen, as for an empty list or a list past the bound.
    whole: bool,
}

impl Damage {
    /// Adds a frame's damage on a `width` by `height` screen. A rectangle
    /// covering the screen is the whole screen.
    fn add(&mut self, damage: &[Rect], width: u32, height: u32) {
        let screen = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        if damage.iter().any(|r| r.clip(width, height) == Some(screen)) {
            self.whole = true;
        }
        if self.whole {
            self.rects.clear();
            return;
        }
        self.rects
            .extend(damage.iter().filter_map(|r| r.clip(width, height)));
        if self.rects.len() > crate::DAMAGE_CLIPS_MAX {
            let joined = self.rects.iter().copied().reduce(Rect::union);
            self.rects.clear();
            self.rects.extend(joined);
        }
    }

    /// The whole screen, as after a modeset, whose damage no clip names.
    fn set_whole(&mut self) {
        self.rects.clear();
        self.whole = true;
    }

    /// The rectangles to send, or `None` for the whole screen.
    fn clips(&self) -> Option<&[Rect]> {
        (!self.whole && !self.rects.is_empty()).then_some(self.rects.as_slice())
    }

    fn clear(&mut self) {
        self.rects.clear();
        self.whole = false;
    }
}

/// Which frame each of a display's buffers holds, for [`Canvas::age`].
#[derive(Debug, Default)]
struct Ages {
    /// Frames drawn since the buffers were set up.
    frame: u64,
    /// The frame each buffer holds, 0 when its contents are unknown.
    held: [u64; 2],
}

impl Ages {
    fn age(&self, buffer: usize) -> u32 {
        match self.held.get(buffer) {
            Some(&held) if held != 0 => {
                u32::try_from(self.frame.saturating_sub(held).saturating_add(1)).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// `buffer` now holds the next frame.
    fn drawn(&mut self, buffer: usize) {
        self.frame = self.frame.saturating_add(1);
        if let Some(held) = self.held.get_mut(buffer) {
            *held = self.frame;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn damage_is_clipped_to_the_screen() {
        let mut d = Damage::default();
        d.add(&[rect(90, 90, 20, 20), rect(200, 0, 5, 5)], 100, 100);
        assert_eq!(d.clips(), Some(&[rect(90, 90, 10, 10)][..]));
    }

    #[test]
    fn damage_past_the_bound_becomes_its_bounding_box() {
        let mut d = Damage::default();
        let many: Vec<Rect> = (0..=crate::DAMAGE_CLIPS_MAX)
            .map(|i| rect(u32::try_from(i).unwrap_or(0), 7, 1, 1))
            .collect();
        d.add(&many, 4096, 4096);
        let last = u32::try_from(crate::DAMAGE_CLIPS_MAX).unwrap_or(0);
        assert_eq!(d.clips(), Some(&[rect(0, 7, last + 1, 1)][..]));
    }

    #[test]
    fn the_whole_screen_stays_whole_until_cleared() {
        let mut d = Damage::default();
        d.add(&[rect(0, 0, 200, 100)], 100, 100);
        d.add(&[rect(0, 0, 1, 1)], 100, 100);
        assert_eq!(d.clips(), None);
        d.clear();
        d.add(&[rect(0, 0, 1, 1)], 100, 100);
        assert_eq!(d.clips(), Some(&[rect(0, 0, 1, 1)][..]));
    }

    #[test]
    fn ages_count_frames_since_a_buffer_was_drawn() {
        let mut a = Ages::default();
        assert_eq!(a.age(0), 0);
        a.drawn(0);
        a.drawn(1);
        assert_eq!(a.age(0), 2);
        assert_eq!(a.age(1), 1);
    }
}
