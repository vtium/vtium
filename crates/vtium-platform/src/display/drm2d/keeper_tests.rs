//! drm2d with a keeper of its DRM master, as the drawer's cards have under
//! the privilege split: the card never sets or drops master itself, asks
//! the keeper at each wake-up, and draws, commits test-only and pans while
//! its open file holds the master the keeper took.
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker holds DRM master for
//!   the drawer.

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::display::fake::FakeCard;
use crate::display::mode::mode;

const A: u32 = 41;

/// What the keeper was asked, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Take,
    LetGo,
}

/// A keeper that records each call and answers as the test sets it.
#[derive(Clone, Default)]
struct Keeper {
    asked: Rc<RefCell<Vec<Asked>>>,
    refuse: Rc<Cell<bool>>,
}

impl MasterKeeper for Keeper {
    fn take(&self) -> io::Result<()> {
        self.asked.borrow_mut().push(Asked::Take);
        if self.refuse.get() {
            return Err(Errno::BUSY.into());
        }
        Ok(())
    }

    fn let_go(&self) -> io::Result<()> {
        self.asked.borrow_mut().push(Asked::LetGo);
        Ok(())
    }
}

/// A card opened while no file holds master, whose own SET_MASTER would
/// therefore succeed, with a keeper.
fn kept() -> (Drm2d<FakeCard>, Keeper) {
    let card = FakeCard::new(&[(A, vec![mode(640, 480, 60, true)])]);
    card.state().master = false;
    let mut v = Drm2d::with(card, ModePolicy::default()).unwrap();
    let keeper = Keeper::default();
    v.set_master_keeper(Some(Box::new(keeper.clone())));
    (v, keeper)
}

fn flip(v: &mut Drm2d<FakeCard>) {
    v.card().complete_flips();
    v.dispatch().unwrap();
}

/// A wake-up asks the keeper and never the kernel: with the keeper
/// refusing, the wake-up fails and the file stays without master, though
/// the card's own call would have taken it. The next wake-up asks the
/// keeper again, and once the keeper's process has set master on its copy
/// of the file, the display lights, with a test-only commit first. A sleep
/// lets go through the keeper and never drops master itself.
#[test]
fn a_kept_card_takes_and_lets_go_of_master_only_through_its_keeper() {
    let (mut v, keeper) = kept();
    keeper.refuse.set(true);
    assert!(v.wake_up().is_err());
    assert!(
        !v.card().state().master,
        "the card never took master itself"
    );
    assert_eq!(*keeper.asked.borrow(), [Asked::Take]);

    // The broker sets master on its copy: the open file holds it.
    keeper.refuse.set(false);
    v.card().state().master = true;
    let commits = v.card().state().commits.len();
    v.wake_up().unwrap();
    flip(&mut v);
    assert_eq!(*keeper.asked.borrow(), [Asked::Take, Asked::Take]);
    let s = v.card().state();
    assert!(
        s.commits[commits..].iter().any(|c| c.test_only()),
        "the modeset was tested first"
    );
    assert!(
        s.commits[commits..].iter().any(|c| !c.test_only()),
        "the modeset was committed"
    );
    drop(s);
    assert!(v.events().any(|e| e == Event::Awake));

    v.sleep();
    flip(&mut v);
    assert_eq!(
        *keeper.asked.borrow(),
        [Asked::Take, Asked::Take, Asked::LetGo]
    );
    let s = v.card().state();
    assert_eq!(s.master_drops, 0, "the card never dropped master itself");
    assert_eq!(s.master_drops_refused, 0);
}

/// After a release, the keeper's process dropped master on its copy, so
/// the next wake-up asks the keeper again, even where drm2d last knew the
/// file as master: it asks the kernel first.
#[test]
fn a_wake_up_asks_the_keeper_after_its_process_dropped_master() {
    let (mut v, keeper) = kept();
    v.card().state().master = true;
    v.wake_up().unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    assert_eq!(*keeper.asked.borrow(), [Asked::Take]);
    // The broker drops master at a release the drawer never answered.
    v.card().state().master = false;
    keeper.refuse.set(true);
    let _ = v.wake_up();
    assert_eq!(*keeper.asked.borrow(), [Asked::Take, Asked::Take]);
}

/// The drawer's display pans while its file holds the master the keeper
/// took: a test-only commit tests the pan, and each frame's commit moves
/// the plane's source rectangle.
#[cfg(feature = "scroll-pan")]
#[test]
fn a_kept_card_pans() {
    use crate::display::Tall;
    let (mut v, _keeper) = kept();
    v.card().state().master = true;
    v.wake_up().unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    let commits = v.card().state().commits.len();
    v.set_tall(
        DisplayId(A),
        Some(Tall {
            ring: 480,
            turned: false,
        }),
    )
    .unwrap();
    flip(&mut v);
    v.events().for_each(drop);
    assert!(v.pans(DisplayId(A)), "the pan's test-only commit passed");
    assert!(
        v.card().state().commits[commits..]
            .iter()
            .any(|c| c.test_only()),
        "the pan was tested"
    );
    let mut token = v.lend(DisplayId(A)).expect("a token");
    assert!(token.set_window(100));
    token.draw(|c| c.pixels[0] = 1).expect("its memory");
    v.swap(token, &[]).expect("the frame is swapped");
    let plane = v.outputs[0].planes.as_ref().unwrap().primary;
    let last = v.card().last_commit().unwrap();
    assert_eq!(last.get(plane, "SRC_Y").map(|y| y >> 16), Some(100));
}
