//! What a modeset that cannot set the displays where the placement put
//! them does: it leaves the new displays off, sends the displays the
//! placement moved back to the CRTC each was moved off, and records the new
//! displays the card refused ([`super::Drm2d::place`]).

use std::io;

use super::super::kms::Kms;
use super::super::{DisplayId, Event};
use super::buffers::Source;
use super::output::{Moved, Output};
use super::{Drm2d, Failure, placement_refused};

/// Where the placement put a display it moved: the display, the CRTC and
/// that CRTC's index into the card's list of CRTCs.
type Placed = (DisplayId, u32, usize);

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Lets go of the new displays `new`, which a modeset could not set
    /// beside the others, for `why`; each stays off until the next hotplug.
    /// Returns each one's connector with the CRTC it sat on, for
    /// [`Self::record_refused`].
    pub(super) fn leave_off(&mut self, new: Vec<Output<S::Memory>>, why: &str) -> Vec<(u32, u32)> {
        let mut sat = Vec::with_capacity(new.len());
        for o in new {
            log::warn!("{} stays off until the next hotplug: {why}", o.name);
            sat.push((o.id.0, o.crtc));
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
            self.identities.retain(|(i, _)| *i != o.id);
            self.free(o);
        }
        sat
    }

    /// Records each new display left off, of those in `sat` with the CRTC
    /// it sat on, that sat on one of the CRTCs `back`: the CRTCs that lit
    /// displays went back to because the card refused where the placement
    /// put them, and not for a failure that may pass. Until a connector
    /// recorded is unplugged, a hotplug moves no lit display for it.
    pub(super) fn record_refused(&mut self, sat: &[(u32, u32)], back: &[u32]) {
        for &(id, crtc) in sat {
            if back.contains(&crtc) && !self.move_refused.contains(&id) {
                self.move_refused.push(id);
            }
        }
    }

    /// Where each of the displays `going` goes back to: the CRTC the last
    /// placement moved it off, and whether it was lit there.
    pub(super) fn returns<'a>(
        &'a self,
        going: &'a [DisplayId],
    ) -> impl Iterator<Item = Moved> + 'a {
        self.outputs
            .iter()
            .filter(|o| going.contains(&o.id))
            .filter_map(|o| o.moved_from)
    }

    /// Moves each of the displays `going`, which [`Self::going_back`]
    /// found, back to the CRTC the last placement moved it off, as the
    /// modeset that was to set it on its new CRTC failed. Each other display
    /// stays where it is placed, and that CRTC is its own from then on. A
    /// display whose CRTC has no primary plane stays off, as one the scan
    /// cannot bind does, and the owner hears it removed.
    pub(super) fn move_back(&mut self, going: &[DisplayId]) {
        self.send_back(going, &[]);
        for o in &mut self.outputs {
            if let Some(Moved { crtc, .. }) = o.moved_from.take()
                && !going.contains(&o.id)
            {
                log::debug!("display {} stays off CRTC {crtc}", o.name);
            }
        }
    }

    /// Puts each display the placement moved where one choice of the
    /// displays going back has it: each of `going` on the CRTC it was moved
    /// off, and each other on the CRTC `placed` gives it, where it is not
    /// already there. Each keeps its record of where it was moved from. A
    /// display whose CRTC has no primary plane stays off, as one the scan
    /// cannot bind does, and the owner hears it removed.
    fn send_back(&mut self, going: &[DisplayId], placed: &[Placed]) {
        for mut o in std::mem::take(&mut self.outputs) {
            let to = match o.moved_from {
                Some(m) if going.contains(&o.id) => Some((m.crtc, m.pipe)),
                Some(_) => placed
                    .iter()
                    .find(|p| p.0 == o.id)
                    .map(|&(_, crtc, pipe)| (crtc, pipe)),
                None => None,
            };
            match to.filter(|&(crtc, _)| crtc != o.crtc) {
                Some((crtc, pipe)) => match self.move_to(&mut o, crtc, pipe) {
                    Ok(()) => self.outputs.push(o),
                    Err(why) => self.let_go(o, &why),
                },
                None => self.outputs.push(o),
            }
        }
    }

    /// The atomic modeset that failed with `e` where the displays are
    /// placed, with fewer displays moved: the new displays are left off
    /// until the next hotplug, and the displays the placement moved go back
    /// to the CRTCs they were moved off where they can ([`Self::going_back`]).
    /// `refused` says the card refused the placement, and not for a failure
    /// that may pass.
    ///
    /// An atomic commit that fails does not say which CRTC the card refused,
    /// so where several moved displays would go back to one CRTC, the choice
    /// of the one that goes may be the choice the card refuses again. Where
    /// the card refuses a choice, the displays that went back in it lose the
    /// tie to any rival that has not lost one, and the next choice is tried:
    /// two displays moved off one CRTC go back one, then the other. Each
    /// choice is tried once. Where the card refuses every choice, the first
    /// is set without some of the displays that stay on the CRTC the
    /// placement moved them to ([`Self::set_without`]): they are left off
    /// until the next hotplug, as the new displays are, so the others light.
    /// No choice the card refused is tried again, by this modeset or by the
    /// retries.
    ///
    /// Where the card refused the placement and a choice is set, each new
    /// display left off that sat on a CRTC a display lit there went back to
    /// in that choice is recorded ([`Self::record_refused`]).
    pub(super) fn set_fewer(&mut self, e: &io::Error, refused: bool) -> Result<(), Failure> {
        let (online, new): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outputs)
            .into_iter()
            .partition(|o| o.online);
        self.outputs = online;
        let fresh = !new.is_empty();
        let sat = self.leave_off(
            new,
            &format!("it cannot be set beside the displays online: {e}"),
        );
        let placed: Vec<Placed> = self
            .outputs
            .iter()
            .filter(|o| o.moved_from.is_some())
            .map(|o| (o.id, o.crtc, o.pipe))
            .collect();
        let choices = self.choices();
        let mut last = None;
        for going in &choices {
            // A choice that leaves no display off and sends none back is the
            // placement that failed.
            if !fresh && going.is_empty() {
                continue;
            }
            self.send_back(going, &placed);
            let tried = self.atomic_modeset_or_default();
            let again = placement_refused(&tried);
            last = Some((tried, going.clone()));
            if !again {
                break;
            }
        }
        let first = choices.into_iter().next().unwrap_or_default();
        let staying: Vec<DisplayId> = self
            .outputs
            .iter()
            .filter(|o| o.moved_from.is_some() && !first.contains(&o.id))
            .map(|o| o.id)
            .collect();
        let (set, going) = match last {
            Some((tried, going)) if !placement_refused(&tried) => (tried, going),
            Some(last) if staying.is_empty() => last,
            _ => {
                self.send_back(&first, &placed);
                (self.set_without(&staying), first)
            }
        };
        if refused && set.is_ok() {
            let back: Vec<u32> = self
                .returns(&going)
                .filter(|m| m.lit)
                .map(|m| m.crtc)
                .collect();
            self.record_refused(&sat, &back);
        }
        // Each display is on its CRTC for good, as the modeset set it or as
        // the retries find it.
        for o in &mut self.outputs {
            o.moved_from = None;
        }
        set
    }

    /// The atomic modeset without some of the displays `staying`, which
    /// stay on the CRTC the placement moved them to and which the card
    /// refused there beside the others: without each one alone, the last in
    /// the card's connector order first, then without them all. The first
    /// set the card does not refuse is kept, and the displays it is without
    /// are left off until the next hotplug, as the new displays are; each of
    /// the others keeps its CRTC.
    fn set_without(&mut self, staying: &[DisplayId]) -> Result<(), Failure> {
        let order: Vec<DisplayId> = self.outputs.iter().map(|o| o.id).collect();
        let mut tries: Vec<Vec<DisplayId>> = staying.iter().rev().map(|&id| vec![id]).collect();
        if staying.len() != 1 {
            tries.push(staying.to_vec());
        }
        let mut set = Ok(());
        for off in tries {
            let (aside, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outputs)
                .into_iter()
                .partition(|o| off.contains(&o.id));
            self.outputs = kept;
            set = self.atomic_modeset_or_default();
            if !placement_refused(&set) {
                self.leave_off(
                    aside,
                    "the card refuses it on the CRTC it was moved to, beside the others",
                );
                return set;
            }
            self.outputs.extend(aside);
            self.outputs
                .sort_by_key(|o| order.iter().position(|&id| id == o.id));
        }
        set
    }

    /// The choices of the displays going back, in the order
    /// [`Self::set_fewer`] tries them: [`Self::going_back`]'s, then, for
    /// each choice in which a display went back to a CRTC another moved
    /// display was moved off too, the choice in which each display that went
    /// back in a choice before loses that tie. Each choice appears once.
    fn choices(&self) -> Vec<Vec<DisplayId>> {
        let mut choices: Vec<Vec<DisplayId>> = Vec::new();
        let mut lost: Vec<DisplayId> = Vec::new();
        loop {
            let going = self.going_back(&[], None, &lost);
            if choices.contains(&going) {
                return choices;
            }
            let rivals = |o: &Output<S::Memory>| {
                let Some(m) = o.moved_from else {
                    return false;
                };
                self.outputs.iter().any(|p| {
                    p.id != o.id
                        && !lost.contains(&p.id)
                        && p.moved_from.is_some_and(|q| q.crtc == m.crtc)
                })
            };
            let won: Vec<DisplayId> = self
                .outputs
                .iter()
                .filter(|o| going.contains(&o.id) && !lost.contains(&o.id) && rivals(o))
                .map(|o| o.id)
                .collect();
            choices.push(going);
            if won.is_empty() {
                return choices;
            }
            lost.extend(won);
        }
    }

    /// The displays that go back to the CRTC the last placement moved them
    /// off, of all but the displays `leaving`, which are new to the modeset:
    /// a new display never goes back, holds no CRTC a display goes back to,
    /// and is let go where it sits on one.
    ///
    /// Where the whole modeset failed, as an atomic commit does, `failed`
    /// is `None` and each moved display goes back where it can. Where it
    /// names the moved displays whose own set failed, as the legacy calls
    /// tell, those go back, and so does each moved display on a CRTC that a
    /// display going back goes to, so as to free it; a moved display set on
    /// its new CRTC and in no one's way stays there.
    ///
    /// A display cannot go back where a display that stays holds that CRTC.
    /// Where several displays would go back to one CRTC, one goes: one not
    /// in `lost` before one in it, then the one that was lit there, then
    /// the first in the card's connector order. A display that cannot go
    /// back stays where it is placed, so it holds that CRTC in turn, and the
    /// test repeats until no more displays stay. So no two displays end on
    /// one CRTC.
    pub(super) fn going_back(
        &self,
        leaving: &[DisplayId],
        failed: Option<&[DisplayId]>,
        lost: &[DisplayId],
    ) -> Vec<DisplayId> {
        let outputs: Vec<&Output<S::Memory>> = self
            .outputs
            .iter()
            .filter(|o| !leaving.contains(&o.id))
            .collect();
        let mut going: Vec<bool> = outputs.iter().map(|o| o.moved_from.is_some()).collect();
        loop {
            let mut stays = false;
            if let Some(failed) = failed {
                let need = needed(&outputs, &going, failed);
                for (goes, n) in going.iter_mut().zip(need) {
                    if *goes && !n {
                        *goes = false;
                        stays = true;
                    }
                }
            }
            let held: Vec<u32> = outputs
                .iter()
                .zip(&going)
                .filter(|&(_, &goes)| !goes)
                .map(|(o, _)| o.crtc)
                .collect();
            // Of the displays going back to one CRTC, the one that goes:
            // one that has not lost a tie before one that has, then a lit
            // one before one that was not, then the first.
            let rank = |o: &Output<S::Memory>, m: Moved| (!lost.contains(&o.id), m.lit);
            let first = |at: usize, o: &Output<S::Memory>, m: Moved| {
                outputs
                    .iter()
                    .zip(&going)
                    .enumerate()
                    .all(|(j, (p, &goes))| {
                        let Some(q) = p.moved_from.filter(|q| goes && j != at && q.crtc == m.crtc)
                        else {
                            return true;
                        };
                        let (mine, theirs) = (rank(o, m), rank(p, q));
                        mine > theirs || mine == theirs && at < j
                    })
            };
            let staying: Vec<usize> = outputs
                .iter()
                .zip(&going)
                .enumerate()
                .filter_map(|(at, (o, &goes))| {
                    let m = o.moved_from.filter(|_| goes)?;
                    (held.contains(&m.crtc) || !first(at, o, m)).then_some(at)
                })
                .collect();
            for at in staying {
                if let Some(goes) = going.get_mut(at) {
                    *goes = false;
                    stays = true;
                }
            }
            if !stays {
                break;
            }
        }
        outputs
            .iter()
            .zip(going)
            .filter(|&(_, goes)| goes)
            .map(|(o, _)| o.id)
            .collect()
    }
}

/// Of the displays `outputs` that `going` marks as able to go back, those
/// needed: each in `failed`, whose set failed, and then each that sits on
/// the CRTC a display needed goes back to, as it must leave that CRTC.
pub(super) fn needed<M>(outputs: &[&Output<M>], going: &[bool], failed: &[DisplayId]) -> Vec<bool> {
    let mut need: Vec<bool> = outputs
        .iter()
        .zip(going)
        .map(|(o, &goes)| goes && failed.contains(&o.id))
        .collect();
    loop {
        let wanted: Vec<u32> = outputs
            .iter()
            .zip(&need)
            .filter(|&(_, &n)| n)
            .filter_map(|(o, _)| Some(o.moved_from?.crtc))
            .collect();
        let mut more = false;
        for ((o, n), &goes) in outputs.iter().zip(need.iter_mut()).zip(going) {
            if goes && !*n && wanted.contains(&o.crtc) {
                *n = true;
                more = true;
            }
        }
        if !more {
            return need;
        }
    }
}
