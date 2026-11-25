//! The pan's stand-in for drm2d: one display driven through atomic
//! commits, whose primary plane shows a window of a buffer taller than
//! the screen. drm2d has no pan yet, so this file does what the pan needs
//! and nothing else: it finds a connected connector with the mode asked
//! for, its CRTC and primary plane, sets the mode, and flips buffers with
//! the plane's `SRC_Y` set at each flip. vtium-platform makes and maps the
//! tall dumb buffers ([`MappedDumb`]).
//!
//! References:
//! - ADR-1310 D6, "Where the driver allows it": the test-only commit of the
//!   pan and the driver's largest framebuffer.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::Path;
use std::time::Duration;

use drm_ffi::{
    DRM_CLIENT_CAP_ATOMIC, DRM_CLIENT_CAP_UNIVERSAL_PLANES, DRM_EVENT_FLIP_COMPLETE,
    DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_ATOMIC_TEST_ONLY,
    DRM_MODE_OBJECT_CONNECTOR, DRM_MODE_OBJECT_CRTC, DRM_MODE_OBJECT_PLANE,
    DRM_MODE_PAGE_FLIP_EVENT, drm_mode_modeinfo,
};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::fs::{Mode, OFlags, open};
use vtium_platform::display::MappedDumb;

/// `DRM_MODE_TYPE_PREFERRED`, `drm/drm_mode.h`.
const PREFERRED: u32 = 1 << 3;
/// The value of a plane's `type` property for a primary plane.
const PRIMARY: u64 = 1;
/// XRGB8888's fourcc.
const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");

/// An object's properties by name, with their ids.
struct Props(Vec<(String, u32, u64)>);

impl Props {
    fn read(fd: BorrowedFd<'_>, id: u32, kind: u32) -> io::Result<Props> {
        let (mut ids, mut values) = (Vec::new(), Vec::new());
        drm_ffi::mode::get_properties(fd, id, kind, Some(&mut ids), Some(&mut values))?;
        let mut list = Vec::new();
        for (pid, value) in ids.into_iter().zip(values) {
            // drm-ffi's second read of a property passes the counts the
            // first gave, so it needs room for the values and enumerators,
            // or the kernel answers EFAULT.
            let (mut v, mut e) = (Vec::new(), Vec::new());
            let p = drm_ffi::mode::get_property(fd, pid, Some(&mut v), Some(&mut e))?;
            list.push((c_name(&p.name), pid, value));
        }
        Ok(Props(list))
    }

    fn id(&self, name: &str) -> io::Result<u32> {
        self.0
            .iter()
            .find(|p| p.0 == name)
            .map(|p| p.1)
            .ok_or_else(|| io::Error::other(format!("no property {name}")))
    }

    fn value(&self, name: &str) -> Option<u64> {
        self.0.iter().find(|p| p.0 == name).map(|p| p.2)
    }
}

fn c_name(raw: &[core::ffi::c_char]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .map(|&c| c.to_ne_bytes()[0])
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A mode as the kernel's `struct drm_mode_modeinfo` lays it out, for a
/// `MODE_ID` blob.
fn mode_bytes(m: &drm_mode_modeinfo) -> Vec<u8> {
    let mut b = Vec::new();
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

/// One display of a card, driven by atomic commits.
pub struct Kms {
    fd: OwnedFd,
    connector: u32,
    crtc: u32,
    plane: u32,
    mode: drm_mode_modeinfo,
    blob: u32,
    connector_crtc: u32,
    mode_id: u32,
    active: u32,
    plane_props: Vec<(&'static str, u32)>,
    /// The largest framebuffer the driver makes, across and down.
    pub max: (u32, u32),
    /// A flip was asked for and its event has not come yet.
    pending: bool,
}

impl Kms {
    /// Opens the card at `path` and finds a connected connector with a
    /// `size` mode, or with its preferred mode where `size` is `None`.
    pub fn open(path: &Path, size: Option<(u32, u32)>) -> io::Result<Kms> {
        let fd = open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())?;
        let raw = fd.as_fd();
        drm_ffi::set_capability(raw, u64::from(DRM_CLIENT_CAP_UNIVERSAL_PLANES), true)?;
        drm_ffi::set_capability(raw, u64::from(DRM_CLIENT_CAP_ATOMIC), true)?;
        let (mut crtcs, mut connectors) = (Vec::new(), Vec::new());
        let res =
            drm_ffi::mode::get_resources(raw, None, Some(&mut crtcs), Some(&mut connectors), None)?;
        let mut found = None;
        for &c in &connectors {
            let mut modes = Vec::new();
            let info =
                drm_ffi::mode::get_connector(raw, c, None, None, Some(&mut modes), None, false)?;
            if info.connection != 1 {
                continue;
            }
            let pick = match size {
                Some((w, h)) => modes
                    .iter()
                    .find(|m| (u32::from(m.hdisplay), u32::from(m.vdisplay)) == (w, h)),
                None => modes
                    .iter()
                    .find(|m| m.type_ & PREFERRED != 0)
                    .or(modes.first()),
            };
            if let Some(m) = pick {
                found = Some((c, info.encoder_id, *m));
                break;
            }
        }
        let (connector, encoder, mode) =
            found.ok_or_else(|| io::Error::other("no connected connector has that mode"))?;
        let enc = drm_ffi::mode::get_encoder(raw, encoder)?;
        let crtc = if enc.crtc_id != 0 {
            enc.crtc_id
        } else {
            crtcs
                .iter()
                .enumerate()
                .find(|(i, _)| enc.possible_crtcs & (1 << i) != 0)
                .map(|(_, &c)| c)
                .ok_or_else(|| io::Error::other("the encoder drives no CRTC"))?
        };
        let index = crtcs.iter().position(|&c| c == crtc).unwrap_or(0);
        let mut planes = Vec::new();
        drm_ffi::mode::get_plane_resources(raw, Some(&mut planes))?;
        let mut plane = None;
        for &p in &planes {
            let info = drm_ffi::mode::get_plane(raw, p, None)?;
            let props = Props::read(raw, p, DRM_MODE_OBJECT_PLANE)?;
            if info.possible_crtcs & (1 << index) != 0 && props.value("type") == Some(PRIMARY) {
                plane = Some((p, props));
                break;
            }
        }
        let (plane, pprops) =
            plane.ok_or_else(|| io::Error::other("the CRTC has no primary plane"))?;
        let mut plane_props = Vec::new();
        for name in [
            "FB_ID", "CRTC_ID", "SRC_X", "SRC_Y", "SRC_W", "SRC_H", "CRTC_X", "CRTC_Y", "CRTC_W",
            "CRTC_H",
        ] {
            plane_props.push((name, pprops.id(name)?));
        }
        let cprops = Props::read(raw, crtc, DRM_MODE_OBJECT_CRTC)?;
        let nprops = Props::read(raw, connector, DRM_MODE_OBJECT_CONNECTOR)?;
        let blob = drm_ffi::mode::create_property_blob(raw, &mut mode_bytes(&mode))?.blob_id;
        Ok(Kms {
            connector,
            crtc,
            plane,
            mode,
            blob,
            connector_crtc: nprops.id("CRTC_ID")?,
            mode_id: cprops.id("MODE_ID")?,
            active: cprops.id("ACTIVE")?,
            plane_props,
            max: (res.max_width, res.max_height),
            pending: false,
            fd,
        })
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// The mode's size and its refresh rate in Hz.
    pub fn mode(&self) -> (u32, u32, u32) {
        (
            u32::from(self.mode.hdisplay),
            u32::from(self.mode.vdisplay),
            self.mode.vrefresh,
        )
    }

    /// A framebuffer over `dumb`.
    pub fn framebuffer(&self, dumb: &MappedDumb) -> io::Result<u32> {
        let (w, h) = dumb.size();
        let fb = drm_ffi::mode::add_fb2(
            self.fd(),
            w,
            h,
            XRGB8888,
            &[dumb.handle(), 0, 0, 0],
            &[dumb.pitch(), 0, 0, 0],
            &[0; 4],
            &[0; 4],
            0,
        )?;
        Ok(fb.fb_id)
    }

    pub fn remove_framebuffer(&self, fb: u32) {
        let _ = drm_ffi::mode::rm_fb(self.fd(), fb);
    }

    /// Commits framebuffer `fb` on the plane, shown from pixel row `src_y`,
    /// with `flags`; with the mode and the connector too where `modeset`.
    fn commit(&self, fb: u32, src_y: u32, modeset: bool, flags: u32) -> io::Result<()> {
        let (w, h, _) = self.mode();
        let mut objects = Vec::new();
        let mut counts = Vec::new();
        let (mut props, mut values) = (Vec::new(), Vec::new());
        if modeset {
            objects.extend([self.connector, self.crtc]);
            counts.extend([1, 2]);
            props.extend([self.connector_crtc, self.mode_id, self.active]);
            values.extend([u64::from(self.crtc), u64::from(self.blob), 1]);
        }
        objects.push(self.plane);
        counts.push(self.plane_props.len() as u32);
        for &(name, id) in &self.plane_props {
            let value = match name {
                "FB_ID" => u64::from(fb),
                "CRTC_ID" => u64::from(self.crtc),
                "SRC_Y" => u64::from(src_y) << 16,
                "SRC_W" => u64::from(w) << 16,
                "SRC_H" => u64::from(h) << 16,
                "CRTC_W" => u64::from(w),
                "CRTC_H" => u64::from(h),
                _ => 0,
            };
            props.push(id);
            values.push(value);
        }
        drm_ffi::mode::atomic_commit(
            self.fd(),
            flags,
            &mut objects,
            &mut counts,
            &mut props,
            &mut values,
        )
    }

    /// Sets the mode with `fb` shown from its top, blocking until it is on.
    pub fn set_mode(&mut self, fb: u32) -> io::Result<()> {
        self.commit(fb, 0, true, DRM_MODE_ATOMIC_ALLOW_MODESET)
    }

    /// Shows `fb` from its top, blocking until it is on, after any flip
    /// in flight.
    pub fn show(&mut self, fb: u32) -> io::Result<()> {
        self.wait_flip(Duration::from_secs(2))?;
        self.commit(fb, 0, false, 0)
    }

    /// Whether the driver would take `fb` shown from pixel row `src_y`: a
    /// test-only commit, which changes nothing.
    pub fn test(&self, fb: u32, src_y: u32) -> io::Result<()> {
        self.commit(fb, src_y, false, DRM_MODE_ATOMIC_TEST_ONLY)
    }

    /// Asks for a flip to `fb` shown from pixel row `src_y`, which
    /// [`Kms::wait_flip`] waits for.
    pub fn flip(&mut self, fb: u32, src_y: u32) -> io::Result<()> {
        self.commit(
            fb,
            src_y,
            false,
            DRM_MODE_ATOMIC_NONBLOCK | DRM_MODE_PAGE_FLIP_EVENT,
        )?;
        self.pending = true;
        Ok(())
    }

    /// Waits for the flip asked for last, at most `bound`. Returns whether
    /// one was pending.
    pub fn wait_flip(&mut self, bound: Duration) -> io::Result<bool> {
        if !self.pending {
            return Ok(false);
        }
        let timeout = rustix::event::Timespec::try_from(bound).map_err(io::Error::other)?;
        let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
        while self.pending {
            if poll(&mut fds, Some(&timeout))? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no flip event came",
                ));
            }
            let mut buf = [0u8; 1024];
            let n = rustix::io::read(&self.fd, &mut buf)?;
            let mut at = 0;
            while at + 8 <= n {
                let kind = u32::from_ne_bytes(buf[at..at + 4].try_into().unwrap_or_default());
                let len =
                    u32::from_ne_bytes(buf[at + 4..at + 8].try_into().unwrap_or_default()) as usize;
                if kind == DRM_EVENT_FLIP_COMPLETE {
                    self.pending = false;
                }
                at += len.max(8);
            }
        }
        Ok(true)
    }

    /// The driver's name and description, from `DRM_IOCTL_VERSION`.
    pub fn driver(&self) -> String {
        let (mut name, mut desc) = (Vec::new(), Vec::new());
        match drm_ffi::get_version(self.fd(), Some(&mut name), None, Some(&mut desc)) {
            Ok(v) => format!(
                "{} {}.{}.{} ({})",
                c_name(&name),
                v.version_major,
                v.version_minor,
                v.version_patchlevel,
                c_name(&desc)
            ),
            Err(e) => format!("unknown ({e})"),
        }
    }
}

impl Drop for Kms {
    fn drop(&mut self) {
        let _ = self.wait_flip(Duration::from_secs(1));
        let _ = drm_ffi::mode::destroy_property_blob(self.fd(), self.blob);
    }
}
