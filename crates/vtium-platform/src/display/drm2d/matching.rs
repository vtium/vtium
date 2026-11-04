//! Placing displays on CRTCs: which CRTC each display takes, given the
//! CRTCs each can take and the one each wants ([`super::Drm2d::place`]).

/// What placing one display on a CRTC goes by: the group it is placed
/// in, the groups taken in order, with the lit displays in group 0; and
/// the CRTC it wants, and the CRTCs it can take, by index into the card's
/// list of CRTCs.
#[derive(Clone, Copy, Debug)]
pub(super) struct Place {
    pub(super) group: u8,
    pub(super) want: Option<usize>,
    pub(super) able: u32,
}

/// The bit of the CRTC at `index` in the card's list, as a mask of
/// possible CRTCs names it; 0 past the 32 such a mask holds.
pub(super) fn index_bit(index: usize) -> u32 {
    u32::try_from(index)
        .ok()
        .and_then(|i| 1u32.checked_shl(i))
        .unwrap_or(0)
}

/// Each display's CRTC, by index into the card's `crtcs` CRTCs, or `None`
/// for a display left without one ([`super::Drm2d::place`]).
///
/// The displays are placed one at a time, group by group, each group in
/// its order in `places`, and a display placed stays placed: a later one
/// may move it to another CRTC it can take, but never takes its CRTC from
/// it. So where the card has too few CRTCs, the displays placed first
/// keep theirs.
///
/// Of the placements of the displays it places, the matching takes one
/// that costs the least ([`cost`]): it moves as few lit displays off the
/// CRTC they want as it can, and then puts as few of the others off the
/// CRTC they want as it can. Each display is placed along the cheapest
/// chain of moves that frees a CRTC for it ([`augment`]), which is the
/// Hungarian method's step: placed that way, one display at a time, the
/// placement always costs the least for the displays it holds. A display
/// placed later that wants a CRTC an earlier chain ended on, as a monitor
/// that returns wants the CRTC it held, therefore gets that CRTC wherever
/// the cost allows: its own chain moves the display there on.
pub(super) fn matching(places: &[Place], crtcs: usize) -> Vec<Option<usize>> {
    let mut order: Vec<usize> = (0..places.len()).collect();
    order.sort_by_key(|&d| places.get(d).map(|p| p.group));
    let mut slot = vec![None; places.len()];
    let mut owner = vec![None; crtcs];
    for d in order {
        augment(d, places, &mut slot, &mut owner);
    }
    slot
}

/// What placing the display `p` on the CRTC at `c` costs: nothing on the
/// CRTC it wants; on any other, `heavy` for a lit display, those in group
/// 0, and 1 for any other. `heavy` is more than all the displays that are
/// not lit can cost together, so the costs order placements by the lit
/// displays put off the CRTC they want first.
fn cost(p: &Place, c: usize, heavy: i64) -> i64 {
    if p.want == Some(c) {
        0
    } else if p.group == 0 {
        heavy
    } else {
        1
    }
}

/// Puts display `d` on the CRTC at `c`, which the display placed there
/// before, if any, has already left.
fn assign(d: usize, c: usize, slot: &mut [Option<usize>], owner: &mut [Option<usize>]) {
    if let Some(old) = slot.get(d).copied().flatten()
        && let Some(held) = owner.get_mut(old)
        && *held == Some(d)
    {
        *held = None;
    }
    if let Some(at) = slot.get_mut(d) {
        *at = Some(c);
    }
    if let Some(held) = owner.get_mut(c) {
        *held = Some(d);
    }
}

/// Places display `d` along the cheapest chain of moves that ends on a
/// free CRTC: `d` takes a CRTC it can take, the display there moves to
/// another CRTC it can take, and so on until one moves to a free CRTC.
/// Whether it placed `d`; where it did not, nothing moved.
///
/// A chain's cost is what it adds to the placement's: `d`'s cost on the
/// CRTC it takes, and for each display it moves, its cost on the new CRTC
/// less its cost on the old ([`cost`]). A move back to the CRTC a display
/// wants lowers the cost. Of chains that cost the same, it takes the one
/// that moves the fewest displays, then the one that ends on the CRTC
/// first in the card's list.
///
/// The search is Bellman-Ford's over the CRTCs, since a move can cost less
/// than nothing. The placement before the step costs the least for the
/// displays it holds, so no chain of moves around a loop lowers the cost,
/// and the search settles within a pass for each CRTC.
fn augment(
    d: usize,
    places: &[Place],
    slot: &mut [Option<usize>],
    owner: &mut [Option<usize>],
) -> bool {
    let crtcs = owner.len();
    let heavy = i64::try_from(places.len())
        .unwrap_or(i64::MAX)
        .saturating_add(1);
    let Some(p) = places.get(d) else {
        return false;
    };
    // For each CRTC reached, what freeing it costs and how many displays
    // that moves, and the CRTC whose display moves to it, `None` where `d`
    // takes it itself.
    let mut best: Vec<Option<(i64, usize, Option<usize>)>> = vec![None; crtcs];
    for (c, at) in best.iter_mut().enumerate() {
        if p.able & index_bit(c) != 0 {
            *at = Some((cost(p, c, heavy), 0, None));
        }
    }
    for _ in 0..crtcs {
        let mut changed = false;
        for c in 0..crtcs {
            let (Some((spent, moves, _)), Some(Some(moving))) =
                (best.get(c).copied().flatten(), owner.get(c).copied())
            else {
                continue;
            };
            let Some(m) = places.get(moving) else {
                continue;
            };
            let leave = cost(m, c, heavy);
            for to in (0..crtcs).filter(|&to| to != c && m.able & index_bit(to) != 0) {
                let next = (
                    spent
                        .saturating_add(cost(m, to, heavy))
                        .saturating_sub(leave),
                    moves.saturating_add(1),
                );
                if let Some(at) = best.get_mut(to)
                    && at.is_none_or(|(was, was_moves, _)| next < (was, was_moves))
                {
                    *at = Some((next.0, next.1, Some(c)));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let end = (0..crtcs)
        .filter(|&c| owner.get(c) == Some(&None))
        .filter_map(|c| {
            let (spent, moves, _) = best.get(c).copied().flatten()?;
            Some((spent, moves, c))
        })
        .min();
    let Some((_, _, end)) = end else {
        return false;
    };
    // The chain, from the CRTC `d` takes to the free one, read back from
    // the free one; a chain longer than the CRTCs would mean a loop.
    let mut chain = vec![end];
    while let Some(from) = chain
        .last()
        .and_then(|&c| best.get(c).copied().flatten())
        .and_then(|(_, _, from)| from)
    {
        if chain.len() > crtcs || chain.contains(&from) {
            return false;
        }
        chain.push(from);
    }
    chain.reverse();
    // Each display moves on to the CRTC after its own, the last first, and
    // then `d` takes the first.
    for at in (1..chain.len()).rev() {
        if let (Some(&from), Some(&to)) = (chain.get(at.saturating_sub(1)), chain.get(at))
            && let Some(moving) = owner.get(from).copied().flatten()
        {
            assign(moving, to, slot, owner);
        }
    }
    if let Some(&first) = chain.first() {
        assign(d, first, slot, owner);
    }
    true
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    reason = "test arithmetic on bounded values; a test fails by panicking"
)]
mod tests {
    use super::*;

    /// What placing `d` on CRTC `c` costs: nothing on the CRTC it wants;
    /// elsewhere, more for a lit display than all the displays that are
    /// not lit cost together, and 1 for a display that is not lit.
    fn cost(places: &[Place], d: usize, c: usize) -> u64 {
        let p = places[d];
        if p.want == Some(c) {
            0
        } else if p.group == 0 {
            places.len() as u64 + 1
        } else {
            1
        }
    }

    /// The displays a placement must place, as a mask, and the least a
    /// placement of exactly them costs, found by trying every placement.
    /// A display is placed where it can be beside those placed before it,
    /// group by group, as the matching takes them.
    fn brute(places: &[Place], crtcs: usize) -> (u32, u64) {
        let n = places.len();
        let mut best = vec![None::<u64>; 1 << n];
        fn walk(
            places: &[Place],
            crtcs: usize,
            d: usize,
            used: u32,
            placed: u32,
            spent: u64,
            best: &mut [Option<u64>],
        ) {
            if d == places.len() {
                let b = &mut best[placed as usize];
                *b = Some(b.map_or(spent, |b| b.min(spent)));
                return;
            }
            walk(places, crtcs, d + 1, used, placed, spent, best);
            for c in 0..crtcs {
                if places[d].able & (1 << c) != 0 && used & (1 << c) == 0 {
                    let spent = spent + cost(places, d, c);
                    walk(
                        places,
                        crtcs,
                        d + 1,
                        used | 1 << c,
                        placed | 1 << d,
                        spent,
                        best,
                    );
                }
            }
        }
        walk(places, crtcs, 0, 0, 0, 0, &mut best);
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&d| places[d].group);
        let mut set = 0u32;
        for d in order {
            if best[(set | 1 << d) as usize].is_some() {
                set |= 1 << d;
            }
        }
        (set, best[set as usize].unwrap())
    }

    /// Checks the matching of `places` against [`brute`]: each display on
    /// a CRTC it can take, no CRTC twice, the same displays placed, and
    /// the same cost.
    fn check(places: &[Place], crtcs: usize) {
        let slots = matching(places, crtcs);
        let mut used = 0u32;
        let mut placed = 0u32;
        let mut spent = 0;
        for (d, slot) in slots.iter().enumerate() {
            if let Some(c) = *slot {
                assert!(places[d].able & (1 << c) != 0, "{places:?}: {slots:?}");
                assert!(used & (1 << c) == 0, "{places:?}: {slots:?}");
                used |= 1 << c;
                placed |= 1 << d;
                spent += cost(places, d, c);
            }
        }
        assert_eq!(
            (placed, spent),
            brute(places, crtcs),
            "{places:?}: {slots:?}"
        );
    }

    // Every placement of up to three displays on three CRTCs, each display
    // lit or not, with any CRTCs it can take and any CRTC it wants, places
    // the displays it can, group by group, and costs no more than the best.
    #[test]
    fn the_matching_places_three_displays_on_three_crtcs_at_the_least_cost() {
        let all: Vec<Place> = (0..2u8)
            .flat_map(|g| {
                (0..8u32).flat_map(move |able| {
                    (0..4usize).map(move |w| Place {
                        group: g * 2,
                        want: (w < 3).then_some(w),
                        able,
                    })
                })
            })
            .collect();
        for &a in &all {
            check(&[a], 3);
            for &b in &all {
                check(&[a, b], 3);
                for &c in &all {
                    check(&[a, b, c], 3);
                }
            }
        }
    }

    // Placements of four and five displays on four CRTCs, chosen by a
    // fixed sequence of numbers, cost no more than the best either.
    #[test]
    fn the_matching_places_four_or_five_displays_on_four_crtcs_at_the_least_cost() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |m: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % m
        };
        for _ in 0..20_000 {
            let n = 4 + next(2) as usize;
            let places: Vec<Place> = (0..n)
                .map(|_| Place {
                    group: [0, 1, 2][next(3) as usize],
                    want: [None, Some(0), Some(1), Some(2), Some(3)][next(5) as usize],
                    able: next(16) as u32,
                })
                .collect();
            check(&places, 4);
        }
    }

    // Two displays plugged in at once, where lighting both takes moving a
    // lit display: the matching moves one lit display, B to CRTC 3, not
    // both. A lit on 0 can take 0 and 2, B lit on 1 can take 1 and 3, C
    // can take 0 and 1, and D can take 0 and 2.
    #[test]
    fn two_displays_plugged_in_at_once_move_as_few_lit_displays_as_they_can() {
        let lit = |want: usize, able: u32| Place {
            group: 0,
            want: Some(want),
            able,
        };
        let new = |able: u32| Place {
            group: 2,
            want: None,
            able,
        };
        let places = [lit(0, 0b0101), lit(1, 0b1010), new(0b0011), new(0b0101)];
        assert_eq!(matching(&places, 4), [Some(0), Some(3), Some(1), Some(2)]);
    }

    // A monitor that returns gets the CRTC it wants even where a display
    // placed before it moved a lit display there. A is lit on 0 and can
    // take 0, 1 and 2; C, plugged in, can take only 0, so A moves; R, which
    // returns wanting 1, can take 1 and 2. A ends on 2, and R on 1.
    #[test]
    fn a_returning_monitor_keeps_the_crtc_it_wants_from_a_lit_display_moved_there() {
        let places = [
            Place {
                group: 0,
                want: Some(0),
                able: 0b111,
            },
            Place {
                group: 2,
                want: None,
                able: 0b001,
            },
            Place {
                group: 2,
                want: Some(1),
                able: 0b110,
            },
        ];
        assert_eq!(matching(&places, 3), [Some(2), Some(0), Some(1)]);
    }
}
