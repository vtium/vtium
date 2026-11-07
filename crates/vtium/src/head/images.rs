//! Pictures in each console (ADR-1100 D5, D7, D9, D10, D11).
//!
//! Six keys set how a console takes pictures ([`Settings`]). Every build
//! reads them, so one vtium.conf serves every build; a build without the
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
//!   pictures, so a program is never offered a protocol that one of the
//!   displays showing it cannot draw. A head with no display yet switches
//!   them on. vtium-term offers a protocol switched on only while
//!   the budget is above 0, so a budget of 0 turns both off.
//! - **The cell size** the console's pictures are placed in: the shown
//!   console's is the head's cell, and a hidden console keeps its own,
//!   since only the shown console draws in the head's font.
//!
//! Each of these is a host event: the head gives it between two input
//! calls, so what a console's store holds follows from its bytes and its
//! host events in order (ADR-1100 D5).
//!
//! **Requests.** A handler that needs the image helper, for a file, shared
//! memory, PNG or zlib, leaves a request on its terminal, and the input
//! call stops after the string that made it. Each console has one
//! [`Helper`], vtium-platform's image host, for as long as the console
//! lives, so the crashes its session counts last as long as the session
//! (ADR-1101 D4, D8). The head hands the host each request with the
//! console's session: the login child and the pty's owner, read again
//! for each request (ADR-1101 D2). The host either answers at once, and
//! the answer goes straight back to the terminal with the pty never
//! paused, or sends the request: then the head pauses the console's pty,
//! watches the host's socket in the loop, so the loop wakes when the
//! reply arrives, and starts the reply timer, [`REPLY_TIMEOUT`]. The
//! reply, its sizes checked against the request, or `FAILED` once the
//! timer fires, goes back to the terminal, the replies it writes go to the
//! program, and the pty resumes. The host stops its helper itself on a
//! failed reply and counts the crash; on the timer the head stops it and
//! nothing is counted. While a console's host cannot start a helper, with
//! no program or past the crash limit, its terminal is told that no
//! helper runs, so the kitty handler answers such a request at once with
//! the reply ADR-1100 D7 gives for "no helper".
//!
//! **Notes.** A handler that drops or ignores what a program sent says
//! why, and vtium-term hands the host a note the first time each
//! protocol's reason comes and again each time its count reaches a power
//! of ten, so a program that sends a bad picture every frame costs the
//! journal a line for each tenfold. At each image turn the head writes
//! each console's notes to vtium's log under the target `image`, with the
//! head's name and the console's id ([`log_notes`]). A note whose reason
//! the operator can act on, a setting, a bound, the budget, memory or the
//! image helper refusing a picture that then never draws, goes out at info
//! level, which vtium shows by default. A note about what the program sent
//! breaking its protocol goes out at debug level, since only the program's
//! author can act on it.
//!
//! [`crate::app::App`] opens the program `image-helper` names once at
//! start and makes each console's host from it
//! ([`crate::app::App::set_image_helpers`] puts a test's stand-in in its
//! place).

use crate::config::{Config, keys};

/// The image helper's program vtium is built with, `image-helper`'s
/// default, set at build time with `VTIUM_IMAGE`: the key table's, as
/// vtium-platform's `image_helper::PROGRAM` is, which a test holds equal
/// to this.
pub const PROGRAM: &str = vtium_settings::keys::IMAGE_PROGRAM;

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

/// The program `config`'s `image-helper` names. Every build reads the key,
/// as it reads the other image keys; a build without images starts no
/// helper (ADR-1101 D1).
pub fn program(config: &Config) -> std::path::PathBuf {
    std::path::PathBuf::from(config.text(&keys::IMAGE_HELPER).unwrap_or(PROGRAM))
}

#[cfg(feature = "images")]
pub use built::*;

#[cfg(feature = "images")]
mod built {
    use std::os::fd::OwnedFd;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use vtium_image::decode::{Reply, Request};
    use vtium_image::{Cause, Iterm2, Kitty, Limits, Sixel, Switches};
    use vtium_platform::calloop::generic::Generic;
    use vtium_platform::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
    use vtium_platform::image_helper::{ImageHost, Program, Session};
    use vtium_platform::pty::Pty;
    use vtium_term::{ImageString, Terminal};

    use super::Settings;
    use crate::app::App;
    use crate::term::Term;

    /// How long the image helper has to answer a request before the
    /// request fails, the one place time enters the image pipeline
    /// (ADR-1100 D7, ADR-1101 D8): 2,000 ms.
    pub const REPLY_TIMEOUT: Duration = Duration::from_millis(2000);

    /// One console's image helper host, vtium-platform's [`ImageHost`] or
    /// a test's stand-in, which lasts as long as the console (ADR-1101).
    /// It takes one request at a time; the head asks it for the reply at
    /// each turn of the loop while one is out. The host checks every reply
    /// before it reads a pixel, and the terminal checks a reply's sizes
    /// against the request again ([`vtium_image::decode::Reply::checked`]).
    pub trait Helper {
        /// Sends `request` for the console whose session is `session`;
        /// `Err` holds the reply when the host answers at once.
        fn send(&mut self, request: &Request, session: &Session) -> Result<(), Reply>;
        /// The reply to the request out, once it has come. A failed one
        /// means the host has stopped its helper and counted the crash.
        fn reply(&mut self) -> Option<Reply>;
        /// The reply timer fired: the request out is dropped and the
        /// helper stopped, with no crash counted.
        fn stop(&mut self);
        /// The console's session ended: the request out is dropped, the
        /// helper stopped, and the crash count cleared.
        fn session_ended(&mut self);
        /// While a request is out, a descriptor that becomes readable when
        /// the reply may have come.
        fn wake(&self) -> Option<OwnedFd>;
        /// Whether the host can start a helper: it has a program and is
        /// not past the crash limit.
        fn available(&self) -> bool;
    }

    impl Helper for ImageHost {
        fn send(&mut self, request: &Request, session: &Session) -> Result<(), Reply> {
            ImageHost::send(self, request, session)
        }
        fn reply(&mut self) -> Option<Reply> {
            self.readable()
        }
        fn stop(&mut self) {
            ImageHost::stop(self);
        }
        fn session_ended(&mut self) {
            ImageHost::session_ended(self);
        }
        fn wake(&self) -> Option<OwnedFd> {
            ImageHost::wake(self)
        }
        fn available(&self) -> bool {
            ImageHost::available(self)
        }
    }

    /// What makes a console's host, once for each console.
    pub type Helpers = Rc<dyn Fn() -> Box<dyn Helper>>;

    /// What makes each console's [`ImageHost`]: from the program at
    /// `path`, opened and checked here, once, so every helper starts from
    /// the file vtium opened at start (ADR-1101 D1), and with `tmpdir`,
    /// vtium's own `TMPDIR`, where a temporary file may also lie (D3, D5).
    /// A program vtium refuses is logged here, once, and every host then
    /// answers as with no helper.
    pub fn hosts(path: &Path, tmpdir: Option<PathBuf>) -> Helpers {
        let program = match Program::open(path) {
            Ok(program) => Some(program),
            Err(why) => {
                log::warn!(
                    "{why}; files, shared memory, PNG and zlib pictures are answered as with no helper"
                );
                None
            }
        };
        Rc::new(move || Box::new(ImageHost::new(program.clone(), tmpdir.clone())))
    }

    /// vtium's `TMPDIR` in `env`, when it is an absolute path: the
    /// helper's temporary-file rule compares it with resolved paths.
    pub fn tmpdir(env: &[(std::ffi::OsString, std::ffi::OsString)]) -> Option<PathBuf> {
        env.iter()
            .rev()
            .find(|(k, _)| k == "TMPDIR")
            .map(|(_, v)| PathBuf::from(v))
            .filter(|p| p.is_absolute())
    }

    /// The handlers a console's terminal takes pictures through, and the
    /// limits `settings` give them. The budget and the switches come at
    /// the head's next image turn: until then the store holds nothing, so
    /// nothing is offered.
    pub(crate) fn install(terminal: &mut Terminal, settings: &Settings) {
        terminal.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        terminal.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        terminal.set_image_handler(ImageString::Iterm2, Some(Box::new(Iterm2::default())));
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
        /// The console's host can start a helper.
        pub(crate) helper: bool,
    }

    /// The budget and switches `settings` give a head whose displays are
    /// `sizes` in pixels and which `draw` pictures, every one of them
    /// (ADR-1100 D5, D10). vtium-term offers no protocol while the budget
    /// is 0, whatever its switch says. The iTerm2 switch waits for each
    /// console's helper ([`iterm2`]).
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

    /// `switches` with iTerm2's turned on while sixel is offered, the budget
    /// is above 0 and the console's host can start a helper, and off
    /// otherwise (ADR-1103 D1): its pictures follow sixel's rules and decode
    /// only in the helper. Turned off, the handler abandons a transfer in
    /// parts it holds.
    pub(crate) fn iterm2(switches: Switches, limit: u64, helper: bool) -> Switches {
        Switches {
            iterm2: switches.sixel && limit > 0 && helper,
            ..switches
        }
    }

    /// The loop watching a host's socket while a request is out, so the
    /// loop wakes when the reply arrives (ADR-1101 D7). Dropping it takes
    /// the descriptor out of the loop.
    struct Watch {
        handle: LoopHandle<'static, App>,
        token: RegistrationToken,
    }

    impl Watch {
        /// Watches `fd` in the loop behind `handle`, level-triggered, so
        /// the loop wakes for as long as there is something to read; the
        /// turn after the wake reads it. `None`, logged, when the loop
        /// cannot take it: the reply then waits for the next wake.
        fn new(fd: OwnedFd, handle: &LoopHandle<'static, App>) -> Option<Watch> {
            let source = Generic::new(fd, Interest::READ, Mode::Level);
            match handle.insert_source(source, |_, _, _: &mut App| Ok(PostAction::Continue)) {
                Ok(token) => Some(Watch {
                    handle: handle.clone(),
                    token,
                }),
                Err(e) => {
                    log::warn!(
                        "cannot watch the image helper's socket ({}); its reply waits for the loop's next wake",
                        e.error
                    );
                    None
                }
            }
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            self.handle.remove(self.token);
        }
    }

    /// The session of the console whose pty is `pty`, as the image host
    /// reads it before each request: the login child and the pty's owner.
    fn session(pty: Option<&Pty>) -> Session {
        Session {
            child: pty.and_then(Pty::child_id),
            pty_owner: match pty {
                Some(pty) => pty.slave_owner().map_err(|e| e.to_string()),
                None => Err("the console has no pty".to_owned()),
            },
        }
    }

    /// One console's side of the image pipeline: what it was last given,
    /// its host, and the request out.
    #[derive(Default)]
    pub(crate) struct Pictures {
        given: Option<Given>,
        host: Option<Box<dyn Helper>>,
        /// The request out, and when it fails unanswered.
        out: Option<(Request, Instant)>,
        /// The loop watching the host's socket while the request is out.
        watch: Option<Watch>,
    }

    impl Pictures {
        /// Gives `term` the cell `cell` for its pictures at once, as a
        /// resize gives the cell size, before the next image turn.
        pub(crate) fn give_cell(&mut self, term: &mut Term, cell: (u32, u32)) {
            if let Some(given) = self.given {
                self.give(term, Given { cell, ..given });
            }
        }

        /// The image turn for one console: its host made by `helpers` at
        /// the first turn, `term` given `limit`, `switches`, `cell` and
        /// whether its host can start a helper, then its request routed.
        /// Returns whether a reply was handed back.
        fn turn(
            &mut self,
            term: &mut Term,
            pty: Option<&mut Pty>,
            (limit, switches, cell): (u64, Switches, (u32, u32)),
            now: Instant,
            helpers: Option<&Helpers>,
            handle: &LoopHandle<'static, App>,
        ) -> bool {
            if self.host.is_none() {
                self.host = helpers.map(|make| make());
            }
            let helper = self.host.as_ref().is_some_and(|h| h.available());
            self.give(
                term,
                Given {
                    limit,
                    switches: iterm2(switches, limit, helper),
                    cell,
                    helper,
                },
            );
            self.route(term, pty, now, handle)
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
                    log::warn!("images cannot be shown with character cells of {w}x{h}");
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

        /// Sends the request `term` made to the console's host, pausing
        /// `pty` and watching the host's socket in the loop behind
        /// `handle` unless the host answers at once, and hands back the
        /// reply once it came or the timer fired at `now`. Returns whether
        /// a reply was handed back.
        fn route(
            &mut self,
            term: &mut Term,
            mut pty: Option<&mut Pty>,
            now: Instant,
            handle: &LoopHandle<'static, App>,
        ) -> bool {
            if let Some(request) = term.terminal.take_image_request() {
                let Some(host) = &mut self.host else {
                    // A terminal told no helper runs makes no request.
                    return self.answer(term, pty, Reply::Failed);
                };
                if let Err(reply) = host.send(&request, &session(pty.as_deref())) {
                    return self.answer(term, pty, reply);
                }
                self.watch = host.wake().and_then(|fd| Watch::new(fd, handle));
                self.out = Some((request, now.checked_add(REPLY_TIMEOUT).unwrap_or(now)));
                if let Some(pty) = pty.as_deref_mut() {
                    pty.pause();
                }
            }
            let (Some((request, due)), Some(host)) = (&self.out, &mut self.host) else {
                return false;
            };
            let reply = match host.reply() {
                Some(reply) => {
                    let failed = reply == Reply::Failed;
                    // Checked against the request, as the terminal checks
                    // it again. The host's own checks are wider, so a
                    // reply it passed fails here only through a defect,
                    // and the helper that sent it is stopped.
                    let reply = reply.checked(request);
                    if reply == Reply::Failed && !failed {
                        host.stop();
                    }
                    reply
                }
                None if now >= *due => {
                    host.stop();
                    Reply::Failed
                }
                None => return false,
            };
            self.answer(term, pty, reply)
        }

        /// Ends the request out, if any, hands `reply` to `term`, writes
        /// the replies it makes to the program and resumes `pty`.
        fn answer(&mut self, term: &mut Term, pty: Option<&mut Pty>, reply: Reply) -> bool {
            self.out = None;
            self.watch = None;
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
        /// reply, and the host stops its helper and clears its crash count
        /// (ADR-1101 D8), so the next session starts with none out and no
        /// crash counted.
        pub(crate) fn end_session(&mut self, term: &mut Term) {
            if let Some(host) = &mut self.host {
                host.session_ended();
            }
            self.watch = None;
            if self.out.take().is_some() {
                term.terminal.abandon_image_request();
                // what the ended program would have read goes nowhere
                term.terminal.take_replies();
            }
        }

        /// The request out, if any, fails at once, as when its time runs
        /// out: the host's helper stops and `term` takes the failure, so
        /// it takes the program's output again. The reset key needs the
        /// terminal taking input (ADR-1005 D10).
        pub(crate) fn fail_out(&mut self, term: &mut Term, pty: Option<&mut Pty>) {
            if self.out.is_none() {
                return;
            }
            if let Some(host) = &mut self.host {
                host.stop();
            }
            self.answer(term, pty, Reply::Failed);
        }

        /// When the request out fails unanswered.
        pub(crate) fn due(&self) -> Option<Instant> {
            self.out.as_ref().map(|(_, due)| *due)
        }
    }

    impl crate::head::Head {
        /// The image turn at `now`: each console gets the budget, the
        /// switches and the cell size the head's state gives, and whether
        /// its own host, which `helpers` makes at its first turn, can start
        /// a helper; the request each one made goes to its host, or its
        /// reply comes back, the host's socket watched in the loop behind
        /// `handle` meanwhile. A reply the shown console takes draws the
        /// head at the turn's end, as the picture it placed or the answer
        /// it wrote changed the screen.
        pub(crate) fn image_turn(
            &mut self,
            now: Instant,
            helpers: Option<&Helpers>,
            handle: &LoopHandle<'static, App>,
        ) {
            let draw = self.views.values().all(crate::term::View::draws_placements);
            let sizes = self.views.values().map(|v| v.size);
            let (limit, switches) = offer(&self.image_settings, sizes, draw);
            let shown = &mut self.console;
            if shown.pictures.turn(
                &mut shown.term,
                shown.pty.as_mut(),
                (limit, switches, self.cell),
                now,
                helpers,
                handle,
            ) {
                self.draw_now = true;
            }
            for console in &mut self.hidden {
                console.pictures.turn(
                    &mut console.term,
                    console.pty.as_mut(),
                    (limit, switches, console.fit.cell),
                    now,
                    helpers,
                    handle,
                );
            }
            // after every turn, so a note a reply made reaches the log
            // this turn, and one loop reads every console, shown or hidden
            for console in std::iter::once(&mut self.console).chain(&mut self.hidden) {
                log_notes(&self.name, console.id, &mut console.term);
            }
        }

        /// When each request out of the head's consoles fails unanswered.
        pub(crate) fn image_due(&self) -> impl Iterator<Item = Instant> + '_ {
            self.consoles().filter_map(|c| c.pictures.due())
        }
    }

    /// Writes the notes the image handlers of console `id` of head `head`
    /// left on `term` to vtium's log, a line each: at info level when the
    /// operator can act on the reason, and at debug level when only the
    /// program's author can.
    pub(crate) fn log_notes(head: &str, id: u64, term: &mut Term) {
        for note in term.terminal.take_image_notes() {
            let level = match note.cause {
                Cause::Console => log::Level::Info,
                Cause::Program => log::Level::Debug,
            };
            let name = note.protocol.name();
            let why = &note.why;
            match note.count {
                1 => say(
                    level,
                    format_args!("head {head}: console {id}: {name}: {why}"),
                ),
                n => say(
                    level,
                    format_args!("head {head}: console {id}: {name}: {why} ({n} times)"),
                ),
            }
        }
    }

    /// Writes a line of the image log at `level`. Under test it also keeps
    /// the line, for the tests to read back.
    fn say(level: log::Level, line: std::fmt::Arguments<'_>) {
        log::log!(target: "image", level, "{line}");
        #[cfg(test)]
        SAID.with(|s| s.borrow_mut().push((level, line.to_string())));
    }

    #[cfg(test)]
    thread_local! {
        /// The lines the image log wrote on this thread, with their level.
        static SAID: std::cell::RefCell<Vec<(log::Level, String)>> =
            const { std::cell::RefCell::new(Vec::new()) };
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

        // covers: image.iterm2
        #[test]
        fn iterm2_is_on_while_sixel_is_offered_and_a_helper_can_start() {
            let all = Switches {
                sixel: true,
                kitty: true,
                iterm2: false,
            };
            assert!(iterm2(all, 1 << 20, true).iterm2);
            assert_eq!(
                iterm2(all, 1 << 20, true),
                Switches {
                    iterm2: true,
                    ..all
                }
            );
            assert!(!iterm2(all, 1 << 20, false).iterm2, "no helper");
            assert!(!iterm2(all, 0, true).iterm2, "a budget of 0");
            let no_sixel = Switches {
                sixel: false,
                ..all
            };
            assert!(!iterm2(no_sixel, 1 << 20, true).iterm2, "sixel off");
            let on = Switches {
                iterm2: true,
                ..all
            };
            assert!(!iterm2(on, 1 << 20, false).iterm2, "it goes off again");
        }

        /// The lines the image log wrote on this thread since the last
        /// call, with their level.
        fn said() -> Vec<(log::Level, String)> {
            SAID.with(|s| std::mem::take(&mut *s.borrow_mut()))
        }

        /// A console's terminal with the three handlers, every protocol on,
        /// and an image helper when `helper`.
        #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
        fn console(helper: bool) -> Term {
            let clock: crate::term::Clock = Rc::new(Instant::now);
            let mut terminal = Terminal::new(8, 4, 0);
            install(&mut terminal, &settings());
            terminal.screen_mut().set_image_limit(1 << 20);
            terminal.set_image_helper(helper);
            terminal.set_image_switches(Switches {
                sixel: true,
                kitty: true,
                iterm2: true,
            });
            Term::new(
                terminal,
                crate::sync::Sync::new(Duration::ZERO, 0),
                crate::blink::Blink::new(false),
                clock,
            )
        }

        // ADR-1103 D1: a picture a protocol answers nothing for is dropped
        // without a word to the program, and its reason goes to vtium's
        // log. A string that breaks the protocol is the program's to fix,
        // so it goes out at debug level.
        #[test]
        fn a_refused_pictures_note_reaches_the_log_once_and_repeats_do_not_flood_it() {
            let mut term = console(true);
            let _ = said();
            let refused = b"\x1b]1337;File=size=3:QUJD\x07";
            term.terminal.input(refused);
            log_notes("main", 3, &mut term);
            let why = "head main: console 3: iTerm2: an iTerm2 picture without inline=1";
            assert_eq!(said(), [(log::Level::Debug, why.to_owned())]);
            log_notes("main", 3, &mut term);
            assert_eq!(said(), [], "logged once");
            // 1,000 refusals, a turn each, log at the 10th, 100th and 1,000th
            for _ in 1..1000 {
                term.terminal.input(refused);
                log_notes("main", 3, &mut term);
            }
            let lines: Vec<_> = said().into_iter().map(|(_, line)| line).collect();
            assert_eq!(
                lines,
                [
                    format!("{why} (10 times)"),
                    format!("{why} (100 times)"),
                    format!("{why} (1000 times)"),
                ]
            );
        }

        // A picture the console refuses never draws, and the operator can
        // act on why, so the note goes out at info level, which vtium shows
        // by default, even when the program asked for no answer: here a
        // kitty PNG with `q=2` on a console with no image helper.
        #[test]
        fn a_picture_the_console_refuses_is_logged_where_the_operator_sees_it() {
            let mut term = console(false);
            let _ = said();
            term.terminal
                .input(b"\x1b_Ga=T,f=100,i=1,q=2;iVBORw0KGgo=\x1b\\");
            assert_eq!(term.terminal.take_replies(), b"");
            log_notes("main", 3, &mut term);
            assert_eq!(
                said(),
                [(
                    log::Level::Info,
                    "head main: console 3: kitty graphics: no image helper reads a kitty \
                     picture's file, PNG or zlib payload"
                        .to_owned()
                )]
            );
        }

        // A session that ends with a request out drops it with no reply:
        // the helper did not fail, so nothing reaches the log, where a
        // handed-back FAILED would log "the image helper fails" at info.
        #[test]
        #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
        fn a_session_that_ends_with_a_request_out_logs_nothing() {
            let mut term = console(true);
            term.terminal
                .input(b"\x1b_Ga=t,f=100,i=1,q=2;iVBORw0KGgo=\x1b\\");
            let request = term.terminal.take_image_request();
            assert!(request.is_some(), "a request is out");
            let mut pictures = Pictures {
                out: request.map(|r| (r, Instant::now())),
                ..Pictures::default()
            };
            let _ = said();
            pictures.end_session(&mut term);
            assert!(!term.terminal.image_request_pending());
            log_notes("main", 3, &mut term);
            assert_eq!(said(), []);
        }

        #[test]
        fn a_consoles_terminal_takes_pictures_through_three_handlers() {
            let mut terminal = Terminal::new(8, 4, 0);
            install(&mut terminal, &settings());
            let installed = |which, is: fn(&dyn core::any::Any) -> bool| {
                terminal
                    .image_handler(which)
                    .is_some_and(|h| is(h.as_any()))
            };
            assert!(installed(ImageString::Sixel, |h| h.is::<Sixel>()));
            assert!(installed(ImageString::Kitty, |h| h.is::<Kitty>()));
            assert!(installed(ImageString::Iterm2, |h| h.is::<Iterm2>()));
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
