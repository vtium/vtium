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
//!
//! In a build with `scroll-pan`, the owner can ask a display for buffers
//! taller than the screen (`Drm2d::set_tall`). The display then shows a
//! window of each buffer as large as the screen, and each frame's commit
//! sets where that window starts through the primary plane's source
//! rectangle. The display tests the window at its furthest start with a
//! test-only commit when it makes such buffers, and keeps buffers the
//! screen's size where the card refuses them.

mod atomic;
mod back;
mod buffers;
mod cursor;
mod dark;
mod events;
mod frames;
mod left_off;
mod legacy_search;
mod matching;
mod modeset;
mod output;

pub use output::CURSOR_MAX;
mod power;
mod scan;
mod source;
mod staging;
mod wake;

use std::cell::RefCell;
use std::io;

use calloop::RegistrationToken;
use drm_ffi::{
    DRM_CAP_DUMB_BUFFER, DRM_CLIENT_CAP_ATOMIC, DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT,
    DRM_CLIENT_CAP_UNIVERSAL_PLANES,
};
use rustix::io::Errno;

#[cfg(feature = "scroll-pan")]
use super::Tall;
#[cfg(doc)]
use super::Token;
use super::edid::Edid;
use super::kms::{Connector, Encoder, Kms, Property};
use super::mode::{Choice, Mode, ModePolicy};
use super::{Buffers, DarkPolicy, Density, DisplayId, Dpms, Event, Identity, Interrupt};
#[cfg(feature = "split")]
use crate::vt::MasterKeeper;
use buffers::{Buffer, Dumbs, Source};
use dark::Dark;
use output::{Output, invalid};
use source::{Link, Timers};

fn errno(e: &io::Error) -> Option<Errno> {
    e.raw_os_error().map(Errno::from_raw_os_error)
}

/// The error of display work that the kernel's request for the VT stopped.
fn released() -> io::Error {
    io::Error::new(
        io::ErrorKind::Interrupted,
        "the kernel asked for the VT, so the display work stopped",
    )
}

/// A result whose error is [`released`] counts as done: the release
/// that follows ends the work's retries.
fn stopped(e: io::Error) -> io::Result<()> {
    if e.kind() == io::ErrorKind::Interrupted {
        Ok(())
    } else {
        Err(e)
    }
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

/// Whether this open file of the card holds its DRM master, as far as
/// vtium knows. The kernel answers the question through an AUTH_MAGIC
/// call, and that call can fail for another reason than the answer, so
/// vtium sometimes cannot tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Master {
    Held,
    NotHeld,
    /// The kernel gave no answer, so vtium acts as though the file may or
    /// may not hold master. A sleep tries to drop master, as libdrm's
    /// `drmIsMaster` counts every answer but a refusal as master. A
    /// wake-up still calls SET_MASTER, which the kernel answers with
    /// success on the file that is already the master
    /// (`drm_setmaster_ioctl`).
    Unknown,
}

impl Master {
    /// What the kernel's answer says: unknown when the question failed.
    fn from_answer(answer: io::Result<bool>) -> Master {
        match answer {
            Ok(true) => Master::Held,
            Ok(false) => Master::NotHeld,
            Err(e) => {
                log::debug!("cannot ask whether vtium holds DRM master: {e}");
                Master::Unknown
            }
        }
    }
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

/// What a modeset staged for one display: its new buffers, where its mode
/// needs them, and its mode's blob.
#[cfg(feature = "scroll-pan")]
type Staged<M> = (Option<Vec<Buffer<M>>>, u32);

/// A DRM card's displays, drawn into the buffers `S` makes, dumb buffers
/// by default. Outside the crate the card is a [`Card`](crate::display::Card); the tests stand
/// one in.
#[expect(private_bounds, reason = "the card is a Card outside the crate")]
pub struct Drm2d<K: Kms, S: Source<K> = Dumbs> {
    card: K,
    source: S,
    /// The buffers each display keeps.
    buffers: Buffers,
    /// The widest and the tallest framebuffer the card makes, as its
    /// resources reported them at the last scan.
    #[cfg(feature = "scroll-pan")]
    max_fb: (u32, u32),
    /// Buffers a display let go of while a token for them was out, as at a
    /// modeset that gave it new ones: each stays allocated and mapped until
    /// its token comes back, and is freed then.
    retired: RefCell<Vec<Buffer<S::Memory>>>,
    /// The levels of the 1-bit or gray panel the card drives.
    levels: Option<u8>,
    /// No universal planes or no atomic commits: the legacy calls.
    legacy: bool,
    hotspot: bool,
    master: Master,
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
    /// The card's CRTCs, in its order, as the last scan read them.
    crtcs: Vec<u32>,
    /// The identity of each display bound.
    identities: Vec<(DisplayId, Identity)>,
    /// The connected connectors the policy leaves dark, in the card's
    /// connector order.
    dark: Vec<Dark>,
    /// A modeset could not set the dark connectors off beside the lit
    /// displays, and that was logged as a warning.
    dark_refused: bool,
    /// The displays left off while their monitors are still plugged in,
    /// which keep their ids with no CRTC until a placement lights them
    /// ([`left_off::LeftOff`]).
    left_off: Vec<left_off::LeftOff>,
    /// The connectors whose display the card refused to set beside the
    /// others even with lit displays moved for it. Until such a connector
    /// is unplugged or a modeset lights its display, a hotplug moves no lit
    /// display for it and no search tries it again, so the lit displays do
    /// not blank at each hotplug for a display that cannot light.
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
    /// the displays since: the next input tries again, and so does each
    /// slow retry while the VT is shown ([`Self::retry_slowly`]).
    wake_failed: bool,
    /// The displays the last legacy modeset could not set, at the mode
    /// chosen or at the mode they were set to: each whose set failed for a
    /// reason that may pass, or every display, where the card refused them
    /// all. A display the card refused beside others that lit is left off
    /// instead. The wake-up retries try these again, and none is asked to
    /// redraw until a modeset sets it.
    unset: Vec<DisplayId>,
    /// The card was let go after the kernel removed it: each display was
    /// reported removed, and drm2d sends the card nothing again.
    gone: bool,
    events: Vec<Event>,
    timers: Timers,
    link: Option<Box<dyn Link>>,
    /// Whether the kernel has asked for the VT, asked between the steps
    /// of a scan and before a modeset ([`Drm2d::set_interrupt`]).
    interrupt: Option<Interrupt>,
    /// Who sets and drops DRM master on the card, where vtium may not make
    /// those calls itself ([`Drm2d::set_master_keeper`]).
    #[cfg(feature = "split")]
    keeper: Option<Box<dyn MasterKeeper>>,
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
        // The open made this file the card's master where no process held
        // master, and left it without master where another process held
        // it.
        let master = Master::from_answer(card.is_master());
        Ok(Drm2d {
            card,
            source: Dumbs,
            buffers: Buffers::default(),
            #[cfg(feature = "scroll-pan")]
            max_fb: (0, 0),
            retired: RefCell::new(Vec::new()),
            levels,
            legacy,
            hotspot,
            master,
            foreground: false,
            awake: false,
            policy,
            card_path: None,
            dark_policy: None,
            scanned: false,
            outputs: Vec::new(),
            crtcs: Vec::new(),
            identities: Vec::new(),
            dark: Vec::new(),
            dark_refused: false,
            left_off: Vec::new(),
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
            gone: false,
            events: Vec::new(),
            timers: Timers::default(),
            link: None,
            interrupt: None,
            #[cfg(feature = "split")]
            keeper: None,
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

    /// Gives the card a keeper of its DRM master, as the drawer's card has
    /// under the privilege split. From then on the card never sets or
    /// drops master itself: each wake-up, the slow retry included, asks the
    /// keeper, and so does each let-go. The kernel refuses the drawer's own
    /// `DRM_IOCTL_SET_MASTER` even while its open file holds master, so
    /// before it asks, a wake-up asks the kernel whether the file still
    /// holds master, as the broker drops it at each release. Commits, flips
    /// and pans need only that the file holds master, so they run as with
    /// a master vtium took itself.
    ///
    /// References:
    /// - ADR-1400 D4, "What the broker does": the broker holds DRM master
    ///   for the drawer, and the drawer asks at each try of its wake-up.
    #[cfg(feature = "split")]
    pub fn set_master_keeper(&mut self, keeper: Option<Box<dyn MasterKeeper>>) {
        self.keeper = keeper;
    }

    /// Gives the card the loop's check of whether the kernel has asked for
    /// the VT. The card asks it before each connector read of a scan and
    /// before a modeset, and stops its display work where it answers yes
    /// (ADR-1401 D2, "A VT release while display work runs").
    pub fn set_interrupt(&mut self, interrupt: Interrupt) {
        self.interrupt = Some(interrupt);
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

    /// Parks the display `id`: drm2d sets its connector off and turns off
    /// the CRTC it leaves, frees its buffers, its cursor and its mode, and
    /// reports it removed. The display then stays dark, as one the policy
    /// left dark at a scan: [`Self::dark`] lists it, a scan keeps it dark,
    /// and [`Self::release_dark`] lets the next scan ask the policy of it
    /// again, which lights it with new buffers. vtium parks a display that
    /// shows only a notice once the notice's time is up, so the display
    /// sends no signal and holds no memory. The card must be awake.
    pub fn park(&mut self, id: DisplayId) -> io::Result<()> {
        if !self.awake {
            return Err(invalid("the card is asleep"));
        }
        let Some(at) = self.outputs.iter().position(|o| o.id == id && o.announced) else {
            return Err(invalid("the display is not online"));
        };
        let Some(identity) = self
            .identities
            .iter()
            .find_map(|(i, identity)| (*i == id).then(|| identity.clone()))
        else {
            return Err(invalid("the display has no identity"));
        };
        let o = self.outputs.remove(at);
        log::info!("display {} is parked: set off, its buffers freed", o.name);
        self.identities.retain(|(i, _)| *i != id);
        self.events.push(Event::Removed(id));
        self.free(o);
        self.dark.push(Dark {
            id: id.0,
            identity,
            available: true,
        });
        if self.outputs.is_empty() {
            self.darken();
            Ok(())
        } else {
            self.request_modeset(false)
        }
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

    /// The connector's name, as `HDMI-A-1`, of a display online or left
    /// off.
    pub fn name(&self, id: DisplayId) -> Option<&str> {
        self.output(id)
            .map(|o| o.name.as_str())
            .or_else(|| self.left_off(id).map(|l| l.name.as_str()))
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

    /// The buffers each display keeps.
    pub fn buffers(&self) -> Buffers {
        self.buffers
    }

    /// Asks display `id` for buffers taller than its screen, or, with
    /// `None`, for buffers the screen's size again. Where the buffers it
    /// holds differ from those asked for, a modeset makes new ones, and
    /// [`Event::Redraw`] follows; while the displays are blanked, the input
    /// that ends the blank runs that modeset. Each new ask is tried afresh:
    /// the display tests the pan when it makes the tall buffers, and keeps
    /// buffers the screen's size, logged once, where the card refuses them
    /// ([`Drm2d::pans`]). A display the card drives through the legacy
    /// calls never pans.
    #[cfg(feature = "scroll-pan")]
    pub fn set_tall(&mut self, id: DisplayId, tall: Option<Tall>) -> io::Result<()> {
        let awake = self.awake;
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) else {
            return Ok(());
        };
        if o.pan.asked == tall {
            return Ok(());
        }
        o.pan.asked = tall;
        o.pan.refused = false;
        o.pan.proven = None;
        if tall.is_some() && o.planes.is_none() {
            Self::refuse_pan(
                o,
                "the card has only the legacy calls, which cannot move the window without a modeset",
            );
        }
        let stale = o.online && o.buffers.first().is_some_and(|b| b.size != o.buffer_size());
        if !(stale && awake) {
            return Ok(());
        }
        if self.blanked {
            self.owed = true;
            return Ok(());
        }
        self.request_modeset(false)
    }

    /// Whether display `id` holds buffers taller than its screen and shows
    /// a window of them: each token it lends then reports the buffer's size
    /// and the screen's apart, and carries the window.
    #[cfg(feature = "scroll-pan")]
    pub fn pans(&self, id: DisplayId) -> bool {
        self.output(id).is_some_and(Output::holds_tall)
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
    /// gray one, which the card's driver says (`depth`); `None`
    /// for a color display.
    pub fn levels(&self, id: DisplayId) -> Option<u8> {
        self.levels.filter(|_| self.output(id).is_some())
    }

    /// The bits a pixel of the display's buffers holds: 32, since every
    /// dumb buffer vtium makes is XRGB8888. `None` for a display that is
    /// not online.
    pub fn buffer_bits(&self, id: DisplayId) -> Option<u8> {
        self.output(id).map(|_| 32)
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

    #[cfg(test)]
    pub(crate) fn card(&self) -> &K {
        &self.card
    }

    #[cfg(test)]
    pub(crate) fn waits(&self) -> (bool, bool) {
        (self.waiting.is_some(), self.retries.is_some())
    }

    /// Whether the slow retry's timer is armed in the loop.
    #[cfg(test)]
    pub(crate) fn slow_armed(&self) -> bool {
        self.timers.slow_retry.is_some()
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
    if let Some(c) = o.cursor.take()
        && !card.removed()
    {
        for (dumb, fb) in std::iter::once((c.dumb, c.fb)).chain(c.spare) {
            let _ = card.rm_fb(fb);
            let _ = card.destroy_dumb(dumb.handle);
        }
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
        if let Some(link) = &self.link {
            for token in [self.timers.wake_retry.take(), self.timers.slow_retry.take()]
                .into_iter()
                .flatten()
            {
                link.disarm(token);
            }
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
#[cfg(all(test, feature = "split"))]
mod keeper_tests;
#[cfg(test)]
mod pan_tests;
#[cfg(test)]
mod tests;
