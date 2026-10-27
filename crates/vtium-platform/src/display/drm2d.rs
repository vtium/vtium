//! drm2d: a DRM card's displays, drawn on the CPU into dumb buffers.
//!
//! Each display has two dumb buffers in XRGB8888. The compositor draws into
//! the back one and [`Drm2d::swap`] flips to it with a non-blocking commit
//! that asks for a completion event; the display is busy until that event
//! arrives. A device with atomic modesetting commits through atomic
//! requests, carrying the frame's damage as `FB_DAMAGE_CLIPS` where the
//! primary plane takes them and the hardware cursor on the cursor plane;
//! any other device, a device with universal planes and no atomic commits
//! among them, uses the legacy calls and presents whole frames.

mod output;
mod source;

use std::io;

use drm_ffi::{
    DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH, DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC,
    DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, DRM_CLIENT_CAP_UNIVERSAL_PLANES,
    DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_ATOMIC_TEST_ONLY,
    DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::io::Errno;

use super::kms::{Connector, Kms, Object, Request, Resources};
use super::mode::{self, Mode, ModePolicy};
use super::{Ages, Canvas, Damage, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
use output::{
    ARGB8888, Buffer, CURSOR_MAX, Cursor, Output, PLANE_CURSOR, PLANE_OVERLAY, PLANE_PRIMARY,
    Planes, Props, XRGB8888, clip_bytes, connector_name, dpms_of, dpms_value, invalid, mode_bytes,
    value,
};
use source::{Clock, Link, Timers};

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}

/// An operation that waits until no page flip is in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    Modeset,
    Sleep,
}

/// A DRM card's displays, drawn into dumb buffers. Outside the crate the
/// card is a [`Card`]; the tests stand one in.
#[expect(private_bounds, reason = "the card is a Card outside the crate")]
pub struct Drm2d<K: Kms> {
    card: K,
    /// No universal planes or no atomic commits: the legacy calls.
    legacy: bool,
    hotspot: bool,
    master: bool,
    awake: bool,
    policy: ModePolicy,
    outputs: Vec<Output<K::Map>>,
    waiting: Option<Waiting>,
    /// A flip's event was lost, and that was logged as a warning.
    lost_flip: bool,
    /// Wake-ups tried again so far, while retrying.
    retries: Option<usize>,
    events: Vec<Event>,
    timers: Timers,
    link: Option<Box<dyn Link>>,
}

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms> Drm2d<K> {
    /// Takes a card, asleep: [`Drm2d::wake_up`] takes DRM master and sets
    /// the displays up.
    pub(crate) fn with(card: K, policy: ModePolicy) -> io::Result<Drm2d<K>> {
        let universal = card.set_client_cap(DRM_CLIENT_CAP_UNIVERSAL_PLANES, true);
        let atomic = card.set_client_cap(DRM_CLIENT_CAP_ATOMIC, true);
        let legacy = universal.is_err() || atomic.is_err();
        if legacy {
            log::info!("the card has no universal planes or no atomic commits: legacy modesetting");
        }
        let hotspot = card
            .set_client_cap(DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, true)
            .is_ok();
        if card.cap(DRM_CAP_DUMB_BUFFER).unwrap_or(0) == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the driver has no dumb buffers",
            ));
        }
        Ok(Drm2d {
            card,
            legacy,
            hotspot,
            master: false,
            awake: false,
            policy,
            outputs: Vec::new(),
            waiting: None,
            lost_flip: false,
            retries: None,
            events: Vec::new(),
            timers: Timers::default(),
            link: None,
        })
    }

    /// The events since the last drain, in order.
    pub fn events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// The displays online, in the card's connector order.
    pub fn displays(&self) -> impl Iterator<Item = DisplayId> + '_ {
        self.outputs.iter().filter(|o| o.announced).map(|o| o.id)
    }

    fn output(&self, id: DisplayId) -> Option<&Output<K::Map>> {
        self.outputs.iter().find(|o| o.id == id && o.announced)
    }

    fn output_mut(&mut self, id: DisplayId) -> Option<&mut Output<K::Map>> {
        self.outputs.iter_mut().find(|o| o.id == id && o.announced)
    }

    /// The connector's name, as `HDMI-A-1`.
    pub fn name(&self, id: DisplayId) -> Option<&str> {
        self.output(id).map(|o| o.name.as_str())
    }

    /// The size in pixels of the display's mode.
    pub fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        self.output(id).map(Output::size)
    }

    /// The refresh rate in mHz of the display's mode.
    pub fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        self.output(id).map(|o| o.mode.refresh_mhz())
    }

    /// Whether a swap carries its damage to the device. Damage clips go
    /// only with atomic commits (aaronsb/kmscon#84).
    pub fn supports_damage(&self, id: DisplayId) -> bool {
        !self.legacy
            && self
                .output(id)
                .and_then(|o| o.planes.as_ref())
                .is_some_and(|p| p.damage)
    }

    /// Whether a frame swapped is still waiting to reach the screen.
    pub fn is_flipping(&self, id: DisplayId) -> bool {
        self.output(id).is_some_and(|o| o.flipping)
    }

    /// Whether a swap with no new damage has something to send: a moved
    /// cursor, or a frame whose swap failed.
    pub fn need_commit(&self, id: DisplayId) -> bool {
        self.awake
            && self
                .output(id)
                .is_some_and(|o| o.online && (o.fresh || (o.need_commit && !self.legacy)))
    }

    /// Whether the pointer is on the display's cursor plane, set up by
    /// [`Drm2d::setup_cursor`], so the compositor draws none. False on a
    /// display without a cursor plane, as with the legacy calls, or whose
    /// plane refused the image.
    pub fn hardware_cursor(&self, id: DisplayId) -> bool {
        self.output(id).is_some_and(|o| o.cursor.is_some())
    }

    /// The display's power state, `None` where its connector has none.
    pub fn dpms(&self, id: DisplayId) -> Option<Dpms> {
        self.output(id).and_then(|o| o.dpms).map(|(_, d)| d)
    }

    /// Lends the back buffer for the next frame, while the display is
    /// awake, online and no flip or modeset is pending.
    pub fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        if !self.awake || self.waiting.is_some() {
            return None;
        }
        let o = self.output_mut(id)?;
        if !o.online || o.flipping {
            return None;
        }
        o.lent = true;
        let (width, height) = o.size();
        let age = o.ages.age(o.back);
        let buffer = o.buffers.get_mut(o.back)?;
        let stride = buffer.dumb.pitch;
        let len = usize::try_from(u64::from(stride).saturating_mul(u64::from(height))).ok()?;
        let pixels = buffer.map.as_mut().get_mut(..len)?;
        Some(Canvas {
            pixels,
            width,
            height,
            stride,
            age,
        })
    }

    /// Presents what changed: the frame drawn into the canvas lent since
    /// the last swap, with its `damage`, or else the cursor plane's state
    /// alone, keeping the front buffer on screen, or else nothing. A
    /// rectangle covering the screen is the whole screen. Without a canvas
    /// lent, `damage` is ignored. A failed swap does not count as
    /// presented: its error reaches the caller, and the frame, its damage
    /// and the cursor's state go with the next swap (aaronsb/kmscon#93).
    pub fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let (awake, waiting) = (self.awake, self.waiting.is_some());
        let (legacy, hotspot) = (self.legacy, self.hotspot);
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id && o.announced) else {
            return Err(SwapError::Off);
        };
        // The frame drawn into the canvas is kept whatever the swap's
        // outcome: the back buffer holds it, so it counts as a frame.
        if std::mem::take(&mut o.lent) && !damage.is_empty() {
            let (w, h) = o.size();
            o.damage.add(damage, w, h);
            o.ages.drawn(o.back);
            o.fresh = true;
        }
        if !awake || !o.online || !o.dpms_on() {
            return Err(SwapError::Off);
        }
        if waiting || o.flipping {
            return Err(SwapError::Busy);
        }
        let swapped = if o.fresh {
            Swapped::Frame
        } else if o.need_commit && !legacy {
            Swapped::Cursor
        } else {
            return Ok(Swapped::Nothing);
        };
        let sent = if legacy {
            self.card.page_flip(o.crtc, o.back_fb())
        } else {
            let clips = match o.damage.clips() {
                Some(rects)
                    if swapped == Swapped::Frame && o.planes.as_ref().is_some_and(|p| p.damage) =>
                {
                    match self.card.create_blob(&clip_bytes(rects)) {
                        Ok(blob) => Some(blob),
                        Err(e) => {
                            log::warn!("cannot create the damage blob: {e}");
                            None
                        }
                    }
                }
                _ => None,
            };
            let mut req = Request::default();
            // A cursor-only commit leaves the primary plane as it is, so
            // the kernel sees no damage to it.
            let prepared = match swapped {
                Swapped::Frame => o.prepare(&mut req, o.back_fb(), o.mode_blob, clips, hotspot),
                _ => o.prepare_cursor(&mut req, hotspot),
            };
            let sent = prepared.and_then(|()| {
                self.card
                    .commit(&req, DRM_MODE_PAGE_FLIP_EVENT | DRM_MODE_ATOMIC_NONBLOCK)
            });
            if let Some(blob) = clips {
                if let Err(e) = self.card.destroy_blob(blob) {
                    log::warn!("cannot destroy the damage blob: {e}");
                }
            }
            sent
        };
        if let Err(e) = sent {
            if errno(&e) != Some(Errno::BUSY) {
                log::warn!("cannot flip {}: {e}", o.name);
            }
            return Err(e.into());
        }
        o.start_flip();
        if swapped == Swapped::Frame {
            o.back = o.front();
            o.damage.clear();
            o.fresh = false;
        }
        o.need_commit = false;
        self.settle();
        Ok(swapped)
    }

    /// Sets a display's power state. A display whose connector has no
    /// DPMS property keeps its state.
    pub fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        let awake = self.awake;
        let o = self
            .outputs
            .iter_mut()
            .find(|o| o.id == id && o.announced)
            .filter(|o| awake && o.online)
            .ok_or_else(|| invalid("the display is not online"))?;
        let Some((prop, now)) = o.dpms else {
            return Ok(());
        };
        if now == dpms {
            return Ok(());
        }
        log::info!("setting DPMS of {} to {dpms:?}", o.name);
        self.card
            .set_connector_property(o.id.0, prop, dpms_value(dpms))?;
        o.dpms = Some((prop, dpms));
        Ok(())
    }

    /// Puts `pixels`, ARGB8888 rows of `width`, on the display's cursor
    /// plane, with its hotspot at `hot`. The image is cut to the plane's
    /// size, at most [`CURSOR_MAX`] a side, and stays hidden until
    /// [`Drm2d::show_cursor`].
    pub fn setup_cursor(
        &mut self,
        id: DisplayId,
        pixels: &[u32],
        width: u32,
        height: u32,
        hot: (i32, i32),
    ) -> io::Result<()> {
        let awake = self.awake;
        let unsupported = || io::Error::from(io::ErrorKind::Unsupported);
        let o = self
            .outputs
            .iter()
            .find(|o| o.id == id && o.announced)
            .filter(|o| awake && o.online)
            .ok_or_else(|| invalid("the display is not online"))?;
        if o.planes.as_ref().is_none_or(|p| p.cursor.is_none()) {
            return Err(unsupported());
        }
        let side = |cap| {
            self.card
                .cap(cap)
                .ok()
                .map(|v| u32::try_from(v.min(CURSOR_MAX)).unwrap_or(0))
                .filter(|&v| v > 0)
                .ok_or_else(unsupported)
        };
        let (cw, ch) = (side(DRM_CAP_CURSOR_WIDTH)?, side(DRM_CAP_CURSOR_HEIGHT)?);
        self.destroy_cursor(id);
        let dumb = self.card.create_dumb(cw, ch)?;
        let made = self.card.add_fb(&dumb, cw, ch, ARGB8888).and_then(|fb| {
            let mut map = self.card.map_dumb(&dumb).inspect_err(|_| {
                let _ = self.card.rm_fb(fb);
            })?;
            let dst = map.as_mut();
            dst.fill(0);
            let pitch = usize::try_from(dumb.pitch).unwrap_or(0);
            let cols = usize::try_from(width.min(cw)).unwrap_or(0);
            let src_w = usize::try_from(width).unwrap_or(0);
            for y in 0..usize::try_from(height.min(ch)).unwrap_or(0) {
                let src = pixels.get(y.saturating_mul(src_w)..).unwrap_or_default();
                let row = dst.get_mut(y.saturating_mul(pitch)..).unwrap_or_default();
                for (px, out) in src.iter().take(cols).zip(row.chunks_exact_mut(4)) {
                    out.copy_from_slice(&px.to_ne_bytes());
                }
            }
            Ok(fb)
        });
        let fb = match made {
            Ok(fb) => fb,
            Err(e) => {
                let _ = self.card.destroy_dumb(dumb.handle);
                return Err(e);
            }
        };
        if let Some(o) = self.output_mut(id) {
            o.cursor = Some(Cursor {
                dumb,
                fb,
                width: cw,
                height: ch,
                hot,
                ..Cursor::default()
            });
        }
        Ok(())
    }

    /// Takes the cursor plane's image away, hiding it with the next swap.
    pub fn destroy_cursor(&mut self, id: DisplayId) {
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) else {
            return;
        };
        if let Some(c) = o.cursor.take() {
            o.need_commit |= c.visible;
            let _ = self.card.rm_fb(c.fb);
            let _ = self.card.destroy_dumb(c.dumb.handle);
        }
    }

    /// Shows the cursor with its hotspot at (`x`, `y`), from the next
    /// swap.
    pub fn show_cursor(&mut self, id: DisplayId, x: i32, y: i32) -> io::Result<()> {
        let o = self
            .output_mut(id)
            .ok_or_else(|| invalid("no such display"))?;
        let c = o
            .cursor
            .as_mut()
            .ok_or_else(|| invalid("the display has no cursor set up"))?;
        if !c.visible || c.at != (x, y) {
            o.need_commit = true;
        }
        c.at = (x, y);
        c.visible = true;
        Ok(())
    }

    /// Hides the cursor from the next swap.
    pub fn hide_cursor(&mut self, id: DisplayId) {
        if let Some(o) = self.output_mut(id) {
            if let Some(c) = o.cursor.as_mut().filter(|c| c.visible) {
                c.visible = false;
                o.need_commit = true;
            }
        }
    }

    /// Moves where the cursor's coordinates start, as for a grid placed
    /// away from the display's corner.
    pub fn set_cursor_offset(&mut self, id: DisplayId, x: i32, y: i32) {
        if let Some(o) = self.output_mut(id) {
            if let Some(c) = o.cursor.as_mut() {
                if c.offset != (x, y) {
                    o.need_commit = true;
                }
                c.offset = (x, y);
            }
        }
    }

    /// Takes DRM master and sets every display up again. While another
    /// process holds master, or the setup fails, the wake-up is tried
    /// again every [`crate::WAKE_RETRY_MS`], at most
    /// [`crate::WAKE_RETRIES`] times.
    pub fn wake_up(&mut self) -> io::Result<()> {
        let woken = self.try_wake();
        self.retries = woken.is_err().then_some(0);
        self.settle();
        woken
    }

    fn try_wake(&mut self) -> io::Result<()> {
        if !self.master {
            self.card.set_master().inspect_err(|e| {
                log::warn!("cannot take DRM master: {e}");
            })?;
            self.master = true;
        }
        self.awake = true;
        if let Err(e) = self.scan(true, true) {
            self.release_master();
            self.awake = false;
            return Err(e);
        }
        self.events.push(Event::Awake);
        Ok(())
    }

    /// A wake-up retry's timer fired.
    pub(crate) fn retry_wake(&mut self) {
        let Some(tried) = self.retries else {
            return;
        };
        if self.try_wake().is_ok() {
            self.retries = None;
        } else if tried.saturating_add(1) >= crate::WAKE_RETRIES {
            log::error!("cannot wake the display after {tried} retries");
            self.retries = None;
            self.events.push(Event::WakeFailed);
        } else {
            self.retries = Some(tried.saturating_add(1));
        }
        self.settle();
    }

    /// Lets the displays go: blanks them, waits for the flips in flight
    /// (at most [`crate::FLIP_WAIT_MS`]), hides the cursor and overlay
    /// planes and drops DRM master. [`Event::Asleep`] tells when it is
    /// done.
    pub fn sleep(&mut self) {
        self.retries = None;
        if !self.awake {
            self.events.push(Event::Asleep);
            self.settle();
            return;
        }
        for o in self.outputs.iter_mut().filter(|o| o.online) {
            if o.dpms_on() {
                o.clear_front();
            }
        }
        self.waiting = Some(Waiting::Sleep);
        self.resolve();
        self.settle();
    }

    fn finish_sleep(&mut self) {
        if self.master && !self.legacy {
            let mut req = Request::default();
            self.clear_planes(&mut req);
            if !req.entries.is_empty() {
                if let Err(e) = self.card.commit(&req, DRM_MODE_ATOMIC_NONBLOCK) {
                    log::warn!("cannot hide the cursor and overlay planes: {e}");
                }
            }
        }
        self.release_master();
        self.awake = false;
        for o in &mut self.outputs {
            o.flipping = false;
        }
        self.events.push(Event::Asleep);
    }

    fn release_master(&mut self) {
        if let Err(e) = self.card.drop_master() {
            log::debug!("cannot drop DRM master: {e}");
        }
        self.master = false;
    }

    /// Looks for connectors plugged in or out, as on a udev hotplug event,
    /// and sets up what changed. A failure is retried as a wake-up is.
    pub fn poll(&mut self) -> io::Result<()> {
        let scanned = self.scan(false, false);
        if scanned.is_err() && self.awake {
            self.retries.get_or_insert(0);
        }
        self.settle();
        scanned
    }

    /// Reads the card's events: the page flips completed.
    pub fn dispatch(&mut self) -> io::Result<()> {
        let mut flipped = Vec::new();
        let read = self.card.read_events(&mut flipped);
        for crtc in flipped {
            self.flipped(crtc);
        }
        self.resolve();
        self.settle();
        read
    }

    /// A flip's event arrived. One that comes after its flip was taken as
    /// done still tells the owner it may draw, as a swap tried meanwhile
    /// was busy.
    fn flipped(&mut self, crtc: u32) {
        let ready = self.awake && self.waiting.is_none();
        match self.outputs.iter_mut().find(|o| o.crtc == crtc) {
            Some(o) => {
                o.flipping = false;
                if o.announced && ready {
                    self.events.push(Event::Flipped(o.id));
                }
            }
            None => log::debug!("a page flip on CRTC {crtc}, which drives no display"),
        }
    }

    /// The bound on the flip in flight on `crtc` ran out: the flip is taken
    /// as done, so drawing and any operation waiting on it go ahead.
    pub(crate) fn flip_expired(&mut self, crtc: u32) {
        let Some(o) = self.outputs.iter_mut().find(|o| o.crtc == crtc) else {
            return;
        };
        o.flip_timer = None;
        if !o.flipping {
            return;
        }
        if std::mem::replace(&mut self.lost_flip, true) {
            log::debug!("no page flip event for {}; taking the flip as done", o.name);
        } else {
            log::warn!(
                "no page flip event for {} within {} ms; taking the flip as done",
                o.name,
                crate::FLIP_WAIT_MS
            );
        }
        self.flipped(crtc);
        self.resolve();
        self.settle();
    }

    /// Runs the operation that waits, once no flip is in flight.
    fn resolve(&mut self) {
        if self.outputs.iter().any(|o| o.flipping) {
            return;
        }
        match self.waiting.take() {
            Some(Waiting::Modeset) => {
                if let Err(e) = self.modeset() {
                    log::debug!("the modeset failed: {e}");
                    self.retries.get_or_insert(0);
                }
            }
            Some(Waiting::Sleep) => self.finish_sleep(),
            None => {}
        }
    }

    /// Arms and disarms the loop's timers to match what waits: a bound for
    /// each flip in flight, armed when the flip starts, and the wake-up
    /// retries.
    fn settle(&mut self) {
        let Some(link) = &self.link else {
            return;
        };
        for o in &mut self.outputs {
            let want = o.flipping.then_some(o.flips);
            match (want, o.flip_timer.take()) {
                (Some(flip), Some((t, armed))) if armed == flip => o.flip_timer = Some((t, armed)),
                (want, armed) => {
                    if let Some((t, _)) = armed {
                        link.disarm(t);
                    }
                    o.flip_timer = want.and_then(|f| Some((link.arm(Clock::Flip(o.crtc))?, f)));
                }
            }
        }
        let token = &mut self.timers.wake_retry;
        match (self.retries.is_some(), token.take()) {
            (true, None) => *token = link.arm(Clock::WakeRetry),
            (false, Some(t)) => link.disarm(t),
            (_, t) => *token = t,
        }
    }

    /// The hotplug scan: binds a display to each connected connector with
    /// modes, lets go of those gone, and sets modes when anything is new.
    fn scan(&mut self, read_dpms: bool, force: bool) -> io::Result<()> {
        if !self.awake {
            return Ok(());
        }
        let res = self.card.resources()?;
        for o in &mut self.outputs {
            o.available = false;
        }
        let mut needs = force;
        for &id in &res.connectors {
            // A connector the kernel lists can be gone by the time it is
            // read, as during a monitor's power cycle.
            let conn = match self.card.connector(id) {
                Ok(conn) => conn,
                Err(e) => {
                    log::debug!("connector {id} is gone: {e}");
                    continue;
                }
            };
            if !conn.connected || conn.modes.is_empty() {
                continue;
            }
            if let Some(o) = self.outputs.iter_mut().find(|o| o.id.0 == id) {
                o.available = true;
                if !o.online {
                    needs = true;
                } else if read_dpms {
                    self.restore_dpms(id);
                }
                continue;
            }
            if let Some(o) = self.bind(&res, &conn) {
                self.outputs.push(o);
                needs = true;
            }
        }
        let (kept, gone): (Vec<_>, Vec<_>) = self.outputs.drain(..).partition(|o| o.available);
        self.outputs = kept;
        for o in gone {
            log::info!("display {} is gone", o.name);
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
            self.free(o);
        }
        if self.outputs.is_empty() {
            if self.master {
                self.release_master();
            }
            return Ok(());
        }
        // The last display's going dropped master; a display back takes
        // it again before its modeset (aaronsb/kmscon#237).
        if !self.master {
            self.card.set_master()?;
            self.master = true;
        }
        if needs {
            self.request_modeset()
        } else {
            self.announce();
            Ok(())
        }
    }

    /// Sets the DPMS state the display had again, where the connector's
    /// changed without vtium, as across a monitor's power cycle.
    fn restore_dpms(&mut self, id: u32) {
        let Ok(props) = self.card.properties(id, Object::Connector) else {
            return;
        };
        let Some(o) = self.outputs.iter_mut().find(|o| o.id.0 == id) else {
            return;
        };
        let Some((prop, ours)) = o.dpms else {
            return;
        };
        if value(&props, "DPMS").map(dpms_of) != Some(ours) {
            log::debug!("DPMS of {} changed; setting it again", o.name);
            if let Err(e) = self.card.set_connector_property(id, prop, dpms_value(ours)) {
                log::warn!("cannot set DPMS of {}: {e}", o.name);
            }
        }
    }

    /// A display for a connected connector, or `None` when it has no free
    /// CRTC or no primary plane.
    fn bind(&self, res: &Resources, conn: &Connector) -> Option<Output<K::Map>> {
        let name = connector_name(conn.kind, conn.kind_id);
        let shown = (conn.encoder != 0)
            .then(|| self.card.encoder(conn.encoder).ok())
            .flatten()
            .filter(|e| e.crtc != 0)
            .and_then(|e| self.card.crtc_mode(e.crtc).ok().flatten());
        let wanted = self.policy.wanted.as_ref().and_then(|f| f(&name));
        let choice = mode::choose(
            &conn.modes,
            shown.as_ref(),
            wanted,
            self.policy.use_original,
        )?;
        let connector_props = self
            .card
            .properties(conn.id, Object::Connector)
            .unwrap_or_default();
        let dpms = connector_props
            .iter()
            .find(|p| p.name == "DPMS")
            .map(|p| (p.id, dpms_of(p.value)));
        let Some((crtc, crtc_index)) = self.find_crtc(res, conn) else {
            log::error!("no free CRTC for {name}");
            return None;
        };
        let planes = if self.legacy {
            None
        } else {
            let Some(mut planes) = self.find_planes(crtc, crtc_index) else {
                log::error!("no primary plane for {name}");
                return None;
            };
            planes.connector_props = Props::new(&connector_props);
            Some(planes)
        };
        log::info!(
            "display {name} at {}x{}",
            choice.current.size().0,
            choice.current.size().1
        );
        Some(Output {
            id: DisplayId(conn.id),
            name,
            crtc,
            planes,
            mode: choice.current,
            choice,
            mode_blob: 0,
            buffers: Vec::new(),
            back: 0,
            ages: Ages::default(),
            damage: Damage::default(),
            lent: false,
            fresh: false,
            need_commit: false,
            cursor: None,
            dpms,
            online: false,
            flipping: false,
            flips: 0,
            flip_timer: None,
            available: true,
            announced: false,
        })
    }

    /// The CRTC the connector's encoder drives, if no display has it, else
    /// the first free one an encoder of the connector can drive.
    fn find_crtc(&self, res: &Resources, conn: &Connector) -> Option<(u32, usize)> {
        let free = |crtc: u32| !self.outputs.iter().any(|o| o.crtc == crtc);
        let index = |crtc: u32| res.crtcs.iter().position(|&c| c == crtc);
        if conn.encoder != 0 {
            if let Ok(enc) = self.card.encoder(conn.encoder) {
                if enc.crtc != 0 && free(enc.crtc) {
                    return index(enc.crtc).map(|i| (enc.crtc, i));
                }
            }
        }
        conn.encoders.iter().find_map(|&e| {
            let enc = self.card.encoder(e).ok()?;
            res.crtcs.iter().enumerate().find_map(|(i, &crtc)| {
                let possible = u32::try_from(i)
                    .ok()
                    .and_then(|i| 1u32.checked_shl(i))
                    .is_some_and(|bit| enc.possible_crtcs & bit != 0);
                (possible && free(crtc)).then_some((crtc, i))
            })
        })
    }

    /// The primary and cursor planes for the CRTC at `index`, with their
    /// properties and the CRTC's.
    fn find_planes(&self, crtc: u32, index: usize) -> Option<Planes> {
        let bit = u32::try_from(index)
            .ok()
            .and_then(|i| 1u32.checked_shl(i))?;
        let mut planes = Planes::default();
        let mut primary = None;
        for plane in self.card.planes().ok()? {
            if self.card.plane_crtcs(plane).unwrap_or(0) & bit == 0 {
                continue;
            }
            let Ok(props) = self.card.properties(plane, Object::Plane) else {
                continue;
            };
            match value(&props, "type") {
                Some(PLANE_PRIMARY) if primary.is_none() => {
                    planes.damage = props.iter().any(|p| p.name == "FB_DAMAGE_CLIPS");
                    planes.primary_props = Props::new(&props);
                    primary = Some(plane);
                }
                Some(PLANE_CURSOR) if planes.cursor.is_none() => {
                    planes.cursor = Some((plane, Props::new(&props)));
                }
                _ => {}
            }
            if primary.is_some() && planes.cursor.is_some() {
                break;
            }
        }
        planes.primary = primary?;
        planes.crtc_props = Props::new(&self.card.properties(crtc, Object::Crtc).ok()?);
        Some(planes)
    }

    /// Hides every cursor and overlay plane, which another program may
    /// have left in the middle of the screen.
    fn clear_planes(&self, req: &mut Request) {
        for plane in self.card.planes().unwrap_or_default() {
            let Ok(props) = self.card.properties(plane, Object::Plane) else {
                continue;
            };
            if matches!(value(&props, "type"), Some(PLANE_CURSOR | PLANE_OVERLAY)) {
                let props = Props::new(&props);
                let cleared = props
                    .set(req, plane, "CRTC_ID", 0)
                    .and_then(|()| props.set(req, plane, "FB_ID", 0));
                if let Err(e) = cleared {
                    log::warn!("cannot hide plane {plane}: {e}");
                }
            }
        }
    }

    /// A modeset now, or once no flip is in flight.
    fn request_modeset(&mut self) -> io::Result<()> {
        if self.waiting == Some(Waiting::Sleep) {
            return Ok(());
        }
        if self.outputs.iter().any(|o| o.flipping) {
            self.waiting = Some(Waiting::Modeset);
            return Ok(());
        }
        self.modeset()
    }

    /// Sets every display's mode, then announces each display set.
    fn modeset(&mut self) -> io::Result<()> {
        let set = if self.legacy {
            self.legacy_modeset();
            Ok(())
        } else {
            match self.atomic_modeset() {
                Err(Failure::Test(e)) if errno(&e) != Some(Errno::ACCESS) => {
                    log::warn!("retrying the modeset with each display's default mode");
                    let modes: Vec<Mode> = self.outputs.iter().map(|o| o.mode).collect();
                    for o in &mut self.outputs {
                        o.mode = o.choice.default;
                    }
                    let retried = self.atomic_modeset();
                    if retried.is_err() {
                        for (o, mode) in self.outputs.iter_mut().zip(modes) {
                            o.mode = mode;
                        }
                    }
                    retried.map_err(Failure::into_inner)
                }
                set => set.map_err(Failure::into_inner),
            }
        };
        self.announce();
        set
    }

    /// Announces each display online: new ones as added, the rest to be
    /// redrawn.
    fn announce(&mut self) {
        for o in self.outputs.iter_mut().filter(|o| o.online) {
            if o.announced {
                if o.ages.frame == 0 {
                    self.events.push(Event::Redraw(o.id));
                }
            } else {
                o.announced = true;
                self.events.push(Event::Added(o.id));
            }
        }
    }

    /// Gives `o` two cleared buffers the size of its mode.
    fn allocate(&self, o: &mut Output<K::Map>) -> io::Result<()> {
        let (w, h) = o.size();
        let sized = o.buffers.len() == 2 && self.buffer_size_matches(o);
        if !sized {
            self.free_buffers(o);
            for _ in 0..2 {
                let buffer = self.buffer(w, h).inspect_err(|_| self.free_buffers(o))?;
                o.buffers.push(buffer);
            }
            o.back = 0;
        }
        for b in &mut o.buffers {
            b.map.as_mut().fill(0);
        }
        o.ages = Ages::default();
        Ok(())
    }

    fn buffer_size_matches(&self, o: &Output<K::Map>) -> bool {
        o.buffers.first().is_some_and(|b| b.size == o.size())
    }

    fn buffer(&self, width: u32, height: u32) -> io::Result<Buffer<K::Map>> {
        let dumb = self.card.create_dumb(width, height)?;
        let fb = match self.card.add_fb(&dumb, width, height, XRGB8888) {
            Ok(fb) => fb,
            Err(e) => {
                let _ = self.card.destroy_dumb(dumb.handle);
                return Err(e);
            }
        };
        match self.card.map_dumb(&dumb) {
            Ok(map) => Ok(Buffer {
                dumb,
                fb,
                map,
                size: (width, height),
            }),
            Err(e) => {
                let _ = self.card.rm_fb(fb);
                let _ = self.card.destroy_dumb(dumb.handle);
                Err(e)
            }
        }
    }

    fn free_buffers(&self, o: &mut Output<K::Map>) {
        for b in o.buffers.drain(..) {
            drop(b.map);
            let _ = self.card.rm_fb(b.fb);
            if let Err(e) = self.card.destroy_dumb(b.dumb.handle) {
                log::warn!("cannot destroy a dumb buffer: {e}");
            }
        }
    }

    fn free(&self, mut o: Output<K::Map>) {
        if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
            link.disarm(t);
        }
        self.free_buffers(&mut o);
        if let Some(c) = o.cursor.take() {
            let _ = self.card.rm_fb(c.fb);
            let _ = self.card.destroy_dumb(c.dumb.handle);
        }
        if o.mode_blob != 0 {
            let _ = self.card.destroy_blob(o.mode_blob);
        }
    }

    /// One atomic modeset of every display: tried first as a test-only
    /// commit, then committed without blocking, with a flip event.
    fn atomic_modeset(&mut self) -> Result<(), Failure> {
        let mut req = Request::default();
        self.clear_planes(&mut req);
        let mut blobs = Vec::with_capacity(self.outputs.len());
        let mut outputs = std::mem::take(&mut self.outputs);
        let mut prepared = Ok(());
        for o in &mut outputs {
            log::info!(
                "preparing the modeset of {} at {}x{}",
                o.name,
                o.size().0,
                o.size().1
            );
            prepared = self
                .allocate(o)
                .and_then(|()| self.card.create_blob(&mode_bytes(&o.mode.0)))
                .and_then(|blob| {
                    blobs.push(blob);
                    // The committed buffer becomes the front one.
                    o.prepare(&mut req, o.back_fb(), blob, None, self.hotspot)
                });
            if prepared.is_err() {
                break;
            }
        }
        let outcome = prepared.map_err(Failure::Other).and_then(|()| {
            self.card
                .commit(
                    &req,
                    DRM_MODE_ATOMIC_TEST_ONLY | DRM_MODE_ATOMIC_ALLOW_MODESET,
                )
                .map_err(|e| {
                    // EACCES while another process holds master is expected.
                    if errno(&e) == Some(Errno::ACCESS) {
                        log::debug!("test-only atomic commit refused: {e}");
                    } else {
                        log::error!("test-only atomic commit failed: {e}");
                    }
                    Failure::Test(e)
                })?;
            self.card
                .commit(
                    &req,
                    DRM_MODE_ATOMIC_ALLOW_MODESET
                        | DRM_MODE_ATOMIC_NONBLOCK
                        | DRM_MODE_PAGE_FLIP_EVENT,
                )
                .map_err(|e| {
                    log::error!("modeset atomic commit failed: {e}");
                    Failure::Other(e)
                })
        });
        let mut blobs = blobs.into_iter();
        for o in &mut outputs {
            let blob = blobs.next();
            if outcome.is_ok() {
                if o.mode_blob != 0 {
                    let _ = self.card.destroy_blob(o.mode_blob);
                }
                o.mode_blob = blob.unwrap_or(0);
                o.online = true;
                o.start_flip();
                o.back = o.front();
                o.damage.set_whole();
                o.fresh = false;
                o.ages = Ages::default();
            } else {
                if let Some(blob) = blob {
                    let _ = self.card.destroy_blob(blob);
                }
                if !o.online {
                    self.free_buffers(o);
                }
            }
        }
        self.outputs = outputs;
        outcome
    }

    /// The legacy modeset: each CRTC in turn, with no flip event.
    fn legacy_modeset(&mut self) {
        let mut outputs = std::mem::take(&mut self.outputs);
        for o in &mut outputs {
            log::info!(
                "preparing the modeset of {} at {}x{}",
                o.name,
                o.size().0,
                o.size().1
            );
            if let Err(e) = self.allocate(o) {
                log::error!("cannot allocate buffers for {}: {e}", o.name);
                continue;
            }
            // Another program may have left its cursor on the CRTC.
            if let Err(e) = self.card.hide_cursor(o.crtc) {
                log::warn!("cannot hide the hardware cursor: {e}");
            }
            match self.card.set_crtc(o.crtc, o.back_fb(), o.id.0, &o.mode) {
                Ok(()) => {
                    o.online = true;
                    o.back = o.front();
                }
                Err(e) => {
                    log::error!("cannot set the CRTC of {}: {e}", o.name);
                    if !o.online {
                        self.free_buffers(o);
                    }
                }
            }
        }
        self.outputs = outputs;
    }

    #[cfg(test)]
    pub(crate) fn card(&self) -> &K {
        &self.card
    }

    #[cfg(test)]
    pub(crate) fn waits(&self) -> (bool, bool) {
        (self.waiting.is_some(), self.retries.is_some())
    }
}

/// How an atomic modeset failed: its test-only commit, or anything else.
enum Failure {
    Test(io::Error),
    Other(io::Error),
}

impl Failure {
    fn into_inner(self) -> io::Error {
        match self {
            Failure::Test(e) | Failure::Other(e) => e,
        }
    }
}

impl<K: Kms> Drop for Drm2d<K> {
    fn drop(&mut self) {
        if let Some(link) = &self.link {
            if let Some(token) = self.timers.wake_retry.take() {
                link.disarm(token);
            }
        }
        for o in std::mem::take(&mut self.outputs) {
            self.free(o);
        }
    }
}
#[cfg(test)]
mod tests;
