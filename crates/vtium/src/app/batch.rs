//! The console's resources kept in step with the batch, the list of
//! fonts, color schemes, color maps and pointer files that root activated
//! in /etc/vtium/batch.conf. vtium reads the batch again at each new
//! login, and what changed takes effect at that login, with no restart.
//!
//! References:
//! - ADR-1205 D4, its note of 2026-11-19: every resource passes one gate, and an activation or a deactivation takes effect at the next login.

use std::path::Path;

use super::App;
use crate::config::keys;
use crate::font;

impl App {
    /// Reads the batch from `places` in place of this machine's, now and
    /// at each new login, with `rule` the trust rule each resource passes:
    /// a test's batch file, fresh-install list and fixture folder. The
    /// heads count as made under that batch.
    #[cfg(test)]
    pub(crate) fn set_batch_places(
        &mut self,
        places: vtium_settings::batch::Places,
        rule: vtium_settings::cullet::Rule,
    ) {
        self.batch_places = (places, rule);
        self.reread_batch(None);
        // the heads stand as if made under this batch, as at a start
        for head in &mut self.heads {
            head.batch = self.batch_changes;
        }
    }

    /// The console's gate for this login, which holds the batch that the
    /// font helpers, the color schemes, the color maps and the pointer
    /// files draw from.
    #[cfg(test)]
    pub(crate) fn gate(&self) -> std::sync::Arc<vtium_settings::cullet::Gate> {
        font::gate()
    }

    /// The request naming the active fonts that the console sends first to
    /// each font helper it starts; `None` when the helper cannot start.
    #[cfg(test)]
    pub(crate) fn active_fonts_request(&self) -> Option<Vec<u8>> {
        self.launcher.as_ref().ok()?.active().map(<[u8]>::to_vec)
    }

    /// Reads the batch again before a new login, as vtium reads the font
    /// keys again there, so a resource that root activated or deactivated
    /// since the last login takes effect at this one, with no restart.
    /// When the batch changed, the console's launcher takes the new active
    /// fonts before any glyph is asked for again, every color scheme is read
    /// again at its next use, and every stored glyph is dropped and drawn
    /// again. When the pointer file's place in the batch changed, the
    /// pointer file loads again through the new gate. Then head `ended`,
    /// whose session ended, catches up with the batch in force, whether or
    /// not this call found it changed: its palette, its vtlook and its font.
    /// Returns whether head `ended` must resolve its font again.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: the batch is read again at each new login, where vtium reads the font settings again.
    pub(super) fn reread_batch(&mut self, ended: Option<usize>) -> bool {
        self.batch_changed();
        ended.is_some_and(|h| self.catch_up(h))
    }

    /// Reads the batch again, and when it changed brings the console's
    /// font helpers, glyphs, color schemes and pointer file up to it.
    fn batch_changed(&mut self) {
        let before = font::gate();
        let (places, rule) = &self.batch_places;
        let Some(findings) = font::load_batch(places, &self.font_sources.trust, rule.clone())
        else {
            return;
        };
        font::log_batch_findings(&findings);
        let me = rustix::process::geteuid().as_raw();
        let gate = font::gate();
        // The launcher and the pointer file's helper take the new active
        // fonts first, so that every helper started or asked from here on
        // reads them before any ask.
        if let Ok(launcher) = &mut self.launcher {
            *launcher = launcher.clone().with_active(me, &gate.batch);
        }
        if let (Ok(launcher), Some((_, helper))) = (&self.launcher, self.pointer_file.as_mut()) {
            helper.activate(launcher);
        }
        if let Some(path) = self.config.text(&keys::POINTER_THEME) {
            let path = Path::new(path);
            if before.batch.holds(path) != gate.batch.holds(path) {
                self.reload_pointer_file();
            }
        }
        self.batch_changes = self.batch_changes.saturating_add(1);
        // A font the batch gains may draw characters that another font
        // drew or that no font had, and a font it loses may have drawn any
        // of them, so every change drops the glyphs, and every frit's own
        // cell is asked for again at its head's next login.
        for frit in self.frits.values_mut() {
            frit.own.clear();
        }
        self.drop_glyphs();
        crate::config::Config::forget_palettes();
    }

    /// Brings head `h`, whose session just ended, up to the batch in force,
    /// at every session end of the head, whether or not this session end
    /// found the batch changed: every console of the head takes the palette
    /// the batch allows. A head whose next login is the first since another
    /// head's session end found the change thus catches up at its own.
    /// When the batch changed since the head last caught up, the head
    /// chooses its vtlook again through the gate in force, so a `.cube`
    /// file root activated or deactivated takes effect at this login, and
    /// each display of the head takes the new color table. Returns whether
    /// the batch changed since the head last caught up, in which case the
    /// caller resolves the head's font again.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: every resource passes one gate, and an activation or a deactivation takes effect at the next login.
    fn catch_up(&mut self, h: usize) -> bool {
        let top = self.config.palette();
        let config = self.head_config(h);
        let changes = self.batch_changes;
        let Some(head) = self.heads.get_mut(h) else {
            return false;
        };
        head.repalette(&config, top);
        let behind = head.batch != changes;
        head.batch = changes;
        #[cfg(feature = "filters")]
        if behind {
            let chosen = crate::head::look::choose(head.name(), &config);
            if chosen != head.vtlook {
                head.vtlook = chosen;
                self.give_colors_again(h);
            }
        }
        behind
    }

    /// Drops every glyph the font helpers drew, of every frit, and asks
    /// for the screen's glyphs again, so that no glyph of a font the batch
    /// no longer holds stays drawn. Each running font helper ends, with
    /// every ask it has not answered, so the helper that answers the new
    /// asks starts under the console's launcher, which already names the
    /// new active fonts, and no answer from the old batch's fonts arrives
    /// after the drop. The glyph store keeps no record of the
    /// font each glyph came from, so vtium cannot drop the glyphs of one
    /// font alone. Root changes the batch rarely, so the cost is one
    /// refill of each screen at the login that finds the change. Each
    /// display forgets the glyphs it cached for the characters it shows
    /// and draws them again as the helper's answers arrive.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: a deactivation takes effect at the next login.
    fn drop_glyphs(&mut self) {
        let now = (self.clock)();
        for frit in self.frits.values_mut() {
            match (frit.helper.as_mut(), &self.launcher) {
                (Some(helper), Ok(launcher)) => helper.begin_again(launcher),
                (Some(helper), Err(_)) => helper.retain(&[], now),
                (None, _) => crate::deferred::lock(&frit.generated).retain(&[]),
            }
            frit.asked.clear();
        }
        for h in 0..self.heads.len() {
            let Some(head) = self.heads.get_mut(h) else {
                continue;
            };
            if head.frit().is_none() {
                continue;
            }
            let mut visible: Vec<char> = head.visible().into_iter().flat_map(|s| s.chars).collect();
            visible.sort_unstable();
            visible.dedup();
            for v in head.views.values_mut() {
                v.glyphs_changed(&visible);
            }
            head.draw_now = true;
            self.ask_glyphs(h);
        }
    }

    /// Drops the pointer file's loader, its helper and its pictures, so
    /// that the next turn loads the pointer file again through the
    /// console's new gate. A pointer file the new batch lists loads, and
    /// one it no longer lists is refused with one line, while every
    /// display draws vtium's own pictures.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: an activation or a deactivation takes effect at the next login.
    fn reload_pointer_file(&mut self) {
        self.pointer_file = None;
        self.pointer_file_started = false;
        self.pointer_store.forget();
        for head in &mut self.heads {
            head.draw_now = true;
        }
    }
}
