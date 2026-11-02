//! Pictures in each console (ADR-1100 D5, D7, D9, D10, D11).
//!
//! Six keys set how a console takes pictures ([`Settings`]). Every build
//! reads them, as the C reference read its image options in every build
//! (`src/config.c:1221-1247` at kmscon b1f7a7c5); a build without the
//! `images` feature then draws no picture, and the rest of this module is
//! not built.
//!
//! In a build with `images`, each console's terminal gets the sixel and
//! kitty graphics handlers when it is made ([`install`]). At each turn of
//! the loop the head gives each of its consoles what the head's state
//! decides ([`crate::head::Head::image_turn`]):
//!
//! - **The budget.** `image-storage-limit`, or under `auto` four frames
//!   of the head's displays, which the head works out again whenever a
//!   display joins or leaves it or changes size.
//! - **The offering rule.** A head switches sixel and kitty graphics on
//!   while the protocol's key is on and every display of the head draws
//!   pictures, as the C reference turned both off while any display of a
//!   terminal could not draw them (`src/render/image_budget.c:449-457,
//!   505-514`). A head with no display yet switches them on, as the C
//!   reference's did. vtium-term offers a protocol switched on only while
//!   the budget is above 0, so a budget of 0 turns both off.
//! - **The cell size** the console's pictures are placed in: the shown
//!   console's is the head's cell, and a hidden console keeps its own, as
//!   the C reference gave libtsm the font's cell whenever it changed
//!   (`src/terminal.c:1147-1162`).
//!
//! Each of these is a host event: the head gives it between two input
//! calls, so what a console's store holds follows from its bytes and its
//! host events in order (ADR-1100 D5).
//!
//! **Requests.** A handler that needs the image helper, for a file, shared
//! memory, PNG or zlib, leaves a request on its terminal, and the input
//! call stops after the string that made it. The head then takes the
//! request, sends it to the console's [`Helper`], pauses the console's
//! pty, and starts the reply timer, [`REPLY_TIMEOUT`]. The reply, its
//! sizes checked against the request, or `FAILED` once the timer fires,
//! goes back to the terminal, the replies it writes go to the program,
//! and the pty resumes. Any failure, the timer, a `FAILED` reply or one
//! that fails its checks, stops the helper (ADR-1101 D8). The C reference did
//! the same: it paused the pty when it sent a request and resumed it after
//! the reply (`src/terminal_image.c:58-92, 100-122`).
//!
//! **No helper yet.** The image helper is its own program (ADR-1101),
//! which vtium does not build yet. Until it does, [`crate::app::App`]
//! holds no [`Helpers`], and every terminal is told that no helper runs,
//! so the kitty handler answers a file, shared-memory or PNG request at
//! once with the reply ADR-1100 D7 gives for "no helper", as the C
//! reference answered a request no helper could take
//! (`src/terminal_image.c:84-89`). The helper plugs in through
//! [`crate::app::App::set_image_helpers`].

use crate::config::{Config, keys};

/// What the image keys give (ADR-1100 D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// `image-storage-limit` in bytes; `None` for `auto`.
    pub limit: Option<u64>,
    /// `max-graphic-size`, the largest picture each side.
    pub graphic: (u32, u32),
    /// `num-color-registers`.
    pub registers: u32,
    /// `private-color-registers`.
    pub private_registers: bool,
    /// `sixel`.
    pub sixel: bool,
    /// `kitty-graphics`.
    pub kitty: bool,
}

/// `max-graphic-size`'s default.
const GRAPHIC_DEFAULT: (u32, u32) = (10_000, 10_000);

/// `num-color-registers`' default.
const REGISTERS_DEFAULT: u32 = 1024;

impl Settings {
    /// The image keys `config` sets.
    pub fn read(config: &Config) -> Settings {
        Settings {
            limit: config.number(&keys::IMAGE_STORAGE_LIMIT),
            graphic: config
                .size(&keys::MAX_GRAPHIC_SIZE)
                .unwrap_or(GRAPHIC_DEFAULT),
            registers: config
                .number(&keys::NUM_COLOR_REGISTERS)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(REGISTERS_DEFAULT),
            private_registers: config.switch(&keys::PRIVATE_COLOR_REGISTERS),
            sixel: config.switch(&keys::SIXEL),
            kitty: config.switch(&keys::KITTY_GRAPHICS),
        }
    }
}

#[cfg(feature = "images")]
pub use built::*;

#[cfg(feature = "images")]
mod built {
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use vtium_image::decode::{Reply, Request};
    use vtium_image::{Kitty, Limits, Sixel, Switches};
    use vtium_platform::pty::Pty;
    use vtium_term::{ImageString, Terminal};

    use super::Settings;
    use crate::term::Term;

    /// How long the image helper has to answer a request before the
    /// request fails, the one place time enters the image pipeline
    /// (ADR-1100 D7, ADR-1101 D8): 2,000 ms, the C reference's.
    pub const REPLY_TIMEOUT: Duration = Duration::from_millis(2000);

    /// The image helper as one console sees it: it takes one request at a
    /// time and answers it later (ADR-1101). Dropping it stops the helper.
    ///
    /// The console sends a request with [`Helper::send`] and asks at each
    /// turn of the loop for the reply with [`Helper::reply`]. The helper
    /// checks nothing a reply's sizes need; the terminal checks them
    /// against the request ([`vtium_image::decode::Reply::checked`]).
    pub trait Helper {
        /// Sends `request`, the console's one request out.
        fn send(&mut self, request: Request);
        /// The reply to the request out, once it has come.
        fn reply(&mut self) -> Option<Reply>;
    }

    /// What starts a console's helper, at the console's first request. The
    /// app holds one while an image helper is built; until then it holds
    /// none, and every terminal is told no helper runs.
    pub type Helpers = Rc<dyn Fn() -> Box<dyn Helper>>;

    /// The handlers a console's terminal takes pictures through, and the
    /// limits `settings` give them. The budget and the switches come at
    /// the head's next image turn: until then the store holds nothing, so
    /// nothing is offered.
    pub(crate) fn install(terminal: &mut Terminal, settings: &Settings) {
        terminal.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        terminal.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        terminal.set_image_limits(Limits {
            graphic: settings.graphic,
            registers: settings.registers,
            private_registers: settings.private_registers,
        });
        terminal.set_image_helper(false);
    }

    /// What a head gives each of its consoles at an image turn.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct Given {
        /// The budget, in bytes.
        pub(crate) limit: u64,
        pub(crate) switches: Switches,
        /// The cell size pictures are placed in.
        pub(crate) cell: (u32, u32),
        /// An image helper answers the console's requests.
        pub(crate) helper: bool,
    }

    /// The budget and switches `settings` give a head whose displays are
    /// `sizes` in pixels and which `draw` pictures, every one of them
    /// (ADR-1100 D5, D10). vtium-term offers no protocol while the budget
    /// is 0, whatever its switch says.
    pub(crate) fn offer(
        settings: &Settings,
        sizes: impl IntoIterator<Item = (u32, u32)>,
        draw: bool,
    ) -> (u64, Switches) {
        let limit = settings
            .limit
            .unwrap_or_else(|| vtium_image::auto_limit(sizes));
        let switches = Switches {
            sixel: settings.sixel && draw,
            kitty: settings.kitty && draw,
            iterm2: false,
        };
        (limit, switches)
    }

    /// One console's side of the image pipeline: what it was last given,
    /// its helper, and the request out.
    #[derive(Default)]
    pub(crate) struct Pictures {
        given: Option<Given>,
        helper: Option<Box<dyn Helper>>,
        /// The request out, and when it fails unanswered.
        out: Option<(Request, Instant)>,
    }

    impl Pictures {
        /// Gives `term` the cell `cell` for its pictures at once, as a
        /// resize gives the cell size, before the next image turn.
        pub(crate) fn give_cell(&mut self, term: &mut Term, cell: (u32, u32)) {
            if let Some(given) = self.given {
                self.give(term, Given { cell, ..given });
            }
        }

        /// Gives `term` what `given` holds, where it changed.
        pub(crate) fn give(&mut self, term: &mut Term, given: Given) {
            let old = self.given.replace(given);
            if old == Some(given) {
                return;
            }
            let terminal = &mut term.terminal;
            if old.is_none_or(|o| o.cell != given.cell) {
                let (w, h) = given.cell;
                if terminal.screen_mut().set_image_cell(w, h).is_err() {
                    log::warn!("pictures cannot be placed in cells of {w}x{h}");
                }
            }
            if old.is_none_or(|o| o.limit != given.limit) {
                terminal.screen_mut().set_image_limit(given.limit);
            }
            if old.is_none_or(|o| o.switches != given.switches) {
                terminal.set_image_switches(given.switches);
            }
            if old.is_none_or(|o| o.helper != given.helper) {
                terminal.set_image_helper(given.helper);
            }
        }

        /// Sends the request `term` made to the helper `helpers` starts,
        /// pausing `pty`, and hands back the reply once it came or the
        /// timer fired at `now`. Returns whether a reply was handed back.
        pub(crate) fn route(
            &mut self,
            term: &mut Term,
            pty: Option<&mut Pty>,
            now: Instant,
            helpers: Option<&Helpers>,
        ) -> bool {
            let mut pty = pty;
            if let Some(request) = term.terminal.take_image_request() {
                if self.helper.is_none() {
                    self.helper = helpers.map(|start| start());
                }
                let Some(helper) = &mut self.helper else {
                    // A terminal told no helper runs makes no request.
                    return self.answer(term, pty, Reply::Failed);
                };
                helper.send(request.clone());
                self.out = Some((request, now.checked_add(REPLY_TIMEOUT).unwrap_or(now)));
                if let Some(pty) = pty.as_deref_mut() {
                    pty.pause();
                }
            }
            let Some((request, due)) = &self.out else {
                return false;
            };
            let reply = match self.helper.as_mut().and_then(|h| h.reply()) {
                // Its sizes checked against the request, as the terminal
                // checks them again.
                Some(reply) => reply.checked(request),
                None if now >= *due => Reply::Failed,
                None => return false,
            };
            if reply == Reply::Failed {
                // Any failure stops the helper: the timer, a lost or
                // failing socket, or a reply that fails its checks. The
                // next request starts another (ADR-1101 D8).
                self.helper = None;
            }
            self.answer(term, pty, reply)
        }

        /// Hands `reply` to `term`, writes the replies it makes to the
        /// program and resumes `pty`.
        fn answer(&mut self, term: &mut Term, pty: Option<&mut Pty>, reply: Reply) -> bool {
            self.out = None;
            term.terminal.image_reply(reply);
            let bytes = term.terminal.take_replies();
            if let Some(pty) = pty {
                if !bytes.is_empty() {
                    pty.reply(&bytes);
                }
                pty.resume();
            }
            true
        }

        /// The console's session ended: the request out is dropped with no
        /// reply and the helper is stopped (ADR-1101 D8), so the next
        /// session starts with none out, as the C reference reset its
        /// parser and cancelled the request (`src/terminal_image.c:172-178`).
        pub(crate) fn end_session(&mut self, term: &mut Term) {
            self.helper = None;
            if self.out.take().is_some() {
                term.terminal.image_reply(Reply::Failed);
                term.terminal.take_replies();
            }
        }

        /// When the request out fails unanswered.
        pub(crate) fn due(&self) -> Option<Instant> {
            self.out.as_ref().map(|(_, due)| *due)
        }
    }

    impl crate::head::Head {
        /// The image turn at `now`: each console gets the budget, the
        /// switches and the cell size the head's state gives, and the
        /// request each one made goes to its helper, which `helpers`
        /// starts, or its reply comes back. A reply the shown console takes
        /// draws the head at the turn's end, as the picture it placed or the
        /// answer it wrote changed the screen.
        pub(crate) fn image_turn(&mut self, now: Instant, helpers: Option<&Helpers>) {
            let draw = self.views.values().all(crate::term::View::draws_placements);
            let sizes = self.views.values().map(|v| v.size);
            let (limit, switches) = offer(&self.image_settings, sizes, draw);
            let given = |cell| Given {
                limit,
                switches,
                cell,
                helper: helpers.is_some(),
            };
            let shown = &mut self.console;
            shown.pictures.give(&mut shown.term, given(self.cell));
            if shown
                .pictures
                .route(&mut shown.term, shown.pty.as_mut(), now, helpers)
            {
                self.draw_now = true;
            }
            for console in &mut self.hidden {
                console
                    .pictures
                    .give(&mut console.term, given(console.fit.cell));
                console
                    .pictures
                    .route(&mut console.term, console.pty.as_mut(), now, helpers);
            }
        }

        /// When each request out of the head's consoles fails unanswered.
        pub(crate) fn image_due(&self) -> impl Iterator<Item = Instant> + '_ {
            self.consoles().filter_map(|c| c.pictures.due())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn settings() -> Settings {
            Settings {
                limit: None,
                graphic: (10_000, 10_000),
                registers: 1024,
                private_registers: true,
                sixel: true,
                kitty: true,
            }
        }

        #[test]
        fn auto_is_four_frames_of_the_heads_displays_within_its_floor_and_cap() {
            let (limit, on) = offer(&settings(), [(1920, 1080)], true);
            assert_eq!(limit, 1920 * 1080 * 4 * 4);
            assert!(on.sixel && on.kitty && !on.iterm2);
            let (limit, _) = offer(&settings(), [(1920, 1080), (1920, 1080)], true);
            assert_eq!(limit, 2 * 1920 * 1080 * 4 * 4);
            let (limit, _) = offer(&settings(), [], true);
            assert_eq!(limit, vtium_image::AUTO_FLOOR);
            let (limit, _) = offer(&settings(), [(7680, 4320); 4], true);
            assert_eq!(limit, vtium_image::AUTO_CAP);
        }

        #[test]
        fn a_head_switches_nothing_on_with_a_display_that_draws_no_picture() {
            let set = |limit| Settings {
                limit: Some(limit),
                ..settings()
            };
            let off = Switches {
                sixel: false,
                kitty: false,
                iterm2: false,
            };
            assert_eq!(offer(&set(1 << 20), [(640, 480)], false), (1 << 20, off));
            let (_, on) = offer(&set(1 << 20), [(640, 480)], true);
            assert!(on.sixel && on.kitty);
            let sixel_only = Settings {
                kitty: false,
                ..set(1 << 20)
            };
            let (_, on) = offer(&sixel_only, [(640, 480)], true);
            assert!(on.sixel && !on.kitty);
        }
    }
}
