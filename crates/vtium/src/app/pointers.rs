//! The pointer files the console loads: one for the top level's
//! `pointer-theme`, and one for each other theme a head's look names.
//!
//! Each theme has its own loader, its own font helper and its own store of
//! pictures. A display's view reads the store of the theme its head's look
//! names, so two heads whose looks name different themes draw different
//! pointers. A look that names no theme takes the top level's. Two looks
//! that name the same path share one loader and one store, so vtium reads
//! and draws each theme once.
//!
//! References:
//! - ADR-1205 D6: each look picks its pointer theme, and the top level's is the default for a look that names none.
//! - ADR-1309 D5: the compiled-in pictures draw until the font helper answers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::App;
use crate::config::keys;
use crate::helper::Helper;
use crate::pointer_file::{Loader, Store};

/// One pointer file the console loads, by the path `pointer-theme` gives,
/// or none where it is unset.
#[derive(Default)]
pub(super) struct Theme {
    /// The path `pointer-theme` names, `None` for vtium's own pictures.
    path: Option<PathBuf>,
    /// Whether the loader has been started, or the path found unusable.
    started: bool,
    /// The pointer file while it loads, with the font helper that draws
    /// its pictures.
    file: Option<(Loader, Helper)>,
    /// The pointer file's pictures and the heights its displays ask for,
    /// which the loader shares with each view that draws this theme.
    store: Store,
}

impl Theme {
    fn new(path: Option<PathBuf>) -> Theme {
        Theme {
            path,
            ..Theme::default()
        }
    }

    /// Drops the loader, its helper and its pictures, so that the next
    /// turn loads the file again through the console's new gate. Every
    /// view that reads the store draws vtium's own pictures meanwhile.
    fn reload(&mut self) {
        self.file = None;
        self.started = false;
        self.store.forget();
    }
}

impl App {
    /// The index in `pointers` of the theme at `path`, added when no
    /// theme there has that path.
    fn pointer_theme(&mut self, path: Option<PathBuf>) -> usize {
        match self.pointers.iter().position(|t| t.path == path) {
            Some(i) => i,
            None => {
                self.pointers.push(Theme::new(path));
                self.pointers.len().saturating_sub(1)
            }
        }
    }

    /// The `pointer-theme` of the top level, with the command line over
    /// it.
    fn top_pointer_path(&self) -> Option<PathBuf> {
        self.config.text(&keys::POINTER_THEME).map(PathBuf::from)
    }

    /// The store of the pointer file that head `h` draws: the theme its
    /// look names, else the top level's. The theme starts loading at the
    /// next turn.
    pub(super) fn pointer_store(&mut self, h: usize) -> Store {
        let path = self
            .head_config(h)
            .text(&keys::POINTER_THEME)
            .map(PathBuf::from);
        let i = self.pointer_theme(path);
        self.pointers
            .get(i)
            .map(|t| t.store.clone())
            .unwrap_or_default()
    }

    /// The store of the top level's pointer file, for a test.
    #[cfg(test)]
    pub(crate) fn top_pointer_store(&mut self) -> Store {
        let path = self.top_pointer_path();
        let i = self.pointer_theme(path);
        self.pointers
            .get(i)
            .map(|t| t.store.clone())
            .unwrap_or_default()
    }

    /// Loads each pointer file a head draws, and the top level's, from the
    /// first turn on: starts each loader not yet started, then for each
    /// one takes the font helper's answer to the last pointer ask, sends
    /// the next ask, and draws every head once the pictures change, so
    /// that each display's pointer takes them. A loader and its helper go
    /// once the loader asks for nothing more. A helper that cannot start
    /// leaves vtium's own pictures for that theme.
    ///
    /// References:
    /// - ADR-1309 D5: the compiled-in pictures draw until the font helper answers.
    pub(super) fn pointer_turn(&mut self, now: Instant) {
        let top = self.top_pointer_path();
        self.pointer_theme(top);
        let size = self
            .config
            .number(&keys::POINTER_SIZE)
            .and_then(|n| u32::try_from(n).ok());
        let mut changed = false;
        for i in 0..self.pointers.len() {
            let start = self.pointers.get_mut(i).filter(|t| !t.started).map(|t| {
                t.started = true;
                (t.path.clone(), t.store.clone())
            });
            if let Some((Some(path), store)) = start {
                let file = self.start_pointer_file(&path, size, &store);
                if let Some(t) = self.pointers.get_mut(i) {
                    t.file = file;
                }
            }
            if let Some(t) = self.pointers.get_mut(i) {
                changed |= turn(&mut t.file, now);
            }
        }
        if changed {
            for head in &mut self.heads {
                head.draw_now = true;
            }
        }
    }

    /// A loader for the pointer file at `path`, which publishes into
    /// `store`, with a font helper of its own; `None`, and the line logged,
    /// when the path names nothing vtium uses or the helper cannot start.
    fn start_pointer_file(
        &self,
        path: &Path,
        size: Option<u32>,
        store: &Store,
    ) -> Option<(Loader, Helper)> {
        let loader = Loader::start(path, size, store)?;
        match self
            .launcher
            .clone()
            .and_then(|l| self.helper_wake().map(|w| (l, w)))
        {
            Ok((launcher, wake)) => {
                store.set_wake(Arc::clone(&wake));
                Some((loader, Helper::for_pointers(launcher, wake, self.epoch)))
            }
            Err(why) => {
                log::warn!("{}", crate::pointer_file::lost_line(path, &why));
                None
            }
        }
    }

    /// The deadlines of the font helpers that draw pointer files.
    pub(super) fn pointer_deadlines(&self) -> impl Iterator<Item = Instant> + '_ {
        self.pointers
            .iter()
            .filter_map(|t| t.file.as_ref().and_then(|(_, h)| h.deadline()))
    }

    /// Gives each pointer file's helper the launcher's new active fonts,
    /// and loads again each pointer file whose place in the batch changed
    /// between `before` and `after`. A pointer file the new batch lists
    /// loads, and one it no longer lists is refused with one line, while
    /// every display that draws it shows vtium's own pictures.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: an activation or a deactivation takes effect at the next login.
    pub(super) fn rebatch_pointers(
        &mut self,
        before: &vtium_settings::batch::Batch,
        after: &vtium_settings::batch::Batch,
    ) {
        let mut reloaded = false;
        for t in &mut self.pointers {
            if let (Ok(launcher), Some((_, helper))) = (&self.launcher, t.file.as_mut()) {
                helper.activate(launcher);
            }
            if t.path
                .as_deref()
                .is_some_and(|p| before.holds(p) != after.holds(p))
            {
                t.reload();
                reloaded = true;
            }
        }
        if reloaded {
            for head in &mut self.heads {
                head.draw_now = true;
            }
        }
    }
}

/// One turn of the loader in `file`: takes the font helper's answer to the
/// last ask, sends the next ask, and publishes the pictures. Returns
/// whether the pictures changed. The loader and its helper go once the
/// loader asks for nothing more.
fn turn(file: &mut Option<(Loader, Helper)>, now: Instant) -> bool {
    let Some((loader, helper)) = file else {
        return false;
    };
    helper.poll(now);
    loop {
        if let Some(answer) = helper.pointer_answer() {
            loader.take(answer);
        }
        if helper.busy() {
            break;
        }
        match loader.next_ask() {
            Some(ask) => helper.pointer(&ask, now),
            None => break,
        }
    }
    loader.emit();
    let changed = !helper.busy() && loader.publish();
    if loader.ended() && !helper.busy() {
        *file = None;
    }
    changed
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, and a test fails by panicking"
)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use vtium_settings::Trust;
    use vtium_settings::batch::Places;
    use vtium_settings::cullet::Rule;

    use super::super::App;
    use super::super::tests::{Still, no_fonts, seat_with, settle, shown_at, start};
    use crate::font;
    use crate::scratch::Scratch;

    /// Writes `text` to `path`, readable by everyone and written by its
    /// owner alone, as the trust rule asks.
    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    // Two heads on two displays: head a's look names its own pointer file,
    // and head b's look names none. Each display asks the pointer file of
    // its own head for its cell height, so head a's display asks the
    // look's file and head b's display asks the top level's.
    //
    // References:
    // - ADR-1205 D6: each look picks its pointer theme, and a look that names none takes the top level's.
    #[test]
    fn each_head_draws_the_pointer_theme_its_look_names() {
        let dir = Scratch::new("look-pointers");
        std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"/>\n";
        let (top, own) = (dir.join("top.svg"), dir.join("own.svg"));
        write(&top, svg);
        write(&own, svg);
        let file = dir.join("batch.conf");
        write(&file, &format!("{}\n{}\n", top.display(), own.display()));
        let me = rustix::process::geteuid().as_raw();
        let fonts = font::Sources {
            trust: Trust::owner(me).below(&dir),
            ..no_fonts()
        };
        let heads = format!(
            "version = 1\n[look own]\npointer-theme = {}\n[look plain]\n\
             [head a]\nlook = own\n[head b]\nlook = plain\n[rules]\n\
             display connector:mem/A -> a\ndisplay connector:mem/B -> b\n",
            own.display()
        );
        let theme = format!("--pointer-theme={}", top.display());
        let (mut lp, mut app) = seat_with(&[theme.as_str()], None, fonts);
        app.set_heads(start(&heads));
        app.set_batch_places(
            Places {
                file,
                vconsole_conf: PathBuf::from("/dev/null"),
                cmdline: PathBuf::from("/dev/null"),
                ..Places::system()
            },
            Rule::new(Trust::owner(me)).below(&dir),
        );
        app.add_video(
            PathBuf::from("/dev/mem0"),
            Box::new(Still::new(&[(1, "A"), (2, "B")])),
        );
        let t = Instant::now();
        shown_at(&mut app, t);
        settle(&mut lp, &mut app, t);
        // a turn loads the pointer files, and each display's next frame
        // asks its head's file
        app.turn(t);
        settle(&mut lp, &mut app, t);
        let asked = |app: &App, path: &Path| {
            app.pointers
                .iter()
                .find(|p| p.path.as_deref() == Some(path))
                .map(|p| p.store.asked().len())
        };
        assert_eq!(
            asked(&app, &own),
            Some(1),
            "head a's display asks its look's file"
        );
        assert_eq!(
            asked(&app, &top),
            Some(1),
            "head b's display asks the top level's file"
        );
        assert_eq!(app.pointers.len(), 2, "one loader for each file");
    }
}
