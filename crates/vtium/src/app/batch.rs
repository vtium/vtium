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
    /// a test's batch file, fresh-install list and fixture folder.
    #[cfg(test)]
    pub(crate) fn set_batch_places(
        &mut self,
        places: vtium_settings::batch::Places,
        rule: vtium_settings::cullet::Rule,
    ) {
        self.batch_places = (places, rule);
        self.reread_batch(None);
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
    /// When the batch changed, the console's launcher and each running font
    /// helper are told the new active fonts, every color scheme is read
    /// again at its next use, and head `h`'s palette is read again for its
    /// console `id`, whose next session starts with it. The other heads keep
    /// their palette until their own next login. When the new batch no
    /// longer lists a resource the old one listed, every stored glyph is
    /// dropped and drawn again, because a glyph may come from a font that
    /// is no longer active. When the pointer file's place in the batch
    /// changed, the pointer file loads again through the new gate.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: the batch is read again at each new login, where vtium reads the font settings again.
    pub(super) fn reread_batch(&mut self, ended: Option<(usize, u64)>) {
        let before = font::gate();
        let (places, rule) = &self.batch_places;
        let Some(findings) = font::load_batch(places, &self.font_sources.trust, rule.clone())
        else {
            return;
        };
        font::log_batch_findings(&findings);
        let me = rustix::process::geteuid().as_raw();
        let gate = font::gate();
        if before
            .batch
            .entries()
            .iter()
            .any(|e| !gate.batch.entries().contains(e))
        {
            self.drop_glyphs();
        }
        if let Some(path) = self.config.text(&keys::POINTER_THEME) {
            let path = Path::new(path);
            if before.batch.holds(path) != gate.batch.holds(path) {
                self.reload_pointer_file();
            }
        }
        if let Ok(launcher) = &mut self.launcher {
            *launcher = launcher.clone().with_active(me, &gate.batch);
        }
        if let Ok(launcher) = &self.launcher {
            for frit in self.frits.values_mut() {
                if let Some(helper) = frit.helper.as_mut() {
                    helper.activate(launcher);
                }
            }
            if let Some((_, helper)) = self.pointer_file.as_mut() {
                helper.activate(launcher);
            }
        }
        crate::config::Config::forget_palettes();
        if let Some((h, id)) = ended {
            let top = self.config.palette();
            let config = self.head_config(h);
            if let Some(head) = self.heads.get_mut(h) {
                head.repalette(&config, top, id);
            }
        }
    }

    /// Drops every glyph the font helpers drew, of every frit, and asks
    /// for the screen's glyphs again, so that no glyph of a font the batch
    /// no longer holds stays drawn. The glyph store keeps no record of the
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
            match frit.helper.as_mut() {
                Some(helper) => helper.retain(&[], now),
                None => crate::deferred::lock(&frit.generated).retain(&[]),
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
