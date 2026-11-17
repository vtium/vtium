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
//! Before a console's session starts again, the console closes its pty
//! and hard-resets its terminal. The open chord opens a console next to
//! the one shown and shows it, and the close chord closes the one shown,
//! shows the next, and does nothing on the head's last console.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use vtium_platform::display::DisplayId;
use vtium_render::Depth;

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
pub(crate) fn console_shares(rules: &vtium_settings::Config, caps: Caps) -> Vec<usize> {
    let mut spare = caps.total.saturating_sub(rules.heads.len());
    rules
        .heads
        .iter()
        .map(|h| {
            let asked = usize::try_from(h.consoles).unwrap_or(usize::MAX).max(1);
            if asked > caps.head {
                log::warn!(
                    "head {}: consoles = {asked} is more than head-consoles-max, so it opens {}",
                    h.name,
                    caps.head
                );
            }
            let more = asked.min(caps.head).saturating_sub(1);
            let given = more.min(spare);
            if given < more {
                log::warn!(
                    "head {}: consoles-max ({}) leaves it {} consoles",
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

/// The heads of `rules`, each with the keys its sections set over
/// `config`, its look's font keys and its own `login` among them
/// (ADR-1401 D3, ADR-1403 D5), the consoles `caps` give it, and the
/// scrollback cap and length and the bytes `scrollback` gives each
/// console.
pub(crate) fn heads(
    rules: &vtium_settings::Config,
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
            let mut settings: Vec<vtium_settings::Setting> = rules
                .settings(h)
                .into_iter()
                .map(|(_, s)| s.clone())
                .collect();
            crate::head::look::with_look_vtlook(&mut settings, rules.look(h));
            let (own, reports) = config.for_head(&crate::config::path(config), &settings);
            for r in reports {
                log::warn!("head {}: {r}; the top level's value stands", h.name);
            }
            let term = crate::head::terminal(config, lines, bytes, Rc::clone(clock));
            let mut head = Head::new(&h.name, &own, settings, term);
            head.wanted = wanted;
            head.when_display_lost = h.when_display_lost;
            head.login = h.login.is_some().then_some(own);
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

/// What the end of a console's session returns to the configuration
/// (ADR-1201 D11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Returns {
    /// The head's font, which an OSC 50 switch changed.
    pub(crate) font: bool,
    /// The console's size, which a zoom changed or the font returning
    /// sets.
    pub(crate) size: bool,
}

/// What a return at a session's end may change of a head, kept so that
/// it can be put back when the configured font draws nothing at its size
/// (ADR-1201 D11).
pub(crate) struct Kept {
    /// The shown console's size: in points, `font-size=0`, and its height.
    points: Option<u32>,
    auto_size: bool,
    font_size: u32,
    asked: Option<(usize, usize)>,
    configured: (Option<u32>, bool),
    sized: bool,
    switched: Option<u64>,
    /// The hidden console whose session ended, and its fit.
    hidden: Option<(u64, console::Fit)>,
    /// The font the head drew, where the return replaced it.
    pub(crate) font: Option<Option<crate::font::Chosen>>,
}

/// Whether vtium ends once its heads stand as `heads` do: once a head
/// ended under `oneshot` and no head that started still runs. Under the
/// respawn policy no head ends.
pub(crate) fn exit_due(heads: &[Head]) -> Option<Exit> {
    let ended = heads.iter().any(Head::ended);
    let running = heads.iter().any(|head| head.started() && !head.ended());
    (ended && !running).then_some(Exit::Oneshot)
}

/// What each console's running program was told about 24-bit color when
/// it started, so that vtium can say when a black-and-white display joins
/// a head whose programs were told they have 24-bit color. A program gets
/// its environment once, when it starts, so vtium cannot take the news
/// back; it logs one line instead.
///
/// References:
/// - ADR-1301 D10: a display that joins or leaves later.
#[derive(Debug, Default)]
pub(crate) struct Colorterm {
    /// By console id, what that console's program was told at its last
    /// start. A console whose session an earlier vtium left running has no
    /// entry, since vtium does not know what that session was told.
    told: BTreeMap<u64, Told>,
}

/// What one console's program was told as it started.
#[derive(Debug, Default)]
struct Told {
    /// It was told `COLORTERM=truecolor`.
    truecolor: bool,
    /// The black-and-white displays that joined since, which the log has
    /// already named for this program.
    named: BTreeSet<(PathBuf, DisplayId)>,
}

/// Whether 24-bit color can show on a display drawn at `depth`. On a gray
/// display vtium sends color and the display turns it into a shade, so
/// 24-bit color still picks a shade more finely than the 256-color palette
/// does. On a black-and-white display vtium decides only whether each
/// character cell is lit or dark, so 24-bit color gains a program nothing.
///
/// References:
/// - ADR-1301 D10: the rule.
fn shows_truecolor(depth: Depth) -> bool {
    match depth {
        Depth::Color | Depth::Gray(_) => true,
        Depth::One => false,
    }
}

impl Head {
    /// Whether a program that starts now in one of the head's consoles is
    /// told `COLORTERM=truecolor`: when every display of the head that is
    /// set up now shows 24-bit color. Setting a display up reads its depth,
    /// so each of those displays has its depth by now. A head with no
    /// display set up, as when a session starts again after the head lost
    /// every display, counts `unlit`: the depth its `display-depth` names,
    /// or color at `auto`, since 24-bit color that reaches a
    /// black-and-white display still draws as lit or dark.
    ///
    /// References:
    /// - ADR-1301 D10: the displays that count, and a head with no display
    ///   set up.
    pub(crate) fn truecolor(&self, unlit: Depth) -> bool {
        if self.views.is_empty() {
            return shows_truecolor(unlit);
        }
        self.views.values().all(|v| shows_truecolor(v.depth()))
    }

    /// Console `id`'s program started, told `COLORTERM=truecolor` when
    /// `truecolor` holds. Consoles that have closed are forgotten.
    pub(crate) fn told_truecolor(&mut self, id: u64, truecolor: bool) {
        let told = Told {
            truecolor,
            named: BTreeSet::new(),
        };
        self.colorterm.told.insert(id, told);
        let open: BTreeSet<u64> = self.consoles().map(|c| c.id).collect();
        self.colorterm.told.retain(|id, _| open.contains(id));
    }

    /// The display `key`, which the log calls `name`, has just been set up
    /// on the head. When it is black and white and a program still running
    /// on the head was told `COLORTERM=truecolor`, vtium logs one line that
    /// says so. That program keeps its environment until it starts again.
    /// The line names each display once for each start of a program, so a
    /// display that is set up again, as when it moves between heads, adds
    /// no second line. Returns whether vtium logged the line.
    ///
    /// References:
    /// - ADR-1301 D10: a display that joins or leaves later.
    pub(crate) fn black_and_white_joins(&mut self, key: &(PathBuf, DisplayId), name: &str) -> bool {
        if self
            .views
            .get(key)
            .is_none_or(|v| shows_truecolor(v.depth()))
        {
            return false;
        }
        let running: Vec<u64> = self
            .consoles()
            .filter(|c| c.pty.is_some())
            .map(|c| c.id)
            .collect();
        let mut news = false;
        for id in running {
            if let Some(told) = self.colorterm.told.get_mut(&id)
                && told.truecolor
                && told.named.insert(key.clone())
            {
                news = true;
            }
        }
        if news {
            log::warn!(
                "Display {name} shows only black and white, but the programs already running on head {} were told they have 24-bit color. They keep that until they start again.",
                self.name()
            );
        }
        news
    }

    /// The black-and-white displays that the log has named since the
    /// programs now running on the head started, so a test can check that
    /// vtium logged the line.
    #[cfg(test)]
    pub(crate) fn black_and_white_named(&self) -> BTreeSet<(PathBuf, DisplayId)> {
        self.colorterm
            .told
            .values()
            .flat_map(|told| told.named.iter().cloned())
            .collect()
    }
}

impl Head {
    /// Opens the consoles past the first up to those the head wants at its
    /// start, behind the one shown, each on a terminal `term` makes.
    pub(crate) fn open_wanted(&mut self, mut term: impl FnMut() -> Term) {
        while self.console_count() < self.wanted {
            let id = self.next_id();
            let console = self.fresh_console(id, term());
            self.hidden.push(console);
        }
    }

    /// Opens exactly the consoles of `ids`, in order, on terminals `term`
    /// makes, until the head has `room` consoles; returns the ids left
    /// without one. These are the consoles whose sessions an earlier vtium
    /// left running (ADR-1401 D13), which keep their ids: the console
    /// shown, which has no session yet, takes the lowest id where `ids`
    /// lacks its own, and the head's next new console takes an id after the
    /// last of them.
    pub(crate) fn open_kept(
        &mut self,
        ids: &[u64],
        room: usize,
        mut term: impl FnMut() -> Term,
    ) -> Vec<u64> {
        if let Some(&lowest) = ids.first()
            && !ids.contains(&self.console.id)
            && !self.console.started()
        {
            self.console.id = lowest;
            self.next_id = self.next_id.max(lowest.saturating_add(1));
        }
        let mut left = Vec::new();
        for &id in ids {
            if self.consoles().any(|c| c.id == id) {
                continue;
            }
            if self.console_count() >= room {
                left.push(id);
                continue;
            }
            let console = self.fresh_console(id, term());
            self.hidden.push(console);
            self.next_id = self.next_id.max(id.saturating_add(1));
        }
        left
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
        let mut console = self.fresh_console(id, term);
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
    /// keeps its place and its id (D5). Returns each console hung up, with
    /// what its session's end does. A head that ended or never started has
    /// nothing to hang up.
    pub(crate) fn hang_up(&mut self, since: Duration, oneshot: bool) -> Vec<(u64, AfterExit)> {
        self.hangup_due = None;
        if self.ended() || !self.started() {
            return Vec::new();
        }
        log::warn!(
            "head {}: its last display is gone: when-display-lost hangs its sessions up",
            self.name()
        );
        let running: Vec<u64> = self.consoles().filter(|c| !c.ended).map(|c| c.id).collect();
        let mut ended = Vec::new();
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
            ended.push((id, self.session_ended(id, next)));
        }
        ended
    }

    /// What the end of console `id`'s session returns (ADR-1201 D11): the
    /// head's font, where the last OSC 50 switch came from that console or
    /// the head has one console left; and the console's size, where the
    /// font returns or the console no longer draws at the configured size,
    /// as after a zoom or a program's request for a grid (XTWINOPS 8). A
    /// session ending in another console of the head
    /// leaves the font, so it never changes under another session.
    pub(crate) fn returns(&self, id: u64) -> Returns {
        let font = self
            .switched
            .is_some_and(|from| from == id || self.console_count() == 1);
        let fit = if self.console.id == id {
            Some(self.fit())
        } else {
            self.hidden.iter().find(|c| c.id == id).map(|c| c.fit)
        };
        // A zoom and a program's grid both leave the fit at no points and
        // no automatic size, which the configured size never is.
        let resized = fit.is_some_and(|f| (f.points, f.auto_size) != self.configured);
        Returns {
            font,
            size: font || resized,
        }
    }

    /// What a return at the end of console `id`'s session may change,
    /// before it changes it.
    pub(crate) fn keep(&self, id: u64) -> Kept {
        Kept {
            points: self.points,
            auto_size: self.auto_size,
            font_size: self.font_size,
            asked: self.asked,
            configured: self.configured,
            sized: self.sized,
            switched: self.switched,
            hidden: self
                .hidden
                .iter()
                .find(|c| c.id == id)
                .map(|c| (c.id, c.fit)),
            font: None,
        }
    }

    /// Puts back what [`Head::keep`] kept: the font, the switch that chose
    /// it, the size the shown console and the ended one draw at, and the
    /// configured size, so the head stands as before the return. Every
    /// display's renderer is rebuilt.
    pub(crate) fn put_back(&mut self, kept: Kept) {
        self.points = kept.points;
        self.auto_size = kept.auto_size;
        self.font_size = kept.font_size;
        self.asked = kept.asked;
        self.configured = kept.configured;
        self.sized = kept.sized;
        self.switched = kept.switched;
        if let Some((id, fit)) = kept.hidden
            && let Some(c) = self.hidden.iter_mut().find(|c| c.id == id)
        {
            c.fit = fit;
        }
        if let Some(font) = kept.font {
            self.font = font;
        }
        self.refont = true;
    }

    /// Takes the font size and the line spacing the head's configuration
    /// `config` configures, read again at a session's end (ADR-1201 D11),
    /// for the consoles that open and the sessions that end from now on. A
    /// change of the line spacing changes the cell, so the font is drawn
    /// again (ADR-1302 D13). `config` is the one [`Config::for_head`]
    /// gives, so the head's look's `font-size` and `font-line-spacing`
    /// still win over the top level's.
    pub(crate) fn reconfigure(&mut self, config: &Config) {
        let (points, auto_size, sized) = crate::head::configured_size(config);
        self.configured = (points, auto_size);
        self.sized = sized;
        let spacing = config.font_line_spacing().map(u32::from);
        if spacing != self.line_spacing {
            self.line_spacing = spacing;
            self.refont = true;
        }
    }

    /// Console `id` draws at the configured size from now on, filling the
    /// displays: the shown console at the next relayout, and a hidden one
    /// in `hidden`, its grid and cell at that size.
    pub(crate) fn configured_size(&mut self, id: u64, hidden: console::Fit) {
        let (points, auto_size) = self.configured;
        if self.console.id == id {
            self.points = points;
            self.auto_size = auto_size;
            self.asked = None;
        } else if let Some(c) = self.hidden.iter_mut().find(|c| c.id == id) {
            c.fit = console::Fit {
                points,
                auto_size,
                asked: None,
                ..hidden
            };
        }
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

    /// The program of console `id` ended while no vtium ran (ADR-1401
    /// D13), which vtium learns at `since`, as it takes the console's kept
    /// pty back: as an exit at that moment, with no quick exit counted,
    /// since vtium never saw it start. `None` when the head has no such
    /// console.
    pub(crate) fn ended_away(
        &mut self,
        id: u64,
        since: Duration,
        oneshot: bool,
    ) -> Option<AfterExit> {
        let console = self.console_mut(id)?;
        console.pty = None;
        let next = console
            .respawn
            .as_mut()
            .map_or(Next::Restart, |r| r.hung_up(since, oneshot));
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
                    "head {}: the session of console {id} ended within {} s of starting, more than {QUICK_RESTARTS} times in a row",
                    self.name(),
                    QUICK.as_secs()
                );
                return AfterExit::GaveUp;
            }
        };
        let Some(console) = self.console_mut(id) else {
            return after;
        };
        // The session's request to the image helper goes with it, so the
        // reset below finds the terminal taking input.
        #[cfg(feature = "images")]
        console.pictures.end_session(&mut console.term);
        // So do an OSC 50 request and an XTWINOPS 8 request a hidden
        // console still holds: each would change the font or the grid for
        // the next session once it is shown, and hold that session's output
        // meanwhile (ADR-1201 D11).
        console.term.terminal.take_font_request();
        console.term.terminal.take_grid_request();
        if after == AfterExit::Restart {
            // A new session starts from a hard reset (RIS), with nothing of
            // the last session's output left.
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
        // Link hints end with the shown console's session.
        //
        // References:
        // - ADR-1002, section 7: how link hints end.
        if self.console.id == id {
            self.end_link_hints();
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

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
mod tests {
    use std::path::Path;
    use std::process::Command;
    use std::time::Instant;

    use vtium_font::{Font, Unifont};
    use vtium_platform::pty::{Erase, Pty, Size};
    use vtium_render::Config as RenderConfig;

    use super::*;
    use crate::font::DisplayFace;
    use crate::term::View;

    /// A head named main, with one console and no display.
    fn head() -> Head {
        let config = crate::config::load(&[], &mut |_: &Path| Ok(None)).config;
        let clock: Clock = Rc::new(Instant::now);
        let term = crate::head::terminal(&config, (0, 0), 0, clock);
        Head::new("main", &config, Vec::new(), term)
    }

    /// The display `n` of one card.
    fn key(n: u32) -> (PathBuf, DisplayId) {
        (PathBuf::from("/dev/dri/card0"), DisplayId(n))
    }

    /// Sets the display `n` up on `head`, drawn at `depth`.
    fn set_up(head: &mut Head, n: u32, depth: Depth) {
        let face: DisplayFace = Box::new(Unifont::new(16));
        let config = RenderConfig {
            depth,
            ..RenderConfig::default()
        };
        let view = View::new(
            DisplayId(n),
            Font::new(face),
            (36, 34),
            None,
            config,
            false,
            crate::pointer_file::Store::default(),
        );
        head.views.insert(key(n), view);
    }

    /// Starts a program in console `id` of `head`, told `truecolor`, as
    /// vtium's start of a session does.
    fn start(head: &mut Head, id: u64, truecolor: bool) {
        let mut sh = Command::new("/bin/sh");
        sh.args(["-c", "read a"]);
        let size = Size {
            cols: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = Pty::spawn(sh, size, Erase::Delete).unwrap();
        head.console_mut(id).unwrap().pty = Some(pty);
        head.told_truecolor(id, truecolor);
    }

    // A head whose displays all show color or gray tells its programs
    // `COLORTERM=truecolor`. One black-and-white display among them, set
    // up as the program starts, leaves the variable out.
    //
    // References:
    // - ADR-1301 D10: the rule, and the displays that count.
    #[test]
    fn every_display_of_the_head_must_show_24_bit_color() {
        let shown = |depths: &[Depth]| {
            let mut head = head();
            for (n, &depth) in (1..).zip(depths) {
                set_up(&mut head, n, depth);
            }
            head.truecolor(Depth::Color)
        };
        assert!(shown(&[Depth::Color]), "color");
        assert!(shown(&[Depth::Gray(4)]), "gray4");
        assert!(shown(&[Depth::Gray(16)]), "gray16");
        assert!(!shown(&[Depth::One]), "black and white");
        assert!(shown(&[Depth::Color, Depth::Gray(16)]), "color and gray");
        assert!(
            !shown(&[Depth::Color, Depth::One]),
            "color and black and white"
        );
        assert!(!shown(&[Depth::One, Depth::Color]), "in either order");
    }

    // A head with no display set up counts the depth its `display-depth`
    // names, which the caller passes, or color at `auto`. Once a display
    // is set up, that display's depth decides, and `unlit` counts no more.
    //
    // References:
    // - ADR-1301 D10: a head with no display set up.
    #[test]
    fn a_head_with_no_display_counts_its_configured_depth() {
        let mut head = head();
        assert!(head.truecolor(Depth::Color));
        assert!(head.truecolor(Depth::Gray(16)));
        assert!(!head.truecolor(Depth::One));
        set_up(&mut head, 1, Depth::Color);
        assert!(head.truecolor(Depth::One));
    }

    // A black-and-white display that joins a head whose running program
    // was told `truecolor` is logged once. It is not logged for a program
    // told no `COLORTERM`, for a color display, for a program that no
    // longer runs, or a second time when the same display is set up again.
    // A program that starts after the display joined counts the display.
    //
    // References:
    // - ADR-1301 D10: a display that joins or leaves later, and each start
    //   reads the displays again.
    #[test]
    fn a_black_and_white_display_joining_a_running_truecolor_program_is_logged_once() {
        let mut head = head();
        set_up(&mut head, 1, Depth::Color);
        start(&mut head, 1, true);
        set_up(&mut head, 2, Depth::Gray(16));
        assert!(!head.black_and_white_joins(&key(2), "DP-1"), "gray");
        set_up(&mut head, 3, Depth::One);
        assert!(head.black_and_white_joins(&key(3), "DP-2"));
        assert!(!head.black_and_white_joins(&key(3), "DP-2"), "set up again");
        // the program starts again, and is told what the displays show now
        assert!(!head.truecolor(Depth::Color));
        start(&mut head, 1, false);
        set_up(&mut head, 4, Depth::One);
        assert!(!head.black_and_white_joins(&key(4), "DP-3"), "told none");
        // a display that is not the head's is not its news
        assert!(!head.black_and_white_joins(&key(9), "DP-9"));

        // a program that ended hears nothing
        let mut head = self::head();
        set_up(&mut head, 1, Depth::Color);
        start(&mut head, 1, true);
        head.console_mut(1).unwrap().pty = None;
        set_up(&mut head, 2, Depth::One);
        assert!(!head.black_and_white_joins(&key(2), "DP-2"), "ended");
    }
}
