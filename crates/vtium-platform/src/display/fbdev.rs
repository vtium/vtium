//! fbdev: a framebuffer device's one display.
//!
//! The display keeps the mode it is given, in one buffer: the
//! framebuffer, mapped. A device whose pixels are XRGB8888 is drawn into
//! directly; any other true-color layout of 16, 24 or 32 bits, and a
//! 1-bit device (vtium/vtium#135), is drawn into an XRGB8888 copy in
//! memory, converted into the device's layout where a swap's damage says.
//! fbdev has no flip event, so a timer at the mode's refresh interval
//! stands in for it.

use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};
use calloop::{LoopHandle, RegistrationToken};
use rustix::fs::{Mode as FileMode, OFlags, open};

use super::{Canvas, Density, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
use crate::ioctl::{
    FB_ACTIVATE_FORCE, FB_ACTIVATE_NOW, FB_BLANK_NORMAL, FB_BLANK_POWERDOWN, FB_BLANK_UNBLANK,
    FB_NONSTD_REV_PIX_IN_B, FB_VISUAL_MONO01, FB_VISUAL_MONO10, FB_VISUAL_TRUECOLOR,
    FbFixScreeninfo, FbVarScreeninfo, fb_blank, fb_get_fix, fb_get_var, fb_put_var,
};
use crate::mmap::Mapping;

/// fbdev's one display.
const DISPLAY: DisplayId = DisplayId(0);

/// What fbdev asks of a framebuffer device, as a trait: [`Fb`] implements
/// it through the kernel's ioctls, and the tests' stand-in in memory.
pub(crate) trait Framebuffer {
    type Map: AsMut<[u8]>;

    fn var(&self) -> io::Result<FbVarScreeninfo>;
    fn put_var(&self, var: &mut FbVarScreeninfo) -> io::Result<()>;
    fn fix(&self) -> io::Result<FbFixScreeninfo>;
    fn blank(&self, level: u32) -> io::Result<()>;
    fn map(&self, len: usize) -> io::Result<Self::Map>;
}

/// A framebuffer device, `/dev/fbN`.
pub struct Fb {
    fd: OwnedFd,
}

impl Fb {
    /// Takes a framebuffer the seat opened.
    pub fn new(fd: OwnedFd) -> Fb {
        Fb { fd }
    }

    pub fn open(path: &Path) -> io::Result<Fb> {
        let fd = open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY,
            FileMode::empty(),
        )?;
        Ok(Fb { fd })
    }
}

impl Framebuffer for Fb {
    type Map = Mapping;

    fn var(&self) -> io::Result<FbVarScreeninfo> {
        fb_get_var(self.fd.as_fd())
    }

    fn put_var(&self, var: &mut FbVarScreeninfo) -> io::Result<()> {
        fb_put_var(self.fd.as_fd(), var)
    }

    fn fix(&self) -> io::Result<FbFixScreeninfo> {
        fb_get_fix(self.fd.as_fd())
    }

    fn blank(&self, level: u32) -> io::Result<()> {
        fb_blank(self.fd.as_fd(), level)
    }

    fn map(&self, len: usize) -> io::Result<Mapping> {
        Mapping::new(self.fd.as_fd(), len, 0)
    }
}

/// Where a pixel's channels sit in the device's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    /// Bytes a pixel.
    bytes: usize,
    /// Each channel's offset and length in bits: red, green, blue.
    channels: [(u32, u32); 3],
}

impl Layout {
    fn xrgb8888(&self) -> bool {
        self.bytes == 4 && self.channels == [(16, 8), (8, 8), (0, 8)]
    }

    /// An XRGB8888 pixel in this layout, each channel cut to its length.
    fn encode(&self, xrgb: u32) -> u32 {
        let mut out = 0u32;
        for (i, (offset, length)) in self.channels.into_iter().enumerate() {
            let shift = 16u32.saturating_sub(u32::try_from(i).unwrap_or(0).saturating_mul(8));
            let value = (xrgb >> shift) & 0xff;
            let cut = value.checked_shr(8u32.saturating_sub(length)).unwrap_or(0);
            out |= cut.checked_shl(offset).unwrap_or(0);
        }
        out
    }
}

/// How the device holds a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Packing {
    TrueColor(Layout),
    /// One bit a pixel, eight to a byte.
    Mono {
        /// A set bit is lit, white: `FB_VISUAL_MONO10`. `FB_VISUAL_MONO01`
        /// sets the unlit bits.
        set_lit: bool,
        /// The first pixel of a byte is its most significant bit.
        msb_first: bool,
    },
}

impl Packing {
    /// The byte a cleared framebuffer holds: every pixel unlit, black,
    /// which `FB_VISUAL_MONO01` writes as set bits.
    fn clear(&self) -> u8 {
        match self {
            Packing::Mono { set_lit: false, .. } => 0xff,
            _ => 0,
        }
    }

    fn xrgb8888(&self) -> bool {
        matches!(self, Packing::TrueColor(l) if l.xrgb8888())
    }

    /// A 1-bit device of `visual`, `None` for any other. The kernel's own
    /// drawing puts the first pixel of a byte in its low bit on a
    /// little-endian host and in its high bit on a big-endian one, and
    /// `FB_NONSTD_REV_PIX_IN_B` reverses that (`fb_draw.h`'s
    /// `fb_reverse_init`).
    fn mono(var: &FbVarScreeninfo, visual: u32) -> Option<Packing> {
        let set_lit = match visual {
            FB_VISUAL_MONO10 => true,
            FB_VISUAL_MONO01 => false,
            _ => return None,
        };
        let reversed = var.nonstd & FB_NONSTD_REV_PIX_IN_B != 0;
        (var.bits_per_pixel == 1).then_some(Packing::Mono {
            set_lit,
            msb_first: cfg!(target_endian = "big") != reversed,
        })
    }
}

/// Whether an XRGB8888 pixel is lit on a 1-bit display: its BT.601 luma
/// at least half, as the kernel's `drm_fb_xrgb8888_to_mono` decides for a
/// DRM panel, so both backends show a frame alike. The renderer draws a
/// 1-bit display in pure black and white, which this keeps exact.
fn lit(xrgb: u32) -> bool {
    // at most 256 * 255, so nothing wraps
    let (r, g, b) = (xrgb >> 16 & 0xff, xrgb >> 8 & 0xff, xrgb & 0xff);
    let luma = r
        .wrapping_mul(77)
        .wrapping_add(g.wrapping_mul(150))
        .wrapping_add(b.wrapping_mul(29));
    luma >> 8 >= 128
}

/// The framebuffer, mapped while the device is awake.
struct Screen<M> {
    map: M,
    width: u32,
    height: u32,
    /// Bytes a line in the framebuffer.
    stride: usize,
    packing: Packing,
    /// The XRGB8888 copy drawn into, where the layout is another.
    shadow: Option<Vec<u8>>,
    /// Nothing drawn since the framebuffer was cleared.
    fresh: bool,
}

/// A framebuffer device's display. Outside the crate the device is an
/// [`Fb`]; the tests stand one in.
#[expect(private_bounds, reason = "the device is an Fb outside the crate")]
pub struct Fbdev<F: Framebuffer> {
    dev: F,
    awake: bool,
    screen: Option<Screen<F::Map>>,
    online: bool,
    announced: bool,
    size: (u32, u32),
    /// The physical size in millimetres, 0 where the device has none.
    mm: (u32, u32),
    rate_mhz: u32,
    /// The stand-in for a flip event: the refresh interval.
    interval: Duration,
    /// `None` until set: the device's state is not read back.
    dpms: Option<Dpms>,
    /// The canvas was lent since the last swap.
    lent: bool,
    /// A swap's stand-in flip event is pending.
    vblank: bool,
    /// Unblanks tried again so far, while retrying.
    retries: Option<usize>,
    /// The retries ran out with [`Event::WakeFailed`], and the display has
    /// not come on since: the next input tries again.
    wake_failed: bool,
    events: Vec<Event>,
    timer: Option<RegistrationToken>,
    link: Option<Box<dyn Link>>,
}

trait Link {
    fn arm(&self, after: Duration) -> Option<RegistrationToken>;
    fn disarm(&self, token: RegistrationToken);
}

#[expect(private_bounds, reason = "the device is an Fb outside the crate")]
impl<F: Framebuffer> Fbdev<F> {
    pub(crate) fn with(dev: F) -> Fbdev<F> {
        Fbdev {
            dev,
            awake: false,
            screen: None,
            online: false,
            announced: false,
            size: (0, 0),
            mm: (0, 0),
            rate_mhz: 0,
            interval: Duration::from_millis(15),
            dpms: None,
            lent: false,
            vblank: false,
            retries: None,
            wake_failed: false,
            events: Vec::new(),
            timer: None,
            link: None,
        }
    }

    /// The events since the last drain, in order.
    pub fn events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    pub fn displays(&self) -> impl Iterator<Item = DisplayId> + '_ {
        self.announced.then_some(DISPLAY).into_iter()
    }

    pub fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        (id == DISPLAY && self.online).then_some(self.size)
    }

    pub fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        (id == DISPLAY && self.online).then_some(self.rate_mhz)
    }

    /// The display's density, from its mode and the physical size the
    /// device reports, or [`Density::ASSUMED`] where it reports none or one
    /// out of bounds.
    pub fn density(&self, id: DisplayId) -> Option<Density> {
        (id == DISPLAY && self.online).then(|| Density::of(self.size, self.mm))
    }

    /// fbdev takes no damage clips.
    pub fn supports_damage(&self, _: DisplayId) -> bool {
        false
    }

    /// fbdev has no cursor plane: the compositor draws the pointer.
    pub fn hardware_cursor(&self, _: DisplayId) -> bool {
        false
    }

    /// The display's levels: 2 where the device's visual is
    /// `FB_VISUAL_MONO01` or `FB_VISUAL_MONO10`, `None` for true color.
    pub fn levels(&self, id: DisplayId) -> Option<u8> {
        let s = self.screen.as_ref().filter(|_| id == DISPLAY)?;
        matches!(s.packing, Packing::Mono { .. }).then_some(2)
    }

    /// Whether the last swap's stand-in flip event is pending.
    pub fn is_flipping(&self, id: DisplayId) -> bool {
        id == DISPLAY && self.vblank
    }

    pub fn dpms(&self, id: DisplayId) -> Option<Dpms> {
        (id == DISPLAY).then_some(self.dpms).flatten()
    }

    /// Sets the display's power state through `FBIOBLANK`, ending any
    /// retries of an unblank. While it is not on the display lends no
    /// canvas; unblanked, its buffer is age 0 and [`Event::Redraw`]
    /// follows.
    pub fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        if id != DISPLAY || !self.online || !self.awake {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the display is not online",
            ));
        }
        self.retries = None;
        self.dev.blank(match dpms {
            Dpms::On => FB_BLANK_UNBLANK,
            Dpms::Standby | Dpms::Suspend => FB_BLANK_NORMAL,
            Dpms::Off => FB_BLANK_POWERDOWN,
        })?;
        let was = self.dpms.replace(dpms);
        if dpms == Dpms::On {
            self.wake_failed = false;
            if was.is_some_and(|d| d != Dpms::On) {
                if let Some(s) = self.screen.as_mut() {
                    s.fresh = true;
                }
                self.events.push(Event::Redraw(DISPLAY));
            }
        }
        Ok(())
    }

    /// Powers the display on after the blank, on the input that ends it.
    /// Where `FBIOBLANK` fails, the unblank is tried again every
    /// [`crate::WAKE_RETRY_MS`], at most [`crate::WAKE_RETRIES`] times,
    /// since no later input comes to `unblank`.
    pub fn unblank(&mut self) -> io::Result<()> {
        if self.online && self.awake && self.dpms.is_some_and(|d| d != Dpms::On) {
            if let Err(e) = self.set_dpms(DISPLAY, Dpms::On) {
                self.retries = Some(0);
                self.settle();
                return Err(e);
            }
        }
        Ok(())
    }

    /// The user pressed a key or acted with the pointer (a move, a button
    /// or the wheel). After [`Event::WakeFailed`], it tries the unblank
    /// again, at most [`crate::WAKE_RETRIES`] times more.
    /// Otherwise, and while the retries run, it does nothing.
    pub fn input(&mut self) -> io::Result<()> {
        if self.retries.is_some() || !std::mem::take(&mut self.wake_failed) {
            return Ok(());
        }
        self.unblank()
    }

    /// An unblank retry's timer fired. Once the retries run out, the
    /// device reports [`Event::WakeFailed`].
    fn retry_unblank(&mut self) {
        let Some(tried) = self.retries else {
            return;
        };
        if let Err(e) = self.unblank() {
            if tried.saturating_add(1) >= crate::WAKE_RETRIES {
                log::error!("cannot unblank the framebuffer after {tried} retries: {e}");
                self.retries = None;
                self.wake_failed = true;
                self.events.push(Event::WakeFailed);
            } else {
                self.retries = Some(tried.saturating_add(1));
            }
        }
        self.settle();
    }

    /// Whether the display takes a frame now: it is awake and powered on,
    /// and the last swap's stand-in flip event is not pending.
    pub fn ready_for_frame(&self, id: DisplayId) -> bool {
        self.lends(id) && !self.vblank
    }

    /// Whether [`Fbdev::canvas`] lends the buffer: the display is awake, set
    /// up and powered on.
    fn lends(&self, id: DisplayId) -> bool {
        id == DISPLAY
            && self.awake
            && self.screen.is_some()
            && self.dpms.is_none_or(|d| d == Dpms::On)
    }

    /// Lends the buffer for the next frame. It is the framebuffer itself,
    /// or its copy, and holds the frame before this one.
    pub fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        if !self.lends(id) {
            return None;
        }
        let s = self.screen.as_mut()?;
        self.lent = true;
        let age = u32::from(!s.fresh);
        let (pixels, stride) = match &mut s.shadow {
            Some(shadow) => (shadow.as_mut_slice(), s.width.saturating_mul(4)),
            None => {
                let len = s.stride.saturating_mul(usize::try_from(s.height).ok()?);
                (
                    s.map.as_mut().get_mut(..len)?,
                    u32::try_from(s.stride).ok()?,
                )
            }
        };
        Some(Canvas {
            pixels,
            width: s.width,
            height: s.height,
            stride,
            age,
        })
    }

    /// Presents the frame drawn into the canvas lent since the last swap:
    /// converts its `damage` into the device's layout where it is drawn
    /// into a copy, and starts the stand-in for the flip event. With no
    /// damage, or without a canvas lent, it does nothing.
    pub fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let lent = id == DISPLAY && std::mem::take(&mut self.lent);
        if id != DISPLAY || !self.awake || self.dpms.is_some_and(|d| d != Dpms::On) {
            return Err(SwapError::Off);
        }
        let s = self.screen.as_mut().ok_or(SwapError::Off)?;
        if !lent || damage.is_empty() {
            return Ok(Swapped::Nothing);
        }
        if s.shadow.is_some() {
            let whole = [Rect {
                x: 0,
                y: 0,
                width: s.width,
                height: s.height,
            }];
            let rects = if s.fresh { &whole[..] } else { damage };
            let (width, height) = (s.width, s.height);
            let clipped: Vec<Rect> = rects.iter().filter_map(|r| r.clip(width, height)).collect();
            for r in clipped {
                s.convert(r);
            }
        }
        s.fresh = false;
        self.vblank = true;
        self.settle();
        Ok(Swapped::Frame)
    }

    /// Maps the framebuffer and sets the display up, then reports it.
    pub fn wake_up(&mut self) -> io::Result<()> {
        self.awake = true;
        if let Err(e) = self.activate() {
            self.awake = false;
            return Err(e);
        }
        // A wake-up counts as user activity (the operator's decision on
        // vtium/vtium#18): a blanked display comes on.
        if self.dpms.is_some_and(|d| d != Dpms::On) {
            match self.dev.blank(FB_BLANK_UNBLANK) {
                Ok(()) => self.dpms = Some(Dpms::On),
                Err(e) => {
                    log::warn!("cannot unblank the framebuffer: {e}");
                    self.retries = Some(0);
                    self.settle();
                }
            }
        }
        if self.announced {
            self.events.push(Event::Redraw(DISPLAY));
        } else {
            self.announced = true;
            self.events.push(Event::Added(DISPLAY));
        }
        self.wake_failed = false;
        self.events.push(Event::Awake);
        Ok(())
    }

    /// Clears and unmaps the framebuffer. fbdev has no flips to wait for,
    /// so [`Event::Asleep`] follows at once.
    pub fn sleep(&mut self) {
        if let Some(mut s) = self.screen.take() {
            let clear = s.packing.clear();
            s.map.as_mut().fill(clear);
        }
        self.awake = false;
        self.lent = false;
        self.vblank = false;
        self.retries = None;
        self.wake_failed = false;
        self.settle();
        self.events.push(Event::Asleep);
    }

    /// The device's timer fired: the stand-in for a flip event, or the
    /// next try of an unblank that failed.
    fn fired(&mut self) {
        if self.vblank {
            self.vblank();
        } else {
            self.retry_unblank();
        }
    }

    /// The stand-in for a flip event fired.
    pub(crate) fn vblank(&mut self) {
        if std::mem::take(&mut self.vblank) && self.awake {
            self.events.push(Event::Flipped(DISPLAY));
        }
        self.settle();
    }

    /// Arms and disarms the timer to match what waits: the stand-in flip
    /// event, else an unblank's retry.
    fn settle(&mut self) {
        let Some(link) = &self.link else {
            return;
        };
        let retry = u64::try_from(crate::WAKE_RETRY_MS).unwrap_or(u64::MAX);
        let after = if self.vblank {
            Some(self.interval)
        } else {
            self.retries.map(|_| Duration::from_millis(retry))
        };
        match (after, self.timer.take()) {
            (Some(after), None) => self.timer = link.arm(after),
            (None, Some(t)) => link.disarm(t),
            (_, t) => self.timer = t,
        }
    }

    /// Sets the device to its mode in one buffer, at 32, 24 or 16 bits of
    /// true color, else at 1 bit where its visual is monochrome, and maps
    /// it cleared.
    fn activate(&mut self) -> io::Result<()> {
        let mut var = self.dev.var()?;
        var.xoffset = 0;
        var.yoffset = 0;
        var.activate = FB_ACTIVATE_NOW | FB_ACTIVATE_FORCE;
        var.xres_virtual = var.xres;
        var.yres_virtual = var.yres;
        self.dev.put_var(&mut var).inspect_err(|e| {
            log::debug!("cannot reset the framebuffer's offsets: {e}");
        })?;
        let mut var = self.dev.var()?;
        let mut fix = self.dev.fix()?;
        if fix.visual != FB_VISUAL_TRUECOLOR || var.bits_per_pixel != 32 {
            for depth in [32, 24, 16] {
                let mut wanted = FbVarScreeninfo {
                    bits_per_pixel: depth,
                    activate: FB_ACTIVATE_NOW | FB_ACTIVATE_FORCE,
                    ..var
                };
                if self.dev.put_var(&mut wanted).is_err() {
                    continue;
                }
                var = self.dev.var()?;
                fix = self.dev.fix()?;
                if fix.visual == FB_VISUAL_TRUECOLOR {
                    break;
                }
            }
        }
        let unsupported = |what: String| io::Error::new(io::ErrorKind::Unsupported, what);
        let packing = match Packing::mono(&var, fix.visual) {
            Some(mono) => mono,
            None => {
                if ![32, 24, 16].contains(&var.bits_per_pixel) {
                    return Err(unsupported(format!(
                        "the framebuffer has {} bits a pixel, not 1, 16, 24 or 32",
                        var.bits_per_pixel
                    )));
                }
                if fix.visual != FB_VISUAL_TRUECOLOR {
                    return Err(unsupported(
                        "the framebuffer is neither true color nor 1-bit".into(),
                    ));
                }
                let channels = [var.red, var.green, var.blue].map(|c| (c.offset, c.length));
                if channels.iter().any(|&(_, length)| length > 8) {
                    return Err(unsupported("the framebuffer's channels pass 8 bits".into()));
                }
                Packing::TrueColor(Layout {
                    bytes: usize::try_from(var.bits_per_pixel / 8).unwrap_or(4),
                    channels,
                })
            }
        };
        self.rate_mhz = refresh_mhz(&var);
        self.interval = refresh_interval(self.rate_mhz);
        let height = usize::try_from(var.yres).map_err(|_| unsupported("height".into()))?;
        let stride = usize::try_from(fix.line_length).map_err(|_| unsupported("stride".into()))?;
        let len = stride.saturating_mul(height);
        let mut map = self.dev.map(len)?;
        map.as_mut().fill(packing.clear());
        let shadow = (!packing.xrgb8888()).then(|| {
            let px = usize::try_from(var.xres)
                .unwrap_or(0)
                .saturating_mul(height);
            vec![0u8; px.saturating_mul(4)]
        });
        let visual = match packing {
            Packing::Mono { set_lit: true, .. } => ", FB_VISUAL_MONO10: a 1-bit display",
            Packing::Mono { set_lit: false, .. } => ", FB_VISUAL_MONO01: a 1-bit display",
            Packing::TrueColor(_) => "",
        };
        log::info!(
            "framebuffer at {}x{}, {} bits a pixel{visual}",
            var.xres,
            var.yres,
            var.bits_per_pixel
        );
        self.size = (var.xres, var.yres);
        // fbdev marks a size it does not know 0 or all ones; all ones gives
        // a density under DPI_MIN, so both read as no size.
        self.mm = (var.width, var.height);
        self.screen = Some(Screen {
            map,
            width: var.xres,
            height: var.yres,
            stride,
            packing,
            shadow,
            fresh: true,
        });
        self.online = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn dev(&self) -> &F {
        &self.dev
    }
}

impl<M: AsMut<[u8]>> Screen<M> {
    /// Converts `r` from the copy into the framebuffer.
    fn convert(&mut self, r: Rect) {
        match self.packing {
            Packing::TrueColor(layout) => self.convert_true(r, layout),
            Packing::Mono { set_lit, msb_first } => self.pack(r, set_lit, msb_first),
        }
    }

    fn convert_true(&mut self, r: Rect, layout: Layout) {
        let Some(shadow) = &self.shadow else {
            return;
        };
        let bytes = layout.bytes;
        let width = usize::try_from(self.width).unwrap_or(0);
        let (x, w) = (
            usize::try_from(r.x).unwrap_or(0),
            usize::try_from(r.width).unwrap_or(0),
        );
        let map = self.map.as_mut();
        for y in r.y..r.y.saturating_add(r.height) {
            let y = usize::try_from(y).unwrap_or(0);
            let from = y.saturating_mul(width).saturating_add(x).saturating_mul(4);
            let to = y
                .saturating_mul(self.stride)
                .saturating_add(x.saturating_mul(bytes));
            let src = shadow.get(from..).unwrap_or_default().chunks_exact(4);
            let dst = map
                .get_mut(to..)
                .unwrap_or_default()
                .chunks_exact_mut(bytes);
            for (px, out) in src.zip(dst).take(w) {
                let xrgb = <[u8; 4]>::try_from(px).map_or(0, u32::from_ne_bytes);
                let encoded = layout.encode(xrgb).to_le_bytes();
                out.copy_from_slice(encoded.get(..bytes).unwrap_or_default());
            }
        }
    }

    /// Packs `r` from the copy into the framebuffer at 1 bit a pixel, in
    /// whole bytes: the pixels of a byte the damage only partly covers
    /// come from the copy too, so the framebuffer is never read. Bits past
    /// the line's last pixel are left clear.
    fn pack(&mut self, r: Rect, set_lit: bool, msb_first: bool) {
        let Some(shadow) = &self.shadow else {
            return;
        };
        let width = usize::try_from(self.width).unwrap_or(0);
        let first = usize::try_from(r.x).unwrap_or(0) / 8;
        let end = usize::try_from(r.x.saturating_add(r.width))
            .unwrap_or(0)
            .min(width)
            .div_ceil(8);
        let map = self.map.as_mut();
        for y in r.y..r.y.saturating_add(r.height) {
            let y = usize::try_from(y).unwrap_or(0);
            let line = y.saturating_mul(width).saturating_mul(4);
            let row = y.saturating_mul(self.stride);
            for byte in first..end {
                let mut out = 0u8;
                for bit in 0..8 {
                    let x = byte.saturating_mul(8).saturating_add(bit);
                    if x >= width {
                        break;
                    }
                    let at = line.saturating_add(x.saturating_mul(4));
                    let px = shadow
                        .get(at..at.saturating_add(4))
                        .and_then(|p| <[u8; 4]>::try_from(p).ok())
                        .map_or(0, u32::from_ne_bytes);
                    if lit(px) == set_lit {
                        out |= if msb_first { 0x80 >> bit } else { 1 << bit };
                    }
                }
                if let Some(b) = map.get_mut(row.saturating_add(byte)) {
                    *b = out;
                }
            }
        }
    }
}

/// The refresh rate in mHz from the mode's timings, 60 Hz when they are
/// unknown, held to 1 mHz to 200 Hz.
fn refresh_mhz(var: &FbVarScreeninfo) -> u32 {
    let lines = u64::from(var.upper_margin)
        .saturating_add(u64::from(var.lower_margin))
        .saturating_add(u64::from(var.yres));
    let columns = u64::from(var.left_margin)
        .saturating_add(u64::from(var.right_margin))
        .saturating_add(u64::from(var.xres));
    // The pixel clock is in picoseconds.
    let frame = lines
        .saturating_mul(columns)
        .saturating_mul(u64::from(var.pixclock));
    let mhz = match 1_000_000_000_000_000u64.checked_div(frame) {
        Some(mhz) => mhz,
        None => {
            log::warn!("cannot read the refresh rate; taking 60 Hz");
            60_000
        }
    };
    u32::try_from(mhz.clamp(1, 200_000)).unwrap_or(200_000)
}

/// The stand-in flip event's interval: one refresh, in whole
/// milliseconds below a second, 15 ms when that rounds to none.
fn refresh_interval(mhz: u32) -> Duration {
    let ms = match 1_000_000u32.checked_div(mhz) {
        Some(0) | None => 15,
        Some(ms) => ms.min(999),
    };
    Duration::from_millis(u64::from(ms))
}

impl Fbdev<Fb> {
    /// Takes a framebuffer device, asleep: [`Fbdev::wake_up`] maps it.
    pub fn new(dev: Fb) -> Fbdev<Fb> {
        Fbdev::with(dev)
    }
}

#[expect(private_bounds, reason = "the device is an Fb outside the crate")]
impl<F: Framebuffer + 'static> Fbdev<F> {
    /// Puts the stand-in flip event in the loop behind `handle`. `access`
    /// finds the device in the loop's data, or `None` once it is gone.
    pub fn insert<D, A>(&mut self, handle: &LoopHandle<'static, D>, access: A)
    where
        D: 'static,
        A: Fn(&mut D) -> Option<&mut Fbdev<F>> + Clone + 'static,
    {
        self.link = Some(Box::new(LoopLink {
            handle: handle.clone(),
            access,
            dev: PhantomData::<fn(&F)>,
        }));
        self.settle();
    }
}

impl<F: Framebuffer> Drop for Fbdev<F> {
    fn drop(&mut self) {
        if let (Some(link), Some(token)) = (&self.link, self.timer.take()) {
            link.disarm(token);
        }
    }
}

struct LoopLink<D: 'static, F, A> {
    handle: LoopHandle<'static, D>,
    access: A,
    dev: PhantomData<fn(&F)>,
}

impl<D, F, A> Link for LoopLink<D, F, A>
where
    D: 'static,
    F: Framebuffer + 'static,
    A: Fn(&mut D) -> Option<&mut Fbdev<F>> + Clone + 'static,
{
    fn arm(&self, after: Duration) -> Option<RegistrationToken> {
        let access = self.access.clone();
        self.handle
            .insert_source(Timer::from_duration(after), move |_, _, data| {
                if let Some(video) = access(data) {
                    video.timer = None;
                    video.fired();
                }
                TimeoutAction::Drop
            })
            .inspect_err(|e| log::error!("cannot arm the framebuffer's timer: {e}"))
            .ok()
    }

    fn disarm(&self, token: RegistrationToken) {
        self.handle.remove(token);
    }
}

#[cfg(test)]
mod tests;
