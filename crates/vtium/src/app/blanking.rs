//! The blank and the terminals' activity: vtium activates and deactivates
//! the terminals with the VT, blanks the displays when the blank's time runs
//! out, and wakes them at input or at a request from a program or from
//! outside the session.

use std::time::{Duration, Instant};

use vtium_platform::display::{DisplayId, Dpms};
use vtium_term::{BlankRequest, BlankTimeout};

use crate::blank::Order;
use crate::blink::Timer;

use super::drawing::power_down;
use super::{App, after};

impl App {
    /// The terminals start drawing and blinking.
    pub(super) fn activate(&mut self, now: Instant) {
        self.awake = true;
        for head in &mut self.heads {
            head.set_visible(true);
            // The cursor's first phase lasts the interval its program set,
            // so a phase longer than 500 ms is not cut short.
            let terminal = &head.console.term.terminal;
            let cursor = crate::blink::cursor_period(terminal);
            let blinks = terminal.cursor_blinks();
            for (timer, arm) in head.console.term.blink.activate(cursor, blinks) {
                match timer {
                    Timer::Text => head.text_due = Some(after(now, arm.first)),
                    Timer::Cursor => {
                        head.cursor_due = Some((after(now, arm.first), arm.every));
                    }
                }
            }
            // A line measured before the VT switched away no longer
            // describes this second, so each display drops it.
            for v in head.views.values_mut() {
                v.lost = true;
                v.timer_line = None;
            }
            head.draw_now = true;
        }
        if self.measure() {
            self.timer_due = Some(after(now, Duration::from_secs(1)));
        }
    }

    /// The terminals stop drawing and blinking.
    pub(super) fn deactivate(&mut self) {
        self.awake = false;
        for head in &mut self.heads {
            head.set_visible(false);
            head.text_due = None;
            head.cursor_due = None;
            head.console.term.blink.rapid_stop();
        }
        self.timer_due = None;
    }

    /// Whether any head has a display.
    fn any_display(&self) -> bool {
        self.heads.iter().any(|head| !head.views.is_empty())
    }

    /// The blank is due: every display powers down. With none, the
    /// countdown starts again.
    pub(super) fn blank_now(&mut self, now: Instant) {
        if !self.any_display() {
            self.blank.input(now);
            return;
        }
        // A card a hand-over gave up is the program's to power.
        for (node, video) in &mut self.videos {
            if !self.hand_overs.given_up(node) {
                power_down(video.as_mut());
            }
        }
        self.deactivate();
    }

    /// The user's input: a key press or a pointer action (a move, a
    /// button or the wheel), or a device's wake-up but a re-take's, which
    /// follows the user's VT switch. It restarts
    /// the blank's countdown, as a display change does for now; the idle
    /// lock (vtium/vtium#87) will count the user's input alone.
    pub(super) fn user_input(&mut self, now: Instant) {
        self.restart_blank(now);
        self.wake_notices();
    }

    /// What a program asked of the blank with the Linux console's
    /// sequences, taken under `program-blank`: a timeout of minutes, or
    /// `dpms-timeout` again after RIS, and a wake, which a key or pointer
    /// event would give. A timeout lasts until changed (the operator's
    /// decisions on vtium/vtium#79).
    pub(super) fn blank_request(&mut self, request: BlankRequest, now: Instant) {
        let timeout = request.timeout.map(|t| match t {
            BlankTimeout::Minutes(m) => Some(Duration::from_secs(60 * u64::from(m))),
            BlankTimeout::Configured => self.dpms_timeout,
        });
        if let Some(timeout) = timeout {
            self.blank.set_timeout(timeout, now);
        }
        if request.wake {
            self.wake(now);
        }
    }

    /// A blank or wake from outside the session: the blank the timer runs,
    /// at once, or the wake a program's `CSI 13 ]` gives. While the VT is
    /// another's its displays are not vtium's to blank, and the blank is
    /// dropped.
    pub(super) fn outside_order(&mut self, order: Order, now: Instant) {
        match order {
            Order::Blank if !self.vt.is_active() => {
                log::debug!("the VT is another's: the blank sent is dropped");
            }
            Order::Blank => {
                if self.blank.blank_now() {
                    self.blank_now(now);
                }
            }
            Order::Wake => self.wake(now),
        }
    }

    /// The blank's countdown starts again and a blank in force ends. While
    /// the VT is not ours nothing is woken: the acquire sets every device
    /// up.
    pub(super) fn wake(&mut self, now: Instant) {
        if self.vt.is_active() {
            self.restart_blank(now);
        } else {
            self.blank.input(now);
        }
    }

    /// The blank's countdown starts again, and a blank in force ends:
    /// every device unblanks, and one a re-take owes is set up again.
    fn restart_blank(&mut self, now: Instant) {
        if self.blank.input(now) {
            let retake = std::mem::take(&mut self.retake_owed);
            for (node, video) in &mut self.videos {
                // A card a hand-over gave up stays asleep, and its end
                // wakes it whole.
                if self.hand_overs.given_up(node) {
                    continue;
                }
                // A display the rules leave dark is never unblanked: a
                // device holding one powers up its other displays alone.
                let dark: Vec<DisplayId> = self
                    .dark
                    .keys()
                    .filter(|(n, _)| n == node)
                    .map(|&(_, id)| id)
                    .collect();
                let unblanked = if dark.is_empty() {
                    video.unblank()
                } else {
                    video
                        .displays()
                        .into_iter()
                        .filter(|id| !dark.contains(id))
                        .try_for_each(|id| video.set_dpms(id, Dpms::On))
                };
                if let Err(e) = unblanked {
                    log::warn!("cannot unblank: {e}");
                }
                if retake && let Err(e) = video.wake_up() {
                    log::warn!("cannot set up {} again: {e}", node.display());
                }
            }
            self.darken();
            if self.vt.is_active() {
                self.activate(now);
            }
            self.wake_notices();
        }
    }

    /// Every display of every head draws whole at the turn's end.
    pub(super) fn lose_every_view(&mut self) {
        for head in &mut self.heads {
            for v in head.views.values_mut() {
                v.lost = true;
            }
            head.draw_now = true;
        }
    }

    /// The user's input reaches every device, which retries a failed
    /// wake-up, and the blank. While the VT is another's, as a released
    /// fake VT is with its keyboards still open, it reaches neither: the
    /// displays are not vtium's to wake.
    pub(super) fn activity(&mut self, now: Instant) {
        if !self.vt.is_active() {
            return;
        }
        for video in self.videos.values_mut() {
            if let Err(e) = video.input() {
                log::warn!("cannot wake the displays: {e}");
            }
        }
        self.user_input(now);
    }
}
