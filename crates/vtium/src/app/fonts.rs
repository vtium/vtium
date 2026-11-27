//! The font each head draws with and its zoom: the font set a head's
//! configuration names, the character cell it draws at, the zoom keys, and
//! the font a head goes back to.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use vtium_render::Orientation;

use crate::config::Config;
use crate::config::keys;
use crate::font::{self, Chosen};
use crate::head::{Climb, Frit, Head};
use crate::helper::{self, FritText};
use crate::layout::{self, Zoom};
use crate::ordered;

use super::window_ops::NoStep;
use super::{App, after, logged_name};

impl App {
    /// Chooses head `h`'s font once, at its first display (ADR-1200 D9,
    /// ADR-1401 D3), as [`App::configured_font`] gives it.
    pub(super) fn choose_font(&mut self, h: usize) {
        self.choose_font_adding(h, None);
    }

    /// [`App::choose_font`] while a display of size `adding` is added to
    /// head `h`, before the display has a view, so that the choice counts
    /// its size.
    pub(super) fn choose_font_adding(&mut self, h: usize, adding: Option<(u32, u32)>) {
        if self.heads.get(h).is_none_or(|head| head.font.is_some()) {
            return;
        }
        let (chosen, made) = self.configured_font(h, adding);
        if let Some(head) = self.heads.get_mut(h) {
            head.font = Some(chosen);
            head.made = made;
        }
        self.share(h);
    }

    /// The configuration head `h` reads the keys a section may set from:
    /// the top level, with the command line over it, and the head's
    /// sections over both (ADR-1403 D5). With no such head, the top level.
    pub(super) fn head_config(&self, h: usize) -> Config {
        match self.heads.get(h) {
            Some(head) if !head.settings.is_empty() => {
                let path = crate::config::path(&self.config);
                self.config.for_head(&path, &head.settings).0
            }
            _ => self.config.clone(),
        }
    }

    /// The font the configuration gives head `h` (ADR-1200 D9, ADR-1401
    /// D3): the head's own font, else the frit `font-name` or vconsole
    /// names, or the one-source frit made on the fly for a kbd name, a
    /// path or vconsole's font, generated at run time (ADR-1301 D2); else
    /// the built-in font, with one line naming the font and why. The font
    /// draws in the cell the head's look states, the built-in font as a
    /// frit does (ADR-1200 D7, ADR-1403 D5). With it comes the frit made
    /// on the fly, where no frit bears the name, which OSC 50 switches
    /// back to by that name ([`font::by_name`]). With no font named, a head
    /// on a small display, its views' sizes and `adding` among them, draws
    /// the built-in font in place of the default set, but only at the
    /// automatic height, where the head's rows are 8 pixels high. A
    /// configured `font-size`, or a cell the head's look states, sets the
    /// height itself, and the default set draws there.
    ///
    /// References:
    /// - ADR-1204 D2: the default set, and the built-in font's 8×8 strike
    ///   where the small-display rule picks 8-pixel rows.
    fn configured_font(&self, h: usize, adding: Option<(u32, u32)>) -> (Chosen, Option<FritText>) {
        let Some(head) = self.heads.get(h) else {
            return (Chosen::BUILTINS, None);
        };
        let look = head.look_cell();
        let s = &self.font_sources;
        let wanted = match &head.font_name {
            Some(name) => Some(font::Wanted {
                name: name.clone(),
                came: font::Came::Config,
            }),
            None => font::wanted(
                &self.head_config(h),
                &font::read_text(&s.vconsole_conf),
                &font::read_text(&s.kernel_cmdline),
            ),
        };
        let sizes = head.views.values().map(|v| v.size).chain(adding);
        let automatic = !head.sized() && look.is_none();
        let Some(wanted) = wanted.filter(|w| {
            w.came != font::Came::Default || !automatic || font::default_set_fits(sizes)
        }) else {
            return (Chosen::builtins(look), None);
        };
        if let Some(builtin) = font::builtin_named(&wanted.name, look) {
            return (builtin, None);
        }
        // The frit's cell is read as text before the first frame; its fonts
        // are the helper's to parse (#145).
        let on_the_fly = font::frit(&s.frits, &wanted.name).is_none();
        let frit = match font::frit_named(s, &wanted) {
            Ok(frit) => frit,
            // the one line a frit vtium does not read gets at the start: the
            // start's warnings leave it to this read
            Err(refusal) => {
                log::warn!("{refusal} vtium draws its built-in font instead.");
                return (Chosen::builtins(look), None);
            }
        };
        let made = on_the_fly.then(|| frit.clone());
        match Chosen::frit(frit, head.sized(), look).map(|chosen| (chosen, made)) {
            Ok((chosen, made)) => {
                log::info!(
                    "the font is the frit {}, drawn in the built-in font until its glyphs are generated",
                    chosen.name(0, &self.locale)
                );
                (chosen, made)
            }
            Err(why) => {
                log::warn!(
                    "the font {} is not drawn ({why}): vtium draws its built-in font",
                    wanted.name
                );
                (Chosen::builtins(look), None)
            }
        }
    }

    /// A frit that states no cell takes the cell its first source gives at
    /// `height`, the height head `h` draws at, which the helper is asked
    /// for and waited for at most [`helper::OWN_CELL_MS`] (ADR-1200 D17):
    /// a bitmap source's own cell, where no
    /// `font-size` is configured, so a bitmap font draws at its own size
    /// (the operator, 2026-10-28), or an outline source's cell at
    /// `height` (the maintainer, 2026-11-01). Each answer is kept for the
    /// frit, by the height asked at, for every head drawing it. A refusal,
    /// or no answer in time, leaves the display's cell, or for an outline
    /// frit the built-in font's width, with one line saying why; an answer
    /// that comes after the wait is taken by [`App::late_cell`]. A frit
    /// whose first source is a bitmap font with strikes asks at each
    /// height, as an outline frit does, since each height can draw another
    /// strike, and its answers hold at the height asked at alone.
    ///
    /// References:
    /// - ADR-1204 D2: the strike for the automatic height, for a configured
    ///   font size and for each zoom step.
    pub(super) fn own_cell(&mut self, h: usize, height: u32) {
        self.own_cell_within(h, height, Duration::from_millis(helper::OWN_CELL_MS));
    }

    /// [`App::own_cell`], waiting at most `wait` for the helper's answer.
    pub(super) fn own_cell_within(&mut self, h: usize, height: u32, wait: Duration) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let sized = head.sized();
        if !head
            .font
            .as_ref()
            .is_some_and(|f| f.wants_own_cell(height, sized))
        {
            return;
        }
        let outline = head.chosen().outline();
        let struck = head.chosen().struck();
        let Some(name) = head.frit().map(str::to_owned) else {
            return;
        };
        let known = self.frits.get(&name).and_then(|f| {
            if struck {
                f.own.get(&height).cloned()
            } else {
                f.answer_at(height, outline)
            }
        });
        let own = match known {
            Some(own) => own,
            None => {
                let clock = Rc::clone(&self.clock);
                let Some(helper) = self.helper(&name) else {
                    return;
                };
                let own = helper.own_cell(height, wait, &*clock);
                // A font set with strikes, like an outline one, draws a
                // cell at each height, as wide as the built-in font where
                // the helper gives none.
                match (&own, outline || struck) {
                    (Ok(helper::OwnCell::Bitmap(cell)), _) if struck => log::info!(
                        "the font set {name} draws its {}x{} strike at {height} pixels",
                        cell.width,
                        cell.height
                    ),
                    (Ok(helper::OwnCell::Bitmap(cell)), _) => log::info!(
                        "the frit {name} draws in its fonts' own {}x{} cell",
                        cell.width,
                        cell.height
                    ),
                    (Ok(helper::OwnCell::Outline(cell)), _) => log::info!(
                        "the frit {name} draws in its outline font's {}x{} cell at {height} pixels",
                        cell.width,
                        cell.height
                    ),
                    (Err(why), false) => log::warn!(
                        "the frit {name}'s own cell is not known ({why}): it draws in the display's cell"
                    ),
                    (Err(why), true) => log::warn!(
                        "the frit {name}'s cell at {height} pixels is not known ({why}): it draws as wide as the built-in font"
                    ),
                }
                if let Some(frit) = self.frits.get_mut(&name) {
                    frit.own.insert(height, own.clone());
                }
                own
            }
        };
        if let (Ok(answer), Some(head)) = (own, self.heads.get_mut(h))
            && let Some(font) = &mut head.font
        {
            font.set_own_cell_at(answer, height, sized);
        }
    }

    /// Head `h`'s frit joins the frits in use: a frit another head draws
    /// already gives the head its store, and one no head draws yet is
    /// added, its helper started at its first need.
    pub(super) fn share(&mut self, h: usize) {
        let Some(Chosen::Frit {
            name,
            frit,
            generated,
            ..
        }) = self.heads.get_mut(h).and_then(|head| head.font.as_mut())
        else {
            return;
        };
        match self.frits.get(name.as_str()) {
            Some(shared) => *generated = std::sync::Arc::clone(&shared.generated),
            None => {
                self.frits.insert(
                    name.clone(),
                    Frit {
                        frit: frit.clone(),
                        generated: std::sync::Arc::clone(generated),
                        helper: None,
                        refused: false,
                        asked: Vec::new(),
                        own: BTreeMap::new(),
                    },
                );
            }
        }
    }

    /// Drops each frit no head draws any more, and with it its helper. The
    /// record of what the frit's helpers used up stays in `failures`.
    fn prune(&mut self) {
        let used = ordered::set(self.heads.iter().filter_map(Head::frit).map(str::to_owned));
        self.frits.retain(|name, _| used.contains(name));
    }

    /// The name of the font set the shown head draws, as OSC 50's query
    /// reports it.
    pub fn font_name(&self) -> &str {
        self.shown().map_or("unifont", |head| {
            head.chosen().name(head.font_size, &self.locale)
        })
    }

    /// OSC 50 asked head `h` for the font `name`. A frit, a built-in font,
    /// or the path of a font that root installed and trusts switches the
    /// head's live console as a zoom does, on a new grid. A frit, the one
    /// vtium makes for such a path included, comes with the helper and the
    /// store that every head drawing it shares. Anything else leaves the
    /// font as it was. The first refusal since the last switch is logged
    /// with its reason and the rest are counted, so a program's flood of
    /// requests logs one line.
    ///
    /// References:
    /// - ADR-1200 D5: what OSC 50 switches to, and the four checks a font's path passes.
    pub(super) fn switch_font(&mut self, h: usize, name: &str) {
        self.choose_font(h);
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let same = match head.chosen() {
            Chosen::Frit { name: now, .. } => now == name,
            Chosen::Builtin { name: Some(n), .. } => *n == name,
            Chosen::Builtin { name: None, .. } => false,
        };
        if same {
            return;
        }
        let (sized, look) = (head.sized(), head.look_cell());
        let was = head.frit().map(str::to_owned);
        let new = font::by_name(
            &self.font_sources,
            name,
            sized,
            look,
            head.made.as_ref(),
            &font::font_folders(&self.config),
            &font::gate(),
        );
        let new = new.and_then(|new| {
            let Some(head) = self.heads.get_mut(h) else {
                return Ok(());
            };
            let old = head.font.replace(new);
            self.share(h);
            self.retry_late_cell(h);
            if let Some(head) = self.heads.get_mut(h) {
                head.refont = true;
            }
            if self.relayout(h) {
                return Ok(());
            }
            let Some(head) = self.heads.get_mut(h) else {
                return Ok(());
            };
            head.font = old;
            head.refont = true;
            let size = head.font_size;
            self.relayout(h);
            Err(format!("it draws nothing at {size} pixels"))
        });
        self.prune();
        // The frit the head left, where another head still draws it, keeps
        // the cells of the heads drawing it alone.
        if let Some(other) = was.and_then(|was| {
            self.heads
                .iter()
                .position(|head| head.frit() == Some(was.as_str()))
        }) {
            self.ask_glyphs(other);
        }
        let wait = new.is_ok() && self.waits_for_cell(h);
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let current = head.chosen().name(head.font_size, &self.locale).to_owned();
        match new {
            Ok(()) => {
                head.switched = Some(head.console.id);
                if wait {
                    head.console.term.terminal.hold_replies();
                }
                let refused = std::mem::take(&mut head.font_refused);
                if refused > 1 {
                    log::info!(
                        "OSC 50 was refused {} more times before this switch",
                        refused.saturating_sub(1)
                    );
                }
                log::info!("OSC 50: the font is now {}", logged_name(&current));
            }
            Err(why) => {
                if head.font_refused == 0 {
                    log::warn!(
                        "OSC 50 asked for the font {}: {}; the font stays {}, and OSC 50's further refusals are counted until a switch",
                        logged_name(name),
                        why.trim_end_matches('.'),
                        logged_name(&current)
                    );
                }
                head.font_refused = head.font_refused.saturating_add(1);
            }
        }
    }

    /// A zoom of head `h`: the base height the font's next step, larger or
    /// smaller ([`Chosen::zoom`]), which ends `font-size`'s following of
    /// the displays (ADR-1200 D9). Below the smallest it stops, with a
    /// line naming the set and its smallest height. A height with no font
    /// keeps the one in use.
    pub(super) fn zoom(&mut self, h: usize, dir: Zoom) {
        self.drop_climb(h);
        self.choose_font(h);
        let dpi = self.dpi(h);
        if self.heads.get(h).is_some_and(|head| head.chosen().struck()) {
            self.zoom_struck(h, dir);
            return;
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let chosen = head.chosen();
        let Some(size) = chosen.zoom(head.font_size, dir, &self.locale, dpi) else {
            if dir == Zoom::Out {
                let smallest = chosen.smallest(&self.locale);
                let name = chosen.name(smallest, &self.locale).to_owned();
                self.zoom_stops(h, smallest, None, &name);
            }
            return;
        };
        // The zoom fills the displays again, in place of a grid a program
        // asked for.
        self.take_size(h, size, None);
    }

    /// A zoom of head `h` whose frit's first source is a bitmap font with
    /// strikes: the head draws at the next height in the zoom's direction
    /// whose cell differs, the height of a strike or a whole multiple of
    /// the tallest, which `strike_step` finds by asking the helper. A zoom
    /// out stops where the helper says that no strike is that short, with
    /// a line naming the smallest height, and where the helper could not
    /// answer, with a line saying why. When the helper has yet to answer at
    /// a height the step needs, the head keeps its size, and vtium takes
    /// the step once the late answer comes ([`App::resume_climbs`]). A
    /// second zoom key pressed meanwhile takes the place of the first.
    ///
    /// References:
    /// - ADR-1204 D2: the cell of the tallest strike no taller than each
    ///   zoom step's height, and a whole multiple of the tallest strike's
    ///   cell above it.
    fn zoom_struck(&mut self, h: usize, dir: Zoom) {
        let Some(size) = self.heads.get(h).map(|head| head.font_size) else {
            return;
        };
        let end = after((self.clock)(), Duration::from_millis(helper::OWN_CELL_MS));
        match self.strike_step(h, size, dir, end) {
            Ok(next) => {
                self.take_size(h, next, None);
                return;
            }
            Err(NoStep::Owed) => {
                if let Some(head) = self.heads.get_mut(h) {
                    head.climb = Some(Climb::Zoom(dir));
                }
                return;
            }
            Err(NoStep::End) => {}
        }
        if dir != Zoom::Out {
            return;
        }
        let Some(head) = self.heads.get(h) else {
            return;
        };
        // strike_step asks one pixel below `size` for a step out, so its
        // answer there says why there is no step.
        let answer = size.checked_sub(1).and_then(|at| {
            head.frit()
                .and_then(|name| self.frits.get(name))
                .and_then(|frit| frit.own.get(&at))
                .cloned()
        });
        let name = head.chosen().name(size, &self.locale).to_owned();
        match answer {
            Some(Err(no)) if no.from_frit => self.zoom_stops(h, size, None, &name),
            Some(Err(no)) if !no.late => self.zoom_stops(h, size, Some(&no.why), &name),
            _ => {}
        }
    }

    /// Logs that a zoom out of head `h` stops at `size`, the smallest
    /// height of the font set `name`, or with `why`, because the font
    /// helper could not answer. A head logs the stop once for each height,
    /// however often a key asks for it.
    fn zoom_stops(&mut self, h: usize, size: u32, why: Option<&str>, name: &str) {
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        if head.zoom_stop_logged == Some(size) {
            return;
        }
        head.zoom_stop_logged = Some(size);
        match why {
            None => log::info!(
                "head {}: the font set {name} draws no smaller than {size} pixels, so a zoom out stops there",
                head.name()
            ),
            Some(why) => log::warn!(
                "head {}: the font helper could not say which strike of the font set {name} draws below {size} pixels ({why}), so a zoom out stops there",
                head.name()
            ),
        }
    }

    /// Head `h` draws at the base height `size`, which ends `font-size`'s
    /// following of the displays (ADR-1200 D9), on the grid `asked`, or
    /// filling the displays with `None`: the zoom path, which a zoom and a
    /// program's XTWINOPS 8 take. A height with no font keeps the size in
    /// use. Returns whether the head draws at `size`.
    pub(super) fn take_size(&mut self, h: usize, size: u32, asked: Option<(usize, usize)>) -> bool {
        self.drop_climb(h);
        let Some(head) = self.heads.get_mut(h) else {
            return false;
        };
        let outline = head.chosen().outline();
        let (old, auto, points, was) = (head.font_size, head.auto_size, head.points, head.asked);
        head.auto_size = false;
        head.points = None;
        head.font_size = size;
        head.asked = asked;
        // An outline frit asks the helper again at each zoom, a height
        // whose answer failed before included.
        if let Some(frit) = head
            .frit()
            .filter(|_| outline)
            .and_then(|n| self.frits.get_mut(n))
        {
            frit.own.retain(|_, answer| answer.is_ok());
        }
        if self.relayout(h) {
            return true;
        }
        if let Some(head) = self.heads.get_mut(h) {
            head.font_size = old;
            head.auto_size = auto;
            head.points = points;
            head.asked = was;
        }
        false
    }

    /// The session in head `h`'s console `id` ended, or the console closed.
    /// vtium first reads the batch again, as at each new login. When the
    /// batch changed, the font helpers start again with the new active
    /// fonts, every stored glyph is dropped and asked for again, the palette
    /// files are read again, every head takes its new palette, and the
    /// pointer file may load again. Head `h` then chooses its vtlook again
    /// and resolves its font again when the batch changed since the head
    /// last caught up with it.
    ///
    /// Where an OSC 50 switch from that console, or from any console of a
    /// head left with one, changed the head's font, vtium returns the head
    /// to the font the configuration gives it, read again. Where the font
    /// returns or a zoom left the console off the configured size, the
    /// console returns to that size. Where the `font-size` read again turns
    /// the configured size on or off, vtium rebuilds the frit the head draws
    /// so that it drops or takes its own character cell. A configured font
    /// that draws nothing at that size leaves the font and the size as they
    /// were, with one line saying why.
    ///
    /// References:
    /// - ADR-1201 D11: what the end of a session returns, the font and the size.
    /// - ADR-1200 D16: a frit's own character cell follows whether `font-size` is configured.
    /// - ADR-1205 D4, its note of 2026-11-19: the batch is read again at each new login.
    pub(super) fn return_font(&mut self, h: usize, id: u64) {
        // the batch changed since the head's font was resolved, so the
        // font is resolved again, as a newly active first source can set
        // its cell
        let rebatched = self.reread_batch(Some(h));
        let Some(due) = self.heads.get(h).map(|head| head.returns(id)) else {
            return;
        };
        if !due.size && !rebatched {
            return;
        }
        self.reread_font_keys();
        let config = self.head_config(h);
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let mut kept = head.keep(id);
        let was_sized = head.sized();
        head.reconfigure(&config);
        let shown = head.console.id == id;
        // The frit made on the fly for the configured font, the head's
        // once the return holds.
        let mut made = None;
        let new = if due.font {
            head.switched = None;
            let (chosen, configured) = self.configured_font(h, None);
            made = Some(configured);
            Some(chosen)
        } else if head.sized() != was_sized || rebatched {
            // The frit drawn, with its own cell or without it as the
            // `font-size` read again now says, and with its own cell asked
            // for again under a changed batch.
            //
            // References:
            // - ADR-1200 D16: a frit's own character cell follows whether `font-size` is configured.
            match &head.font {
                Some(Chosen::Frit { frit, .. }) => {
                    Chosen::frit(frit.clone(), head.sized(), head.look_cell()).ok()
                }
                _ => None,
            }
        } else {
            None
        };
        // The frit the head drew, where the font changed.
        let mut left = None;
        if let Some(new) = new {
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            if !same_font(head.chosen(), &new) || head.sized() != was_sized || rebatched {
                left = Some(head.frit().map(str::to_owned));
                kept.font = Some(head.font.replace(new));
                head.refont = true;
                self.share(h);
                self.retry_late_cell(h);
            }
        }
        if let Some(head) = self.heads.get_mut(h) {
            let fresh = head.fresh_fit();
            head.configured_size(id, fresh);
        }
        if (shown || left.is_some()) && !self.relayout(h) {
            let Some(head) = self.heads.get_mut(h) else {
                return;
            };
            let tried = head.font_size;
            head.put_back(kept);
            self.relayout(h);
            self.prune();
            if let Some(head) = self.heads.get(h) {
                log::warn!(
                    "head {}: the configured font draws nothing at {tried} pixels: the font and the size stay {} at {}",
                    head.name(),
                    head.chosen().name(head.font_size, &self.locale),
                    head.font_size
                );
            }
            return;
        }
        if let Some(made) = made
            && let Some(head) = self.heads.get_mut(h)
        {
            head.made = made;
        }
        let hidden = self
            .heads
            .get(h)
            .is_some_and(|head| head.hidden.iter().any(|c| c.id == id));
        if hidden
            && let Some(hidden) = self.configured_fit(h)
            && let Some(head) = self.heads.get_mut(h)
        {
            head.configured_size(id, hidden);
        }
        if let Some(was) = left {
            self.prune();
            // The frit the head left, where another head still draws it,
            // keeps the cells of the heads drawing it alone.
            if let Some(other) = was.and_then(|was| {
                self.heads
                    .iter()
                    .position(|head| head.frit() == Some(was.as_str()))
            }) {
                self.ask_glyphs(other);
            }
            if let Some(head) = self.heads.get(h)
                && (due.font || head.sized() != was_sized)
            {
                log::info!(
                    "head {}: the session in console {id} ended: the font returns to {}",
                    head.name(),
                    head.chosen().name(head.font_size, &self.locale)
                );
            }
        }
    }

    /// Reads the configuration again with the reader vtium started with,
    /// for its font keys alone (ADR-1201 D11): each report naming a font
    /// key, a file that could not be read whole, or a file the ownership
    /// check refuses, with why and the command that mends it (ADR-1403
    /// D3), is logged as the start logs it, and the key is dropped as it
    /// is at the start. Every other key, its reports and
    /// the count of reports past the most kept wait for the next start.
    /// With no reader, the configuration vtium started with stands.
    fn reread_font_keys(&mut self) {
        const FONT_KEYS: [&crate::config::Key; 4] = [
            &keys::FONT_NAME,
            &keys::FONT_SIZE,
            &keys::FONT_ENGINE,
            &keys::FONT_LINE_SPACING,
        ];
        let Some(read) = self.reread.as_mut() else {
            return;
        };
        let loaded = read();
        use crate::config::Problem;
        let font_key = |k: &str| FONT_KEYS.iter().any(|f| f.name == k);
        for r in &loaded.reports {
            let file = matches!(
                r.problem,
                Problem::Unreadable(_)
                    | Problem::NotAFile
                    | Problem::FileTooLong
                    | Problem::Untrusted(_)
            );
            if file || r.problem.key().is_some_and(font_key) {
                crate::warn::warn(r);
            }
        }
        self.config.take_keys(&loaded.config, &FONT_KEYS);
    }

    /// Head `h`'s frit asks the helper for its cell again where the last
    /// ask had no answer in time, even while another head draws the frit; a
    /// refusal holds (ADR-1200 D17).
    fn retry_late_cell(&mut self, h: usize) {
        if let Some(frit) = self
            .heads
            .get(h)
            .and_then(Head::frit)
            .and_then(|name| self.frits.get_mut(name))
        {
            frit.own
                .retain(|_, answer| !answer.as_ref().is_err_and(|no| no.late));
        }
    }

    /// The fit a hidden console of head `h` takes at the configured size:
    /// the grid and cell the head's displays give its font there, as
    /// [`App::relayout`] lays the shown console out. `None` while the head
    /// has no display, when the console keeps the shown console's grid and
    /// cell until it is shown, as a console the open chord opens does.
    fn configured_fit(&mut self, h: usize) -> Option<crate::head::console::Fit> {
        let dpi = self.dpi(h);
        let head = self.heads.get(h)?;
        if head.views.is_empty() {
            return None;
        }
        let fresh = head.fresh_fit();
        let height = height_at(head, (fresh.points, fresh.auto_size), dpi, &self.locale);
        self.own_cell(h, height);
        let head = self.heads.get(h)?;
        let locale = self.locale.as_str();
        let (face, base) = font::select(height, head.has_font, |x| head.face(x, locale)).ok()?;
        let cell = |at: u32| {
            head.face(at, locale).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let sizes: Vec<(u32, u32)> = head.views.values().map(|v| v.size).collect();
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        let l = layout::layout(&sizes, turned, fresh.asked, base, face.size_step(), cell)?;
        Some(crate::head::console::Fit {
            grid: l.grid,
            font_size: base,
            cell: cell(base)?,
            ..fresh
        })
    }
}

/// The height `head` draws at with its display's density `dpi`: the
/// configured `font-size` in points, else the automatic rule, else the
/// height it has, as a bitmap frit's whole multiples snap it.
pub(super) fn draw_height(head: &Head, dpi: f64, locale: &str) -> u32 {
    height_at(head, (head.points, head.auto_size), dpi, locale)
}

/// The height `head` draws at with its display's density `dpi` at the
/// size `(points, auto_size)`, a console's [`crate::head::console::Fit`]:
/// as [`draw_height`] has it.
fn height_at(head: &Head, size: (Option<u32>, bool), dpi: f64, locale: &str) -> u32 {
    let (points, auto_size) = size;
    let height = if let Some(points) = points {
        font::points_to_pixels(points, dpi)
    } else if auto_size {
        let sizes = head.views.values().map(|v| v.size);
        let w = sizes.clone().map(|s| s.0).min().unwrap_or(0);
        let ht = sizes.map(|s| s.1).min().unwrap_or(0);
        head.chosen().auto_height(w, ht, locale, dpi)
    } else {
        head.font_size
    };
    head.chosen().snap(height)
}

/// Whether `a` and `b` are the same font: the same built-in by name, or
/// the same frit, by its name and its text.
fn same_font(a: &Chosen, b: &Chosen) -> bool {
    match (a, b) {
        (Chosen::Builtin { name: x, cell: a }, Chosen::Builtin { name: y, cell: b }) => {
            x == y && a == b
        }
        (Chosen::Frit { frit: x, .. }, Chosen::Frit { frit: y, .. }) => {
            x.name == y.name && x.text == y.text
        }
        _ => false,
    }
}
