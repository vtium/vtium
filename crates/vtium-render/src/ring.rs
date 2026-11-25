//! The ring of drawn rows: how the CPU path shows a scroll without drawing
//! the rows that moved.
//!
//! Each line of the grid keeps a fixed place, a slot, in a ring of drawn
//! rows. Grid row `y` lies in slot `(base + y) mod slots`, so a scroll by
//! `n` rows moves `base` by `n` and leaves every line in its slot. The
//! display shows a window over the ring that starts at `base`. The grid's
//! last row touches the screen's far edge, so the spare pixel lines, left
//! when the screen's length is not a whole number of rows, lie above the
//! grid's first row. The ring has one slot more than the screen has rows,
//! and that slot lies directly above the grid's first row, so the spare
//! lines are the bottom of that slot. On a grid smaller than the screen,
//! the slots the grid leaves lie above it too. The renderer draws the
//! rows of history above the screen into those slots, as it draws them
//! in every way of drawing, so the spare lines show the bottom of the
//! row above, and the border's color only where no row exists.
//!
//! The pan keeps the ring in the display's own buffer, which is one
//! screen longer than the ring along the axis the rows run down. Each
//! pixel line that lies in the ring's first screen-length has a second
//! copy, its twin, one ring's length further on, so a window that starts
//! anywhere in the ring ends inside the buffer and shows its lines in
//! order. vtium writes the copy that the window being shown holds, and
//! records, for that buffer, that the slot's other copy is stale. It draws
//! the slot's row again just before it shows a window that holds the
//! stale copy. A frame that scrolls writes one copy more. A scroll up
//! moves the window toward the ring's end, past which it shows every
//! first copy again at once, so vtium writes the first copy with each
//! twin it writes. A scroll down moves the window toward the ring's start,
//! past which it shows every twin at once, so vtium writes the twin with
//! each first copy it writes. Without that, the frame that passes the end
//! or the start would draw the whole screen. The border is drawn into both
//! copies at once, since it is a thin strip and its copies then never go
//! stale.
//!
//! The copy keeps the ring in system memory, with no twins, and copies the
//! window into a display buffer the screen's size. Both write through the
//! same filters as every other pixel the CPU path writes, and neither
//! reads the display's buffer.
//!
//! The pan and the copy move the renderer's history of drawn cells with
//! the lines, so they read which cells changed in place from the frame's
//! scroll record, and a scroll draws only the rows that came in and the
//! cells written in place.
//!
//! The renderer keeps one history of drawn cells for all of a display's
//! buffers. Since a line's slot is the same in every buffer, a buffer `n`
//! frames old lacks exactly the cells stamped in the last `n` frames, as
//! it does without the ring.
//!
//! References:
//! - ADR-1310 D6: the pan over a tall buffer, the twins drawn only where a window reaches them, the copy, and the window at rest.
//! - ADR-1310 D10: the grid flush to the screen's bottom edge, with the spare lines above it, which show the rows of history above the screen.
//! - ADR-1401 D2: the copy's ring in system memory, in a build with `scroll-pan`.
//! - ADR-1310 D2: the scroll record the ring reads its changes from.

use core::cell::Cell;
use std::time::Instant;

use vtium_font::Face;
use vtium_term::{Frame, LineSize};

use super::{DrawState, Drawn, Grid, Layout, Renderer};
use crate::bounds::{COPY_FRAMES, OVERSCAN_ROWS, TWIN_BUFFERS};
use crate::compose::{Area, int};
use crate::link_row::LinkRow;
use crate::target::{Orientation, Rect, Target};

#[path = "ring/tests.rs"]
#[cfg(test)]
mod tests;

/// Where the ring lies along the axis the text's rows run down: the
/// display's y axis, or its x axis on a display turned on its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geometry {
    /// The rows run along the display's x axis.
    turned: bool,
    /// The grid's rows count up as the buffer's coordinate grows: on a
    /// display that is upright or turned to the left. On the two others
    /// they count up as it falls.
    forward: bool,
    /// A row's length along the axis, in pixels.
    ch: u32,
    /// The ring's slots: one more than the most rows the screen holds.
    slots: u32,
    /// The screen's length along the axis.
    screen: u32,
    /// The display's length across the axis.
    across: u32,
    /// How far the grid's first row lies from the screen's edge where row
    /// numbers start: the spare lines, since the grid's last row touches
    /// the other edge.
    off: u32,
}

impl Geometry {
    /// The ring of a renderer whose layout is `l`, around the grid `g`,
    /// whose screen's rows it holds as they lie on the display.
    fn new(l: &Layout, g: &Grid) -> Geometry {
        let g = &l.screen_of(g);
        let (screen, across) = if l.orientation.turned() {
            (l.width, l.height)
        } else {
            (l.height, l.width)
        };
        let to = |v: i64| u32::try_from(v.max(0)).unwrap_or(0);
        let off = match l.orientation {
            Orientation::Normal => to(g.off_y),
            Orientation::UpsideDown => screen.saturating_sub(to(g.max_y)),
            Orientation::Right => screen.saturating_sub(to(g.max_x)),
            Orientation::Left => to(g.off_x),
        };
        Geometry {
            turned: l.orientation.turned(),
            forward: matches!(l.orientation, Orientation::Normal | Orientation::Left),
            ch: u32::try_from(l.ch).unwrap_or(1).max(1),
            slots: u32::try_from(l.max_rows)
                .unwrap_or(u32::MAX)
                .saturating_add(1),
            screen,
            across,
            off,
        }
    }

    /// The ring's length along the axis.
    fn period(&self) -> u32 {
        self.slots.saturating_mul(self.ch)
    }

    /// The pan's buffer's length along the axis: the ring and a screen of
    /// twins.
    fn extent(&self) -> u32 {
        self.period().saturating_add(self.screen)
    }

    /// The pan's buffer as width and height.
    fn pan_size(&self) -> (u32, u32) {
        match self.turned {
            true => (self.extent(), self.across),
            false => (self.across, self.extent()),
        }
    }

    /// Where the window starts, counted in the rows' direction from the
    /// ring's start, when grid row 0 lies in slot `base`.
    fn window(&self, base: u32) -> u32 {
        base.saturating_mul(self.ch)
    }

    /// Where the window starts in the pan's buffer, along the axis: the
    /// plane's `SRC_Y`, or `SRC_X` on a turned display.
    fn src(&self, base: u32) -> u32 {
        match self.forward {
            true => self.window(base),
            false => self.period().saturating_sub(self.window(base)),
        }
    }

    /// How many lines from the ring's start the window shows as twins:
    /// the part of the window that passes the ring's end.
    fn twins_shown(&self, base: u32) -> u32 {
        self.window(base)
            .saturating_add(self.screen)
            .saturating_sub(self.period())
    }

    /// The slot that line `l` of the ring belongs to, `l` counted in the
    /// rows' direction from the ring's start.
    fn slot_of(&self, l: u32) -> u32 {
        let p = self.period();
        let from = rem(
            rem(l, p).saturating_add(p).saturating_sub(rem(self.off, p)),
            p,
        );
        from.checked_div(self.ch).unwrap_or(0)
    }

    /// How far line `l` of the ring lies into its slot.
    fn depth_in_slot(&self, l: u32) -> u32 {
        let p = self.period();
        let from = rem(
            rem(l, p).saturating_add(p).saturating_sub(rem(self.off, p)),
            p,
        );
        rem(from, self.ch)
    }

    /// Whether slot `s` holds any of the ring's lines from `a` up to `b`.
    fn slot_meets(&self, s: u32, a: u32, b: u32) -> bool {
        let p = self.period();
        let start = rem(self.off.saturating_add(s.saturating_mul(self.ch)), p);
        let end = start.saturating_add(self.ch);
        // the slot's lines, as up to two runs that do not pass the end
        let runs = [(start, end.min(p)), (0, end.saturating_sub(p))];
        runs.iter().any(|&(s0, s1)| s0 < s1 && s0 < b && a < s1)
    }

    /// The buffer coordinate `b` lies in the twins' part of the pan's
    /// buffer.
    fn in_twins(&self, b: u32) -> bool {
        match self.forward {
            true => b >= self.period(),
            false => b < self.screen,
        }
    }

    /// The first copy of the line whose twin lies at `b`.
    fn first_of(&self, b: u32) -> u32 {
        match self.forward {
            true => b.saturating_sub(self.period()),
            false => b.saturating_add(self.period()),
        }
    }

    /// The twin of the line whose first copy lies at `b`, where it has one.
    fn twin_of(&self, b: u32) -> Option<u32> {
        match self.forward {
            true => (b < self.screen).then(|| b.saturating_add(self.period())),
            false => b.checked_sub(self.period()),
        }
    }

    /// The ring's line that the first copy at `b` holds, counted in the
    /// rows' direction.
    fn line_of(&self, b: u32) -> u32 {
        match self.forward {
            true => b,
            false => self.extent().saturating_sub(1).saturating_sub(b),
        }
    }

    /// Where the buffer's coordinates from `b0` up to `b1` change part: at
    /// the ring's end and at the screen's length.
    fn split(&self, b0: u32, b1: u32) -> impl Iterator<Item = (u32, u32)> {
        let mut cuts = [self.screen, self.period()];
        cuts.sort_unstable();
        let mut at = b0;
        let mut points = cuts.into_iter().filter(move |&c| c > b0 && c < b1);
        core::iter::from_fn(move || {
            if at >= b1 {
                return None;
            }
            let next = points.next().unwrap_or(b1);
            let piece = (at, next);
            at = next;
            Some(piece)
        })
    }

    /// Where display coordinate `d` along the axis lies in the copy's
    /// ring, which holds each line once, when grid row 0 lies in slot
    /// `base`.
    fn copy_at(&self, base: u32, d: u32) -> u32 {
        let p = self.period();
        let w = rem(self.window(base), p);
        let c = match self.forward {
            true => w,
            // the ring is laid out in the buffer's direction, so the
            // window's first display line is the last of its lines
            false => rem(
                p.saturating_mul(2)
                    .saturating_sub(w)
                    .saturating_sub(rem(self.screen, p)),
                p,
            ),
        };
        rem(c.saturating_add(rem(d, p)), p)
    }
}

/// What the ring keeps from frame to frame.
#[derive(Debug, Default)]
pub(crate) struct Ring {
    /// The slot that grid row 0 lies in.
    base: u32,
    /// The scroll the next frame shows, and whether it is drawn whole.
    pending: Option<(i64, bool)>,
    /// Whether the frame being drawn reads which cells changed in place,
    /// after its lines moved, rather than at their places on the screen.
    in_place: bool,
    /// Whether the last frame drew the hovered link's row or the link
    /// labels over the screen's cells.
    decorated: bool,
    /// For each buffer the pan remembers, the serial of the frame it holds
    /// and a bit for each slot whose twin is stale in it, then a bit for
    /// each slot whose first copy is stale in it.
    twins: Vec<(u64, Vec<u64>)>,
    /// The frame's damage in the pan's buffer.
    damage: Vec<Rect>,
    /// The rows the frame being drawn moved the lines up, or down when
    /// negative.
    moved: i64,
    /// The serial of the frame that the buffer the pan drew last holds,
    /// when that frame drew nothing and so did not count. A display that
    /// presents the frame anyway, for an overlay, makes it count
    /// ([`Renderer::pan_damage`]).
    uncounted: Option<u64>,
    /// The copy's ring in system memory, empty until the copy draws.
    pixels: Vec<u32>,
    /// The damage of the copy's last frames, the newest last.
    log: Vec<Vec<Rect>>,
    /// The areas the copy wrote into the display's buffer.
    copied: Vec<Rect>,
    /// The frame being planned is drawn on the CPU path, which draws the
    /// rows of history above the screen.
    above: bool,
    /// How many of the grid's rows above the screen exist in the frame
    /// being planned: the rows of history the frame holds above the screen,
    /// the hidden row whose ink reaches down included.
    held: usize,
    /// How many of the rows the spare lines show existed in the frame
    /// planned last.
    existed: Option<usize>,
}

impl Ring {
    /// The frame planned next is drawn on the CPU path, with the rows of
    /// history above the screen.
    pub(crate) fn draw_above(&mut self) {
        self.above = true;
    }

    /// Moves the window to the ring's start, where it holds no twin, and
    /// forgets every buffer's twins: the frame draws every row anyway, so
    /// it reads no change in place.
    fn rest(&mut self) {
        self.base = 0;
        self.in_place = false;
        self.twins.clear();
    }

    /// Whether the frame being drawn reads which cells changed in place.
    pub(crate) fn in_place(&self) -> bool {
        self.in_place
    }

    /// Takes the stale copies of the buffer being drawn, which holds the
    /// frame with serial `held`: a bit for each slot's twin, then one for
    /// each slot's first copy. A new buffer has none, and a buffer the
    /// ring does not remember has every copy stale.
    fn take_twins(&mut self, held: Option<u64>, slots: u32) -> Vec<u64> {
        let words = usize::try_from(slots.saturating_mul(2).div_ceil(64)).unwrap_or(0);
        let mut bits = match held {
            None => Vec::new(),
            Some(h) => match self.twins.iter().position(|&(k, _)| k == h) {
                Some(i) => self.twins.swap_remove(i).1,
                None => vec![u64::MAX; words],
            },
        };
        bits.resize(words, 0);
        bits
    }

    /// Keeps `bits` as the stale twins of the buffer that holds the frame
    /// with serial `key`, and forgets the oldest buffer past
    /// [`TWIN_BUFFERS`].
    fn keep_twins(&mut self, key: u64, bits: Vec<u64>) {
        self.twins.retain(|&(k, _)| k != key);
        self.twins.push((key, bits));
        if self.twins.len() > TWIN_BUFFERS
            && let Some(i) = self
                .twins
                .iter()
                .enumerate()
                .min_by_key(|(_, (k, _))| *k)
                .map(|(i, _)| i)
        {
            self.twins.swap_remove(i);
        }
    }
}

/// `a` modulo `b`, and 0 for a `b` of 0.
fn rem(a: u32, b: u32) -> u32 {
    a.checked_rem(b).unwrap_or(0)
}

fn bit(bits: &[u64], s: u32) -> bool {
    let (w, b) = (usize::try_from(s / 64).unwrap_or(usize::MAX), s % 64);
    bits.get(w).is_some_and(|&v| v & (1 << b) != 0)
}

fn set_bit(bits: &mut [u64], s: u32, on: bool) {
    let (w, b) = (usize::try_from(s / 64).unwrap_or(usize::MAX), s % 64);
    if let Some(v) = bits.get_mut(w) {
        if on {
            *v |= 1 << b;
        } else {
            *v &= !(1 << b);
        }
    }
}

/// The pan's buffer as the renderer sees it: a display the screen's size,
/// whose every write lands at its place in the window over the ring.
pub(crate) struct Pan<'a, T: Target + ?Sized> {
    inner: &'a mut T,
    g: Geometry,
    /// Where the window starts in the buffer, along the axis.
    src: u32,
    /// Set while the renderer draws the border, whose twins are written
    /// with it.
    border: &'a Cell<bool>,
    /// The way the frame moved the lines: up when positive, which moves
    /// the window toward the ring's end, and down when negative, which
    /// moves it toward the ring's start.
    moved: i64,
    /// A bit for each slot whose twin is stale in this buffer, then a bit
    /// for each slot whose first copy is.
    stale: &'a mut [u64],
}

impl<T: Target + ?Sized> Pan<'_, T> {
    /// Writes `pixels` at buffer coordinate `b` along the axis, at `at`
    /// across it.
    fn put(&mut self, b: u32, at: u32, pixels: &[u32]) {
        match self.g.turned {
            true => self.inner.write(b, at, pixels),
            false => self.inner.write(at, b, pixels),
        }
    }

    /// Writes the buffer's coordinates from `b0` up to `b1`, which lie in
    /// one part of the buffer, the first copy and its twin where they need
    /// it.
    fn piece(&mut self, (b0, b1): (u32, u32), at: u32, pixels: &[u32]) {
        self.put(b0, at, pixels);
        if self.g.in_twins(b0) {
            let first = self.g.first_of(b0);
            if self.border.get() || self.moved > 0 {
                // a scroll up moves the window toward the ring's end, past
                // which it shows every first copy again at once, so the
                // first copy is written with the twin rather than all in
                // the one frame that passes the end
                self.put(first, at, pixels);
            } else {
                // the first copy is stale until the slot's row is drawn
                // again for a window that shows it: a screen at rest may
                // rewrite its cells in every frame, and a scroll down shows
                // the first copies of the twins it leaves one row at a time
                let last = self.g.first_of(b1.saturating_sub(1));
                self.mark((first, last.saturating_add(1)), self.g.slots);
            }
            return;
        }
        let Some(twin) = self.g.twin_of(b0) else {
            return;
        };
        if self.border.get() || self.moved < 0 {
            // a scroll down moves the window toward the ring's start, past
            // which it shows every twin at once, so the twin is written
            // with the first copy rather than all in the one frame that
            // passes the start
            self.put(twin, at, pixels);
            return;
        }
        // the window does not show the twin: it is stale until the slot's
        // row is drawn again for a window that does
        self.mark((b0, b1), 0);
    }

    /// Marks stale in this buffer a copy of each slot that holds a line of
    /// the first copies from buffer coordinate `b0` up to `b1`: the twin
    /// with `of` at 0, the first copy with `of` at the ring's slots.
    fn mark(&mut self, (b0, b1): (u32, u32), of: u32) {
        let (l0, l1) = match self.g.forward {
            true => (b0, b1),
            false => (
                self.g.line_of(b1.saturating_sub(1)),
                self.g.line_of(b0).saturating_add(1),
            ),
        };
        let mut l = l0;
        while l < l1 {
            let s = self.g.slot_of(l);
            set_bit(self.stale, of.saturating_add(s), true);
            let into = self.g.depth_in_slot(l);
            l = l.saturating_add(self.g.ch.saturating_sub(into).max(1));
        }
    }
}

impl<T: Target + ?Sized> Target for Pan<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let (d, at, len) = match self.g.turned {
            true => (x, y, pixels.len()),
            false => (y, x, 1),
        };
        if d >= self.g.screen {
            return;
        }
        let len = u32::try_from(len)
            .unwrap_or(u32::MAX)
            .min(self.g.screen.saturating_sub(d));
        let b0 = self.src.saturating_add(d);
        let g = self.g;
        for (p0, p1) in g.split(b0, b0.saturating_add(len)) {
            let run = match self.g.turned {
                true => {
                    let from = usize::try_from(p0.saturating_sub(b0)).unwrap_or(usize::MAX);
                    let to = usize::try_from(p1.saturating_sub(b0)).unwrap_or(usize::MAX);
                    pixels.get(from..to).unwrap_or(&[])
                }
                false => pixels,
            };
            self.piece((p0, p1), at, run);
        }
    }
}

/// What draws over the pan's window in the display's coordinates, as
/// vtium's notices do: each write lands at its place in the window, and in
/// the line's other copy where it has one, as the border's writes do. A
/// line's twin that is not stale then always holds what its first copy
/// holds, whichever copy a later window shows.
struct Over<'a, T: Target + ?Sized> {
    inner: &'a mut T,
    g: Geometry,
    src: u32,
}

impl<T: Target + ?Sized> Target for Over<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let both = Cell::new(true);
        let mut pan = Pan {
            inner: &mut *self.inner,
            g: self.g,
            src: self.src,
            border: &both,
            moved: 0,
            stale: &mut [],
        };
        pan.write(x, y, pixels);
    }
}

/// The copy's ring in system memory, as the renderer sees it: a display
/// the screen's size, whose every write lands at its place in the ring.
struct CopyRing<'a> {
    pixels: &'a mut [u32],
    g: Geometry,
    base: u32,
}

impl Target for CopyRing<'_> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let g = self.g;
        let p = g.period();
        if g.turned {
            if x >= g.screen || y >= g.across {
                return;
            }
            let len = u32::try_from(pixels.len())
                .unwrap_or(u32::MAX)
                .min(g.screen.saturating_sub(x));
            let row = to_usize(y).saturating_mul(to_usize(p));
            let mut done = 0u32;
            while done < len {
                let b = g.copy_at(self.base, x.saturating_add(done));
                let n = len.saturating_sub(done).min(p.saturating_sub(b)).max(1);
                let at = row.saturating_add(to_usize(b));
                let src = pixels.get(to_usize(done)..to_usize(done.saturating_add(n)));
                if let (Some(dst), Some(src)) = (
                    self.pixels
                        .get_mut(at..at.saturating_add(src.map_or(0, <[u32]>::len))),
                    src,
                ) {
                    dst.copy_from_slice(src);
                }
                done = done.saturating_add(n);
            }
        } else {
            if y >= g.screen || x >= g.across {
                return;
            }
            let b = g.copy_at(self.base, y);
            let n = to_usize(g.across.saturating_sub(x)).min(pixels.len());
            let at = to_usize(b)
                .saturating_mul(to_usize(g.across))
                .saturating_add(to_usize(x));
            if let (Some(dst), Some(src)) = (
                self.pixels.get_mut(at..at.saturating_add(n)),
                pixels.get(..n),
            ) {
                dst.copy_from_slice(src);
            }
        }
    }
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Copies the display area `r` of the window over the copy's ring
/// `pixels` into `target`.
fn copy_out(pixels: &[u32], g: Geometry, base: u32, r: Rect, target: &mut impl Target) {
    let p = g.period();
    let (x1, y1) = (r.x.saturating_add(r.width), r.y.saturating_add(r.height));
    for y in r.y..y1 {
        if g.turned {
            let row = to_usize(y).saturating_mul(to_usize(p));
            let mut x = r.x;
            while x < x1.min(g.screen) {
                let b = g.copy_at(base, x);
                let n = x1
                    .min(g.screen)
                    .saturating_sub(x)
                    .min(p.saturating_sub(b))
                    .max(1);
                let at = row.saturating_add(to_usize(b));
                if let Some(src) = pixels.get(at..at.saturating_add(to_usize(n))) {
                    target.write(x, y, src);
                }
                x = x.saturating_add(n);
            }
        } else if y < g.screen {
            let b = g.copy_at(base, y);
            let w = to_usize(r.width.min(g.across.saturating_sub(r.x)));
            let at = to_usize(b)
                .saturating_mul(to_usize(g.across))
                .saturating_add(to_usize(r.x));
            if let Some(src) = pixels.get(at..at.saturating_add(w)) {
                target.write(r.x, y, src);
            }
        }
    }
}

impl<F: Face> Renderer<F> {
    /// The size of the display buffer that [`Renderer::draw_pan`] draws
    /// into, as width and height: the ring of rows, one more than the
    /// screen holds, and one screen more for the twins, along the axis
    /// the rows run down. That is the height on an upright display and the
    /// width on a display turned on its side.
    pub fn pan_buffer_size(&self) -> (u32, u32) {
        Geometry::new(&self.layout, &Grid::default()).pan_size()
    }

    /// The bytes the copy's ring in system memory holds once
    /// [`Renderer::draw_copy`] has drawn: the ring of rows, one more than
    /// the screen holds, at four bytes a pixel.
    pub fn copy_ring_bytes(&self) -> usize {
        let g = Geometry::new(&self.layout, &Grid::default());
        to_usize(g.period())
            .saturating_mul(to_usize(g.across))
            .saturating_mul(4)
    }

    /// Draws `frame` into `target`, a buffer `age` frames old of the size
    /// [`Renderer::pan_buffer_size`] gives, as [`Renderer::draw`] draws.
    /// The screen's lines moved `scrolled` rows up since the frame drawn
    /// last, or down when `scrolled` is negative. A line keeps its pixels
    /// where they lie in the ring, so only the rows that came in and the
    /// cells that changed are drawn. Pass the count the frame's
    /// [`Frame::scrolled`] states. A frame whose record states another
    /// count, or cannot say, is drawn whole. When `whole` says the frame
    /// changes every cell, as a switch between the main and the alternate
    /// screen or a clear does, the window moves back to the ring's start,
    /// where it holds no twin. A jump past the screen's rows and a frame
    /// drawn whole for another reason do too.
    ///
    /// Returns what the frame drew, its damage in the buffer's own
    /// coordinates, and where the window starts in the buffer along the
    /// axis the rows run down: `SRC_Y`, or `SRC_X` on a turned display.
    /// Every frame of a renderer has to be drawn this way once one is, or
    /// the renderer's history forgotten ([`Renderer::forget_history`]).
    ///
    /// References:
    /// - ADR-1310 D6: the pan over a tall buffer and the window at rest.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    pub fn draw_pan(
        &mut self,
        frame: &Frame<'_>,
        target: &mut impl Target,
        age: u32,
        state: &DrawState,
        scrolled: i64,
        whole: bool,
    ) -> (Drawn<'_>, u32) {
        let start = Instant::now();
        #[cfg(feature = "gpu")]
        self.gpu.drew_on_cpu();
        self.ring.pending = Some((scrolled, whole));
        self.ring.uncounted = None;
        self.ring.draw_above();
        let plan = self.plan(frame, age, state);
        let g = Geometry::new(&self.layout, &plan.grid);
        let base = self.ring.base;
        let held = self.cells.held();
        let mut stale = self.ring.take_twins(held, g.slots);
        self.draw_stale_twins(&g, &plan.grid, &mut stale);
        let flag = Cell::new(false);
        let painted = {
            let mut pan = Pan {
                inner: target,
                g,
                src: g.src(base),
                border: &flag,
                moved: self.ring.moved,
                stale: &mut stale,
            };
            self.paint_through(&plan, frame, state, &mut pan, Some(&flag))
        };
        let ((cells, rows), missing) = painted;
        let stats = self.finish(&plan, (cells, rows), missing, start).stats;
        let src = g.src(base);
        self.ring.damage.clear();
        for &r in self.cells.damage() {
            buffer_rects(&g, src, r, &mut self.ring.damage);
        }
        // a frame that drew nothing does not count, and the buffer still
        // holds the frame it held, unless the display presents it anyway
        let key = match self.cells.damage().is_empty() {
            true => {
                self.ring.uncounted = held;
                held
            }
            false => Some(self.cells.serial()),
        };
        if let Some(key) = key {
            self.ring.keep_twins(key, stale);
        }
        (
            Drawn {
                damage: &self.ring.damage,
                stats,
            },
            src,
        )
    }

    /// A target over `target`, the buffer [`Renderer::draw_pan`] drew the
    /// last frame into, that draws in the display's coordinates, as
    /// vtium's notices and the on-screen display do. Each pixel lands
    /// where the window shows that place of the display, and in the line's
    /// other copy too, so whichever copy a later window shows holds it.
    ///
    /// The pixels stay on their lines in the ring, and a scroll moves them
    /// with the lines. So pass each area drawn through this target to
    /// [`Renderer::redraw_area`] after every frame it is drawn over, not
    /// only when the overlay changes or goes: the renderer then stamps the
    /// cells under it, moves those stamps with the lines on a scroll, and
    /// the next frame draws them again wherever the lines went. The area's
    /// damage in the buffer is what [`Renderer::pan_damage`] gives.
    ///
    /// Over the copy, draw straight into the display's buffer instead: the
    /// copy writes the whole window again when it moves.
    ///
    /// References:
    /// - ADR-1310 D6: the pan's two copies of a line, and the window over them.
    pub fn pan_overlay<'a, T: Target + ?Sized>(
        &self,
        target: &'a mut T,
    ) -> impl Target + use<'a, T, F> {
        let g = Geometry::new(&self.layout, &self.grid.unwrap_or_default());
        Over {
            inner: target,
            g,
            src: g.src(self.ring.base),
        }
    }

    /// The areas of the pan's buffer that drawing over the display area
    /// `area` through [`Renderer::pan_overlay`] writes: where the window
    /// shows it, and its other copy where it has one. A display presents
    /// them as damage beside the frame's own, so call this for each area
    /// drawn over a frame the display presents, and only then.
    ///
    /// When the frame drew nothing of its own, it does not count as a
    /// frame: the buffer still holds the frame it held. The display then
    /// presents it for the overlay alone, so this call makes the frame
    /// count after all. The renderer's history and its record of the stale
    /// copies then follow the buffers the display really presents.
    pub fn pan_damage(&mut self, area: Rect) -> Vec<Rect> {
        let g = Geometry::new(&self.layout, &self.grid.unwrap_or_default());
        let (w, h) = (self.layout.width, self.layout.height);
        // only the part of the area on the display is written
        let (x, y) = (area.x.min(w), area.y.min(h));
        let area = Rect {
            x,
            y,
            width: area.width.min(w.saturating_sub(x)),
            height: area.height.min(h.saturating_sub(y)),
        };
        let mut out = Vec::new();
        if area.area() > 0 {
            buffer_rects(&g, g.src(self.ring.base), area, &mut out);
        }
        if !out.is_empty()
            && let Some(held) = self.ring.uncounted.take()
        {
            // both halves keep the frames after this one cheap: without
            // the serial, the next buffer's age names a frame older than
            // it holds; without the new key, this buffer's record waits
            // under a frame no buffer holds. Either way a buffer finds no
            // record of its stale copies, takes every copy as stale and
            // draws the whole screen
            self.cells.count_presented();
            let serial = self.cells.serial();
            if let Some(entry) = self.ring.twins.iter_mut().find(|(k, _)| *k == held) {
                entry.0 = serial;
            }
        }
        out
    }

    /// Draws `frame` into the copy's ring in system memory, then copies
    /// the window over it into `target`, a display buffer `age` frames old
    /// of the screen's size, as [`Renderer::draw`] draws. `scrolled` and
    /// `whole` say what [`Renderer::draw_pan`] says. The copy writes the
    /// areas that changed in the last `age` frames, which is the whole
    /// window when it moved in those frames, and returns them as the
    /// frame's damage. Every frame of a renderer has to be drawn this way
    /// once one is, or the renderer's history forgotten.
    ///
    /// References:
    /// - ADR-1310 D6: the copy, where the driver refuses the pan.
    /// - ADR-1401 D2: the copy keeps its ring in system memory.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    pub fn draw_copy(
        &mut self,
        frame: &Frame<'_>,
        target: &mut impl Target,
        age: u32,
        state: &DrawState,
        scrolled: i64,
        whole: bool,
    ) -> Drawn<'_> {
        let start = Instant::now();
        #[cfg(feature = "gpu")]
        self.gpu.drew_on_cpu();
        let size = self.copy_ring_bytes() / 4;
        // the ring is one buffer that holds every frame drawn into it
        let ring_age = match self.ring.pixels.len() == size {
            true => 1,
            false => {
                self.ring.pixels.clear();
                self.ring.pixels.resize(size, 0);
                self.ring.log.clear();
                0
            }
        };
        self.ring.pending = Some((scrolled, whole));
        let before = self.ring.base;
        self.ring.draw_above();
        let plan = self.plan(frame, ring_age, state);
        let g = Geometry::new(&self.layout, &plan.grid);
        let base = self.ring.base;
        let mut pixels = core::mem::take(&mut self.ring.pixels);
        let painted = {
            let mut ring = CopyRing {
                pixels: &mut pixels,
                g,
                base,
            };
            self.paint_through(&plan, frame, state, &mut ring, None)
        };
        let ((cells, rows), missing) = painted;
        let stats = self.finish(&plan, (cells, rows), missing, start).stats;
        let screen = Rect {
            x: 0,
            y: 0,
            width: self.layout.width,
            height: self.layout.height,
        };
        // the copy's damage log holds one entry for each frame the display
        // swaps in, the newest last; this frame's entry is the area it
        // changed, which may be none, or the whole screen when the window
        // moved, since every pixel of the screen then shows another line
        self.ring.copied.clear();
        let entry = match self.ring.base == before {
            true => self.cells.damage().to_vec(),
            false => vec![screen],
        };
        self.ring.log.push(entry);
        if self.ring.log.len() > COPY_FRAMES {
            self.ring.log.remove(0);
        }
        // a buffer `age` frames old lacks what the last `age` frames
        // changed, and the whole window when one of them moved it
        let n = to_usize(age);
        let log = &self.ring.log;
        let lacking = log.iter().skip(log.len().saturating_sub(n)).flatten();
        if n == 0 || n > log.len() || lacking.clone().any(|&r| r == screen) {
            self.ring.copied.push(screen);
        } else {
            self.ring.copied.extend(lacking.copied());
        }
        if self.ring.copied.is_empty() {
            // nothing to copy: the display keeps the buffer it shows
            self.ring.log.pop();
        }
        for &r in &self.ring.copied {
            copy_out(&pixels, g, base, r, target);
        }
        self.ring.pixels = pixels;
        Drawn {
            damage: &self.ring.copied,
            stats,
        }
    }

    /// The scroll that the frame being planned shows: whether it rests the
    /// window and draws the frame whole, and how many rows its lines
    /// moved. A frame not drawn through the ring moves nothing.
    ///
    /// A frame drawn through the ring moves the history of what was drawn
    /// by those rows, so it reads which cells changed in place, as the
    /// frame's scroll record states them once the lines moved: the rows
    /// that came in and the cells written in place. A one-line scroll then
    /// draws one row, not every row that moved.
    ///
    /// That holds only while the record states the scroll the ring moves
    /// by. When it states another, or cannot say, the lines' pixels would
    /// stand where the record does not put them, and neither the changes
    /// in place nor those at the cells' places on the screen would say
    /// which rows to draw. Such a frame rests the window and draws whole,
    /// so a caller that passes the wrong count costs a redraw and never
    /// shows a wrong picture.
    ///
    /// References:
    /// - ADR-1310 D2: the scroll record, and the changes it states in place.
    pub(super) fn ring_scroll(&mut self, frame: &Frame<'_>, grid: &Grid) -> (bool, i64) {
        self.ring.in_place = false;
        self.ring.moved = 0;
        let Some((n, whole)) = self.ring.pending.take() else {
            return (false, 0);
        };
        let agrees = frame
            .scrolled()
            .is_some_and(|s| i64::try_from(s).is_ok_and(|s| s == n));
        // the hovered link's row and the link labels stand at places on
        // the screen, not on lines, so a scroll under them draws whole
        let decorated = LinkRow::new(frame, grid.cols, grid.rows).is_some();
        let was = core::mem::replace(&mut self.ring.decorated, decorated);
        let rows = u64::try_from(grid.rows).unwrap_or(u64::MAX);
        let moved = n != 0;
        let rest = whole || !agrees || (moved && (n.unsigned_abs() >= rows || decorated || was));
        self.ring.in_place = !rest;
        (rest, n)
    }

    /// The frame was drawn whole: the window rests at the ring's start.
    pub(super) fn ring_rest(&mut self) {
        self.ring.rest();
    }

    /// The grid's lines moved `n` rows up, or down when `n` is negative:
    /// each keeps its slot in the ring, and the renderer's record of what
    /// it drew moves with it.
    ///
    /// What lay over the lines before they moved changes: the areas other
    /// code drew over, and the arrow drawn in software, which stay at
    /// their places on the screen. The cursor's last places move with the
    /// lines, so the cells it was drawn in are drawn again once it is gone.
    /// The border's areas move with the lines too, and the border that the
    /// window shows anew changes ([`Renderer::scroll_border`]).
    pub(super) fn scroll_rows(&mut self, g: &Grid, n: i64) {
        for area in core::mem::take(&mut self.touched) {
            self.stamp_area(g, area);
        }
        if let Some(p) = self.pointer.take() {
            self.stamp_pointer(&p);
        }
        self.cells.rotate(n);
        // a scroll down moves the hidden row above the spare lines into
        // them, and the row the spare lines show in part further down: the
        // ring holds no pixel of a hidden row, and only the part on the
        // display of a row shown in part
        if n < 0 && g.above > 0 {
            let k = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
            let l = self.layout;
            let spare = l.spare_lines(&l.screen_of(g));
            let in_part = g.above > 1 && spare.checked_rem(l.ch).is_some_and(|r| r != 0);
            let last = k
                .saturating_add(1)
                .saturating_add(usize::from(in_part))
                .min(g.rows);
            for row in k..last {
                for col in 0..g.cols {
                    self.cells.stamp(col, row);
                }
            }
        }
        self.scroll_border(g, n);
        let shift = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
        let rows = self.sizes.len();
        let shift_sizes = shift.min(rows);
        if n > 0 {
            self.sizes.rotate_left(shift_sizes);
            if let Some(tail) = self.sizes.get_mut(rows.saturating_sub(shift_sizes)..) {
                tail.fill(LineSize::Single);
            }
        } else {
            self.sizes.rotate_right(shift_sizes);
            if let Some(head) = self.sizes.get_mut(..shift_sizes) {
                head.fill(LineSize::Single);
            }
        }
        let moved = |at: Option<(usize, usize)>| {
            let (col, row) = at?;
            let row = i64::try_from(row).ok()?.checked_sub(n)?;
            let row = usize::try_from(row).ok().filter(|&r| r < g.rows)?;
            Some((col, row))
        };
        self.cursor_at = moved(self.cursor_at);
        self.held_at = moved(self.held_at);
        let slots = i64::from(Geometry::new(&self.layout, g).slots.max(1));
        let base = i64::from(self.ring.base)
            .saturating_add(n.rem_euclid(slots))
            .rem_euclid(slots);
        self.ring.base = u32::try_from(base).unwrap_or(0);
        self.ring.moved = n;
    }

    /// The border after the grid's lines moved `n` rows up, or down when
    /// `n` is negative. The border's areas that the history keeps move
    /// with the lines, as the cells' history does. Two bands across the
    /// screen then change. The lines that the window did not show before,
    /// at the screen's end the lines moved away from, hold whatever the
    /// ring held there. And the spare lines above the grid show the border
    /// where no row of history exists, but a scroll up moves the rows that
    /// leave the screen into them, so the part of the spare lines above the
    /// rows that exist is drawn as border. Every other line keeps its
    /// border pixels as it moves, since the border is one color. The
    /// border skips the rows drawn, so a band that crosses them draws only
    /// its side margins. The arrow drawn in software is stamped on its own.
    ///
    /// References:
    /// - ADR-1310 D10: the spare lines above the grid show the rows of history above the screen, and the border's color where none exists.
    fn scroll_border(&mut self, g: &Grid, n: i64) {
        let l = self.layout;
        let (w, h) = (l.width, l.height);
        let turned = l.orientation.turned();
        let forward = matches!(l.orientation, Orientation::Normal | Orientation::Left);
        // the lines move to lower coordinates when the rows count up as
        // the coordinate grows
        let by = n.saturating_mul(l.ch);
        let by = if forward { by.saturating_neg() } else { by };
        let shift = if turned { (by, 0) } else { (0, by) };
        self.cells.move_border_areas(shift, (w, h));
        let k = usize::try_from(n.unsigned_abs())
            .unwrap_or(usize::MAX)
            .min(g.rows);
        if k == 0 || g.cols == 0 {
            return;
        }
        let screen = i64::from(if turned { w } else { h });
        // the lines the window shows anew: those whose place before the
        // move lay past the screen's end
        let fresh = match by < 0 {
            true => (screen.saturating_add(by), screen),
            false => (0, by),
        };
        let cut = |v: i64| u32::try_from(v.clamp(0, screen)).unwrap_or(0);
        let (from, to) = (cut(fresh.0), cut(fresh.1));
        let band = match turned {
            true => Rect {
                x: from,
                y: 0,
                width: to.saturating_sub(from),
                height: h,
            },
            false => Rect {
                x: 0,
                y: from,
                width: w,
                height: to.saturating_sub(from),
            },
        };
        // the rows of the spare lines where no row of history exists that
        // now hold pixels of a row: a scroll up moves the rows leaving the
        // screen there, and a scroll down moves lines in from above the
        // grid, which the window did not show
        let before = g.above.saturating_sub(self.ring.existed.unwrap_or(0));
        let now = g.above.saturating_sub(self.existing_above(g));
        let bands = match n > 0 {
            true => [(before.saturating_sub(k).max(1), now), (0, 0)],
            false => [(1, k.min(now)), (before.saturating_add(k).max(1), now)],
        };
        let empty = bands.map(|rows| self.strip_band(g, rows));
        for r in [Some(band)].into_iter().chain(empty).flatten() {
            if r.area() > 0 {
                self.cells.stamp_border_area(r);
            }
        }
    }

    /// The band of the spare lines that the grid's rows from `r0` up to
    /// `r1` cover, all across the display; `None` where it holds no pixel.
    fn strip_band(&self, g: &Grid, (r0, r1): (usize, usize)) -> Option<Rect> {
        if r0 >= r1 {
            return None;
        }
        let r = self.layout.rows_rect(g, (r0, r1))?;
        let (w, h) = (self.layout.width, self.layout.height);
        Some(match self.layout.orientation.turned() {
            true => Rect {
                y: 0,
                height: h,
                ..r
            },
            false => Rect {
                x: 0,
                width: w,
                ..r
            },
        })
    }

    /// How many of the rows the spare lines show exist in the frame being
    /// planned.
    fn existing_above(&self, g: &Grid) -> usize {
        self.ring.held.min(g.above.saturating_sub(1))
    }

    /// The grid of the screen's grid `s` that the frame being planned
    /// draws: on the CPU path, with the rows of history above the screen
    /// that the spare lines show, and the one above them for its ink; on
    /// the GPU path, the screen's grid alone. Notes how many of the rows
    /// the spare lines show exist in `frame`.
    ///
    /// References:
    /// - ADR-1310 D10, "Every build": every way of drawing on the CPU path draws the bottom of the row above into the spare lines.
    pub(super) fn with_rows_above(&mut self, frame: &Frame<'_>, s: Grid) -> Grid {
        let cpu = core::mem::take(&mut self.ring.above);
        let g = match cpu {
            true => self.layout.with_rows_above(s),
            false => s,
        };
        self.ring.held = frame.rows_above().min(g.above);
        g
    }

    /// Readies the grid's rows above the screen for the frame being
    /// planned. A row the frame does not hold sends no ink, as no row does
    /// where none exists, so the cells its ink reached are drawn again
    /// without it. When the frame holds more or fewer of the rows the
    /// spare lines show than the frame planned last, those rows and the
    /// spare lines above them are stamped, so each buffer draws the rows
    /// that came and the border where rows went. A frame that `reset` the
    /// history draws everything anyway.
    pub(super) fn rows_above_exist(&mut self, g: &Grid, reset: bool) {
        let missing = g.above.saturating_sub(self.ring.held);
        for row in 0..missing {
            if !self.cells.rows_reach(row, row) {
                continue;
            }
            for col in 0..g.cols {
                self.reach_to(col, row, 1, (true, crate::reach::Reach::NONE));
            }
        }
        let existing = self.existing_above(g);
        let was = self.ring.existed.replace(existing);
        if reset || was == Some(existing) || g.above < 2 {
            return;
        }
        for row in 1..g.above {
            for col in 0..g.cols {
                self.cells.stamp(col, row);
            }
        }
        let band = self.strip_band(g, (1, g.above.saturating_sub(existing)));
        if let Some(r) = band {
            self.cells.stamp_border_area(r);
        }
    }

    /// The display area that the rows of `g` cover where they exist: the
    /// screen's grid and the rows of history the frame holds above it.
    /// The border leaves it. `None` when the screen's grid is empty.
    pub(super) fn drawn_area(&self, g: &Grid) -> Option<Area> {
        if g.cols == 0 || g.rows <= g.above {
            return None;
        }
        let top = g.above.saturating_sub(self.existing_above(g));
        let l = &self.layout;
        Some(l.area(
            g,
            0,
            int(top).saturating_mul(l.ch),
            int(g.cols).saturating_mul(l.cw),
            int(g.rows.saturating_sub(top)).saturating_mul(l.ch),
        ))
    }

    /// The frame's pictures with their rows counted on `g`, which reaches
    /// above the screen: the pictures on every row the frame holds, whole,
    /// when it holds rows beyond the screen, or the screen's. `None` on a
    /// grid of the screen's rows alone.
    #[cfg(feature = "images")]
    pub(super) fn placements_above(
        frame: &Frame<'_>,
        g: &Grid,
    ) -> Option<Vec<vtium_image::FramePlacement>> {
        if g.above == 0 {
            return None;
        }
        let list = match frame.rows_above().saturating_add(frame.rows_below()) {
            0 => frame.placements(),
            _ => frame.placements_beyond(),
        };
        let by = i64::try_from(g.above).unwrap_or(i64::MAX);
        Some(
            list.iter()
                .map(|p| vtium_image::FramePlacement {
                    row: p.row.saturating_add(by),
                    ..p.clone()
                })
                .collect(),
        )
    }

    /// How many rows of history above the screen a frame of a grid of
    /// `grid` columns and rows asks the terminal for: the overscan's 4
    /// rows (the bound `OVERSCAN_ROWS`), or as many as the spare lines above the grid show
    /// where that is more. The terminal hands one row more, for the ink of
    /// the row above the last.
    ///
    /// References:
    /// - ADR-1310 D2: K rows on each side, and one more for the ink of the rows next to them.
    /// - ADR-1310 D10, "A strip taller than a row": the capture holds as many rows above as the spare lines show.
    pub fn rows_above_asked(&self, (cols, rows): (usize, usize)) -> usize {
        let s = self.layout.grid_of(cols, rows);
        self.layout.strip_rows(&s).max(OVERSCAN_ROWS)
    }

    /// Readies the copies of the buffer being drawn, whose stale copies
    /// `stale` marks, for the window this frame shows. A row this frame
    /// draws whole anyway writes the copy the window shows afresh, and
    /// marks the other copy stale as it does, so its marks are cleared. A
    /// row whose stale copy the window shows, its twin or its first copy,
    /// is drawn whole into this buffer, and only this buffer: the other
    /// buffers keep their history.
    fn draw_stale_twins(&mut self, g: &Geometry, grid: &Grid, stale: &mut [u64]) {
        let (rows, above) = (grid.rows, grid.above);
        let base = self.ring.base;
        let shown = g.twins_shown(base);
        // the first copies the window shows, counted in the rows'
        // direction from the ring's start
        let w = g.window(base);
        let firsts = (w, w.saturating_add(g.screen).min(g.period()));
        for s in 0..g.slots {
            // the screen's row in the slot, counted from the screen's first
            // row, and the slots the screen's rows leave above them
            let y = rem(
                s.saturating_add(g.slots).saturating_sub(rem(base, g.slots)),
                g.slots,
            );
            let y = usize::try_from(y).unwrap_or(usize::MAX);
            let screen = rows.saturating_sub(above);
            let row = match y < screen {
                true => Some(y.saturating_add(above)),
                // a row of history above the screen, which the spare lines
                // show; the hidden row above them is never drawn into it
                false => usize::try_from(g.slots)
                    .ok()
                    .and_then(|slots| slots.checked_sub(y))
                    .and_then(|up| above.checked_sub(up))
                    .filter(|&r| r >= 1),
            };
            let Some(row) = row else {
                // a slot no grid row lies in, whose twin is drawn with the
                // border
                continue;
            };
            let copies = [
                (s, shown > 0 && g.slot_meets(s, 0, shown)),
                (
                    g.slots.saturating_add(s),
                    g.slot_meets(s, firsts.0, firsts.1),
                ),
            ];
            for (b, shows) in copies {
                if !bit(stale, b) {
                    continue;
                }
                if self.cells.row_whole_stale(row) {
                    set_bit(stale, b, false);
                } else if shows {
                    self.cells.force_row(row);
                    set_bit(stale, b, false);
                }
            }
        }
    }
}

/// Adds to `out` the pan buffer's areas that the display area `r` was
/// written to, with the window at `src`: where the window shows it, and
/// its other copy where it has one.
fn buffer_rects(g: &Geometry, src: u32, r: Rect, out: &mut Vec<Rect>) {
    let (d0, len) = match g.turned {
        true => (r.x, r.width),
        false => (r.y, r.height),
    };
    let b0 = src.saturating_add(d0);
    let rect = |b: u32, n: u32| match g.turned {
        true => Rect {
            x: b,
            width: n,
            ..r
        },
        false => Rect {
            y: b,
            height: n,
            ..r
        },
    };
    for (p0, p1) in g.split(b0, b0.saturating_add(len)) {
        let n = p1.saturating_sub(p0);
        out.push(rect(p0, n));
        let other = match g.in_twins(p0) {
            true => Some(g.first_of(p0)),
            false => g.twin_of(p0),
        };
        if let Some(o) = other {
            out.push(rect(o, n));
        }
    }
}
