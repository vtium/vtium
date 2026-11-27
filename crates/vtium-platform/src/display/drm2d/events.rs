//! The card's events: the poll for connectors plugged in or out, the
//! completion events of flips, and the timers that bound how long a flip
//! and a lost event may hold the displays up.

use std::io;

use super::super::Event;
use super::super::kms::{CrtcEvent, Kms};
use super::buffers::Source;
use super::source::Clock;
use super::{Drm2d, Scan, Waiting, stopped};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
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
        scanned.or_else(stopped)
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
    pub(super) fn resolve(&mut self) {
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
    /// retries, fast and slow.
    pub(super) fn settle(&mut self) {
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
        let slow = self.retrying_slowly();
        let token = &mut self.timers.slow_retry;
        match (slow, token.take()) {
            (true, None) => *token = link.arm(Clock::SlowRetry),
            (false, Some(t)) => link.disarm(t),
            (_, t) => *token = t,
        }
    }
}
