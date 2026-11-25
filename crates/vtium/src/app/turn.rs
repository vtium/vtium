//! The end of each turn of the loop and the timers: what vtium does after
//! each dispatch, and how long the loop may wait before the next.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use vtium_platform::input::DeviceInput;
use vtium_platform::pty::Pty;
use vtium_platform::udev::DeviceEvent;
use vtium_platform::vt::{Switch, VtEvent};

use crate::blink::Timer;
use crate::head::Head;
use crate::helper::Helper;
use crate::sync::Action;

use super::drawing::render_timer_line;
use super::{App, Exit, LEAVE_WAIT, Leaving, after};

impl App {
    /// One turn's end: every source drained, the timers due run, and the
    /// displays drawn where something asks.
    pub fn turn(&mut self, now: Instant) {
        let events: Vec<VtEvent> = self.vt.events().collect();
        for e in events {
            self.vt_event(e, now);
        }
        // After the VT's events, so none shows once the VT is released, and
        // before the devices', so a flip they queue is read this turn.
        self.frames(now);
        #[cfg_attr(not(test), expect(unused_mut, reason = "a test adds its events"))]
        let mut devices: Vec<DeviceEvent> = self
            .udev
            .as_mut()
            .map(|u| u.events().collect())
            .unwrap_or_default();
        #[cfg(test)]
        devices.splice(0..0, std::mem::take(&mut self.queued_devices));
        self.device_events(devices, now);
        self.settled_videos();
        let nodes: Vec<PathBuf> = self.videos.keys().cloned().collect();
        for node in nodes {
            let events = self
                .videos
                .get_mut(&node)
                .map(|v| v.events())
                .unwrap_or_default();
            for e in events {
                self.video_event(&node, e, now);
            }
        }
        self.let_go_of_gone_cards();
        if std::mem::take(&mut self.keyboard_check) && !self.routes.any_keyboard() {
            let line = crate::notice::NO_KEYBOARD;
            log::warn!("{line}");
            self.notice(line);
        }
        self.take_focus();
        self.guard();
        self.attachment(now);
        self.first_start(now);
        // After the VT's events and the guards, which may have taken the
        // keyboards from a console.
        self.let_go_of_held_keys();
        if let Some(r) = &self.releasing
            && r.is_empty()
        {
            self.releasing = None;
            if let Err(e) = self.vt.acknowledge_release() {
                log::warn!("cannot let the VT go: {e}");
            }
        }
        let inputs: Vec<DeviceInput> = match &mut self.input {
            Some(input) => input.device_events(self.vt.as_vt()).collect(),
            None => Vec::new(),
        };
        for head in &mut self.heads {
            head.tracking_changed();
        }
        for e in inputs {
            self.drained_input(e, now);
        }
        self.report_focus();
        // One ring a turn, however many consoles of however many heads
        // sent BEL: the heads share the one VT.
        let mut bell = false;
        for h in 0..self.heads.len() {
            bell |= self.session_turn(h, now);
        }
        if bell && self.bell {
            self.vt.bell();
        }
        // After the sessions' turns, whose respawns drop a request out,
        // and before the draw, which shows what a reply placed.
        #[cfg(feature = "images")]
        for head in &mut self.heads {
            head.image_turn(now, self.image_helpers.as_ref(), &self.handle);
        }
        if let Some(order) = self.order.take() {
            self.outside_order(order, now);
        }
        self.timers(now);
        self.helper_turn(now);
        self.pointer_turn(now);
        if self.awake {
            self.draw(now);
        }
        // The draw's commits and the timers' power changes may find a card
        // removed after its events were drained.
        self.let_go_of_gone_cards();
        self.report(now);
        for console in self.heads.iter_mut().flat_map(Head::consoles_mut) {
            console.term.coalesce.turn_end();
            console.term.changed = false;
        }
        // A synchronized update's end stops the terminal only until the
        // displays take the finished frame. No display draws a hidden
        // console, nor any console while the VT is not shown or the
        // displays are blanked, so those take output again at once.
        for head in &mut self.heads {
            if !self.awake {
                head.console.term.unshown = false;
            }
            for console in &mut head.hidden {
                console.term.unshown = false;
            }
        }
        let flipping = self.heads.iter().any(|head| {
            head.views.iter().any(|((node, _), v)| {
                v.away() || self.videos.get(node).is_some_and(|d| d.is_flipping(v.id))
            })
        });
        if let Some(wait) = self.wait.turn(now, self.presented, flipping) {
            log::error!(
                "no display appeared within {} s (display-wait), so vtium exits",
                wait.as_secs()
            );
            self.request_exit(Exit::NoDisplay);
        }
        self.sync_kept();
        self.exiting(now);
    }

    fn timers(&mut self, now: Instant) {
        let awake = self.awake;
        for head in &mut self.heads {
            if head.notice_due.is_some_and(|d| d <= now) {
                head.notices_timed_out();
            }
            head.kernel_turn(now);
            if head.osd_due.is_some_and(|d| d <= now) {
                head.osd_due = None;
                head.osd_shown = false;
            }
            head.brief_turn(now);
            if let Some(due) = head.text_due.filter(|&d| d <= now) {
                head.draw_now |= head.console.term.blink.fired(Timer::Text, awake);
                head.text_due = Some(after(due, crate::blink::TEXT_PERIOD));
            }
            // The cursor's interval can differ from the one its timer was
            // armed with: a program set another with `CSI 16 ; n ]`, or
            // another console of the head is shown now. The phase that is
            // running keeps its end, so a program that keeps changing the
            // interval never stops the blink, and a longer interval never
            // stretches that phase. A shorter interval ends the phase
            // within the new interval. Each later phase lasts the new
            // interval.
            let every = crate::blink::cursor_period(&head.console.term.terminal);
            // A program changes the cursor's style at any moment: a style
            // that blinks starts the timer from now, and a steady one
            // stops it with the cursor shown.
            let blinks = head.console.term.terminal.cursor_blinks();
            match (
                head.console.term.blink.cursor_runs(blinks, awake),
                head.cursor_due,
            ) {
                (true, None) => head.cursor_due = Some((after(now, every), every)),
                (false, Some(_)) => {
                    head.cursor_due = None;
                    head.draw_now = true;
                }
                _ => {}
            }
            if let Some((due, armed)) = head.cursor_due {
                if armed != every && due > now {
                    head.cursor_due = Some((due.min(after(now, every)), every));
                } else if due <= now {
                    head.draw_now |= head.console.term.blink.fired(Timer::Cursor, awake);
                    head.cursor_due = Some((after(due, every), every));
                }
            }
            let term = &mut head.console.term;
            // SGR 6's timer runs while a shown cell carries it
            if term.changed || head.draw_now {
                term.blink
                    .rapid_seen(crate::blink::rapid_on_screen(&term.terminal));
            }
            head.draw_now |= term.blink.rapid_turn(now, awake);
            if term.sync.deadline().is_some_and(|d| d <= now) {
                let action = term.sync.timer(&mut term.terminal, now, term.unparsed);
                term.act(action);
                if action == Action::Expired {
                    for v in head.views.values_mut() {
                        if let Some(t) = &mut v.timer {
                            t.expired();
                        }
                    }
                }
            }
        }
        // While the VT is another's its displays are not vtium's to blank,
        // and its countdown waits for the acquire.
        if self.vt.is_active() && self.blank.fire(now) {
            self.blank_now(now);
        }
        #[cfg(feature = "images")]
        self.animate(now);
    }

    /// Each console's animation clock at this turn (ADR-1102 D3): the
    /// shown console's plays while the VT is ours, its displays are lit
    /// and it has one, and a hidden console's only answers its program's
    /// commands. A frame that changes on a shown console draws its head.
    #[cfg(feature = "images")]
    fn animate(&mut self, now: Instant) {
        let lit = self.awake && !self.blank.blanked();
        for head in &mut self.heads {
            let shown = lit && !head.views.is_empty();
            let term = &mut head.console.term;
            let frames = term.terminal.screen().images();
            head.draw_now |= term.animation.turn(frames, now, shown);
            for console in &mut head.hidden {
                let term = &mut console.term;
                term.animation
                    .turn(term.terminal.screen().images(), now, false);
            }
        }
    }

    /// The render timer's figures for each display, once a second: its
    /// line in the log under `render-timer=log` or `both`, and its line in
    /// the on-screen display's box under `osd` or `both`, and the console
    /// redraws the display so the new line shows. That frame counts in the
    /// next second's figures, so they include the cost of showing them.
    fn report(&mut self, now: Instant) {
        for head in &mut self.heads {
            if let Some(line) = head.figures_line(now) {
                log::debug!(target: "render", "{line}");
            }
        }
        let Some(due) = self.timer_due.filter(|&d| d <= now) else {
            return;
        };
        self.timer_due = Some(after(due, Duration::from_secs(1)));
        let (log, osd) = self.render_timer();
        for head in &mut self.heads {
            let mut shows = false;
            for ((node, id), view) in &mut head.views {
                let px = u64::from(view.size.0).saturating_mul(u64::from(view.size.1));
                let Some(report) = view
                    .timer
                    .as_mut()
                    .and_then(|t| t.report(Duration::from_secs(1), px))
                else {
                    continue;
                };
                if log {
                    log::info!(target: "terminal", "{}", render_timer_line(&report, node, *id));
                }
                if osd {
                    view.timer_line = Some(report.osd());
                    shows = true;
                }
            }
            head.draw_now |= shows;
        }
    }

    fn exiting(&mut self, now: Instant) {
        if self.exit.is_none() {
            return;
        }
        match self.leaving {
            None => {
                self.deactivate();
                // The sessions a keeper holds run on for the next vtium
                // (ADR-1401 D13); every other session is hung up.
                self.release_sessions();
                for console in self.heads.iter_mut().flat_map(Head::consoles_mut) {
                    console.pty = None;
                }
                self.leaving = Some(match self.vt.leave() {
                    Ok(Switch::Pending) => Leaving::Until(after(now, LEAVE_WAIT)),
                    Ok(Switch::Done) => Leaving::Done,
                    Err(e) => {
                        log::warn!("cannot switch back: {e}");
                        Leaving::Done
                    }
                });
            }
            Some(Leaving::Until(deadline)) => {
                if now >= deadline || !self.vt.is_active() && self.releasing.is_none() {
                    self.leaving = Some(Leaving::Done);
                }
            }
            Some(Leaving::Done) => {}
        }
    }

    /// The longest the loop may wait before the next turn: until the next
    /// timer the app keeps is due.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let mut due: Vec<Instant> = Vec::new();
        due.extend(self.timer_due);
        due.extend(self.wait.due());
        if self.vt.is_active() {
            due.extend(self.blank.deadline());
        }
        for head in &self.heads {
            due.extend(head.text_due);
            due.extend(head.cursor_due.map(|(d, _)| d));
            due.extend(head.console.term.blink.rapid_due());
            due.extend(head.console.term.sync.deadline());
            due.extend(head.views.values().filter_map(|v| v.wait.due()));
            due.extend(head.figures.due);
            due.extend(head.hangup_due);
            due.extend(head.notice_due);
            due.extend(head.kernel_due());
            due.extend(head.osd_due);
            due.extend(head.brief_due());
            #[cfg(feature = "images")]
            due.extend(head.image_due());
            #[cfg(feature = "images")]
            {
                let term = &head.console.term;
                due.extend(term.animation.due(term.terminal.screen().images()));
            }
        }
        for frit in self.frits.values() {
            due.extend(frit.helper.as_ref().and_then(Helper::deadline));
        }
        due.extend(self.pointer_deadlines());
        // A notice's time runs out while the displays draw, and its screen
        // has no other timer, since its cursor is hidden (ADR-1401 D3).
        if self.awake {
            due.extend(
                self.notices
                    .values()
                    .filter(|n| !n.parked)
                    .filter_map(|n| n.due),
            );
        }
        if let Some(Leaving::Until(d)) = self.leaving {
            due.push(d);
        }
        // A card let go after the turn's guard leaves the guard owed.
        if self.leaving == Some(Leaving::Done) || self.guard_due {
            return Some(Duration::ZERO);
        }
        // The rest of a read waits in the pty while the terminal takes no
        // output, as after a synchronized update's end. Once the turn has
        // drawn and the terminal takes output again, the loop feeds it at
        // once. calloop does not shorten its wait for the idle callback that
        // feeds it, so the wait itself is zero. While the terminal still
        // takes nothing, the loop waits for what it waits on, such as the
        // display's flip, and does not spin.
        //
        // References:
        // - ADR-1401 D2, the note of 2026-11-17: the loop wakes at once for
        //   the rest of the read.
        let owed = self
            .heads
            .iter()
            .flat_map(Head::consoles)
            .any(|c| c.term.takes_input() && c.pty.as_ref().is_some_and(Pty::feed_owed));
        if owed {
            return Some(Duration::ZERO);
        }
        due.into_iter()
            .min()
            .map(|d| d.saturating_duration_since(now))
    }
}
