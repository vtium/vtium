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
use super::super::{Ages, Damage, DisplayId, Dpms, Rect};

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

/// The longest side of the hardware cursor, `VIDEO_CURSOR_MAX_SIZE` in the
/// C reference (video.h:172).
pub(super) const CURSOR_MAX: u64 = 64;

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

/// A dumb buffer with its framebuffer, mapped.
pub(super) struct Buffer<M> {
    pub(super) dumb: Dumb,
    pub(super) fb: u32,
    pub(super) map: M,
    /// The size it was made for.
    pub(super) size: (u32, u32),
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

/// A connector driven through a CRTC.
pub(super) struct Output<M> {
    pub(super) id: DisplayId,
    pub(super) name: String,
    pub(super) crtc: u32,
    /// `None` on a device without atomic commits.
    pub(super) planes: Option<Planes>,
    pub(super) choice: Choice,
    /// The mode the display is set to, or is to be set to.
    pub(super) mode: Mode,
    pub(super) mode_blob: u32,
    pub(super) buffers: Vec<Buffer<M>>,
    pub(super) back: usize,
    pub(super) ages: Ages,
    pub(super) damage: Damage,
    /// The canvas was lent since the last swap.
    pub(super) lent: bool,
    /// The back buffer holds a frame drawn with damage and not presented.
    pub(super) fresh: bool,
    pub(super) cursor: Option<Cursor>,
    /// What the cursor plane shows, as the last commit that took set it.
    pub(super) cursor_set: Shown,
    /// `None` where the connector has no DPMS property.
    pub(super) dpms: Option<(u32, Dpms)>,
    pub(super) online: bool,
    /// Its buffers hold nothing known since a modeset or a wake-up, so the
    /// next announcement asks for a redraw.
    pub(super) redraw: bool,
    pub(super) flipping: bool,
    /// Flips started, so the bound's timer names the flip it bounds.
    pub(super) flips: u64,
    /// The timer bounding the flip in flight, and the flip it was armed
    /// for.
    pub(super) flip_timer: Option<(RegistrationToken, u64)>,
    pub(super) available: bool,
    pub(super) announced: bool,
}

impl<M: AsMut<[u8]>> Output<M> {
    pub(super) fn size(&self) -> (u32, u32) {
        self.mode.size()
    }

    pub(super) fn back_fb(&self) -> u32 {
        self.buffers.get(self.back).map_or(0, |b| b.fb)
    }

    pub(super) fn front(&self) -> usize {
        self.back ^ 1
    }

    /// A flip is in flight, to be bounded by its own timer.
    pub(super) fn start_flip(&mut self) {
        self.flipping = true;
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

    /// Clears the front buffer, which the display shows or flips to.
    pub(super) fn clear_front(&mut self) {
        let front = self.front();
        if let Some(b) = self.buffers.get_mut(front) {
            b.map.as_mut().fill(0);
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
