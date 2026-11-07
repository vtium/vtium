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

mod back;
mod buffers;
mod dark;
mod matching;
mod output;

pub use output::CURSOR_MAX;
mod source;

use std::cell::{Cell, RefCell};
use std::io;
use std::sync::Arc;

use calloop::RegistrationToken;
use drm_ffi::{
    DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH, DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC,
    DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT, DRM_CLIENT_CAP_UNIVERSAL_PLANES,
    DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_ATOMIC_TEST_ONLY,
    DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::io::Errno;

use super::edid::{Edid, EdidId};
use super::kms::{Connector, CrtcEvent, Encoder, Kms, Object, Property, Request, Resources};
use super::mode::{self, Choice, Mode, ModePolicy};
use super::path;
use super::{
    Ages, Buffers, Damage, DarkPolicy, Density, DisplayId, Dpms, Event, Identity, Loan, Memory,
    Rect, SwapError, Swapped, Token, lock,
};
use back::needed;
use buffers::{Buffer, Dumbs, FORMAT, Source};
use dark::Dark;
use matching::{Place, index_bit, matching};
use output::{
    ARGB8888, ColorProps, Cursor, Moved, Output, PLANE_CURSOR, PLANE_OVERLAY, PLANE_PRIMARY,
    Planes, Props, bits_per_color, clip_bytes, connector_name, dpms_value, invalid, mode_bytes,
    value,
};
use source::{Clock, Link, Timers};

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}

/// Whether `e` says the kernel removed the card: every ioctl on a card
/// the kernel removed fails with ENODEV (`drm_ioctl`).
fn card_removed(e: &io::Error) -> bool {
    errno(e) == Some(Errno::NODEV)
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

/// A connected connector the scan binds a display to, once the displays
/// gone have let their CRTCs go: what the scan found of it, the identity
/// it gives the display, and the mode chosen for it.
struct New {
    conn: Connector,
    found: Found,
    identity: Identity,
    choice: Choice,
}

/// What a placement did: whether it bound, moved or let go of a display,
/// and whether it left a new display without a CRTC.
#[derive(Clone, Copy, Debug)]
struct Placed {
    changed: bool,
    unplaced: bool,
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
    /// The connectors whose display the card refused to set beside the
    /// others even with lit displays moved for it. Until such a connector
    /// is unplugged, a hotplug moves no lit display for it, so the lit
    /// displays do not blank at each hotplug for a display that cannot
    /// light.
    move_refused: Vec<u32>,
    /// The CRTCs that owe the event of a flip whose display was let go while
    /// the flip was in flight, one entry for each such flip, oldest first,
    /// each with the timer that ends it. A display that takes such a CRTC
    /// must not take that event as its own flip's, which the kernel
    /// completes after it (`stall_checks`), so the event is dropped when it
    /// comes. An event that never comes, as a driver loses one, would make
    /// the entry drop the next display's event instead, so the entry ends
    /// after [`crate::FLIP_WAIT_MS`], the bound the flip had
    /// ([`Self::stale_expired`]).
    stale: Vec<(u32, Option<RegistrationToken>)>,
    /// The connectors whose color properties the card refused at a
    /// modeset, each logged once as a warning.
    color_refused: RefCell<Vec<String>>,
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
    /// An ioctl answered ENODEV: the kernel removed the card, as when a
    /// native driver takes the display over from simpledrm, and fails every
    /// ioctl on it so (`drm_ioctl`). The operation that met it stops, and
    /// the card is let go as the operation ends ([`Self::let_card_go`]).
    removed: Cell<bool>,
    /// The card was let go after the kernel removed it: each display was
    /// reported removed, and drm2d sends the card nothing again.
    gone: bool,
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
            move_refused: Vec::new(),
            stale: Vec::new(),
            color_refused: RefCell::new(Vec::new()),
            waiting: None,
            lost_flip: false,
            blanked: false,
            owed: false,
            retries: None,
            waking: false,
            wake_failed: false,
            unset: Vec::new(),
            removed: Cell::new(false),
            gone: false,
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

    /// Whether the kernel removed the card: an ioctl answered ENODEV, as
    /// every ioctl on an unplugged card does (`drm_ioctl`). drm2d then
    /// reported each display removed and sends the card nothing again, so
    /// the owner can let the card go, which closes it.
    pub fn gone(&self) -> bool {
        self.gone
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
    /// only with atomic commits (the operator's decision of 2026-10-26).
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
    /// go with the next swap.
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
                        // The commit after it fails the same way.
                        Err(e) if card_removed(&e) => None,
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
                && !card_removed(e)
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
            if card_removed(&e) {
                // The card is gone with every display on it, and the owner
                // hears each removed.
                self.removed.set(true);
                self.settle();
                return Err(SwapError::Off);
            }
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
            if let Err(e) = self
                .card
                .set_connector_property(o.id.0, prop, dpms_value(dpms))
            {
                self.seen(&e);
                self.settle();
                return Err(e);
            }
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
        if self.gone {
            return Err(Errno::NODEV.into());
        }
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
                self.seen(e);
                if !card_removed(e) {
                    log::warn!("cannot take DRM master: {e}");
                }
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

    /// Lets the displays go: blanks them, waits for the flips in flight (at
    /// most [`crate::FLIP_WAIT_MS`]), hides the cursor and overlay planes
    /// that show something and drops DRM master. [`Event::Asleep`] tells
    /// when it is done. A card that never woke drops the master its open
    /// gave it, so the vtium of the VT shown can take it (vtium/vtium#139).
    ///
    /// The wait lets the program that takes the card commit at once: the
    /// kernel refuses a commit that does not block with `EBUSY` on a CRTC
    /// whose last commit has not flipped yet (`stall_checks`). A bound that
    /// runs out during the sleep is logged at debug level, since the
    /// program taking the card may hold the flip up, as when another master
    /// starts on the card while vtium's first modeset is in flight. Where no
    /// plane shows anything, as before vtium shows a pointer, the sleep
    /// commits nothing (vtium/vtium#280).
    pub fn sleep(&mut self) {
        if self.gone {
            self.foreground = false;
            self.events.push(Event::Asleep);
            return;
        }
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

    /// Ends the sleep once no flip is in flight: hides the planes that show
    /// something, under the master the commit needs, and drops DRM master.
    fn finish_sleep(&mut self) {
        let hide = self.planes_to_hide();
        if !hide.entries.is_empty()
            && let Err(e) = self.card.commit(&hide, DRM_MODE_ATOMIC_NONBLOCK)
        {
            log::warn!("cannot hide the cursor and overlay planes: {e}");
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

    /// What hides each cursor and overlay plane that shows something, on a
    /// CRTC or with a framebuffer, before vtium lets the card go, so the
    /// program that takes the card finds none of vtium's pointer on it. A
    /// plane already hidden is left out: the modeset hides every one, so
    /// only the planes vtium has shown since are in it. It is empty under
    /// the legacy calls and without master, where vtium commits nothing.
    fn planes_to_hide(&self) -> Request {
        let mut req = Request::default();
        if !self.master || self.legacy {
            return req;
        }
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
            let other = matches!(value(&props, "type"), Some(PLANE_CURSOR | PLANE_OVERLAY));
            let shows = ["CRTC_ID", "FB_ID"]
                .iter()
                .any(|name| value(&props, name).is_some_and(|v| v != 0));
            if other && shows {
                let props = Props::new(&props);
                let hidden = props
                    .set(&mut req, plane, "CRTC_ID", 0)
                    .and_then(|()| props.set(&mut req, plane, "FB_ID", 0));
                if let Err(e) = hidden {
                    log::warn!("cannot hide plane {plane}: {e}");
                }
            }
        }
        req
    }

    /// Notes an ioctl's error `e` that says the kernel removed the card,
    /// for [`Self::let_card_go`].
    fn seen(&self, e: &io::Error) {
        if card_removed(e) {
            self.removed.set(true);
        }
    }

    /// Lets the card go once an ioctl said the kernel removed it, as
    /// udev's report of its removal has the owner let it go, but at once:
    /// udev reports the removal only after udevd has handled it, and a
    /// card removed this early in boot may have no udevd to report it
    /// (vtium/vtium#295). Each display is reported removed, a sleep under
    /// way reports [`Event::Asleep`], and every wait, retry and timer ends.
    /// The displays' buffers, cursors and mode blobs go with no ioctl, as
    /// the kernel frees them when the card's file closes, and drm2d sends
    /// the card nothing again ([`Drm2d::gone`]).
    fn let_card_go(&mut self) {
        if self.gone || !self.removed.get() {
            return;
        }
        self.gone = true;
        log::warn!(
            "the kernel removed the card, as a new driver does when it takes the display over; vtium lets its displays go and uses the card no more"
        );
        for mut o in std::mem::take(&mut self.outputs) {
            if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
                link.disarm(t);
            }
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
        }
        for (_, timer) in std::mem::take(&mut self.stale) {
            if let (Some(link), Some(t)) = (&self.link, timer) {
                link.disarm(t);
            }
        }
        self.retired.borrow_mut().clear();
        self.identities.clear();
        self.dark.clear();
        self.unset.clear();
        if self.waiting.take() == Some(Waiting::Sleep) {
            self.events.push(Event::Asleep);
        }
        self.awake = false;
        self.master = false;
        self.retries = None;
        self.owed = false;
        self.wake_failed = false;
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
        if self.gone {
            return Ok(());
        }
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
    /// the vblank's event wakes. The event of a flip whose display was let
    /// go while it was in flight is dropped ([`Self::stale`]).
    fn flipped(&mut self, crtc: u32, frame: bool) {
        if let Some(at) = self.stale.iter().position(|&(c, _)| c == crtc) {
            let (_, timer) = self.stale.remove(at);
            if let (Some(link), Some(t)) = (&self.link, timer) {
                link.disarm(t);
            }
            log::debug!("the event on CRTC {crtc} is the flip of a display let go");
            return;
        }
        let ready = self.awake && self.waiting.is_none();
        match self.on_crtc(crtc).and_then(|at| self.outputs.get_mut(at)) {
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
        let Some(o) = self.on_crtc(crtc).and_then(|at| self.outputs.get_mut(at)) else {
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
        if matches!(self.waiting, Some(Waiting::Sleep)) {
            // The program taking the card may hold the flip up.
            log::debug!(
                "no {event} event for {} as the card goes to sleep; taking the flip as done",
                o.name
            );
        } else if std::mem::replace(&mut self.lost_flip, true) {
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

    /// The bound on the oldest entry for `crtc` in [`Self::stale`] ran out:
    /// its event is taken as lost, and the next event on `crtc` is a
    /// display's own.
    pub(crate) fn stale_expired(&mut self, crtc: u32) {
        if let Some(at) = self.stale.iter().position(|&(c, _)| c == crtc) {
            self.stale.remove(at);
            log::debug!("no event on CRTC {crtc} for the flip of a display let go");
        }
    }

    /// The index of the display that an event or a bound on `crtc` belongs
    /// to: the display whose flip in flight was committed on `crtc`, else
    /// the display on `crtc`. A hotplug can move a display to another CRTC
    /// while it flips, and give the CRTC it left to a new display, before
    /// the flip's event arrives ([`Drm2d::place`]).
    fn on_crtc(&self, crtc: u32) -> Option<usize> {
        self.outputs
            .iter()
            .position(|o| o.flipping && o.flip_crtc == crtc)
            .or_else(|| self.outputs.iter().position(|o| o.crtc == crtc))
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
        self.let_card_go();
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
                    o.flip_timer =
                        want.and_then(|f| Some((link.arm(Clock::Flip(o.flip_crtc))?, f)));
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

    /// The scan: lets go of the displays gone, binds a display to each new
    /// connected connector with modes, and sets modes as `scan` asks. The
    /// displays gone let their CRTCs go first, so a connector that comes
    /// back under a new id in one scan, as a DisplayPort multi-stream
    /// connector does when its monitor drops for a moment, can take the
    /// CRTC its old display held (vtium/vtium#252).
    fn scan(&mut self, scan: Scan) -> io::Result<()> {
        if !self.awake {
            return Ok(());
        }
        // The card takes master as it wakes and lets it go only as it
        // falls asleep, so a scan always runs under master.
        debug_assert!(self.master, "an awake card holds DRM master");
        let res = self.card.resources().inspect_err(|e| self.seen(e))?;
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
        // Every later scan reads which CRTCs each display bound before can
        // drive from those connectors, so it can move one ([`Self::place`]).
        let follow = scan == Scan::WakeUp && self.scanned;
        let mut seen = Vec::new();
        let mut new = Vec::new();
        let mut connected = Vec::new();
        for &id in &res.connectors {
            let Some(conn) = self.connector(id) else {
                continue;
            };
            if !conn.connected || conn.modes.is_empty() {
                continue;
            }
            connected.push(id);
            if self.scanned {
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
            if let Some(n) = self.candidate(conn, found, identity) {
                new.push(n);
            }
        }
        self.dark.retain(|d| {
            if !d.available {
                log::info!("display {}, left dark, is gone", d.identity.connector);
            }
            d.available
        });
        self.dark
            .sort_by_key(|d| res.connectors.iter().position(|&c| c == d.id));
        let (kept, gone): (Vec<_>, Vec<_>) = self.outputs.drain(..).partition(|o| o.available);
        self.outputs = kept;
        // The CRTC each display gone held, with its identity, so a monitor
        // that comes back under a new connector id takes it back.
        let mut freed = Vec::new();
        for o in gone {
            if let Some((_, identity)) = self.identities.iter().find(|(i, _)| *i == o.id) {
                freed.push((identity.clone(), o.crtc));
            }
            log::info!("display {} is gone", o.name);
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
            self.free(o);
            changed = true;
        }
        self.move_refused.retain(|id| connected.contains(id));
        // Blanked, a hotplug's modeset waits for the input that ends the
        // blank, so no lit display moves at it: a display that drew on
        // with its record moved would flip on a CRTC its connector is not
        // on. A new display that could light only by moving one waits for
        // that input too, whose scan places it.
        let defer = self.blanked && scan != Scan::WakeUp;
        let placed = self.place(&res, &seen, new, &freed, follow, defer);
        if placed.changed {
            needs = true;
            changed = true;
        }
        if defer && placed.unplaced {
            self.owed = true;
        }
        self.scanned = true;
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
        if needs && defer {
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

    /// Connector `conn` as a display the scan binds, with the identity
    /// `identity`, or `None` where no mode of its can be chosen.
    fn candidate(&self, conn: Connector, found: Found, identity: Identity) -> Option<New> {
        let wanted = self.policy.wanted.as_ref().and_then(|f| f(&found.name));
        let choice = mode::choose(
            &conn.modes,
            found.shown.as_ref(),
            wanted,
            self.policy.use_original,
        )?;
        Some(New {
            conn,
            found,
            identity,
            choice,
        })
    }

    /// A display for connector `conn` on `crtc`, at `pipe` in the card's
    /// list of CRTCs, at the mode `choice`, or `None` when that CRTC has no
    /// primary plane.
    fn bind(
        &self,
        conn: &Connector,
        found: Found,
        choice: Choice,
        crtc: u32,
        pipe: usize,
    ) -> Option<Output<S::Memory>> {
        let Found {
            name,
            props: connector_props,
            edid,
            ..
        } = found;
        // Its modeset turns the display on, whatever DPMS the connector was
        // left at.
        let dpms = connector_props
            .iter()
            .find(|p| p.name == "DPMS")
            .map(|p| (p.id, Dpms::On));
        let planes = if self.legacy {
            None
        } else {
            let Some(mut planes) = self.find_planes(crtc, pipe) else {
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
        // Where the range of `max bpc` cannot be read, the modeset sets the
        // bits vtium draws, and a card that refuses them sets the mode
        // without the color properties (ADR-1401 D14).
        let color = ColorProps::new(&connector_props, |id| {
            self.card
                .property_range(id)
                .inspect_err(|e| log::warn!("cannot read the range of max bpc of {name}: {e}"))
                .ok()
                .flatten()
        });
        Some(Output {
            id: DisplayId(conn.id),
            name,
            crtc,
            pipe,
            planes,
            color,
            mode: choice.current,
            moved_from: None,
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
            flip_crtc: crtc,
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

    /// The CRTCs an encoder of connector `conn` can drive, a bit per index
    /// into the card's list of CRTCs; `current` is the encoder it is on, as
    /// read with it. An encoder that cannot be read adds none.
    fn reach(&self, conn: &Connector, current: Option<Encoder>) -> u32 {
        conn.encoders
            .iter()
            .filter_map(|&e| {
                if e == conn.encoder {
                    current
                } else {
                    self.encoder(e)
                }
            })
            .fold(0, |all, e| all | e.possible_crtcs)
    }

    /// Places the displays on CRTCs, one CRTC each: the new ones, `new`,
    /// and the ones bound before. Binds a display to each new connector
    /// placed, moves each display bound before to the CRTC it is placed
    /// on, and says whether it bound, moved or let go of any display and
    /// whether it left a new display without a CRTC.
    ///
    /// Every scan places the displays together, wherever the card has a
    /// CRTC for each. At a hotplug, and at the retry of a failed one, a lit
    /// display bound before stays on its CRTC whenever every new display
    /// can light without it moving. Where a new display can light only on a
    /// CRTC that a lit display holds, and that lit display can move to
    /// another CRTC its connector's encoders drive, the lit display moves
    /// and the new display takes its CRTC, and the placement moves as few
    /// lit displays as it can. The modeset after the scan sets the moved
    /// display's mode on its new CRTC anew, so it blanks once while it
    /// moves and then lights beside the new one, as a desktop such as KWin
    /// does at a hotplug (the maintainer's decision on vtium/vtium#268).
    /// Each display bound before that the placement puts off the CRTC it
    /// wants remembers that CRTC (`moved_from`), and whether it was lit
    /// there, until a modeset sets it on its new CRTC; a modeset that
    /// cannot set the displays in their new places moves it back
    /// ([`Self::move_back`]). A new display that no placement gives a CRTC
    /// is not bound, with an error, nothing moves for it, and the next scan
    /// tries it again.
    ///
    /// At a hotplug or a retry, a display online wants the CRTC vtium last
    /// set it on, which is the one it was moved off where a move still
    /// waits for its modeset. So a move whose reason is gone before its
    /// modeset, as when the new display is unplugged first, is undone
    /// without blanking the display.
    ///
    /// Where `pin` is set, as while the scan's modeset waits for the input
    /// that ends a blank, no display bound before moves at a hotplug or a
    /// retry. A new display the card refused beside the others even with
    /// lit displays moved for it (`move_refused`) moves no display: it is
    /// placed after the others, on a CRTC none of them takes.
    ///
    /// The displays are placed together, not each on its own as it is
    /// bound. Placed one at a time, the first display at the first scan
    /// could take the only CRTC a later one can drive, and a display
    /// plugged in would stay dark where a lit display held the only CRTCs
    /// it can drive.
    ///
    /// A wake-up after the first scan (`follow`) places each display bound
    /// before on the CRTC its connector is on now, read from the connectors
    /// `seen` as the wake-up read them. The program that held the card while
    /// vtium slept may have moved a connector to another CRTC. A modeset on
    /// the CRTC the display had would take the connector off a CRTC that
    /// program left lit, and the kernel refuses a lit CRTC with no connector
    /// whatever mode the commit sets (`drm_atomic_helper_check_modeset`), so
    /// the display would never wake again (vtium/vtium#249).
    ///
    /// Each display bound before wants one CRTC. At a wake-up that is the
    /// CRTC its connector is on, or, where its connector is on no CRTC or
    /// the display returns, its own CRTC; at any other scan it is the CRTC
    /// above. It can take that CRTC and any CRTC its connector's encoders
    /// drive, as read from the connectors `seen`. A new display wants the
    /// CRTC a display gone in this scan held (`freed`), where that display
    /// showed the same monitor ([`same_monitor`]) and no new display placed
    /// before it took that CRTC back, as a DisplayPort multi-stream monitor
    /// that comes back under a new connector id does; else it wants the
    /// CRTC its connector is on. A display that returns, as each display
    /// new after the first scan does, has its first modeset turn off the
    /// CRTC its connector is on ([`Self::set_off_first`]).
    ///
    /// The displays counted lit are placed first: at a wake-up and at the
    /// first scan those whose connector is on a CRTC, and at any other scan
    /// those bound before that are online. At a wake-up and at the first
    /// scan the displays whose connector is on no CRTC come next, and the
    /// rest last; at any other scan the rest come next. Within a group the
    /// displays bound before come first, then the new ones, each in the
    /// card's connector order. The matching ([`matching`]) places them one
    /// at a time, and of the placements of the displays it can place, it
    /// takes one that puts as few lit displays as it can off the CRTC they
    /// want, and then as few of the others.
    ///
    /// The CRTCs the displays bound before hold are a placement of them
    /// all, so the matching always finds one for them, and a later step
    /// only moves them. A display bound before whose new CRTC has no
    /// primary plane stays off, as one the scan cannot bind does, and the
    /// owner hears it removed. A new display the matching could not place
    /// is not bound, with an error, and the next scan tries it again.
    fn place(
        &mut self,
        res: &Resources,
        seen: &[Connector],
        new: Vec<New>,
        freed: &[(Identity, u32)],
        follow: bool,
        pin: bool,
    ) -> Placed {
        let index = |crtc: u32| res.crtcs.iter().position(|&c| c == crtc);
        let bit = |crtc: u32| index(crtc).map_or(0, index_bit);
        let olds = std::mem::take(&mut self.outputs);
        // Each CRTC a display gone in this scan held, until a new display
        // claims it.
        let mut freed: Vec<Option<&(Identity, u32)>> = freed.iter().map(Some).collect();
        let mut places = Vec::with_capacity(olds.len().saturating_add(new.len()));
        // The CRTC each display wants, and whether it is lit there.
        let mut homes = Vec::with_capacity(olds.len());
        for o in &olds {
            let conn = seen.iter().find(|c| c.id == o.id.0);
            let current = conn
                .filter(|c| c.encoder != 0)
                .and_then(|c| self.encoder(c.encoder));
            let reach = conn.map_or(0, |c| self.reach(c, current)) | bit(o.crtc);
            if !follow {
                let want = o.moved_from.filter(|_| o.online).map_or(o.crtc, |m| m.crtc);
                // Lit here means online. Only the displays online before the
                // modeset can go back ([`Self::going_back`]), so every display
                // whose record `going_back` reads is lit, and where two at a
                // hotplug would go back to one CRTC, connector order decides.
                homes.push((want, o.online));
                places.push(Place {
                    group: if o.online { 0 } else { 1 },
                    want: index(want),
                    able: if pin { bit(want) } else { reach | bit(want) },
                });
                continue;
            }
            let on = current.map_or(0, |e| e.crtc);
            let (group, want) = match on {
                _ if o.returning => (2, o.crtc),
                0 => (1, o.crtc),
                on => (0, on),
            };
            homes.push((want, group == 0));
            places.push(Place {
                group,
                want: index(want),
                able: reach | bit(on),
            });
        }
        for n in &new {
            let on = n.found.current.map_or(0, |e| e.crtc);
            let group = match on {
                _ if self.scanned => 2,
                0 => 1,
                _ => 0,
            };
            let able = self.reach(&n.conn, n.found.current) | bit(on);
            let back = freed
                .iter_mut()
                .find(|slot| {
                    slot.is_some_and(|&(ref was, crtc)| {
                        same_monitor(was, &n.identity) && able & bit(crtc) != 0
                    })
                })
                .and_then(Option::take)
                .map(|&(_, crtc)| crtc);
            places.push(Place {
                group,
                want: index(back.unwrap_or(on)),
                able,
            });
        }
        // The new displays the card refused even with lit displays moved
        // for them take part in the matching only after it, on the CRTCs
        // left free.
        let refused: Vec<usize> = new
            .iter()
            .enumerate()
            .filter(|(_, n)| self.move_refused.contains(&n.conn.id))
            .map(|(i, _)| i.saturating_add(olds.len()))
            .collect();
        let mut matched = places.clone();
        for &d in &refused {
            if let Some(p) = matched.get_mut(d) {
                p.able = 0;
            }
        }
        let mut slots = matching(&matched, res.crtcs.len());
        for &d in &refused {
            let Some(p) = places.get(d) else {
                continue;
            };
            let free = |c: &usize| p.able & index_bit(*c) != 0 && !slots.contains(&Some(*c));
            let take = p
                .want
                .filter(free)
                .or_else(|| (0..res.crtcs.len()).find(free));
            if let Some(slot) = slots.get_mut(d) {
                *slot = take;
            }
        }
        let mut slots = slots.into_iter();
        let at = |slot: Option<usize>| slot.and_then(|c| Some((*res.crtcs.get(c)?, c)));
        let mut changed = false;
        let mut unplaced = false;
        for ((mut o, slot), (want, lit)) in olds.into_iter().zip(slots.by_ref()).zip(homes) {
            // A display placed off the CRTC it wants is moved off it for
            // another display, until a modeset sets it on its new CRTC.
            o.moved_from = at(index(want))
                .filter(|&(_, pipe)| Some(pipe) != slot)
                .map(|(crtc, pipe)| Moved { crtc, pipe, lit });
            let placed = match at(slot) {
                None => Err(format!("no free CRTC for {}", o.name)),
                Some((crtc, _)) if crtc == o.crtc => {
                    self.outputs.push(o);
                    continue;
                }
                Some((crtc, pipe)) => self.move_to(&mut o, crtc, pipe),
            };
            changed = true;
            match placed {
                Ok(()) => self.outputs.push(o),
                Err(why) => self.let_go(o, &why),
            }
        }
        for (n, slot) in new.into_iter().zip(slots) {
            let New {
                conn,
                found,
                identity,
                choice,
            } = n;
            let Some((crtc, pipe)) = at(slot) else {
                log::error!("no free CRTC for {}", found.name);
                unplaced = true;
                continue;
            };
            if let Some(o) = self.bind(&conn, found, choice, crtc, pipe) {
                self.identities.retain(|(i, _)| *i != o.id);
                self.identities.push((o.id, identity));
                self.outputs.push(o);
                changed = true;
            }
        }
        self.outputs
            .sort_by_key(|o| res.connectors.iter().position(|&c| c == o.id.0));
        Placed { changed, unplaced }
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
    /// the displays online and the new ones cannot be set together, or the
    /// displays where the placement moved some of them, the new ones are
    /// left off, each display the placement moved goes back to the CRTC it
    /// was moved off where it can ([`Self::move_back`], [`Self::set_fewer`]),
    /// and the rest are set alone. A display whose new mode cannot be set
    /// stays at the mode it is set to: held back where the card refused the
    /// mode, and else owed it, which starts the wake-up retries.
    fn modeset(&mut self) -> io::Result<()> {
        // The legacy calls set each display on its own, so the displays set
        // are announced whatever failed for another. A moved display that
        // cannot be set on its new CRTC goes back, with each moved display
        // on the CRTC it goes back to; a display new to the modeset on one
        // of those CRTCs is left off, and the displays are set again. A
        // moved display set on its new CRTC stays there.
        let (set, unset) = if self.legacy {
            // The displays new to this modeset, which a moved display going
            // back may leave off.
            let fresh: Vec<DisplayId> = self
                .outputs
                .iter()
                .filter(|o| !o.online)
                .map(|o| o.id)
                .collect();
            let mut unset = self.legacy_modeset();
            if self.removed.get() {
                return Err(Errno::NODEV.into());
            }
            // The moved displays whose set failed. One new to the modeset
            // among them never goes back, as `going_back` leaves it out.
            let failed: Vec<(DisplayId, bool)> = unset
                .iter()
                .copied()
                .filter(|&(id, _)| {
                    self.outputs
                        .iter()
                        .any(|o| o.id == id && o.moved_from.is_some())
                })
                .collect();
            if !failed.is_empty() {
                let ids: Vec<DisplayId> = failed.iter().map(|&(id, _)| id).collect();
                let going = self.going_back(&fresh, Some(&ids), &[]);
                // The displays going back because the card refused a set,
                // not for a failure that may pass.
                let refused: Vec<DisplayId> = failed
                    .iter()
                    .filter(|&&(_, refused)| refused)
                    .map(|&(id, _)| id)
                    .collect();
                // Of the displays going back, those a refused one needs: the
                // refused ones that go back, then each on the CRTC one of
                // those goes back to.
                let refusal: Vec<DisplayId> = {
                    let outs: Vec<&Output<S::Memory>> = self
                        .outputs
                        .iter()
                        .filter(|o| !fresh.contains(&o.id))
                        .collect();
                    let goes: Vec<bool> = outs.iter().map(|o| going.contains(&o.id)).collect();
                    outs.iter()
                        .zip(needed(&outs, &goes, &refused))
                        .filter(|&(_, n)| n)
                        .map(|(o, _)| o.id)
                        .collect()
                };
                let back: Vec<u32> = self.returns(&going).map(|m| m.crtc).collect();
                let lit_back: Vec<u32> = self
                    .returns(&refusal)
                    .filter(|m| m.lit)
                    .map(|m| m.crtc)
                    .collect();
                let (new, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outputs)
                    .into_iter()
                    .partition(|o| fresh.contains(&o.id) && back.contains(&o.crtc));
                self.outputs = kept;
                let sat = self.leave_off(
                    new,
                    "a display moved to light it cannot be set on its new CRTC",
                );
                self.record_refused(&sat, &lit_back);
                self.move_back(&going);
                if !going.is_empty() {
                    unset = self.legacy_modeset();
                }
            }
            (Ok(()), unset.into_iter().map(|(id, _)| id).collect())
        } else {
            let mut set = self.atomic_modeset_or_default();
            if self.removed.get() {
                return Err(Errno::NODEV.into());
            }
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
            // The card refused the displays where they are placed, which a
            // display new to the modeset or one the placement moved may cause.
            let refusal = placement_refused(&set);
            let moved = self.outputs.iter().any(|o| o.moved_from.is_some());
            let set = match set.map_err(Failure::into_inner) {
                Err(e) if errno(&e) != Some(Errno::ACCESS) && (self.mixed() || moved) => {
                    self.set_fewer(&e, refusal).map_err(Failure::into_inner)
                }
                set => set,
            };
            (set, Vec::new())
        };
        self.unset = unset;
        if set.is_ok() {
            // Each display set is on its new CRTC for good.
            for o in self.outputs.iter_mut() {
                if !self.unset.contains(&o.id) {
                    o.moved_from = None;
                }
            }
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

    /// Lets go of display `o`, which a placement could not put on a CRTC
    /// for `why`: it stays off until a scan finds a CRTC for it, and the
    /// owner hears it removed.
    fn let_go(&mut self, o: Output<S::Memory>, why: &str) {
        log::warn!("{why}; it stays off until a scan finds a CRTC for it");
        if o.announced {
            self.events.push(Event::Removed(o.id));
        }
        self.identities.retain(|(i, _)| *i != o.id);
        self.free(o);
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

    /// Frees display `o`'s buffers, cursor and mode. Where a flip of its
    /// own is in flight, the CRTC it was committed on owes an event that no
    /// display waits for, bounded by a timer of its own ([`Self::stale`]).
    fn free(&mut self, mut o: Output<S::Memory>) {
        if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
            link.disarm(t);
        }
        if o.flipping {
            let timer = self
                .link
                .as_ref()
                .and_then(|l| l.arm(Clock::Stale(o.flip_crtc)));
            self.stale.push((o.flip_crtc, timer));
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
    /// kernel's console left there, because a driver can keep a returning
    /// panel dark under frames committed on the mode the console set at
    /// its return until its mode is set again, as amdgpu does. A
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
                self.seen(&e);
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

    /// Adds to `req` what takes each connector vtium does not bind off the
    /// CRTC a display takes, where it is on one: a connector the placement
    /// found no CRTC for, as a third monitor on a card with two CRTCs, or
    /// one left dark whose own commit the card refused. The placement knows
    /// only the displays' connectors, so a display can take a CRTC another
    /// connector is on, and two connectors on one CRTC are a clone, which
    /// the kernel refuses for encoders that cannot clone each other
    /// (`drm_atomic_check_valid_clones`). vtium does not read which
    /// encoders can clone, so it also takes off a connector whose encoder
    /// could clone with the display's: a mirror the card could have kept,
    /// such as two monitors a desktop left on one CRTC, ends, and that
    /// connector stays dark. Each connector's CRTC is read from its
    /// `CRTC_ID`. A connector whose properties cannot be read is left as it
    /// is, with a warning where its encoder shows it on a CRTC a display
    /// takes, as the card then refuses the commit. The legacy CRTC set
    /// takes every other connector off the CRTC it sets
    /// (`update_output_state`), so it needs none of this.
    fn take_others_off(&self, req: &mut Request) {
        let connectors = match self.card.resources() {
            Ok(r) => r.connectors,
            Err(e) => {
                log::warn!("cannot list the card's connectors: {e}");
                return;
            }
        };
        let taken = |crtc: u32| crtc != 0 && self.outputs.iter().any(|o| o.crtc == crtc);
        for id in connectors {
            if self.outputs.iter().any(|o| o.id.0 == id)
                || req.entries.iter().any(|&(object, _, _)| object == id)
            {
                continue;
            }
            let props = match self.card.properties(id, Object::Connector) {
                Ok(props) => props,
                Err(e) => {
                    match self.connector_crtc(id).filter(|&c| taken(c)) {
                        Some(crtc) => log::warn!(
                            "cannot take connector {id} off CRTC {crtc}, which a display takes: {e}"
                        ),
                        None => log::debug!("cannot read the properties of connector {id}: {e}"),
                    }
                    continue;
                }
            };
            let on = value(&props, "CRTC_ID").and_then(|c| u32::try_from(c).ok());
            let Some(crtc) = on.filter(|&c| taken(c)) else {
                continue;
            };
            if let Err(e) = Props::new(&props).set(req, id, "CRTC_ID", 0) {
                log::warn!(
                    "cannot take connector {id} off CRTC {crtc}, which a display takes: {e}"
                );
            }
        }
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
        self.take_others_off(&mut req);
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
            let req = self.with_color(&outputs, req)?;
            self.card
                .commit(
                    &req,
                    DRM_MODE_ATOMIC_ALLOW_MODESET
                        | DRM_MODE_ATOMIC_NONBLOCK
                        | DRM_MODE_PAGE_FLIP_EVENT,
                )
                .map_err(|e| {
                    self.seen(&e);
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

    /// A test-only commit of `req`, the card's refusal logged unless
    /// `quiet`.
    fn test_commit(&self, req: &Request, quiet: bool) -> Result<(), Failure> {
        self.card
            .commit(
                req,
                DRM_MODE_ATOMIC_TEST_ONLY | DRM_MODE_ATOMIC_ALLOW_MODESET,
            )
            .map_err(|e| {
                self.seen(&e);
                if quiet || card_removed(&e) {
                } else if errno(&e) == Some(Errno::ACCESS) {
                    // EACCES while another process holds master is expected.
                    log::debug!("test-only atomic commit refused: {e}");
                } else {
                    log::error!("test-only atomic commit failed: {e}");
                }
                Failure::Test(e)
            })
    }

    /// The modeset `req` of `outputs`, tested, with each connector's color
    /// properties set for sRGB at the bits per color vtium's buffers hold,
    /// where the connector has the properties and the card takes them
    /// (ADR-1401 D14). A desktop with HDR on leaves its connector in BT.2020
    /// with HDR metadata and more bits per color than vtium draws, and the
    /// monitor would read vtium's 8-bit sRGB frames as that
    /// (vtium/vtium#253).
    ///
    /// Where the card refuses the modeset with every connector's color
    /// properties and takes it without them, vtium adds each connector's
    /// back in connector order, as many as the card takes beside those
    /// already added: all three, else all but `max bpc`, else none. So one
    /// connector the card refuses costs no other connector its reset, and
    /// the warning names the connector that refused. A connector left
    /// without some of its properties keeps them as vtium found them, and
    /// the display is set all the same.
    fn with_color(&self, outputs: &[Output<S::Memory>], req: Request) -> Result<Request, Failure> {
        let bpc = bits_per_color(FORMAT);
        let sets: Vec<_> = outputs
            .iter()
            .map(|o| (o, o.color.sdr(bpc)))
            .filter(|(_, set)| !set.is_empty())
            .collect();
        let with = |req: &Request, o: &Output<S::Memory>, set: &[(u32, &str, u64)]| {
            let mut req = req.clone();
            for &(id, _, value) in set {
                req.set(o.id.0, id, value);
            }
            req
        };
        if sets.is_empty() {
            self.test_commit(&req, false)?;
            return Ok(req);
        }
        let all = sets
            .iter()
            .fold(req.clone(), |all, (o, set)| with(&all, o, set));
        if self.test_commit(&all, true).is_ok() {
            return Ok(all);
        }
        self.test_commit(&req, false)?;
        let mut kept = req;
        for (o, set) in sets {
            let without_bpc: Vec<_> = set
                .iter()
                .filter(|&&(_, name, _)| name != "max bpc")
                .copied()
                .collect();
            let mut refusal = None;
            let mut steps = vec![(set.as_slice(), None)];
            if without_bpc.len() != set.len() && !without_bpc.is_empty() {
                steps.push((without_bpc.as_slice(), Some("max bpc")));
            }
            let mut taken = false;
            for (step, left) in steps {
                let next = with(&kept, o, step);
                match self.test_commit(&next, true) {
                    Ok(()) => {
                        kept = next;
                        taken = true;
                        if let (Some(left), Some(e)) = (left, &refusal) {
                            self.color_refused(&o.name, Some(left), e);
                        }
                        break;
                    }
                    Err(Failure::Test(e) | Failure::Other(e)) => {
                        refusal.get_or_insert(e);
                    }
                }
            }
            if let (false, Some(e)) = (taken, &refusal) {
                self.color_refused(&o.name, None, e);
            }
        }
        Ok(kept)
    }

    /// Logs that the card refused to set color property `property` of
    /// connector `name`, or every one of its color properties where `None`,
    /// which stay as vtium found them (ADR-1401 D14): a warning the first
    /// time for that connector, and at debug level after that.
    fn color_refused(&self, name: &str, property: Option<&str>, e: &io::Error) {
        let line = match property {
            None => format!(
                "cannot set the color properties of {name} for sRGB: {e}; its color properties stay as vtium found them"
            ),
            Some(p) => format!(
                "cannot set `{p}` of {name} for sRGB: {e}; its `{p}` stays as vtium found it"
            ),
        };
        let mut warned = self.color_refused.borrow_mut();
        if warned.iter().any(|w| w == name) {
            log::debug!("{line}");
        } else {
            warned.push(name.to_owned());
            log::warn!("{line}");
        }
    }

    /// The legacy modeset: each CRTC in turn, with no flip event. A
    /// display whose new mode cannot be set is set at the mode it is set
    /// to: held back where the card refused the mode with `EINVAL`, and
    /// else owed it. Returns the displays it could not set, each with
    /// whether the card refused it (`EINVAL`).
    fn legacy_modeset(&mut self) -> Vec<(DisplayId, bool)> {
        self.legacy_dark();
        let mut unset = Vec::new();
        let mut outputs = std::mem::take(&mut self.outputs);
        let driven: Vec<u32> = outputs
            .iter()
            .filter(|o| !o.returning)
            .map(|o| o.crtc)
            .collect();
        for o in &mut outputs {
            if self.removed.get() {
                break;
            }
            let mut set = self.legacy_set(o, &driven);
            let again = !self.removed.get()
                && match &set {
                    Ok(()) => false,
                    Err(Failure::Test(_)) => o.hold_back(),
                    Err(Failure::Other(_)) => o.keep_set_mode(),
                };
            if again {
                set = self.legacy_set(o, &driven);
            }
            if let Err(failure) = set {
                unset.push((o.id, matches!(failure, Failure::Test(_))));
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
        // The legacy calls set the color properties one at a time, before
        // the CRTC, as they set DPMS. Only atomic drivers attach these
        // properties, and on one the kernel commits each set as a modeset of
        // its own where it changes `max bpc` on a lit CRTC, and on amdgpu
        // also where it changes `Colorspace` or turns the HDR metadata on or
        // off (`amdgpu_dm_connector_atomic_check`); `drm_atomic_commit_init`
        // allows legacy commits to modeset.
        for (id, name, value) in o.color.sdr(bits_per_color(FORMAT)) {
            if let Err(e) = self.card.set_connector_property(o.id.0, id, value) {
                self.color_refused(&o.name, Some(name), &e);
            }
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
                self.seen(&e);
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

/// Whether `now` names the monitor `was` named. Where both connectors have
/// a `PATH`, as a DisplayPort multi-stream port keeps when its connector is
/// made again under a new id, the two must name the same port; two
/// monitors of one model with no serial number in their EDIDs read the
/// same EDID, so the port decides. Where neither has a `PATH`, the two
/// must read the same EDID. Where only one has a `PATH`, they are not the
/// same monitor.
fn same_monitor(was: &Identity, now: &Identity) -> bool {
    match (&was.connector_path, &now.connector_path) {
        (Some(was), Some(now)) => was == now,
        (None, None) => was.edid.is_some() && was.edid == now.edid,
        _ => false,
    }
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
/// as refused while another process holds DRM master or as a card the
/// kernel removed.
fn refused(set: &Result<(), Failure>) -> bool {
    matches!(set, Err(Failure::Test(e)) if !matches!(errno(e), Some(Errno::ACCESS | Errno::NODEV)))
}

/// Whether the card refused the displays where they are placed, as past its
/// bandwidth, and not for a reason that may pass: its test commit failed
/// with `EINVAL` or `ENOSPC`.
fn placement_refused(set: &Result<(), Failure>) -> bool {
    matches!(
        set,
        Err(Failure::Test(e)) if matches!(errno(e), Some(Errno::INVAL | Errno::NOSPC))
    )
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
        for (_, timer) in std::mem::take(&mut self.stale) {
            if let (Some(link), Some(t)) = (&self.link, timer) {
                link.disarm(t);
            }
        }
    }
}
#[cfg(test)]
mod tests;
