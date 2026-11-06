//! A display on a DRM card: its CRTC and planes, its buffers and cursor,
//! and the atomic state a commit sets for it.

use std::io;

use calloop::RegistrationToken;
use drm_ffi::{
    DRM_MODE_DPMS_OFF, DRM_MODE_DPMS_ON, DRM_MODE_DPMS_STANDBY, DRM_MODE_DPMS_SUSPEND,
    drm_mode_modeinfo,
};

use super::super::kms::{Dumb, Property, Request};
use super::super::mode::{Choice, Mode};
use super::super::{Ages, Damage, DisplayId, Dpms, Loan, Rect, try_lock};
use super::buffers::Buffer;

/// A DRM fourcc code.
pub(super) const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*code)
}

pub(super) const XRGB8888: u32 = fourcc(b"XR24");
pub(super) const ARGB8888: u32 = fourcc(b"AR24");

/// The values of a plane's `type` property, `drm/drm_mode.h`.
pub(super) const PLANE_OVERLAY: u64 = 0;
pub(super) const PLANE_PRIMARY: u64 = 1;
pub(super) const PLANE_CURSOR: u64 = 2;

/// The longest side of the hardware cursor: 64 pixels, the size the
/// kernel reports for the cursor plane of a driver that sets none
/// (`DRM_CAP_CURSOR_WIDTH`). vtium-render's `POINTER_SIDE_MAX` bounds the
/// arrow to the same side, which a test in vtium holds equal.
pub const CURSOR_MAX: u64 = 64;

pub(super) fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, what.to_owned())
}

/// A connector's name, as libdrm spells its kind: `HDMI-A-1`.
pub(super) fn connector_name(kind: u32, id: u32) -> String {
    let kind = match kind {
        1 => "VGA",
        2 => "DVI-I",
        3 => "DVI-D",
        4 => "DVI-A",
        5 => "Composite",
        6 => "SVIDEO",
        7 => "LVDS",
        8 => "Component",
        9 => "DIN",
        10 => "DP",
        11 => "HDMI-A",
        12 => "HDMI-B",
        13 => "TV",
        14 => "eDP",
        15 => "Virtual",
        16 => "DSI",
        17 => "DPI",
        18 => "Writeback",
        19 => "SPI",
        20 => "USB",
        _ => "Unknown",
    };
    format!("{kind}-{id}")
}

/// A mode as the kernel's `struct drm_mode_modeinfo` lays it out, for a
/// `MODE_ID` blob.
pub(super) fn mode_bytes(m: &drm_mode_modeinfo) -> Vec<u8> {
    let mut b = Vec::with_capacity(size_of::<drm_mode_modeinfo>());
    b.extend(m.clock.to_ne_bytes());
    for half in [
        m.hdisplay,
        m.hsync_start,
        m.hsync_end,
        m.htotal,
        m.hskew,
        m.vdisplay,
        m.vsync_start,
        m.vsync_end,
        m.vtotal,
        m.vscan,
    ] {
        b.extend(half.to_ne_bytes());
    }
    for word in [m.vrefresh, m.flags, m.type_] {
        b.extend(word.to_ne_bytes());
    }
    b.extend(m.name.iter().map(|c| c.to_ne_bytes()[0]));
    b
}

/// Rectangles as the kernel's `struct drm_mode_rect`s, for a
/// `FB_DAMAGE_CLIPS` blob.
pub(super) fn clip_bytes(rects: &[Rect]) -> Vec<u8> {
    let mut b = Vec::with_capacity(rects.len().saturating_mul(16));
    let int = |v: u32| i32::try_from(v).unwrap_or(i32::MAX).to_ne_bytes();
    for r in rects {
        b.extend(int(r.x));
        b.extend(int(r.y));
        b.extend(int(r.x.saturating_add(r.width)));
        b.extend(int(r.y.saturating_add(r.height)));
    }
    b
}

/// A signed property value, as the kernel reads it from 64 bits.
pub(super) fn signed(v: i64) -> u64 {
    u64::from_ne_bytes(v.to_ne_bytes())
}

pub(super) fn dpms_value(dpms: Dpms) -> u64 {
    u64::from(match dpms {
        Dpms::On => DRM_MODE_DPMS_ON,
        Dpms::Standby => DRM_MODE_DPMS_STANDBY,
        Dpms::Suspend => DRM_MODE_DPMS_SUSPEND,
        Dpms::Off => DRM_MODE_DPMS_OFF,
    })
}

/// An object's property ids, by name.
#[derive(Debug, Default)]
pub(super) struct Props(Vec<(String, u32)>);

impl Props {
    pub(super) fn new(list: &[Property]) -> Props {
        Props(list.iter().map(|p| (p.name.clone(), p.id)).collect())
    }

    /// Adds `name = value` for `object` to `req`.
    pub(super) fn set(
        &self,
        req: &mut Request,
        object: u32,
        name: &str,
        value: u64,
    ) -> io::Result<()> {
        let id = self
            .0
            .iter()
            .find_map(|(n, id)| (n == name).then_some(*id))
            .ok_or_else(|| invalid(&format!("no object property {name}")))?;
        req.set(object, id, value);
        Ok(())
    }
}

/// The bits of each color in a pixel of `format`, a DRM fourcc of the
/// buffers vtium makes; `None` for any other format.
pub(super) const fn bits_per_color(format: u32) -> Option<u64> {
    match format {
        XRGB8888 | ARGB8888 => Some(8),
        _ => None,
    }
}

/// The ids of a connector's properties that tell the sink how to read the
/// colors it is sent, each where the connector has it: `Colorspace`,
/// `HDR_OUTPUT_METADATA` and `max bpc`, with the range of `max bpc` where
/// it could be read (ADR-1401 D14).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ColorProps {
    colorspace: Option<u32>,
    hdr_metadata: Option<u32>,
    max_bpc: Option<(u32, Option<(u64, u64)>)>,
}

impl ColorProps {
    /// The color properties among a connector's `props`, with the range of
    /// `max bpc` as `range` reads it.
    pub(super) fn new(
        props: &[Property],
        range: impl FnOnce(u32) -> Option<(u64, u64)>,
    ) -> ColorProps {
        let id = |name: &str| props.iter().find(|p| p.name == name).map(|p| p.id);
        ColorProps {
            colorspace: id("Colorspace"),
            hdr_metadata: id("HDR_OUTPUT_METADATA"),
            max_bpc: id("max bpc").map(|id| (id, range(id))),
        }
    }

    /// The value each property the connector has takes for a display
    /// drawn in sRGB at `bpc` bits per color, as (property id, name, value):
    /// `Colorspace` at `Default`, 0 (`DRM_MODE_COLORIMETRY_DEFAULT`); no
    /// HDR metadata, blob 0; and `max bpc` at `bpc`, or at the value of its
    /// range nearest `bpc` where the range does not hold it, left out where
    /// `bpc` is `None`.
    pub(super) fn sdr(&self, bpc: Option<u64>) -> Vec<(u32, &'static str, u64)> {
        let max_bpc = self.max_bpc.map(|(id, range)| {
            let nearest = |b: u64| range.map_or(b, |(least, most)| b.max(least).min(most));
            (id, bpc.map(nearest))
        });
        [
            (self.colorspace, "Colorspace", Some(0)),
            (self.hdr_metadata, "HDR_OUTPUT_METADATA", Some(0)),
            (max_bpc.map(|m| m.0), "max bpc", max_bpc.and_then(|m| m.1)),
        ]
        .into_iter()
        .filter_map(|(id, name, value)| Some((id?, name, value?)))
        .collect()
    }
}

pub(super) fn value(props: &[Property], name: &str) -> Option<u64> {
    props.iter().find(|p| p.name == name).map(|p| p.value)
}

/// The objects an atomic commit sets for a display.
#[derive(Debug, Default)]
pub(super) struct Planes {
    pub(super) primary: u32,
    pub(super) primary_props: Props,
    pub(super) connector_props: Props,
    pub(super) crtc_props: Props,
    /// The cursor plane, where the CRTC has one.
    pub(super) cursor: Option<(u32, Props)>,
    /// Whether the primary plane takes `FB_DAMAGE_CLIPS`.
    pub(super) damage: bool,
}

/// What a cursor plane shows: the image's framebuffer, where its corner
/// is on the CRTC and its hotspot, or `None` where the plane is off.
pub(super) type Shown = Option<(u32, (i64, i64), (i32, i32))>;

/// The hardware cursor's image on the cursor plane, and where it is shown.
#[derive(Debug, Default)]
pub(super) struct Cursor {
    pub(super) dumb: Dumb,
    pub(super) fb: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) hot: (i32, i32),
    pub(super) at: (i32, i32),
    pub(super) offset: (i32, i32),
    pub(super) visible: bool,
}

impl Cursor {
    /// Where the image's corner goes on the CRTC.
    fn corner(&self) -> (i64, i64) {
        let at = |offset: i32, at: i32, hot: i32| {
            i64::from(offset)
                .saturating_add(i64::from(at))
                .saturating_sub(i64::from(hot))
        };
        (
            at(self.offset.0, self.at.0, self.hot.0),
            at(self.offset.1, self.at.1, self.hot.1),
        )
    }
}

/// The CRTC a placement put a display off, as [`Output::moved_from`]
/// records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Moved {
    pub(super) crtc: u32,
    /// The CRTC's index into the card's list of CRTCs.
    pub(super) pipe: usize,
    /// The display was lit on that CRTC: vtium had set it there, or at a
    /// wake-up its connector was on it.
    pub(super) lit: bool,
}

/// A connector driven through a CRTC.
pub(super) struct Output<M> {
    pub(super) id: DisplayId,
    pub(super) name: String,
    pub(super) crtc: u32,
    /// The CRTC's index into the card's list of CRTCs.
    pub(super) pipe: usize,
    /// `None` on a device without atomic commits.
    pub(super) planes: Option<Planes>,
    /// The connector's properties that set how the sink reads its colors,
    /// which each modeset sets for 8-bit sRGB (vtium/vtium#253).
    pub(super) color: ColorProps,
    pub(super) choice: Choice,
    /// The modes its connector listed when `choice` was made. A scan that
    /// finds the connector listing others chooses again.
    pub(super) modes: Vec<Mode>,
    /// The mode the display is set to, or is to be set to.
    pub(super) mode: Mode,
    /// The mode the last modeset that took set the display to, which its
    /// buffers were made for; `None` before its first.
    pub(super) set: Option<Mode>,
    /// A mode chosen from a new list of modes that the card refused, with
    /// the default mode refused too. The display stays at `set`, and a scan
    /// tries this mode again only when the card wakes or the set of
    /// displays changes.
    pub(super) held: Option<Mode>,
    /// A mode chosen from a new list of modes whose modeset failed for
    /// another reason than the card refusing it, as where its buffers could
    /// not be made. The display stays at `set`, the wake-up retries and a
    /// wake-up try this mode again, and once the retries run out it is
    /// held back.
    pub(super) owed: Option<Mode>,
    /// The CRTC the display wanted that the last placement put it off to
    /// light another display, until a modeset sets it on its new CRTC. A
    /// modeset that cannot set the displays there moves it back
    /// ([`super::Drm2d::move_back`]).
    pub(super) moved_from: Option<Moved>,
    /// The connector's physical size in millimetres, 0 where it has none.
    pub(super) mm: (u32, u32),
    pub(super) mode_blob: u32,
    /// One, two or three buffers, as the card's [`super::Buffers`] says.
    pub(super) buffers: Vec<Buffer<M>>,
    /// The buffer the next frame is drawn into. The buffers take turns,
    /// so the one before it is the last committed, which is on screen or
    /// flipping to it.
    pub(super) back: usize,
    pub(super) ages: Ages,
    pub(super) damage: Damage,
    /// The token lent for the back buffer, until it comes back.
    pub(super) loan: Option<Loan>,
    /// Taken anew whenever what the buffers mean changes under a token
    /// lent: new buffers, a VT release, a blank. A token of an older lease
    /// comes back too late.
    pub(super) lease: u64,
    /// The sleep found the buffer it clears lent, so the token's memory is
    /// cleared when it comes back.
    pub(super) clear_owed: bool,
    /// The back buffer holds a frame drawn with damage and not presented.
    pub(super) fresh: bool,
    pub(super) cursor: Option<Cursor>,
    /// What the cursor plane shows, as the last commit that took set it.
    pub(super) cursor_set: Shown,
    /// `None` where the connector has no DPMS property.
    pub(super) dpms: Option<(u32, Dpms)>,
    /// The owner powered it down through `set_dpms`, as the blank does,
    /// whether or not its connector has a DPMS property.
    pub(super) down: bool,
    pub(super) online: bool,
    /// Its buffers hold nothing known since a modeset or a wake-up, so the
    /// next announcement asks for a redraw.
    pub(super) redraw: bool,
    pub(super) flipping: bool,
    /// The flip in flight is the display's next vblank, waited for after
    /// the card refused a commit as busy: it shows no frame. It is set only
    /// with `flipping`, and the event or the bound that ends the flip
    /// clears both, so nothing waiting for the flips, a sleep included,
    /// ever finds it set.
    pub(super) vblank: bool,
    /// Flips started, so the bound's timer names the flip it bounds.
    pub(super) flips: u64,
    /// The CRTC the flip in flight was committed on. A hotplug that moves
    /// the display to another CRTC while it flips leaves the flip's event,
    /// and the bound on it, on this CRTC until the flip ends.
    pub(super) flip_crtc: u32,
    /// The timer bounding the flip in flight, and the flip it was armed
    /// for.
    pub(super) flip_timer: Option<(RegistrationToken, u64)>,
    /// The next flip whose event arrives ends a modeset: the atomic
    /// modeset's own commit, or the first frame after a legacy modeset,
    /// which sends no event of its own. That event logs that the new mode
    /// is on screen and clears the mark; a flip the bound takes as done
    /// clears it with no line, so a late event logs nothing.
    pub(super) modeset_flip: bool,
    pub(super) available: bool,
    pub(super) announced: bool,
    /// Bound after the card's first scan, as a display plugged in or
    /// back: until a modeset sets it, its first sets it from off.
    pub(super) returning: bool,
}

impl<M: AsMut<[u8]>> Output<M> {
    pub(super) fn size(&self) -> (u32, u32) {
        self.mode.size()
    }

    /// After a modeset that failed for another reason than the card
    /// refusing the mode, gives the display back the mode it is set to,
    /// which its buffers fit, where a scan chose another, and owes it the
    /// mode chosen, which the wake-up retries try again. Whether it did.
    pub(super) fn keep_set_mode(&mut self) -> bool {
        let Some(set) = self.set.filter(|&set| set != self.mode) else {
            return false;
        };
        self.owed = Some(self.mode);
        self.mode = set;
        true
    }

    /// After the card refused the mode a scan chose, and the default mode
    /// too, gives the display back the mode it is set to, which its
    /// buffers fit, and holds the mode chosen back. Whether it did.
    pub(super) fn hold_back(&mut self) -> bool {
        let Some(set) = self.set.filter(|&set| set != self.mode) else {
            return false;
        };
        self.held = Some(self.mode);
        self.owed = None;
        self.mode = set;
        true
    }

    /// Once the wake-up retries ran out, holds back the mode the display
    /// is owed, as a mode the card refused is held back, and gives it the
    /// mode it is set to, should a modeset still wait to set the mode owed.
    pub(super) fn hold_back_owed(&mut self) {
        let Some(owed) = self.owed.take() else {
            return;
        };
        log::warn!(
            "cannot set display {} to {}x{}; it stays at the mode it is set to",
            self.name,
            owed.size().0,
            owed.size().1
        );
        self.held = Some(owed);
        if let Some(set) = self.set {
            self.mode = set;
        }
    }

    pub(super) fn back_fb(&self) -> u32 {
        self.buffers.get(self.back).map_or(0, |b| b.fb)
    }

    /// The buffer last committed: on screen, or flipping to it.
    pub(super) fn front(&self) -> usize {
        let n = self.buffers.len();
        self.back
            .saturating_add(n)
            .saturating_sub(1)
            .checked_rem(n)
            .unwrap_or(0)
    }

    /// The buffer after the back one, which the frame after a committed
    /// frame is drawn into: the other of two, the third of three while the
    /// second flips to screen, and with one the buffer shown.
    pub(super) fn after_back(&self) -> usize {
        self.back
            .saturating_add(1)
            .checked_rem(self.buffers.len())
            .unwrap_or(0)
    }

    /// Whether a token lent is still out.
    pub(super) fn lent(&self) -> bool {
        self.loan.as_ref().is_some_and(Loan::out)
    }

    /// A token for the back buffer is free to draw into: no token is out,
    /// and no flip waits for that buffer. With two buffers the back one is
    /// on screen until the flip in flight completes; with one, it is the
    /// buffer shown, which vtium draws into whatever flips; with three,
    /// the third is free while the second flips. A frame drawn and kept
    /// from a failed swap waits for its flip whatever the buffers.
    pub(super) fn buffer_free(&self) -> bool {
        !self.lent() && (!self.flipping || (self.buffers.len() != 2 && !self.fresh))
    }

    /// What the buffers held is unknown, as after a token came back too
    /// late or was dropped: every buffer is age 0.
    pub(super) fn forget(&mut self) {
        self.ages = Ages::default();
    }

    /// A flip is in flight, to be bounded by its own timer.
    pub(super) fn start_flip(&mut self) {
        self.flipping = true;
        self.flip_crtc = self.crtc;
        self.flips = self.flips.wrapping_add(1);
    }

    /// What the cursor plane is to show, as the next commit sets it.
    pub(super) fn cursor_shown(&self) -> Shown {
        self.planes.as_ref()?.cursor.as_ref()?;
        let c = self.cursor.as_ref().filter(|c| c.visible)?;
        Some((c.fb, c.corner(), c.hot))
    }

    /// Whether the cursor plane is to show something other than what the
    /// last commit that took set.
    pub(super) fn cursor_changed(&self) -> bool {
        self.cursor_shown() != self.cursor_set
    }

    pub(super) fn dpms_on(&self) -> bool {
        self.dpms.is_none_or(|(_, d)| d == Dpms::On)
    }

    /// Powered on: what its buffers held counts as unknown, its next frame
    /// goes whole, and the owner is told to redraw it.
    pub(super) fn powered_on(&mut self) {
        self.ages = Ages::default();
        self.damage.set_whole();
        self.redraw = true;
    }

    /// Clears the front buffer, which the display shows or flips to. A
    /// token lent for it, as with one buffer, may be drawing into it, so its
    /// memory is cleared when the token comes back.
    pub(super) fn clear_front(&mut self) {
        let front = self.front();
        let lent = self.lent() && self.loan.as_ref().is_some_and(|l| l.buffer == front);
        if let Some(b) = self.buffers.get(front) {
            match try_lock(&b.map).filter(|_| !lent) {
                Some(mut memory) => memory.as_mut().fill(0),
                None => self.clear_owed = true,
            }
        }
        if let Some(held) = self.ages.held.get_mut(front) {
            *held = 0;
        }
    }

    /// Adds this display's state to `req`: its mode, the framebuffer
    /// `fb` on the primary plane, the damage blob and the cursor plane.
    pub(super) fn prepare(
        &self,
        req: &mut Request,
        fb: u32,
        mode_blob: u32,
        clips: Option<u32>,
        hotspot: bool,
    ) -> io::Result<()> {
        let p = self
            .planes
            .as_ref()
            .ok_or_else(|| invalid("no atomic planes"))?;
        let (w, h) = self.size();
        let crtc = u64::from(self.crtc);
        let conn = self.id.0;
        p.connector_props.set(req, conn, "CRTC_ID", crtc)?;
        p.crtc_props
            .set(req, self.crtc, "MODE_ID", u64::from(mode_blob))?;
        // DPMS is ACTIVE on an atomic device; the property alone would be
        // undone by this commit.
        p.crtc_props
            .set(req, self.crtc, "ACTIVE", u64::from(self.dpms_on()))?;
        let plane = p.primary;
        let pp = &p.primary_props;
        pp.set(req, plane, "FB_ID", u64::from(fb))?;
        pp.set(req, plane, "CRTC_ID", crtc)?;
        pp.set(req, plane, "SRC_X", 0)?;
        pp.set(req, plane, "SRC_Y", 0)?;
        pp.set(req, plane, "SRC_W", u64::from(w) << 16)?;
        pp.set(req, plane, "SRC_H", u64::from(h) << 16)?;
        pp.set(req, plane, "CRTC_X", 0)?;
        pp.set(req, plane, "CRTC_Y", 0)?;
        pp.set(req, plane, "CRTC_W", u64::from(w))?;
        pp.set(req, plane, "CRTC_H", u64::from(h))?;
        if let Some(clips) = clips {
            pp.set(req, plane, "FB_DAMAGE_CLIPS", u64::from(clips))?;
        }
        self.prepare_cursor(req, hotspot)
    }

    /// Adds the cursor plane's state to `req`, where the display has one.
    pub(super) fn prepare_cursor(&self, req: &mut Request, hotspot: bool) -> io::Result<()> {
        let p = self
            .planes
            .as_ref()
            .ok_or_else(|| invalid("no atomic planes"))?;
        let crtc = u64::from(self.crtc);
        if let Some((cp, cprops)) = &p.cursor {
            match &self.cursor {
                Some(c) if c.visible => {
                    if hotspot {
                        cprops.set(req, *cp, "HOTSPOT_X", signed(i64::from(c.hot.0)))?;
                        cprops.set(req, *cp, "HOTSPOT_Y", signed(i64::from(c.hot.1)))?;
                    }
                    let (x, y) = c.corner();
                    cprops.set(req, *cp, "FB_ID", u64::from(c.fb))?;
                    cprops.set(req, *cp, "CRTC_ID", crtc)?;
                    cprops.set(req, *cp, "SRC_X", 0)?;
                    cprops.set(req, *cp, "SRC_Y", 0)?;
                    cprops.set(req, *cp, "SRC_W", u64::from(c.width) << 16)?;
                    cprops.set(req, *cp, "SRC_H", u64::from(c.height) << 16)?;
                    cprops.set(req, *cp, "CRTC_X", signed(x))?;
                    cprops.set(req, *cp, "CRTC_Y", signed(y))?;
                    cprops.set(req, *cp, "CRTC_W", u64::from(c.width))?;
                    cprops.set(req, *cp, "CRTC_H", u64::from(c.height))?;
                }
                _ => {
                    cprops.set(req, *cp, "FB_ID", 0)?;
                    cprops.set(req, *cp, "CRTC_ID", 0)?;
                }
            }
        }
        Ok(())
    }
}
