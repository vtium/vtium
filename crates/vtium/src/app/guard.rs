//! The guards that run after a device comes or goes: under the closed
//! policy a display shows a notice when no head holds a screen, and a head
//! that loses its displays hangs up its sessions as its configuration says.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use vtium_platform::display::{DisplayId, Dpms};
use vtium_settings::WhenDisplayLost;

use crate::head::session::{self, AfterExit};
use crate::head::{Attached, Head};
use crate::osd;

use super::{App, after};

impl App {
    /// The guards (ADR-1401 D9), after a device came or went: every device
    /// present placed by the rules in force, then the display guard and the
    /// anti-lockout guard over the placement.
    ///
    /// The display guard lights the boot display in the first head while
    /// the rules light none, and lets it go dark once they light another.
    /// The anti-lockout guard sends every keyboard to the focus, opened to
    /// deliver, while none reaches a head with a lit display, and moves the
    /// focus to a lit head, with a notice and the on-screen display; once one
    /// reaches a lit head each goes back to its rule. A keyboard the rescue
    /// key moved follows the focus throughout, and reaches it while the
    /// focus's head is lit.
    pub(super) fn guard(&mut self) {
        if !std::mem::take(&mut self.guard_due) {
            return;
        }
        let displays: Vec<(PathBuf, DisplayId)> =
            self.lit.keys().chain(self.dark.keys()).cloned().collect();
        // The displays a device left dark at its bind and never announced,
        // each with its device and connector; their place among their
        // card's connectors is unknown, so they sort last on it.
        let held: Vec<(PathBuf, String, vtium_settings::Identity)> = self
            .videos
            .iter()
            .flat_map(|(node, video)| {
                let card = self.cards.get(node).cloned().unwrap_or_default();
                video.dark().into_iter().map(move |named| {
                    let display = vtium_devices::display(&named, &card, u32::MAX);
                    (node.clone(), named.connector, display)
                })
            })
            .collect();
        let devices: Vec<vtium_settings::Identity> = self
            .lit
            .values()
            .chain(self.dark.values())
            .chain(held.iter().map(|(_, _, d)| d))
            .chain(self.routes.unrescued())
            .cloned()
            .collect();
        // A keyboard the rescue key moved roams, and reaches the focus's
        // head while it is lit.
        let rescued: Vec<vtium_settings::Identity> = self.routes.rescued().cloned().collect();
        let placement = self
            .vt_heads
            .placement(&self.rules, &devices, &rescued, self.focus);
        // Under open, each display no path catches shows its own notice
        // (ADR-1401 D3).
        let on_screens = displays.len().saturating_add(held.len());
        let unused: Vec<((PathBuf, String), Option<usize>)> = displays
            .iter()
            .map(|(node, _)| node)
            .chain(held.iter().map(|(node, _, _)| node))
            .zip(devices.iter().zip(&placement.placed))
            .take(on_screens)
            .filter_map(|(node, (d, p))| match p.place {
                vtium_settings::Place::Unused(owner) => {
                    Some(((node.clone(), d.connector.clone()?), owner))
                }
                _ => None,
            })
            .collect();
        self.show_notices(&unused);
        let (shown, rest) = placement
            .placed
            .split_at(displays.len().min(placement.placed.len()));
        let (unannounced, inputs) = rest.split_at(held.len().min(rest.len()));
        // under closed the guard lights the boot display for a notice
        // alone, on the screen no head holds
        let screen = self.screen;
        let lighting = displays
            .iter()
            .zip(shown)
            .find(|(_, p)| p.by == vtium_settings::By::NeverDark)
            .and_then(|(key, p)| {
                let h = match p.place {
                    vtium_settings::Place::Notice => screen,
                    place => self.vt_heads.head(place),
                };
                Some((key.clone(), h?))
            })
            .filter(|&(_, h)| Some(h) != screen || self.screen_shows());
        if lighting != self.guard_lit {
            if let Some((key, _)) = self.guard_lit.take() {
                self.unlight(key);
            }
            if let Some((key, h)) = lighting {
                if Some(h) == screen {
                    let report = self.report.clone().filter(|_| !self.stopped);
                    let text = self.screen_notice();
                    // a report's console line, as the start logged it
                    let logged = match (&report, self.outcome.line) {
                        (Some(_), Some(line)) => line.text(),
                        _ => text.as_str(),
                    };
                    log::warn!(
                        "the display guard lights display {} for the notice alone: {logged}",
                        key.0.display()
                    );
                    if let Some(head) = self.heads.get_mut(h) {
                        match report {
                            Some(report) => head.set_report(report),
                            None => head.set_notice(&text),
                        }
                    }
                } else {
                    log::warn!(
                        "the display guard lights display {} in head {}",
                        key.0.display(),
                        self.heads.get(h).map_or("?", Head::name)
                    );
                }
                self.dark.remove(&key);
                self.guard_lit = Some((key.clone(), h));
                if let Some(video) = self.videos.get_mut(&key.0)
                    && let Err(e) = video.set_dpms(key.1, Dpms::On)
                {
                    log::warn!("cannot set display {} on: {e}", video.name(key.1));
                }
                self.add_display(&key.0, key.1);
            }
        }
        // A display the guard lights that its device never announced is
        // let go and scanned again, announced while none is lit, and lit
        // once it comes.
        let release = held
            .iter()
            .zip(unannounced)
            .find(|(_, p)| p.by == vtium_settings::By::NeverDark);
        if let Some(((node, connector, _), _)) = release {
            if let Some((key, _)) = self.guard_lit.take() {
                self.unlight(key);
            }
            self.any_lit.set(!self.lit.is_empty());
            if let Some(video) = self.videos.get_mut(node)
                && video.release_dark(connector)
            {
                log::warn!(
                    "the display guard asks {} for display {connector} again",
                    node.display()
                );
                if let Err(e) = video.hotplug() {
                    log::warn!("{}: hotplug: {e}", node.display());
                }
            }
        }
        let guarded = inputs
            .iter()
            .map(|p| p.by == vtium_settings::By::AntiLockout);
        for (id, disposition) in self.routes.guard(&self.rules, guarded) {
            if let Some(input) = &mut self.input {
                input.set_disposition(id, disposition);
            }
        }
        if placement.anti_lockout {
            let to = placement.focus.and_then(|p| self.vt_heads.head(p));
            if let Some(to) = to.filter(|&h| Some(h) != self.focus) {
                self.move_focus(to);
            }
            if !self.locked_out {
                let to = self
                    .focus
                    .and_then(|h| self.heads.get(h))
                    .map_or("", Head::name);
                let line = crate::notice::anti_lockout(to);
                log::warn!("anti-lockout: {line}");
                if let Some(head) = self.focus.and_then(|h| self.heads.get_mut(h)) {
                    head.add_notice(&line);
                }
            }
        }
        self.locked_out = placement.anti_lockout;
        self.any_lit.set(!self.lit.is_empty());
        self.bound_pointers();
        // The guard may have sent a keyboard to another head.
        self.rehover();
    }

    /// Whether the screen the display guard lights for a notice alone has
    /// anything to show. Under `osd = silent` the file's console line does
    /// not show, so the screen shows only the two states in which vtium can
    /// serve nothing: the panic screen and the notice of a crash loop that
    /// stopped every head. They show at every level, since the person at
    /// the console has no other way to learn why nothing works.
    ///
    /// References:
    /// - ADR-1401 D9: what the display guard lights under `osd = silent`.
    /// - ADR-1401 D5, "What every level shows".
    fn screen_shows(&self) -> bool {
        !self.osd.silent() || self.stopped || self.report.is_some()
    }

    /// What the screen the display guard lights for a notice alone shows
    /// (ADR-1401 D7, D9): that a crash loop stopped every head, else the
    /// panic screen for a broken closed file, else the start's console
    /// line, else that no head has a display, each with its next step.
    fn screen_notice(&self) -> String {
        if self.stopped {
            return crate::notice::stopped();
        }
        self.report
            .as_ref()
            .map(vtium_render::NoticeReport::text)
            .or_else(|| self.outcome.dark_notice().map(crate::head::outcome_notice))
            .unwrap_or_default()
    }

    /// The display the display guard lit goes dark again: the rules light
    /// another.
    fn unlight(&mut self, key: (PathBuf, DisplayId)) {
        let Some(display) = self.lit.remove(&key) else {
            return;
        };
        if let Some(h) = self.head_of(&key.0, key.1) {
            if let Some(head) = self.heads.get_mut(h) {
                head.drop_views(|k| *k == key);
            }
            self.relayout(h);
        }
        log::info!("display {} goes dark again", key.0.display());
        self.dark.insert(key, display);
        self.darken();
    }

    /// A head's first display came, its last went, or its next came. With
    /// none the head's session runs on at the grid it had, which no
    /// display resizes, until its `when-display-lost` hangs it up (ADR-1401 D8), and
    /// the blank does not run: the blank in force ends, and the countdown
    /// starts afresh when a display comes, as at the user's input for now
    /// (vtium/vtium#87 tells the two apart). The next is drawn whole, and
    /// resizes the grid when its own differs, and stops the hang-up's
    /// time.
    pub(super) fn attachment(&mut self, now: Instant) {
        let several = self.heads.iter().filter(|h| !h.notice_only).count() > 1;
        let full = self.osd.mode == osd::Mode::Full && self.osd.shows_on_change();
        let mut changed = false;
        for head in &mut self.heads {
            let attached = if head.views.is_empty() {
                match head.attached {
                    Attached::NotYet => Attached::NotYet,
                    Attached::Yes | Attached::Gone => Attached::Gone,
                }
            } else {
                Attached::Yes
            };
            let (cols, rows) = head.grid;
            let of = if several {
                format!(" of head {}", head.name())
            } else {
                String::new()
            };
            match (head.attached, attached) {
                // A screen that shows a notice alone is not a terminal: it
                // has no session to hang up or keep, and no OSD.
                (Attached::Yes, Attached::Gone) | (Attached::Gone, Attached::Yes)
                    if head.notice_only =>
                {
                    changed = true;
                }
                (Attached::Yes, Attached::Gone) => {
                    head.drop_osd();
                    let hangup = match head.when_display_lost {
                        WhenDisplayLost::Keep => None,
                        WhenDisplayLost::Hangup => Some(0),
                        WhenDisplayLost::HangupAfter(secs) => Some(secs),
                    };
                    head.hangup_due =
                        hangup.map(|secs| after(now, Duration::from_secs(secs.into())));
                    match hangup {
                        None => log::warn!(
                            "the last display{of} is gone: the session runs on at {cols}x{rows} until the next comes"
                        ),
                        // The hang-up logs its own line.
                        Some(0) => {}
                        Some(secs) => log::warn!(
                            "the last display{of} is gone: the session hangs up in {secs} s unless one comes"
                        ),
                    }
                    changed = true;
                }
                (Attached::Gone, Attached::Yes) => {
                    head.hangup_due = None;
                    if full {
                        head.show_osd();
                    }
                    log::info!("a display{of} came: the session is drawn again at {cols}x{rows}");
                    changed = true;
                }
                (Attached::NotYet, Attached::Yes) => changed = true,
                _ => {}
            }
            head.attached = attached;
        }
        if changed {
            self.wake(now);
        }
        for h in 0..self.heads.len() {
            let due = self
                .heads
                .get(h)
                .and_then(|head| head.hangup_due)
                .is_some_and(|d| d <= now);
            if due {
                self.hang_up(h, now);
            }
        }
    }

    /// Hangs up head `h`'s sessions, which lost their last display under
    /// `when-display-lost = hangup` (ADR-1401 D8); each console then
    /// follows the respawn policy, its session starting again in the same
    /// console, after the end of its session returned what it switched or
    /// zoomed (ADR-1201 D11).
    fn hang_up(&mut self, h: usize, now: Instant) {
        let since = now.saturating_duration_since(self.epoch);
        let oneshot = self.oneshot;
        let ended = self
            .heads
            .get_mut(h)
            .map(|head| head.hang_up(since, oneshot))
            .unwrap_or_default();
        for (id, after) in ended {
            if matches!(after, AfterExit::Restart | AfterExit::Ended) {
                self.return_font(h, id);
            }
            if after == AfterExit::Restart {
                self.spawn(h, id);
            }
        }
        if let Some(exit) = session::exit_due(&self.heads) {
            self.request_exit(exit);
        }
    }
}
