//! How a display joins a head and leaves it: a video device's displays come
//! and go, each walks the rules to the head it joins, and the head lays its
//! displays out again and gives each one its colors.

use std::path::{Path, PathBuf};

use vtium_platform::display::{DPI_ASSUMED, DisplayId, Dpms};
#[cfg(feature = "filters")]
use vtium_render::Depth;
use vtium_render::{Config as RenderConfig, Orientation};

use crate::config::keys;
use crate::font;
use crate::head::{Head, place_display};
use crate::layout;
use crate::painter::Painter;
use crate::term::View;
use crate::video::Video;

use super::App;
use super::devices::depth;
use super::drawing::display_line;
use super::fonts::draw_height;

impl App {
    /// Adds a video device, woken while the VT is ours and no hand-over
    /// holds its place, and else put to sleep, which lets go of the DRM
    /// master a card's open took; the VT's acquire, or the hand-over's
    /// end, wakes it (vtium/vtium#139).
    pub fn add_video(&mut self, node: PathBuf, mut video: Box<dyn Video>) {
        if let Some(check) = self.vt.release_check() {
            video.set_interrupt(check);
        }
        // A card a hand-over gave up stays asleep until the hand-over ends
        // (ADR-1401 D16).
        if self.awake && !self.hand_overs.given_up(&node) {
            if let Err(e) = video.wake_up() {
                log::warn!("cannot wake {}: {e}", node.display());
            }
        } else {
            video.sleep();
        }
        self.videos.insert(node, video);
    }

    /// Lets go of each card the kernel removed, as drm2d finds at the first
    /// ioctl that answers ENODEV, which closes its file, so a card added
    /// next can take its number. udev's report of the removal may come
    /// later, or early in boot never. Its displays' removals, drained or
    /// not, go with it.
    pub(super) fn let_go_of_gone_cards(&mut self) {
        let gone: Vec<PathBuf> = self
            .videos
            .iter()
            .filter(|(_, v)| v.gone())
            .map(|(node, _)| node.clone())
            .collect();
        for node in gone {
            log::info!(
                "{}: the kernel removed the card, and vtium released the card",
                node.display()
            );
            self.remove_video(&node);
        }
    }

    /// Removes a video device and its displays, or its wait for the seat
    /// manager, and lets the VT close it.
    pub fn remove_video(&mut self, node: &Path) {
        self.waiting_videos.remove(node);
        self.videos.remove(node);
        self.vt.close_device(node);
        self.cards.remove(node);
        self.lit.retain(|(n, _), _| n != node);
        self.unlit.retain(|(n, _), _| n != node);
        self.dark.retain(|(n, _), _| n != node);
        self.guard_lit.take_if(|((n, _), _)| n == node);
        self.guard_due = true;
        for h in 0..self.heads.len() {
            let lost = self
                .heads
                .get_mut(h)
                .is_some_and(|head| head.drop_views(|(n, _)| n == node));
            if lost {
                self.relayout(h);
            }
        }
        self.retaking.remove(node);
        if let Some(r) = &mut self.releasing {
            r.remove(node);
        }
    }

    /// The rules' identity of the display `id` of `node`, its place in
    /// its card's list the place among the displays its device lists, in
    /// the kernel's order of connectors, whatever their object ids.
    fn identity(&self, node: &Path, id: DisplayId) -> Option<vtium_settings::Identity> {
        let video = self.videos.get(node)?;
        let card = self.cards.get(node).cloned().unwrap_or_default();
        let order = video
            .displays()
            .iter()
            .position(|&d| d == id)
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
        Some(match video.identity(id) {
            Some(named) => vtium_devices::display(&named, &card, order),
            None => vtium_devices::unnamed_display(&video.name(id), &card, order),
        })
    }

    /// The head the display `id` of `node` joins, by the first rule that
    /// matches it, with the display as the rules saw it; a display the
    /// rules leave dark is set off and logged ([`place_display`]).
    fn head_for(
        &mut self,
        node: &Path,
        id: DisplayId,
    ) -> Option<(usize, vtium_settings::Identity)> {
        let display = self.identity(node, id)?;
        let key = (node.to_owned(), id);
        if let Some((_, h)) = self.guard_lit.as_ref().filter(|(k, _)| *k == key) {
            return Some((*h, display));
        }
        let notice = display
            .connector
            .clone()
            .and_then(|c| self.notices.get(&(node.to_owned(), c)))
            .filter(|n| !n.parked);
        if let Some(n) = notice {
            return Some((n.head, display));
        }
        match place_display(&self.rules, &self.vt_heads, &display) {
            Ok(h) => Some((h, display)),
            Err(line) => {
                log::info!("{line}");
                self.dark.insert(key, display);
                self.guard_due = true;
                self.darken();
                None
            }
        }
    }

    /// Sets off each display a device announced that the rules leave
    /// dark, after a wake-up or an unblank lit it: a DRM card never
    /// announces one ([`open_video`]), a framebuffer does.
    pub(super) fn darken(&mut self) {
        for (node, id) in self.dark.keys() {
            if self.hand_overs.given_up(node) {
                continue;
            }
            if let Some(video) = self.videos.get_mut(node)
                && let Err(e) = video.set_dpms(*id, Dpms::Off)
            {
                log::warn!("cannot set display {} off: {e}", video.name(*id));
            }
        }
    }

    pub(super) fn add_display(&mut self, node: &Path, id: DisplayId) {
        let Some(video) = self.videos.get(node) else {
            return;
        };
        let Some(size) = video.size(id) else {
            return;
        };
        let refresh = video.refresh_mhz(id);
        let (backend, name) = (video.backend(), video.name(id));
        let levels = video.levels(id);
        let measured = video.dpi(id).is_some();
        let Some((h, display)) = self.head_for(node, id) else {
            return;
        };
        if !measured
            && self.head_config(h).number(&keys::DISPLAY_DPI).is_none()
            && self.assumed.insert((node.to_owned(), id))
        {
            crate::warn::warn(format_args!(
                "display {name} reports no physical size: vtium assumes {DPI_ASSUMED} DPI, which display-dpi can state"
            ));
        }
        self.choose_font_adding(h, Some(size));
        // the display draws the pointer theme of its head's look
        let pointers = self.pointer_store(h);
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let Ok(face) = head.face(head.font_size.max(1), &self.locale) else {
            log::error!("cannot load the font for display {name}");
            return;
        };
        let font = head.font_at(face, &self.config);
        let mut view = View::new(
            id,
            font,
            size,
            refresh,
            RenderConfig {
                depth: depth(&self.head_config(h), levels),
                ..head.render_config(&self.config)
            },
            self.measure(),
            pointers,
        );
        if self.config.switch(&keys::SOFT_CURSOR) {
            view.pointer = crate::pointer::Pointer::software();
        }
        let key = (node.to_owned(), id);
        if let Some(head) = self.heads.get_mut(h) {
            head.views.insert(key.clone(), view);
        }
        #[cfg(feature = "filters")]
        self.give_colors(h, &key);
        if !self.relayout(h) {
            if let Some(head) = self.heads.get_mut(h) {
                head.drop_views(|k| *k == key);
            }
            return;
        }
        // Lit once it draws, so the focus never starts on a head whose
        // display could not be set up.
        if !self.focus_settled {
            self.start_lit.insert(key.clone(), display.clone());
        }
        self.lit.insert(key.clone(), display);
        if let Some(head) = self.heads.get_mut(h) {
            head.black_and_white_joins(&key, &name);
        }
        self.focus_due = true;
        self.guard_due = true;
        // the line names the path that draws the display (ADR-1303 D10):
        // a render thread that has not told the loop its path yet logs it
        // with the head's other display lines once it has
        self.painter(h);
        if let Some(head) = self.heads.get(h)
            && let Some(v) = head.views.get(&key)
            && head.painter.as_ref().is_none_or(Painter::known)
        {
            let path = head.painter.as_ref().map(Painter::path).unwrap_or_default();
            display_line(&name, backend, path, v);
        }
    }

    /// Sets each of head `h`'s displays' font and its grid from the base
    /// height, as [`layout::layout`] decides, and its terminal and its pty
    /// at the grid. Returns false, changing nothing, when the base font
    /// cannot be found.
    pub(super) fn relayout(&mut self, h: usize) -> bool {
        if self.heads.get(h).is_none_or(|head| head.views.is_empty()) {
            // nothing to lay out, but a switch still changed the font
            let locale = self.locale.as_str();
            if let Some(head) = self.heads.get_mut(h) {
                let name = head.chosen().name(head.font_size, locale).to_owned();
                head.name_font(&name);
            }
            return true;
        }
        self.choose_font(h);
        let dpi = self.dpi(h);
        if let Some(height) = self
            .heads
            .get(h)
            .map(|head| draw_height(head, dpi, &self.locale))
        {
            self.own_cell(h, height);
        }
        // A console that the head shows while it waits for the cell of the
        // font set an OSC 50 switch chose would otherwise report the new
        // name with the cell the head keeps. So that console holds its
        // replies too, until the wait ends and `end_cell_waits` sends
        // them from the new cell. The console that made the switch holds
        // its replies already, and a second hold changes nothing.
        if self
            .heads
            .get(h)
            .is_some_and(|head| head.switched.is_some())
            && self.waits_for_cell(h)
            && let Some(head) = self.heads.get_mut(h)
        {
            head.console.term.terminal.hold_replies();
        }
        let App {
            heads,
            config,
            locale,
            ..
        } = self;
        let Some(head) = heads.get_mut(h) else {
            return true;
        };
        let locale = locale.as_str();
        let sizes: Vec<(u32, u32)> = head.views.values().map(|v| v.size).collect();
        head.font_size = draw_height(head, dpi, locale);
        let selected = font::select(head.font_size, head.has_font, |x| head.face(x, locale));
        let base = match selected {
            Ok((face, base)) => {
                if !head.has_font {
                    for (range, n) in vtium_font::Coverage::new(|r| face.coverage(r)).partial() {
                        crate::warn::warn(format_args!(
                            "the font covers {} of {} characters of {range:?}; that range draws the sprites unless glyph-source says otherwise",
                            n.covered, n.total
                        ));
                    }
                }
                head.has_font = true;
                head.step = face.size_step();
                base
            }
            Err(e) => {
                log::error!("cannot load the font: {e}");
                return false;
            }
        };
        head.font_size = base;
        let name = head.chosen().name(base, locale).to_owned();
        head.name_font(&name);
        let cell = |head: &Head, at: u32| {
            head.face(at, locale).ok().map(|f| {
                let c = f.cell();
                (c.width, c.height)
            })
        };
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        let Some(l) = layout::layout(&sizes, turned, head.asked, base, head.step, |at| {
            cell(head, at)
        }) else {
            return true;
        };
        head.cell = cell(head, base).unwrap_or(head.cell);
        let render = head.render_config(config);
        let keys: Vec<(PathBuf, DisplayId)> = head.views.keys().cloned().collect();
        let refont = std::mem::take(&mut head.refont);
        for (key, height) in keys.into_iter().zip(l.heights) {
            let orientation = head.orientation;
            let stale = head
                .views
                .get(&key)
                .is_some_and(|v| refont || v.height != height || v.orientation != orientation);
            if !stale {
                continue;
            }
            let Ok(face) = head.face(height, locale) else {
                continue;
            };
            let font = head.font_at(face, config);
            if let Some(v) = head.views.get_mut(&key) {
                v.rebuild(font, height, render);
            }
        }
        head.resize(l.grid);
        self.bound_pointers();
        self.ask_glyphs(h);
        true
    }

    /// Each pointer moves within the grid of the head it delivers to, and
    /// a pointer added later within the focus's until it is placed.
    pub(super) fn bound_pointers(&mut self) {
        let Some(input) = &mut self.input else {
            return;
        };
        for (h, head) in self.heads.iter().enumerate() {
            let (w, ht) = head.pixels(head.grid);
            if self.focus == Some(h) {
                input.set_pointer_max(w, ht);
            }
            input.set_pointer_max_of(
                &self.routes.devices_of(&self.vt_heads, h, self.focus),
                w,
                ht,
            );
        }
    }

    /// Checks display `key` of head `h` against the requirements of the
    /// head's vtlook, and hands the display's view the vtlook's color table
    /// when the display meets them, else the built-in vtlook. vtium runs
    /// the check when the display joins the head, at each modeset that may
    /// change the display's size, refresh or depth, and each time the head
    /// moves between the GPU path and the CPU path. A display that stops
    /// meeting the requirements draws the built-in vtlook, with the line
    /// that names the requirement it misses, and a display that meets them
    /// again draws the vtlook again, with no reread of the vtlook. vtium
    /// logs each new answer once, so a check that gives the same answer
    /// logs nothing.
    ///
    /// The display's depth is its drawing's: 1 bit, gray, or for a color
    /// display the bits a pixel of its buffer holds, so a 16-bit
    /// framebuffer counts as 16. Its density comes from `display-dpi`,
    /// else from the size the display reports, else vtium assumes 96.
    ///
    /// References:
    /// - ADR-1304 D7: when vtium checks a display against the requirements, and how it judges depth, size, density and the path.
    #[cfg(feature = "filters")]
    pub(super) fn give_colors(&mut self, h: usize, key: &(PathBuf, DisplayId)) {
        use vtium_settings::vtlook::Depth as Needs;
        let Some(video) = self.videos.get(&key.0) else {
            return;
        };
        let name = video.name(key.1);
        let (levels, bits, reported) = (
            video.levels(key.1),
            video.buffer_bits(key.1),
            video.dpi(key.1),
        );
        let Some(size) = video
            .size(key.1)
            .or_else(|| Some(self.heads.get(h)?.views.get(key)?.size))
        else {
            return;
        };
        let depth = match depth(&self.head_config(h), levels) {
            Depth::One => Needs::One,
            Depth::Gray(_) => Needs::Gray,
            Depth::Color => match bits {
                Some(1) => Needs::One,
                Some(b) if b <= 16 => Needs::Sixteen,
                _ => Needs::TwentyFour,
            },
        };
        let turned = self
            .heads
            .get(h)
            .is_some_and(|head| matches!(head.orientation, Orientation::Right | Orientation::Left));
        let dpi = self
            .head_config(h)
            .number(&keys::DISPLAY_DPI)
            .map(|d| d as f64)
            .or_else(|| {
                let (across, down) = reported?;
                Some(if turned { across } else { down })
            })
            .unwrap_or(DPI_ASSUMED);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the clamp holds the rounded density within u32"
        )]
        let dpi = dpi.round().clamp(0.0, f64::from(u32::MAX)) as u32;
        let display = crate::head::look::Display {
            name: &name,
            size,
            depth,
            dpi,
            gpu: self.draws_on_gpu(h),
        };
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        let (table, line) = head.vtlook.table_for(&display);
        let again = head.vtlook.met_again(&display);
        let Some(view) = head.views.get_mut(key) else {
            return;
        };
        view.set_colors(table);
        if view.set_unmet(line.clone()).is_some()
            && let Some(said) = line.or(again)
        {
            log::info!(target: "look", "{said}");
        }
    }

    /// Checks each display of head `h` against the requirements of the
    /// head's vtlook again, as [`App::give_colors`] does, after the head
    /// moved between the GPU path and the CPU path.
    #[cfg(feature = "filters")]
    pub(super) fn give_colors_again(&mut self, h: usize) {
        let keys: Vec<_> = self
            .heads
            .get(h)
            .map(|head| head.views.keys().cloned().collect())
            .unwrap_or_default();
        for key in &keys {
            self.give_colors(h, key);
        }
    }

    /// Whether head `h` draws on the GPU path, as a vtlook's `gpu`
    /// requirement judges it: the path its render thread reported, once it
    /// has, and the path the head is set up for until then, so that a
    /// display that joins before the render thread starts is judged by
    /// the path it is about to draw on. A head whose render thread could
    /// not start draws on the CPU path.
    #[cfg(feature = "filters")]
    fn draws_on_gpu(&self, h: usize) -> bool {
        let Some(head) = self.heads.get(h) else {
            return false;
        };
        match &head.painter {
            Some(painter) if painter.known() => painter.path() == crate::painter::Path::Gpu,
            _ if head.painter_refused => false,
            _ => matches!(self.gpu_source(h), Ok(Some(_))),
        }
    }

    /// The density head `h`'s points convert at (the operator's decision
    /// on vtium/vtium#145): `display-dpi`; else the lowest of the head's
    /// displays', each one's measured down its rows as vtium draws it, or
    /// [`DPI_ASSUMED`] where it measures none; [`DPI_ASSUMED`] with no
    /// display.
    pub(super) fn dpi(&self, h: usize) -> f64 {
        if let Some(dpi) = self.head_config(h).number(&keys::DISPLAY_DPI) {
            return dpi as f64;
        }
        let Some(head) = self.heads.get(h) else {
            return DPI_ASSUMED;
        };
        let turned = matches!(head.orientation, Orientation::Right | Orientation::Left);
        head.views
            .keys()
            .map(|(node, id)| {
                let measured = self.videos.get(node).and_then(|v| v.dpi(*id));
                measured.map_or(
                    DPI_ASSUMED,
                    |(across, down)| if turned { across } else { down },
                )
            })
            .reduce(f64::min)
            .unwrap_or(DPI_ASSUMED)
    }

    /// The head holding the display `id` of `node`.
    pub(super) fn head_of(&self, node: &Path, id: DisplayId) -> Option<usize> {
        let key = (node.to_owned(), id);
        self.heads
            .iter()
            .position(|head| head.views.contains_key(&key))
    }
}
