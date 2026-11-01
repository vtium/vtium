//! The screen's side of the image store (ADR-1100, D8): every event that
//! writes, erases, scrolls, drops or rewraps lines tells the store, with
//! the lines of the screen it acts on, and marks the cells under each box
//! the store reports changed (ADR-1001).
//!
//! Built without the `images` feature, each hook is an empty function the
//! compiler removes, so the minspec build carries none of it.

#[cfg(feature = "images")]
mod with {
    use vtium_image::{
        FramePlacement, ImageKey, LineKey, NewImage, Place, PlacementKey, Refusal, Rows, Screens,
        Store,
    };

    use super::super::Screen;
    use crate::line::{Line, LineId};
    use crate::scrollback::Change;

    fn key(line: &Line) -> LineKey {
        LineKey::new(line.id().get())
    }

    /// One buffer's lines as the store reads them: the screen's rows, and
    /// the scrollback's newest rows above them, as many as its tallest
    /// placement reaches.
    #[derive(Clone, Debug, Default)]
    pub(crate) struct Keys {
        screen: Vec<LineKey>,
        above: Vec<LineKey>,
    }

    impl Keys {
        pub(crate) fn rows(&self) -> Rows<'_> {
            Rows {
                screen: &self.screen,
                above: &self.above,
            }
        }
    }

    impl Screen {
        /// The image store.
        pub fn images(&self) -> &Store {
            &self.images
        }

        /// Sets the bytes the screen's images may hold, main and alternate
        /// together (`image-storage-limit`).
        pub fn set_image_limit(&mut self, bytes: u64) {
            self.images.set_limit(bytes);
        }

        /// Sets the cell size in pixels that images are placed in.
        pub fn set_image_cell(&mut self, width: u32, height: u32) -> Result<(), Refusal> {
            self.images.set_cell(width, height)
        }

        /// Runs `f` over the store with both buffers' lines, then settles
        /// and marks what it changed, as a screen event does.
        pub(crate) fn with_images<R>(
            &mut self,
            f: impl FnOnce(&mut Store, Screens<'_>, usize) -> R,
        ) -> R {
            let (main, alt) = (self.keys(false), self.keys(true));
            let screens = Screens {
                main: main.rows(),
                alt: alt.rows(),
            };
            let out = f(&mut self.images, screens, self.cols);
            self.images_settled();
            out
        }

        /// Stores an image in the buffer in use.
        pub fn add_image(&mut self, image: NewImage) -> Result<ImageKey, Refusal> {
            self.with_images(|store, screens, _| store.add_image(image, None, screens))
        }

        /// Places an image of the buffer in use on its screen.
        pub fn place_image(
            &mut self,
            image: ImageKey,
            place: Place,
        ) -> Result<PlacementKey, Refusal> {
            self.with_images(|store, screens, cols| store.place(image, place, screens, cols))
        }

        /// The placements the view shows, in draw order (D9).
        pub fn placements(&self) -> Vec<FramePlacement> {
            if !self.images.has_placements() {
                return Vec::new();
            }
            let view = self.view();
            let screen: Vec<LineKey> = view.iter().map(|l| key(l)).collect();
            let reach = self.images.reach();
            let above: Vec<LineKey> = if self.flags.alternate {
                Vec::new()
            } else {
                let top = self
                    .scrollback
                    .view_index()
                    .unwrap_or(self.scrollback.len());
                (0..top)
                    .rev()
                    .take(reach)
                    .filter_map(|i| self.scrollback.get(i).map(key))
                    .collect()
            };
            let rows = Rows {
                screen: &screen,
                above: &above,
            };
            self.images.frame(rows, self.cols)
        }

        /// The keys of the main (`alt` false) or the alternate buffer's
        /// screen rows, and above the main screen the scrollback's newest
        /// rows, as many as the main buffer's tallest placement reaches.
        pub(crate) fn keys(&self, alt: bool) -> Keys {
            let lines = if alt { &self.alt } else { &self.main };
            let screen = lines.iter().take(self.rows).map(key).collect();
            let above = if alt {
                Vec::new()
            } else {
                let reach = self.images.main_reach();
                let len = self.scrollback.len();
                (0..len)
                    .rev()
                    .take(reach)
                    .filter_map(|i| self.scrollback.get(i).map(key))
                    .collect()
            };
            Keys { screen, above }
        }

        /// The keys of the screen in use.
        fn active_keys(&self) -> Keys {
            self.keys(self.flags.alternate)
        }

        /// Cells `x0` to `x1` of row `y` were written or erased.
        pub(in crate::screen) fn images_cut(&mut self, y: usize, x0: usize, x1: usize) {
            if !self.images.has_cuttable() {
                return;
            }
            let keys = self.active_keys();
            self.images.cut(y, x0, x1, keys.rows(), self.cols);
            self.images_settled();
        }

        /// Rows `top` to `bottom` are about to scroll `n` rows; returns
        /// whether the store took the scroll, which `images_scrolled` then
        /// ends once the lines have moved.
        pub(in crate::screen) fn images_scroll(
            &mut self,
            band: (usize, usize),
            n: usize,
            up: bool,
            into_scrollback: bool,
        ) -> bool {
            if !self.images.has_placements() {
                return false;
            }
            let keys = self.active_keys();
            self.images
                .scroll(band, n, up, into_scrollback, keys.rows());
            true
        }

        pub(in crate::screen) fn images_scrolled(&mut self, took: bool) {
            if took {
                let keys = self.active_keys();
                self.images.scroll_done(keys.rows());
            }
            self.images_dropped();
            self.images_settled();
        }

        /// Lines left the screen or the scrollback with nowhere to go.
        pub(in crate::screen) fn images_free(&mut self, lines: &[Line]) {
            if self.images.is_empty() {
                return;
            }
            for line in lines {
                self.images.free_line(key(line));
            }
            self.images_settled();
        }

        /// What the scrollback did to its rows since: each row dropped from
        /// its oldest end clips its placements, which go on from the row
        /// after it, and each anchor a rewrap of waiting rows carried moves
        /// to its new row (ADR-1003, "Images").
        pub(in crate::screen) fn images_dropped(&mut self) {
            let changes = self.scrollback.take_changes();
            if changes.is_empty() || self.images.is_empty() {
                return;
            }
            let oldest = self
                .scrollback
                .oldest_id()
                .or_else(|| self.main.first().map(Line::id));
            for (i, change) in changes.iter().enumerate() {
                match *change {
                    Change::Dropped(line) => {
                        let next = match changes.get(i.saturating_add(1)) {
                            Some(&Change::Dropped(next)) => Some(next),
                            _ => oldest,
                        };
                        let to = next.map(|n| LineKey::new(n.get()));
                        self.images.discard_line(LineKey::new(line.get()), to);
                        // the piece now on a waiting row is carried when
                        // that row rewraps
                        if let Some(next) = next {
                            for (_, col) in self.images.anchors_on(LineKey::new(next.get())) {
                                self.scrollback.note_anchor(next, col);
                            }
                        }
                    }
                    Change::Moved { from, col, to, to_col } => {
                        for (p, at) in self.images.anchors_on(LineKey::new(from.get())) {
                            if at == col {
                                self.images.move_anchor(p, Some((LineKey::new(to.get()), to_col)));
                            }
                        }
                    }
                }
            }
            self.images_settled();
        }

        /// After a resize: the anchors on rows left waiting are recorded,
        /// for the rewrap that later reaches them to carry.
        pub(in crate::screen) fn images_register_waiting(&mut self) {
            if !self.images.main_has_placements() {
                self.scrollback.set_waiting_anchors(Vec::new());
                return;
            }
            let anchors = self
                .images
                .main_anchor_lines()
                .into_iter()
                .map(|(line, cols)| (LineId::new(line.get()), cols))
                .collect();
            self.scrollback.set_waiting_anchors(anchors);
        }

        /// ED 2's own rule, after its erase.
        pub(in crate::screen) fn images_erase_screen(&mut self) {
            if !self.images.has_placements() {
                return;
            }
            let keys = self.active_keys();
            self.images.erase_screen(keys.rows());
            self.images_settled();
        }

        /// ED 3: the placements on every scrollback row go.
        pub(in crate::screen) fn images_clear_scrollback(&mut self) {
            if self.images.is_empty() {
                return;
            }
            let ids: Vec<LineId> = self.scrollback.all_rows().map(Line::id).collect();
            for id in ids {
                self.images.free_line(LineKey::new(id.get()));
            }
            self.images_settled();
        }

        pub(in crate::screen) fn images_alternate(&mut self, on: bool) {
            self.images.set_alternate(on);
            self.images_settled();
        }

        /// RIS: both buffers are emptied.
        pub fn reset_images(&mut self) {
            self.images.reset();
            self.images_settled();
        }

        /// The main buffer's anchors on `line`, for a rewrap to carry.
        pub(in crate::screen) fn images_anchors(&self, line: LineId) -> Vec<(PlacementKey, u32)> {
            self.images.anchors_on(LineKey::new(line.get()))
        }

        pub(in crate::screen) fn images_carry(
            &mut self,
            moved: &[(PlacementKey, Option<(LineId, u32)>)],
        ) {
            for &(p, to) in moved {
                let to = to.map(|(line, col)| (LineKey::new(line.get()), col));
                self.images.move_anchor(p, to);
            }
            self.images_settled();
        }

        /// Adds a handler's work to the input call's.
        pub(crate) fn add_image_work(&mut self, work: u64) {
            self.images.add_work(work);
        }

        /// Starts an input call's count of image work afresh.
        pub(crate) fn take_image_work(&mut self) -> u64 {
            self.images.take_work()
        }

        /// The rows above the main screen a rewrap takes beside the screen's
        /// own, so placements reaching down onto it are carried.
        pub(in crate::screen) fn images_reach(&self) -> usize {
            self.images.main_reach()
        }

        /// Ends a screen event: what the pieces it cut owe is settled, and
        /// the cells under every box the store changed are marked.
        fn images_settled(&mut self) {
            if self.images.owes() {
                let (main, alt) = (self.keys(false), self.keys(true));
                let screens = Screens {
                    main: main.rows(),
                    alt: alt.rows(),
                };
                self.images.settle_event(screens);
            }
            let changed = self.images.take_changed();
            if changed.is_empty() {
                return;
            }
            if self.scrollback.offset() != 0 {
                self.damage.mark_all();
                return;
            }
            let keys = self.active_keys();
            let rows = keys.rows();
            let row_of = |line: LineKey| -> Option<i64> {
                if let Some(y) = rows.screen.iter().position(|&k| k == line) {
                    return i64::try_from(y).ok();
                }
                let k = rows.above.iter().position(|&k| k == line)?;
                i64::try_from(k).ok()?.checked_add(1)?.checked_neg()
            };
            for region in changed {
                let Some(top) = row_of(region.line) else {
                    continue;
                };
                let first = usize::try_from(top.max(0)).unwrap_or(0);
                let end = top.saturating_add(i64::from(region.rows));
                let end = usize::try_from(end.max(0)).unwrap_or(0).min(self.rows);
                let x0 = usize::try_from(region.col)
                    .unwrap_or(usize::MAX)
                    .min(self.cols);
                let x1 = x0
                    .saturating_add(usize::try_from(region.cols).unwrap_or(usize::MAX))
                    .min(self.cols);
                for y in first..end {
                    self.damage.mark_cells(y, x0..x1);
                }
            }
        }
    }
}

#[cfg(not(feature = "images"))]
mod without {
    use super::super::Screen;
    use crate::line::Line;

    impl Screen {
        #[inline(always)]
        pub(in crate::screen) fn images_cut(&mut self, _: usize, _: usize, _: usize) {}

        #[inline(always)]
        pub(in crate::screen) fn images_scroll(
            &mut self,
            _: (usize, usize),
            _: usize,
            _: bool,
            _: bool,
        ) -> bool {
            false
        }

        #[inline(always)]
        pub(in crate::screen) fn images_scrolled(&mut self, _: bool) {}

        #[inline(always)]
        pub(in crate::screen) fn images_free(&mut self, _: &[Line]) {}

        #[inline(always)]
        pub(in crate::screen) fn images_dropped(&mut self) {}

        #[inline(always)]
        pub(in crate::screen) fn images_erase_screen(&mut self) {}

        #[inline(always)]
        pub(in crate::screen) fn images_clear_scrollback(&mut self) {}

        #[inline(always)]
        pub(in crate::screen) fn images_alternate(&mut self, _: bool) {}
    }
}
