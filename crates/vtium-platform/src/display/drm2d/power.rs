//! A display's power: its DPMS state, the blank that input ends, and the
//! retries of a failed wake-up or modeset that input starts.

use std::io;

use super::super::kms::Kms;
use super::super::{DisplayId, Dpms, Event};
use super::buffers::Source;
use super::output::{dpms_value, invalid};
use super::{Drm2d, Scan};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
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
}
