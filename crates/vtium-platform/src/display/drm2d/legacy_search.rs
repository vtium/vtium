//! The search a legacy modeset runs where the card refused to set a
//! display: the placements of every display that [`Drm2d::relight`] tries
//! with atomic commits, set through the legacy calls (ADR-1401 D8).

use super::super::DisplayId;
use super::super::kms::Kms;
use super::buffers::Source;
use super::matching::index_bit;
use super::{Drm2d, Failure};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// After the legacy modeset could not set the displays `unset`, each
    /// with whether the card refused it (`EINVAL`), tries other placements
    /// of every display, a lit display moved included, until every display
    /// is set (vtium/vtium#298). Returns the displays still unset, each
    /// with whether the card refused it.
    ///
    /// The legacy calls have no test-only commit, so the search tries a
    /// placement by setting its displays. It sets the lit display the
    /// placement moves first, alone, on its new CRTC. The kernel sets a
    /// CRTC all or nothing (`drm_atomic_helper_set_config` checks the new
    /// state before it commits it, and `drm_crtc_helper_set_config` puts
    /// the old one back where a set fails). So where the card refuses that
    /// set, the lit display stays lit where it was, and the search gives
    /// the placement up. Where the card takes it, the display lights on its
    /// new CRTC and stays there, whether or not the displays it moved for
    /// then light. The search then sets each display still unset on the
    /// CRTC the placement gives it. So a lit display the search moves
    /// blanks once while it moves, and never ends dark.
    ///
    /// The search is bounded as the atomic one is. It tries the placements
    /// of [`Self::placements`], at most [`super::back::RELIGHT_TRIES`], in
    /// the order of what they cost. It skips each placement that moves more
    /// than one lit display, moves a lit display it has already moved, or
    /// sets a display on a CRTC the card refused it in this search. It runs
    /// only where the card refused every display the modeset could not set,
    /// and not where each of them is recorded as refused. A failure that
    /// may pass ends it, with no retry and no delay.
    ///
    /// Where no placement sets every display, each display still unset
    /// takes back the CRTC the placement put it on, or else a CRTC it can
    /// take that no other display holds ([`Self::keep_unset_apart`]). Where
    /// the card refused every placement tried, each of those displays that
    /// a placement tried on another CRTC is recorded as refused, so a later
    /// modeset does not search for it again.
    pub(super) fn legacy_relight(
        &mut self,
        mut unset: Vec<(DisplayId, bool)>,
    ) -> Vec<(DisplayId, bool)> {
        let refused_all = !unset.is_empty() && unset.iter().all(|&(_, refused)| refused);
        let recorded = unset
            .iter()
            .all(|(id, _)| self.move_refused.contains(&id.0));
        if !refused_all || recorded || self.card.removed() {
            return unset;
        }
        let is_unset = |unset: &[(DisplayId, bool)], id: DisplayId| unset.iter().any(|u| u.0 == id);
        let placed: Vec<(DisplayId, usize)> = self.outputs.iter().map(|o| (o.id, o.pipe)).collect();
        // The sets the card refused in this search, as the display and the
        // index of its CRTC.
        let mut refusals: Vec<(DisplayId, usize)> = self
            .outputs
            .iter()
            .filter(|o| is_unset(&unset, o.id))
            .map(|o| (o.id, o.pipe))
            .collect();
        // The lit displays the search moved.
        let mut moved: Vec<DisplayId> = Vec::new();
        // The unset displays some placement tried on another CRTC.
        let mut elsewhere: Vec<DisplayId> = Vec::new();
        let mut refused = true;
        for placement in self.placements() {
            // Each set the placement needs, as an index into the outputs
            // and the index of the CRTC: every display unset, and every lit
            // display the placement puts on another CRTC, the lit first.
            let mut sets: Vec<(usize, usize, bool)> = Vec::new();
            let mut skip = false;
            for (i, (o, &pipe)) in self.outputs.iter().zip(&placement).enumerate() {
                let lit = !is_unset(&unset, o.id);
                if lit && o.pipe == pipe {
                    continue;
                }
                skip |= refusals.contains(&(o.id, pipe)) || lit && moved.contains(&o.id);
                sets.push((i, pipe, lit));
            }
            if skip || sets.iter().filter(|s| s.2).count() > 1 {
                continue;
            }
            sets.sort_by_key(|s| !s.2);
            for &(i, pipe, lit) in &sets {
                let Some(id) = self.outputs.get(i).map(|o| o.id) else {
                    continue;
                };
                if !lit && !placed.contains(&(id, pipe)) && !elsewhere.contains(&id) {
                    elsewhere.push(id);
                }
            }
            let mut outputs = std::mem::take(&mut self.outputs);
            let mut passing = false;
            for (i, pipe, lit) in sets {
                let Some(&crtc) = self.crtcs.get(pipe) else {
                    continue;
                };
                let driven: Vec<u32> = outputs
                    .iter()
                    .enumerate()
                    .filter(|&(j, p)| j != i && !p.returning && !is_unset(&unset, p.id))
                    .map(|(_, p)| p.crtc)
                    .collect();
                let Some(o) = outputs.get_mut(i) else {
                    continue;
                };
                let was = (o.crtc, o.pipe);
                (o.crtc, o.pipe) = (crtc, pipe);
                match self.legacy_set(o, &driven) {
                    Ok(()) => {
                        if was.0 != crtc {
                            log::info!(
                                "display {} moves from CRTC {} to CRTC {crtc}",
                                o.name,
                                was.0
                            );
                        }
                        o.cursor_set = None;
                        if lit {
                            moved.push(o.id);
                        } else {
                            unset.retain(|u| u.0 != o.id);
                        }
                    }
                    Err(Failure::Test(_)) if !self.card.removed() => {
                        (o.crtc, o.pipe) = was;
                        refusals.push((o.id, pipe));
                        // The lit display still holds the CRTC the others
                        // need, so the placement cannot go on.
                        if lit {
                            break;
                        }
                    }
                    Err(_) => {
                        (o.crtc, o.pipe) = was;
                        if let Some(u) = unset.iter_mut().find(|u| u.0 == o.id) {
                            u.1 = false;
                        }
                        passing = true;
                        break;
                    }
                }
            }
            self.outputs = outputs;
            if unset.is_empty() {
                return unset;
            }
            if passing || self.card.removed() {
                refused = false;
                break;
            }
        }
        self.keep_unset_apart(&mut unset, &placed);
        if refused {
            for id in elsewhere {
                if is_unset(&unset, id) && !self.move_refused.contains(&id.0) {
                    self.move_refused.push(id.0);
                }
            }
        }
        unset
    }

    /// Puts each display of `unset`, which the search could not set, on a
    /// CRTC no other display holds: the one the placement put it on, as
    /// `placed` gives each display's index of its CRTC, else the one it is
    /// on, else the first CRTC it can take. A lit display the search moved
    /// may hold the CRTC the placement gave it. A display with no such
    /// CRTC is let go, present but dark ([`Self::keep_left_off`]), so no
    /// two displays end on one CRTC.
    fn keep_unset_apart(
        &mut self,
        unset: &mut Vec<(DisplayId, bool)>,
        placed: &[(DisplayId, usize)],
    ) {
        let mut i = 0;
        while let Some(o) = self.outputs.get(i) {
            if !unset.iter().any(|u| u.0 == o.id) {
                i = i.saturating_add(1);
                continue;
            }
            let held = |pipe: usize| {
                self.outputs
                    .iter()
                    .enumerate()
                    .any(|(j, p)| j != i && p.pipe == pipe)
            };
            let able = o.able;
            let pipe = placed
                .iter()
                .find(|p| p.0 == o.id)
                .map(|p| p.1)
                .into_iter()
                .chain([o.pipe])
                .chain((0..self.crtcs.len()).filter(|&c| able & index_bit(c) != 0))
                .find(|&p| !held(p));
            match pipe.and_then(|p| Some((p, *self.crtcs.get(p)?))) {
                Some((pipe, crtc)) => {
                    if let Some(o) = self.outputs.get_mut(i) {
                        (o.crtc, o.pipe) = (crtc, pipe);
                    }
                    i = i.saturating_add(1);
                }
                None => {
                    let o = self.outputs.remove(i);
                    unset.retain(|u| u.0 != o.id);
                    let why = format!("no free CRTC for {}", o.name);
                    self.let_go(o, &why);
                }
            }
        }
    }
}
