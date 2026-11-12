//! The displays left off while their monitors are still plugged in
//! (ADR-1401 D8). Such a display keeps its id, with no CRTC and no
//! buffers, until a placement lights it. The owner hears it unlit, not
//! removed, so its head keeps it, and is told to draw it whole once a
//! modeset lights it.

use super::super::edid::EdidId;
use super::super::kms::Kms;
use super::super::{DisplayId, Event};
use super::Drm2d;
use super::buffers::Source;
use super::output::Output;

/// A display left off: the owner has heard of it, its monitor is still
/// plugged in, and it has no CRTC and no buffers. Each scan finds its
/// connector as a new one and places it again. The display keeps this
/// record, through every placement that leaves it off again, until a
/// modeset lights it.
#[derive(Debug)]
pub(super) struct LeftOff {
    pub(super) id: DisplayId,
    /// The connector's name, as `HDMI-A-1`.
    pub(super) name: String,
    /// The monitor its EDID named, so a scan that finds another monitor
    /// on the connector reports the display removed.
    pub(super) edid: Option<EdidId>,
    /// Seen by the scan in progress.
    pub(super) available: bool,
}

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Keeps a record of display `o`, which a placement or a modeset
    /// leaves off while its monitor is still plugged in, before `o` is
    /// freed. The owner hears it unlit once, as it goes from lit to left
    /// off. A display the owner never heard of needs no record, and one
    /// placed again from its record and left off again keeps the record it
    /// has, so the owner hears nothing new.
    pub(super) fn keep_left_off(&mut self, o: &Output<S::Memory>) {
        if !o.announced {
            return;
        }
        log::info!(
            "display {} stays connected but dark until a placement lights it",
            o.name
        );
        let edid = self.known_edid(o.id).cloned();
        self.events.push(Event::Unlit(o.id));
        self.left_off.push(LeftOff {
            id: o.id,
            name: o.name.clone(),
            edid,
            available: true,
        });
    }

    /// The record of display `id`, where it is left off.
    pub(super) fn left_off(&self, id: DisplayId) -> Option<&LeftOff> {
        self.left_off.iter().find(|l| l.id == id)
    }

    /// At a scan, marks the record of connector `id`, connected, as seen.
    /// Where the connector shows another monitor than the one the display
    /// was left off with, the record goes, and the owner hears the display
    /// removed; the scan then binds the new monitor as a new display.
    pub(super) fn see_left_off(&mut self, id: u32) {
        let Some(at) = self.left_off.iter().position(|l| l.id.0 == id) else {
            return;
        };
        let another = self
            .left_off
            .get(at)
            .is_some_and(|l| self.another_monitor(&l.name, l.edid.as_ref()));
        if another {
            let l = self.left_off.remove(at);
            log::info!("display {}, left off, shows another monitor", l.name);
            self.events.push(Event::Removed(l.id));
        } else if let Some(l) = self.left_off.get_mut(at) {
            l.available = true;
        }
    }

    /// At a scan that found connector `id` left dark by the policy, lets
    /// the display's record go, where it has one: the owner hears the
    /// display removed.
    pub(super) fn darken_left_off(&mut self, id: u32) {
        if let Some(at) = self.left_off.iter().position(|l| l.id.0 == id) {
            let l = self.left_off.remove(at);
            self.events.push(Event::Removed(l.id));
        }
    }

    /// After a scan has read every connector, lets go of the record of
    /// each display whose connector it did not find connected: the owner
    /// hears that display removed. The scan's start marks every record
    /// unseen.
    pub(super) fn drop_left_off_gone(&mut self) {
        let events = &mut self.events;
        self.left_off.retain(|l| {
            if !l.available {
                log::info!("display {}, left off, is gone", l.name);
                events.push(Event::Removed(l.id));
            }
            l.available
        });
    }
}
