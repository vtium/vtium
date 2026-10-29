//! The connectors a display's policy leaves dark (ADR-1401 D3): each set
//! off where another master left it on a CRTC, so a frame that master
//! left goes dark, through the atomic modeset or the legacy calls.

use std::io;

use drm_ffi::{DRM_MODE_ATOMIC_ALLOW_MODESET, DRM_MODE_ATOMIC_NONBLOCK};

use super::super::Identity;
use super::super::kms::{Kms, Object, Request};
use super::Drm2d;
use super::output::{Props, value};

/// A connected connector the policy leaves dark.
#[derive(Debug)]
pub(super) struct Dark {
    pub(super) id: u32,
    pub(super) identity: Identity,
    /// Seen by the scan in progress.
    pub(super) available: bool,
}

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms> Drm2d<K> {
    /// The CRTC dark connector `d` is on, as another master left it.
    pub(super) fn dark_crtc(&self, d: &Dark) -> Option<u32> {
        self.connector_crtc(d.id)
    }

    /// The CRTC connector `id` is on, as the kernel reports it.
    pub(super) fn connector_crtc(&self, id: u32) -> Option<u32> {
        let conn = self
            .card
            .connector(id)
            .inspect_err(|e| log::debug!("connector {id} is gone: {e}"))
            .ok()?;
        let crtc = (conn.encoder != 0)
            .then(|| self.encoder(conn.encoder))
            .flatten()?
            .crtc;
        (crtc != 0).then_some(crtc)
    }

    /// Adds to `req` what sets each dark connector off where another
    /// master left it on a CRTC, and returns the CRTCs it turns off, whose
    /// planes [`Self::clear_planes`] takes down and every connector on
    /// which, one vtium neither binds nor darkens included, leaves it, as
    /// the kernel turns off only a CRTC no connector is on
    /// (`drm_atomic_helper_check_modeset`). A CRTC a display takes only
    /// loses the connector; one that is not active shows nothing and stays
    /// out of the commit, which asks no flip event of it
    /// (`drm_atomic_crtc_check`).
    pub(super) fn prepare_dark(&self, req: &mut Request) -> Vec<u32> {
        let mut off = Vec::new();
        for d in &self.dark {
            let Some(crtc) = self.dark_crtc(d) else {
                continue;
            };
            let name = &d.identity.connector;
            let taken = self.outputs.iter().any(|o| o.crtc == crtc);
            let crtc_props = if taken || off.contains(&crtc) {
                None
            } else {
                match self.card.properties(crtc, Object::Crtc) {
                    Ok(props) if value(&props, "ACTIVE") == Some(0) => continue,
                    Ok(props) => Some(props),
                    Err(e) => {
                        log::warn!(
                            "cannot set {name} off: cannot read the properties of CRTC {crtc}: {e}"
                        );
                        continue;
                    }
                }
            };
            // Each part goes in whole, or not at all.
            let mut part = Request::default();
            let set = self.unbind(&mut part, d.id).and_then(|()| {
                let Some(props) = &crtc_props else {
                    return Ok(());
                };
                let props = Props::new(props);
                props.set(&mut part, crtc, "MODE_ID", 0)?;
                props.set(&mut part, crtc, "ACTIVE", 0)?;
                for other in self.card.resources()?.connectors {
                    if other != d.id && self.connector_crtc(other) == Some(crtc) {
                        self.unbind(&mut part, other)?;
                    }
                }
                Ok(())
            });
            match set {
                Ok(()) => {
                    req.entries.extend(part.entries);
                    if crtc_props.is_some() {
                        off.push(crtc);
                    }
                }
                Err(e) => log::warn!("cannot set {name} off: {e}"),
            }
        }
        off
    }

    /// Adds to `part` what takes connector `id` off its CRTC.
    pub(super) fn unbind(&self, part: &mut Request, id: u32) -> io::Result<()> {
        let props = self.card.properties(id, Object::Connector)?;
        Props::new(&props).set(part, id, "CRTC_ID", 0)
    }

    /// Sets the dark connectors off while no display is online, taking
    /// DRM master for it.
    pub(super) fn darken(&mut self) {
        if !self.master {
            if let Err(e) = self.card.set_master() {
                log::warn!("cannot take DRM master to set the dark displays off: {e}");
                return;
            }
            self.master = true;
        }
        if self.legacy {
            self.legacy_dark();
            return;
        }
        let mut req = Request::default();
        let off = self.prepare_dark(&mut req);
        if req.entries.is_empty() {
            return;
        }
        self.clear_planes(&mut req, &off);
        let flags = DRM_MODE_ATOMIC_ALLOW_MODESET | DRM_MODE_ATOMIC_NONBLOCK;
        if let Err(e) = self.card.commit(&req, flags) {
            log::warn!("cannot set the dark displays off: {e}");
        }
    }

    /// The legacy calls' [`Self::prepare_dark`]: turns off each CRTC a
    /// dark connector is on that no display takes and that shows a mode.
    pub(super) fn legacy_dark(&self) {
        let mut off = Vec::new();
        for d in &self.dark {
            let Some(crtc) = self.dark_crtc(d) else {
                continue;
            };
            if off.contains(&crtc)
                || self.outputs.iter().any(|o| o.crtc == crtc)
                || !matches!(self.card.crtc_mode(crtc), Ok(Some(_)))
            {
                continue;
            }
            match self.card.disable_crtc(crtc) {
                Ok(()) => off.push(crtc),
                Err(e) => log::warn!("cannot set {} off: {e}", d.identity.connector),
            }
        }
    }
}
