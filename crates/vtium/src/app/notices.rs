//! The notices on displays that no head holds: vtium wakes them, shows
//! them, silences them, and cycles them on a timer.

use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use vtium_platform::display::{DisplayId, Dpms};

use crate::head::Head;

use super::{App, Unused, after};

impl App {
    /// A key on this VT, a switch back to it or a wake from the blank
    /// starts each notice's full `osd-timeout` again (ADR-1401 D3), whether
    /// the notice is still showing or parked. The guards light a parked
    /// notice's display at their next run, and its time starts at the
    /// first turn it is on the display again.
    pub(super) fn wake_notices(&mut self) {
        for n in self.notices.values_mut() {
            n.due = None;
            if std::mem::take(&mut n.parked) {
                self.guard_due = true;
            }
        }
    }

    /// Lights the notice of each display of `unused`, each with the head of
    /// other VTs it is configured for, if any, and lets go of each notice
    /// whose display has gone or now has a path (ADR-1401 D3). A parked
    /// notice stays parked. An announced display joins its notice's head; a
    /// display its card left dark is let go and asked for again, and joins
    /// it when it comes.
    pub(super) fn show_notices(&mut self, unused: &[((PathBuf, String), Option<usize>)]) {
        // While the VT is another's, a card lists none of the displays it
        // left dark, a parked one included, so the guard can tell nothing
        // of the notices: each stays as it is, and the cards' wake-up runs
        // the guards again.
        if !self.vt.is_active() {
            return;
        }
        if self.osd.silent() {
            self.silence_notices(unused);
            return;
        }
        let gone: Vec<(PathBuf, String)> = self
            .notices
            .keys()
            .filter(|k| !unused.iter().any(|(u, _)| u == *k))
            .cloned()
            .collect();
        for key in gone {
            self.wanted.borrow_mut().remove(&key);
            let Some(n) = self.notices.remove(&key) else {
                continue;
            };
            // A display that now has a path leaves the notice's head for
            // the head its rules give it, lit as it was: it is never set
            // off on the way.
            let views: Vec<(PathBuf, DisplayId)> = self
                .heads
                .get_mut(n.head)
                .map(|h| {
                    let views: Vec<_> = h.views.keys().cloned().collect();
                    h.drop_views(|_| true);
                    views
                })
                .unwrap_or_default();
            self.spare_notices.push(n.head);
            for view in views {
                self.lit.remove(&view);
                self.add_display(&view.0, view.1);
            }
        }
        let vt = self.vt_heads.vt();
        // Which VTs vtium runs on, asked once for every notice that needs
        // it (ADR-1401 D3, "Enter on a head's own keyboard").
        let mut running = None;
        for (key, owner) in unused {
            let head = match self.notices.get(key) {
                Some(n) if n.parked => continue,
                Some(n) => {
                    let head = n.head;
                    let text = self.unused_text(vt, &key.1, *owner, &mut running);
                    if let Some(n) = self.notices.get_mut(key)
                        && n.text != text
                    {
                        if let Some(h) = self.heads.get_mut(head) {
                            h.set_notice(&text);
                        }
                        log::info!("display {}: {text}", key.1);
                        n.text = text;
                    }
                    head
                }
                None => {
                    let text = self.unused_text(vt, &key.1, *owner, &mut running);
                    let spare = self
                        .spare_notices
                        .pop()
                        .filter(|&h| self.heads.get(h).is_some_and(|h| h.views.is_empty()));
                    let head = match spare {
                        Some(h) => h,
                        None => {
                            let term = crate::head::terminal(
                                &self.config,
                                (0, 0),
                                0,
                                Rc::clone(&self.clock),
                            );
                            self.heads.push(Head::notice_screen(&self.config, term));
                            self.heads.len().saturating_sub(1)
                        }
                    };
                    if let Some(h) = self.heads.get_mut(head) {
                        h.set_notice(&text);
                    }
                    log::info!("display {}: {text}", key.1);
                    self.notices.insert(
                        key.clone(),
                        Unused {
                            head,
                            due: None,
                            parked: false,
                            text,
                        },
                    );
                    head
                }
            };
            if self.heads.get(head).is_some_and(|h| !h.views.is_empty()) {
                continue;
            }
            // an announced display the rules left dark joins its notice
            let announced = self
                .dark
                .iter()
                .find(|((node, _), d)| *node == key.0 && d.connector.as_ref() == Some(&key.1))
                .map(|(k, _)| k.clone());
            if let Some(k) = announced {
                self.dark.remove(&k);
                if let Some(video) = self.videos.get_mut(&k.0)
                    && let Err(e) = video.set_dpms(k.1, Dpms::On)
                {
                    log::warn!("cannot set display {} on: {e}", video.name(k.1));
                }
                self.add_display(&k.0, k.1);
                continue;
            }
            // a display its card left dark is asked for again
            self.wanted.borrow_mut().insert(key.clone());
            if let Some(video) = self.videos.get_mut(&key.0)
                && video.release_dark(&key.1)
                && let Err(e) = video.hotplug()
            {
                log::warn!("{}: hotplug: {e}", key.0.display());
            }
        }
    }

    /// Under `osd = silent`, a display of `unused`, which no path catches
    /// on this VT, shows no notice. vtium leaves it off, since the rules
    /// place it nowhere, and a key press or a switch back to this VT leaves
    /// it off too. The log names each display and what its notice would
    /// say, once while the display stays unused. Such a display ends the
    /// start's wait for a display: a display came, and vtium leaves it off
    /// on purpose, so vtium must not exit and hand the VT to a getty.
    ///
    /// References:
    /// - ADR-1401 D3, "What a display with no path shows".
    fn silence_notices(&mut self, unused: &[((PathBuf, String), Option<usize>)]) {
        let vt = self.vt_heads.vt();
        self.silenced
            .retain(|key| unused.iter().any(|(u, _)| u == key));
        for (key, owner) in unused {
            if !self.silenced.insert(key.clone()) {
                continue;
            }
            let named =
                owner
                    .and_then(|h| self.rules.heads.get(h))
                    .map(|head| crate::notice::Owner {
                        head: &head.name,
                        vts: &head.vts,
                    });
            let text = crate::notice::unused_display(vt, &key.1, named.as_ref());
            log::info!(
                "display {}: {text} Because osd = silent, vtium shows no notice and leaves the display off.",
                key.1
            );
            self.presented = true;
        }
    }

    /// Each notice on its display for `osd-timeout` since the first turn it
    /// was there is put away: vtium parks its display, so it sends no signal
    /// and holds no memory, or sets it off where the device cannot park it
    /// (ADR-1401 D3). Under `osd-timeout = never` a notice stays.
    pub(super) fn notice_cycle(&mut self, now: Instant) {
        let Some(timeout) = self.osd.timeout else {
            return;
        };
        let mut due = Vec::new();
        for (key, n) in self.notices.iter_mut().filter(|(_, n)| !n.parked) {
            let Some(head) = self.heads.get(n.head) else {
                continue;
            };
            let Some(view) = head.views.keys().next() else {
                continue;
            };
            let at = *n.due.get_or_insert_with(|| after(now, timeout));
            if now >= at {
                due.push((key.clone(), view.clone()));
            }
        }
        for (key, view) in due {
            self.park_notice(&key, view);
        }
    }

    /// Puts away the notice of the display at `view`, named `key`: parked
    /// by its device, which reports it removed and keeps it dark until let
    /// go, or set off where the device cannot park a display.
    fn park_notice(&mut self, key: &(PathBuf, String), view: (PathBuf, DisplayId)) {
        let Some(n) = self.notices.get_mut(key) else {
            return;
        };
        n.parked = true;
        n.due = None;
        // The notices that [`App::notice`] added under the display's own
        // go with the display.
        if let Some(head) = self.heads.get_mut(n.head) {
            head.set_notice(&n.text);
        }
        self.wanted.borrow_mut().remove(key);
        let Some(video) = self.videos.get_mut(&view.0) else {
            return;
        };
        match video.park(view.1) {
            Ok(()) => log::info!(
                "display {}: the notice's time is up, so the display is off and its buffers freed until a key on this VT",
                key.1
            ),
            Err(e) => {
                if e.kind() != io::ErrorKind::Unsupported {
                    log::warn!("cannot park display {}: {e}", key.1);
                }
                if let Err(e) = video.set_dpms(view.1, Dpms::Off) {
                    log::warn!("cannot set display {} off: {e}", key.1);
                }
                let h = self.notices.get(key).map(|n| n.head);
                if let Some(head) = h.and_then(|h| self.heads.get_mut(h)) {
                    head.drop_views(|k| *k == view);
                }
                if let Some(display) = self.lit.remove(&view) {
                    self.dark.insert(view, display);
                }
            }
        }
    }
}
