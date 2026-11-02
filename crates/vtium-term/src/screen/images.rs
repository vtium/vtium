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
    use crate::scrollback::{Change, Move};

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

        /// The placements the view shows, in draw order (D9). The store is
        /// given the lines below the view as well, as many as a relative
        /// placement starts above its root, so a child whose root has left
        /// the bottom of a view moved back into the scrollback still draws
        /// (D13).
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
            let below = self.below_view(self.images.reach_below());
            self.images.frame_with_below(rows, &below, self.cols)
        }

        /// The keys of the `n` lines below the view's bottom, top to
        /// bottom: the newer scrollback rows and then the screen's own,
        /// none while the view is the screen.
        fn below_view(&self, n: usize) -> Vec<LineKey> {
            let Some(top) = self.scrollback.view_index() else {
                return Vec::new();
            };
            if n == 0 || self.flags.alternate {
                return Vec::new();
            }
            let len = self.scrollback.len();
            let first = top.saturating_add(self.rows);
            let from_scrollback = (first..len).filter_map(|i| self.scrollback.get(i).map(key));
            let from_screen = self
                .main
                .iter()
                .take(self.rows)
                .skip(first.saturating_sub(len))
                .map(key);
            from_scrollback.chain(from_screen).take(n).collect()
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

        /// What the scrollback did to its rows since, in order: each row
        /// dropped from its oldest end clips its placements by the rows it
        /// stood for, and they go on from the row after it; each anchor a
        /// rewrap of waiting rows carried moves to its new row (ADR-1003,
        /// "Images").
        pub(in crate::screen) fn images_dropped(&mut self) {
            let changes = self.scrollback.take_changes();
            if changes.is_empty() || self.images.is_empty() {
                return;
            }
            for change in changes {
                match change {
                    Change::Dropped { line, rows, next } => {
                        // nothing followed it in the scrollback: the
                        // screen's first line did
                        let next = next.or_else(|| self.main.first().map(Line::id));
                        let to = next.map(|n| LineKey::new(n.get()));
                        let rows = u32::try_from(rows).unwrap_or(u32::MAX);
                        self.images.discard_rows(LineKey::new(line.get()), rows, to);
                        let Some(next) = next else {
                            continue;
                        };
                        let cols: Vec<u32> = self
                            .images
                            .anchors_on(LineKey::new(next.get()))
                            .into_iter()
                            .map(|(_, col)| col)
                            .collect();
                        if !cols.is_empty() {
                            let moves = self.scrollback.note_anchor(next, &cols, &mut self.next_id);
                            self.images_move(&moves);
                        }
                    }
                    Change::Carried(moves) => self.images_move(&moves),
                }
            }
            self.images_settled();
        }

        /// Moves the placements anchored at each move's cell to where the
        /// rewrap carried it, all at once.
        fn images_move(&mut self, moves: &[Move]) {
            let moved: Vec<(PlacementKey, LineKey, u32)> = moves
                .iter()
                .flat_map(|m| {
                    self.images
                        .anchors_on(LineKey::new(m.from.get()))
                        .into_iter()
                        .filter(|&(_, col)| col == m.col)
                        .map(|(p, _)| (p, LineKey::new(m.to.get()), m.to_col))
                })
                .collect();
            for (p, line, col) in moved {
                self.images.move_anchor(p, Some((line, col)));
            }
        }

        /// Before a resize: the anchors of the main buffer's placements on
        /// scrollback rows, for the scrollback to carry on the rows it
        /// leaves to wait.
        pub(in crate::screen) fn images_give_anchors(&mut self) {
            if !self.images.main_has_placements() {
                return;
            }
            let screen: Vec<LineId> = self.main.iter().map(Line::id).collect();
            let anchors = self
                .images
                .main_anchor_lines()
                .into_iter()
                .map(|(line, cols)| (LineId::new(line.get()), cols))
                .filter(|(line, _)| !screen.contains(line))
                .collect();
            self.scrollback.set_anchors(anchors);
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
                // a relative placement's box starts `row` rows below the
                // line its root is anchored on, above it when negative
                let Some(top) = row_of(region.line).map(|y| y.saturating_add(region.row)) else {
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
