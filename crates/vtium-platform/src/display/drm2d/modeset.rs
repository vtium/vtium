//! Setting the displays' modes: the modeset that sets every display and
//! announces each one set, the displays held back or owed a mode, and the
//! legacy modeset of a card without atomic commits.

use std::io;

use rustix::io::Errno;

use super::super::kms::Kms;
use super::super::mode::Mode;
use super::super::{DisplayId, Dpms, Event};
use super::back::needed;
use super::buffers::{FORMAT, Source};
use super::output::{Output, bits_per_color};
use super::{Drm2d, Failure, Waiting, card_removed, errno, placement_refused, refused, released};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// A modeset now, or once no flip is in flight: a wake-up's when
    /// `wake`.
    pub(super) fn request_modeset(&mut self, wake: bool) -> io::Result<()> {
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
    pub(super) fn modeset(&mut self) -> io::Result<()> {
        if self.interrupted() {
            return Err(released());
        }
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
            if self.card.removed() {
                return Err(Errno::NODEV.into());
            }
            // The card refused displays where they are placed: another
            // placement of them all, one that moves a lit display included,
            // may light them ([`Self::legacy_relight`]).
            unset = self.legacy_relight(unset);
            if self.card.removed() {
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
            if self.card.removed() {
                return Err(Errno::NODEV.into());
            }
            // The legacy calls say which display the card refused. Each one
            // still refused here is left off, present but dark, so the
            // retries do not set it again; a display whose set failed for a
            // reason that may pass stays bound and unset, for the retries.
            // Where no display is set, every one stays, and the retries try
            // them again, as they try an atomic modeset the card refuses
            // whole.
            let refused: Vec<DisplayId> = unset.iter().filter(|u| u.1).map(|u| u.0).collect();
            let set_any = self
                .outputs
                .iter()
                .any(|o| !unset.iter().any(|u| u.0 == o.id));
            if !refused.is_empty() && set_any {
                let (off, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outputs)
                    .into_iter()
                    .partition(|o| refused.contains(&o.id));
                self.outputs = kept;
                self.leave_off(off, "the card refuses it beside the other displays");
                unset.retain(|u| !u.1);
            }
            (Ok(()), unset.into_iter().map(|(id, _)| id).collect())
        } else {
            let mut set = self.atomic_modeset_or_default(true);
            // A display whose new mode the card refuses, as its default
            // too, stays at the mode it is set to, and the modeset is tried
            // again, so the others are set and nothing is retried.
            if refused(&set) && self.hold_back() {
                set = self.atomic_modeset_or_default(true);
            }
            // A display whose new mode failed for another reason, as where
            // its buffers cannot be made, stays at the mode it is set to,
            // which needs no new buffers, and the modeset is tried again,
            // so the card is set and stays awake while the retries try the
            // new mode.
            if set.is_err() && self.keep_set_modes() {
                set = self.atomic_modeset_or_default(true);
            }
            // The card refused the displays where they are placed: another
            // placement of them all, one that moves a lit display included,
            // may light them.
            if placement_refused(&set) && self.relight() {
                set = Ok(());
            }
            // The kernel removed the card: no fewer displays are tried.
            if self.card.removed() {
                return Err(Errno::NODEV.into());
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
            // Each display set is on its new CRTC for good, and lit, so no
            // record of the card refusing it is needed any more.
            for o in self.outputs.iter_mut() {
                if !self.unset.contains(&o.id) {
                    o.moved_from = None;
                }
            }
            let lit: Vec<u32> = self
                .outputs
                .iter()
                .filter(|o| !self.unset.contains(&o.id))
                .map(|o| o.id.0)
                .collect();
            self.move_refused.retain(|id| !lit.contains(id));
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

    /// Turns off `crtc`, which display `name` held until it went, on a card
    /// driven through the legacy calls, where no display left takes the
    /// CRTC and it still shows a mode. Removing the framebuffers of the
    /// gone display turns its CRTC off on some drivers, such as amdgpu.
    /// Others, such as i915, keep the CRTC lit with the gone connector on
    /// it, which holds a multi-stream connector's share of the link. The
    /// legacy call that turns the CRTC off takes every connector off it.
    pub(super) fn legacy_gone_off(&self, crtc: u32, name: &str) {
        if !self.legacy
            || self.card.removed()
            || self.outputs.iter().any(|o| o.crtc == crtc)
            || !matches!(self.card.crtc_mode(crtc), Ok(Some(_)))
        {
            return;
        }
        if let Err(e) = self.card.disable_crtc(crtc) {
            log::warn!("cannot turn off the CRTC that display {name} held: {e}");
        }
    }

    /// Lets go of display `o`, which a placement could not put on a CRTC
    /// for `why`: it stays off until a scan finds a CRTC for it, present
    /// but dark ([`Self::keep_left_off`]).
    pub(super) fn let_go(&mut self, o: Output<S::Memory>, why: &str) {
        log::warn!("{why}; it stays off until a scan finds a CRTC for it");
        self.keep_left_off(&o);
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
    pub(super) fn owing(&self) -> bool {
        self.outputs.iter().any(|o| o.owed.is_some())
    }

    /// Whether a display the last legacy modeset could not set is still
    /// bound.
    pub(super) fn unset_any(&self) -> bool {
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
    ///
    /// Where `commit` is false, each modeset is only tested
    /// ([`Self::atomic_modeset`]).
    pub(super) fn atomic_modeset_or_default(&mut self, commit: bool) -> Result<(), Failure> {
        let mut dark = !self.dark.is_empty();
        let mut set = self.atomic_modeset(dark, commit);
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
            set = self.atomic_modeset(false, commit);
        }
        if !refused(&set) {
            return set;
        }
        log::warn!("retrying the modeset with each display's default mode");
        let modes: Vec<Mode> = self.outputs.iter().map(|o| o.mode).collect();
        for o in &mut self.outputs {
            o.mode = o.choice.default;
        }
        let retried = self.atomic_modeset(dark, commit);
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
    pub(super) fn announce(&mut self) {
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
                // The owner has heard of a display left off, which it is
                // told to draw whole now that it lights.
                let before = self.left_off.len();
                self.left_off.retain(|l| l.id != o.id);
                if self.left_off.len() == before {
                    self.events.push(Event::Added(o.id));
                } else {
                    self.events.push(Event::Redraw(o.id));
                }
            }
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
            if self.card.removed() {
                break;
            }
            let mut set = self.legacy_set(o, &driven);
            let again = !self.card.removed()
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
    pub(super) fn legacy_set(
        &self,
        o: &mut Output<S::Memory>,
        driven: &[u32],
    ) -> Result<(), Failure> {
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
                if !card_removed(&e) {
                    log::error!("cannot set the CRTC of {}: {e}", o.name);
                }
                self.release(made.unwrap_or_default());
                if errno(&e) == Some(Errno::INVAL) {
                    Err(Failure::Test(e))
                } else {
                    Err(Failure::Other(e))
                }
            }
        }
    }
}
