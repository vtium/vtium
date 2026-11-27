//! The atomic modeset: the request that turns displays off first and sets
//! the rest, the planes it clears or hides, its test-only commits, and the
//! connectors' color properties it carries.

use std::io;

use drm_ffi::{
    DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_ATOMIC_TEST_ONLY,
    DRM_MODE_PAGE_FLIP_EVENT,
};
use rustix::io::Errno;

#[cfg(feature = "scroll-pan")]
use super::super::DisplayId;
use super::super::Dpms;
use super::super::kms::{Kms, Object, Request};
use super::buffers::{FORMAT, Source};
use super::output::{
    Output, PLANE_CURSOR, PLANE_OVERLAY, Props, bits_per_color, mode_bytes, value,
};
use super::{Drm2d, Failure, card_removed, errno};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Hides every cursor and overlay plane, which another program may
    /// have left in the middle of the screen, and every plane on a CRTC in
    /// `off`, which the commit turns off.
    pub(super) fn clear_planes(&self, req: &mut Request, off: &[u32]) {
        self.hide_planes(req, off, true);
    }

    /// Hides every plane on a CRTC in `off`, and every cursor and overlay
    /// plane too when `others`.
    fn hide_planes(&self, req: &mut Request, off: &[u32], others: bool) {
        let all = self.card.planes().unwrap_or_else(|e| {
            if !card_removed(&e) {
                log::warn!("cannot list the card's planes: {e}");
            }
            Vec::new()
        });
        for plane in all {
            let props = match self.card.properties(plane, Object::Plane) {
                Ok(props) => props,
                Err(_) if self.card.removed() => return,
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
        let req = self.off_first()?;
        if req.entries.is_empty() {
            return Ok(());
        }
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
                    if !card_removed(&e) {
                        log::error!("cannot turn off what the modeset would leave lit: {e}");
                    }
                    Failure::Other(e)
                }
            })
    }

    /// What [`Self::set_off_first`] commits, empty where it turns nothing
    /// off.
    fn off_first(&self) -> Result<Request, Failure> {
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
        let mut req = Request::default();
        if ids.is_empty() && off.is_empty() {
            return Ok(req);
        }
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
            if !card_removed(&e) {
                log::error!("cannot turn off what the modeset would leave lit: {e}");
            }
            Failure::Other(e)
        })?;
        self.hide_planes(&mut req, &off, false);
        Ok(req)
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
                Err(_) if self.card.removed() => return,
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
    /// commit, then committed without blocking, with a flip event. Where
    /// `commit` is false, the modeset is only tested, in a test-only commit
    /// that also carries what [`Self::set_off_first`] would turn off first,
    /// and nothing changes on the card or in the displays: so a search of
    /// placements turns no lit display off ([`Self::relight`]).
    pub(super) fn atomic_modeset(&mut self, dark: bool, commit: bool) -> Result<(), Failure> {
        if self.card.removed() {
            return Err(Failure::Other(Errno::NODEV.into()));
        }
        let mut req = if commit {
            self.set_off_first()?;
            Request::default()
        } else {
            self.off_first()?
        };
        if self.card.removed() {
            return Err(Failure::Other(Errno::NODEV.into()));
        }
        let off = if dark {
            self.prepare_dark(&mut req)
        } else {
            Vec::new()
        };
        self.take_others_off(&mut req);
        if !self.card.removed() {
            self.clear_planes(&mut req, &off);
        }
        if self.card.removed() {
            return Err(Failure::Other(Errno::NODEV.into()));
        }
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
        for o in &mut outputs {
            log::info!(
                "preparing the modeset of {} at {}x{}",
                o.name,
                o.size().0,
                o.size().1
            );
            #[cfg(feature = "scroll-pan")]
            let made = self.stage_tall(o);
            #[cfg(not(feature = "scroll-pan"))]
            let made = self.stage(o);
            let o = &*o;
            prepared = made.and_then(|made| {
                let fb = Self::staged_fb(o, made.as_ref());
                match self.card.create_blob(&mode_bytes(&o.mode.0)) {
                    Ok(blob) => {
                        staged.push((made, blob));
                        // The modeset shows each display's window at the
                        // buffer's start.
                        o.prepare(&mut req, fb, (0, 0), blob, None, self.hotspot)
                    }
                    Err(e) => {
                        self.release(made.unwrap_or_default());
                        Err(e)
                    }
                }
            });
            if prepared.is_err() || self.card.removed() {
                break;
            }
        }
        if self.card.removed() {
            prepared = Err(Errno::NODEV.into());
        }
        #[cfg(feature = "scroll-pan")]
        let mut pans: Vec<(DisplayId, Option<String>)> = Vec::new();
        let outcome = prepared.map_err(Failure::Other).and_then(|()| {
            let req = self.with_color(&outputs, req)?;
            if !commit {
                return Ok(());
            }
            #[cfg(feature = "scroll-pan")]
            {
                pans = self.test_pans(&outputs, &staged, &req);
                if pans.iter().any(|(_, why)| why.is_some()) {
                    return Ok(());
                }
            }
            self.card
                .commit(
                    &req,
                    DRM_MODE_ATOMIC_ALLOW_MODESET
                        | DRM_MODE_ATOMIC_NONBLOCK
                        | DRM_MODE_PAGE_FLIP_EVENT,
                )
                .map_err(|e| {
                    if !card_removed(&e) {
                        log::error!("modeset atomic commit failed: {e}");
                    }
                    Failure::Other(e)
                })
        });
        // A display not set keeps what it had: an online one its buffers
        // and what it shows.
        #[cfg(feature = "scroll-pan")]
        let pan_refused = pans.iter().any(|(_, why)| why.is_some());
        #[cfg(feature = "scroll-pan")]
        let set = commit && outcome.is_ok() && !pan_refused;
        // The displays whose tall buffers the card has not taken yet, where
        // the modeset failed: the card may refuse a framebuffer taller than
        // the screen whatever the window, so these try the modeset again
        // with buffers the screen's size.
        #[cfg(feature = "scroll-pan")]
        let untested: Vec<DisplayId> = if outcome.is_err() && !pan_refused && !self.card.removed() {
            outputs
                .iter()
                .zip(&staged)
                .filter(|(o, (made, _))| Self::untested_tall(o, made.as_ref()))
                .map(|(o, _)| o.id)
                .collect()
        } else {
            Vec::new()
        };
        #[cfg(not(feature = "scroll-pan"))]
        let set = commit && outcome.is_ok();
        for (o, (made, blob)) in outputs.iter_mut().zip(staged) {
            if set {
                if o.mode_blob != 0 && !self.card.removed() {
                    let _ = self.card.destroy_blob(o.mode_blob);
                }
                o.mode_blob = blob;
                self.install(o, made);
                // Every modeset shows the window at the buffer's start: the
                // one a wake-up runs after a sleep, and a hotplug's.
                #[cfg(feature = "scroll-pan")]
                {
                    o.reset_window();
                    if pans.iter().any(|(id, _)| *id == o.id) {
                        o.pan.proven = o.buffers.first().map(|b| b.size);
                    }
                }
                o.online = true;
                o.returning = false;
                o.start_flip();
                o.cursor_committed();
                o.modeset_flip = true;
                o.modeset_flip_no = Some(o.flips);
                o.damage.set_whole();
            } else {
                if !self.card.removed() {
                    let _ = self.card.destroy_blob(blob);
                }
                self.release(made.unwrap_or_default());
            }
        }
        for (o, was) in outputs.iter_mut().zip(was) {
            if !set {
                // Nothing changed on the card: the state recorded stays.
                o.dpms = was;
            } else if was.is_some_and(|(_, d)| d != Dpms::On) {
                o.powered_on();
            }
        }
        #[cfg(feature = "scroll-pan")]
        if pan_refused {
            // The card refused the pan of these displays: each keeps
            // buffers the screen's size from now on, and the modeset runs
            // again with them. Each run refuses at least one more display's
            // pan, so this ends.
            for o in outputs.iter_mut() {
                if let Some((_, Some(why))) = pans.iter().find(|(id, _)| *id == o.id) {
                    Self::refuse_pan(o, why);
                }
            }
            self.outputs = outputs;
            return self.atomic_modeset(dark, commit);
        }
        #[cfg(feature = "scroll-pan")]
        if !untested.is_empty()
            && let Err(failed) = outcome
        {
            return self.without_tall(outputs, &untested, failed, dark, commit);
        }
        self.outputs = outputs;
        outcome
    }

    /// After a modeset that `failed` and would have shown the displays
    /// `untested` in tall buffers no commit has taken yet, runs it again
    /// with those displays in buffers the screen's size. Where that takes,
    /// the tall buffers were to blame: each of those displays keeps
    /// buffers the screen's size from now on, logged once. Where it fails
    /// too, the tall buffers were not to blame: the displays keep their
    /// ask, and the modeset fails as it did.
    ///
    /// References:
    /// - ADR-1401 D2, "A buffer taller than the screen": where the card
    ///   refuses the test, the display makes buffers the screen's size.
    #[cfg(feature = "scroll-pan")]
    fn without_tall(
        &mut self,
        mut outputs: Vec<Output<S::Memory>>,
        untested: &[DisplayId],
        failed: Failure,
        dark: bool,
        commit: bool,
    ) -> Result<(), Failure> {
        for o in outputs.iter_mut().filter(|o| untested.contains(&o.id)) {
            o.pan.refused = true;
        }
        self.outputs = outputs;
        let retried = self.atomic_modeset(dark, commit);
        let (Failure::Test(e) | Failure::Other(e)) = &failed;
        let why = format!("the card refused a modeset with its tall buffers: {e}");
        for o in self.outputs.iter_mut().filter(|o| untested.contains(&o.id)) {
            if retried.is_ok() {
                Self::log_pan_refused(o, &why);
            } else {
                o.pan.refused = false;
            }
        }
        retried.map_err(|_| failed)
    }

    /// A test-only commit of `req`, the card's refusal logged unless
    /// `quiet`.
    pub(super) fn test_commit(&self, req: &Request, quiet: bool) -> Result<(), Failure> {
        self.card
            .commit(
                req,
                DRM_MODE_ATOMIC_TEST_ONLY | DRM_MODE_ATOMIC_ALLOW_MODESET,
            )
            .map_err(|e| {
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
    pub(super) fn color_refused(&self, name: &str, property: Option<&str>, e: &io::Error) {
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
}
