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
        FloatingRegion, FramePlacement, ImageKey, LineKey, LinePlaces, NewImage, Place,
        PlaceholderRun, PlacementKey, Refusal, Rows, Screens, Store,
    };

    use std::collections::BTreeSet;

    use super::super::Screen;
    use crate::line::{Line, LineId, LineSize};
    use crate::placeholder;
    use crate::scrollback::{Change, Move};

    /// The placeholder runs ED 2 reads before its erase, for the store to
    /// hold through its rule: `None` when nothing needs them.
    pub(in crate::screen) type HeldRuns = Option<[Vec<PlaceholderRun>; 2]>;

    fn key(line: &Line) -> LineKey {
        LineKey::new(line.id().get())
    }

    /// Mirrors placement `p`, whose box's top row `r` draws right to left:
    /// its box ends at the right edge of the cell its first column is drawn
    /// in. The cells of it left of the screen are cut off, its visible part
    /// with them. Returns false when nothing of it is left to draw.
    ///
    /// On a line of `size` double, the placement's column counts the line's
    /// cells, as `r` does, and the cell drawn in place `v` covers screen
    /// columns `2v` and `2v + 1`. So the box ends at screen column
    /// `2v + 2`, keeping its own width.
    ///
    /// References:
    /// - ADR-1004 D8, "Pictures": the box of a picture in a right-to-left
    ///   paragraph starts at the right edge of its first logical column's
    ///   cell and extends left, and on a double-size row a picture starts
    ///   on the screen columns where the cell it was placed in is drawn.
    #[cfg(feature = "bidi")]
    fn mirror(p: &mut FramePlacement, r: &crate::order::RowOrder, size: LineSize) -> bool {
        let left = mirrored_left(p.col, p.cols, r, size);
        let cut = u32::try_from(left.min(0).unsigned_abs()).unwrap_or(u32::MAX);
        if cut >= p.cols {
            return false;
        }
        p.col = u32::try_from(left.max(0)).unwrap_or(0);
        p.cols = p.cols.saturating_sub(cut);
        let shift = cut.saturating_mul(p.cell_width);
        p.offset_x = p.offset_x.saturating_sub(i64::from(shift));
        if p.x >= shift {
            p.x = p.x.saturating_sub(shift);
            return true;
        }
        let trim = shift.saturating_sub(p.x);
        if trim >= p.width {
            return false;
        }
        // the source pixels the trimmed visible pixels showed, rounded so
        // the source still covers every visible pixel
        let src_trim = u64::from(trim)
            .saturating_mul(u64::from(p.src_width))
            .checked_div(u64::from(p.width))
            .and_then(|t| u32::try_from(t).ok())
            .unwrap_or(0)
            .min(p.src_width.saturating_sub(1));
        p.x = 0;
        p.width = p.width.saturating_sub(trim);
        p.src_x = p.src_x.saturating_add(src_trim);
        p.src_width = p.src_width.saturating_sub(src_trim);
        true
    }

    /// The screen column where the box of a picture `cols` cells wide,
    /// placed in column `col` of a line of `size` whose row `r` draws right
    /// to left, starts: the box ends at the right edge of the screen columns
    /// where that cell is drawn, two for each cell on a double-size line,
    /// and extends left, past the screen's left edge when it is wider than
    /// the room there.
    #[cfg(feature = "bidi")]
    fn mirrored_left(col: u32, cols: u32, r: &crate::order::RowOrder, size: LineSize) -> i64 {
        let first = usize::try_from(col).unwrap_or(usize::MAX);
        let end = r.visual_at(first).saturating_add(1);
        let end = if size.is_double() {
            end.saturating_mul(2)
        } else {
            end
        };
        let end = i64::try_from(end).unwrap_or(i64::MAX);
        end.saturating_sub(i64::from(cols))
    }

    /// Where the lines of a frame draw their cells, for the store to place
    /// each relative placement from where its root's box is drawn: on a
    /// double-size line each cell covers two screen columns, and in a
    /// right-to-left paragraph a picture's box is mirrored. `order` is the
    /// view's drawing order, `None` for a frame drawn in written order.
    ///
    /// References:
    /// - ADR-1004 D8, "Double-size rows": a child starts from its parent's
    ///   drawn top left, offset by H screen columns and V rows.
    struct Drawn<'a> {
        screen: &'a Screen,
        view: &'a [&'a Line],
        top: usize,
        #[cfg(feature = "bidi")]
        order: Option<&'a crate::order::Order>,
    }

    impl LinePlaces for Drawn<'_> {
        fn scale(&self, row: i64) -> i64 {
            if self.screen.size_at(self.view, self.top, row).is_double() {
                2
            } else {
                1
            }
        }

        fn left(&self, row: i64, col: u32, cols: u32) -> Option<i64> {
            let size = self.screen.size_at(self.view, self.top, row);
            if size.is_double() && !shown(col, size, self.screen.cols) {
                return None;
            }
            #[cfg(feature = "bidi")]
            if let Some(order) = self.order
                && let Some(r) = self.screen.order_at(order, self.view.len(), self.top, row)
                && r.rtl
            {
                return Some(mirrored_left(col, cols, &r, size));
            }
            let _ = cols;
            Some(i64::from(col).saturating_mul(self.scale(row)))
        }
    }

    /// Moves picture `p`, whose box's top row is a line of `size` on a
    /// screen `cols` wide, to the screen column where its first cell is
    /// drawn in written order. A double-size line shows one cell for every
    /// two screen columns, and the placement's column counts the line's
    /// cells, so the cell in column n starts at screen column 2n. Returns
    /// false when the line does not show that cell, which happens when a
    /// line made double after the picture was placed hides it.
    ///
    /// References:
    /// - ADR-1004 D8, "Double-size rows": a picture starts on the screen
    ///   columns where the cell it was placed in is drawn.
    fn to_screen(p: &mut FramePlacement, size: LineSize, cols: usize) -> bool {
        if !size.is_double() {
            return true;
        }
        if !shown(p.col, size, cols) {
            return false;
        }
        p.col = p.col.saturating_mul(2);
        true
    }

    /// Whether a line of `size` on a screen `cols` wide shows its cell in
    /// column `col`.
    fn shown(col: u32, size: LineSize, cols: usize) -> bool {
        usize::try_from(col).is_ok_and(|c| c < size.columns(cols))
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
        /// and marks what it changed, as a screen event does. While a buffer
        /// holds a child of a virtual placement, `f` runs with the
        /// placeholder runs read from that buffer's screen lines held, so a
        /// kitty delete by cell, row or column or of every placement, and a
        /// reservation that may evict, take the child by the cells it draws
        /// on (ADR-1100, D13).
        pub(crate) fn with_images<R>(
            &mut self,
            f: impl FnOnce(&mut Store, Screens<'_>, usize) -> R,
        ) -> R {
            let held = self.decision_runs([true, true]);
            let (main, alt) = (self.keys(false), self.keys(true));
            let screens = Screens {
                main: main.rows(),
                alt: alt.rows(),
            };
            let cols = self.cols;
            let out = match &held {
                Some([m, a]) => self
                    .images
                    .with_runs([m, a], screens, cols, |store| f(store, screens, cols)),
                None => f(&mut self.images, screens, cols),
            };
            self.images_settled();
            out
        }

        /// The placeholder runs of the screen lines of each buffer `which`
        /// names, main then alternate, that holds a child of a virtual
        /// placement, for the store to hold through a decision; `None`, with
        /// nothing read, while no such buffer holds one.
        fn decision_runs(&mut self, which: [bool; 2]) -> Option<[Vec<PlaceholderRun>; 2]> {
            let need = [
                which[0] && self.images.has_virtual_children(false),
                which[1] && self.images.has_virtual_children(true),
            ];
            if !need[0] && !need[1] {
                return None;
            }
            let mut read = |alt: bool, need: bool| {
                if need {
                    self.screen_placeholder_runs(alt)
                } else {
                    Vec::new()
                }
            };
            let main = read(false, need[0]);
            let alt = read(true, need[1]);
            Some([main, alt])
        }

        /// Stores an image in the buffer in use.
        pub fn add_image(&mut self, image: NewImage) -> Result<ImageKey, Refusal> {
            self.with_images(|store, screens, _| store.add_image(image, None, screens))
        }

        /// Stores an image in the buffer in use whose pixels another store
        /// already holds, sharing them.
        ///
        /// References:
        /// - ADR-1100 D17, "Its budget": the consoles of a head share one
        ///   copy of the login picture's pixels.
        pub fn add_shared_image(
            &mut self,
            image: NewImage,
            pixels: vtium_image::Pixels,
        ) -> Result<ImageKey, Refusal> {
            self.with_images(|store, screens, _| store.add_shared_image(image, pixels, screens))
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
        ///
        /// A double-width or double-height line draws each of its cells
        /// two screen columns wide. A picture on such a line starts at the
        /// screen column where the cell it was placed in is drawn, and a
        /// run of placeholder cells covers the screen columns where its
        /// cells are drawn, twice as many as it has cells. The store
        /// places a relative placement from where its root is drawn,
        /// offset in screen columns.
        ///
        /// References:
        /// - ADR-1004 D8, "Double-size rows": where pictures and
        ///   placeholder runs on a double-size line are drawn.
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
            let runs = self.frame_runs();
            let top = self
                .scrollback
                .view_index()
                .unwrap_or(self.scrollback.len());
            let drawn = Drawn {
                screen: self,
                view: &view,
                top,
                #[cfg(feature = "bidi")]
                order: None,
            };
            let mut out = self
                .images
                .frame_drawn(rows, &below, &runs, self.cols, &drawn);
            out.retain_mut(|p| {
                // the store places an entry made from a placeholder run, and
                // a relative placement from where its root is drawn
                p.from_placeholder_run()
                    || p.relative
                    || to_screen(p, self.size_at(&view, top, p.row), self.cols)
            });
            out
        }

        /// The size of the line at view row `row` of `view`, whose top line
        /// is at place `top` in reading order: a row above the view when
        /// negative, and a row below it past the view's last. The alternate
        /// screen has no lines outside its view.
        fn size_at(&self, view: &[&Line], top: usize, row: i64) -> LineSize {
            let line = match usize::try_from(row) {
                Ok(y) => view.get(y).copied().or_else(|| {
                    // a line below the view, which a relative placement's
                    // root can be anchored on
                    (!self.flags.alternate)
                        .then(|| top.checked_add(y).and_then(|place| self.line_at(place)))
                        .flatten()
                }),
                Err(_) if self.flags.alternate => None,
                Err(_) => {
                    let back = usize::try_from(row.unsigned_abs()).unwrap_or(usize::MAX);
                    top.checked_sub(back).and_then(|place| self.line_at(place))
                }
            };
            line.map_or(LineSize::Single, |l| l.flags().size())
        }

        /// The drawing order of the line at view row `row`, as
        /// [`size_at`](Self::size_at) finds the line: the view's own rows
        /// from `order`, and the lines outside the view in reading order
        /// alone. `None` for a line drawn in written order.
        #[cfg(feature = "bidi")]
        fn order_at(
            &self,
            order: &crate::order::Order,
            height: usize,
            top: usize,
            row: i64,
        ) -> Option<crate::order::RowOrder> {
            match usize::try_from(row) {
                Ok(y) if y < height => order.row(y).cloned(),
                _ if self.flags.alternate => None,
                Ok(y) => top.checked_add(y).and_then(|place| self.place_order(place)),
                Err(_) => {
                    let back = usize::try_from(row.unsigned_abs()).unwrap_or(usize::MAX);
                    top.checked_sub(back)
                        .and_then(|place| self.place_order(place))
                }
            }
        }

        /// [`placements`](Self::placements), drawn through `order`
        /// (ADR-1004 D8). A run of placeholder cells is placed where its
        /// cells are drawn: its characters are left to right, so its
        /// cells keep their order inside the run. A picture is never
        /// flipped, and one whose box's top row belongs to a paragraph
        /// that runs right to left is mirrored as a whole: its box ends at
        /// the right edge of the cell its first logical column is drawn
        /// in and extends left, off the left edge when it is wider than
        /// the room there, as ADR-1003 has a picture anchored to the right
        /// overflow.
        #[cfg(feature = "bidi")]
        pub(crate) fn placements_in(&self, order: &crate::order::Order) -> Vec<FramePlacement> {
            if !self.images.has_placements() {
                return Vec::new();
            }
            let view = self.view();
            let rows_of: std::collections::BTreeMap<LineKey, usize> =
                view.iter().enumerate().map(|(y, l)| (key(l), y)).collect();
            let screen: Vec<LineKey> = view.iter().map(|l| key(l)).collect();
            let reach = self.images.reach();
            let top = self
                .scrollback
                .view_index()
                .unwrap_or(self.scrollback.len());
            let above: Vec<LineKey> = if self.flags.alternate {
                Vec::new()
            } else {
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
            let mut runs = self.frame_runs();
            for run in &mut runs {
                let Some(r) = rows_of.get(&run.line).and_then(|&y| order.row(y)) else {
                    continue;
                };
                let first = usize::try_from(run.col).unwrap_or(usize::MAX);
                let last = first.saturating_add(usize::try_from(run.cells).unwrap_or(0));
                if let Some(v) = (first..last).map(|c| r.visual_at(c)).min() {
                    run.col = u32::try_from(v).unwrap_or(run.col);
                }
            }
            let drawn = Drawn {
                screen: self,
                view: &view,
                top,
                order: Some(order),
            };
            let mut out = self
                .images
                .frame_drawn(rows, &below, &runs, self.cols, &drawn);
            out.retain_mut(|p| {
                // an entry made from a placeholder run is placed by the
                // runs above, and the store places a relative placement
                // from where its root is drawn
                if p.from_placeholder_run() || p.relative {
                    return true;
                }
                // the size of the box's top row, and the row's order
                let size = self.size_at(&view, top, p.row);
                if size.is_double() && !shown(p.col, size, self.cols) {
                    return false;
                }
                match self.order_at(order, view.len(), top, p.row) {
                    Some(r) if r.rtl => mirror(p, &r, size),
                    _ => to_screen(p, size, self.cols),
                }
            });
            out
        }

        /// The placements with a row in `lines`, which are the lines at
        /// places `top` onward in reading order, in draw order. Each
        /// placement's row is its box's top row, counted from the view's top
        /// row, `above` lines below `top`, so a row above the view is
        /// negative. `order` is the drawing order of
        /// each of `lines`, top to bottom, in a build with the bidi
        /// feature. The frame's capture of the rows beyond the screen reads
        /// its pictures through this, the way
        /// [`placements`](Self::placements) reads the view's.
        ///
        /// References:
        /// - ADR-1310 D2, "What a row beyond the screen carries": the
        ///   pictures placed on each row.
        #[cfg(feature = "rows-beyond")]
        pub(in crate::screen) fn placements_over(
            &self,
            lines: &[&Line],
            top: usize,
            above: usize,
            #[cfg(feature = "bidi")] order: &crate::order::Order,
        ) -> Vec<FramePlacement> {
            if !self.images.has_placements() || lines.is_empty() {
                return Vec::new();
            }
            let screen: Vec<LineKey> = lines.iter().map(|l| key(l)).collect();
            let keys_above: Vec<LineKey> = if self.flags.alternate {
                Vec::new()
            } else {
                (0..top)
                    .rev()
                    .take(self.images.reach())
                    .filter_map(|i| self.scrollback.get(i).map(key))
                    .collect()
            };
            let rows = Rows {
                screen: &screen,
                above: &keys_above,
            };
            // the lines below the last of `lines`, as far as a relative
            // placement's root can reach
            let end = top.saturating_add(lines.len());
            let below: Vec<LineKey> = if self.flags.alternate {
                Vec::new()
            } else {
                (end..)
                    .map_while(|place| self.line_at(place))
                    .take(self.images.reach_below())
                    .map(key)
                    .collect()
            };
            let mut runs = Vec::new();
            if self.images.has_virtual(self.flags.alternate) {
                let from = if self.flags.alternate { 0 } else { top };
                let (mut carry, _) = placeholder::carry_from(self.rows_above(from), self.cols);
                for line in lines {
                    placeholder::runs(line, key(line), self.cols, &mut carry, &mut |r| {
                        runs.push(r);
                    });
                }
            }
            #[cfg(feature = "bidi")]
            {
                let rows_of: std::collections::BTreeMap<LineKey, usize> =
                    lines.iter().enumerate().map(|(y, l)| (key(l), y)).collect();
                for run in &mut runs {
                    let Some(r) = rows_of.get(&run.line).and_then(|&y| order.row(y)) else {
                        continue;
                    };
                    let first = usize::try_from(run.col).unwrap_or(usize::MAX);
                    let last = first.saturating_add(usize::try_from(run.cells).unwrap_or(0));
                    if let Some(v) = (first..last).map(|c| r.visual_at(c)).min() {
                        run.col = u32::try_from(v).unwrap_or(run.col);
                    }
                }
            }
            let drawn = Drawn {
                screen: self,
                view: lines,
                top,
                #[cfg(feature = "bidi")]
                order: Some(order),
            };
            let mut out = self
                .images
                .frame_drawn(rows, &below, &runs, self.cols, &drawn);
            out.retain_mut(|p| {
                // an entry made from a placeholder run is placed by the
                // runs above, and the store places a relative placement
                // from where its root is drawn
                if p.from_placeholder_run() || p.relative {
                    return true;
                }
                let size = self.size_at(lines, top, p.row);
                #[cfg(feature = "bidi")]
                {
                    if size.is_double() && !shown(p.col, size, self.cols) {
                        return false;
                    }
                    if let Some(r) = self.order_at(order, lines.len(), top, p.row)
                        && r.rtl
                    {
                        return mirror(p, &r, size);
                    }
                }
                to_screen(p, size, self.cols)
            });
            let shift = i64::try_from(above).unwrap_or(i64::MAX);
            for p in &mut out {
                p.row = p.row.saturating_sub(shift);
            }
            out
        }

        /// The runs a frame hands the store: those on the rows the view
        /// shows, read only while the shown buffer holds a virtual
        /// placement, since only a virtual placement draws through them.
        fn frame_runs(&self) -> Vec<PlaceholderRun> {
            if self.images.has_virtual(self.flags.alternate) {
                self.placeholder_runs()
            } else {
                Vec::new()
            }
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

        /// The runs of placeholder cells on the rows the view shows, left
        /// to right and top to bottom, read from the cells (ADR-1100, D13):
        /// what a frame hands the store beside the rows' line keys, so the
        /// store draws each virtual placement where its runs lie.
        pub fn placeholder_runs(&self) -> Vec<PlaceholderRun> {
            let mut out = Vec::new();
            // the alternate screen has no scrollback above it
            let top = match self.flags.alternate {
                true => 0,
                false => self
                    .scrollback
                    .view_index()
                    .unwrap_or(self.scrollback.len()),
            };
            let (mut carry, _) = placeholder::carry_from(self.rows_above(top), self.cols);
            for line in self.view() {
                placeholder::runs(line, key(line), self.cols, &mut carry, &mut |r| {
                    out.push(r);
                });
            }
            out
        }

        /// The runs of placeholder cells on the screen's own lines of the
        /// main (`alt` false) or the alternate buffer, read from the cells
        /// for a store decision that needs a virtual placement's position:
        /// a kitty delete by cell, row or column, or a reservation that may
        /// evict (ADR-1100, D13). The cells read count as input work (D5).
        pub fn screen_placeholder_runs(&mut self, alt: bool) -> Vec<PlaceholderRun> {
            self.screen_runs(alt).into_iter().map(|(_, r)| r).collect()
        }

        /// Marks changed every placeholder cell of kitty image id `image`
        /// on the screen's own lines of the main (`alt` false) or the
        /// alternate buffer, as the store asks when a virtual placement of
        /// that image is made, replaced or deleted, or the image is sent
        /// again (ADR-1100, D13). A buffer not shown has no cells on the
        /// display, and a view moved into the scrollback is marked whole,
        /// as the store's other changes mark it. The cells read count as
        /// input work (D5).
        pub fn mark_placeholders(&mut self, alt: bool, image: u32) {
            self.mark_placeholders_of(alt, &BTreeSet::from([image]), &[]);
        }

        /// [`mark_placeholders`](Self::mark_placeholders) for every kitty
        /// image id of `images` at once, and the cells where each box of
        /// `floating` draws, through one read of the screen. A box follows
        /// a virtual placement, so it lies `row` rows and `col` columns from
        /// the origin that placement's runs give (D13); a box whose virtual
        /// placement no run shows draws nowhere and marks nothing.
        fn mark_placeholders_of(
            &mut self,
            alt: bool,
            images: &BTreeSet<u32>,
            floating: &[FloatingRegion],
        ) {
            if alt != self.flags.alternate || images.is_empty() && floating.is_empty() {
                return;
            }
            if self.scrollback.offset() != 0 {
                self.damage.mark_all();
                return;
            }
            let runs = self.screen_runs(alt);
            #[cfg(feature = "rows-beyond")]
            if !alt && self.changes_above(images, floating, &runs) {
                self.damage.mark_all();
                return;
            }
            for (y, run) in &runs {
                if !images.contains(&run.image) {
                    continue;
                }
                let x0 = usize::try_from(run.col).unwrap_or(usize::MAX);
                let x1 = x0.saturating_add(usize::try_from(run.cells).unwrap_or(usize::MAX));
                self.damage.mark_cells(*y, x0..x1.min(self.cols));
            }
            let rows = || {
                runs.iter()
                    .map(|(y, run)| (i64::try_from(*y).unwrap_or(i64::MAX), run))
            };
            for region in floating {
                let Some((top, left)) = region.top_left(rows()) else {
                    continue;
                };
                let clip = |v: i64, end: usize| usize::try_from(v.max(0)).unwrap_or(0).min(end);
                let y0 = clip(top, self.rows);
                let y1 = clip(top.saturating_add(i64::from(region.rows)), self.rows);
                let x0 = clip(left, self.cols);
                let x1 = clip(left.saturating_add(i64::from(region.cols)), self.cols);
                for y in y0..y1 {
                    self.damage.mark_cells(y, x0..x1);
                }
            }
        }

        /// The scrollback's rows above index `top`, nearest first: those a
        /// picture row that soft-wraps down into the first row read may
        /// start on.
        fn rows_above(&self, top: usize) -> impl Iterator<Item = &Line> + '_ {
            (0..top).rev().filter_map(|i| self.scrollback.get(i))
        }

        /// Whether a changed picture draws on a row that a frame can capture
        /// above the main screen, one of the ROWS_MAX + 1 newest lines of
        /// the scrollback: through placeholder cells of one of `images` on
        /// such a row, or in one of the `floating` boxes that reaches such a
        /// row. `screen` holds the screen's runs, each with its row. A row
        /// above the screen keeps no damage of its own, and a display keeps
        /// its pixels by line id, so such a change has the whole frame read
        /// as drawn whole.
        ///
        /// References:
        /// - ADR-1310 D2, "What the capture holds under `scroll-pan`": a row
        ///   beyond the screen reads as changed only in a frame drawn whole.
        #[cfg(feature = "rows-beyond")]
        fn changes_above(
            &self,
            images: &BTreeSet<u32>,
            floating: &[FloatingRegion],
            screen: &[(usize, PlaceholderRun)],
        ) -> bool {
            let side = crate::bounds::ROWS_MAX.saturating_add(1);
            let top = self.scrollback.len();
            let from = top.saturating_sub(side);
            let (mut carry, _) = placeholder::carry_from(self.rows_above(from), self.cols);
            let mut above: Vec<(i64, PlaceholderRun)> = Vec::new();
            for place in from..top {
                let Some(line) = self.scrollback.get(place) else {
                    continue;
                };
                let back = i64::try_from(top.saturating_sub(place)).unwrap_or(i64::MAX);
                placeholder::runs(line, key(line), self.cols, &mut carry, &mut |r| {
                    above.push((back.saturating_neg(), r));
                });
            }
            if above.iter().any(|(_, r)| images.contains(&r.image)) {
                return true;
            }
            let reach = i64::try_from(side).unwrap_or(i64::MAX).saturating_neg();
            let rows = || {
                let screen = screen
                    .iter()
                    .map(|(y, run)| (i64::try_from(*y).unwrap_or(i64::MAX), run));
                above.iter().map(|(y, run)| (*y, run)).chain(screen)
            };
            floating.iter().any(|region| {
                region.top_left(rows()).is_some_and(|(top, _)| {
                    top < 0 && top.saturating_add(i64::from(region.rows)) > reach
                })
            })
        }

        /// The runs on the screen's own lines of a buffer, each with its
        /// screen row, counting the cells read as input work, those of the
        /// rows above the screen whose wraps reach its first row included.
        fn screen_runs(&mut self, alt: bool) -> Vec<(usize, PlaceholderRun)> {
            // the main screen's rows may continue its scrollback's newest
            // rows; the alternate screen has no scrollback
            let top = if alt { 0 } else { self.scrollback.len() };
            let (mut carry, mut read) = placeholder::carry_from(self.rows_above(top), self.cols);
            let lines = if alt { &self.alt } else { &self.main };
            let mut out = Vec::new();
            for (y, line) in lines.iter().take(self.rows).enumerate() {
                let cells = placeholder::runs(line, key(line), self.cols, &mut carry, &mut |r| {
                    out.push((y, r));
                });
                read = read.saturating_add(cells);
            }
            self.images
                .add_work(u64::try_from(read).unwrap_or(u64::MAX));
            out
        }

        /// The keys of the main (`alt` false) or the alternate buffer's
        /// screen rows, and above the main screen the scrollback's newest
        /// rows, as many as the main buffer's tallest placement reaches.
        pub(crate) fn keys(&self, alt: bool) -> Keys {
            self.keys_reaching(alt, self.images.main_reach())
        }

        /// The keys of the main (`alt` false) or the alternate buffer's
        /// screen rows, and above the main screen the scrollback's newest
        /// `reach` rows.
        fn keys_reaching(&self, alt: bool, reach: usize) -> Keys {
            let lines = if alt { &self.alt } else { &self.main };
            let screen = lines.iter().take(self.rows).map(key).collect();
            let above = if alt {
                Vec::new()
            } else {
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

        /// The rectangle of rows `band` and columns `cols`, inside left and
        /// right margins, is about to scroll `n` rows; returns whether the
        /// store took the scroll, which `images_scrolled` then ends once the
        /// cells have moved (ADR-1100, D6).
        pub(in crate::screen) fn images_scroll_in(
            &mut self,
            band: (usize, usize),
            cols: (usize, usize),
            n: usize,
            up: bool,
        ) -> bool {
            if !self.images.has_placements() {
                return false;
            }
            let keys = self.active_keys();
            self.images.scroll_in(band, cols, n, up, keys.rows());
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

        /// Before ED 2 erases the screen: the placeholder runs on the
        /// screen in use, while it holds a child of a virtual placement,
        /// read now, since ED 2 takes such a child by the cells it draws on
        /// and its erase removes those cells (ADR-1100, D13).
        pub(in crate::screen) fn images_before_erase_screen(&mut self) -> HeldRuns {
            if !self.images.has_placements() {
                return None;
            }
            let alt = self.flags.alternate;
            self.decision_runs([!alt, alt])
        }

        /// ED 2's own rule, after its erase, with the runs read before it.
        pub(in crate::screen) fn images_erase_screen(&mut self, held: HeldRuns) {
            if !self.images.has_placements() {
                return;
            }
            let keys = self.active_keys();
            let rows = keys.rows();
            match &held {
                Some([m, a]) => {
                    let (main, alt) = (self.keys(false), self.keys(true));
                    let screens = Screens {
                        main: main.rows(),
                        alt: alt.rows(),
                    };
                    self.images
                        .with_runs([m, a], screens, self.cols, |store| store.erase_screen(rows));
                }
                None => self.images.erase_screen(rows),
            }
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
            // one read of the shown screen marks every changed image's
            // placeholder cells and every changed box that follows a
            // virtual placement; a buffer not shown has no cells on the
            // display
            let shown = self.flags.alternate;
            let placeholders: BTreeSet<u32> = self
                .images
                .take_changed_virtual()
                .into_iter()
                .filter(|&(alt, _)| alt == shown)
                .map(|(_, id)| id)
                .collect();
            let floating = self.images.take_changed_floating();
            if floating.whole.get(usize::from(shown)) == Some(&true) {
                // more boxes changed than the store keeps notes of
                self.damage.mark_all();
            } else {
                let boxes: Vec<FloatingRegion> = floating
                    .boxes
                    .into_iter()
                    .filter(|&(alt, _)| alt == shown)
                    .map(|(_, region)| region)
                    .collect();
                self.mark_placeholders_of(shown, &placeholders, &boxes);
            }
            let changed = self.images.take_changed();
            if changed.is_empty() {
                return;
            }
            if self.scrollback.offset() != 0 {
                self.damage.mark_all();
                return;
            }
            // A box anchored on a line of the scrollback can reach down
            // onto the screen: a box `row` rows below its line and `rows`
            // tall, from at most `row + rows` lines above the screen. The
            // store keeps no reach for a buffer whose last placement has
            // just gone, so the walk up the scrollback is sized from the
            // changed boxes themselves.
            let reach = changed
                .iter()
                .map(|r| {
                    let below = u64::try_from(r.row.max(0)).unwrap_or(0);
                    let rows = below.saturating_add(u64::from(r.rows));
                    usize::try_from(rows).unwrap_or(usize::MAX)
                })
                .max()
                .unwrap_or(0);
            // A frame captures up to ROWS_MAX + 1 rows above the screen, so
            // the walk also finds each box that reaches one of those rows.
            #[cfg(feature = "rows-beyond")]
            let reach = reach.saturating_add(crate::bounds::ROWS_MAX.saturating_add(1));
            let keys = self.keys_reaching(self.flags.alternate, reach);
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
                // A row above the screen that a frame captures keeps no
                // damage of its own, and a display keeps its pixels by line
                // id, so a box that changes on such a row has every row
                // read as changed.
                #[cfg(feature = "rows-beyond")]
                {
                    let captured = i64::try_from(crate::bounds::ROWS_MAX)
                        .unwrap_or(i64::MAX)
                        .saturating_add(1);
                    let bottom = top.saturating_add(i64::from(region.rows));
                    if top < 0 && bottom > captured.saturating_neg() {
                        self.damage.mark_all();
                        return;
                    }
                }
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

    #[cfg(test)]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    mod tests {
        use crate::Terminal;
        use crate::images::{Kitty, Switches};
        use crate::terminal::ImageString;

        // ADR-1100, D13: a frame reads the placeholder cells only while the
        // shown buffer holds a virtual placement, so a screen of
        // placeholder cells beside an ordinary kitty placement hands the
        // store no runs, and one with a virtual placement hands them all.
        // A frame counts no input work, so the test reads the runs the
        // frame would hand.
        // covers: image.kitty-placeholders
        #[test]
        fn a_frame_reads_runs_only_while_the_shown_buffer_holds_a_virtual_placement() {
            let mut t = Terminal::new(12, 6, 0);
            t.screen_mut().set_image_limit(1 << 30);
            t.screen_mut().set_image_cell(8, 16).unwrap();
            t.set_image_switches(Switches {
                sixel: false,
                kitty: true,
                iterm2: false,
            });
            t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
            let mut s = b"\x1b_Ga=t,f=24,s=8,v=16,i=42,q=2;".to_vec();
            s.extend(b"////".repeat(128));
            s.extend(b"\x1b\\\x1b_Ga=p,i=42,p=1,q=2\x1b\\");
            t.input(&s);
            t.input("\x1b[2;1H\x1b[38;5;42m\u{10EEEE}\u{0305}\u{0305}".as_bytes());
            assert_eq!(t.screen().placeholder_runs().len(), 1);
            assert!(t.screen().frame_runs().is_empty(), "no virtual placement");
            t.input(b"\x1b_Ga=p,U=1,i=42,p=2,c=1,r=1,q=2\x1b\\");
            assert_eq!(t.screen().frame_runs().len(), 1);
            t.input(b"\x1b[?1049h");
            assert!(
                t.screen().frame_runs().is_empty(),
                "the alternate buffer holds none"
            );
        }

        /// A terminal of 12 columns by 4 rows with a scrollback, cells of
        /// 8×16 pixels and the kitty handler, after a picture three rows tall
        /// with id 7 is shown at the left of row 1 and two lines scrolled, so
        /// the picture's rows are -1, 0 and 1.
        fn picture_across_the_top_edge() -> Terminal {
            let mut t = Terminal::new(12, 4, 100);
            t.screen_mut().set_scrollback_max(10);
            t.screen_mut().set_image_limit(1 << 30);
            t.screen_mut().set_image_cell(8, 16).unwrap();
            t.set_image_switches(Switches {
                sixel: false,
                kitty: true,
                iterm2: false,
            });
            t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
            let mut s = b"\x1b[2;1H\x1b_Ga=T,f=24,s=8,v=48,i=7,C=1,q=2;".to_vec();
            s.extend(b"////".repeat(384));
            s.extend(b"\x1b\\");
            t.input(&s);
            t.input(b"\x1b[4;1H\n\n");
            t
        }

        // A kitty delete takes the picture off every cell its box covered,
        // so the screen's rows of that box are marked changed, also when
        // the box's top row has scrolled above the screen and the store
        // holds no other placement. A build with the rows beyond the screen
        // marks everything, since the box reaches row -1.
        //
        // References:
        // - ADR-1100 D8: the store reports each changed box, and the screen
        //   marks the cells under it.
        // covers: image.kitty
        #[test]
        fn a_kitty_delete_marks_the_screen_rows_of_a_box_whose_top_scrolled_off() {
            let mut t = picture_across_the_top_edge();
            assert_eq!(t.screen().placements().len(), 1);
            let age = t.screen_mut().checkpoint();
            t.input(b"\x1b_Ga=d,d=i,i=7,q=2\x1b\\");
            assert!(t.screen().placements().is_empty(), "deleted");
            for row in 0..2 {
                assert!(t.screen().damage().changed(row, 0, age), "row {row}");
            }
            #[cfg(not(feature = "rows-beyond"))]
            assert!(!t.screen().damage().row_changed(2, age), "past the box");
        }
    }
}

#[cfg(not(feature = "images"))]
mod without {
    use super::super::Screen;
    use crate::line::Line;

    /// Without images, ED 2 holds no runs.
    pub(in crate::screen) type HeldRuns = ();

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
        pub(in crate::screen) fn images_scroll_in(
            &mut self,
            _: (usize, usize),
            _: (usize, usize),
            _: usize,
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
        pub(in crate::screen) fn images_before_erase_screen(&mut self) -> HeldRuns {}

        #[inline(always)]
        pub(in crate::screen) fn images_erase_screen(&mut self, _: HeldRuns) {}

        #[inline(always)]
        pub(in crate::screen) fn images_clear_scrollback(&mut self) {}

        #[inline(always)]
        pub(in crate::screen) fn images_alternate(&mut self, _: bool) {}
    }
}
