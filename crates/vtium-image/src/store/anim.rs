//! Kitty animation on the store's side (ADR-1102): how an image's frames
//! are kept and charged to the frame allowance, how they are composed into
//! whole pixels, and the state the commands record for the animation clock.
//!
//! An image's own pixels are its first frame, the *root frame*. The store
//! keeps each later frame in one of two kinds. A *whole frame* holds every
//! pixel of the image. A *change frame* holds its *base*, an earlier frame
//! or a color, and its *layers*, the rectangles of pixels the program sent,
//! each with its offset and its blend, in the order they arrived. The
//! store composes a frame into whole pixels when its command arrives when
//! one of D1's three rules holds: the image's whole frame is at most
//! `FRAME_WHOLE_MAX` bytes, the frame's layers cover half the picture or
//! more, or composing it would apply more than `FRAME_CHAIN_MAX` layers.
//! Before a command changes a frame that change frames are laid on, the
//! store composes each of those frames into a whole frame, so a frame
//! never changes when another one does.
//!
//! Frames past the root are charged to the console's frame allowance,
//! `FRAME_ALLOWANCE_FACTOR` times its image budget, and never to the image
//! budget. Every command reserves its whole cost before it allocates a
//! pixel, and a command the allowance refuses changes nothing.
//!
//! The store records what the commands say: the frames, their gaps, the run
//! state, the loops, the current frame and two stamps. It never records
//! which frame shows. The binary's animation clock picks that, and its
//! snapshot asks [`Store::frame_pixels`] for the chosen frame's whole
//! pixels. A change frame composed for a snapshot goes into a small cache
//! of `FRAME_CACHE_FRAMES` frames for each image, which nothing the store
//! compares, prints, answers or decides reads.

use core::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::{Image, Reservation, Store, all_opaque};
use crate::bounds::{
    FRAME_ALLOWANCE_FACTOR, FRAME_CACHE_FRAMES, FRAME_CHAIN_MAX, FRAME_OVERHEAD, FRAME_WHOLE_MAX,
    FRAMES_MAX,
};
use crate::decode::convert::premultiply;
use crate::image::{ImageKey, Pixels, Refusal};

#[cfg(test)]
mod tests;

/// Whether an animation plays, as the last `a=a` with `s` set it (D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Run {
    /// `s=1`, and the state of an animation that no command has run.
    #[default]
    Stopped,
    /// `s=2`: it plays, and waits on its last frame for more to arrive.
    Loading,
    /// `s=3`: it plays, and goes back to the first frame after the last.
    Looping,
}

/// What the store records of an image's animation, for the binary's
/// animation clock (ADR-1102, D3 and D4). The clock tells the commands
/// apart by the two stamps: `frame_stamp` moves when a command makes a
/// frame current, and `run_stamp` when a command sets the run state or the
/// loops; both move when the frames are deleted or freed, which also sets
/// `current` to 1 and `run` to stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Animation {
    /// The frames the image holds, its root frame included.
    pub frames: u32,
    /// The frame a command last made current, from 1.
    pub current: u32,
    pub run: Run,
    /// The loops `v` asked for: 1 plays forever, and a larger number plays
    /// that many less one.
    pub loops: u32,
    pub frame_stamp: u64,
    pub run_stamp: u64,
}

/// One frame's whole pixels, as a snapshot shows them, with the three values
/// that name them together with the image's key: the frame's number and its
/// pixel revision (ADR-1102, D3).
#[derive(Clone, Debug, PartialEq)]
pub struct FramePixels {
    /// The frame's number, 1 for the root frame.
    pub frame: u32,
    /// The store's revision when a command last changed what the frame
    /// shows.
    pub revision: u64,
    /// No pixel of the frame is below full alpha, as far as the store knows
    /// without composing it.
    pub opaque: bool,
    pub pixels: Pixels,
}

/// What a change frame is laid on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base {
    /// A canvas of one premultiplied color.
    Color(u32),
    /// An earlier frame, by number.
    Frame(u32),
}

/// A rectangle of premultiplied pixels laid on a canvas at its offset.
#[derive(Clone, Debug, PartialEq)]
struct Layer {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// The pixels replace the canvas's, where otherwise they blend over it.
    replace: bool,
    pixels: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Whole(Pixels),
    Change { base: Base, layers: Vec<Layer> },
}

/// A frame past the root.
#[derive(Clone, Debug, PartialEq)]
struct Frame {
    kind: Kind,
    /// Milliseconds it shows; negative for a gapless frame.
    gap: i32,
    opaque: bool,
    /// Its pixel revision.
    revision: u64,
}

/// The composed frames kept for one image, keyed by frame number and pixel
/// revision. It is not part of the store's state: two stores compare equal
/// whatever their caches hold, and a store's clone takes a copy.
#[derive(Default)]
struct Cache(Mutex<Vec<(u32, u64, Pixels)>>);

impl Cache {
    fn lock(&self) -> MutexGuard<'_, Vec<(u32, u64, Pixels)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Frame `frame` at `revision`, which becomes the frame asked for last.
    /// The image's current frame, which every frame entry asks for, then
    /// stays while the clock shows other frames, so a snapshot composes it
    /// once.
    fn get(&self, frame: u32, revision: u64) -> Option<Pixels> {
        let mut cache = self.lock();
        let at = cache
            .iter()
            .position(|(f, r, _)| (*f, *r) == (frame, revision))?;
        let hit = cache.remove(at);
        let pixels = hit.2.clone();
        cache.push(hit);
        Some(pixels)
    }

    /// Keeps `pixels` as frame `frame` at `revision`, dropping the frame
    /// asked for longest ago past the bound.
    fn put(&self, frame: u32, revision: u64, pixels: Pixels) {
        let mut cache = self.lock();
        cache.retain(|(f, _, _)| *f != frame);
        cache.push((frame, revision, pixels));
        let bound = usize::try_from(FRAME_CACHE_FRAMES).unwrap_or(usize::MAX);
        while cache.len() > bound {
            cache.remove(0);
        }
    }

    fn forget(&self, frame: u32) {
        self.lock().retain(|(f, _, _)| *f != frame);
    }

    fn clear(&self) {
        self.lock().clear();
    }

    fn len(&self) -> usize {
        self.lock().len()
    }
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.lock().clone()))
    }
}

impl PartialEq for Cache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cache({} frames)", self.len())
    }
}

/// An image's animation: its frames past the root and what the commands
/// set.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Anim {
    /// Frames 2 and up, in order.
    frames: Vec<Frame>,
    /// The root frame's gap, 0 until a command sets it.
    root_gap: i32,
    run: Run,
    loops: u32,
    current: u32,
    frame_stamp: u64,
    run_stamp: u64,
    /// Whether the commands alone tell that the clock shows `current`: true
    /// while the animation is stopped on a frame a command named, and false
    /// once a command sets it running, until a command names the frame on
    /// show again. It decides which frame edits are damage, and never what
    /// the replay prints.
    shows_current: bool,
    /// What the frame allowance holds for it: each frame's cost, and the
    /// cache's full bound while it holds a change frame.
    charge: u64,
    cache: Cache,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            root_gap: 0,
            run: Run::Stopped,
            loops: 1,
            current: 1,
            frame_stamp: 0,
            run_stamp: 0,
            shows_current: true,
            charge: 0,
            cache: Cache::default(),
        }
    }
}

/// How one frame past the root is charged, for a plan's cost.
#[derive(Clone, Copy, Debug)]
struct Shape {
    change: bool,
    cost: u64,
}

/// An image's sizes, as its frames' charges use them.
#[derive(Clone, Copy, Debug)]
struct Size {
    width: u32,
    height: u32,
}

impl Size {
    fn of(image: &Image) -> Self {
        Self {
            width: image.width,
            height: image.height,
        }
    }

    fn pixels(self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height))
    }

    fn len(self) -> usize {
        usize::try_from(self.pixels()).unwrap_or(usize::MAX)
    }

    /// A whole frame's bytes, without its overhead.
    fn bytes(self) -> u64 {
        self.pixels().saturating_mul(4)
    }

    /// What a whole frame costs.
    fn whole(self) -> u64 {
        self.bytes().saturating_add(FRAME_OVERHEAD)
    }

    /// What the cache's full bound costs.
    fn cache(self) -> u64 {
        self.bytes().saturating_mul(FRAME_CACHE_FRAMES)
    }
}

/// What a layer of `width` by `height` pixels costs.
pub(crate) fn layer_cost(width: u32, height: u32) -> u64 {
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4)
        .saturating_add(FRAME_OVERHEAD)
}

impl Anim {
    /// What the frame allowance holds for `image`.
    pub(super) fn charge_of(image: &Image) -> u64 {
        image.anim.as_ref().map_or(0, |a| a.charge)
    }

    fn count(&self) -> u32 {
        u32::try_from(self.frames.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }

    fn frame(&self, n: u32) -> Option<&Frame> {
        let i = usize::try_from(n.checked_sub(2)?).ok()?;
        self.frames.get(i)
    }

    fn frame_mut(&mut self, n: u32) -> Option<&mut Frame> {
        let i = usize::try_from(n.checked_sub(2)?).ok()?;
        self.frames.get_mut(i)
    }

    fn exists(&self, n: u32) -> bool {
        n >= 1 && n <= self.count()
    }

    pub(super) fn state(&self) -> Animation {
        Animation {
            frames: self.count(),
            current: self.current,
            run: self.run,
            loops: self.loops,
            frame_stamp: self.frame_stamp,
            run_stamp: self.run_stamp,
        }
    }

    /// Whether frame `n` may be the frame the image's placements show. The
    /// clock shows the current frame until a command sets the animation
    /// running. From then on the clock picks the frame, and the store cannot
    /// tell which one shows, so every frame may. A stop alone keeps the
    /// frame the clock was showing, which the store still cannot tell. The
    /// store knows the frame on show again after `c` while stopped, `c` with
    /// `s=1` in one command, which makes the frame current and then stops on
    /// it, and frames deleted or freed, which show the root, stopped.
    ///
    /// References: ADR-1102 D3, D4 and D5.
    fn may_show(&self, n: u32) -> bool {
        !self.shows_current || n == self.current
    }

    /// Whether every frame past the root is opaque.
    pub(super) fn opaque(&self) -> bool {
        self.frames.iter().all(|f| f.opaque)
    }

    /// The layers composing frame `n` applies, counted from the nearest
    /// whole frame or color beneath it.
    fn depth(&self, n: u32) -> u64 {
        let mut depth = 0u64;
        let mut at = n;
        while let Some(Frame {
            kind: Kind::Change { base, layers },
            ..
        }) = self.frame(at)
        {
            depth = depth.saturating_add(u64::try_from(layers.len()).unwrap_or(u64::MAX));
            match *base {
                Base::Frame(k) if k < at => at = k,
                _ => break,
            }
        }
        depth
    }

    /// The change frames laid directly on frame `n`.
    fn laid_on(&self, n: u32) -> Vec<u32> {
        self.frames
            .iter()
            .zip(2u32..)
            .filter(|(f, _)| matches!(f.kind, Kind::Change { base: Base::Frame(k), .. } if k == n))
            .map(|(_, i)| i)
            .collect()
    }

    fn shapes(&self, size: Size) -> Vec<Shape> {
        self.frames
            .iter()
            .map(|f| match &f.kind {
                Kind::Whole(_) => Shape {
                    change: false,
                    cost: size.whole(),
                },
                Kind::Change { layers, .. } => Shape {
                    change: true,
                    cost: layers
                        .iter()
                        .map(|l| layer_cost(l.width, l.height))
                        .fold(0u64, u64::saturating_add),
                },
            })
            .collect()
    }

    /// The charge of frames shaped as `shapes`: each frame's cost, and the
    /// cache's bound while any is a change frame.
    fn total(shapes: &[Shape], size: Size) -> u64 {
        let frames = shapes
            .iter()
            .map(|s| s.cost)
            .fold(0u64, u64::saturating_add);
        if shapes.iter().any(|s| s.change) {
            frames.saturating_add(size.cache())
        } else {
            frames
        }
    }

    /// Whether frame `n`, or a color, is opaque as a base.
    fn base_opaque(&self, base: Base, root_opaque: bool) -> bool {
        match base {
            Base::Color(c) => c >> 24 == 0xff,
            Base::Frame(1) => root_opaque,
            Base::Frame(k) => self.frame(k).is_some_and(|f| f.opaque),
        }
    }

    /// Frame `n`'s whole pixels, composed from its base and its layers.
    fn compose(&self, root: &[u32], n: u32, size: Size) -> Vec<u32> {
        let mut chain: Vec<&[Layer]> = Vec::new();
        let mut at = n;
        let mut canvas = loop {
            let Some(f) = self.frame(at) else {
                break root.to_vec();
            };
            match &f.kind {
                Kind::Whole(p) => break p.as_slice().to_vec(),
                Kind::Change { base, layers } => {
                    chain.push(layers);
                    match *base {
                        Base::Color(c) => break vec![c; size.len()],
                        Base::Frame(k) if k < at => at = k,
                        Base::Frame(_) => break vec![0; size.len()],
                    }
                }
            }
        };
        for layers in chain.iter().rev() {
            for l in layers.iter() {
                lay(&mut canvas, size.width, l);
            }
        }
        canvas
    }

    /// The pixels composing frame `n` writes: the base's copy and each
    /// layer's pixels.
    fn compose_work(&self, n: u32, size: Size) -> u64 {
        let mut work = size.pixels();
        let mut at = n;
        while let Some(Frame {
            kind: Kind::Change { base, layers },
            ..
        }) = self.frame(at)
        {
            for l in layers {
                work = work.saturating_add(u64::from(l.width).saturating_mul(u64::from(l.height)));
            }
            match *base {
                Base::Frame(k) if k < at => at = k,
                _ => break,
            }
        }
        work
    }
}

/// Premultiplied "source over" in whole numbers, per channel:
/// `src + (dst × (255 − a) + 127) / 255`, the division dropping its
/// remainder (ADR-1102, D2).
pub(crate) fn over(src: u32, dst: u32) -> u32 {
    let inv = 255u32.saturating_sub(src >> 24);
    let mut out = 0u32;
    for shift in [0u32, 8, 16, 24] {
        let s = src.wrapping_shr(shift) & 0xff;
        let d = dst.wrapping_shr(shift) & 0xff;
        let blended = d
            .saturating_mul(inv)
            .saturating_add(127)
            .checked_div(255)
            .unwrap_or(0);
        out |= s.saturating_add(blended).min(255).wrapping_shl(shift);
    }
    out
}

/// Copies or blends the `w` by `h` rectangle at `from` of `src`, `src_w`
/// pixels wide, onto the rectangle at `to` of `dst`, `dst_w` pixels wide.
/// Rows that fall outside either buffer are left out, which a checked
/// rectangle never meets.
#[expect(
    clippy::too_many_arguments,
    reason = "two buffers, their widths, two corners, a size and a blend"
)]
fn blit(
    dst: &mut [u32],
    dst_w: u32,
    to: (u32, u32),
    src: &[u32],
    src_w: u32,
    from: (u32, u32),
    (w, h): (u32, u32),
    replace: bool,
) {
    let at = |x: u32, y: u32, width: u32| {
        usize::try_from(
            u64::from(y)
                .saturating_mul(u64::from(width))
                .saturating_add(u64::from(x)),
        )
        .unwrap_or(usize::MAX)
    };
    let w = usize::try_from(w).unwrap_or(0);
    for row in 0..h {
        let d = at(to.0, to.1.saturating_add(row), dst_w);
        let s = at(from.0, from.1.saturating_add(row), src_w);
        let (Some(dst), Some(src)) = (
            dst.get_mut(d..d.saturating_add(w)),
            src.get(s..s.saturating_add(w)),
        ) else {
            continue;
        };
        if dst.len() != src.len() {
            continue;
        }
        if replace {
            dst.copy_from_slice(src);
        } else {
            for (d, &s) in dst.iter_mut().zip(src) {
                *d = over(s, *d);
            }
        }
    }
}

/// Lays `layer` onto `canvas`, an image `width` pixels wide.
fn lay(canvas: &mut [u32], width: u32, layer: &Layer) {
    blit(
        canvas,
        width,
        (layer.x, layer.y),
        layer.pixels.as_slice(),
        layer.width,
        (0, 0),
        (layer.width, layer.height),
        layer.replace,
    );
}

/// Takes `pixels` as a buffer to write: the same allocation when nothing
/// else holds it, such as a frame a display still draws, else a copy.
fn writable(pixels: Pixels) -> Vec<u32> {
    pixels
        .into_vec()
        .unwrap_or_else(|held| held.as_slice().to_vec())
}

/// A frame's data, `a=f` (ADR-1102, D2): the rectangle `width` by `height`
/// at `x`, `y` and its premultiplied pixels, laid by replacement or by
/// blending onto the canvas the keys name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewFrame {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u32>,
    /// Every pixel of the data is opaque.
    pub(crate) opaque: bool,
    /// `X=1`.
    pub(crate) replace: bool,
    /// `c`: the frame a new frame starts from, 0 for the color.
    pub(crate) base: u32,
    /// `Y`: the canvas's color, 32-bit RGBA with straight alpha.
    pub(crate) background: u32,
    /// `r`: the frame to edit, 0 for a new frame.
    pub(crate) edit: u32,
    /// `z`: the gap; 0 keeps the gap the frame has, 40 for a new frame.
    pub(crate) gap: i32,
}

/// A composition, `a=c` (ADR-1102, D2): the `width` by `height` rectangle at
/// `src_x`, `src_y` of frame `src` laid onto the one at `dst_x`, `dst_y` of
/// frame `dst`. A width or height of 0 takes the image's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Compose {
    pub(crate) src: u32,
    pub(crate) dst: u32,
    pub(crate) src_x: u32,
    pub(crate) src_y: u32,
    pub(crate) dst_x: u32,
    pub(crate) dst_y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// `C=1`.
    pub(crate) replace: bool,
}

/// An animation control, `a=a` (ADR-1102, D3 and D4). Each 0 is a key not
/// given, or one the spec says to ignore.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Control {
    /// `c`: the frame to make current.
    pub(crate) current: u32,
    /// `s`: 1 stops, 2 runs in loading mode, 3 runs and loops.
    pub(crate) run: u32,
    /// `v`: the loops.
    pub(crate) loops: u32,
    /// `r` and `z`: the frame whose gap `gap` sets.
    pub(crate) frame: u32,
    pub(crate) gap: i32,
}

/// The gap a new frame has when `z` sets none.
const DEFAULT_GAP: i32 = 40;

/// A rectangle `w` by `h` at `x`, `y` lies inside an image of `size`.
fn inside(size: Size, x: u32, y: u32, w: u32, h: u32) -> bool {
    let fits =
        |o: u32, l: u32, side: u32| u64::from(o).saturating_add(u64::from(l)) <= u64::from(side);
    w > 0 && h > 0 && fits(x, w, size.width) && fits(y, h, size.height)
}

impl Store {
    /// The frame allowance: [`FRAME_ALLOWANCE_FACTOR`] times the image
    /// budget, 0 when the budget is.
    pub fn frame_limit(&self) -> u64 {
        self.limit.saturating_mul(FRAME_ALLOWANCE_FACTOR)
    }

    /// The bytes the frame allowance has granted: every animation's frames,
    /// their caches' bounds, and the frame reservations not yet spent.
    pub fn frame_bytes(&self) -> u64 {
        self.frame_used
    }

    /// An image's animation state, in either buffer, once a command gave it
    /// frames or set its playing; `None` for a picture no animation command
    /// named.
    pub fn animation(&self, image: ImageKey) -> Option<Animation> {
        self.any_image(image)?.anim.as_deref().map(Anim::state)
    }

    /// Frame `frame`'s gap in milliseconds, negative for a gapless frame, of
    /// image `image` in either buffer.
    pub fn frame_gap(&self, image: ImageKey, frame: u32) -> Option<i32> {
        let image = self.any_image(image)?;
        let anim = image.anim.as_deref();
        match frame {
            1 => Some(anim.map_or(0, |a| a.root_gap)),
            n => Some(anim?.frame(n)?.gap),
        }
    }

    /// Frame `frame`'s whole pixels of image `image`, in either buffer, for
    /// a head's snapshot (ADR-1102, D3): the root frame or a whole frame as
    /// the store holds it, a change frame from the image's cache, or a
    /// change frame composed now and kept in the cache. Composing reads at
    /// most a copy of the base and `FRAME_CHAIN_MAX` layers, and changes
    /// nothing the store compares, prints or decides. `None` for an image
    /// or a frame that does not exist.
    pub fn frame_pixels(&self, image: ImageKey, frame: u32) -> Option<FramePixels> {
        let img = self.any_image(image)?;
        if frame == 1 {
            return Some(FramePixels {
                frame: 1,
                revision: img.revision,
                opaque: img.opaque,
                pixels: img.pixels.clone(),
            });
        }
        let anim = img.anim.as_deref()?;
        let f = anim.frame(frame)?;
        let pixels = match &f.kind {
            Kind::Whole(p) => p.clone(),
            Kind::Change { .. } => match anim.cache.get(frame, f.revision) {
                Some(p) => p,
                None => {
                    let p = Pixels::new(anim.compose(img.pixels.as_slice(), frame, Size::of(img)));
                    anim.cache.put(frame, f.revision, p.clone());
                    p
                }
            },
        };
        Some(FramePixels {
            frame,
            revision: f.revision,
            opaque: f.opaque,
            pixels,
        })
    }

    fn any_image(&self, key: ImageKey) -> Option<&Image> {
        self.buffers.iter().find_map(|b| b.images.get(&key))
    }

    /// A command changed the pixels of frame `n` of image `key` of buffer
    /// `b`: when that frame may be the one the image's placements show, it
    /// reports each of them as changed, so the terminal marks the cells
    /// under them, as it does for a placement whose image changes.
    fn frame_changed(&mut self, b: usize, key: ImageKey, n: u32) {
        let shows = self
            .image(b, key)
            .is_some_and(|image| match image.anim.as_deref() {
                Some(anim) => anim.may_show(n),
                None => n == 1,
            });
        if shows {
            self.mark_image(b, key);
        }
    }

    /// Checks a frame command before its data arrives, on image `key` of
    /// the buffer in use: the rectangle `w` by `h` at `x`, `y` lies inside
    /// the image (EINVAL), frame `edit` or else frame `base` exists
    /// (ENOENT), and a new frame stays within [`FRAMES_MAX`] (ENOSPC).
    pub(crate) fn check_frame(
        &self,
        key: ImageKey,
        (x, y, w, h): (u32, u32, u32, u32),
        base: u32,
        edit: u32,
    ) -> Result<(), Refusal> {
        let image = self.image(self.active(), key).ok_or(Refusal::NotFound)?;
        let count = image.anim.as_deref().map_or(1, Anim::count);
        let exists = |n: u32| n >= 1 && n <= count;
        if edit != 0 && !exists(edit) || edit == 0 && base != 0 && !exists(base) {
            return Err(Refusal::NotFound);
        }
        if !inside(Size::of(image), x, y, w, h) {
            return Err(Refusal::Invalid);
        }
        if edit == 0 && u64::from(count) >= FRAMES_MAX {
            return Err(Refusal::NoSpace);
        }
        Ok(())
    }

    /// Reserves `bytes` of the frame allowance for a frame's data on image
    /// `key` of the buffer in use, before the data is allocated, and holds
    /// the image until the reservation is spent (ADR-1102, D1).
    pub(crate) fn frame_reserve(
        &mut self,
        key: ImageKey,
        bytes: u64,
    ) -> Result<Reservation, Refusal> {
        let b = self.active();
        if self.image(b, key).is_none() {
            return Err(Refusal::NotFound);
        }
        self.reserve_frames(bytes, (b, key))?;
        Ok(self.grant_frames(bytes, key, b))
    }

    /// Reserves `bytes` of the frame allowance. When they do not fit, it
    /// first counts what freeing the frames of every other animated image
    /// would give back: when that falls short it frees nothing and refuses
    /// with ENOSPC, and otherwise it frees whole animations, least recently
    /// used first, until the bytes fit. The frames of image `own` and every
    /// root frame stay.
    pub(super) fn reserve_frames(
        &mut self,
        bytes: u64,
        own: (usize, ImageKey),
    ) -> Result<(), Refusal> {
        let limit = self.frame_limit();
        if self.frame_used.saturating_add(bytes) <= limit {
            self.frame_used = self.frame_used.saturating_add(bytes);
            return Ok(());
        }
        let mut others: Vec<(u64, usize, ImageKey)> = Vec::new();
        let mut freeable = 0u64;
        let mut visited = 0usize;
        for (b, buf) in self.buffers.iter().enumerate() {
            visited = visited.saturating_add(buf.images.len());
            for (&k, image) in &buf.images {
                let charge = Anim::charge_of(image);
                if charge > 0 && (b, k) != own {
                    others.push((image.used_at, b, k));
                    freeable = freeable.saturating_add(charge);
                }
            }
        }
        self.count(visited);
        if self
            .frame_used
            .saturating_sub(freeable)
            .saturating_add(bytes)
            > limit
        {
            return Err(Refusal::NoSpace);
        }
        others.sort_unstable();
        for (_, b, k) in others {
            if self.frame_used.saturating_add(bytes) <= limit {
                break;
            }
            self.drop_frames(b, k);
        }
        self.frame_used = self.frame_used.saturating_add(bytes);
        Ok(())
    }

    /// Frees every frame of image `key` of buffer `b` but its root, and its
    /// cache, giving back their allowance; its current frame becomes the
    /// root and its run state stopped, and both its stamps move (D1, D5).
    fn drop_frames(&mut self, b: usize, key: ImageKey) {
        let Some(anim) = self.image_mut(b, key).and_then(|i| i.anim.as_deref_mut()) else {
            return;
        };
        let charge = core::mem::take(&mut anim.charge);
        let visited = anim.frames.len();
        anim.frames.clear();
        anim.cache.clear();
        anim.current = 1;
        anim.run = Run::Stopped;
        anim.shows_current = true;
        anim.frame_stamp = anim.frame_stamp.saturating_add(1);
        anim.run_stamp = anim.run_stamp.saturating_add(1);
        self.frame_used = self.frame_used.saturating_sub(charge);
        self.count(visited);
    }

    /// `d=f` and `d=F` (ADR-1102, D5): every frame of image `key` of the
    /// buffer in use but its root goes. An image no animation command named
    /// is left as it is.
    pub(crate) fn delete_frames(&mut self, key: ImageKey) {
        let b = self.active();
        self.drop_frames(b, key);
    }

    /// Settles image `key`'s charge as it goes from `before` to `after`,
    /// where the allowance already holds `held` for the command: reserves
    /// what more it needs, or gives back what it no longer does. Refused,
    /// nothing changes. An image that takes a charge takes an animation.
    fn settle_frames(
        &mut self,
        (b, key): (usize, ImageKey),
        before: u64,
        after: u64,
        held: u64,
    ) -> Result<(), Refusal> {
        let have = before.saturating_add(held);
        if after > have {
            self.reserve_frames(after.saturating_sub(have), (b, key))?;
        } else {
            self.frame_used = self.frame_used.saturating_sub(have.saturating_sub(after));
        }
        if let Some(image) = self.image_mut(b, key)
            && (image.anim.is_some() || after > 0)
        {
            image.anim.get_or_insert_with(Box::default).charge = after;
        }
        Ok(())
    }

    /// Composes each change frame in `frames` of image `key` of buffer `b`
    /// into a whole frame, keeping what it shows and its pixel revision.
    fn make_whole(&mut self, b: usize, key: ImageKey, frames: &[u32]) {
        let mut work = 0u64;
        if let Some(image) = self.image_mut(b, key) {
            let size = Size::of(image);
            let root = image.pixels.clone();
            if let Some(anim) = image.anim.as_deref_mut() {
                for &n in frames {
                    if !matches!(
                        anim.frame(n),
                        Some(Frame {
                            kind: Kind::Change { .. },
                            ..
                        })
                    ) {
                        continue;
                    }
                    work = work.saturating_add(anim.compose_work(n, size));
                    let pixels = anim.compose(root.as_slice(), n, size);
                    anim.cache.forget(n);
                    if let Some(f) = anim.frame_mut(n) {
                        f.kind = Kind::Whole(Pixels::new(pixels));
                    }
                }
            }
        }
        self.add_work(work);
    }

    /// Stores a frame's data, `a=f`, on the image whose frame reservation
    /// `reservation` is, and returns the frame's number (ADR-1102, D1, D2).
    /// The reservation holds the data's bytes; the command reserves the rest
    /// of its cost before it allocates anything. Refused, the animation is
    /// as it was and the reservation is given back.
    pub(crate) fn add_frame(
        &mut self,
        new: NewFrame,
        reservation: Reservation,
    ) -> Result<u32, Refusal> {
        let (b, key) = (reservation.buffer, reservation.old);
        let held = if reservation.frames {
            self.hold(b, key, false);
            self.spend(&reservation).unwrap_or(0)
        } else {
            self.unreserve(reservation);
            0
        };
        let out = match key {
            Some(key) if b == self.active() => self.add_frame_held(b, key, new, held),
            _ => Err(Refusal::NotFound),
        };
        if out.is_err() {
            self.frame_used = self.frame_used.saturating_sub(held);
        }
        out
    }

    /// [`add_frame`](Self::add_frame) once the reservation is spent, its
    /// `held` bytes still counted by the allowance.
    fn add_frame_held(
        &mut self,
        b: usize,
        key: ImageKey,
        mut new: NewFrame,
        held: u64,
    ) -> Result<u32, Refusal> {
        self.check_frame(
            key,
            (new.x, new.y, new.width, new.height),
            new.base,
            new.edit,
        )?;
        let area = u64::from(new.width).saturating_mul(u64::from(new.height));
        if u64::try_from(new.pixels.len()).ok() != Some(area) {
            return Err(Refusal::Invalid);
        }
        let plan = {
            let image = self.image(b, key).ok_or(Refusal::NotFound)?;
            let empty = Anim::default();
            let anim = image.anim.as_deref().unwrap_or(&empty);
            Plan::frame(anim, Size::of(image), &new, area)
        };
        self.count(plan.visited);
        self.settle_frames((b, key), plan.before, plan.after, held)?;
        // the whole cost is reserved: compose, then store
        self.make_whole(b, key, &plan.laid);
        let revision = self.bump();
        let layer = Layer {
            x: new.x,
            y: new.y,
            width: new.width,
            height: new.height,
            replace: new.replace,
            pixels: Pixels::new(core::mem::take(&mut new.pixels)),
        };
        let image = self.image_mut(b, key).ok_or(Refusal::NotFound)?;
        let size = Size::of(image);
        let (number, work) = if new.edit == 1 {
            // the root frame is the image's own pixels, always whole
            let mut pixels = writable(core::mem::replace(
                &mut image.pixels,
                Pixels::new(Vec::new()),
            ));
            lay(&mut pixels, size.width, &layer);
            image.opaque = all_opaque(&pixels);
            image.pixels = Pixels::new(pixels);
            image.revision = revision;
            // z sets the gap of the frame an edit changes, the root's too
            if new.gap != 0 {
                image.anim.get_or_insert_with(Box::default).root_gap = new.gap;
            }
            (1, area.saturating_add(size.pixels()))
        } else {
            let root = image.pixels.clone();
            let root_opaque = image.opaque;
            let anim = image.anim.get_or_insert_with(Box::default);
            if new.edit == 0 {
                anim.push(&root, size, &new, layer, plan.whole, root_opaque, revision)
            } else {
                anim.edit(&root, size, &new, layer, plan.whole, revision)
            }
        };
        self.add_work(work);
        if new.edit != 0 {
            self.frame_changed(b, key, number);
        }
        debug_assert!(
            self.image(b, key).is_none_or(|i| i
                .anim
                .as_deref()
                .is_none_or(|a| a.charge == Anim::total(&a.shapes(Size::of(i)), Size::of(i)))),
            "an animation's charge follows its frames"
        );
        Ok(number)
    }

    /// A composition, `a=c`, on image `key` of the buffer in use (ADR-1102,
    /// D2): ENOENT for a frame that does not exist, EINVAL for a rectangle
    /// that leaves the image or that overlaps itself in one frame, ENOSPC
    /// when the allowance refuses the destination composed whole; each
    /// refusal changes nothing.
    pub(crate) fn compose_frames(&mut self, key: ImageKey, c: Compose) -> Result<(), Refusal> {
        let b = self.active();
        let (plan, rect) = {
            let image = self.image(b, key).ok_or(Refusal::NotFound)?;
            let size = Size::of(image);
            let empty = Anim::default();
            let anim = image.anim.as_deref().unwrap_or(&empty);
            if !anim.exists(c.src) || !anim.exists(c.dst) {
                return Err(Refusal::NotFound);
            }
            let w = if c.width == 0 { size.width } else { c.width };
            let h = if c.height == 0 { size.height } else { c.height };
            if !inside(size, c.src_x, c.src_y, w, h) || !inside(size, c.dst_x, c.dst_y, w, h) {
                return Err(Refusal::Invalid);
            }
            let apart = |a: u32, b: u32, l: u32| {
                u64::from(a).saturating_add(u64::from(l)) <= u64::from(b)
                    || u64::from(b).saturating_add(u64::from(l)) <= u64::from(a)
            };
            let overlaps = !apart(c.src_x, c.dst_x, w) && !apart(c.src_y, c.dst_y, h);
            if c.src == c.dst && overlaps {
                return Err(Refusal::Invalid);
            }
            (Plan::compose(anim, size, c.dst), (w, h))
        };
        self.count(plan.visited);
        self.settle_frames((b, key), plan.before, plan.after, 0)?;
        self.make_whole(b, key, &plan.laid);
        let revision = self.bump();
        let image = self.image_mut(b, key).ok_or(Refusal::NotFound)?;
        let size = Size::of(image);
        let empty = Anim::default();
        let anim = image.anim.as_deref().unwrap_or(&empty);
        let src = anim.compose(image.pixels.as_slice(), c.src, size);
        let area = u64::from(rect.0).saturating_mul(u64::from(rect.1));
        let work = area
            .saturating_add(anim.compose_work(c.src, size))
            .saturating_add(size.pixels());
        let onto = |pixels: Pixels| {
            let mut pixels = writable(pixels);
            blit(
                &mut pixels,
                size.width,
                (c.dst_x, c.dst_y),
                &src,
                size.width,
                (c.src_x, c.src_y),
                rect,
                c.replace,
            );
            let opaque = all_opaque(&pixels);
            (Pixels::new(pixels), opaque)
        };
        if c.dst == 1 {
            let old = core::mem::replace(&mut image.pixels, Pixels::new(Vec::new()));
            (image.pixels, image.opaque) = onto(old);
            image.revision = revision;
        } else if let Some(anim) = image.anim.as_deref_mut() {
            anim.cache.forget(c.dst);
            if let Some(f) = anim.frame_mut(c.dst) {
                if let Kind::Whole(p) = &mut f.kind {
                    let old = core::mem::replace(p, Pixels::new(Vec::new()));
                    (*p, f.opaque) = onto(old);
                }
                f.revision = revision;
            }
        }
        self.add_work(work);
        self.frame_changed(b, key, c.dst);
        Ok(())
    }

    /// An animation control, `a=a`, on image `key` of the buffer in use
    /// (ADR-1102, D3, D4): ENOENT for a frame that does not exist and EINVAL
    /// for an `s` past 3, each changing nothing.
    pub(crate) fn control_animation(&mut self, key: ImageKey, c: Control) -> Result<(), Refusal> {
        let b = self.active();
        let image = self.image_mut(b, key).ok_or(Refusal::NotFound)?;
        let count = image.anim.as_deref().map_or(1, Anim::count);
        let exists = |n: u32| n >= 1 && n <= count;
        if c.current != 0 && !exists(c.current) || c.frame != 0 && !exists(c.frame) {
            return Err(Refusal::NotFound);
        }
        if c.run > 3 {
            return Err(Refusal::Invalid);
        }
        let anim = image.anim.get_or_insert_with(Box::default);
        if c.frame != 0 && c.gap != 0 {
            match anim.frame_mut(c.frame) {
                Some(f) => f.gap = c.gap,
                None => anim.root_gap = c.gap,
            }
        }
        // the clock takes `c` before `s`: it shows the frame, then runs or
        // stops from it
        if c.current != 0 {
            anim.current = c.current;
            anim.frame_stamp = anim.frame_stamp.saturating_add(1);
            anim.shows_current = true;
        }
        anim.run = match c.run {
            1 => Run::Stopped,
            2 => Run::Loading,
            3 => Run::Looping,
            _ => anim.run,
        };
        if anim.run != Run::Stopped {
            anim.shows_current = false;
        }
        if c.loops != 0 {
            anim.loops = c.loops;
        }
        if c.run != 0 || c.loops != 0 {
            anim.run_stamp = anim.run_stamp.saturating_add(1);
        }
        Ok(())
    }
}

/// What a frame command will cost and compose, worked out before anything
/// is allocated.
struct Plan {
    /// The image's charge now, and once the command is done.
    before: u64,
    after: u64,
    /// The change frames composed whole first, since a frame they are laid
    /// on changes, or since they are the destination of a composition.
    laid: Vec<u32>,
    /// The frame made or edited is kept whole.
    whole: bool,
    /// The frames the plan walked, as input work.
    visited: usize,
}

impl Plan {
    /// The plan of `a=f` with `new`'s keys and `area` pixels of data on
    /// `anim`, an image of `size`: a new frame, or an edit of frame
    /// `new.edit`. D1's three rules decide whether the frame is kept whole.
    fn frame(anim: &Anim, size: Size, new: &NewFrame, area: u64) -> Self {
        let mut shapes = anim.shapes(size);
        let half = |covered: u64| covered.saturating_mul(2) >= size.pixels();
        let small = size.bytes() <= FRAME_WHOLE_MAX;
        let cost = layer_cost(new.width, new.height);
        let whole_shape = Shape {
            change: false,
            cost: size.whole(),
        };
        let (laid, whole) = if new.edit == 0 {
            let depth = match new.base {
                0 => 1,
                c => anim.depth(c).saturating_add(1),
            };
            let whole = small || half(area) || depth > FRAME_CHAIN_MAX;
            shapes.push(if whole {
                whole_shape
            } else {
                Shape { change: true, cost }
            });
            (Vec::new(), whole)
        } else {
            let whole = match anim.frame(new.edit) {
                Some(Frame {
                    kind: Kind::Change { layers, .. },
                    ..
                }) => {
                    let covered = layers
                        .iter()
                        .map(|l| u64::from(l.width).saturating_mul(u64::from(l.height)))
                        .fold(area, u64::saturating_add);
                    small
                        || half(covered)
                        || anim.depth(new.edit).saturating_add(1) > FRAME_CHAIN_MAX
                }
                _ => true,
            };
            if let Some(s) = slot(&mut shapes, new.edit) {
                *s = if whole {
                    whole_shape
                } else {
                    Shape {
                        change: true,
                        cost: s.cost.saturating_add(cost),
                    }
                };
            }
            (anim.laid_on(new.edit), whole)
        };
        for &n in &laid {
            if let Some(s) = slot(&mut shapes, n) {
                *s = whole_shape;
            }
        }
        Self {
            before: anim.charge,
            after: Anim::total(&shapes, size),
            visited: shapes.len(),
            laid,
            whole,
        }
    }

    /// The plan of `a=c` into frame `dst` of `anim`, an image of `size`: the
    /// destination is composed whole, and so is every change frame laid on
    /// it.
    fn compose(anim: &Anim, size: Size, dst: u32) -> Self {
        let mut shapes = anim.shapes(size);
        let mut laid = anim.laid_on(dst);
        if dst >= 2 {
            laid.push(dst);
        }
        for &n in &laid {
            if let Some(s) = slot(&mut shapes, n) {
                *s = Shape {
                    change: false,
                    cost: size.whole(),
                };
            }
        }
        Self {
            before: anim.charge,
            after: Anim::total(&shapes, size),
            visited: shapes.len(),
            laid,
            whole: true,
        }
    }
}

/// Frame `n`'s place among the shapes of the frames past the root.
fn slot(shapes: &mut [Shape], n: u32) -> Option<&mut Shape> {
    let i = usize::try_from(n.checked_sub(2)?).ok()?;
    shapes.get_mut(i)
}

impl Anim {
    /// Appends a new frame from `new`'s keys and `layer`, its data, on the
    /// image whose root frame is `root`: whole when `whole`, otherwise a
    /// change frame on its base. Returns its number and the pixels written.
    #[expect(
        clippy::too_many_arguments,
        reason = "the root frame, the size, the keys, the data, the rule's answer, the root's opacity and the revision"
    )]
    fn push(
        &mut self,
        root: &Pixels,
        size: Size,
        new: &NewFrame,
        layer: Layer,
        whole: bool,
        root_opaque: bool,
        revision: u64,
    ) -> (u32, u64) {
        let base = match new.base {
            0 => {
                let [r, g, b, a] = new.background.to_be_bytes();
                Base::Color(premultiply(r, g, b, a))
            }
            c => Base::Frame(c),
        };
        let area = u64::from(layer.width).saturating_mul(u64::from(layer.height));
        let mut work = area;
        let (kind, opaque) = if whole {
            let mut pixels = match base {
                Base::Color(c) => vec![c; size.len()],
                Base::Frame(k) => {
                    work = work.saturating_add(self.compose_work(k, size));
                    self.compose(root.as_slice(), k, size)
                }
            };
            work = work.saturating_add(size.pixels());
            lay(&mut pixels, size.width, &layer);
            let opaque = all_opaque(&pixels);
            (Kind::Whole(Pixels::new(pixels)), opaque)
        } else {
            let opaque = self.base_opaque(base, root_opaque) && !(layer.replace && !new.opaque);
            let kind = Kind::Change {
                base,
                layers: vec![layer],
            };
            (kind, opaque)
        };
        self.frames.push(Frame {
            kind,
            gap: if new.gap == 0 { DEFAULT_GAP } else { new.gap },
            opaque,
            revision,
        });
        (self.count(), work)
    }

    /// Lays `layer`, the data of `new`, onto frame `new.edit`, past the root,
    /// of the image whose root frame is `root`: composed into its whole
    /// pixels when `whole`, otherwise as one more layer. Returns its number
    /// and the pixels written.
    fn edit(
        &mut self,
        root: &Pixels,
        size: Size,
        new: &NewFrame,
        layer: Layer,
        whole: bool,
        revision: u64,
    ) -> (u32, u64) {
        let n = new.edit;
        let mut work = u64::from(layer.width).saturating_mul(u64::from(layer.height));
        self.cache.forget(n);
        let composed = if whole {
            work = work
                .saturating_add(self.compose_work(n, size))
                .saturating_add(size.pixels());
            Some(self.compose(root.as_slice(), n, size))
        } else {
            None
        };
        if let Some(f) = self.frame_mut(n) {
            match (composed, &mut f.kind) {
                (Some(mut pixels), _) => {
                    lay(&mut pixels, size.width, &layer);
                    f.opaque = all_opaque(&pixels);
                    f.kind = Kind::Whole(Pixels::new(pixels));
                }
                (None, Kind::Change { layers, .. }) => {
                    f.opaque &= !(layer.replace && !new.opaque);
                    layers.push(layer);
                }
                (None, Kind::Whole(_)) => {}
            }
            f.revision = revision;
            if new.gap != 0 {
                f.gap = new.gap;
            }
        }
        (n, work)
    }
}
