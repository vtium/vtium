//! A head's consoles over their lives (ADR-1401 D1, D10, and the
//! operator's answers on vtium/vtium#156): how many each head opens
//! within the caps, the console chords, what a console's exit does, and
//! when vtium ends.
//!
//! A head starts with the consoles its `consoles` gives it. Under a
//! closed policy it keeps them to its end; under an open one the console
//! chords open a console after the one shown and close the one shown,
//! within the caps, and never the head's last. A console whose program
//! exits keeps its place among them (ADR-1401 D1, D5). Each console
//! counts its own quick exits and follows the respawn policy on its own.
//! Under respawn its program starts again in the same console, which
//! keeps its `VTIUM_CONSOLE` id. Under `oneshot` the console ends and
//! shows why, and the head ends once every one of its consoles has. Too
//! many quick exits of any console of any head end vtium. Under `oneshot`
//! vtium ends once no head that started still runs: a head whose display
//! never lit has nothing left to run, so it holds up no exit (the team
//! lead's steering of 2026-10-30).
//!
//! The C reference started a session's child again in the same terminal,
//! which closed its pty and hard-reset its parser first, and each pty
//! counted its own quick exits (`src/misc/pty.c:737-768`,
//! `src/terminal.c:1711-1748, 1856-1868` at kmscon b1f7a7c5). Under
//! `oneshot` it ended the whole process at the first exit, where vtium
//! ends that console alone. Its seat's chords opened a session next to
//! the current one and showed it, and closed the current one, showing the
//! next and refusing the last (`src/seat.c:584-640, 1165-1236`).

use std::rc::Rc;
use std::time::Duration;

use crate::app::Exit;
use crate::config::Config;
use crate::head::Head;
use crate::head::chord::HeadChord;
use crate::head::console::{self, Console};
use crate::respawn::{Next, QUICK, QUICK_RESTARTS, Respawn};
use crate::term::{Clock, Term};

/// The console caps (ADR-1401 D10): `head-consoles-max` and
/// `consoles-max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Caps {
    pub(crate) head: usize,
    pub(crate) total: usize,
}

/// The consoles each head of `rules` opens at its start: its `consoles`,
/// at most `caps.head`, with a warning past it. Each head keeps its
/// first; the others open, in the file's order, while the total is below
/// `caps.total`, so the share is the file's alone, whatever order the
/// displays come in.
pub(crate) fn console_shares(rules: &vtium_config::Config, caps: Caps) -> Vec<usize> {
    let mut spare = caps.total.saturating_sub(rules.heads.len());
    rules
        .heads
        .iter()
        .map(|h| {
            let asked = usize::try_from(h.consoles).unwrap_or(usize::MAX).max(1);
            if asked > caps.head {
                log::warn!(
                    "head {}: consoles = {asked} is past head-consoles-max: it opens {}",
                    h.name,
                    caps.head
                );
            }
            let more = asked.min(caps.head).saturating_sub(1);
            let given = more.min(spare);
            if given < more {
                log::warn!(
                    "head {}: consoles-max, {}, leaves it {} consoles",
                    h.name,
                    caps.total,
                    given.saturating_add(1)
                );
            }
            spare = spare.saturating_sub(given);
            given.saturating_add(1)
        })
        .collect()
}

/// The scrollback memory each console of `heads` heads takes under
/// `config` and `caps` ([`crate::head::console_scrollback`]).
pub(crate) fn scrollback_bytes(config: &Config, caps: Caps, heads: usize) -> usize {
    crate::head::console_scrollback(
        crate::config::budget::sb_memory(config),
        crate::config::budget::sb_memory_cap(),
        caps.total,
        heads,
    )
}

/// The heads of `rules`, each with its font keys and its `login` (ADR-1401
/// D3), the consoles `caps` give it, and the scrollback cap and length
/// and the bytes `scrollback` gives each console.
pub(crate) fn heads(
    rules: &vtium_config::Config,
    config: &Config,
    scrollback: ((usize, usize), usize),
    caps: Caps,
    clock: &Clock,
) -> Vec<Head> {
    let (lines, bytes) = scrollback;
    rules
        .heads
        .iter()
        .zip(console_shares(rules, caps))
        .map(|(h, wanted)| {
            let font = h.font(&vtium_config::Font::default());
            let term = crate::head::terminal(config, lines, bytes, Rc::clone(clock));
            let mut head = Head::new(&h.name, config, &font, term);
            head.wanted = wanted;
            head.when_display_lost = h.when_display_lost;
            head.login = h.login.as_deref().and_then(|text| {
                config
                    .with_login(text)
                    .map_err(|reports| {
                        for r in reports {
                            log::warn!("head {}: login: {}, kmscon.conf's runs", h.name, r.problem);
                        }
                    })
                    .ok()
            });
            head
        })
        .collect()
}

/// What a console's exit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterExit {
    /// The console's program starts again in it, from a hard reset.
    Restart,
    /// The console's program ended under `oneshot`; the console keeps its
    /// place, and shows that its session ended.
    Ended,
    /// The console's program exited quickly too many times running, which
    /// ends the run (ADR-1401 D1).
    GaveUp,
}

/// Whether vtium ends once its heads stand as `heads` do: once a head
/// ended under `oneshot` and no head that started still runs. Under the
/// respawn policy no head ends.
pub(crate) fn exit_due(heads: &[Head]) -> Option<Exit> {
    let ended = heads.iter().any(Head::ended);
    let running = heads.iter().any(|head| head.started() && !head.ended());
    (ended && !running).then_some(Exit::Oneshot)
}

impl Head {
    /// Opens the consoles past the first up to those the head wants at its
    /// start, behind the one shown, each on a terminal `term` makes.
    pub(crate) fn open_wanted(&mut self, mut term: impl FnMut() -> Term) {
        while self.console_count() < self.wanted {
            let id = self.next_id();
            let console = Console::new(id, term(), self.fresh_fit());
            self.hidden.push(console);
        }
    }

    /// Starts the count of each console not started yet at `since`, and
    /// returns their ids, whose sessions start.
    pub(crate) fn start_consoles(&mut self, since: Duration) -> Vec<u64> {
        self.consoles_mut()
            .filter(|c| !c.started())
            .map(|c| {
                c.respawn = Some(Respawn::new(since));
                c.id
            })
            .collect()
    }

    /// A console chord (ADR-1401 D5): the next or the previous console
    /// shown, wrapping. Returns whether the shown console changed.
    pub(crate) fn console_chord(&mut self, chord: HeadChord, visible: bool) -> bool {
        let back = match chord {
            HeadChord::NextConsole => false,
            HeadChord::PreviousConsole => true,
            _ => return false,
        };
        if self.console_count() < 2 {
            return false;
        }
        let to = console::step(self.at, self.console_count(), back);
        self.show(to, visible);
        true
    }

    /// Opens a console on a terminal `term` makes, just after the console
    /// shown, and leaves it hidden until its session starts (ADR-1401 D5),
    /// its count of starts and quick exits begun at `since`. It takes the
    /// shown console's grid at the configured font size, as a head's
    /// consoles do at its start. Returns its id and its ring position, where
    /// [`Head::show`] shows it.
    pub(crate) fn open_console(&mut self, term: Term, since: Duration) -> (u64, usize) {
        let id = self.next_id();
        let mut console = Console::new(id, term, self.fresh_fit());
        console.respawn = Some(Respawn::new(since));
        self.hidden.insert(self.at.min(self.hidden.len()), console);
        (id, self.at.saturating_add(1))
    }

    /// Takes back the hidden console `id`, the open chord's, whose session
    /// could not start: it leaves the head, which never showed it, and its
    /// id stays given.
    pub(crate) fn drop_hidden(&mut self, id: u64) {
        self.hidden.retain(|c| c.id != id);
        self.compose_notices();
    }

    /// Closes the console shown (ADR-1401 D5): its pty closes, so its
    /// programs get SIGHUP, and the head shows the console after it, or
    /// the first when it was the last. The console that closes is told
    /// nothing: no focus-out, and nothing it owed its program is sent.
    /// Returns it, its pty gone, for the caller to drop; the head's last
    /// console never closes.
    pub(crate) fn close_shown(&mut self, visible: bool) -> Option<Console> {
        if self.hidden.is_empty() {
            return None;
        }
        let (k, to) = if self.at < self.hidden.len() {
            (self.at, self.at)
        } else {
            (0, 0)
        };
        let next = self.hidden.remove(k);
        let mut closed = self.swap_shown(next, visible, true);
        self.at = to;
        log::info!("head {}: console {} closes", self.name(), closed.id);
        closed.pty = None;
        self.compose_notices();
        Some(closed)
    }

    /// Hangs up the head's sessions at `since`, as a line losing its
    /// carrier (ADR-1401 D8): every console's pty closes, so its programs
    /// get SIGHUP, and each console follows the respawn policy with no
    /// quick exit counted, since vtium ended the session. Each console
    /// keeps its place and its id (D5). Returns the consoles whose
    /// sessions start again. A head that ended or never started has
    /// nothing to hang up.
    pub(crate) fn hang_up(&mut self, since: Duration, oneshot: bool) -> Vec<u64> {
        self.hangup_due = None;
        if self.ended() || !self.started() {
            return Vec::new();
        }
        log::warn!(
            "head {}: its last display is gone: when-display-lost hangs its sessions up",
            self.name()
        );
        let running: Vec<u64> = self.consoles().filter(|c| !c.ended).map(|c| c.id).collect();
        let mut again = Vec::new();
        for id in running {
            let Some(console) = self.console_mut(id) else {
                continue;
            };
            console.pty = None;
            // An exit read before the hang-up is the hung-up session's.
            console.term.exited = None;
            let next = console
                .respawn
                .as_mut()
                .map_or(Next::Restart, |r| r.hung_up(since, oneshot));
            if self.session_ended(id, next) == AfterExit::Restart {
                again.push(id);
            }
        }
        again
    }

    /// The program of console `id` exited at `since`; `None` when the head
    /// has no such console.
    pub(crate) fn exited(&mut self, id: u64, since: Duration, oneshot: bool) -> Option<AfterExit> {
        let console = self.console_mut(id)?;
        // The pty closes with what is queued for the child.
        console.pty = None;
        let next = console
            .respawn
            .as_mut()
            .map_or(Next::Restart, |r| r.exited(since, oneshot));
        Some(self.session_ended(id, next))
    }

    /// The session of console `id` ended, and `next` follows.
    fn session_ended(&mut self, id: u64, next: Next) -> AfterExit {
        let after = match next {
            Next::Restart => AfterExit::Restart,
            Next::Oneshot => AfterExit::Ended,
            // The run ends in this turn, so the head draws no notice.
            Next::TooQuick => {
                log::error!(
                    "head {}: the session of console {id} ended less than {} s after it started, more than {QUICK_RESTARTS} times running",
                    self.name(),
                    QUICK.as_secs()
                );
                return AfterExit::GaveUp;
            }
        };
        let Some(console) = self.console_mut(id) else {
            return after;
        };
        if after == AfterExit::Restart {
            // A new session starts from a hard reset (RIS), with nothing of
            // the last session's output left (`terminal_close` at kmscon
            // b1f7a7c5).
            let term = &mut console.term;
            term.terminal.input(b"\x1bc");
            term.terminal.take_replies();
            term.unparsed = false;
        } else {
            console.ended = true;
            log::warn!(
                "head {}: console {id}: its program ended under oneshot, and it does not start again",
                self.name()
            );
        }
        // The shown console is drawn at once, a hidden one when it is shown.
        if self.console.id == id {
            self.draw_now = true;
        }
        // The shown console's notice names the console chord while another
        // console runs.
        self.compose_notices();
        after
    }
}
