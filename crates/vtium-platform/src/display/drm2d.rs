//! drm2d: a DRM card's displays, drawn on the CPU into dumb buffers.
//!
//! Each display has one, two or three dumb buffers in XRGB8888, which its
//! buffer source makes ([`buffers`]), as many as [`Buffers`] says. The
//! display lends the back one in a [`Token`], the head's render thread
//! draws into it, and [`Drm2d::swap`] takes the token back and flips to the
//! buffer with a non-blocking commit that asks for a completion event; the
//! display is busy until that event arrives. A
//! device with atomic modesetting commits through atomic requests,
//! carrying the frame's damage as `FB_DAMAGE_CLIPS` where the primary plane
//! takes them and the hardware cursor on the cursor plane; any other
//! device, a device with universal planes and no atomic commits among
//! them, uses the legacy calls and presents whole frames.

mod buffers;
mod dark;
mod output;

pub use output::CURSOR_MAX;
mod source;

use std::cell::RefCell;
use std::io;
use std::sync::Arc;

use drm_ffi::{
    DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH, DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC,
    DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, DRM_CLIENT_CAP_UNIVERSAL_PLANES,
    DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_ATOMIC_TEST_ONLY,
    DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::io::Errno;

use super::edid::{Edid, EdidId};
use super::kms::{Connector, CrtcEvent, Encoder, Kms, Object, Property, Request, Resources};
use super::mode::{self, Mode, ModePolicy};
use super::path;
use super::{
    Ages, Buffers, Damage, DarkPolicy, Density, DisplayId, Dpms, Event, Identity, Loan, Memory,
    Rect, SwapError, Swapped, Token, lock,
};
use buffers::{Buffer, Dumbs, Source};
use dark::Dark;
use output::{
    ARGB8888, Cursor, Output, PLANE_CURSOR, PLANE_OVERLAY, PLANE_PRIMARY, Planes, Props,
    clip_bytes, connector_name, dpms_value, invalid, mode_bytes, value,
};
use source::{Clock, Link, Timers};

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}

/// The physical size of connector `name`, `mm` as the connector reports
/// it, or where it reports a side of 0, the size its EDID in sysfs states
/// (ADR-1301 D3); `mm` where that states none either.
fn physical_size(name: &str, mm: (u32, u32), edid: Option<&Edid>) -> (u32, u32) {
    if mm.0 != 0 && mm.1 != 0 {
        return mm;
    }
    let edid = edid.and_then(|e| e.mm);
    if let Some((w, h)) = edid {
        log::info!("display {name} reports no physical size: its EDID in sysfs states {w}x{h} mm");
    }
    edid.unwrap_or(mm)
}

/// A connected connector as a scan finds it: its name, the encoder it is
/// on and the mode that encoder's CRTC shows, its properties, and its EDID
/// in sysfs, read once for its identity and its physical size.
struct Found {
    name: String,
    current: Option<Encoder>,
    shown: Option<Mode>,
    props: Vec<Property>,
    /// The connector is visible: its CRTC shows a mode and is on, by
    /// `ACTIVE` on an atomic card and by the connector's DPMS on a legacy
    /// one, either read as on where the property is missing.
    visible: bool,
    edid: Option<Edid>,
    /// The text of its `PATH` property, where it has one.
    path: Option<String>,
}

/// An operation that waits until no page flip is in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    /// A modeset, a wake-up's when `wake`.
    Modeset {
        wake: bool,
    },
    Sleep,
}

/// What a scan of the connectors is for. Only a wake-up's commits a
/// modeset while blanked (the operator's decision on vtium/vtium#18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scan {
    /// A hotplug: sets modes when anything is new.
    Hotplug,
    /// A retry after a hotplug or an unblank failed: sets every mode.
    Retry,
    /// A wake-up: sets every mode, blanked or not.
    WakeUp,
}

/// A DRM card's displays, drawn into the buffers `S` makes, dumb buffers
/// by default. Outside the crate the card is a [`Card`]; the tests stand
/// one in.
#[expect(private_bounds, reason = "the card is a Card outside the crate")]
pub struct Drm2d<K: Kms, S: Source<K> = Dumbs> {
    card: K,
    source: S,
    /// The buffers each display keeps.
    buffers: Buffers,
    /// Buffers a display let go of while a token for them was out, as at a
    /// modeset that gave it new ones: each stays allocated and mapped until
    /// its token comes back, and is freed then.
    retired: RefCell<Vec<Buffer<S::Memory>>>,
    /// The levels of the 1-bit or gray panel the card drives.
    levels: Option<u8>,
    /// No universal planes or no atomic commits: the legacy calls.
    legacy: bool,
    hotspot: bool,
    master: bool,
    /// The seat gave the card its displays: woken and not put to sleep
    /// since, though waking may have failed.
    foreground: bool,
    awake: bool,
    policy: ModePolicy,
    /// udev's `ID_PATH` of the card, for each display's identity.
    card_path: Option<String>,
    dark_policy: Option<Box<DarkPolicy>>,
    /// The card has been scanned once: a connector lit from then on was
    /// not lit at start.
    scanned: bool,
    outputs: Vec<Output<S::Memory>>,
    /// The identity of each display bound.
    identities: Vec<(DisplayId, Identity)>,
    /// The connected connectors the policy leaves dark, in the card's
    /// connector order.
    dark: Vec<Dark>,
    /// A modeset could not set the dark connectors off beside the lit
    /// displays, and that was logged as a warning.
    dark_refused: bool,
    waiting: Option<Waiting>,
    /// A flip's or a vblank's event was lost, and that was logged as a
    /// warning; the next event the card delivers clears it.
    lost_flip: bool,
    /// The blank powered every display down, and no input has come since:
    /// every display online was set to a DPMS state other than on, with
    /// its connector's DPMS property or without one.
    blanked: bool,
    /// A hotplug while blanked found a change that needs a modeset, which
    /// the input ending the blank runs.
    owed: bool,
    /// Wake-ups tried again so far, while retrying.
    retries: Option<usize>,
    /// The retries are a wake-up's; else a failed hotplug's or unblank's,
    /// or a mode owed's.
    waking: bool,
    /// The retries ran out with [`Event::WakeFailed`], and nothing has set
    /// the displays since: the next input tries again.
    wake_failed: bool,
    /// The displays the last legacy modeset could not set, at the mode
    /// chosen or at the mode they were set to. The wake-up retries try them
    /// again, and none is asked to redraw until a modeset sets it.
    unset: Vec<DisplayId>,
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
        let dumb = card
            .cap(DRM_CAP_DUMB_BUFFER)
            .inspect_err(|e| log::warn!("cannot read the card's dumb buffer capability: {e}"))
            .unwrap_or(0);
        if dumb == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the driver has no dumb buffers",
            ));
        }
        let levels = card.levels();
        Ok(Drm2d {
            card,
            source: Dumbs,
            buffers: Buffers::default(),
            retired: RefCell::new(Vec::new()),
            levels,
            legacy,
            hotspot,
            master: false,
            foreground: false,
            awake: false,
            policy,
            card_path: None,
            dark_policy: None,
            scanned: false,
            outputs: Vec::new(),
            identities: Vec::new(),
            dark: Vec::new(),
            dark_refused: false,
            waiting: None,
            lost_flip: false,
            blanked: false,
            owed: false,
            retries: None,
            waking: false,
            wake_failed: false,
            unset: Vec::new(),
            events: Vec::new(),
            timers: Timers::default(),
            link: None,
        })
    }
}

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Sets udev's `ID_PATH` of the card, which the identity of each
    /// connector the scan finds from then on carries.
    pub fn set_card_path(&mut self, path: Option<String>) {
        self.card_path = path;
    }

    /// Sets which displays are left dark, asked of each connector as it is
    /// connected from the next scan; unset, every display is lit.
    pub fn set_dark(&mut self, dark: Box<DarkPolicy>) {
        self.dark_policy = Some(dark);
    }

    /// The identity of a display online.
    pub fn identity(&self, id: DisplayId) -> Option<&Identity> {
        self.output(id)?;
        self.identities
            .iter()
            .find_map(|(i, identity)| (*i == id).then_some(identity))
    }

    /// The identity of each connected connector left dark, in the card's
    /// connector order.
    pub fn dark(&self) -> impl Iterator<Item = &Identity> + '_ {
        self.dark.iter().map(|d| &d.identity)
    }

    /// Lets go of the connector `connector` the policy left dark, so the
    /// next scan asks the policy of it again, as of one just plugged in:
    /// the head file's display guard lights it so (ADR-1401 D9). Returns
    /// whether it was dark.
    pub fn release_dark(&mut self, connector: &str) -> bool {
        let before = self.dark.len();
        self.dark.retain(|d| d.identity.connector != connector);
        self.dark.len() != before
    }

    /// The events since the last drain, in order.
    pub fn events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// The displays online, in the card's connector order, whatever order
    /// they were plugged in.
    pub fn displays(&self) -> impl Iterator<Item = DisplayId> + '_ {
        self.outputs.iter().filter(|o| o.announced).map(|o| o.id)
    }

    fn output(&self, id: DisplayId) -> Option<&Output<S::Memory>> {
        self.outputs.iter().find(|o| o.id == id && o.announced)
    }

    fn output_mut(&mut self, id: DisplayId) -> Option<&mut Output<S::Memory>> {
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

    /// Whether a frame swapped is still waiting to reach the screen, or a
    /// frame the card refused as busy waits for the display's next vblank.
    pub fn is_flipping(&self, id: DisplayId) -> bool {
        self.output(id).is_some_and(|o| o.flipping)
    }

    /// Whether a swap with no new damage has something to send: a moved
    /// cursor, or a frame whose swap failed.
    pub fn need_commit(&self, id: DisplayId) -> bool {
        self.awake
            && self.output(id).is_some_and(|o| {
                o.online
                    && o.dpms_on()
                    && ((o.fresh && !o.lent()) || (!self.legacy && o.cursor_changed()))
            })
    }

    /// Whether the pointer is on the display's cursor plane, set up by
    /// [`Drm2d::setup_cursor`], so the compositor draws none. False on a
    /// display without a cursor plane, as with the legacy calls, or whose
    /// plane refused the image.
    pub fn hardware_cursor(&self, id: DisplayId) -> bool {
        self.output(id).is_some_and(|o| o.cursor.is_some())
    }

    /// The levels of the display, 2 for a 1-bit panel and 4 or 16 for a
    /// gray one, which the card's driver says ([`super::depth`]); `None`
    /// for a color display.
    pub fn levels(&self, id: DisplayId) -> Option<u8> {
        self.levels.filter(|_| self.output(id).is_some())
    }

    /// The display's density, from its mode and the physical size its
    /// connector reports, or [`Density::ASSUMED`] where it reports none or
    /// one out of bounds.
    pub fn density(&self, id: DisplayId) -> Option<Density> {
        self.output(id).map(|o| Density::of(o.size(), o.mm))
    }

    /// The display's power state, `None` where its connector has none.
    pub fn dpms(&self, id: DisplayId) -> Option<Dpms> {
        self.output(id).and_then(|o| o.dpms).map(|(_, d)| d)
    }

    /// Whether the display takes a frame now: the card is awake and waits
    /// for no modeset or sleep, and the display is online, powered on, has
    /// no token out and has a buffer free to draw into, which with two
    /// buffers means it waits for no flip or vblank. [`Drm2d::lend`] lends
    /// a token only then.
    pub fn ready_for_frame(&self, id: DisplayId) -> bool {
        self.awake
            && self.waiting.is_none()
            && self
                .output(id)
                .is_some_and(|o| o.online && o.dpms_on() && o.buffer_free())
    }

    /// The prepare step: lends the buffer the next frame is drawn into, as
    /// a token with its age, while the display is ready for a frame
    /// ([`Drm2d::ready_for_frame`]). While the token is out the display
    /// lends no other. Give it back with [`Drm2d::swap`], or unused with
    /// [`Drm2d::give_back`]; a token dropped instead leaves every buffer of
    /// the display at age 0.
    pub fn lend(&mut self, id: DisplayId) -> Option<Token> {
        self.reap();
        if let Some(o) = self.output_mut(id)
            && o.loan.as_ref().is_some_and(|l| !l.out())
        {
            o.loan = None;
            o.forget();
        }
        if !self.ready_for_frame(id) {
            return None;
        }
        let o = self.output_mut(id)?;
        let buffer = o.buffers.get(o.back)?;
        let memory: Arc<Memory> = buffer.map.clone();
        let (token, loan) = Loan::lend(
            id,
            o.back,
            o.ages.age(o.back),
            o.lease,
            o.size(),
            buffer.pitch,
            memory,
        );
        o.loan = Some(loan);
        Some(token)
    }

    /// Takes back a token with nothing drawn into its buffer, as one the
    /// owner could not send to the render thread: the buffer holds what it
    /// held, and nothing is presented.
    pub fn give_back(&mut self, token: Token) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == token.id())
            && o.loan.as_ref().is_some_and(|l| l.lent(&token))
        {
            o.loan = None;
            if o.clear_owed {
                o.clear_owed = false;
                if let Some(memory) = token.memory() {
                    lock(memory).as_mut().fill(0);
                }
            }
        }
        drop(token);
        self.reap();
    }

    /// The swap: takes back the token lent for this frame and presents what
    /// changed, the frame drawn into its buffer with its `damage`, or else
    /// the cursor plane's state alone, keeping the front buffer on screen,
    /// or else nothing. A rectangle covering the screen is the whole
    /// screen. A failed swap does not count as presented: its error
    /// reaches the caller, and the frame, its damage and the cursor's state
    /// go with the next swap (aaronsb/kmscon#93).
    ///
    /// A token that comes back too late, to a display that has gone, has
    /// been given new buffers, or was let go for a VT release or powered
    /// down meanwhile, is dropped with [`SwapError::Dropped`]: its frame is
    /// not shown, and every buffer of the display is age 0, so its next
    /// frame is drawn whole.
    pub fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id && o.announced) else {
            drop(token);
            self.reap();
            return Err(SwapError::Dropped);
        };
        let Some(loan) = o.loan.take_if(|l| l.lent(&token)) else {
            return Err(SwapError::Dropped);
        };
        if !loan.current(&token, o.lease) {
            // The buffer holds part of a frame nobody counts, over any frame
            // kept from a failed swap: it goes whole.
            o.forget();
            o.damage.set_whole();
            if std::mem::take(&mut o.clear_owed)
                && let Some(memory) = token.memory()
            {
                lock(memory).as_mut().fill(0);
            }
            drop(token);
            self.reap();
            return Err(SwapError::Dropped);
        }
        drop(token);
        // The frame drawn into the token's buffer is kept whatever the
        // swap's outcome: the back buffer holds it, so it counts as a frame.
        if !damage.is_empty() {
            let (w, h) = o.size();
            o.damage.add(damage, w, h);
            o.ages.drawn(o.back);
            o.fresh = true;
        }
        self.reap();
        self.send(id)
    }

    /// Presents what changed with no new frame: a frame kept from a failed
    /// swap, or else the cursor plane's state alone, keeping the front
    /// buffer on screen, or else nothing. While a token is out, its frame
    /// carries the frame kept, and this sends the cursor plane alone.
    pub fn commit(&mut self, id: DisplayId) -> Result<Swapped, SwapError> {
        self.send(id)
    }

    fn send(&mut self, id: DisplayId) -> Result<Swapped, SwapError> {
        let (awake, waiting) = (self.awake, self.waiting.is_some());
        let (legacy, hotspot) = (self.legacy, self.hotspot);
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id && o.announced) else {
            return Err(SwapError::Off);
        };
        if !awake || !o.online || !o.dpms_on() {
            return Err(SwapError::Off);
        }
        if waiting || o.flipping {
            return Err(SwapError::Busy);
        }
        let swapped = if o.fresh && !o.lent() {
            Swapped::Frame
        } else if !legacy && o.cursor_changed() {
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
            let card = &self.card;
            let commit = |o: &Output<S::Memory>| {
                let mut req = Request::default();
                // A cursor-only commit leaves the primary plane as it is,
                // so the kernel sees no damage to it.
                match swapped {
                    Swapped::Frame => {
                        o.prepare(&mut req, o.back_fb(), o.mode_blob, clips, hotspot)?;
                    }
                    _ => o.prepare_cursor(&mut req, hotspot)?,
                }
                card.commit(&req, DRM_MODE_PAGE_FLIP_EVENT | DRM_MODE_ATOMIC_NONBLOCK)
            };
            let mut sent = commit(o);
            // A commit showing the cursor that failed is tried without it.
            // When that takes, the cursor plane is what failed: it is given
            // up, and the owner draws the pointer itself.
            let mut nothing = false;
            if let Err(e) = &sent
                && errno(e) != Some(Errno::BUSY)
                && o.cursor_shown().is_some()
            {
                if let Some(c) = o.cursor.as_mut() {
                    c.visible = false;
                }
                // with the plane off already, a cursor-only commit has
                // nothing left to send
                nothing = swapped == Swapped::Cursor && o.cursor_set.is_none();
                let again = if nothing { Ok(()) } else { commit(o) };
                if again.is_ok() {
                    log::warn!(
                        "the cursor plane of {} failed a commit, so the pointer is drawn in software: {e}",
                        o.name
                    );
                    drop_cursor(card, o);
                    sent = Ok(());
                } else {
                    nothing = false;
                    if let Some(c) = o.cursor.as_mut() {
                        c.visible = true;
                    }
                }
            }
            if let Some(blob) = clips
                && let Err(e) = card.destroy_blob(blob)
            {
                log::warn!("cannot destroy the damage blob: {e}");
            }
            if nothing {
                return Ok(Swapped::Nothing);
            }
            sent
        };
        if let Err(e) = sent {
            if errno(&e) == Some(Errno::BUSY) {
                // Another commit holds the CRTC while vtium knows of no flip
                // of its own in flight: a commit vtium does not track, or a
                // flip of vtium's that the bound took as done while the
                // kernel still had it. When the first lands, the kernel sends
                // vtium no event. The display's next vblank stands in for
                // that event: it is waited for as a flip in flight is, and
                // the owner draws again after it.
                match self.card.wait_vblank(o.pipe) {
                    Ok(()) => {
                        o.start_flip();
                        o.vblank = true;
                    }
                    // A card with no vblank interrupt and no software vblank
                    // timer refuses the wait with EOPNOTSUPP, and nothing
                    // then asks for the frame again
                    // until the next draw (vtium/vtium#176).
                    Err(v) => log::debug!("cannot wait for the next vblank of {}: {v}", o.name),
                }
                self.settle();
            } else {
                log::warn!("cannot flip {}: {e}", o.name);
            }
            return Err(e.into());
        }
        o.start_flip();
        if swapped == Swapped::Frame {
            o.back = o.after_back();
            o.damage.clear();
            o.fresh = false;
        }
        o.cursor_set = o.cursor_shown();
        self.settle();
        Ok(swapped)
    }

    /// Sets a display's power state. A display whose connector has no
    /// DPMS property keeps its state, and still counts as powered down for
    /// the blank. While a display is not on it lends no token; powered
    /// on, every buffer is age 0 and [`Event::Redraw`] follows.
    pub fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        let awake = self.awake;
        let o = self
            .outputs
            .iter_mut()
            .find(|o| o.id == id && o.announced)
            .filter(|o| awake && o.online)
            .ok_or_else(|| invalid("the display is not online"))?;
        let on = dpms == Dpms::On;
        if let Some((prop, _)) = o.dpms.filter(|&(_, now)| now != dpms) {
            log::info!("setting DPMS of {} to {dpms:?}", o.name);
            self.card
                .set_connector_property(o.id.0, prop, dpms_value(dpms))?;
            o.dpms = Some((prop, dpms));
            if on {
                o.powered_on();
                o.redraw = false;
                self.events.push(Event::Redraw(id));
            }
        }
        // A frame drawn while the display powers down is not shown.
        if !on && !o.down {
            o.lease = o.lease.wrapping_add(1);
        }
        o.down = !on;
        self.blanked = self.outputs.iter().filter(|o| o.online).all(|o| o.down);
        Ok(())
    }

    /// Powers every display on after the blank, on the input that ends
    /// it: through the modeset a hotplug while blanked left owed, with
    /// every display on, or else through each display's DPMS. Where the
    /// modeset or any display's DPMS fails, a modeset is owed and tried
    /// again as a failed hotplug is, setting every display on: the blank
    /// stays ended, since no later input comes to `unblank`. Asleep, it
    /// does nothing, as the wake-up brings every display on.
    pub fn unblank(&mut self) -> io::Result<()> {
        self.blanked = false;
        // Asleep, or letting the displays go for a VT release: nothing to
        // power on, and no retry may outlive the release.
        if !self.awake || !self.foreground {
            return Ok(());
        }
        let done = if std::mem::take(&mut self.owed) {
            self.scan(Scan::Retry)
        } else {
            // Every display is tried, whichever refuses.
            let ids: Vec<DisplayId> = self.displays().collect();
            let mut done = Ok(());
            for id in ids {
                let on = self.set_dpms(id, Dpms::On);
                done = done.and(on);
            }
            done
        };
        if let Err(e) = &done {
            log::warn!("cannot power the displays on: {e}; trying a modeset");
            self.owed = true;
            self.retry(false);
        }
        // The modeset's flip and the retries are bounded.
        self.settle();
        done
    }

    /// The user pressed a key or acted with the pointer (a move, a button
    /// or the wheel). After [`Event::WakeFailed`], it tries again, at most [`crate::WAKE_RETRIES`] times more: the wake-up,
    /// with every display on, where the card is asleep in the foreground,
    /// or else a modeset of every display, owed to [`Drm2d::unblank`]
    /// while blanked. Otherwise, and while the retries run, it does
    /// nothing.
    pub fn input(&mut self) -> io::Result<()> {
        if !self.foreground || self.retries.is_some() || !std::mem::take(&mut self.wake_failed) {
            return Ok(());
        }
        if !self.awake {
            return self.poll();
        }
        let set = self.scan(Scan::Retry);
        if set.is_err() {
            self.retry(false);
        }
        self.settle();
        set
    }

    /// Puts `pixels`, ARGB8888 rows of `width`, on the display's cursor
    /// plane, with its hotspot at `hot`; it stays hidden until
    /// [`Drm2d::show_cursor`]. A display that is not online refuses it
    /// with `NotConnected`, which no error from the card decodes to; one
    /// without a cursor plane, or whose plane, at most [`CURSOR_MAX`] a
    /// side, is smaller than the image, with `Unsupported`, so the owner
    /// draws the image whole rather than cut.
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
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "the display is not online")
            })?;
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
        if width > cw || height > ch {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("a {width}x{height} image on a {cw}x{ch} cursor plane"),
            ));
        }
        self.destroy_cursor(id);
        let dumb = self.card.create_dumb(cw, ch)?;
        let made = self
            .card
            .add_fb(dumb.handle, dumb.pitch, cw, ch, ARGB8888)
            .and_then(|fb| {
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
                    for (px, out) in src.iter().take(cols).zip(row.as_chunks_mut::<4>().0) {
                        *out = px.to_ne_bytes();
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

    /// Takes the cursor plane's image away ([`drop_cursor`]).
    pub fn destroy_cursor(&mut self, id: DisplayId) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) {
            drop_cursor(&self.card, o);
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
        c.at = (x, y);
        c.visible = true;
        Ok(())
    }

    /// Hides the cursor from the next swap.
    pub fn hide_cursor(&mut self, id: DisplayId) {
        if let Some(o) = self.output_mut(id)
            && let Some(c) = o.cursor.as_mut()
        {
            c.visible = false;
        }
    }

    /// Moves where the cursor's coordinates start, as for a grid placed
    /// away from the display's corner.
    pub fn set_cursor_offset(&mut self, id: DisplayId, x: i32, y: i32) {
        if let Some(o) = self.output_mut(id)
            && let Some(c) = o.cursor.as_mut()
        {
            c.offset = (x, y);
        }
    }

    /// Takes DRM master and sets every display up again. While another
    /// process holds master, or the setup fails, the wake-up is tried
    /// again every [`crate::WAKE_RETRY_MS`], at most
    /// [`crate::WAKE_RETRIES`] times.
    pub fn wake_up(&mut self) -> io::Result<()> {
        self.reap();
        self.foreground = true;
        self.retries = None;
        let woken = self.try_wake(Scan::WakeUp);
        if woken.is_err() {
            self.retry(true);
        }
        self.settle();
        woken
    }

    /// Starts the retries, unless they run: a wake-up's when `wake`.
    fn retry(&mut self, wake: bool) {
        self.waking = wake || (self.waking && self.retries.is_some());
        self.retries.get_or_insert(0);
    }

    /// Takes DRM master and sets every display up again: as a wake-up,
    /// which reports [`Event::Awake`], or as a retry, which does not. A
    /// failed wake-up lets master go, as another process may hold the
    /// displays; a failed retry keeps master and the displays online, as
    /// the atomic modeset left them, so the blank still powers them down.
    fn try_wake(&mut self, scan: Scan) -> io::Result<()> {
        if !self.master {
            self.card.set_master().inspect_err(|e| {
                log::warn!("cannot take DRM master: {e}");
            })?;
            self.master = true;
        }
        self.awake = true;
        // Another program held the displays: what the buffers hold counts
        // as unknown, and each display is drawn whole, so a token lent
        // before comes back too late. A retry runs under the master the
        // card kept, so no other program drew into the buffers.
        if scan == Scan::WakeUp {
            for o in &mut self.outputs {
                o.ages = Ages::default();
                o.lease = o.lease.wrapping_add(1);
                o.clear_owed = false;
                o.redraw = true;
            }
        }
        if let Err(e) = self.scan(scan) {
            if scan == Scan::WakeUp {
                self.release_master();
                self.awake = false;
            }
            return Err(e);
        }
        if scan == Scan::WakeUp {
            self.wake_failed = false;
            // The card is awake, so a retry from here on, of a mode owed,
            // is a modeset's.
            self.waking = false;
            self.events.push(Event::Awake);
        }
        Ok(())
    }

    /// A wake-up retry's timer fired. While blanked, a failed hotplug's or
    /// unblank's retry commits nothing: it leaves the modeset owed to the
    /// input that ends the blank.
    pub(crate) fn retry_wake(&mut self) {
        let Some(tried) = self.retries else {
            return;
        };
        // A retry never runs past a VT release.
        if !self.foreground {
            self.retries = None;
            return;
        }
        let scan = if self.waking {
            Scan::WakeUp
        } else {
            Scan::Retry
        };
        if self.blanked && scan == Scan::Retry {
            log::debug!("blanked: the retried modeset waits for input");
            self.owed = true;
            self.retries = None;
            self.settle();
            return;
        }
        // A legacy display left unset counts as not woken.
        let woken = self.try_wake(scan).is_ok() && !self.unset_any();
        if woken && !self.owing() {
            self.retries = None;
        } else if tried.saturating_add(1) >= crate::WAKE_RETRIES {
            self.retries = None;
            if woken {
                // The card is awake and every display is set, but some at
                // the mode they were set to rather than the mode owed.
                for o in &mut self.outputs {
                    o.hold_back_owed();
                }
            } else {
                log::error!("cannot wake the display after {tried} retries");
                self.wake_failed = true;
                self.events.push(Event::WakeFailed);
            }
        } else {
            self.retries = Some(tried.saturating_add(1));
        }
        self.settle();
    }

    /// Lets the displays go: blanks them, waits for the flips in flight
    /// (at most [`crate::FLIP_WAIT_MS`]), hides the cursor and overlay
    /// planes and drops DRM master. [`Event::Asleep`] tells when it is
    /// done. A card that never woke drops the master its open gave it, so
    /// the vtium of the VT shown can take it (vtium/vtium#139).
    pub fn sleep(&mut self) {
        self.reap();
        self.foreground = false;
        self.owed = false;
        self.retries = None;
        self.wake_failed = false;
        if !self.awake {
            self.release_master();
            self.events.push(Event::Asleep);
            self.settle();
            return;
        }
        for o in self.outputs.iter_mut().filter(|o| o.online) {
            // A frame drawn while the VT is let go is not shown.
            o.lease = o.lease.wrapping_add(1);
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
            self.clear_planes(&mut req, &[]);
            if !req.entries.is_empty()
                && let Err(e) = self.card.commit(&req, DRM_MODE_ATOMIC_NONBLOCK)
            {
                log::warn!("cannot hide the cursor and overlay planes: {e}");
            }
        }
        self.release_master();
        self.awake = false;
        self.retries = None;
        self.owed = false;
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
    /// and sets up what changed. A failure is retried as a wake-up is,
    /// though a retry while blanked leaves its modeset owed to input. A
    /// wake-up that failed and ran out of retries is tried again.
    pub fn poll(&mut self) -> io::Result<()> {
        self.reap();
        if !self.awake {
            if !self.foreground || self.retries.is_some() {
                return Ok(());
            }
            return self.wake_up();
        }
        let scanned = self.scan(Scan::Hotplug);
        if scanned.is_err() && self.awake {
            self.retry(false);
        }
        self.settle();
        scanned
    }

    /// Reads the card's events: the page flips completed and the vblanks
    /// passed.
    pub fn dispatch(&mut self) -> io::Result<()> {
        let mut events = Vec::new();
        let read = self.card.read_events(&mut events);
        // The card delivers events again, so the next one lost warns.
        if !events.is_empty() {
            self.lost_flip = false;
        }
        for e in events {
            match e {
                CrtcEvent::Flipped(crtc) => self.flipped(crtc, true),
                CrtcEvent::Vblank(crtc) => self.flipped(crtc, false),
            }
        }
        self.resolve();
        self.settle();
        read
    }

    /// A flip's event arrived, with `frame`, or the vblank a refused
    /// commit waits for passed. A flip's event that comes after its flip
    /// was taken as done still tells the owner it may draw, as a swap tried
    /// meanwhile was busy. A vblank ends only the wait for it, and shows
    /// no frame, so the owner is told nothing; it draws again at the turn
    /// the vblank's event wakes.
    fn flipped(&mut self, crtc: u32, frame: bool) {
        let ready = self.awake && self.waiting.is_none();
        match self.outputs.iter_mut().find(|o| o.crtc == crtc) {
            Some(o) => {
                if std::mem::take(&mut o.vblank) || frame {
                    o.flipping = false;
                }
                // The card scans the new mode out, which a capture of the
                // card's state at this line shows.
                if frame && std::mem::take(&mut o.modeset_flip) {
                    log::debug!(
                        "display {}: the first flip after its modeset is done",
                        o.name
                    );
                }
                if frame && o.announced && ready {
                    self.events.push(Event::Flipped(o.id));
                }
            }
            None => log::debug!("an event for CRTC {crtc}, which drives no display"),
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
        let (frame, event) = if o.vblank {
            (false, "vblank")
        } else {
            (true, "page flip")
        };
        // A flip taken as done may not be on screen, so it ends a modeset
        // without the line that says the mode is shown.
        if frame {
            o.modeset_flip = false;
        }
        if std::mem::replace(&mut self.lost_flip, true) {
            log::debug!("no {event} event for {}; taking the flip as done", o.name);
        } else {
            log::warn!(
                "no {event} event for {} within {} ms; taking the flip as done",
                o.name,
                crate::FLIP_WAIT_MS
            );
        }
        self.flipped(crtc, frame);
        self.resolve();
        self.settle();
    }

    /// Runs the operation that waits, once no flip is in flight.
    fn resolve(&mut self) {
        if self.outputs.iter().any(|o| o.flipping) {
            return;
        }
        match self.waiting.take() {
            Some(Waiting::Modeset { wake: false }) if self.blanked => {
                // The blank came while the modeset waited.
                log::debug!("blanked: the modeset waits for input");
                self.owed = true;
            }
            Some(Waiting::Modeset { wake }) => {
                if let Err(e) = self.modeset() {
                    log::debug!("the modeset failed: {e}");
                    self.retry(wake);
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

    /// The scan: binds a display to each connected connector with modes,
    /// lets go of those gone, and sets modes as `scan` asks.
    fn scan(&mut self, scan: Scan) -> io::Result<()> {
        if !self.awake {
            return Ok(());
        }
        // The card takes master as it wakes and lets it go only as it
        // falls asleep, so a scan always runs under master.
        debug_assert!(self.master, "an awake card holds DRM master");
        let res = self.card.resources()?;
        for o in &mut self.outputs {
            o.available = false;
        }
        for d in &mut self.dark {
            d.available = false;
        }
        let mut needs = scan != Scan::Hotplug;
        // The set of displays changed: one was bound or let go.
        let mut changed = false;
        // A wake-up after the first scan places each display bound before
        // on the CRTC its connector is on now, from the connected
        // connectors it reads. The first scan binds every display afresh.
        let follow = scan == Scan::WakeUp && self.scanned;
        let mut seen = Vec::new();
        for &id in &res.connectors {
            let Some(conn) = self.connector(id) else {
                continue;
            };
            if !conn.connected || conn.modes.is_empty() {
                continue;
            }
            if follow {
                seen.push(conn.clone());
            }
            // A connector that shows another monitor than the one its
            // display or dark entry was bound to, as when a monitor is
            // swapped faster than its disconnect is seen, is let go and
            // bound again below, as the new monitor, so the head file's
            // rules, the dark rule, its density and its mode follow it.
            let swapped = self
                .outputs
                .iter()
                .find(|o| o.id.0 == id)
                .is_some_and(|o| self.another_monitor(&o.name, self.known_edid(o.id)));
            if swapped {
                let (gone, kept) = std::mem::take(&mut self.outputs)
                    .into_iter()
                    .partition(|o| o.id.0 == id);
                self.outputs = kept;
                for o in gone {
                    log::info!("display {} shows another monitor", o.name);
                    if o.announced {
                        self.events.push(Event::Removed(o.id));
                    }
                    self.identities.retain(|(i, _)| *i != o.id);
                    self.free(o);
                    changed = true;
                }
            } else if self.dark.iter().find(|d| d.id == id).is_some_and(|d| {
                self.another_monitor(&d.identity.connector, d.identity.edid.as_ref())
            }) {
                self.dark.retain(|d| {
                    if d.id == id {
                        log::info!(
                            "display {}, left dark, shows another monitor",
                            d.identity.connector
                        );
                    }
                    d.id != id
                });
            }
            if let Some(o) = self.outputs.iter_mut().find(|o| o.id.0 == id) {
                o.available = true;
                needs |= !o.online;
                // The same monitor listing other modes than those the
                // display's mode was chosen from, as after a DisplayPort
                // link is trained again, gets its mode chosen again from
                // the new list. Under `use-original-mode` the mode vtium
                // set stands in for the mode the console left, which is
                // gone by now, so the display keeps its mode where the new
                // list has it.
                if conn.modes != o.modes {
                    let wanted = self.policy.wanted.as_ref().and_then(|f| f(&o.name));
                    let use_original = self.policy.use_original;
                    if let Some(choice) =
                        mode::choose(&conn.modes, Some(&o.mode), wanted, use_original)
                    {
                        if choice.current != o.mode {
                            let (w, h) = choice.current.size();
                            log::info!("display {} lists new modes: setting {w}x{h}", o.name);
                            o.mode = choice.current;
                            needs = true;
                        }
                        o.choice = choice;
                        o.held = None;
                        o.owed = None;
                    }
                    o.modes = conn.modes;
                }
                continue;
            }
            if let Some(d) = self.dark.iter_mut().find(|d| d.id == id) {
                d.available = true;
                continue;
            }
            // A connector whose properties cannot be read is left unbound
            // and read again at the next scan: its `PATH` belongs to its
            // identity, and an atomic commit needs its `CRTC_ID`.
            let found = match self.find(&conn) {
                Ok(found) => found,
                Err(e) => {
                    log::warn!(
                        "cannot read the properties of connector {}: {e}",
                        connector_name(conn.kind, conn.kind_id)
                    );
                    continue;
                }
            };
            let identity = self.identify(&found);
            if self
                .dark_policy
                .as_ref()
                .is_some_and(|dark| dark(&identity))
            {
                log::info!("display {} is left dark", found.name);
                self.dark.push(Dark {
                    id,
                    identity,
                    available: true,
                });
                needs = true;
                continue;
            }
            if let Some(o) = self.bind(&res, &conn, found) {
                self.identities.retain(|(i, _)| *i != o.id);
                self.identities.push((o.id, identity));
                self.outputs.push(o);
                needs = true;
                changed = true;
            }
        }
        self.scanned = true;
        self.dark.retain(|d| {
            if !d.available {
                log::info!("display {}, left dark, is gone", d.identity.connector);
            }
            d.available
        });
        self.dark
            .sort_by_key(|d| res.connectors.iter().position(|&c| c == d.id));
        let (mut kept, gone): (Vec<_>, Vec<_>) = self.outputs.drain(..).partition(|o| o.available);
        kept.sort_by_key(|o| res.connectors.iter().position(|&c| c == o.id.0));
        self.outputs = kept;
        for o in gone {
            log::info!("display {} is gone", o.name);
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
            self.free(o);
            changed = true;
        }
        if follow {
            self.follow_crtcs(&res, &seen);
        }
        // A mode held back is tried again as the card wakes, as another
        // program may have changed what the card can set, and when the set
        // of displays changes, which frees or takes the card's bandwidth.
        if scan == Scan::WakeUp || changed {
            for o in &mut self.outputs {
                if let Some(held) = o.held.take() {
                    o.mode = held;
                    needs = true;
                }
            }
        }
        // A mode owed is tried again by the retries and as the card wakes.
        // It stays owed until a modeset sets it, so a modeset that waits
        // for a flip keeps the retries running.
        if scan != Scan::Hotplug {
            for o in &mut self.outputs {
                if let Some(owed) = o.owed {
                    o.mode = owed;
                }
            }
        }
        let outputs = &self.outputs;
        self.identities
            .retain(|(id, _)| outputs.iter().any(|o| o.id == *id));
        // With no display the card keeps master while its VT is shown, so
        // the kernel's console, which lights a display at a hotplug while
        // no process holds master, never takes one plugged in meanwhile.
        if self.outputs.is_empty() {
            if needs && !self.dark.is_empty() {
                self.darken();
            }
            return Ok(());
        }
        if needs && self.blanked && scan != Scan::WakeUp {
            // Blanked, a hotplug commits nothing: the modeset waits for the
            // input that ends the blank, and a display plugged in stays
            // dark until then (the operator's decision on vtium/vtium#18).
            log::debug!("blanked: the hotplug's modeset waits for input");
            self.owed = true;
            Ok(())
        } else if needs {
            self.request_modeset(scan == Scan::WakeUp)
        } else {
            self.announce();
            Ok(())
        }
    }

    /// The monitor display `id` was bound to, as its EDID named it.
    fn known_edid(&self, id: DisplayId) -> Option<&EdidId> {
        self.identities
            .iter()
            .find(|(i, _)| *i == id)
            .and_then(|(_, identity)| identity.edid.as_ref())
    }

    /// Whether connector `name`'s EDID in sysfs now names another monitor
    /// than `known`. A connector whose EDID cannot be read, as for a moment
    /// while a monitor is plugged in, counts as showing the same one.
    fn another_monitor(&self, name: &str, known: Option<&EdidId>) -> bool {
        self.card
            .sysfs()
            .and_then(|dir| Edid::sysfs(&dir, name))
            .is_some_and(|now| known != Some(&now.id))
    }

    /// Connector `id`, or `None` where the card cannot give it. One gone
    /// since the card listed it, as during a monitor's power cycle, is
    /// logged at debug level; any other failure, a read refused because
    /// the connector changed between its two reads among them, is a
    /// warning.
    fn connector(&self, id: u32) -> Option<Connector> {
        self.read_logged(id, self.card.connector(id))
    }

    /// What a read of connector `id` gave, or `None`, logged as
    /// [`Self::connector`] logs it.
    pub(super) fn read_logged<T>(&self, id: u32, read: io::Result<T>) -> Option<T> {
        read.inspect_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                log::debug!("connector {id} is gone: {e}");
            } else {
                log::warn!("cannot read connector {id}: {e}");
            }
        })
        .ok()
    }

    /// What a scan finds of connector `conn`, or the failure to read its
    /// properties.
    fn find(&self, conn: &Connector) -> io::Result<Found> {
        let name = connector_name(conn.kind, conn.kind_id);
        let current = (conn.encoder != 0)
            .then(|| self.encoder(conn.encoder))
            .flatten();
        let shown = current.filter(|e| e.crtc != 0).and_then(|e| {
            self.card
                .crtc_mode(e.crtc)
                .inspect_err(|err| {
                    log::warn!("cannot read the mode of CRTC {}: {err}", e.crtc);
                })
                .ok()
                .flatten()
        });
        let props = self.card.properties(conn.id, Object::Connector)?;
        let on = match current.map(|e| e.crtc) {
            _ if self.legacy => value(&props, "DPMS").is_none_or(|d| d == 0),
            Some(crtc) => self
                .card
                .properties(crtc, Object::Crtc)
                .inspect_err(|e| log::warn!("cannot read the properties of CRTC {crtc}: {e}"))
                .ok()
                .and_then(|p| value(&p, "ACTIVE"))
                .is_none_or(|a| a != 0),
            None => false,
        };
        let edid = self.card.sysfs().and_then(|dir| Edid::sysfs(&dir, &name));
        let path = self.path(&name, &props);
        Ok(Found {
            name,
            current,
            visible: on && shown.is_some(),
            shown,
            props,
            edid,
            path,
        })
    }

    /// The text of connector `name`'s `PATH` property, read from the blob
    /// its value names in `props`; `None` where it has no `PATH`, its
    /// value names no blob (0), or the card cannot give the blob.
    fn path(&self, name: &str, props: &[Property]) -> Option<String> {
        let blob = value(props, "PATH")
            .filter(|&b| b != 0)
            .and_then(|b| u32::try_from(b).ok())?;
        let bytes = self
            .card
            .blob(blob)
            .inspect_err(|e| log::warn!("cannot read the PATH of connector {name}: {e}"))
            .ok()?;
        path::text(&bytes)
    }

    /// The identity of the connector `found` describes.
    fn identify(&self, found: &Found) -> Identity {
        Identity {
            connector: found.name.clone(),
            card: self.card_path.clone(),
            edid: found.edid.as_ref().map(|e| e.id.clone()),
            connector_path: found.path.clone(),
            lit_at_start: !self.scanned && found.visible,
        }
    }

    /// A display for a connected connector, or `None` when it has no free
    /// CRTC or no primary plane.
    fn bind(&self, res: &Resources, conn: &Connector, found: Found) -> Option<Output<S::Memory>> {
        let Found {
            name,
            current,
            shown,
            props: connector_props,
            edid,
            ..
        } = found;
        let wanted = self.policy.wanted.as_ref().and_then(|f| f(&name));
        let choice = mode::choose(
            &conn.modes,
            shown.as_ref(),
            wanted,
            self.policy.use_original,
        )?;
        // Its modeset turns the display on, whatever DPMS the connector was
        // left at.
        let dpms = connector_props
            .iter()
            .find(|p| p.name == "DPMS")
            .map(|p| (p.id, Dpms::On));
        let taken: Vec<u32> = self.outputs.iter().map(|o| o.crtc).collect();
        let Some((crtc, crtc_index)) = self.find_crtc(res, conn, current, &taken) else {
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
        let mm = physical_size(&name, conn.mm, edid.as_ref());
        Some(Output {
            id: DisplayId(conn.id),
            name,
            crtc,
            pipe: crtc_index,
            planes,
            mode: choice.current,
            set: None,
            held: None,
            owed: None,
            mm,
            choice,
            modes: conn.modes.clone(),
            mode_blob: 0,
            buffers: Vec::new(),
            back: 0,
            ages: Ages::default(),
            damage: Damage::default(),
            loan: None,
            lease: 0,
            clear_owed: false,
            fresh: false,
            cursor: None,
            cursor_set: None,
            dpms,
            down: false,
            online: false,
            redraw: false,
            flipping: false,
            vblank: false,
            flips: 0,
            flip_timer: None,
            modeset_flip: false,
            available: true,
            announced: false,
            returning: self.scanned,
        })
    }

    /// An encoder, or `None`, logged, when it cannot be read.
    fn encoder(&self, id: u32) -> Option<Encoder> {
        self.card
            .encoder(id)
            .inspect_err(|e| log::warn!("cannot read encoder {id}: {e}"))
            .ok()
    }

    /// The CRTC the connector's encoder, `current` as read with the
    /// connector, drives, if it is not in `taken`, else the first CRTC not
    /// in `taken` that an encoder of the connector can drive.
    fn find_crtc(
        &self,
        res: &Resources,
        conn: &Connector,
        current: Option<Encoder>,
        taken: &[u32],
    ) -> Option<(u32, usize)> {
        let free = |crtc: u32| !taken.contains(&crtc);
        let index = |crtc: u32| res.crtcs.iter().position(|&c| c == crtc);
        if let Some(enc) = current
            && enc.crtc != 0
            && free(enc.crtc)
        {
            return index(enc.crtc).map(|i| (enc.crtc, i));
        }
        conn.encoders.iter().find_map(|&e| {
            let enc = if e == conn.encoder {
                current?
            } else {
                self.encoder(e)?
            };
            res.crtcs.iter().enumerate().find_map(|(i, &crtc)| {
                let possible = u32::try_from(i)
                    .ok()
                    .and_then(|i| 1u32.checked_shl(i))
                    .is_some_and(|bit| enc.possible_crtcs & bit != 0);
                (possible && free(crtc)).then_some((crtc, i))
            })
        })
    }

    /// Places each display a wake-up finds bound from before on a CRTC,
    /// from the connectors `seen` as the wake-up read them. A display moves
    /// to a new CRTC with that CRTC's primary and cursor planes.
    ///
    /// The program that held the card while vtium slept may have moved a
    /// connector to another CRTC. A modeset on the CRTC the display had
    /// would take the connector off a CRTC that program left lit, and the
    /// kernel refuses a lit CRTC with no connector whatever mode the commit
    /// sets (`drm_atomic_helper_check_modeset`), so the display would never
    /// wake again (vtium/vtium#249).
    ///
    /// Each display wants one CRTC: the CRTC its connector is on, or, where
    /// its connector is on no CRTC or the display returns, its own CRTC. A
    /// display that returns has its first modeset turn off the CRTC its
    /// connector is on ([`Self::set_off_first`]). The displays whose
    /// connector is on a CRTC are placed first, then those whose connector
    /// is on none, then those that return, each group in connector order.
    /// A display takes the CRTC it wants where no display took it, and else
    /// the first free CRTC an encoder of its connector can drive. Where no
    /// CRTC is free, a matching step ([`augment`]) moves displays placed
    /// before it to other CRTCs they can drive to free one, moving a display
    /// whose connector is on a CRTC only where nothing else frees one.
    ///
    /// The CRTCs the displays held before the wake-up are a placement of
    /// every display, so the matching always finds one. A display it could
    /// not place would stay off, as one the scan cannot bind does, and the
    /// owner would hear it removed; so does a display whose new CRTC has
    /// no primary plane.
    fn follow_crtcs(&mut self, res: &Resources, seen: &[Connector]) {
        let bit = |crtc: u32| {
            res.crtcs
                .iter()
                .position(|&c| c == crtc)
                .map_or(0, index_bit)
        };
        let index = |crtc: u32| res.crtcs.iter().position(|&c| c == crtc);
        // Each display, its group, the CRTC it wants, and the CRTCs it can
        // take: those its connector's encoders can drive, its own and the
        // one its connector is on.
        let mut places: Vec<_> = std::mem::take(&mut self.outputs)
            .into_iter()
            .map(|o| {
                let conn = seen.iter().find(|c| c.id == o.id.0);
                let current = conn
                    .filter(|c| c.encoder != 0)
                    .and_then(|c| self.encoder(c.encoder));
                let on = current.map_or(0, |e| e.crtc);
                let encoders = conn.map_or(0, |c| {
                    c.encoders
                        .iter()
                        .filter_map(|&e| {
                            if e == c.encoder {
                                current
                            } else {
                                self.encoder(e)
                            }
                        })
                        .fold(0, |all, e| all | e.possible_crtcs)
                });
                let able = encoders | bit(o.crtc) | bit(on);
                let (group, want) = match on {
                    _ if o.returning => (2u8, o.crtc),
                    0 => (1, o.crtc),
                    on => (0, on),
                };
                (o, group, index(want), able)
            })
            .collect();
        places.sort_by_key(|&(_, group, _, _)| group);
        let able: Vec<u32> = places.iter().map(|p| p.3).collect();
        let lit: Vec<bool> = places.iter().map(|p| p.1 == 0).collect();
        let mut slot = vec![None; places.len()];
        let mut owner = vec![None; res.crtcs.len()];
        for (d, &(_, _, want, can)) in places.iter().enumerate() {
            let free = |c: usize| owner.get(c) == Some(&None) && can & index_bit(c) != 0;
            match want
                .filter(|&c| free(c))
                .or_else(|| (0..owner.len()).find(|&c| free(c)))
            {
                Some(c) => assign(d, c, &mut slot, &mut owner),
                None => {
                    let _ = augment(d, &able, &lit, &mut slot, &mut owner, &mut 0)
                        || augment(d, &able, &[], &mut slot, &mut owner, &mut 0);
                }
            }
        }
        for ((mut o, ..), at) in places.into_iter().zip(slot) {
            let placed = match at.and_then(|c| Some((*res.crtcs.get(c)?, c))) {
                None => Err(format!("no free CRTC for {}", o.name)),
                Some((crtc, _)) if crtc == o.crtc => Ok(()),
                Some((crtc, pipe)) => self.move_to(&mut o, crtc, pipe),
            };
            match placed {
                Ok(()) => self.outputs.push(o),
                Err(why) => {
                    log::warn!("{why}; it stays off until a scan finds a CRTC for it");
                    if o.announced {
                        self.events.push(Event::Removed(o.id));
                    }
                    self.identities.retain(|(i, _)| *i != o.id);
                    self.free(o);
                }
            }
        }
        self.outputs
            .sort_by_key(|o| res.connectors.iter().position(|&c| c == o.id.0));
    }

    /// Moves display `o` to `crtc`, at `pipe` in the card's list of CRTCs,
    /// with that CRTC's planes, or says why it cannot. Where the new CRTC
    /// has no cursor plane, the display gives its pointer image up, and the
    /// owner draws the pointer itself.
    fn move_to(&self, o: &mut Output<S::Memory>, crtc: u32, pipe: usize) -> Result<(), String> {
        if !self.legacy {
            let mut planes = self
                .find_planes(crtc, pipe)
                .ok_or_else(|| format!("no primary plane for {} on CRTC {crtc}", o.name))?;
            planes.connector_props = o
                .planes
                .take()
                .map(|p| p.connector_props)
                .unwrap_or_default();
            if planes.cursor.is_none() {
                drop_cursor(&self.card, o);
            }
            o.planes = Some(planes);
        }
        log::info!(
            "display {} moves from CRTC {} to CRTC {crtc}",
            o.name,
            o.crtc
        );
        o.crtc = crtc;
        o.pipe = pipe;
        // The new cursor plane shows nothing of vtium's yet.
        o.cursor_set = None;
        Ok(())
    }

    /// The primary and cursor planes for the CRTC at `index`, with their
    /// properties and the CRTC's.
    fn find_planes(&self, crtc: u32, index: usize) -> Option<Planes> {
        let bit = u32::try_from(index)
            .ok()
            .and_then(|i| 1u32.checked_shl(i))?;
        let mut planes = Planes::default();
        let mut primary = None;
        let all = self
            .card
            .planes()
            .inspect_err(|e| log::warn!("cannot list the card's planes: {e}"))
            .ok()?;
        for plane in all {
            match self.card.plane_crtcs(plane) {
                Ok(crtcs) if crtcs & bit != 0 => {}
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("cannot read the CRTCs of plane {plane}: {e}");
                    continue;
                }
            }
            let props = match self.card.properties(plane, Object::Plane) {
                Ok(props) => props,
                Err(e) => {
                    log::warn!("cannot read the properties of plane {plane}: {e}");
                    continue;
                }
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
        let crtc_props = self
            .card
            .properties(crtc, Object::Crtc)
            .inspect_err(|e| log::warn!("cannot read the properties of CRTC {crtc}: {e}"))
            .ok()?;
        planes.crtc_props = Props::new(&crtc_props);
        Some(planes)
    }

    /// Hides every cursor and overlay plane, which another program may
    /// have left in the middle of the screen, and every plane on a CRTC in
    /// `off`, which the commit turns off.
    fn clear_planes(&self, req: &mut Request, off: &[u32]) {
        self.hide_planes(req, off, true);
    }

    /// Hides every plane on a CRTC in `off`, and every cursor and overlay
    /// plane too when `others`.
    fn hide_planes(&self, req: &mut Request, off: &[u32], others: bool) {
        let all = self.card.planes().unwrap_or_else(|e| {
            log::warn!("cannot list the card's planes: {e}");
            Vec::new()
        });
        for plane in all {
            let props = match self.card.properties(plane, Object::Plane) {
                Ok(props) => props,
                Err(e) => {
                    log::warn!("cannot read the properties of plane {plane}: {e}");
                    continue;
                }
            };
            let on_off = value(&props, "CRTC_ID")
                .and_then(|c| u32::try_from(c).ok())
                .is_some_and(|c| off.contains(&c));
            let other = matches!(value(&props, "type"), Some(PLANE_CURSOR | PLANE_OVERLAY));
            if on_off || others && other {
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

    /// A modeset now, or once no flip is in flight: a wake-up's when
    /// `wake`.
    fn request_modeset(&mut self, wake: bool) -> io::Result<()> {
        let wake = match self.waiting {
            Some(Waiting::Sleep) => return Ok(()),
            Some(Waiting::Modeset { wake: waiting }) => wake || waiting,
            None => wake,
        };
        if self.outputs.iter().any(|o| o.flipping) {
            self.waiting = Some(Waiting::Modeset { wake });
            return Ok(());
        }
        self.modeset()
    }

    /// Sets every display's mode, then announces each display set. Where
    /// the displays online and the new ones cannot be set together, the
    /// new ones are left off until the next hotplug and the rest are set
    /// alone. A display whose new mode cannot be set stays at the mode it
    /// is set to: held back where the card refused the mode, and else owed
    /// it, which starts the wake-up retries.
    fn modeset(&mut self) -> io::Result<()> {
        // The legacy calls set each display on its own, so the displays set
        // are announced whatever failed for another.
        let (set, unset) = if self.legacy {
            (Ok(()), self.legacy_modeset())
        } else {
            let mut set = self.atomic_modeset_or_default();
            // A display whose new mode the card refuses, as its default
            // too, stays at the mode it is set to, and the modeset is tried
            // again, so the others are set and nothing is retried.
            if refused(&set) && self.hold_back() {
                set = self.atomic_modeset_or_default();
            }
            // A display whose new mode failed for another reason, as where
            // its buffers cannot be made, stays at the mode it is set to,
            // which needs no new buffers, and the modeset is tried again,
            // so the card is set and stays awake while the retries try the
            // new mode.
            if set.is_err() && self.keep_set_modes() {
                set = self.atomic_modeset_or_default();
            }
            let set = match set.map_err(Failure::into_inner) {
                Err(e) if errno(&e) != Some(Errno::ACCESS) && self.mixed() => {
                    let (online, new) = std::mem::take(&mut self.outputs)
                        .into_iter()
                        .partition(|o| o.online);
                    self.outputs = online;
                    for o in new {
                        log::warn!(
                            "cannot set {} beside the displays online: {e}; it stays off until the next hotplug",
                            o.name
                        );
                        self.free(o);
                    }
                    self.atomic_modeset_or_default()
                        .map_err(Failure::into_inner)
                }
                set => set,
            };
            (set, Vec::new())
        };
        self.unset = unset;
        if set.is_ok() {
            // A modeset brings every display on.
            self.blanked = false;
            self.owed = false;
            self.wake_failed = false;
            for o in &mut self.outputs {
                o.down = false;
            }
            self.announce();
            if self.owing() || self.unset_any() {
                self.retry(false);
            }
        }
        set
    }

    /// Gives each display back the mode it is set to where a scan chose
    /// another whose modeset failed for another reason than the card
    /// refusing it, owing it that mode. Whether any display did.
    fn keep_set_modes(&mut self) -> bool {
        let mut any = false;
        for o in &mut self.outputs {
            any |= o.keep_set_mode();
        }
        any
    }

    /// Whether a display is owed a mode, which the retries try again.
    fn owing(&self) -> bool {
        self.outputs.iter().any(|o| o.owed.is_some())
    }

    /// Whether a display the last legacy modeset could not set is still
    /// bound.
    fn unset_any(&self) -> bool {
        self.outputs.iter().any(|o| self.unset.contains(&o.id))
    }

    /// Gives each display back the mode it is set to where a scan chose
    /// another that the card refused, holding that mode back. Whether any
    /// display did.
    fn hold_back(&mut self) -> bool {
        let mut any = false;
        for o in &mut self.outputs {
            any |= o.hold_back();
        }
        any
    }

    /// Whether some displays are online and some are new.
    fn mixed(&self) -> bool {
        self.outputs.iter().any(|o| o.online) && self.outputs.iter().any(|o| !o.online)
    }

    /// An atomic modeset of every display, tried again with each display's
    /// default mode where the test commit fails. Where it fails with the
    /// dark connectors set off, it is tried without them first, so a dark
    /// connector never costs the lit displays (ADR-1401 D9); the first
    /// such failure is logged as a warning, later ones at debug level.
    fn atomic_modeset_or_default(&mut self) -> Result<(), Failure> {
        let mut dark = !self.dark.is_empty();
        let mut set = self.atomic_modeset(dark);
        if dark && refused(&set) {
            let names: Vec<&str> = self
                .dark
                .iter()
                .map(|d| d.identity.connector.as_str())
                .collect();
            let line = format!(
                "cannot set {} off beside the lit displays; they stay as they are",
                names.join(", ")
            );
            if std::mem::replace(&mut self.dark_refused, true) {
                log::debug!("{line}");
            } else {
                log::warn!("{line}");
            }
            dark = false;
            set = self.atomic_modeset(false);
        }
        if !refused(&set) {
            return set;
        }
        log::warn!("retrying the modeset with each display's default mode");
        let modes: Vec<Mode> = self.outputs.iter().map(|o| o.mode).collect();
        for o in &mut self.outputs {
            o.mode = o.choice.default;
        }
        let retried = self.atomic_modeset(dark);
        if retried.is_err() {
            for (o, mode) in self.outputs.iter_mut().zip(modes) {
                o.mode = mode;
            }
        } else {
            // The card refused the modes chosen, so none is owed.
            for o in &mut self.outputs {
                o.owed = None;
            }
        }
        retried
    }

    /// Announces each display online: new ones as added, and those whose
    /// buffers hold nothing known to be redrawn. A display powered down is
    /// told to redraw when it is powered on, and one the last legacy
    /// modeset could not set once a modeset sets it.
    fn announce(&mut self) {
        for o in self.outputs.iter_mut().filter(|o| o.online) {
            if self.unset.contains(&o.id) {
                continue;
            }
            let redraw = std::mem::take(&mut o.redraw);
            if o.announced {
                if redraw && o.dpms_on() {
                    self.events.push(Event::Redraw(o.id));
                }
            } else {
                o.announced = true;
                self.events.push(Event::Added(o.id));
            }
        }
    }

    /// New buffers for `o` where its mode needs them: as many as
    /// [`Buffers`] says, cleared, the size of its mode. `None` where its
    /// buffers already fit, so a display online keeps what it shows.
    fn stage(&self, o: &Output<S::Memory>) -> io::Result<Option<Vec<Buffer<S::Memory>>>> {
        let count = self.buffers.count();
        if o.buffers.len() == count && self.buffer_size_matches(o) {
            return Ok(None);
        }
        let (w, h) = o.size();
        let mut made = Vec::with_capacity(count);
        for _ in 0..count {
            match self.source.create(&self.card, w, h) {
                Ok(b) => {
                    lock(&b.map).as_mut().fill(0);
                    made.push(b);
                }
                Err(e) => {
                    self.release(made);
                    return Err(e);
                }
            }
        }
        Ok(Some(made))
    }

    /// The framebuffer a modeset puts on the primary plane: the first of
    /// the display's new buffers, or else the front buffer it shows.
    fn staged_fb(o: &Output<S::Memory>, made: Option<&Vec<Buffer<S::Memory>>>) -> u32 {
        match made {
            Some(made) => made.first().map_or(0, |b| b.fb),
            None => o.buffers.get(o.front()).map_or(0, |b| b.fb),
        }
    }

    /// Gives `o` the new buffers of a modeset that took: the first is on
    /// screen, and none holds anything drawn. A token out for an old buffer
    /// comes back too late, and the old buffer stays until it does. A
    /// display that keeps its buffers at a new mode, as one whose refresh
    /// rate alone changed, is drawn whole too: its buffers are age 0 and it
    /// is told to redraw, so its owner reads the new rate.
    fn install(&self, o: &mut Output<S::Memory>, made: Option<Vec<Buffer<S::Memory>>>) {
        let was = o.set.replace(o.mode);
        // Set to another mode than before, the display owes nothing.
        if was != Some(o.mode) {
            o.owed = None;
        }
        if let Some(made) = made {
            let old = std::mem::replace(&mut o.buffers, made);
            self.release(old);
            o.back = 1usize.checked_rem(o.buffers.len()).unwrap_or(0);
            o.ages = Ages::default();
            o.lease = o.lease.wrapping_add(1);
            o.clear_owed = false;
            o.fresh = false;
            o.redraw = true;
        } else if was.is_some_and(|was| was != o.mode) {
            o.ages = Ages::default();
            o.redraw = true;
        }
    }

    fn buffer_size_matches(&self, o: &Output<S::Memory>) -> bool {
        o.buffers.first().is_some_and(|b| b.size == o.size())
    }

    fn free_buffers(&self, o: &mut Output<S::Memory>) {
        self.release(std::mem::take(&mut o.buffers));
    }

    /// Frees `buffers`, keeping each whose memory a token holds until the
    /// token comes back, so the display never waits for the render thread.
    fn release(&self, buffers: Vec<Buffer<S::Memory>>) {
        for b in buffers {
            if Arc::strong_count(&b.map) > 1 {
                self.retired.borrow_mut().push(b);
            } else {
                self.source.destroy(&self.card, b);
            }
        }
    }

    /// Frees the buffers let go of whose tokens have come back.
    fn reap(&self) {
        let back: Vec<Buffer<S::Memory>> = {
            let mut retired = self.retired.borrow_mut();
            let (free, kept) = std::mem::take(&mut *retired)
                .into_iter()
                .partition(|b| Arc::strong_count(&b.map) == 1);
            *retired = kept;
            free
        };
        for b in back {
            self.source.destroy(&self.card, b);
        }
    }

    fn free(&self, mut o: Output<S::Memory>) {
        if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
            link.disarm(t);
        }
        self.free_buffers(&mut o);
        drop_cursor(&self.card, &mut o);
        if o.mode_blob != 0 {
            let _ = self.card.destroy_blob(o.mode_blob);
        }
    }

    /// Turns off, in a blocking commit of its own with no flip event, what
    /// the modeset after it would otherwise leave lit: what a returning
    /// display might still be shown on, and each CRTC [`Self::vacated`]
    /// finds. That is the CRTC a returning display's connector is on, the
    /// CRTC it is about to take if that one shows a mode, and each vacated
    /// CRTC, with their planes and every connector on them.
    ///
    /// The modeset after it then sets a returning display's mode, CRTC and
    /// connector anew, a full modeset, whatever mode another master or the
    /// kernel's console left there. On this machine's amdgpu cards the
    /// small panel showed nothing under frames committed on the mode the
    /// console set at its return, until a VT switch set its mode again. A
    /// CRTC that a display online also drives stays on, and only loses the
    /// returning connector. A display on no CRTC, about to take one with no
    /// mode, starts from off already and needs nothing turned off.
    ///
    /// A vacated CRTC may show a mode with `ACTIVE` at 0, as a desktop's
    /// DPMS off leaves it. The modeset asks a flip event of every CRTC in
    /// its commit, and the kernel refuses a flip event on a CRTC that is
    /// off and stays off (`drm_atomic_crtc_check`), so this commit, which
    /// asks for none, turns the vacated CRTCs off.
    fn set_off_first(&self) -> Result<(), Failure> {
        let mut ids = Vec::new();
        let mut off = Vec::new();
        for o in self.outputs.iter().filter(|o| o.returning) {
            let on = self.connector_crtc(o.id.0);
            if on.is_some() {
                ids.push(o.id.0);
            }
            for crtc in [on, Some(o.crtc)].into_iter().flatten() {
                let driven = self.outputs.iter().any(|d| !d.returning && d.crtc == crtc);
                if !driven && !off.contains(&crtc) && self.crtc_lit(crtc) {
                    off.push(crtc);
                }
            }
        }
        for (crtc, on) in self.vacated() {
            for id in on {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if !off.contains(&crtc) {
                off.push(crtc);
            }
        }
        if ids.is_empty() && off.is_empty() {
            return Ok(());
        }
        let mut req = Request::default();
        let prepared = (|| -> io::Result<()> {
            for &id in &ids {
                self.unbind(&mut req, id)?;
            }
            let all = self.card.resources()?.connectors;
            for &crtc in &off {
                let props = Props::new(&self.card.properties(crtc, Object::Crtc)?);
                props.set(&mut req, crtc, "MODE_ID", 0)?;
                props.set(&mut req, crtc, "ACTIVE", 0)?;
                for &other in &all {
                    if !ids.contains(&other) && self.connector_crtc(other) == Some(crtc) {
                        self.unbind(&mut req, other)?;
                    }
                }
            }
            Ok(())
        })();
        prepared.map_err(|e| {
            log::error!("cannot turn off what the modeset would leave lit: {e}");
            Failure::Other(e)
        })?;
        self.hide_planes(&mut req, &off, false);
        self.card
            .commit(&req, DRM_MODE_ATOMIC_ALLOW_MODESET)
            .map_err(|e| {
                // EACCES while another process holds master is expected.
                if errno(&e) == Some(Errno::ACCESS) {
                    log::debug!(
                        "the commit turning off what the modeset would leave lit was refused: {e}"
                    );
                    Failure::Test(e)
                } else {
                    log::error!("cannot turn off what the modeset would leave lit: {e}");
                    Failure::Other(e)
                }
            })
    }

    /// Each vacated CRTC, with the connectors on it: a CRTC that no display
    /// takes and that only connectors of the displays are on. The kernel
    /// puts a connector on a CRTC only while that CRTC is enabled, so each
    /// vacated CRTC shows a mode, whether it is active or not. The modeset
    /// moves those connectors to the displays' own CRTCs, and the kernel
    /// refuses a CRTC left enabled with no connector on it
    /// (`drm_atomic_helper_check_modeset`), so [`Self::set_off_first`]
    /// turns these CRTCs off. Each connector's CRTC is read from its
    /// `CRTC_ID`, so a connector whose encoder cannot be read is found too.
    ///
    /// A CRTC that a connector no display drives is on stays as it is. The
    /// connectors are those the card lists to vtium, which asks for no
    /// writeback connectors (`DRM_CLIENT_CAP_WRITEBACK_CONNECTORS`), so a
    /// writeback connector on a CRTC goes unseen. Where vtium cannot read
    /// which CRTC a listed connector is on, that connector might share a
    /// CRTC with a display's connector, so no CRTC counts as vacated.
    fn vacated(&self) -> Vec<(u32, Vec<u32>)> {
        let connectors = match self.card.resources() {
            Ok(r) => r.connectors,
            Err(e) => {
                log::warn!("cannot list the card's connectors: {e}");
                return Vec::new();
            }
        };
        let mut on = Vec::new();
        for id in connectors {
            let crtc = self
                .card
                .properties(id, Object::Connector)
                .and_then(|props| {
                    value(&props, "CRTC_ID")
                        .and_then(|c| u32::try_from(c).ok())
                        .ok_or_else(|| io::Error::other("it has no CRTC_ID"))
                });
            match crtc {
                Ok(0) => {}
                Ok(crtc) => on.push((id, crtc)),
                Err(e) => {
                    log::warn!(
                        "cannot read which CRTC connector {id} is on, so no CRTC is turned off: {e}"
                    );
                    return Vec::new();
                }
            }
        }
        let mut crtcs: Vec<u32> = on.iter().map(|&(_, crtc)| crtc).collect();
        crtcs.sort_unstable();
        crtcs.dedup();
        crtcs
            .into_iter()
            .filter(|&crtc| !self.outputs.iter().any(|o| o.crtc == crtc))
            .filter_map(|crtc| {
                let ids: Vec<u32> = on
                    .iter()
                    .filter(|&&(_, c)| c == crtc)
                    .map(|&(id, _)| id)
                    .collect();
                let ours = ids
                    .iter()
                    .all(|&id| self.outputs.iter().any(|o| o.id.0 == id));
                ours.then_some((crtc, ids))
            })
            .collect()
    }

    /// Whether `crtc` shows a mode or is active, as the card reports it; a
    /// CRTC whose state cannot be read counts as lit.
    fn crtc_lit(&self, crtc: u32) -> bool {
        let mode = self.card.crtc_mode(crtc).map_or(true, |m| m.is_some());
        let active = self
            .card
            .properties(crtc, Object::Crtc)
            .map_or(true, |p| value(&p, "ACTIVE").is_some_and(|a| a != 0));
        mode || active
    }

    /// One atomic modeset of every display: tried first as a test-only
    /// commit, then committed without blocking, with a flip event.
    fn atomic_modeset(&mut self, dark: bool) -> Result<(), Failure> {
        self.set_off_first()?;
        let mut req = Request::default();
        let off = if dark {
            self.prepare_dark(&mut req)
        } else {
            Vec::new()
        };
        self.clear_planes(&mut req, &off);
        let mut staged = Vec::with_capacity(self.outputs.len());
        let mut outputs = std::mem::take(&mut self.outputs);
        // A wake-up or a hotplug counts as user activity (the operator's
        // decision on vtium/vtium#18): the modeset's ACTIVE=1 brings every
        // display on, so it is recorded on.
        let was: Vec<Option<(u32, Dpms)>> = outputs.iter().map(|o| o.dpms).collect();
        for o in &mut outputs {
            o.dpms = o.dpms.map(|(prop, _)| (prop, Dpms::On));
        }
        let mut prepared = Ok(());
        for o in &outputs {
            log::info!(
                "preparing the modeset of {} at {}x{}",
                o.name,
                o.size().0,
                o.size().1
            );
            prepared = self.stage(o).and_then(|made| {
                let fb = Self::staged_fb(o, made.as_ref());
                match self.card.create_blob(&mode_bytes(&o.mode.0)) {
                    Ok(blob) => {
                        staged.push((made, blob));
                        o.prepare(&mut req, fb, blob, None, self.hotspot)
                    }
                    Err(e) => {
                        self.release(made.unwrap_or_default());
                        Err(e)
                    }
                }
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
        // A display not set keeps what it had: an online one its buffers
        // and what it shows.
        for (o, (made, blob)) in outputs.iter_mut().zip(staged) {
            if outcome.is_ok() {
                if o.mode_blob != 0 {
                    let _ = self.card.destroy_blob(o.mode_blob);
                }
                o.mode_blob = blob;
                self.install(o, made);
                o.online = true;
                o.returning = false;
                o.cursor_set = o.cursor_shown();
                o.start_flip();
                o.modeset_flip = true;
                o.damage.set_whole();
            } else {
                let _ = self.card.destroy_blob(blob);
                self.release(made.unwrap_or_default());
            }
        }
        for (o, was) in outputs.iter_mut().zip(was) {
            if outcome.is_err() {
                // Nothing changed on the card: the state recorded stays.
                o.dpms = was;
            } else if was.is_some_and(|(_, d)| d != Dpms::On) {
                o.powered_on();
            }
        }
        self.outputs = outputs;
        outcome
    }

    /// The legacy modeset: each CRTC in turn, with no flip event. A
    /// display whose new mode cannot be set is set at the mode it is set
    /// to: held back where the card refused the mode with `EINVAL`, and
    /// else owed it. Returns the displays it could not set.
    fn legacy_modeset(&mut self) -> Vec<DisplayId> {
        self.legacy_dark();
        let mut unset = Vec::new();
        let mut outputs = std::mem::take(&mut self.outputs);
        let driven: Vec<u32> = outputs
            .iter()
            .filter(|o| !o.returning)
            .map(|o| o.crtc)
            .collect();
        for o in &mut outputs {
            let mut set = self.legacy_set(o, &driven);
            let again = match &set {
                Ok(()) => false,
                Err(Failure::Test(_)) => o.hold_back(),
                Err(Failure::Other(_)) => o.keep_set_mode(),
            };
            if again {
                set = self.legacy_set(o, &driven);
            }
            if set.is_err() {
                unset.push(o.id);
            }
        }
        self.outputs = outputs;
        unset
    }

    /// Sets one display's CRTC at its mode, with new buffers where the mode
    /// needs them. The card refusing the mode, which it does by failing the
    /// CRTC set with `EINVAL`, fails as [`Failure::Test`].
    fn legacy_set(&self, o: &mut Output<S::Memory>, driven: &[u32]) -> Result<(), Failure> {
        log::info!(
            "preparing the modeset of {} at {}x{}",
            o.name,
            o.size().0,
            o.size().1
        );
        let made = self.stage(o).map_err(|e| {
            log::error!("cannot allocate buffers for {}: {e}", o.name);
            Failure::Other(e)
        })?;
        // A display that returns is set from off, as the atomic modeset
        // sets it, where the card can turn its CRTC off; a CRTC a display
        // online drives stays on.
        if o.returning {
            let on = self.connector_crtc(o.id.0);
            if let Some(crtc) = on.filter(|c| !driven.contains(c))
                && let Err(e) = self.card.disable_crtc(crtc)
            {
                log::debug!("cannot turn the CRTC of {} off first: {e}", o.name);
            }
        }
        // Another program may have left its cursor on the CRTC.
        if let Err(e) = self.card.hide_cursor(o.crtc) {
            log::warn!("cannot hide the hardware cursor: {e}");
        }
        let fb = Self::staged_fb(o, made.as_ref());
        match self.card.set_crtc(o.crtc, fb, o.id.0, &o.mode) {
            Ok(()) => {
                self.install(o, made);
                o.online = true;
                o.returning = false;
                o.modeset_flip = true;
                // The CRTC is on, whatever DPMS it was at.
                if !o.dpms_on() {
                    o.dpms = o.dpms.map(|(prop, _)| (prop, Dpms::On));
                    o.powered_on();
                }
                Ok(())
            }
            Err(e) => {
                log::error!("cannot set the CRTC of {}: {e}", o.name);
                self.release(made.unwrap_or_default());
                if errno(&e) == Some(Errno::INVAL) {
                    Err(Failure::Test(e))
                } else {
                    Err(Failure::Other(e))
                }
            }
        }
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

/// The bit of the CRTC at `index` in the card's list, as a mask of
/// possible CRTCs names it; 0 past the 32 such a mask holds.
fn index_bit(index: usize) -> u32 {
    u32::try_from(index)
        .ok()
        .and_then(|i| 1u32.checked_shl(i))
        .unwrap_or(0)
}

/// Puts display `d` on the CRTC at `c`, which the display placed there
/// before, if any, has already left.
fn assign(d: usize, c: usize, slot: &mut [Option<usize>], owner: &mut [Option<usize>]) {
    if let Some(old) = slot.get(d).copied().flatten()
        && let Some(held) = owner.get_mut(old)
        && *held == Some(d)
    {
        *held = None;
    }
    if let Some(at) = slot.get_mut(d) {
        *at = Some(c);
    }
    if let Some(held) = owner.get_mut(c) {
        *held = Some(d);
    }
}

/// One augmenting step of Kuhn's matching of displays to CRTCs: places
/// display `d` on a CRTC its mask in `able` allows, one that is free or
/// one that the display on it frees by moving, through the same step, to
/// another CRTC it can drive. A display that `fixed` marks never moves.
/// `tried` holds the CRTCs this step has tried, so each is tried once.
/// Whether it placed `d`.
fn augment(
    d: usize,
    able: &[u32],
    fixed: &[bool],
    slot: &mut [Option<usize>],
    owner: &mut [Option<usize>],
    tried: &mut u32,
) -> bool {
    let can = able.get(d).copied().unwrap_or(0);
    for c in 0..owner.len() {
        let bit = index_bit(c);
        if can & bit == 0 || *tried & bit != 0 {
            continue;
        }
        *tried |= bit;
        let free = match owner.get(c).copied().flatten() {
            None => true,
            Some(other) => {
                !fixed.get(other).copied().unwrap_or(false)
                    && augment(other, able, fixed, slot, owner, tried)
            }
        };
        if free {
            assign(d, c, slot, owner);
            return true;
        }
    }
    false
}

/// Takes display `o`'s pointer image off the card: its framebuffer, whose
/// removal takes the cursor plane off its CRTC in the kernel
/// (`atomic_remove_fb`), and its dumb buffer. The cursor plane is recorded
/// hidden, and the owner draws the pointer itself
/// ([`Drm2d::hardware_cursor`]).
fn drop_cursor<K: Kms, M>(card: &K, o: &mut Output<M>) {
    if let Some(c) = o.cursor.take() {
        let _ = card.rm_fb(c.fb);
        let _ = card.destroy_dumb(c.dumb.handle);
    }
    o.cursor_set = None;
}

/// How a modeset failed: the card refused it, as an atomic modeset's
/// test-only commit or a legacy CRTC set failing with `EINVAL`, or
/// anything else.
enum Failure {
    Test(io::Error),
    Other(io::Error),
}

/// Whether the card refused a modeset: its test commit failed, other than
/// as refused while another process holds DRM master.
fn refused(set: &Result<(), Failure>) -> bool {
    matches!(set, Err(Failure::Test(e)) if errno(e) != Some(Errno::ACCESS))
}

impl Failure {
    fn into_inner(self) -> io::Error {
        match self {
            Failure::Test(e) | Failure::Other(e) => e,
        }
    }
}

impl<K: Kms, S: Source<K>> Drop for Drm2d<K, S> {
    fn drop(&mut self) {
        if let Some(link) = &self.link
            && let Some(token) = self.timers.wake_retry.take()
        {
            link.disarm(token);
        }
        for o in std::mem::take(&mut self.outputs) {
            self.free(o);
        }
    }
}
#[cfg(test)]
mod tests;
