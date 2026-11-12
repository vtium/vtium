//! What a modeset that cannot set the displays where the placement put
//! them does: it tries the other placements of every display, a lit
//! display moved included, and where the card refuses each, it leaves the
//! new displays off, sends the displays the placement moved back to the
//! CRTC each was moved off, and records the new displays the card refused
//! ([`super::Drm2d::place`]).

use std::io;

use super::super::DisplayId;
use super::super::kms::Kms;
use super::buffers::Source;
use super::matching::index_bit;
use super::output::{Moved, Output};
use super::{Drm2d, Failure, drop_cursor, placement_refused};

/// Where the placement put a display it moved: the display, the CRTC and
/// that CRTC's index into the card's list of CRTCs.
type Placed = (DisplayId, u32, usize);

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// After the card refused the atomic modeset of the displays where the
    /// placement put them, tries other placements of all of them, each
    /// display on a CRTC it can take ([`Output::able`]), until the card
    /// takes one, and keeps that one (vtium/vtium#298). Whether one lit.
    /// Each placement is tried in one test-only commit, which carries what
    /// the modeset would turn off first, so the search turns no lit
    /// display off; only the placement the test takes is set.
    ///
    /// The placement keeps each lit display on the CRTC it wants wherever
    /// some placement does, so the CRTC it gives a new display, or a display
    /// a wake-up moved, can be one the card refuses, as past its bandwidth,
    /// while moving a lit display would light every display. The placements
    /// are tried in the order of [`Self::placements`], so the first that
    /// lights moves as few lit displays as any that lights. The search is
    /// bounded: it tries only placements that move at most one lit display,
    /// at most [`RELIGHT_TRIES`] of them, each once, with no retry and no
    /// delay. Where a placement fails for a reason that may pass, the
    /// search stops.
    ///
    /// Where every placement is refused, each display goes back where the
    /// placement put it, and each new display, one never set, that a
    /// placement tried on another CRTC is recorded as refused: the card
    /// refused it even with lit displays moved for it
    /// ([`Self::record_refused`]). A new display already recorded is
    /// searched for no more, so the lit displays do not blank at every
    /// hotplug for a display the card cannot light beside them.
    pub(super) fn relight(&mut self) -> bool {
        let fresh: Vec<usize> = (0..self.outputs.len())
            .filter(|&d| self.outputs.get(d).is_some_and(|o| !o.online))
            .collect();
        let refused_before = |d: &usize| {
            self.outputs
                .get(*d)
                .is_some_and(|o| self.move_refused.contains(&o.id.0))
        };
        if !fresh.is_empty() && fresh.iter().all(refused_before) {
            return false;
        }
        let placed: Vec<(u32, usize, Option<super::output::Planes>)> = self
            .outputs
            .iter()
            .map(|o| (o.crtc, o.pipe, o.planes.clone()))
            .collect();
        let mut refused = false;
        // The new displays some placement tried put on another CRTC.
        let mut elsewhere: Vec<usize> = Vec::new();
        for placement in self.placements() {
            let mut planes = Vec::with_capacity(placement.len());
            for (o, &pipe) in self.outputs.iter().zip(&placement) {
                let crtc = self.crtcs.get(pipe).copied().unwrap_or(0);
                let Some(mut p) = self.find_planes(crtc, pipe) else {
                    break;
                };
                p.connector_props = o
                    .planes
                    .as_ref()
                    .map(|q| q.connector_props.clone())
                    .unwrap_or_default();
                planes.push((crtc, pipe, p));
            }
            if planes.len() != placement.len() {
                continue;
            }
            for &d in &fresh {
                let moved = placement.get(d) != placed.get(d).map(|p| &p.1);
                if moved && !elsewhere.contains(&d) {
                    elsewhere.push(d);
                }
            }
            for (o, (crtc, pipe, p)) in self.outputs.iter_mut().zip(planes) {
                o.crtc = crtc;
                o.pipe = pipe;
                o.planes = Some(p);
            }
            // Only a test commit tries a placement, so no lit display goes
            // off for one the card refuses; the placement the test took is
            // then set.
            let tested = self.atomic_modeset_or_default(false);
            if tested.is_ok() {
                if self.atomic_modeset_or_default(true).is_ok() {
                    self.relit(&placed);
                    return true;
                }
                refused = false;
                break;
            }
            // A failure that may pass, the card's removal included, ends
            // the search.
            refused = placement_refused(&tested);
            if !refused {
                break;
            }
        }
        for (o, (crtc, pipe, planes)) in self.outputs.iter_mut().zip(placed) {
            o.crtc = crtc;
            o.pipe = pipe;
            o.planes = planes;
        }
        if refused {
            for d in elsewhere {
                if let Some(o) = self.outputs.get(d)
                    && !self.move_refused.contains(&o.id.0)
                {
                    self.move_refused.push(o.id.0);
                }
            }
        }
        false
    }

    /// Settles the displays on the placement [`Self::relight`] lit, from
    /// the CRTCs `placed` they were on: each display that moved says so,
    /// its cursor plane shows nothing of vtium's yet, and one whose new CRTC
    /// has no cursor plane gives its pointer image up, so the owner draws
    /// the pointer itself.
    fn relit(&mut self, placed: &[(u32, usize, Option<super::output::Planes>)]) {
        for (o, &(was, _, _)) in self.outputs.iter_mut().zip(placed) {
            if o.crtc == was {
                continue;
            }
            log::info!(
                "display {} moves from CRTC {was} to CRTC {}",
                o.name,
                o.crtc
            );
            if o.planes.as_ref().is_some_and(|p| p.cursor.is_none()) {
                drop_cursor(&self.card, o);
            }
            o.cursor_set = None;
        }
        self.move_refused
            .retain(|id| !self.outputs.iter().any(|o| o.id.0 == *id));
    }

    /// The placements of the displays [`Self::relight`] tries, by index
    /// into the card's list of CRTCs: each display on a CRTC it can take,
    /// no two on one, at most one lit display off the CRTC it wants, and
    /// not the placement that failed. The search lists them in at most
    /// [`RELIGHT_STEPS`] steps and keeps the first [`RELIGHT_TRIES`]. They
    /// are in the
    /// order of what they cost, as the placement counts it
    /// ([`super::matching`]): first by how many lit displays they put off
    /// the CRTC they want, then by how many other displays, then in the
    /// card's order of CRTCs. A display wants the CRTC the placement moved
    /// it off, and else the one it is on; it is lit there where the
    /// placement says so, and else where it is online.
    fn placements(&self) -> Vec<Vec<usize>> {
        let wants: Vec<(usize, bool)> = self
            .outputs
            .iter()
            .map(|o| o.moved_from.map_or((o.pipe, o.online), |m| (m.pipe, m.lit)))
            .collect();
        let ables: Vec<u32> = self.outputs.iter().map(|o| o.able).collect();
        let now: Vec<usize> = self.outputs.iter().map(|o| o.pipe).collect();
        let mut search = Search {
            ables: &ables,
            wants: &wants,
            crtcs: self.crtcs.len(),
            steps: RELIGHT_STEPS,
            chosen: Vec::with_capacity(ables.len()),
            all: Vec::new(),
        };
        search.walk(0, false);
        let mut all = search.all;
        all.retain(|p| *p != now);
        all.sort_by_cached_key(|p| {
            let off = |lit: bool| {
                p.iter()
                    .zip(&wants)
                    .filter(|&(&c, &(want, l))| l == lit && c != want)
                    .count()
            };
            (off(true), off(false), p.clone())
        });
        all.truncate(RELIGHT_TRIES);
        all
    }

    /// Lets go of the displays `new`, which a modeset could not set beside
    /// the others, for `why`; each stays off, present but dark where the
    /// owner has heard of it ([`Self::keep_left_off`]), and the next scan,
    /// at a hotplug or a wake-up, places it again. Returns each one's
    /// connector with the CRTC it sat on, for [`Self::record_refused`].
    pub(super) fn leave_off(&mut self, new: Vec<Output<S::Memory>>, why: &str) -> Vec<(u32, u32)> {
        let mut sat = Vec::with_capacity(new.len());
        for o in new {
            log::warn!("{} stays off: {why}; the next scan tries it again", o.name);
            sat.push((o.id.0, o.crtc));
            self.keep_left_off(&o);
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
    /// placed, with fewer displays moved: the new displays are left off,
    /// and the displays the placement moved go back to the CRTCs they were
    /// moved off where they can ([`Self::going_back`]). `refused` says the
    /// card refused the placement, and not for a failure that may pass.
    ///
    /// An atomic commit that fails does not say which CRTC the card refused,
    /// so where several moved displays would go back to one CRTC, the choice
    /// of the one that goes may be the choice the card refuses again. Where
    /// the card refuses a choice, the displays that went back in it lose the
    /// tie to any rival that has not lost one, and the next choice is tried:
    /// two displays moved off one CRTC go back one, then the other. Each
    /// choice is tried once in this modeset. Where the card refuses every
    /// choice, the first is set without some of the displays that stay on
    /// the CRTC the placement moved them to ([`Self::set_without`]): they
    /// are left off, as the new displays are, so the others light, and the
    /// retries do not run. A display left off is placed again at later
    /// scans. The modeset runs this only after [`Self::relight`] found no
    /// other placement of every display that the card takes.
    ///
    /// Only displays the placement moved are left off. Where the card
    /// refuses even the displays that were not moved, as when it refuses
    /// every CRTC, the modeset fails, and the retries scan and try the
    /// choices again, as for any modeset that fails.
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
            let tried = self.atomic_modeset_or_default(true);
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
    /// are left off, as the new displays are; each of the others keeps its
    /// CRTC.
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
            set = self.atomic_modeset_or_default(true);
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

/// The most placements [`Drm2d::relight`] tries in one search.
pub(super) const RELIGHT_TRIES: usize = 16;

/// The most steps the search takes to list the placements it may try, a
/// step for each display it puts on a CRTC: placements are many on a card
/// with many CRTCs, 40320 for eight displays on eight.
const RELIGHT_STEPS: usize = 4096;

/// A search for the placements [`Drm2d::relight`] may try.
struct Search<'a> {
    /// The CRTCs each display can take, a bit per index.
    ables: &'a [u32],
    /// The CRTC each display wants, and whether it is lit there.
    wants: &'a [(usize, bool)],
    crtcs: usize,
    /// The steps left.
    steps: usize,
    /// The CRTC of each display placed so far.
    chosen: Vec<usize>,
    all: Vec<Vec<usize>>,
}

impl Search<'_> {
    /// Adds every placement that puts each display after those chosen on
    /// a CRTC it can take that `used` does not hold, while steps are left,
    /// with at most one lit display off the CRTC it wants: `moved` says
    /// one already is.
    fn walk(&mut self, used: u32, moved: bool) {
        let d = self.chosen.len();
        let (Some(&able), Some(&(want, lit))) = (self.ables.get(d), self.wants.get(d)) else {
            self.all.push(self.chosen.clone());
            return;
        };
        for c in 0..self.crtcs {
            let bit = index_bit(c);
            let off = lit && c != want;
            if able & bit == 0 || used & bit != 0 || off && moved {
                continue;
            }
            let Some(left) = self.steps.checked_sub(1) else {
                return;
            };
            self.steps = left;
            self.chosen.push(c);
            self.walk(used | bit, moved || off);
            self.chosen.pop();
        }
    }
}
