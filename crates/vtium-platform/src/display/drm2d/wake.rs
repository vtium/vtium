//! Waking the card and letting it go: DRM master taken and dropped, the
//! wake-up and its retries while another process holds master or the
//! setup fails, and the sleep that blanks the displays, waits for the
//! flips in flight and hides the planes before master goes.

use std::io;

use drm_ffi::DRM_MODE_ATOMIC_NONBLOCK;
use rustix::io::Errno;

use super::super::kms::{Kms, Object, Request};
use super::super::{Ages, Event};
use super::buffers::Source;
use super::output::{PLANE_CURSOR, PLANE_OVERLAY, Props, value};
use super::source::Clock;
use super::{Drm2d, Master, Scan, Waiting, card_removed, stopped};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Takes DRM master: through the keeper where there is one, and
    /// otherwise with the card's own call.
    fn take_master(&self) -> io::Result<()> {
        #[cfg(feature = "split")]
        if let Some(keeper) = &self.keeper {
            return keeper.take();
        }
        self.card.set_master()
    }

    /// Lets DRM master go: through the keeper where there is one, and
    /// otherwise with the card's own call.
    fn drop_master(&self) -> io::Result<()> {
        #[cfg(feature = "split")]
        if let Some(keeper) = &self.keeper {
            return keeper.let_go();
        }
        self.card.drop_master()
    }

    /// Takes DRM master and sets every display up again. While another
    /// process holds master, or the setup fails, the wake-up is tried
    /// again every [`crate::WAKE_RETRY_MS`], at most
    /// [`crate::WAKE_RETRIES`] times, and then every
    /// [`crate::WAKE_RETRY_SLOW_MS`] while the VT is shown
    /// (`retry_slowly`).
    pub fn wake_up(&mut self) -> io::Result<()> {
        if self.gone {
            return Err(Errno::NODEV.into());
        }
        self.reap();
        // A sleep still waiting for a flip is cancelled: the card keeps the
        // master it holds, and the wake-up sets every display up again once
        // the flip ends (ADR-1401 D16).
        if self.waiting == Some(Waiting::Sleep) {
            self.waiting = None;
        }
        self.foreground = true;
        self.retries = None;
        let woken = self.try_wake(Scan::WakeUp);
        if woken.is_err() {
            self.retry(true);
        }
        self.settle();
        // A wake-up the kernel's request for the VT stopped is no failure:
        // the sleep that follows ends its retries, and the acquire's
        // wake-up does the work again.
        woken.or_else(stopped)
    }

    /// Starts the retries, unless they run: a wake-up's when `wake`.
    pub(super) fn retry(&mut self, wake: bool) {
        self.waking = wake || (self.waking && self.retries.is_some());
        self.retries.get_or_insert(0);
    }

    /// Takes DRM master and sets every display up again: as a wake-up,
    /// which reports [`Event::Awake`], or as a retry, which does not. A
    /// failed wake-up lets master go, as another process may hold the
    /// displays; a failed retry keeps master and the displays online, as
    /// the atomic modeset left them, so the blank still powers them down.
    fn try_wake(&mut self, scan: Scan) -> io::Result<()> {
        // A keeper drops master on its own copy at a release, which this
        // file learns only from the kernel.
        #[cfg(feature = "split")]
        if self.keeper.is_some() {
            self.recheck_master();
        }
        if self.master != Master::Held {
            // After the fast retries ran out, each later try that fails
            // logs at debug level, so the slow retries do not fill the log.
            let level = if self.wake_failed {
                log::Level::Debug
            } else {
                log::Level::Warn
            };
            self.take_master().inspect_err(|e| {
                if !card_removed(e) {
                    log::log!(level, "cannot take DRM master: {e}");
                }
            })?;
            self.master = Master::Held;
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
                log::error!(
                    "cannot wake the display after {tried} retries; the card tries again every {} ms while its VT is shown",
                    crate::WAKE_RETRY_SLOW_MS
                );
                self.wake_failed = true;
                self.events.push(Event::WakeFailed);
            }
        } else {
            self.retries = Some(tried.saturating_add(1));
        }
        self.settle();
    }

    /// Whether the slow retries run: the fast retries ran out with
    /// [`Event::WakeFailed`], nothing has set the displays since, and the
    /// card's VT is shown.
    pub(super) fn retrying_slowly(&self) -> bool {
        self.wake_failed && self.foreground && self.retries.is_none() && !self.gone
    }

    /// The slow retry's timer fired. After [`Event::WakeFailed`], while
    /// its VT is shown, the card tries once every
    /// [`crate::WAKE_RETRY_SLOW_MS`]: the wake-up where it is asleep, or
    /// else a modeset of every display. A try that fails leaves the next
    /// one to the timer, and starts no fast retries. Blanked, an awake
    /// card's scan commits nothing: it leaves the modeset owed to the input
    /// that ends the blank, and the slow retries stop.
    pub(crate) fn retry_slowly(&mut self) {
        if !self.retrying_slowly() {
            return;
        }
        let tried = if self.awake {
            self.scan(Scan::Retry)
        } else {
            self.try_wake(Scan::WakeUp)
        };
        if tried.is_ok() {
            self.wake_failed = false;
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
        self.leave_modeset_flips();
        self.waiting = Some(Waiting::Sleep);
        self.resolve();
        self.settle();
    }

    /// Leaves each modeset's flip in flight to the kernel, so the sleep does
    /// not wait for it: the flip counts as done, and its event, when it
    /// comes, is dropped ([`Self::stale`]). The program that takes the card
    /// may find that display busy for a commit that does not block until
    /// the modeset ends, and a commit that blocks waits for it in the
    /// kernel (ADR-1401 D2, "A VT release while display work runs").
    fn leave_modeset_flips(&mut self) {
        for o in &mut self.outputs {
            if !(o.flipping && o.modeset_flip_no == Some(o.flips)) {
                continue;
            }
            if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
                link.disarm(t);
            }
            let timer = self
                .link
                .as_ref()
                .and_then(|l| l.arm(Clock::Stale(o.flip_crtc)));
            self.stale.push((o.flip_crtc, timer));
            o.flipping = false;
            o.modeset_flip = false;
            o.vblank = false;
            log::debug!(
                "display {}: the sleep leaves its modeset's flip to the kernel",
                o.name
            );
        }
    }

    /// Ends the sleep once no flip is in flight: hides the planes that show
    /// something, under the master the commit needs, and drops DRM master.
    pub(super) fn finish_sleep(&mut self) {
        self.recheck_master();
        let hide = self.planes_to_hide();
        if !hide.entries.is_empty()
            && let Err(e) = self.card.commit(&hide, DRM_MODE_ATOMIC_NONBLOCK)
            && !card_removed(&e)
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
        if self.master == Master::NotHeld || self.legacy {
            return req;
        }
        let all = self.card.planes().unwrap_or_else(|e| {
            if !card_removed(&e) {
                log::warn!("cannot list the card's planes: {e}");
            }
            Vec::new()
        });
        for plane in all {
            let props = match self.card.properties(plane, Object::Plane) {
                Ok(props) => props,
                Err(e) if card_removed(&e) => {
                    return Request::default();
                }
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

    /// Lets the card go once an ioctl said the kernel removed it, as
    /// udev's report of its removal has the owner let it go, but at once:
    /// udev reports the removal only after udevd has handled it, and a
    /// card removed this early in boot may have no udevd to report it
    /// (vtium/vtium#295). Each display is reported removed, a sleep under
    /// way reports [`Event::Asleep`], and every wait, retry and timer ends.
    /// The displays' buffers, cursors and mode blobs go with no ioctl, as
    /// the kernel frees them when the card's file closes, and drm2d sends
    /// the card nothing again ([`Drm2d::gone`]).
    pub(super) fn let_card_go(&mut self) {
        if self.gone || !self.card.removed() {
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
        // Each display gets one event from here, its removal: a display
        // left off in the same operation, whose Unlit the owner has not read
        // yet, is reported removed alone.
        self.events.retain(|e| !matches!(e, Event::Unlit(_)));
        for l in std::mem::take(&mut self.left_off) {
            self.events.push(Event::Removed(l.id));
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
        self.master = Master::NotHeld;
        self.retries = None;
        self.owed = false;
        self.wake_failed = false;
    }

    /// Lets go of the DRM master this file holds. A file without master
    /// has nothing to let go, and the kernel would refuse the drop with
    /// `EINVAL` (`drm_dropmaster_ioctl`).
    fn release_master(&mut self) {
        // The kernel removed the card, and its master with it.
        if self.card.removed() {
            self.master = Master::NotHeld;
            return;
        }
        self.recheck_master();
        if self.master != Master::NotHeld
            && let Err(e) = self.drop_master()
        {
            log::debug!("cannot drop DRM master: {e}");
        }
        self.master = Master::NotHeld;
    }

    /// Asks the kernel whether this file still holds the DRM master that
    /// drm2d took. A seat manager shares the open file with vtium and drops
    /// master on it before it tells vtium to let the card go, as seatd
    /// does when the VT is switched away (`seat_disable_client`). Without
    /// master, the sleep then commits nothing and drops nothing. When the
    /// kernel gives no answer, vtium keeps what it knew: a master it held
    /// still counts as held.
    fn recheck_master(&mut self) {
        if self.master == Master::NotHeld || self.card.removed() {
            return;
        }
        match self.card.is_master() {
            Ok(held) => self.master = if held { Master::Held } else { Master::NotHeld },
            // The kernel removed the card, and its master with it.
            Err(_) if self.card.removed() => self.master = Master::NotHeld,
            Err(e) => log::debug!("cannot ask whether vtium holds DRM master: {e}"),
        }
    }
}
