//! A head's consoles over their lives (ADR-1401 D1, D10, and the
//! operator's answers on vtium/vtium#156): how many each head opens
//! within the caps, the console chords, what a console's exit does, and
//! when vtium ends.
//!
//! A console whose program exits while others stay open closes, and the
//! head shows its next; the last console's exit follows the respawn
//! policy, each console counting its own quick exits: its program starts
//! again, or the head ends, showing why. vtium ends once no head that
//! started still runs: a head whose display never lit has nothing left to
//! run, so it holds up no exit (the team lead's steering of 2026-10-30).

use std::rc::Rc;
use std::time::Duration;

use crate::app::Exit;
use crate::config::Config;
use crate::head::chord::HeadChord;
use crate::head::console::{self, Console};
use crate::head::{Ended, Head};
use crate::respawn::{Next, Respawn};
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
            head.on_loss = h.on_loss;
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

/// The consoles every head of `heads` holds, or will hold at its start,
/// which `consoles-max` bounds.
pub(crate) fn console_total(heads: &[Head]) -> usize {
    heads
        .iter()
        .map(|head| head.console_count().max(head.wanted))
        .sum()
}

/// What a console's exit did to its head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterExit {
    /// The console closed; the head shows another when `shown`.
    Closed { shown: bool },
    /// The head's last console starts its program again, from a hard
    /// reset.
    Restart,
    /// The head's last console's program ended for good.
    Ended,
}

/// Whether vtium ends once its heads stand as `heads` do, and how: once
/// a head ended and no head that started still runs, with status 1 when
/// a head gave up. Under `oneshot` no head gives up, and under the
/// respawn policy none ends otherwise.
pub(crate) fn exit_due(heads: &[Head]) -> Option<Exit> {
    let ended: Vec<Ended> = heads.iter().filter_map(|head| head.ended).collect();
    let running = heads
        .iter()
        .any(|head| head.started() && head.ended.is_none());
    if ended.is_empty() || running {
        return None;
    }
    Some(if ended.contains(&Ended::TooQuick) {
        Exit::TooQuick
    } else {
        Exit::Oneshot
    })
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

    /// Whether a new console may open in the head, with `total` consoles
    /// in every head, within `caps`; a line says why not.
    pub(crate) fn may_open(&self, caps: Caps, total: usize) -> bool {
        let refused = if let Some(ended) = self.ended {
            Some(ended.notice().to_owned())
        } else if self.console_count() >= caps.head {
            Some(format!("past head-consoles-max, {}", caps.head))
        } else if total >= caps.total {
            Some(format!("past consoles-max, {}", caps.total))
        } else {
            None
        };
        if let Some(why) = &refused {
            log::warn!("head {}: no new console: {why}", self.name());
        }
        refused.is_none()
    }

    /// Opens the console `id` on `term`, shown, while `visible`; its count
    /// starts at `since` when the head's sessions have. Returns whether its
    /// session starts now.
    pub(crate) fn open(&mut self, id: u64, term: Term, since: Duration, visible: bool) -> bool {
        let mut console = Console::new(id, term, self.fresh_fit());
        let started = self.started();
        if started {
            console.respawn = Some(Respawn::new(since));
        }
        self.push(console, visible);
        started
    }

    /// A console chord but the new console's (ADR-1401 D5): the next or
    /// the previous console shown, or the shown one closed and the next
    /// shown, its pty closed so its programs get SIGHUP. A head of one
    /// console keeps it, as the C reference's seat kept its last session
    /// (`src/seat.c:610-620` at kmscon b1f7a7c5). Returns whether the
    /// shown console changed.
    pub(crate) fn console_chord(&mut self, chord: HeadChord, visible: bool) -> bool {
        if self.console_count() < 2 {
            return false;
        }
        match chord {
            HeadChord::CloseConsole => drop(self.close_shown(visible)),
            HeadChord::NextConsole | HeadChord::PreviousConsole => {
                let back = chord == HeadChord::PreviousConsole;
                let to = console::step(self.at, self.console_count(), back);
                self.show(to, visible);
            }
            HeadChord::NewConsole | HeadChord::NextHead | HeadChord::PreviousHead => {
                return false;
            }
        }
        true
    }

    /// Hangs up the head's sessions at `since`, as a line losing its
    /// carrier (ADR-1401 D8): every console's pty closes, so its programs
    /// get SIGHUP; the consoles but the shown one close, and its exit
    /// follows the respawn policy, the shown next while `visible`. Returns
    /// whether its session starts again. A head that ended or never
    /// started has nothing to hang up.
    pub(crate) fn hang_up(&mut self, since: Duration, oneshot: bool, visible: bool) -> bool {
        self.hangup_due = None;
        if self.ended.is_some() || !self.started() {
            return false;
        }
        log::warn!(
            "head {}: its last display is gone: on-loss hangs its sessions up",
            self.name()
        );
        self.hidden.clear();
        self.at = 0;
        self.exited(self.console.id, since, oneshot, visible) == AfterExit::Restart
    }

    /// The program of console `id` exited at `since`, the console shown
    /// next while `visible`.
    pub(crate) fn exited(
        &mut self,
        id: u64,
        since: Duration,
        oneshot: bool,
        visible: bool,
    ) -> AfterExit {
        if self.console_count() > 1 {
            // The pty closes with what is queued for the child.
            let shown = self.console.id == id;
            if shown {
                drop(self.close_shown(visible));
            } else {
                drop(self.remove_hidden(id));
            }
            return AfterExit::Closed { shown };
        }
        self.console.pty = None;
        let next = self
            .console
            .respawn
            .as_mut()
            .map_or(Next::Restart, |r| r.exited(since, oneshot));
        let ended = match next {
            Next::Restart => {
                // A new session starts from a hard reset (RIS), drawn at
                // once, with nothing of the last session's output left
                // (`terminal_close` at kmscon b1f7a7c5).
                let term = &mut self.console.term;
                term.terminal.input(b"\x1bc");
                term.terminal.take_replies();
                term.unparsed = false;
                self.draw_now = true;
                return AfterExit::Restart;
            }
            Next::Oneshot => Ended::Oneshot,
            Next::TooQuick => Ended::TooQuick,
        };
        log::warn!("head {}: {}", self.name(), ended.notice());
        self.ended = Some(ended);
        self.draw_now = true;
        AfterExit::Ended
    }
}
