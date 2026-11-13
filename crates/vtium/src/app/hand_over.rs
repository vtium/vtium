//! Handing a head's displays to a graphical program (ADR-1401 D16). A
//! program in a console sends setBackground, and vtium gives up every card
//! that drives a display of the sending head: it stops drawing there and
//! drops DRM master, as at a VT release, so the program can open the card
//! and become its master. setForeground from the same console takes the
//! cards back. vtium reads both strings behind `allow-display-ops`
//! (ADR-1000, "Replies and policy").
//!
//! - **Who may send setBackground.** Only a console that a head shows:
//!   the head's shown console, while vtium's VT is ours and the head draws
//!   on a display whose card is not given up. vtium ignores setBackground
//!   from any other console, and logs that it did.
//! - **Under a seat manager** ([`Vt::seat_manager`]), vtium ignores
//!   setBackground and logs that it did. The hand-over acts on the
//!   kernel's VT and on the fake VT, where nothing but vtium holds the
//!   seat.
//! - **Paused heads.** A head pauses on each display that a given-up card
//!   drives, since the card draws nothing of vtium's. A head with every
//!   display on a given-up card is wholly paused: vtium delivers none of
//!   its keyboards' keys, chords or pointer events to it, and lets go of
//!   the keys its consoles hold. The VT keys, the seat's power keys and
//!   the roaming chord still act.
//! - **While a card is given up**, vtium never wakes it: a re-take of the
//!   VT, the blank's end, a card added again at the same place and the
//!   dark displays' power each pass it by. drm2d records nothing of a
//!   hotplug while asleep, and its wake-up reads every connector again.
//! - **The end.** A setForeground from the console that sent
//!   setBackground, that console's session ending, or vtium's VT shown
//!   again after a switch away ends the hand-over. At the first two,
//!   while the VT is ours, vtium puts the VT's modes back
//!   ([`Vt::restore`]) and wakes each card no other hand-over holds, whose
//!   wake-up retry goes on while the program still holds a card. At a VT
//!   acquire, the acquire wakes every card. Each head that draws again
//!   draws whole.
//!
//! [`Vt::seat_manager`]: vtium_platform::vt::Vt::seat_manager
//! [`Vt::restore`]: vtium_platform::vt::Vt::restore

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use vtium_term::DisplayRequest;

use super::App;

/// What ended a hand-over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum End {
    /// setForeground from the console that sent setBackground.
    Foreground,
    /// That console's session ended.
    SessionEnded,
    /// vtium's VT is shown again after a switch away.
    VtShown,
}

/// A hand-over in force: the console that sent setBackground, by its head
/// and its id, and the cards vtium gave up for it.
#[derive(Debug)]
struct HandOver {
    head: usize,
    console: u64,
    cards: BTreeSet<PathBuf>,
}

/// The hand-overs in force. Two heads on different cards may each hand
/// theirs over; a card stays given up while any hand-over holds it.
#[derive(Debug, Default)]
pub(super) struct HandOvers(Vec<HandOver>);

impl HandOvers {
    /// Whether a hand-over holds the card at `node`.
    pub(super) fn given_up(&self, node: &Path) -> bool {
        self.0.iter().any(|o| o.cards.contains(node))
    }

    /// The hand-over that console `console` of head `head` began.
    fn by(&self, head: usize, console: u64) -> Option<usize> {
        self.0
            .iter()
            .position(|o| o.head == head && o.console == console)
    }
}

impl App {
    /// Whether head `h` is wholly paused: it has a display, and a
    /// hand-over gave up the card of each one.
    pub(super) fn wholly_paused(&self, h: usize) -> bool {
        self.heads.get(h).is_some_and(|head| {
            !head.views.is_empty()
                && head
                    .views
                    .keys()
                    .all(|(node, _)| self.hand_overs.given_up(node))
        })
    }

    /// The heads wholly paused now.
    pub(super) fn paused_heads(&self) -> BTreeSet<usize> {
        (0..self.heads.len())
            .filter(|&h| self.wholly_paused(h))
            .collect()
    }

    /// Whether head `h` shows its console `id`: it is the head's shown
    /// console, vtium's VT is ours, and the head draws on a display whose
    /// card no hand-over holds.
    fn shows(&self, h: usize, id: u64) -> bool {
        self.vt.is_active()
            && self.heads.get(h).is_some_and(|head| {
                head.console.id == id
                    && head
                        .views
                        .keys()
                        .any(|(node, _)| !self.hand_overs.given_up(node))
            })
    }

    /// The setBackground and setForeground that head `h`'s consoles sent
    /// this turn, each taken from its terminal, shown or hidden, so that a
    /// hidden console's output never waits on one.
    pub(super) fn display_requests(&mut self, h: usize) {
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let requests: Vec<(u64, DisplayRequest)> = head
            .consoles_mut()
            .filter_map(|c| c.term.terminal.take_display_request().map(|r| (c.id, r)))
            .collect();
        for (id, request) in requests {
            match request {
                DisplayRequest::Background => self.set_background(h, id),
                DisplayRequest::Foreground => self.set_foreground(h, id),
            }
        }
    }

    /// The name of head `h` in a log line.
    fn head_name(&self, h: usize) -> String {
        self.heads
            .get(h)
            .map_or_else(String::new, |head| head.name().to_owned())
    }

    /// setBackground from console `id` of head `h`.
    fn set_background(&mut self, h: usize, id: u64) {
        let name = self.head_name(h);
        if self.vt.seat_manager() {
            log::info!(
                "head {name}: console {id} sent setBackground, and vtium ignores it: a seat manager gives vtium its seat, so a program in a console cannot take the display. Run the graphical program in a session of its own on another VT."
            );
            return;
        }
        if self.hand_overs.by(h, id).is_some() {
            log::debug!(
                "head {name}: console {id} sent setBackground again while its hand-over lasts"
            );
            return;
        }
        if !self.shows(h, id) {
            log::info!(
                "head {name}: console {id} sent setBackground, and vtium ignores it: only the console that a head shows may hand the display over"
            );
            return;
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let cards: BTreeSet<PathBuf> = head.views.keys().map(|(node, _)| node.clone()).collect();
        let before = self.paused_heads();
        for node in &cards {
            if !self.hand_overs.given_up(node)
                && let Some(video) = self.videos.get_mut(node)
            {
                video.sleep();
            }
        }
        log::info!(
            "head {name}: console {id} sent setBackground, so vtium gives up {} until setForeground, the end of the console's session or a switch back to this VT",
            list(&cards)
        );
        self.hand_overs.0.push(HandOver {
            head: h,
            console: id,
            cards,
        });
        let after = self.paused_heads();
        self.pause_changed(&before, &after);
    }

    /// setForeground from console `id` of head `h`: it ends the hand-over
    /// that console began, and vtium ignores it from any other console.
    fn set_foreground(&mut self, h: usize, id: u64) {
        match self.hand_overs.by(h, id) {
            Some(i) => self.end_hand_over(i, End::Foreground),
            None => log::debug!(
                "head {}: console {id} sent setForeground, which ends no hand-over",
                self.head_name(h)
            ),
        }
    }

    /// The session of console `id` of head `h` ended, or the console
    /// closed: the hand-over it began ends.
    pub(super) fn hand_over_session_ended(&mut self, h: usize, id: u64) {
        if let Some(i) = self.hand_overs.by(h, id) {
            self.end_hand_over(i, End::SessionEnded);
        }
    }

    /// vtium's VT is shown again: every hand-over ends. The acquire wakes
    /// every card.
    pub(super) fn hand_over_vt_shown(&mut self) {
        while !self.hand_overs.0.is_empty() {
            self.end_hand_over(0, End::VtShown);
        }
    }

    /// Ends hand-over `i`, for `why`.
    fn end_hand_over(&mut self, i: usize, why: End) {
        let before = self.paused_heads();
        let over = self.hand_overs.0.remove(i);
        let name = self.head_name(over.head);
        let why_text = match why {
            End::Foreground => "console {id} sent setForeground",
            End::SessionEnded => "the session of console {id} ended",
            End::VtShown => "this VT is shown again",
        }
        .replace("{id}", &over.console.to_string());
        let wake: Vec<PathBuf> = over
            .cards
            .into_iter()
            .filter(|node| !self.hand_overs.given_up(node))
            .collect();
        log::info!(
            "head {name}: {why_text}, so vtium takes back {}",
            list(&wake)
        );
        // At a VT acquire the acquire wakes every card, and while the VT is
        // another's the next acquire does.
        if why != End::VtShown && self.vt.is_active() {
            // The program may have changed the VT's modes while it drew.
            if let Err(e) = self.vt.restore() {
                log::warn!("cannot put the VT's modes back after the hand-over: {e}");
            }
            for node in &wake {
                if let Some(video) = self.videos.get_mut(node)
                    && let Err(e) = video.wake_up()
                {
                    log::warn!(
                        "cannot wake {} after the hand-over: {e}; vtium tries again",
                        node.display()
                    );
                }
            }
            self.darken();
        }
        let after = self.paused_heads();
        self.pause_changed(&before, &after);
    }

    /// The heads wholly paused were `before`, and are `after`. A head
    /// that pauses ends its pointer's drag and lets go of the keys its
    /// consoles hold. A head that draws again draws each display whole.
    fn pause_changed(&mut self, before: &BTreeSet<usize>, after: &BTreeSet<usize>) {
        for &h in after.difference(before) {
            if let Some(head) = self.heads.get_mut(h) {
                head.pointer.drag = None;
            }
        }
        for &h in before.difference(after) {
            if let Some(head) = self.heads.get_mut(h) {
                for v in head.views.values_mut() {
                    v.lost = true;
                }
                head.draw_now = true;
            }
        }
        self.let_go_of_held_keys();
    }
}

/// The cards at `nodes`, for a log line.
fn list<'a>(nodes: impl IntoIterator<Item = &'a PathBuf>) -> String {
    let names: Vec<String> = nodes.into_iter().map(|n| n.display().to_string()).collect();
    if names.is_empty() {
        "no card".to_owned()
    } else {
        names.join(", ")
    }
}
