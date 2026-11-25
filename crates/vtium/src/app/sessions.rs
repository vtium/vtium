//! Each console's session: where it runs, how its program starts, and what
//! vtium does when the program exits or keeps failing to start.

use std::io;
use std::rc::Rc;
use std::time::Instant;

use vtium_platform::pty::{Erase, Pty};
use vtium_settings::Policy;

use crate::child::{self, Display as SessionDisplay, Place};
use crate::config::keys;
use crate::head::console::Console;
use crate::head::session::{self, AfterExit};
use crate::head::{Attached, Head};
use crate::respawn;

use super::devices::depth;
use super::{App, Exit, in_scope, no_scope};

impl App {
    /// Each head's first start, when it is due: its consoles open and
    /// their sessions start.
    pub(super) fn first_start(&mut self, now: Instant) {
        if !self.start_due {
            return;
        }
        let since = now.saturating_duration_since(self.epoch);
        for h in 0..self.heads.len() {
            // The first login prompt waits for the login picture, which the
            // head asks for at its first display, so that a banner is in
            // place before the prompt's first line.
            //
            // References:
            // - ADR-1100 D17, "When vtium places the picture": the wait.
            #[cfg(feature = "images")]
            if self.heads.get(h).is_some_and(Head::login_image_waits) {
                continue;
            }
            let due = self.heads.get(h).is_some_and(|head| {
                !head.notice_only
                    && !head.started()
                    && respawn::first_start_due(self.awake, !head.views.is_empty())
            });
            if !due {
                continue;
            }
            // A head with sessions that outlived the last vtium opens those
            // consoles alone, keeping their ids (ADR-1401 D13).
            let restored = self.open_kept(h);
            let App {
                heads,
                config,
                scrollback,
                scrollback_bytes,
                clock,
                ..
            } = self;
            let Some(head) = heads.get_mut(h) else {
                continue;
            };
            if !restored {
                head.open_wanted(|| {
                    crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock))
                });
            }
            for id in head.start_consoles(since) {
                self.start_console(h, id, since);
            }
        }
    }

    /// Where head `h`'s console `id` runs its session.
    fn place(&self, h: usize, id: u64) -> Place<'_> {
        let kms = self.videos.values().any(|v| v.backend() == "drm2d");
        Place {
            seat: Some(self.vt.seat()),
            vtnr: self.vt.number().map(u32::from),
            display: if kms || self.videos.is_empty() {
                SessionDisplay::Kms
            } else {
                SessionDisplay::Fbdev
            },
            console: self.heads.get(h).map(|head| (head.name(), id)),
            logind_seat: self.vt.logind_seat(),
            truecolor: self.truecolor(h),
        }
    }

    /// Whether a program that starts now in head `h` is told
    /// `COLORTERM=truecolor`, as [`Head::truecolor`] decides from the
    /// head's displays. With no display set up, the head counts the depth
    /// its `display-depth` names, or color at `auto`.
    ///
    /// References:
    /// - ADR-1301 D10: the displays that count.
    fn truecolor(&self, h: usize) -> bool {
        let unlit = depth(&self.head_config(h), None);
        self.heads.get(h).is_none_or(|head| head.truecolor(unlit))
    }

    /// Starts the session of head `h`'s console `id`, unless a crash loop
    /// stopped every head under the closed policy (ADR-1401 D1). A session
    /// that cannot start ends vtium, so the unit's getty fallback takes the
    /// VT.
    pub(super) fn spawn(&mut self, h: usize, id: u64) {
        if let Err(why) = self.start_session(h, id) {
            log::error!("{why}");
            self.request_exit(Exit::Spawn);
        }
    }

    /// Starts the session of head `h`'s console `id`, as [`App::spawn`]
    /// does; the error is why it cannot start, for the caller to act on.
    fn start_session(&mut self, h: usize, id: u64) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        let config = self
            .heads
            .get(h)
            .and_then(|head| head.login.as_ref())
            .unwrap_or(&self.config);
        let spec = child::child(config, &self.place(h, id), self.inherited.clone());
        let truecolor = spec.truecolor();
        let command = spec.command().ok_or("no command to run")?;
        let erase = if self.config.switch(&keys::BACKSPACE_DELETE) {
            Erase::Delete
        } else {
            Erase::Backspace
        };
        let Some(size) = self.heads.get(h).and_then(|head| head.console_pty_size(id)) else {
            return Ok(());
        };
        // The login picture is placed before the program starts, so a
        // banner is in place before its first line.
        //
        // References:
        // - ADR-1100 D17, "When vtium places the picture": one placement
        //   for each login prompt.
        #[cfg(feature = "images")]
        if let Some(head) = self.heads.get_mut(h) {
            head.place_login_image(id);
        }
        let spawned = if self.vt.logind_seat() {
            // ADR-1400 D3, its note on each login's own session: the child
            // waits before its exec until systemd has moved it into a scope
            // of its own, outside vtium's logind session.
            let place = match self.vt.number() {
                Some(n) => format!("tty{n}"),
                None => self.vt.seat().to_owned(),
            };
            let scope = &mut self.login_scope;
            Pty::spawn_gated(command, size, erase, |pid| match scope(&place, pid) {
                Ok(unit) => log::debug!(target: "login", "{}", in_scope(&place, &unit)),
                Err(why) => log::warn!(target: "login", "{}", no_scope(&place, &why)),
            })
        } else {
            Pty::spawn(command, size, erase)
        };
        let mut pty = spawned.map_err(|e| format!("cannot start the session: {e}"))?;
        self.watch(h, id, &mut pty)
            .map_err(|e| format!("cannot watch the session's pty: {e}"))?;
        let pty_id = self.next_pty_id();
        if let Some(console) = self.heads.get_mut(h).and_then(|head| head.console_mut(id)) {
            console.pty_id = Some(pty_id);
            console.pty = Some(pty);
        }
        if let Some(head) = self.heads.get_mut(h) {
            head.told_truecolor(id, truecolor);
        }
        Ok(())
    }

    /// Puts `pty`, the session of head `h`'s console `id`, in the loop,
    /// which feeds its output to the console's terminal.
    pub(super) fn watch(&self, h: usize, id: u64, pty: &mut Pty) -> io::Result<()> {
        pty.insert(&self.handle, move |app: &mut App| {
            let Console { pty, term, .. } = app.heads.get_mut(h)?.console_mut(id)?;
            pty.as_mut().map(|p| (p, term))
        })
    }

    /// Opens a console on head `h` at `now`, after the console it shows,
    /// and starts the head's login in it (ADR-1401 D1, D5), unless the head
    /// has `head-consoles-max` consoles or every head together has
    /// `consoles-max`: then nothing opens, and vtium logs a line and shows
    /// a brief line on the head's on-screen display. A console whose login
    /// cannot start leaves, and the head shows a notice. Returns whether
    /// one opened.
    pub(super) fn open_console(&mut self, h: usize, now: Instant) -> bool {
        let total: usize = self
            .heads
            .iter()
            .filter(|head| !head.notice_only)
            .map(Head::console_count)
            .sum();
        let caps = self.caps;
        let osd = self.osd;
        let App {
            heads,
            config,
            scrollback,
            scrollback_bytes,
            clock,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return false;
        };
        let n = head.console_count();
        let full = if n >= caps.head {
            log::warn!(
                "head {}: no console opens: the head has {n}, head-consoles-max",
                head.name()
            );
            Some(crate::notice::no_more_consoles(head.name(), n))
        } else if total >= caps.total {
            log::warn!(
                "head {}: no console opens: every head together has {total}, consoles-max",
                head.name()
            );
            Some(crate::notice::no_more_consoles_together(total))
        } else {
            None
        };
        if let Some(line) = full {
            head.show_brief(line, osd);
            return false;
        }
        let term = crate::head::terminal(config, *scrollback, *scrollback_bytes, Rc::clone(clock));
        let since = now.saturating_duration_since(self.epoch);
        let (id, at) = head.open_console(term, since);
        log::info!("head {}: console {id} opens", head.name());
        // The session starts while the console is hidden, so a session that
        // cannot start leaves the console shown before told nothing. That
        // leaves the run alive: the console leaves, and the head says so,
        // since vtium never keeps a console with no session (the README's
        // "The session").
        // That the console did not open is a notice of a problem. It shows
        // at every level of `osd` but `silent`, where the log line alone
        // holds it, and it goes as the guards' notices go.
        //
        // References:
        // - ADR-1401 D5: the notice of a console that did not open.
        if let Err(why) = self.start_session(h, id) {
            if let Some(head) = self.heads.get_mut(h) {
                log::warn!("head {}: console {id} does not open: {why}", head.name());
                head.drop_hidden(id);
                head.add_notice(crate::notice::NO_CONSOLE_OPENED);
            }
            return false;
        }
        let visible = self.awake;
        if let Some(head) = self.heads.get_mut(h) {
            head.show(at, visible);
        }
        // The head lays the console it shows out on its displays.
        self.relayout(h);
        true
    }

    /// The programs of head `h`'s consoles that exited
    /// ([`session`]): each starts again in its console, ends its console
    /// under `oneshot`, or ends vtium after too many quick exits; under
    /// `oneshot` vtium ends once no head that started still runs.
    fn child_exited(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let exited: Vec<(u64, std::process::ExitStatus, bool)> = self
            .heads
            .get_mut(h)
            .into_iter()
            .flat_map(Head::consoles_mut)
            .filter_map(|c| {
                let adopted = c.pty.as_ref().is_some_and(Pty::adopted);
                c.term.exited.take().map(|status| (c.id, status, adopted))
            })
            .collect();
        for (id, status, adopted) in exited {
            if adopted {
                // vtium did not start it, so it cannot read its status
                // (ADR-1401 D13).
                let name = self.heads.get(h).map(Head::name).unwrap_or_default();
                log::info!(
                    "head {name}: console {id}: its session, which outlived the last vtium, ended; vtium did not start it, so its exit status is not known"
                );
            } else {
                log::info!("child exited: {status}");
            }
            // The session that sent setBackground ended: its hand-over
            // ends (ADR-1401 D16).
            self.hand_over_session_ended(h, id);
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            let after = head.exited(id, since, self.oneshot);
            // Before the console's next session starts, at the size it
            // returns to.
            if matches!(after, Some(AfterExit::Restart | AfterExit::Ended)) {
                self.return_font(h, id);
            }
            match after {
                // No login starts in the turn vtium exits, which hangs
                // every session up.
                Some(AfterExit::Restart) if self.exit.is_none() => self.spawn(h, id),
                Some(AfterExit::GaveUp) => self.gave_up(),
                Some(AfterExit::Restart | AfterExit::Ended) | None => {}
            }
        }
        if let Some(exit) = session::exit_due(&self.heads) {
            self.request_exit(exit);
        }
    }

    /// A session kept ending as soon as it started, which ends the run
    /// (ADR-1401 D1): every head's sessions hang up as vtium leaves, and
    /// under the open policy vtium exits with status 1 for the unit's
    /// getty. Under the closed policy a getty's login would take any
    /// keyboard (D9), so vtium hangs every session up and holds its VT: no
    /// session starts again, no input is delivered, every display a head
    /// held goes dark, and the display guard lights the boot display for a
    /// notice alone, which stays as long as vtium runs.
    fn gave_up(&mut self) {
        if self.rules.policy == Policy::Open {
            log::error!(
                "a session kept ending as soon as it started, so vtium exits. Check the login command in vtium.conf."
            );
            self.request_exit(Exit::TooQuick);
            return;
        }
        log::error!(
            "a session kept ending as soon as it started, so vtium stops every head; with the closed policy it keeps the VT"
        );
        self.stopped = true;
        self.focus = None;
        for head in &mut self.heads {
            for console in head.consoles_mut() {
                console.pty = None;
            }
            head.drop_views(|_| true);
            head.hangup_due = None;
            head.attached = Attached::NotYet;
        }
        self.guard_lit = None;
        let lit = std::mem::take(&mut self.lit);
        self.dark.extend(lit);
        self.unlit.clear();
        self.darken();
        // No rule, so no display joins a head again and the display guard
        // lights the boot display for the notice alone. The heads stay, each
        // isolated as the file wrote it, so a keyboard asking an isolated
        // head still switches no VT, whatever else delivers.
        self.rules = vtium_settings::Config {
            policy: Policy::Closed,
            heads: std::mem::take(&mut self.rules.heads),
            survivors: true,
            ..vtium_settings::Config::default()
        };
        self.guard_due = true;
    }

    /// What head `h`'s programs asked this turn: each console's OSC 52
    /// selection and BEL; the shown console's OSC 50 font and blank sequences,
    /// which a hidden console's wait for it to be shown, since the font
    /// and the displays are the head's; and their exits. Returns whether
    /// any of its consoles sent BEL, which [`App::turn`] rings.
    pub(super) fn session_turn(&mut self, h: usize, now: Instant) -> bool {
        let Some(head) = self.heads.get_mut(h) else {
            return false;
        };
        // The console has no system clipboard: a selection OSC 52 set is
        // held for a paste, or dropped under `no-clipboard-write`.
        // Any console's BEL rings the VT under `bell`, a hidden console's as
        // well as the shown one's.
        let mut bell = false;
        for console in head.consoles_mut() {
            if let Some(selection) = console.term.terminal.take_clipboard_write()
                && self.clipboard_write
            {
                self.selection = Some(selection);
            }
            bell |= console.term.terminal.take_bell();
        }
        // The terminal takes OSC 50 only under `allow-font-ops`.
        if let Some(name) = head.console.term.terminal.take_font_request() {
            self.switch_font(h, &name);
        }
        // And XTWINOPS 8 only under `allow-window-ops`.
        let grid = self
            .heads
            .get_mut(h)
            .and_then(|head| head.console.term.terminal.take_grid_request());
        if let Some(request) = grid {
            self.request_grid(h, request);
        }
        // setBackground and setForeground under `allow-display-ops`, from
        // every console, before the exits, which end a hand-over too.
        self.display_requests(h);
        // After the exit, whose respawn's RIS returns the blank to
        // `dpms-timeout` for the next session.
        self.child_exited(h, now);
        let request = self
            .heads
            .get_mut(h)
            .and_then(|head| head.console.term.terminal.take_blank_request());
        if let Some(request) = request
            && self.program_blank
        {
            self.blank_request(request, now);
        }
        bell
    }
}
